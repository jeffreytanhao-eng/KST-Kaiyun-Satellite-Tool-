//! 基础链路预算模型。
//!
//! 计算自由空间路径损耗（FSL）、简化大气衰减、接收功率、载噪比（C/N0）、
//! Eb/N0 与链路余量，并把结果关联到 Access 窗口逐窗统计。
//!
//! 所有计算均在 Rust 后端完成；链路参数由前端传入。

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::orbit::{compute_access, sample_states, station_ecef, AccessOptions};
use crate::scenario::ScenarioData;

/// 光速（m/s）。
const SPEED_OF_LIGHT: f64 = 299_792_458.0;
/// 玻尔兹曼常数（J/K）。
const BOLTZMANN: f64 = 1.380_649e-23;

/// 链路预算参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkOptions {
    /// 载波频率（Hz）。
    pub frequency_hz: f64,
    /// 发射功率（dBW）。
    pub tx_power_dbw: f64,
    /// 发射天线增益（dBi）。
    pub tx_gain_db: f64,
    /// 接收天线增益（dBi）。
    pub rx_gain_db: f64,
    /// 杂项损耗（dB，馈线/指向等）。
    pub misc_loss_db: f64,
    /// 接收系统噪声温度（K）。
    pub system_noise_temp_k: f64,
    /// 接收带宽（Hz）。
    pub bandwidth_hz: f64,
    /// 所需 C/N0（dB-Hz），用于计算链路余量。
    pub required_cn0_dbhz: f64,
}

impl Default for LinkOptions {
    fn default() -> Self {
        Self {
            frequency_hz: 2.2e9,
            tx_power_dbw: 10.0,
            tx_gain_db: 30.0,
            rx_gain_db: 35.0,
            misc_loss_db: 1.0,
            system_noise_temp_k: 500.0,
            bandwidth_hz: 1.0e6,
            required_cn0_dbhz: 60.0,
        }
    }
}

impl LinkOptions {
    fn validate(&self) -> Result<(), String> {
        if !self.frequency_hz.is_finite() || !(1.0e6..=1.0e12).contains(&self.frequency_hz) {
            return Err("频率需在 1 MHz ~ 1 THz 内".into());
        }
        for (value, label) in [
            (self.tx_power_dbw, "发射功率"),
            (self.tx_gain_db, "发射增益"),
            (self.rx_gain_db, "接收增益"),
            (self.misc_loss_db, "杂项损耗"),
            (self.required_cn0_dbhz, "所需 C/N0"),
        ] {
            if !value.is_finite() || !(-300.0..=300.0).contains(&value) {
                return Err(format!("{label}超出合理范围"));
            }
        }
        if !self.system_noise_temp_k.is_finite() || !(1.0..=100_000.0).contains(&self.system_noise_temp_k)
        {
            return Err("系统噪声温度需在 [1, 100000] K 内".into());
        }
        if !self.bandwidth_hz.is_finite() || !(1.0..=1.0e12).contains(&self.bandwidth_hz) {
            return Err("带宽需在 1 Hz ~ 1 THz 内".into());
        }
        Ok(())
    }
}

/// 单个链路采样点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkSample {
    pub t_seconds: f64,
    pub elevation_deg: f64,
    pub range_km: f64,
    pub free_space_loss_db: f64,
    pub atmospheric_loss_db: f64,
    pub received_power_dbw: f64,
    pub cn0_dbhz: f64,
    pub eb_n0_db: f64,
    pub margin_db: f64,
}

/// 与一个 Access 窗口关联的链路结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkWindow {
    pub index: u32,
    pub start_utc: String,
    pub end_utc: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
    pub min_elevation_deg: f64,
    pub max_elevation_deg: f64,
    pub min_range_km: f64,
    pub max_range_km: f64,
    pub min_margin_db: f64,
    pub mean_margin_db: f64,
    pub worst_elevation_deg: f64,
    pub samples: Vec<LinkSample>,
}

/// 链路预算报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkBudgetReport {
    pub satellite_id: String,
    pub satellite_name: String,
    pub ground_station_id: String,
    pub ground_station_name: String,
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub frequency_hz: f64,
    pub bandwidth_hz: f64,
    pub required_cn0_dbhz: f64,
    pub window_count: u32,
    pub total_duration_seconds: f64,
    pub min_margin_db: f64,
    pub mean_margin_db: f64,
    pub max_range_km: f64,
    pub mean_free_space_loss_db: f64,
    pub windows: Vec<LinkWindow>,
}

