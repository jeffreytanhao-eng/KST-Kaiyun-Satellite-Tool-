use serde::Serialize;

use crate::attitude::AttitudeFrame;
use crate::scenario::SensorObject;

/// 传感器在本体坐标系中的指向基（均为地固系单位向量）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SensorFrame {
    pub boresight: [f64; 3],
    pub right: [f64; 3],
    pub up: [f64; 3],
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn normalize(vector: [f64; 3]) -> Option<[f64; 3]> {
    let norm = dot(vector, vector).sqrt();
    if !norm.is_finite() || norm < 1e-9 {
        return None;
    }
    Some([vector[0] / norm, vector[1] / norm, vector[2] / norm])
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// 由地理纬度/经度构造当地北向与东向单位向量。
fn local_axes(latitude_deg: f64, longitude_deg: f64) -> ([f64; 3], [f64; 3]) {
    let latitude = latitude_deg.to_radians();
    let longitude = longitude_deg.to_radians();
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();
    (
        [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat],
        [-sin_lon, cos_lon, 0.0],
    )
}

/// 计算传感器指向基。
/// - `satellite_ecef`：卫星地固系位置（km）；
/// - `target_ecef`：`target` 指向模式下的目标地面站位置（km）；
/// - `attitude`：卫星真实姿态（若提供，则 `nadir`/`fixed`/`body` 模式均按本体轴计算）。
pub fn frame_at(
    sensor: &SensorObject,
    satellite_ecef: [f64; 3],
    latitude_deg: f64,
    longitude_deg: f64,
    target_ecef: Option<[f64; 3]>,
    attitude: Option<&AttitudeFrame>,
) -> Option<SensorFrame> {
    let (north, east) = local_axes(latitude_deg, longitude_deg);
    let nadir = normalize([-satellite_ecef[0], -satellite_ecef[1], -satellite_ecef[2]])?;

    let (boresight, reference) = match sensor.pointing_mode.as_str() {
        "body" => {
            let body = attitude?;
            let boresight = normalize([
                body.x[0] * sensor.body_boresight[0]
                    + body.y[0] * sensor.body_boresight[1]
                    + body.z[0] * sensor.body_boresight[2],
                body.x[1] * sensor.body_boresight[0]
                    + body.y[1] * sensor.body_boresight[1]
                    + body.z[1] * sensor.body_boresight[2],
                body.x[2] * sensor.body_boresight[0]
                    + body.y[2] * sensor.body_boresight[1]
                    + body.z[2] * sensor.body_boresight[2],
            ])?;
            (boresight, body.y)
        }
        "fixed" => {
            let azimuth = sensor.fixed_azimuth_deg.to_radians();
            let off_nadir = sensor.fixed_off_nadir_deg.to_radians();
            let (sin_off, cos_off) = off_nadir.sin_cos();
            let (sin_az, cos_az) = azimuth.sin_cos();
            if let Some(body) = attitude {
                let horizontal = [
                    cos_az * body.x[0] + sin_az * body.y[0],
                    cos_az * body.x[1] + sin_az * body.y[1],
                    cos_az * body.x[2] + sin_az * body.y[2],
                ];
                (
                    [
                        cos_off * body.z[0] + sin_off * horizontal[0],
                        cos_off * body.z[1] + sin_off * horizontal[1],
                        cos_off * body.z[2] + sin_off * horizontal[2],
                    ],
                    body.y,
                )
            } else {
                let horizontal = [
                    cos_az * north[0] + sin_az * east[0],
                    cos_az * north[1] + sin_az * east[1],
                    cos_az * north[2] + sin_az * east[2],
                ];
                (
                    [
                        cos_off * nadir[0] + sin_off * horizontal[0],
                        cos_off * nadir[1] + sin_off * horizontal[1],
                        cos_off * nadir[2] + sin_off * horizontal[2],
                    ],
                    north,
                )
            }
        }
        "target" => {
            let target = target_ecef?;
            let direction = normalize(sub(target, satellite_ecef))?;
            (direction, attitude.map(|body| body.y).unwrap_or(north))
        }
        _ => match attitude {
            Some(body) => (body.z, body.y),
            None => (nadir, north),
        },
    };

    let boresight = normalize(boresight)?;

    // 以参考轴构造本体上轴，接近平行时退回到地理北/东向，避免退化。
    let reference_dot = dot(boresight, reference);
    let north_dot = dot(boresight, north);
    let east_dot = dot(boresight, east);
    let (reference, reference_dot) = if reference_dot.abs() < 0.95 {
        (reference, reference_dot)
    } else if north_dot.abs() < 0.95 {
        (north, north_dot)
    } else {
        (east, east_dot)
    };
    let up_component = [
        reference[0] - boresight[0] * reference_dot,
        reference[1] - boresight[1] * reference_dot,
        reference[2] - boresight[2] * reference_dot,
    ];
    let up = normalize(up_component)?;
    let right = normalize(cross(up, boresight))?;

    Some(SensorFrame {
        boresight,
        right,
        up,
    })
}

/// 判断地面站是否落在传感器视场内（含最大作用距离）。
pub fn sees(
    sensor: &SensorObject,
    frame: &SensorFrame,
    satellite_ecef: [f64; 3],
    station_ecef: [f64; 3],
) -> bool {
    let direction = sub(station_ecef, satellite_ecef);
    let range = dot(direction, direction).sqrt();
    if !range.is_finite() || range <= 0.0 || range > sensor.max_range_km {
        return false;
    }
    let unit = match normalize(direction) {
        Some(value) => value,
        None => return false,
    };

    let along = dot(unit, frame.boresight);
    if along <= 0.0 {
        return false;
    }

    match sensor.shape.as_str() {
        "rectangular" => {
            let horizontal = dot(unit, frame.right).atan2(along).to_degrees();
            let vertical = dot(unit, frame.up).atan2(along).to_degrees();
            horizontal.abs() <= sensor.horizontal_fov_deg / 2.0
                && vertical.abs() <= sensor.vertical_fov_deg / 2.0
        }
        _ => along.clamp(-1.0, 1.0).acos().to_degrees() <= sensor.half_angle_deg,
    }
}
