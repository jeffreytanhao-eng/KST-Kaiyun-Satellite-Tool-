//! 星座构型生成器。
//!
//! 支持 Walker Delta、Walker Star 与街道式（Streets of Coverage）三种经典构型，
//! 生成可直接参与 SGP4 传播与 Access/覆盖计算的卫星对象。
//!
//! 参考：
//! - Walker, J. G. "Satellite constellations." Journal of the British Interplanetary
//!   Society 37 (1984): 559-572.
//! - Wertz, J. R. "Coverage, responsiveness and the streets of coverage."
//!   Journal of the British Interplanetary Society 54 (2001): 279-284.
//!
//! 生成器只按开普勒圆轨道计算标称轨道参数并写入 TLE；后续 J2/大气等摄动由
//! 现有 SGP4 传播器处理。

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::scenario::{ConstellationObject, SatelliteObject};

/// 地球引力常数（km³/s²）。
const EARTH_MU: f64 = 398_600.4418;
/// 地球赤道半径（km）。
const EARTH_RADIUS_KM: f64 = 6378.1366;

const PLANE_COLORS: [&str; 12] = [
    "#4fc3f7", "#ffb74d", "#81c784", "#e57373", "#ba68c8", "#4db6ac",
    "#ffd54f", "#90a4ae", "#aed581", "#f06292", "#7986cb", "#ff8a65",
];

/// 星座生成请求。
#[derive(Debug, Clone, Deserialize)]
pub struct ConstellationRequest {
    pub name: String,
    /// `walker_delta`、`walker_star` 或 `streets`。
    pub constellation_type: String,
    pub altitude_km: f64,
    pub inclination_deg: f64,
    pub total_satellites: u32,
    pub planes: u32,
    #[serde(default)]
    pub phasing: u32,
    #[serde(default)]
    pub color_by_plane: bool,
    #[serde(default)]
    pub epoch_utc: Option<String>,
    /// 街道式构型下每街卫星数；缺省时由总数与街道数推导。
    #[serde(default)]
    pub satellites_per_plane: Option<u32>,
}

/// 单个轨道面信息。
#[derive(Debug, Clone, Serialize)]
pub struct ConstellationPlane {
    pub index: u32,
    pub raan_deg: f64,
    pub color: String,
    pub satellite_ids: Vec<String>,
}

/// 星座生成结果。
#[derive(Debug, Clone, Serialize)]
pub struct ConstellationGenerationReport {
    pub constellation_type: String,
    pub altitude_km: f64,
    pub inclination_deg: f64,
    pub total_satellites: u32,
    pub planes: u32,
    pub phasing: u32,
    pub satellites_per_plane: u32,
    pub orbital_period_minutes: f64,
    pub epoch_utc: String,
    pub color_by_plane: bool,
    pub note: String,
    pub constellation: ConstellationObject,
    pub plane_details: Vec<ConstellationPlane>,
    pub satellites: Vec<SatelliteObject>,
}

fn checksum(line: &str) -> char {
    let sum: u32 = line
        .chars()
        .map(|ch| match ch {
            '0'..='9' => ch as u32 - '0' as u32,
            '-' => 1,
            _ => 0,
        })
        .sum();
    char::from_digit(sum % 10, 10).unwrap_or('0')
}

fn epoch_fields(epoch: DateTime<Utc>) -> (String, String) {
    let year = format!("{:02}", epoch.year().rem_euclid(100));
    let seconds = epoch.num_seconds_from_midnight() as f64
        + epoch.nanosecond() as f64 / 1_000_000_000.0;
    let mut day = epoch.ordinal() as f64 + seconds / 86_400.0;
    if day >= 367.0 {
        day -= 365.0;
    }
    let day_integer = day.floor() as u32;
    let mut fraction = ((day - day_integer as f64) * 1e8).round() as u64;
    let mut day_integer = day_integer;
    if fraction >= 100_000_000 {
        fraction = 0;
        day_integer += 1;
    }
    (year, format!("{:03}.{:08}", day_integer, fraction))
}

