use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::Deserialize;
use stkcraft_geocalc::{
    coords::{to_coordinates, CoordinateSystem},
    frame::{FrameRotation, Frames},
    pck::Pck,
    State,
};
use stkcraft_time::{
    convert::{et_to_utc, utc_to_et},
    format::iso8601,
    Calendar, LeapSecondTable, UtcDateTime,
};

use crate::error::AppError;

/// 地球半径、引力常数与 IAU_EARTH 定向参数。
/// 以内联 PCK 文本提供，避免运行时依赖外部内核文件。
const EARTH_PCK: &str = r#"\begindata
BODY399_RADII     = ( 6378.1366 6378.1366 6356.7519 )
BODY399_GM        = ( 398600.435507 )
BODY399_POLE_RA   = ( 0.0 -0.641 0.0 )
BODY399_POLE_DEC  = ( 90.0 -0.557 0.0 )
BODY399_PM        = ( 190.147 360.9856235 0.0 )
\begintext
"#;

const EARTH_RADII: [f64; 3] = [6378.1366, 6378.1366, 6356.7519];
const EARTH_GM: f64 = 398_600.435_507;

fn geo_err(err: stkcraft_geocalc::GeoError) -> AppError {
    AppError::Internal(format!("几何计算失败: {err}"))
}

fn time_err(err: stkcraft_time::TimeError) -> AppError {
    AppError::Internal(format!("时间转换失败: {err}"))
}

/// 开普勒轨道根数（角度单位为度，长度为公里）。
#[derive(Debug, Clone, Deserialize)]
pub struct OrbitElements {
    pub semi_major_axis_km: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_perigee_deg: f64,
    pub true_anomaly_deg: f64,
}

impl OrbitElements {
    pub fn validate(&self) -> Result<(), AppError> {
        let finite = [
            self.semi_major_axis_km,
            self.eccentricity,
            self.inclination_deg,
            self.raan_deg,
            self.arg_perigee_deg,
            self.true_anomaly_deg,
        ]
        .iter()
        .all(|value| value.is_finite());

        if !finite {
            return Err(AppError::BadRequest("轨道根数必须为有限数值".into()));
        }
        if self.semi_major_axis_km <= EARTH_RADII[0] {
            return Err(AppError::BadRequest(
                "半长轴必须大于地球赤道半径（6378.1366 km）".into(),
            ));
        }
        if !(0.0..1.0).contains(&self.eccentricity) {
            return Err(AppError::BadRequest(
                "偏心率需在 [0, 1) 区间内".into(),
            ));
        }
        if !(0.0..=180.0).contains(&self.inclination_deg) {
            return Err(AppError::BadRequest("轨道倾角需在 [0, 180] 度内".into()));
        }
        Ok(())
    }
}

/// 单时刻卫星状态：惯性系、地固系与大地坐标。
#[derive(Debug, Clone)]
pub struct SatelliteState {
    pub eci: State,
    pub ecef: State,
    pub longitude_deg: f64,
    pub latitude_deg: f64,
    pub altitude_km: f64,
}

/// 空间计算上下文：时间系统、天球参考系与地球物理常量。
#[derive(Clone)]
pub struct SpaceContext {
    frames: Frames,
    pck: Pck,
    lsk: LeapSecondTable,
    radii: [f64; 3],
    gm: f64,
}

impl SpaceContext {
    pub fn new() -> Result<Self, AppError> {
        let lsk = LeapSecondTable::builtin().map_err(time_err)?;
        let pck = Pck::from_text(EARTH_PCK).map_err(geo_err)?;

        Ok(Self {
            frames: Frames::new(),
            radii: pck.radii(399).unwrap_or(EARTH_RADII),
            gm: pck.gm(399).unwrap_or(EARTH_GM),
            pck,
            lsk,
        })
    }

    /// 解析 RFC3339 UTC 时刻并转换为 ET（TDB 秒，J2000 起算）。
    pub fn epoch_to_et(&self, epoch: &str) -> Result<f64, AppError> {
        let parsed = DateTime::parse_from_rfc3339(epoch.trim()).map_err(|_| {
            AppError::BadRequest(
                "epoch 需为 RFC3339 格式，例如 2026-11-25T12:00:00Z".into(),
            )
        })?;
        let utc = parsed.with_timezone(&Utc);

        let datetime = UtcDateTime::new(
            utc.year(),
            utc.month() as u8,
            utc.day() as u8,
            utc.hour() as u8,
            utc.minute() as u8,
            f64::from(utc.second()) + f64::from(utc.nanosecond()) / 1e9,
        );

        utc_to_et(&datetime, Calendar::Gregorian, &self.lsk).map_err(time_err)
    }

    /// ET 转 ISO8601 UTC 字符串。
    pub fn et_to_iso(&self, et: f64) -> String {
        et_to_utc(et, Calendar::Gregorian, &self.lsk)
            .map(|datetime| iso8601(&datetime, 3))
            .unwrap_or_else(|_| format!("ET {et:.3} s"))
    }

    /// 计算给定时刻的单点状态。
    pub fn state_at(&self, elements: &OrbitElements, et: f64) -> Result<SatelliteState, AppError> {
        let state = self.j2000_state(elements, elements.true_anomaly_deg.to_radians());
        self.describe_state(state, et)
    }

