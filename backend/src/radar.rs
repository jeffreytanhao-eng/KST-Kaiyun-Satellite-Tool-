//! 基础雷达方程分析。
//!
//! 采用单站雷达方程：
//! `R_max = [Pt · Gt · Gr · λ² · σ / ((4π)³ · L · k·T0·B·F · SNR_min)]^(1/4)`
//!
//! 输出最大探测距离、探测窗口与逐采样点 SNR，可关联到现有 Access 窗口。

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::orbit::{compute_access, sample_states, station_ecef, AccessOptions};
use crate::scenario::ScenarioData;

const SPEED_OF_LIGHT: f64 = 299_792_458.0;
const BOLTZMANN: f64 = 1.380_649e-23;
const REFERENCE_TEMPERATURE_K: f64 = 290.0;

/// 雷达参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarOptions {
    /// 工作频率（Hz）。
    pub frequency_hz: f64,
    /// 发射峰值功率（W）。
    pub tx_power_w: f64,
    /// 发射天线增益（dBi）。
    pub tx_gain_db: f64,
    /// 接收天线增益（dBi）。
    pub rx_gain_db: f64,
    /// 系统损耗（dB）。
    pub system_loss_db: f64,
    /// 接收机噪声系数（dB）。
    pub noise_figure_db: f64,
    /// 接收带宽（Hz）。
    pub bandwidth_hz: f64,
    /// 目标雷达截面积（m²）。
    pub target_rcs_m2: f64,
    /// 检测所需最小 SNR（dB）。
    pub required_snr_db: f64,
    /// 相参积累脉冲数（>1 时提供 10log10(N) 增益）。
    #[serde(default = "default_pulses")]
    pub pulses_integrated: f64,
    /// 雷达最小工作仰角（度）；缺省使用地面站配置。
    #[serde(default)]
    pub min_elevation_deg: Option<f64>,
}

fn default_pulses() -> f64 {
    1.0
}

impl Default for RadarOptions {
    fn default() -> Self {
        Self {
            frequency_hz: 3.0e9,
            tx_power_w: 1_000.0,
            tx_gain_db: 35.0,
            rx_gain_db: 35.0,
            system_loss_db: 3.0,
            noise_figure_db: 3.0,
            bandwidth_hz: 1.0e6,
            target_rcs_m2: 1.0,
            required_snr_db: 13.0,
            pulses_integrated: 1.0,
            min_elevation_deg: None,
        }
    }
}

impl RadarOptions {
    fn validate(&self) -> Result<(), String> {
        if !self.frequency_hz.is_finite() || !(1.0e6..=1.0e12).contains(&self.frequency_hz) {
            return Err("雷达频率需在 1 MHz ~ 1 THz 内".into());
        }
        if !self.tx_power_w.is_finite() || !(1e-6..=1e9).contains(&self.tx_power_w) {
            return Err("发射功率需在 [1e-6, 1e9] W 内".into());
        }
        for (value, label) in [
            (self.tx_gain_db, "发射增益"),
            (self.rx_gain_db, "接收增益"),
            (self.system_loss_db, "系统损耗"),
            (self.required_snr_db, "检测所需 SNR"),
        ] {
            if !value.is_finite() || !(-100.0..=200.0).contains(&value) {
                return Err(format!("{label}超出合理范围"));
            }
        }
        if !self.noise_figure_db.is_finite() || !(0.0..=60.0).contains(&self.noise_figure_db) {
            return Err("噪声系数需在 [0, 60] dB 内".into());
        }
        if !self.bandwidth_hz.is_finite() || !(1.0..=1.0e12).contains(&self.bandwidth_hz) {
            return Err("带宽需在 1 Hz ~ 1 THz 内".into());
        }
        if !self.target_rcs_m2.is_finite() || !(1e-6..=1e9).contains(&self.target_rcs_m2) {
            return Err("目标 RCS 需在 [1e-6, 1e9] m² 内".into());
        }
        if !self.pulses_integrated.is_finite() || !(1.0..=1e6).contains(&self.pulses_integrated) {
            return Err("积累脉冲数需在 [1, 1e6] 内".into());
        }
        if let Some(minimum) = self.min_elevation_deg {
            if !minimum.is_finite() || !(0.0..=90.0).contains(&minimum) {
                return Err("雷达最小仰角需在 [0, 90] 度内".into());
            }
        }
        Ok(())
    }