/// 构造一行符合 NORAD 规范的 TLE。
#[allow(clippy::too_many_arguments)]
fn build_tle(
    norad: u32,
    epoch: DateTime<Utc>,
    inclination_deg: f64,
    raan_deg: f64,
    eccentricity: f64,
    arg_perigee_deg: f64,
    mean_anomaly_deg: f64,
    mean_motion_rev_day: f64,
) -> (String, String) {
    let (year, day) = epoch_fields(epoch);

    let mut line1 = String::with_capacity(69);
    line1.push_str("1 ");
    line1.push_str(&format!("{:05}", norad % 100_000));
    line1.push('U');
    line1.push(' ');
    line1.push_str(&format!("{:02}001A  ", year.parse::<u32>().unwrap_or(0)));
    line1.push(' ');
    line1.push_str(&year);
    line1.push_str(&day);
    line1.push(' ');
    line1.push_str(" .00000000");
    line1.push(' ');
    line1.push_str(" 00000+0");
    line1.push(' ');
    line1.push_str(" 00000+0");
    line1.push(' ');
    line1.push('0');
    line1.push(' ');
    line1.push_str(" 999");
    line1.push(checksum(&line1));

    let mut line2 = String::with_capacity(69);
    line2.push_str("2 ");
    line2.push_str(&format!("{:05}", norad % 100_000));
    line2.push(' ');
    line2.push_str(&format!("{:8.4}", inclination_deg));
    line2.push(' ');
    line2.push_str(&format!("{:8.4}", raan_deg.rem_euclid(360.0)));
    line2.push(' ');
    let eccentricity_digits = (eccentricity.clamp(0.0, 0.999_999_9) * 1e7).round() as u64;
    line2.push_str(&format!("{:07}", eccentricity_digits.min(9_999_999)));
    line2.push(' ');
    line2.push_str(&format!("{:8.4}", arg_perigee_deg.rem_euclid(360.0)));
    line2.push(' ');
    line2.push_str(&format!("{:8.4}", mean_anomaly_deg.rem_euclid(360.0)));
    line2.push(' ');
    line2.push_str(&format!("{:11.8}", mean_motion_rev_day));
    line2.push_str(&format!("{:>5}", 0));
    line2.push(checksum(&line2));

    (line1, line2)
}

fn validate(request: &ConstellationRequest) -> Result<(u32, u32), String> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err("星座名称不能为空".into());
    }
    if name.chars().count() > 60 {
        return Err("星座名称不能超过 60 个字符".into());
    }
    if !request.altitude_km.is_finite() || !(160.0..=50_000.0).contains(&request.altitude_km) {
        return Err("轨道高度需在 [160, 50000] km 内".into());
    }
    if !request.inclination_deg.is_finite() || !(0.0..=180.0).contains(&request.inclination_deg) {
        return Err("倾角需在 [0, 180] 度内".into());
    }
    if !(2..=100).contains(&request.total_satellites) {
        return Err("卫星总数需在 [2, 100] 内".into());
    }
    if request.planes == 0 || request.planes > request.total_satellites {
        return Err("轨道面数需在 [1, 卫星总数] 内".into());
    }
    if request.phasing >= request.planes {
        return Err("相位因子需小于轨道面数".into());
    }
    match request.constellation_type.as_str() {
        "walker_delta" | "walker_star" => {
            if request.total_satellites % request.planes != 0 {
                return Err("Walker 构型的卫星总数必须能被轨道面数整除".into());
            }
            Ok((request.planes, request.total_satellites / request.planes))
        }
        "streets" => {
            let per_plane = request.satellites_per_plane.unwrap_or_else(|| {
                (request.total_satellites / request.planes).max(1)
            });
            if per_plane == 0 {
                return Err("街道式构型每街卫星数需大于 0".into());
            }
            if per_plane * request.planes != request.total_satellites {
                return Err("街道式构型需满足：街道数 × 每街卫星数 = 卫星总数".into());
            }
            Ok((request.planes, per_plane))
        }
        other => Err(format!("未知星座构型：{other}")),
    }
}