    /// 按时间均匀采样一个轨道周期，返回 [经度(度), 纬度(度), 高度(km)] 列表。
    pub fn orbit_track(
        &self,
        elements: &OrbitElements,
        et: f64,
        samples: u32,
    ) -> Result<Vec<[f64; 3]>, AppError> {
        let count = samples.clamp(2, 720);
        let period = self.period_seconds(elements.semi_major_axis_km);
        let mean_anomaly_at_epoch = true_to_mean(
            elements.true_anomaly_deg.to_radians(),
            elements.eccentricity,
        );

        let mut track = Vec::with_capacity(count as usize);
        for index in 0..count {
            let elapsed = period * f64::from(index) / f64::from(count);
            let mean_anomaly =
                mean_anomaly_at_epoch + std::f64::consts::TAU * f64::from(index) / f64::from(count);
            let true_anomaly = mean_to_true(mean_anomaly, elements.eccentricity);
            let state = self.j2000_state(elements, true_anomaly);
            let described = self.describe_state(state, et + elapsed)?;
            track.push([
                described.longitude_deg,
                described.latitude_deg,
                described.altitude_km,
            ]);
        }
        Ok(track)
    }

    /// 轨道周期（秒）。
    pub fn period_seconds(&self, semi_major_axis_km: f64) -> f64 {
        std::f64::consts::TAU * (semi_major_axis_km.powi(3) / self.gm).sqrt()
    }

    fn describe_state(&self, eci: State, et: f64) -> Result<SatelliteState, AppError> {
        let ecef = self.to_ecef(&eci, et)?;
        let [longitude, latitude, altitude] = to_coordinates(
            ecef.position,
            CoordinateSystem::Geodetic,
            Some(self.radii),
            399,
        )
        .map_err(geo_err)?;

        Ok(SatelliteState {
            eci,
            ecef,
            longitude_deg: longitude.to_degrees(),
            latitude_deg: latitude.to_degrees(),
            altitude_km: altitude,
        })
    }

    /// 开普勒根数 → J2000 惯性系状态（二体解析解）。
    fn j2000_state(&self, elements: &OrbitElements, true_anomaly: f64) -> State {
        let eccentricity = elements.eccentricity;
        let semi_latus_rectum = elements.semi_major_axis_km * (1.0 - eccentricity * eccentricity);
        let radius = semi_latus_rectum / (1.0 + eccentricity * true_anomaly.cos());
        let scale = (self.gm / semi_latus_rectum).sqrt();
        let (sin_nu, cos_nu) = true_anomaly.sin_cos();

        let position_pqw = [radius * cos_nu, radius * sin_nu, 0.0];
        let velocity_pqw = [
            -scale * sin_nu,
            scale * (eccentricity + cos_nu),
            0.0,
        ];

        let inclination = elements.inclination_deg.to_radians();
        let raan = elements.raan_deg.to_radians();
        let arg_perigee = elements.arg_perigee_deg.to_radians();

        State {
            position: pqw_to_j2000(position_pqw, inclination, raan, arg_perigee),
            velocity: pqw_to_j2000(velocity_pqw, inclination, raan, arg_perigee),
        }
    }

    /// J2000 → IAU_EARTH 地固系（位移与速度均做旋转）。
    fn to_ecef(&self, state: &State, et: f64) -> Result<State, AppError> {
        let FrameRotation {
            rotation,
            rotation_dot,
        } = self
            .frames
            .pxform_with_derivative("J2000", "IAU_EARTH", et, Some(&self.pck))
            .map_err(geo_err)?;

        let rotated_velocity = mat_vec(&rotation, &state.velocity);
        let transport = mat_vec(&rotation_dot, &state.position);
        Ok(State {
            position: mat_vec(&rotation, &state.position),
            velocity: [
                rotated_velocity[0] + transport[0],
                rotated_velocity[1] + transport[1],
                rotated_velocity[2] + transport[2],
            ],
        })
    }
}

/// R3(Ω)·R1(i)·R3(ω) 作用在近焦点系向量上。
fn pqw_to_j2000(vector: [f64; 3], inclination: f64, raan: f64, arg_perigee: f64) -> [f64; 3] {
    let (sin_w, cos_w) = arg_perigee.sin_cos();
    let (sin_i, cos_i) = inclination.sin_cos();
    let (sin_o, cos_o) = raan.sin_cos();

    let x = cos_w * vector[0] - sin_w * vector[1];
    let y = sin_w * vector[0] + cos_w * vector[1];
    let y_rot = cos_i * y - sin_i * vector[2];
    let z_rot = sin_i * y + cos_i * vector[2];

    [
        cos_o * x - sin_o * y_rot,
        sin_o * x + cos_o * y_rot,
        z_rot,
    ]
}

fn mat_vec(matrix: &[[f64; 3]; 3], vector: &[f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

/// 真近点角 → 平近点角。
fn true_to_mean(true_anomaly: f64, eccentricity: f64) -> f64 {
    let half = true_anomaly / 2.0;
    let eccentric_anomaly = 2.0
        * f64::atan2(
            (1.0 - eccentricity).sqrt() * half.sin(),
            (1.0 + eccentricity).sqrt() * half.cos(),
        );
    eccentric_anomaly - eccentricity * eccentric_anomaly.sin()
}

/// 平近点角 → 真近点角（牛顿迭代解开普勒方程）。
fn mean_to_true(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let mut eccentric_anomaly = if eccentricity < 0.8 {
        mean_anomaly
    } else {
        std::f64::consts::PI
    };

    for _ in 0..50 {
        let residual =
            eccentric_anomaly - eccentricity * eccentric_anomaly.sin() - mean_anomaly;
        let slope = 1.0 - eccentricity * eccentric_anomaly.cos();
        let step = residual / slope;
        eccentric_anomaly -= step;
        if step.abs() < 1e-12 {
            break;
        }
    }

    let sin_e = eccentric_anomaly.sin();
    let cos_e = eccentric_anomaly.cos();
    let sin_nu = (1.0 - eccentricity * eccentricity).sqrt() * sin_e / (1.0 - eccentricity * cos_e);
    let cos_nu = (cos_e - eccentricity) / (1.0 - eccentricity * cos_e);
    sin_nu.atan2(cos_nu)
}
