use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sgp4::{Constants, Elements};
use stkcraft_geocalc::coords::{to_coordinates, CoordinateSystem};

use crate::scenario::{GroundStationObject, ScenarioData, SatelliteObject};

/// WGS84/IAU 地球赤道半径与极半径（km）。
const EARTH_RADII: [f64; 3] = [6378.1366, 6378.1366, 6356.7519];
/// 单个任务的采样点上限，避免异常请求拖垮服务。
const MAX_SAMPLES: usize = 20_000;

const FALLBACK_COLORS: [&str; 6] = [
    "#4fc3f7", "#ffb74d", "#81c784", "#e57373", "#ba68c8", "#4db6ac",
];

#[derive(Debug, Clone, Serialize)]
pub struct TrackSample {
    pub t_seconds: f64,
    pub longitude_deg: f64,
    pub latitude_deg: f64,
    pub altitude_km: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SatelliteTrack {
    pub satellite_id: String,
    pub name: String,
    pub color: String,
    pub samples: Vec<TrackSample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropagationReport {
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub satellites: Vec<SatelliteTrack>,
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
    pub windows: Vec<AccessWindow>,
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

/// 解析 TLE 并构建 SGP4 传播器。
fn prepare(satellite: &SatelliteObject) -> Result<(Elements, Constants<'_>), String> {
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
fn gmst_radians(time: DateTime<Utc>) -> f64 {
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

/// 传播单个时刻并返回 TEME→ECEF 后的位置（km）。
fn propagate_ecef(
    elements: &Elements,
    constants: &Constants,
    time: DateTime<Utc>,
) -> Result<[f64; 3], String> {
    let minutes_since_epoch =
        (time.naive_utc() - elements.datetime).num_milliseconds() as f64 / 60_000.0;
    let prediction = constants
        .propagate(minutes_since_epoch)
        .map_err(|err| format!("SGP4 传播失败：{err}"))?;
    Ok(teme_to_ecef(prediction.position, gmst_radians(time)))
}

/// ECEF 位置 → [经度(度), 纬度(度), 高度(km)]。
fn ecef_to_geodetic(position: [f64; 3]) -> Result<[f64; 3], String> {
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
    let equatorial = EARTH_RADII[0];
    let flattening = (EARTH_RADII[0] - EARTH_RADII[2]) / EARTH_RADII[0];
    let eccentricity_sq = flattening * (2.0 - flattening);

    let latitude = station.latitude_deg.to_radians();
    let longitude = station.longitude_deg.to_radians();
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();

    let prime_vertical =
        equatorial / (1.0 - eccentricity_sq * sin_lat * sin_lat).sqrt();
    [
        (prime_vertical + station.altitude_km) * cos_lat * cos_lon,
        (prime_vertical + station.altitude_km) * cos_lat * sin_lon,
        (prime_vertical * (1.0 - eccentricity_sq) + station.altitude_km) * sin_lat,
    ]
}

/// 地固系下计算仰角、方位角与斜距。
pub fn look_angles(
    satellite_ecef: [f64; 3],
    station: &GroundStationObject,
) -> (f64, f64, f64) {
    let station_position = station_ecef(station);
    let range_vector = [
        satellite_ecef[0] - station_position[0],
        satellite_ecef[1] - station_position[1],
        satellite_ecef[2] - station_position[2],
    ];
    let range = (range_vector[0] * range_vector[0]
        + range_vector[1] * range_vector[1]
        + range_vector[2] * range_vector[2])
        .sqrt();

    let latitude = station.latitude_deg.to_radians();
    let longitude = station.longitude_deg.to_radians();
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();

    let up = [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat];
    let east = [-sin_lon, cos_lon, 0.0];
    let north = [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat];

    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

    let elevation = if range > 0.0 {
        (dot(range_vector, up) / range).clamp(-1.0, 1.0).asin().to_degrees()
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
    for (index, satellite) in data.satellites.iter().enumerate() {
        let selected = match satellite_ids {
            Some(ids) => ids.iter().any(|id| id == &satellite.id),
            None => satellite.enabled,
        };
        if !selected {
            continue;
        }

        let (elements, constants) = prepare(satellite)?;
        let mut samples = Vec::with_capacity(count);
        let mut previous_longitude: Option<f64> = None;

        for sample_index in 0..count {
            let t_seconds = sample_index as f64 * step_seconds;
            let time = start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
            let ecef = propagate_ecef(&elements, &constants, time)?;
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

            samples.push(TrackSample {
                t_seconds,
                longitude_deg: longitude,
                latitude_deg: latitude,
                altitude_km: altitude,
            });
        }

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
    })
}

/// 计算卫星与地面站在时间窗口内的 Access 窗口。
pub fn compute_access(
    data: &ScenarioData,
    satellite_id: &str,
    ground_station_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    min_elevation_override: Option<f64>,
) -> Result<AccessReport, String> {
    let satellite = data
        .satellite(satellite_id)
        .ok_or_else(|| format!("场景中不存在卫星 {satellite_id}"))?;
    let station = data
        .ground_station(ground_station_id)
        .ok_or_else(|| format!("场景中不存在地面站 {ground_station_id}"))?;

    let min_elevation = min_elevation_override.unwrap_or(station.min_elevation_deg);
    if !min_elevation.is_finite() || !(0.0..=90.0).contains(&min_elevation) {
        return Err("最小仰角需在 [0, 90] 度内".into());
    }

    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    let count = sample_count(total_seconds, step_seconds)?;

    let (elements, constants) = prepare(satellite)?;

    struct ActiveWindow {
        start_seconds: f64,
        max_elevation_deg: f64,
    }

    let mut windows = Vec::new();
    let mut active: Option<ActiveWindow> = None;
    let mut previous: Option<(f64, f64)> = None;

    let crossing = |t0: f64, e0: f64, t1: f64, e1: f64| -> f64 {
        if (e1 - e0).abs() < 1e-9 {
            return t1;
        }
        let fraction = ((min_elevation - e0) / (e1 - e0)).clamp(0.0, 1.0);
        t0 + (t1 - t0) * fraction
    };

    let push_window = |start_seconds: f64, end_seconds: f64, max_elevation_deg: f64, windows: &mut Vec<AccessWindow>| {
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
        });
    };

    for sample_index in 0..count {
        let t_seconds = sample_index as f64 * step_seconds;
        let time = start + ChronoDuration::milliseconds((t_seconds * 1000.0).round() as i64);
        let ecef = propagate_ecef(&elements, &constants, time)?;
        let (elevation, _, _) = look_angles(ecef, station);

        if elevation >= min_elevation {
            match active.as_mut() {
                Some(window) => {
                    if elevation > window.max_elevation_deg {
                        window.max_elevation_deg = elevation;
                    }
                }
                None => {
                    let start_seconds = match previous {
                        Some((previous_t, previous_elevation)) => {
                            crossing(previous_t, previous_elevation, t_seconds, elevation)
                        }
                        None => t_seconds,
                    };
                    active = Some(ActiveWindow {
                        start_seconds,
                        max_elevation_deg: elevation,
                    });
                }
            }
        } else if let Some(window) = active.take() {
            let end_seconds = match previous {
                Some((previous_t, previous_elevation)) => {
                    crossing(previous_t, previous_elevation, t_seconds, elevation)
                }
                None => t_seconds,
            };
            push_window(
                window.start_seconds,
                end_seconds,
                window.max_elevation_deg,
                &mut windows,
            );
        }

        previous = Some((t_seconds, elevation));
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
        windows,
    })
}