/// 生成星座卫星对象。
pub fn generate(request: &ConstellationRequest) -> Result<ConstellationGenerationReport, String> {
    let (plane_count, per_plane) = validate(request)?;
    let epoch = match request.epoch_utc.as_deref() {
        Some(raw) if !raw.trim().is_empty() => DateTime::parse_from_rfc3339(raw.trim())
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| "星座历元需为 RFC3339 格式时间".to_string())?,
        _ => Utc::now(),
    };

    let semi_major_axis = EARTH_RADIUS_KM + request.altitude_km;
    let mean_motion_rad_s = (EARTH_MU / semi_major_axis.powi(3)).sqrt();
    let mean_motion_rev_day = mean_motion_rad_s * 86_400.0 / (2.0 * std::f64::consts::PI);
    let period_minutes = 1440.0 / mean_motion_rev_day;

    let raan_spread = match request.constellation_type.as_str() {
        "walker_delta" => 360.0,
        _ => 180.0,
    };
    let total = request.total_satellites as f64;
    let planes = plane_count as f64;

    let mut satellites = Vec::with_capacity(request.total_satellites as usize);
    let mut plane_details = Vec::with_capacity(plane_count as usize);
    let mut index: u32 = 0;

    for plane in 0..plane_count {
        let raan_deg = if plane_count == 1 {
            0.0
        } else {
            plane as f64 * raan_spread / planes
        };
        let plane_color = PLANE_COLORS[plane as usize % PLANE_COLORS.len()].to_string();
        let mut plane_ids = Vec::with_capacity(per_plane as usize);

        for slot in 0..per_plane {
            // Walker：面内均匀分布，并按相位因子在面间错开。
            // 街道式：在面内均匀分布的基础上，按街道顺序做更细的均摊，避免覆盖间隙同步。
            let mean_anomaly_deg = match request.constellation_type.as_str() {
                "streets" => {
                    slot as f64 * 360.0 / per_plane as f64
                        + plane as f64 * 360.0 / (per_plane as f64 * planes)
                }
                _ => {
                    slot as f64 * 360.0 / per_plane as f64
                        + request.phasing as f64 * plane as f64 * 360.0 / total
                }
            };

            let norad = 70_000 + index;
            let (line1, line2) = build_tle(
                norad,
                epoch,
                request.inclination_deg,
                raan_deg,
                0.0001,
                0.0,
                mean_anomaly_deg,
                mean_motion_rev_day,
            );
            sgp4::Elements::from_tle(None, line1.as_bytes(), line2.as_bytes())
                .map_err(|err| format!("生成的 TLE 无法被 SGP4 解析：{err}"))?;

            let id = Uuid::new_v4().to_string();
            let name = format!(
                "{} · P{}-S{}",
                request.name.trim(),
                plane + 1,
                slot + 1
            );
            let color = if request.color_by_plane {
                plane_color.clone()
            } else {
                PLANE_COLORS[index as usize % PLANE_COLORS.len()].to_string()
            };
            plane_ids.push(id.clone());
            satellites.push(SatelliteObject {
                id,
                name,
                enabled: true,
                color,
                tle_line1: line1,
                tle_line2: line2,
                norad_id: Some(norad as u64),
                propagator_type: "sgp4".to_string(),
                attitude_mode: "nadir".to_string(),
                attitude_roll_deg: 0.0,
                attitude_pitch_deg: 0.0,
                attitude_yaw_deg: 0.0,
                mass_kg: 500.0,
                area_m2: 2.0,
                drag_coefficient: 2.2,
                integrator_tolerance: 1e-9,
                enable_j2: true,
                enable_drag: true,
                link_options: None,
                radar_options: None,
            });
            index += 1;
        }

        plane_details.push(ConstellationPlane {
            index: plane + 1,
            raan_deg: raan_deg.rem_euclid(360.0),
            color: plane_color,
            satellite_ids: plane_ids,
        });
    }

    let constellation = ConstellationObject {
        id: Uuid::new_v4().to_string(),
        name: request.name.trim().to_string(),
        enabled: true,
        constellation_type: request.constellation_type.clone(),
        altitude_km: request.altitude_km,
        inclination_deg: request.inclination_deg,
        total_satellites: request.total_satellites,
        planes: plane_count,
        phasing: request.phasing,
        color_by_plane: request.color_by_plane,
        epoch_utc: Some(epoch.to_rfc3339()),
        satellite_ids: satellites.iter().map(|item| item.id.clone()).collect(),
    };

    let note = match request.constellation_type.as_str() {
        "walker_delta" => "Walker Delta：轨道面均匀分布于 360°，面内卫星等间隔并按相位因子错开。",
        "walker_star" => "Walker Star：轨道面分布于 180°（极轨构型），相邻面反向覆盖。",
        _ => "街道式覆盖：轨道面均匀分布于 180°，相邻街道按细分相位错开以降低同步间隙。",
    };

    Ok(ConstellationGenerationReport {
        constellation_type: request.constellation_type.clone(),
        altitude_km: request.altitude_km,
        inclination_deg: request.inclination_deg,
        total_satellites: request.total_satellites,
        planes: plane_count,
        phasing: request.phasing,
        satellites_per_plane: per_plane,
        orbital_period_minutes: period_minutes,
        epoch_utc: epoch.to_rfc3339(),
        color_by_plane: request.color_by_plane,
        note: note.to_string(),
        constellation,
        plane_details,
        satellites,
    })
}
