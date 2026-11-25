use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::orbit;
use crate::scenario::SatelliteObject;
use crate::spaceenv::{
    nrlmsise00, Nrlmsise00Activity, Nrlmsise00Ap, Nrlmsise00Input,
};

const EARTH_GM_M3_S2: f64 = 3.986_004_355_07e14;
const EARTH_ROTATION_RAD_S: f64 = 7.292_115_9e-5;

/// 环境模型输入，可由请求覆盖，缺省使用中等太阳活动水平。
#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentOptions {
    #[serde(default = "default_f107")]
    pub daily_f107_sfu: f64,
    #[serde(default = "default_f107")]
    pub mean81_f107_sfu: f64,
    #[serde(default = "default_ap")]
    pub ap: f64,
    #[serde(default = "default_mass")]
    pub mass_kg: f64,
    #[serde(default = "default_area")]
    pub area_m2: f64,
    #[serde(default = "default_cd")]
    pub drag_coefficient: f64,
}

fn default_f107() -> f64 {
    150.0
}

fn default_ap() -> f64 {
    4.0
}

fn default_mass() -> f64 {
    500.0
}

fn default_area() -> f64 {
    2.0
}

fn default_cd() -> f64 {
    2.2
}

impl Default for EnvironmentOptions {
    fn default() -> Self {
        Self {
            daily_f107_sfu: default_f107(),
            mean81_f107_sfu: default_f107(),
            ap: default_ap(),
            mass_kg: default_mass(),
            area_m2: default_area(),
            drag_coefficient: default_cd(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSample {
    pub t_seconds: f64,
    pub altitude_km: f64,
    /// 一阶阻力修正后的高度估算。
    pub corrected_altitude_km: f64,
    pub density_kg_m3: f64,
    pub temperature_k: f64,
    pub drag_accel_m_s2: f64,
    pub cumulative_delta_v_m_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentReport {
    pub satellite_id: String,
    pub satellite_name: String,
    pub model: String,
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub daily_f107_sfu: f64,
    pub mean81_f107_sfu: f64,
    pub ap: f64,
    pub mass_kg: f64,
    pub area_m2: f64,
    pub drag_coefficient: f64,
    pub mean_density_kg_m3: f64,
    pub min_density_kg_m3: f64,
    pub max_density_kg_m3: f64,
    pub delta_v_m_s: f64,
    pub estimated_altitude_loss_km: f64,
    pub start_altitude_km: f64,
    pub end_altitude_km: f64,
    pub corrected_end_altitude_km: f64,
    pub note: String,
    pub samples: Vec<EnvironmentSample>,
}

/// 调用 STKCraft-SpaceEnv 的 NRLMSISE-00，输出大气密度与一阶阻力影响对比。
pub fn analyze_environment(
    satellite: &SatelliteObject,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    options: &EnvironmentOptions,
) -> Result<EnvironmentReport, String> {
    if !options.daily_f107_sfu.is_finite() || options.daily_f107_sfu <= 0.0 {
        return Err("F10.7 需为正数".into());
    }
    if !options.mean81_f107_sfu.is_finite() || options.mean81_f107_sfu <= 0.0 {
        return Err("81 天平均 F10.7 需为正数".into());
    }
    if !options.ap.is_finite() || options.ap < 0.0 {
        return Err("Ap 需为非负数".into());
    }
    if !options.mass_kg.is_finite() || options.mass_kg <= 0.0 {
        return Err("卫星质量需为正数".into());
    }
    if !options.area_m2.is_finite() || options.area_m2 <= 0.0 {
        return Err("迎风面积需为正数".into());
    }
    if !options.drag_coefficient.is_finite() || options.drag_coefficient <= 0.0 {
        return Err("阻力系数需为正数".into());
    }

    let states = orbit::sample_states(satellite, start, end, step_seconds)?;
    if states.is_empty() {
        return Err("没有可用的采样点".into());
    }
    for (_, _, _, geodetic) in &states {
        if geodetic[2] > 1_000.0 {
            return Err(format!(
                "NRLMSISE-00 仅支持 0–1000 km 高度，当前轨道高度 {:.1} km 超出范围",
                geodetic[2]
            ));
        }
    }

    let mut samples = Vec::with_capacity(states.len());
    let mut cumulative_delta_v = 0.0_f64;
    let mut cumulative_loss_m = 0.0_f64;
    let mut density_sum = 0.0_f64;
    let mut min_density = f64::INFINITY;
    let mut max_density: f64 = 0.0;
    let mut previous_t = 0.0_f64;

    for (t_seconds, position, velocity, geodetic) in &states {
        let [longitude_deg, latitude_deg, altitude_km] = *geodetic;
        let time =
            start + chrono::Duration::milliseconds((t_seconds * 1000.0).round() as i64);
        let day_of_year = time.ordinal() as u16;
        let utc_seconds = f64::from(time.num_seconds_from_midnight());
        let solar_hours =
            (utc_seconds / 3600.0 + longitude_deg / 15.0).rem_euclid(24.0);

        let input = Nrlmsise00Input {
            day_of_year,
            utc_seconds,
            altitude_m: altitude_km * 1000.0,
            latitude_rad: latitude_deg.to_radians(),
            longitude_rad: longitude_deg.to_radians(),
            local_apparent_solar_time_hours: solar_hours,
            activity: Nrlmsise00Activity {
                daily_f107_sfu: options.daily_f107_sfu,
                mean81_f107_sfu: options.mean81_f107_sfu,
                ap: Nrlmsise00Ap::Daily(options.ap),
            },
        };
        let state = nrlmsise00(&input)
            .map_err(|err| format!("大气密度计算失败（{day_of_year}/{:06.1}s）：{err}", utc_seconds))?;

        // 相对速度：地固系速度（ECI 轴）减去地球自转牵连速度，单位 m/s。
        let omega_cross_r = [
            -EARTH_ROTATION_RAD_S * position[1] * 1000.0,
            EARTH_ROTATION_RAD_S * position[0] * 1000.0,
            0.0,
        ];
        let relative_velocity = [
            velocity[0] * 1000.0 - omega_cross_r[0],
            velocity[1] * 1000.0 - omega_cross_r[1],
            velocity[2] * 1000.0 - omega_cross_r[2],
        ];
        let speed = (relative_velocity[0] * relative_velocity[0]
            + relative_velocity[1] * relative_velocity[1]
            + relative_velocity[2] * relative_velocity[2])
            .sqrt();
        let drag_accel = 0.5
            * state.effective_density_kg_m3
            * options.drag_coefficient
            * options.area_m2
            / options.mass_kg
            * speed
            * speed;

        let delta_t = (t_seconds - previous_t).max(0.0);
        cumulative_delta_v += drag_accel * delta_t;
        previous_t = *t_seconds;

        // 圆轨道一阶近似：da/dt = 2 a^1.5 * a_t / sqrt(mu)。
        let semi_major_m = (6_378_136.6 + altitude_km * 1000.0).max(1.0);
        let decay_rate = 2.0 * semi_major_m.powf(1.5) * drag_accel / EARTH_GM_M3_S2.sqrt();
        cumulative_loss_m += decay_rate * delta_t;
        let corrected_altitude_km = (altitude_km - cumulative_loss_m / 1000.0).max(0.0);

        density_sum += state.effective_density_kg_m3;
        min_density = min_density.min(state.effective_density_kg_m3);
        max_density = max_density.max(state.effective_density_kg_m3);

        samples.push(EnvironmentSample {
            t_seconds: *t_seconds,
            altitude_km,
            corrected_altitude_km,
            density_kg_m3: state.effective_density_kg_m3,
            temperature_k: state.temperature_k,
            drag_accel_m_s2: drag_accel,
            cumulative_delta_v_m_s: cumulative_delta_v,
        });
    }

    let start_altitude = samples.first().map(|item| item.altitude_km).unwrap_or(0.0);
    let end_altitude = samples.last().map(|item| item.altitude_km).unwrap_or(0.0);
    let corrected_end = samples
        .last()
        .map(|item| item.corrected_altitude_km)
        .unwrap_or(0.0);

    Ok(EnvironmentReport {
        satellite_id: satellite.id.clone(),
        satellite_name: satellite.name.clone(),
        model: "NRLMSISE-00 (STKCraft-SpaceEnv)".to_string(),
        start: start.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        end: end.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        step_seconds,
        daily_f107_sfu: options.daily_f107_sfu,
        mean81_f107_sfu: options.mean81_f107_sfu,
        ap: options.ap,
        mass_kg: options.mass_kg,
        area_m2: options.area_m2,
        drag_coefficient: options.drag_coefficient,
        mean_density_kg_m3: density_sum / samples.len() as f64,
        min_density_kg_m3: min_density,
        max_density_kg_m3: max_density,
        delta_v_m_s: cumulative_delta_v,
        estimated_altitude_loss_km: cumulative_loss_m / 1000.0,
        start_altitude_km: start_altitude,
        end_altitude_km: end_altitude,
        corrected_end_altitude_km: corrected_end,
        note: "大气密度来自 NRLMSISE-00 实际调用；阻力与高度衰减为一阶简化估算，用于对比展示而非精密定轨。"
            .to_string(),
        samples,
    })
}