fn format_utc(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn look_angles(
    satellite: [f64; 3],
    station: [f64; 3],
    up: [f64; 3],
    east: [f64; 3],
    north: [f64; 3],
) -> (f64, f64) {
    let range_vector = [
        satellite[0] - station[0],
        satellite[1] - station[1],
        satellite[2] - station[2],
    ];
    let range = (range_vector[0] * range_vector[0]
        + range_vector[1] * range_vector[1]
        + range_vector[2] * range_vector[2])
        .sqrt();
    if range <= 0.0 {
        return (90.0, 0.0);
    }
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let elevation = (dot(range_vector, up) / range)
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees();
    let azimuth = dot(range_vector, east)
        .atan2(dot(range_vector, north))
        .to_degrees()
        .rem_euclid(360.0);
    (elevation, azimuth)
}

/// 单点链路预算。
fn evaluate_sample(
    options: &LinkOptions,
    t_seconds: f64,
    elevation_deg: f64,
    range_km: f64,
) -> LinkSample {
    let range_m = (range_km * 1000.0).max(1.0);
    let wavelength = SPEED_OF_LIGHT / options.frequency_hz;
    let free_space_loss_db = 20.0 * (4.0 * std::f64::consts::PI * range_m / wavelength).log10();

    // 简化大气衰减：天顶约 0.05 dB，按 1/sin(仰角) 放大，上限 1 dB。
    let elevation_rad = elevation_deg.clamp(0.1, 90.0).to_radians();
    let atmospheric_loss_db = (0.05 / elevation_rad.sin()).clamp(0.05, 1.0);

    let eirp_dbw = options.tx_power_dbw + options.tx_gain_db - options.misc_loss_db;
    let received_power_dbw =
        eirp_dbw + options.rx_gain_db - free_space_loss_db - atmospheric_loss_db;
    let noise_spectral_density_dbw_hz =
        10.0 * (BOLTZMANN * options.system_noise_temp_k).log10();
    let cn0_dbhz = received_power_dbw - noise_spectral_density_dbw_hz;
    let eb_n0_db = cn0_dbhz - 10.0 * options.bandwidth_hz.log10();
    let margin_db = cn0_dbhz - options.required_cn0_dbhz;

    LinkSample {
        t_seconds,
        elevation_deg,
        range_km,
        free_space_loss_db,
        atmospheric_loss_db,
        received_power_dbw,
        cn0_dbhz,
        eb_n0_db,
        margin_db,
    }
}

/// 计算卫星-地面站链路预算，并按 Access 窗口聚合。
pub fn compute_link_budget(
    data: &ScenarioData,
    satellite_id: &str,
    ground_station_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    options: &LinkOptions,
) -> Result<LinkBudgetReport, String> {
    options.validate()?;
    if !step_seconds.is_finite() || !(1.0..=3_600.0).contains(&step_seconds) {
        return Err("任务步长需在 [1, 3600] 秒内".into());
    }

    let satellite = data
        .satellite(satellite_id)
        .ok_or_else(|| format!("场景中不存在卫星 {satellite_id}"))?;
    let station = data
        .ground_station(ground_station_id)
        .ok_or_else(|| format!("场景中不存在地面站 {ground_station_id}"))?;

    let access_options = AccessOptions {
        min_elevation_deg: Some(station.min_elevation_deg),
        ..Default::default()
    };
    let access = compute_access(
        data,
        satellite_id,
        ground_station_id,
        start,
        end,
        step_seconds,
        &access_options,
    )?;

    let sample_step = (step_seconds / 2.0).clamp(1.0, 15.0);
    let states = sample_states(satellite, start, end, sample_step)?;

    let station_position = station_ecef(station);
    let latitude = station.latitude_deg.to_radians();
    let longitude = station.longitude_deg.to_radians();
    let (sin_lat, cos_lat) = latitude.sin_cos();
    let (sin_lon, cos_lon) = longitude.sin_cos();
    let up = [cos_lat * cos_lon, cos_lat * sin_lon, sin_lat];
    let east = [-sin_lon, cos_lon, 0.0];
    let north = [-sin_lat * cos_lon, -sin_lat * sin_lon, cos_lat];

    let mut windows = Vec::with_capacity(access.windows.len());
    let mut overall_min_margin = f64::INFINITY;
    let mut margin_sum = 0.0_f64;
    let mut margin_count = 0usize;
    let mut max_range = 0.0_f64;
    let mut fsl_sum = 0.0_f64;
    let mut fsl_count = 0usize;

    for window in &access.windows {
        let mut samples = Vec::new();
        for (t_seconds, position, _, _) in &states {
            if *t_seconds < window.start_seconds - 1e-6
                || *t_seconds > window.end_seconds + 1e-6
            {
                continue;
            }
            let (elevation, _) = look_angles(*position, station_position, up, east, north);
            let range_km = {
                let dx = position[0] - station_position[0];
                let dy = position[1] - station_position[1];
                let dz = position[2] - station_position[2];
                (dx * dx + dy * dy + dz * dz).sqrt()
            };
            samples.push(evaluate_sample(options, *t_seconds, elevation, range_km));
        }
        if samples.is_empty() {
            continue;
        }

        let mut min_elevation = f64::INFINITY;
        let mut max_elevation = f64::NEG_INFINITY;
        let mut min_range = f64::INFINITY;
        let mut window_max_range = 0.0_f64;
        let mut min_margin = f64::INFINITY;
        let mut window_margin_sum = 0.0_f64;
        let mut worst_elevation = 0.0_f64;

        for sample in &samples {
            min_elevation = min_elevation.min(sample.elevation_deg);
            max_elevation = max_elevation.max(sample.elevation_deg);
            min_range = min_range.min(sample.range_km);
            window_max_range = window_max_range.max(sample.range_km);
            window_margin_sum += sample.margin_db;
            if sample.margin_db < min_margin {
                min_margin = sample.margin_db;
                worst_elevation = sample.elevation_deg;
            }
            margin_sum += sample.margin_db;
            margin_count += 1;
            overall_min_margin = overall_min_margin.min(sample.margin_db);
            max_range = max_range.max(sample.range_km);
            fsl_sum += sample.free_space_loss_db;
            fsl_count += 1;
        }

        let mean_margin = window_margin_sum / samples.len() as f64;
        windows.push(LinkWindow {
            index: window.index,
            start_utc: window.start_utc.clone(),
            end_utc: window.end_utc.clone(),
            start_seconds: window.start_seconds,
            end_seconds: window.end_seconds,
            duration_seconds: window.duration_seconds,
            min_elevation_deg: min_elevation,
            max_elevation_deg: max_elevation,
            min_range_km: min_range,
            max_range_km: window_max_range,
            min_margin_db: min_margin,
            mean_margin_db: mean_margin,
            worst_elevation_deg: worst_elevation,
            samples,
        });
    }

    let total_duration: f64 = windows.iter().map(|window| window.duration_seconds).sum();

    Ok(LinkBudgetReport {
        satellite_id: satellite.id.clone(),
        satellite_name: satellite.name.clone(),
        ground_station_id: station.id.clone(),
        ground_station_name: station.name.clone(),
        start: format_utc(start),
        end: format_utc(end),
        step_seconds,
        frequency_hz: options.frequency_hz,
        bandwidth_hz: options.bandwidth_hz,
        required_cn0_dbhz: options.required_cn0_dbhz,
        window_count: windows.len() as u32,
        total_duration_seconds: total_duration,
        min_margin_db: if overall_min_margin.is_finite() {
            overall_min_margin
        } else {
            0.0
        },
        mean_margin_db: if margin_count > 0 {
            margin_sum / margin_count as f64
        } else {
            0.0
        },
        max_range_km: max_range,
        mean_free_space_loss_db: if fsl_count > 0 {
            fsl_sum / fsl_count as f64
        } else {
            0.0
        },
        windows,
    })
}

/// 导出链路预算 CSV。
pub fn link_csv(report: &LinkBudgetReport) -> String {
    let mut output = String::from(
        "window,start_utc,end_utc,t_seconds,elevation_deg,range_km,fsl_db,atmosphere_db,received_power_dbw,cn0_dbhz,eb_n0_db,margin_db\n",
    );
    for window in &report.windows {
        for sample in &window.samples {
            output.push_str(&format!(
                "{},{},{},{:.1},{:.4},{:.3},{:.3},{:.4},{:.3},{:.3},{:.3},{:.3}\n",
                window.index,
                window.start_utc,
                window.end_utc,
                sample.t_seconds,
                sample.elevation_deg,
                sample.range_km,
                sample.free_space_loss_db,
                sample.atmospheric_loss_db,
                sample.received_power_dbw,
                sample.cn0_dbhz,
                sample.eb_n0_db,
                sample.margin_db,
            ));
        }
    }
    output
}