    fn integration_gain_db(&self) -> f64 {
        10.0 * self.pulses_integrated.log10()
    }

    fn effective_required_snr_db(&self) -> f64 {
        self.required_snr_db - self.integration_gain_db()
    }

    /// 最大探测距离（km）。
    pub fn max_detection_range_km(&self) -> f64 {
        let wavelength = SPEED_OF_LIGHT / self.frequency_hz;
        let tx_gain = 10_f64.powf(self.tx_gain_db / 10.0);
        let rx_gain = 10_f64.powf(self.rx_gain_db / 10.0);
        let loss = 10_f64.powf(self.system_loss_db / 10.0);
        let noise_figure = 10_f64.powf(self.noise_figure_db / 10.0);
        let noise_power =
            BOLTZMANN * REFERENCE_TEMPERATURE_K * self.bandwidth_hz * noise_figure;
        let required_power =
            noise_power * 10_f64.powf(self.effective_required_snr_db() / 10.0);
        let numerator =
            self.tx_power_w * tx_gain * rx_gain * wavelength.powi(2) * self.target_rcs_m2;
        let denominator = (4.0 * std::f64::consts::PI).powi(3) * loss * required_power;
        if denominator <= 0.0 {
            return 0.0;
        }
        (numerator / denominator).powf(0.25) / 1000.0
    }

