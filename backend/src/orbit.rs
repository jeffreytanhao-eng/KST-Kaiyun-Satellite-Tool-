use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sgp4::{Constants, Elements};
use stkcraft_geocalc::coords::{to_coordinates, CoordinateSystem};

use crate::attitude;
use crate::numerical::{self, IntegrationStats, NumericalState};
use crate::scenario::{GroundStationObject, ScenarioData, SatelliteObject, SensorObject};
use crate::sensor;

/// WGS84/IAU 地球赤道半径与极半径（km）。
const EARTH_RADII: [f64; 3] = [6378.1366, 6378.1366, 6356.7519];
/// 单个任务的采样点上限，避免异常请求拖垮服务。
const MAX_SAMPLES: usize = 20_000;
/// 覆盖网格点上限。
pub const MAX_COVERAGE_POINTS: usize = 5_000;

const FALLBACK_COLORS: [&str; 6] = [
    "#4fc3f7", "#ffb74d", "#81c784", "#e57373", "#ba68c8", "#4db6ac",
];

pub const CONSTRAINT_ELEVATION: &str = "min_elevation";
pub const CONSTRAINT_AZIMUTH: &str = "azimuth_range";
pub const CONSTRAINT_OCCLUSION: &str = "earth_occlusion";
pub const CONSTRAINT_SENSOR: &str = "sensor_fov";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSample {
    pub t_seconds: f64,
    pub longitude_deg: f64,
    pub latitude_deg: f64,
    pub altitude_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteTrack {
    pub satellite_id: String,
    pub name: String,
    pub color: String,
    pub samples: Vec<TrackSample>,
}

/// 传感器在某采样时刻的指向基，供三维视场渲染使用。
#[derive(Debug, Clone, Serialize)]
pub struct SensorSample {
    pub t_seconds: f64,
    pub boresight: [f64; 3],
    pub right: [f64; 3],
    pub up: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct SensorTrack {
    pub sensor_id: String,
    pub sensor_name: String,
    pub satellite_id: String,
    pub color: String,
    pub shape: String,
    pub half_angle_deg: f64,
    pub horizontal_fov_deg: f64,
    pub vertical_fov_deg: f64,
    pub max_range_km: f64,
    pub pointing_mode: String,
    pub samples: Vec<SensorSample>,
}

/// 卫星姿态采样（本体三轴 + 四元数，均为地固系）。
#[derive(Debug, Clone, Serialize)]
pub struct AttitudeSample {
    pub t_seconds: f64,
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub z: [f64; 3],
    pub quaternion: [f64; 4],
}

/// 单颗卫星的姿态序列。
#[derive(Debug, Clone, Serialize)]
pub struct AttitudeTrack {
    pub satellite_id: String,
    pub name: String,
    pub color: String,
    pub mode: String,
    pub samples: Vec<AttitudeSample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropagationReport {
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub satellites: Vec<SatelliteTrack>,
    #[serde(default)]
    pub sensor_tracks: Vec<SensorTrack>,
    #[serde(default)]
    pub attitude_tracks: Vec<AttitudeTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessWindow {
    pub index: u32,
    pub start_utc: String,
    pub end_utc: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
    pub max_elevation_deg: f64,
    /// 该窗口满足的约束类型。
    #[serde(default)]
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessReport {
    pub satellite_id: String,
    pub satellite_name: String,
    pub ground_station_id: String,
    pub ground_station_name: String,
    pub min_elevation_deg: f64,
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub window_count: u32,
    pub total_duration_seconds: f64,
    pub longest_window_seconds: f64,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub azimuth_min_deg: Option<f64>,
    #[serde(default)]
    pub azimuth_max_deg: Option<f64>,
    #[serde(default)]
    pub sensor_id: Option<String>,
    #[serde(default)]
    pub sensor_name: Option<String>,
    pub windows: Vec<AccessWindow>,
}

/// 多约束 Access 的可选条件。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccessOptions {
    #[serde(default)]
    pub min_elevation_deg: Option<f64>,
    #[serde(default)]
    pub azimuth_min_deg: Option<f64>,
    #[serde(default)]
    pub azimuth_max_deg: Option<f64>,
    #[serde(default)]
    pub sensor_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CoverageRegion {
    pub min_latitude_deg: f64,
    pub max_latitude_deg: f64,
    pub min_longitude_deg: f64,
    pub max_longitude_deg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoveragePoint {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub visible_seconds: f64,
    pub access_count: u32,
    pub max_elevation_deg: f64,
    pub covered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    pub satellite_ids: Vec<String>,
    pub satellite_names: Vec<String>,
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub min_elevation_deg: f64,
    pub region: CoverageRegion,
    pub grid_step_deg: f64,
    pub total_points: usize,
    pub covered_points: usize,
    pub uncovered_points: usize,
    pub multi_covered_points: usize,
    pub coverage_ratio: f64,
    pub average_visible_seconds: f64,
    pub max_visible_seconds: f64,
    pub total_visible_seconds: f64,
    #[serde(default)]
    pub satellite_stats: Vec<CoverageSatelliteStat>,
    pub points: Vec<CoveragePoint>,
}

/// 单颗卫星在覆盖区域内的协同统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageSatelliteStat {
    pub satellite_id: String,
    pub satellite_name: String,
    pub covered_points: usize,
    pub coverage_ratio: f64,
    pub total_visible_seconds: f64,
    pub max_visible_seconds: f64,
}

fn format_utc(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn color_of(satellite: &SatelliteObject, index: usize) -> String {
    if satellite.color.trim().is_empty() {
        FALLBACK_COLORS[index % FALLBACK_COLORS.len()].to_string()
    } else {
        satellite.color.clone()
    }
}

fn sensor_color(sensor: &SensorObject, index: usize) -> String {
    if sensor.color.trim().is_empty() {
        FALLBACK_COLORS[index % FALLBACK_COLORS.len()].to_string()
    } else {
        sensor.color.clone()
    }
}

/// 解析 TLE 并构建 SGP4 传播器。
pub(crate) fn prepare(satellite: &SatelliteObject) -> Result<(Elements, Constants<'_>), String> {
    let line1 = satellite.tle_line1.trim();
    let line2 = satellite.tle_line2.trim();
    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|err| format!("卫星 {} 的 TLE 解析失败：{err}", satellite.name))?;
    let constants = Constants::from_elements(&elements)
        .map_err(|err| format!("卫星 {} 的 SGP4 初始化失败：{err}", satellite.name))?;
    Ok((elements, constants))
}

/// 格林尼治平恒星时（IAU 1982 表达式，弧度）。
pub(crate) fn gmst_radians(time: DateTime<Utc>) -> f64 {
    let days = time.timestamp_millis() as f64 / 86_400_000.0 - 10_957.5;
    let centuries = days / 36_525.0;
    let degrees = 280.460_618_37
        + 360.985_647_366_29 * days
        + 0.000_387_933 * centuries * centuries
        - centuries * centuries * centuries / 38_710_000.0;
    degrees.rem_euclid(360.0).to_radians()
}

/// TEME → 地固系（仅旋转，用于位置与仰角计算）。
fn teme_to_ecef(position: [f64; 3], gmst: f64) -> [f64; 3] {
    let (sin, cos) = gmst.sin_cos();
    [
        cos * position[0] + sin * position[1],
        -sin * position[0] + cos * position[1],
        position[2],
    ]
}

/// 传播单个时刻并返回地固系位置与速度（km、km/s，速度未扣除地球自转）。
pub(crate) fn propagate_state(
    elements: &Elements,
    constants: &Constants,
    time: DateTime<Utc>,
) -> Result<([f64; 3], [f64; 3]), String> {
    let minutes_since_epoch =
        (time.naive_utc() - elements.datetime).num_milliseconds() as f64 / 60_000.0;
    let prediction = constants
        .propagate(minutes_since_epoch)
        .map_err(|err| format!("SGP4 传播失败：{err}"))?;
    let gmst = gmst_radians(time);
    Ok((
        teme_to_ecef(prediction.position, gmst),
        teme_to_ecef(prediction.velocity, gmst),
    ))
}

/// 传播单个时刻并返回 TEME→ECEF 后的位置（km）。
fn propagate_ecef(
    elements: &Elements,
    constants: &Constants,
    time: DateTime<Utc>,
) -> Result<[f64; 3], String> {
    propagate_state(elements, constants, time).map(|(position, _)| position)
}

/// ECEF 位置 → [经度(度), 纬度(度), 高度(km)]。
pub(crate) fn ecef_to_geodetic(position: [f64; 3]) -> Result<[f64; 3], String> {
    to_coordinates(
        position,
        CoordinateSystem::Geodetic,
        Some(EARTH_RADII),
        399,
    )
    .map(|[longitude, latitude, altitude]| {
        [longitude.to_degrees(), latitude.to_degrees(), altitude]
    })
    .map_err(|err| format!("大地坐标转换失败：{err}"))
}

/// 地面站大地坐标 → 地固系位置（km）。
pub fn station_ecef(station: &GroundStationObject) -> [f64; 3] {
    geodetic_to_ecef(
        station.latitude_deg,
        station.longitude_deg,
        station.altitude_km,
    )
}

pub(crate) fn geodetic_to_ecef(latitude_deg: f64, longitude_deg: f64, altitude_km: f64) -> [f64; 3] {
    let equatorial = EARTH_RADII[0];
    let flattening = (EARTH_RADII[0] - EARTH_RADII[2]) / EARTH_RADII[0];
    let eccentricity_sq = flattening * (2.0 - flattening);

    let latitude = latitude_deg.to_radians();
    let longitude = longitude_deg.to_radians();
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();

    let prime_vertical = equatorial / (1.0 - eccentricity_sq * sin_lat * sin_lat).sqrt();
    [
        (prime_vertical + altitude_km) * cos_lat * cos_lon,
        (prime_vertical + altitude_km) * cos_lat * sin_lon,
        (prime_vertical * (1.0 - eccentricity_sq) + altitude_km) * sin_lat,
    ]
}

fn look_angles_with_basis(
    satellite_ecef: [f64; 3],
    station_position: [f64; 3],
    up: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
) -> (f64, f64, f64) {
    let range_vector = [
        satellite_ecef[0] - station_position[0],
        satellite_ecef[1] - station_position[1],
        satellite_ecef[2] - station_position[2],
    ];
    let range = (range_vector[0] * range_vector[0]
        + range_vector[1] * range_vector[1]
        + range_vector[2] * range_vector[2])
        .sqrt();

    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

    let elevation = if range > 0.0 {
        (dot(range_vector, up) / range)
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees()
    } else {
        90.0
    };
    let azimuth = dot(range_vector, east)
        .atan2(dot(range_vector, north))
        .to_degrees()
        .rem_euclid(360.0);

    (elevation, azimuth, range)
}

fn sample_count(total_seconds: f64, step_seconds: f64) -> Result<usize, String> {
    if !(step_seconds.is_finite()) || step_seconds <= 0.0 {
        return Err("采样步长必须为正数".into());
    }
    let count = (total_seconds / step_seconds).floor() as usize + 1;
    if count > MAX_SAMPLES {
        return Err(format!(
            "采样点过多（{count}），请增大步长或缩短时间范围"
        ));
    }
    Ok(count.max(1))
}

/// 判断地面站到卫星的视线是否被地球遮挡（椭球解析求交）。
pub(crate) fn earth_occluded(from: [f64; 3], to: [f64; 3]) -> bool {
    let a = EARTH_RADII[0];
    let c = EARTH_RADII[2];
    let directions = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let scale = [1.0 / (a * a), 1.0 / (a * a), 1.0 / (c * c)];

    let quadratic_a = directions[0] * directions[0] * scale[0]
        + directions[1] * directions[1] * scale[1]
        + directions[2] * directions[2] * scale[2];
    let quadratic_b = 2.0
        * (from[0] * directions[0] * scale[0]
            + from[1] * directions[1] * scale[1]
            + from[2] * directions[2] * scale[2]);
    let quadratic_c =
        from[0] * from[0] * scale[0] + from[1] * from[1] * scale[1] + from[2] * from[2] * scale[2]
            - 1.0;

    let discriminant = quadratic_b * quadratic_b - 4.0 * quadratic_a * quadratic_c;
    if discriminant <= 1e-12 || quadratic_a <= 0.0 {
        return false;
    }
    let root = discriminant.sqrt();
    let t1 = (-quadratic_b - root) / (2.0 * quadratic_a);
    let t2 = (-quadratic_b + root) / (2.0 * quadratic_a);
    (0.02..0.98).contains(&t1) || (0.02..0.98).contains(&t2)
}

fn target_ecef(data: &ScenarioData, sensor: &SensorObject) -> Option<[f64; 3]> {
    if sensor.pointing_mode != "target" {
        return None;
    }
    let station_id = sensor.target_station_id.as_deref()?;
    data.ground_station(station_id).map(station_ecef)
}

/// 轨道状态来源：SGP4 直接传播，或数值积分序列 Hermite 插值。
pub(crate) enum StateSource<'a> {
    Sgp4 {
        elements: &'a Elements,
        constants: &'a Constants<'a>,
    },
    Numerical {
        states: &'a [NumericalState],
    },
}

impl StateSource<'_> {
    pub(crate) fn state_at(
        &self,
        start: DateTime<Utc>,
        t_seconds: f64,
    ) -> Result<([f64; 3], [f64; 3]), String> {
        match self {
            StateSource::Sgp4 {
                elements,
                constants,
            } => {
                let time =
                    start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
                propagate_state(elements, constants, time)
            }
            StateSource::Numerical { states } => numerical::interpolate(states, t_seconds)
                .map(|state| (state.position, state.velocity))
                .ok_or_else(|| "数值积分序列为空".to_string()),
        }
    }
}

fn satellite_attitude(
    satellite: &SatelliteObject,
    position: [f64; 3],
    velocity: [f64; 3],
    time: DateTime<Utc>,
) -> Option<attitude::AttitudeFrame> {
    attitude::compute(satellite, position, velocity, time)
}


/// 计算场景内卫星在时间窗口内的地固系轨迹。
pub fn propagate_scenario(
    data: &ScenarioData,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    satellite_ids: Option<&[String]>,
) -> Result<PropagationReport, String> {
    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    let count = sample_count(total_seconds, step_seconds)?;

    let mut tracks = Vec::new();
    let mut sensor_tracks = Vec::new();
    let mut attitude_tracks = Vec::new();

    for (index, satellite) in data.satellites.iter().enumerate() {
        let selected = match satellite_ids {
            Some(ids) => ids.iter().any(|id| id == &satellite.id),
            None => satellite.enabled,
        };
        if !selected {
            continue;
        }

        // 数值积分卫星先完成一次完整积分，之后按采样时刻取值。
        let numerical_track = if satellite.propagator_type == "numerical" {
            Some(numerical::propagate(
                satellite,
                start,
                total_seconds,
                step_seconds,
            )?)
        } else {
            None
        };
        let sgp4 = if numerical_track.is_none() {
            Some(prepare(satellite)?)
        } else {
            None
        };
        let source = match (&numerical_track, &sgp4) {
            (Some(track), _) => StateSource::Numerical {
                states: &track.states,
            },
            (None, Some((elements, constants))) => StateSource::Sgp4 {
                elements,
                constants,
            },
            _ => return Err("传播器初始化失败".into()),
        };

        let sensors: Vec<&SensorObject> = data.sensors_of(&satellite.id).collect();
        let sensor_targets: Vec<Option<[f64; 3]>> = sensors
            .iter()
            .map(|sensor| target_ecef(data, sensor))
            .collect();

        let mut samples = Vec::with_capacity(count);
        let mut attitude_samples = Vec::with_capacity(count);
        let mut sensor_samples: Vec<Vec<SensorSample>> = sensors
            .iter()
            .map(|_| Vec::with_capacity(count))
            .collect();
        let mut previous_longitude: Option<f64> = None;

        for sample_index in 0..count {
            let t_seconds = sample_index as f64 * step_seconds;
            let time = start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
            let (ecef, velocity) = source.state_at(start, t_seconds)?;
            let [mut longitude, latitude, altitude] = ecef_to_geodetic(ecef)?;

            // 经度展开，避免 ±180° 跳变导致三维轨迹横穿地球。
            if let Some(previous) = previous_longitude {
                while longitude - previous > 180.0 {
                    longitude -= 360.0;
                }
                while longitude - previous < -180.0 {
                    longitude += 360.0;
                }
            }
            previous_longitude = Some(longitude);

            let attitude = satellite_attitude(satellite, ecef, velocity, time);
            if let Some(frame) = &attitude {
                attitude_samples.push(AttitudeSample {
                    t_seconds,
                    x: frame.x,
                    y: frame.y,
                    z: frame.z,
                    quaternion: frame.quaternion,
                });
            }

            for (sensor_index, sensor) in sensors.iter().enumerate() {
                if let Some(frame) = sensor::frame_at(
                    sensor,
                    ecef,
                    latitude,
                    longitude,
                    sensor_targets[sensor_index],
                    attitude.as_ref(),
                ) {
                    sensor_samples[sensor_index].push(SensorSample {
                        t_seconds,
                        boresight: frame.boresight,
                        right: frame.right,
                        up: frame.up,
                    });
                }
            }

            samples.push(TrackSample {
                t_seconds,
                longitude_deg: longitude,
                latitude_deg: latitude,
                altitude_km: altitude,
            });
        }

        for (sensor_index, sensor) in sensors.iter().enumerate() {
            sensor_tracks.push(SensorTrack {
                sensor_id: sensor.id.clone(),
                sensor_name: sensor.name.clone(),
                satellite_id: satellite.id.clone(),
                color: sensor_color(sensor, index),
                shape: sensor.shape.clone(),
                half_angle_deg: sensor.half_angle_deg,
                horizontal_fov_deg: sensor.horizontal_fov_deg,
                vertical_fov_deg: sensor.vertical_fov_deg,
                max_range_km: sensor.max_range_km,
                pointing_mode: sensor.pointing_mode.clone(),
                samples: std::mem::take(&mut sensor_samples[sensor_index]),
            });
        }

        attitude_tracks.push(AttitudeTrack {
            satellite_id: satellite.id.clone(),
            name: satellite.name.clone(),
            color: color_of(satellite, index),
            mode: satellite.attitude_mode.clone(),
            samples: attitude_samples,
        });

        tracks.push(SatelliteTrack {
            satellite_id: satellite.id.clone(),
            name: satellite.name.clone(),
            color: color_of(satellite, index),
            samples,
        });
    }

    Ok(PropagationReport {
        start: format_utc(start),
        end: format_utc(end),
        step_seconds,
        satellites: tracks,
        sensor_tracks,
        attitude_tracks,
    })
}

#[derive(Debug, Clone, Copy)]
struct ConstraintSample {
    ok: bool,
    elevation_deg: f64,
}

struct AccessContext<'a> {
    source: StateSource<'a>,
    satellite: &'a SatelliteObject,
    start: DateTime<Utc>,
    station_position: [f64; 3],
    up: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
    min_elevation: f64,
    azimuth_range: Option<(f64, f64)>,
    sensor: Option<&'a SensorObject>,
    sensor_target: Option<[f64; 3]>,
}

impl AccessContext<'_> {
    fn evaluate(&self, t_seconds: f64) -> Result<ConstraintSample, String> {
        let time =
            self.start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
        let (ecef, velocity) = self.source.state_at(self.start, t_seconds)?;
        let (elevation, azimuth, _) = look_angles_with_basis(
            ecef,
            self.station_position,
            self.up,
            self.east,
            self.north,
        );

        let mut ok = elevation >= self.min_elevation;
        if ok {
            ok = !earth_occluded(self.station_position, ecef);
        }
        if ok {
            if let Some((minimum, maximum)) = self.azimuth_range {
                ok = az_in_range(azimuth, minimum, maximum);
            }
        }
        if ok {
            if let Some(sensor) = self.sensor {
                ok = match ecef_to_geodetic(ecef) {
                    Ok([longitude, latitude, _]) => {
                        let attitude =
                            satellite_attitude(self.satellite, ecef, velocity, time);
                        sensor::frame_at(
                            sensor,
                            ecef,
                            latitude,
                            longitude,
                            self.sensor_target,
                            attitude.as_ref(),
                        )
                        .map(|frame| sensor::sees(sensor, &frame, ecef, self.station_position))
                        .unwrap_or(false)
                    }
                    Err(_) => false,
                };
            }
        }

        Ok(ConstraintSample {
            ok,
            elevation_deg: elevation,
        })
    }
}

fn az_in_range(value: f64, minimum: f64, maximum: f64) -> bool {
    if minimum <= maximum {
        value >= minimum && value <= maximum
    } else {
        value >= minimum || value <= maximum
    }
}

/// 在状态发生变化的时间区间内二分细化边界，避免固定步长带来的边界误差。
fn refine_boundary<F>(mut low: f64, mut high: f64, ok_low: bool, mut evaluate: F) -> Result<f64, String>
where
    F: FnMut(f64) -> Result<ConstraintSample, String>,
{
    for _ in 0..28 {
        let middle = (low + high) / 2.0;
        if evaluate(middle)?.ok == ok_low {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok((low + high) / 2.0)
}

/// 计算卫星与地面站在时间窗口内的多约束 Access 窗口。
pub fn compute_access(
    data: &ScenarioData,
    satellite_id: &str,
    ground_station_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    options: &AccessOptions,
) -> Result<AccessReport, String> {
    let satellite = data
        .satellite(satellite_id)
        .ok_or_else(|| format!("场景中不存在卫星 {satellite_id}"))?;
    let station = data
        .ground_station(ground_station_id)
        .ok_or_else(|| format!("场景中不存在地面站 {ground_station_id}"))?;

    let min_elevation = options
        .min_elevation_deg
        .unwrap_or(station.min_elevation_deg);
    if !min_elevation.is_finite() || !(0.0..=90.0).contains(&min_elevation) {
        return Err("最小仰角需在 [0, 90] 度内".into());
    }

    let azimuth_range = match (options.azimuth_min_deg, options.azimuth_max_deg) {
        (Some(minimum), Some(maximum)) => {
            if !minimum.is_finite()
                || !maximum.is_finite()
                || !(0.0..=360.0).contains(&minimum)
                || !(0.0..=360.0).contains(&maximum)
                || (minimum - maximum).abs() < 1e-9
            {
                return Err("方位角范围需为 [0, 360] 度内的两个不同值".into());
            }
            Some((minimum, maximum))
        }
        (None, None) => None,
        _ => return Err("方位角范围需同时填写上下限".into()),
    };

    let sensor = match options.sensor_id.as_deref() {
        Some(id) => {
            let sensor = data
                .sensor(id)
                .ok_or_else(|| format!("场景中不存在传感器 {id}"))?;
            if sensor.satellite_id != satellite.id {
                return Err("所选传感器未绑定到该卫星".into());
            }
            if !sensor.enabled {
                return Err("所选传感器未启用".into());
            }
            Some(sensor)
        }
        None => None,
    };

    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    let count = sample_count(total_seconds, step_seconds)?;

    let numerical_track = if satellite.propagator_type == "numerical" {
        Some(numerical::propagate(
            satellite,
            start,
            total_seconds,
            step_seconds,
        )?)
    } else {
        None
    };
    let sgp4 = if numerical_track.is_none() {
        Some(prepare(satellite)?)
    } else {
        None
    };
    let source = match (&numerical_track, &sgp4) {
        (Some(track), _) => StateSource::Numerical {
            states: &track.states,
        },
        (None, Some((elements, constants))) => StateSource::Sgp4 {
            elements,
            constants,
        },
        _ => return Err("传播器初始化失败".into()),
    };

    let station_position = station_ecef(station);
    let latitude = station.latitude_deg.to_radians();
    let longitude = station.longitude_deg.to_radians();
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();

    let mut constraints = vec![
        CONSTRAINT_ELEVATION.to_string(),
        CONSTRAINT_OCCLUSION.to_string(),
    ];
    if azimuth_range.is_some() {
        constraints.push(CONSTRAINT_AZIMUTH.to_string());
    }
    if sensor.is_some() {
        constraints.push(CONSTRAINT_SENSOR.to_string());
    }

    let context = AccessContext {
        source,
        satellite,
        start,
        station_position,
        up: [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat],
        east: [-sin_lon, cos_lon, 0.0],
        north: [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat],
        min_elevation,
        azimuth_range,
        sensor,
        sensor_target: sensor.and_then(|item| target_ecef(data, item)),
    };

    struct ActiveWindow {
        start_seconds: f64,
        max_elevation_deg: f64,
    }

    let mut windows = Vec::new();
    let mut active: Option<ActiveWindow> = None;
    let mut previous: Option<(f64, ConstraintSample)> = None;

    let push_window = |start_seconds: f64,
                       end_seconds: f64,
                       max_elevation_deg: f64,
                       windows: &mut Vec<AccessWindow>| {
        let end_seconds = end_seconds.max(start_seconds);
        let index = windows.len() as u32 + 1;
        windows.push(AccessWindow {
            index,
            start_utc: format_utc(
                start + ChronoDuration::milliseconds((start_seconds * 1000.0).round() as i64),
            ),
            end_utc: format_utc(
                start + ChronoDuration::milliseconds((end_seconds * 1000.0).round() as i64),
            ),
            start_seconds,
            end_seconds,
            duration_seconds: end_seconds - start_seconds,
            max_elevation_deg,
            constraints: constraints.clone(),
        });
    };

    for sample_index in 0..count {
        let t_seconds = sample_index as f64 * step_seconds;
        let current = context.evaluate(t_seconds)?;

        if current.ok {
            match active.as_mut() {
                Some(window) => {
                    if current.elevation_deg > window.max_elevation_deg {
                        window.max_elevation_deg = current.elevation_deg;
                    }
                }
                None => {
                    let start_seconds = match previous {
                        Some((previous_t, previous_sample)) if !previous_sample.ok => {
                            refine_boundary(previous_t, t_seconds, false, |time| {
                                context.evaluate(time)
                            })?
                        }
                        _ => t_seconds,
                    };
                    active = Some(ActiveWindow {
                        start_seconds,
                        max_elevation_deg: current.elevation_deg,
                    });
                }
            }
        } else if let Some(window) = active.take() {
            let end_seconds = match previous {
                Some((previous_t, previous_sample)) if previous_sample.ok => {
                    refine_boundary(previous_t, t_seconds, true, |time| context.evaluate(time))?
                }
                _ => t_seconds,
            };
            push_window(
                window.start_seconds,
                end_seconds,
                window.max_elevation_deg,
                &mut windows,
            );
        }

        previous = Some((t_seconds, current));
    }

    if let Some(window) = active.take() {
        push_window(
            window.start_seconds,
            total_seconds,
            window.max_elevation_deg,
            &mut windows,
        );
    }

    let total_duration: f64 = windows.iter().map(|window| window.duration_seconds).sum();
    let longest = windows
        .iter()
        .map(|window| window.duration_seconds)
        .fold(0.0_f64, f64::max);

    Ok(AccessReport {
        satellite_id: satellite.id.clone(),
        satellite_name: satellite.name.clone(),
        ground_station_id: station.id.clone(),
        ground_station_name: station.name.clone(),
        min_elevation_deg: min_elevation,
        start: format_utc(start),
        end: format_utc(end),
        step_seconds,
        window_count: windows.len() as u32,
        total_duration_seconds: total_duration,
        longest_window_seconds: longest,
        constraints,
        azimuth_min_deg: azimuth_range.map(|(minimum, _)| minimum),
        azimuth_max_deg: azimuth_range.map(|(_, maximum)| maximum),
        sensor_id: sensor.map(|item| item.id.clone()),
        sensor_name: sensor.map(|item| item.name.clone()),
        windows,
    })
}

/// 区域网格覆盖计算：统计每个网格点对选中卫星的可见时长与覆盖次数。
#[allow(clippy::too_many_arguments)]
pub fn compute_coverage(
    data: &ScenarioData,
    satellite_ids: Option<&[String]>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    region: CoverageRegion,
    grid_step_deg: f64,
    min_elevation_deg: f64,
) -> Result<CoverageReport, String> {
    if !region.min_latitude_deg.is_finite()
        || !region.max_latitude_deg.is_finite()
        || !region.min_longitude_deg.is_finite()
        || !region.max_longitude_deg.is_finite()
        || !(-90.0..=90.0).contains(&region.min_latitude_deg)
        || !(-90.0..=90.0).contains(&region.max_latitude_deg)
        || !(-180.0..=180.0).contains(&region.min_longitude_deg)
        || !(-180.0..=180.0).contains(&region.max_longitude_deg)
        || region.min_latitude_deg >= region.max_latitude_deg
        || region.min_longitude_deg >= region.max_longitude_deg
    {
        return Err("覆盖区域需为合法的经纬度范围（最小值必须小于最大值）".into());
    }
    if !grid_step_deg.is_finite() || !(0.1..=30.0).contains(&grid_step_deg) {
        return Err("网格步长需在 [0.1, 30] 度内".into());
    }
    if !min_elevation_deg.is_finite() || !(0.0..=90.0).contains(&min_elevation_deg) {
        return Err("最小仰角需在 [0, 90] 度内".into());
    }

    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    let sample_count = sample_count(total_seconds, step_seconds)?;

    let latitude_steps = ((region.max_latitude_deg - region.min_latitude_deg) / grid_step_deg).ceil()
        as usize;
    let longitude_steps =
        ((region.max_longitude_deg - region.min_longitude_deg) / grid_step_deg).ceil() as usize;
    let point_count = (latitude_steps + 1) * (longitude_steps + 1);
    if point_count > MAX_COVERAGE_POINTS {
        return Err(format!(
            "网格点数为 {point_count}，超过上限 {MAX_COVERAGE_POINTS}，请增大网格步长或缩小区域"
        ));
    }

    let mut satellites = Vec::new();
    for satellite in &data.satellites {
        let selected = match satellite_ids {
            Some(ids) => ids.iter().any(|id| id == &satellite.id),
            None => satellite.enabled,
        };
        if selected {
            satellites.push(satellite);
        }
    }
    if satellites.is_empty() {
        return Err("请至少选择一颗卫星".into());
    }

    // 每颗卫星只传播一次，后续按网格点复用位置序列。
    let mut positions: Vec<Vec<[f64; 3]>> = Vec::with_capacity(satellites.len());
    for satellite in &satellites {
        let series: Vec<[f64; 3]> = if satellite.propagator_type == "numerical" {
            numerical::propagate(satellite, start, total_seconds, step_seconds)?
                .states
                .into_iter()
                .map(|state| state.position)
                .collect()
        } else {
            let (elements, constants) = prepare(satellite)?;
            let mut values = Vec::with_capacity(sample_count);
            for sample_index in 0..sample_count {
                let t_seconds = sample_index as f64 * step_seconds;
                let time =
                    start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
                values.push(propagate_ecef(&elements, &constants, time)?);
            }
            values
        };
        positions.push(series);
    }

    let mut points = Vec::with_capacity(point_count);
    let mut covered_points = 0usize;
    let mut multi_covered_points = 0usize;
    let mut total_visible = 0.0_f64;
    let mut max_visible = 0.0_f64;
    let mut satellite_stats: Vec<CoverageSatelliteStat> = satellites
        .iter()
        .map(|satellite| CoverageSatelliteStat {
            satellite_id: satellite.id.clone(),
            satellite_name: satellite.name.clone(),
            covered_points: 0,
            coverage_ratio: 0.0,
            total_visible_seconds: 0.0,
            max_visible_seconds: 0.0,
        })
        .collect();

    for lat_index in 0..=latitude_steps {
        let latitude_deg = (region.min_latitude_deg + lat_index as f64 * grid_step_deg)
            .min(region.max_latitude_deg);
        for lon_index in 0..=longitude_steps {
            let longitude_deg = (region.min_longitude_deg + lon_index as f64 * grid_step_deg)
                .min(region.max_longitude_deg);

            let station_position = geodetic_to_ecef(latitude_deg, longitude_deg, 0.0);
            let latitude = latitude_deg.to_radians();
            let longitude = longitude_deg.to_radians();
            let (sin_lat, cos_lat) = latitude.sin_cos();
            let (sin_lon, cos_lon) = longitude.sin_cos();
            let up = [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat];
            let east = [-sin_lon, cos_lon, 0.0];
            let north = [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat];

            let mut visible_seconds = 0.0_f64;
            let mut access_count = 0u32;
            let mut point_max_elevation = f64::NEG_INFINITY;
            let mut contributing_satellites = 0usize;

            for (satellite_index, series) in positions.iter().enumerate() {
                let (satellite_visible, satellite_access, satellite_max_elevation) =
                    point_visibility(series, station_position, up, east, north, min_elevation_deg, step_seconds);
                visible_seconds += satellite_visible;
                access_count += satellite_access;
                point_max_elevation = point_max_elevation.max(satellite_max_elevation);

                if satellite_visible > 0.0 {
                    contributing_satellites += 1;
                    let stats = &mut satellite_stats[satellite_index];
                    stats.covered_points += 1;
                    stats.total_visible_seconds += satellite_visible;
                    stats.max_visible_seconds = stats.max_visible_seconds.max(satellite_visible);
                }
            }

            if contributing_satellites >= 2 {
                multi_covered_points += 1;
            }
            if !point_max_elevation.is_finite() {
                point_max_elevation = 0.0;
            }
            let covered = visible_seconds > 0.0;
            if covered {
                covered_points += 1;
                total_visible += visible_seconds;
                max_visible = max_visible.max(visible_seconds);
            }

            points.push(CoveragePoint {
                latitude_deg,
                longitude_deg,
                visible_seconds,
                access_count,
                max_elevation_deg: point_max_elevation,
                covered,
            });
        }
    }

    let coverage_ratio = if points.is_empty() {
        0.0
    } else {
        covered_points as f64 / points.len() as f64
    };
    let average_visible_seconds = if covered_points == 0 {
        0.0
    } else {
        total_visible / covered_points as f64
    };
    for stats in &mut satellite_stats {
        stats.coverage_ratio = if points.is_empty() {
            0.0
        } else {
            stats.covered_points as f64 / points.len() as f64
        };
    }

    Ok(CoverageReport {
        satellite_ids: satellites.iter().map(|item| item.id.clone()).collect(),
        satellite_names: satellites.iter().map(|item| item.name.clone()).collect(),
        start: format_utc(start),
        end: format_utc(end),
        step_seconds,
        min_elevation_deg,
        region,
        grid_step_deg,
        total_points: points.len(),
        covered_points,
        uncovered_points: points.len().saturating_sub(covered_points),
        multi_covered_points,
        coverage_ratio,
        average_visible_seconds,
        max_visible_seconds: max_visible,
        total_visible_seconds: total_visible,
        satellite_stats,
        points,
    })
}

/// 计算单个网格点对单颗卫星的可见时长、过境次数与最大仰角。
fn point_visibility(
    series: &[[f64; 3]],
    station_position: [f64; 3],
    up: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
    min_elevation_deg: f64,
    step_seconds: f64,
) -> (f64, u32, f64) {
    let mut visible_seconds = 0.0_f64;
    let mut access_count = 0u32;
    let mut point_max_elevation = f64::NEG_INFINITY;
    let mut previous_visible: Option<bool> = None;
    let mut previous_elevation = 0.0_f64;

    for satellite_position in series.iter() {
        let (elevation, _, _) =
            look_angles_with_basis(*satellite_position, station_position, up, east, north);
        if elevation > point_max_elevation {
            point_max_elevation = elevation;
        }
        let visible =
            elevation >= min_elevation_deg && !earth_occluded(station_position, *satellite_position);

        if let Some(was_visible) = previous_visible {
            if visible != was_visible {
                let denominator = elevation - previous_elevation;
                let fraction = if denominator.abs() > 1e-9 {
                    ((min_elevation_deg - previous_elevation) / denominator).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                if visible {
                    visible_seconds += fraction * step_seconds;
                } else {
                    visible_seconds += (1.0 - fraction) * step_seconds;
                }
            } else if visible {
                visible_seconds += step_seconds;
            }
        }
        if visible && previous_visible != Some(true) {
            access_count += 1;
        }
        previous_visible = Some(visible);
        previous_elevation = elevation;
    }

    (
        visible_seconds,
        access_count,
        if point_max_elevation.is_finite() {
            point_max_elevation
        } else {
            0.0
        },
    )
}

/// 供环境模型使用：按时间均匀采样卫星的地固系位置、速度与大地坐标。
pub(crate) fn sample_states(
    satellite: &SatelliteObject,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
) -> Result<Vec<(f64, [f64; 3], [f64; 3], [f64; 3])>, String> {
    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    let count = sample_count(total_seconds, step_seconds)?;

    if satellite.propagator_type == "numerical" {
        let track = numerical::propagate(satellite, start, total_seconds, step_seconds)?;
        let mut states = Vec::with_capacity(track.states.len());
        for state in &track.states {
            let geodetic = ecef_to_geodetic(state.position)?;
            states.push((state.t_seconds, state.position, state.velocity, geodetic));
        }
        return Ok(states);
    }

    let (elements, constants) = prepare(satellite)?;

    let mut states = Vec::with_capacity(count);
    for sample_index in 0..count {
        let t_seconds = sample_index as f64 * step_seconds;
        let time = start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
        let (position, velocity) = propagate_state(&elements, &constants, time)?;
        let geodetic = ecef_to_geodetic(position)?;
        states.push((t_seconds, position, velocity, geodetic));
    }
    Ok(states)
}

/// 传播器对比的单点结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareSample {
    pub t_seconds: f64,
    pub sgp4_longitude_deg: f64,
    pub sgp4_latitude_deg: f64,
    pub sgp4_altitude_km: f64,
    pub numerical_longitude_deg: f64,
    pub numerical_latitude_deg: f64,
    pub numerical_altitude_km: f64,
    pub position_difference_km: f64,
    pub velocity_difference_km_s: f64,
}

/// 传播器对比统计。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompareStatistics {
    pub mean_position_difference_km: f64,
    pub max_position_difference_km: f64,
    pub rms_position_difference_km: f64,
    pub mean_velocity_difference_km_s: f64,
    pub max_velocity_difference_km_s: f64,
    pub rms_velocity_difference_km_s: f64,
}

/// SGP4 与数值积分对比报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationCompareReport {
    pub satellite_id: String,
    pub satellite_name: String,
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub numerical_model: String,
    pub j2_enabled: bool,
    pub drag_enabled: bool,
    pub integration: IntegrationStats,
    pub statistics: CompareStatistics,
    pub sgp4_track: SatelliteTrack,
    pub numerical_track: SatelliteTrack,
    pub samples: Vec<CompareSample>,
}

/// 对同一时间窗口同时计算 SGP4 与数值积分，并统计位置/速度差异。
pub fn compare_propagation(
    satellite: &SatelliteObject,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
) -> Result<PropagationCompareReport, String> {
    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    let count = sample_count(total_seconds, step_seconds)?;

    let (elements, constants) = prepare(satellite)?;
    let numerical = numerical::propagate(satellite, start, total_seconds, step_seconds)?;
    if numerical.states.len() != count {
        return Err("数值积分采样数量与预期不一致".into());
    }

    let mut sgp4_samples = Vec::with_capacity(count);
    let mut numerical_samples = Vec::with_capacity(count);
    let mut compare_samples = Vec::with_capacity(count);
    let mut previous_sgp4_longitude: Option<f64> = None;
    let mut previous_numerical_longitude: Option<f64> = None;

    let mut position_sum = 0.0_f64;
    let mut position_sq_sum = 0.0_f64;
    let mut position_max = 0.0_f64;
    let mut velocity_sum = 0.0_f64;
    let mut velocity_sq_sum = 0.0_f64;
    let mut velocity_max = 0.0_f64;

    for sample_index in 0..count {
        let t_seconds = sample_index as f64 * step_seconds;
        let time = start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
        let (sgp4_position, sgp4_velocity) = propagate_state(&elements, &constants, time)?;
        let numerical_state = &numerical.states[sample_index];

        let [mut sgp4_longitude, sgp4_latitude, sgp4_altitude] =
            ecef_to_geodetic(sgp4_position)?;
        let [mut numerical_longitude, numerical_latitude, numerical_altitude] =
            ecef_to_geodetic(numerical_state.position)?;

        if let Some(previous) = previous_sgp4_longitude {
            while sgp4_longitude - previous > 180.0 {
                sgp4_longitude -= 360.0;
            }
            while sgp4_longitude - previous < -180.0 {
                sgp4_longitude += 360.0;
            }
        }
        previous_sgp4_longitude = Some(sgp4_longitude);
        if let Some(previous) = previous_numerical_longitude {
            while numerical_longitude - previous > 180.0 {
                numerical_longitude -= 360.0;
            }
            while numerical_longitude - previous < -180.0 {
                numerical_longitude += 360.0;
            }
        }
        previous_numerical_longitude = Some(numerical_longitude);

        let position_difference = distance(sgp4_position, numerical_state.position);
        let velocity_difference = distance(sgp4_velocity, numerical_state.velocity);
        position_sum += position_difference;
        position_sq_sum += position_difference * position_difference;
        position_max = position_max.max(position_difference);
        velocity_sum += velocity_difference;
        velocity_sq_sum += velocity_difference * velocity_difference;
        velocity_max = velocity_max.max(velocity_difference);

        sgp4_samples.push(TrackSample {
            t_seconds,
            longitude_deg: sgp4_longitude,
            latitude_deg: sgp4_latitude,
            altitude_km: sgp4_altitude,
        });
        numerical_samples.push(TrackSample {
            t_seconds,
            longitude_deg: numerical_longitude,
            latitude_deg: numerical_latitude,
            altitude_km: numerical_altitude,
        });
        compare_samples.push(CompareSample {
            t_seconds,
            sgp4_longitude_deg: sgp4_longitude,
            sgp4_latitude_deg: sgp4_latitude,
            sgp4_altitude_km: sgp4_altitude,
            numerical_longitude_deg: numerical_longitude,
            numerical_latitude_deg: numerical_latitude,
            numerical_altitude_km: numerical_altitude,
            position_difference_km: position_difference,
            velocity_difference_km_s: velocity_difference,
        });
    }

    let divisor = count as f64;
    let statistics = CompareStatistics {
        mean_position_difference_km: position_sum / divisor,
        max_position_difference_km: position_max,
        rms_position_difference_km: (position_sq_sum / divisor).sqrt(),
        mean_velocity_difference_km_s: velocity_sum / divisor,
        max_velocity_difference_km_s: velocity_max,
        rms_velocity_difference_km_s: (velocity_sq_sum / divisor).sqrt(),
    };

    let sgp4_track = SatelliteTrack {
        satellite_id: format!("{}::sgp4", satellite.id),
        name: format!("{} (SGP4)", satellite.name),
        color: "#4fc3f7".to_string(),
        samples: sgp4_samples,
    };
    let numerical_track = SatelliteTrack {
        satellite_id: format!("{}::numerical", satellite.id),
        name: format!("{} (数值积分)", satellite.name),
        color: "#ffb74d".to_string(),
        samples: numerical_samples,
    };

    Ok(PropagationCompareReport {
        satellite_id: satellite.id.clone(),
        satellite_name: satellite.name.clone(),
        start: format_utc(start),
        end: format_utc(end),
        step_seconds,
        numerical_model: numerical.model,
        j2_enabled: numerical.j2_enabled,
        drag_enabled: numerical.drag_enabled,
        integration: numerical.stats,
        statistics,
        sgp4_track,
        numerical_track,
        samples: compare_samples,
    })
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