    /// 指定斜距下的 SNR（dB）。
    pub fn snr_at_range_db(&self, range_km: f64) -> f64 {
        let range_m = (range_km * 1000.0).max(1.0);
        let wavelength = SPEED_OF_LIGHT / self.frequency_hz;
        let tx_gain = 10_f64.powf(self.tx_gain_db / 10.0);
        let rx_gain = 10_f64.powf(self.rx_gain_db / 10.0);
        let loss = 10_f64.powf(self.system_loss_db / 10.0);
        let noise_figure = 10_f64.powf(self.noise_figure_db / 10.0);
        let noise_power =
            BOLTZMANN * REFERENCE_TEMPERATURE_K * self.bandwidth_hz * noise_figure;
        let received_power = self.tx_power_w
            * tx_gain
            * rx_gain
            * wavelength.powi(2)
            * self.target_rcs_m2
            / ((4.0 * std::f64::consts::PI).powi(3) * range_m.powi(4) * loss);
        let snr = received_power / noise_power;
        if snr <= 0.0 {
            f64::NEG_INFINITY
        } else {
            10.0 * snr.log10() + self.integration_gain_db()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarSample {
    pub t_seconds: f64,
    pub elevation_deg: f64,
    pub range_km: f64,
    pub snr_db: f64,
    pub detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarWindow {
    pub index: u32,
    pub start_utc: String,
    pub end_utc: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
    pub max_elevation_deg: f64,
    pub min_range_km: f64,
    pub max_range_km: f64,
    pub max_snr_db: f64,
    pub detected_seconds: f64,
    pub samples: Vec<RadarSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarReport {
    pub satellite_id: String,
    pub satellite_name: String,
    pub ground_station_id: String,
    pub ground_station_name: String,
    pub start: String,
    pub end: String,
    pub step_seconds: f64,
    pub frequency_hz: f64,
    pub bandwidth_hz: f64,
    pub tx_power_w: f64,
    pub target_rcs_m2: f64,
    pub required_snr_db: f64,
    pub integration_gain_db: f64,
    pub max_detection_range_km: f64,
    pub window_count: u32,
    pub detected_window_count: u32,
    pub total_detection_seconds: f64,
    pub min_range_km: f64,
    pub max_range_km: f64,
    pub samples: Vec<RadarSample>,
    pub windows: Vec<RadarWindow>,
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

fn slant_range(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// 计算卫星-地面站雷达探测分析。
pub fn compute_radar(
    data: &ScenarioData,
    satellite_id: &str,
    ground_station_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
    options: &RadarOptions,
) -> Result<RadarReport, String> {
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
        min_elevation_deg: Some(
            options
                .min_elevation_deg
                .unwrap_or(station.min_elevation_deg),
        ),
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

    let sample_step = (step_seconds / 2.0).clamp(1.0, 10.0);
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
    let mut all_samples = Vec::new();
    let mut detected_window_count = 0u32;
    let mut total_detection_seconds = 0.0_f64;
    let mut overall_min_range = f64::INFINITY;
    let mut overall_max_range = 0.0_f64;

    for window in &access.windows {
        let mut samples = Vec::new();
        for (t_seconds, position, _, _) in &states {
            if *t_seconds < window.start_seconds - 1e-6
                || *t_seconds > window.end_seconds + 1e-6
            {
                continue;
            }
            let (elevation, _) = look_angles(*position, station_position, up, east, north);
            let range_km = slant_range(*position, station_position);
            let snr_db = options.snr_at_range_db(range_km);
            samples.push(RadarSample {
                t_seconds: *t_seconds,
                elevation_deg: elevation,
                range_km,
                snr_db,
                detected: snr_db >= options.required_snr_db,
            });
        }
        if samples.is_empty() {
            continue;
        }

        let mut min_range = f64::INFINITY;
        let mut max_range = 0.0_f64;
        let mut max_snr = f64::NEG_INFINITY;
        let mut max_elevation = f64::NEG_INFINITY;
        let mut detected_samples = 0usize;

        for sample in &samples {
            min_range = min_range.min(sample.range_km);
            max_range = max_range.max(sample.range_km);
            max_snr = max_snr.max(sample.snr_db);
            max_elevation = max_elevation.max(sample.elevation_deg);
            if sample.detected {
                detected_samples += 1;
            }
        }
        overall_min_range = overall_min_range.min(min_range);
        overall_max_range = overall_max_range.max(max_range);

        let detected_seconds = if samples.len() > 1 {
            detected_samples as f64 * (window.duration_seconds / samples.len() as f64)
        } else {
            0.0
        };
        if detected_samples > 0 {
            detected_window_count += 1;
            total_detection_seconds += detected_seconds;
        }

        all_samples.extend(samples.iter().cloned());
        windows.push(RadarWindow {
            index: window.index,
            start_utc: window.start_utc.clone(),
            end_utc: window.end_utc.clone(),
            start_seconds: window.start_seconds,
            end_seconds: window.end_seconds,
            duration_seconds: window.duration_seconds,
            max_elevation_deg: max_elevation,
            min_range_km: min_range,
            max_range_km: max_range,
            max_snr_db: max_snr,
            detected_seconds,
            samples,
        });
    }

    Ok(RadarReport {
        satellite_id: satellite.id.clone(),
        satellite_name: satellite.name.clone(),
        ground_station_id: station.id.clone(),
        ground_station_name: station.name.clone(),
        start: format_utc(start),
        end: format_utc(end),
        step_seconds,
        frequency_hz: options.frequency_hz,
        bandwidth_hz: options.bandwidth_hz,
        tx_power_w: options.tx_power_w,
        target_rcs_m2: options.target_rcs_m2,
        required_snr_db: options.required_snr_db,
        integration_gain_db: options.integration_gain_db(),
        max_detection_range_km: options.max_detection_range_km(),
        window_count: windows.len() as u32,
        detected_window_count,
        total_detection_seconds,
        min_range_km: if overall_min_range.is_finite() {
            overall_min_range
        } else {
            0.0
        },
        max_range_km: overall_max_range,
        samples: all_samples,
        windows,
    })
}

/// 导出雷达分析 CSV。
pub fn radar_csv(report: &RadarReport) -> String {
    let mut output = String::from(
        "window,start_utc,end_utc,t_seconds,elevation_deg,range_km,snr_db,detected\n",
    );
    for window in &report.windows {
        for sample in &window.samples {
            output.push_str(&format!(
                "{},{},{},{:.1},{:.4},{:.3},{:.3},{}\n",
                window.index,
                window.start_utc,
                window.end_utc,
                sample.t_seconds,
                sample.elevation_deg,
                sample.range_km,
                sample.snr_db,
                if sample.detected { 1 } else { 0 }
            ));
        }
    }
    output
}
