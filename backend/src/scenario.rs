use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// 场景内容：卫星、地面站与传感器对象集合，整体以 JSONB 存储。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScenarioData {
    #[serde(default)]
    pub satellites: Vec<SatelliteObject>,
    #[serde(default)]
    pub ground_stations: Vec<GroundStationObject>,
    #[serde(default)]
    pub sensors: Vec<SensorObject>,
}

/// 卫星对象：使用 TLE 描述，可选择 SGP4 或数值积分传播。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteObject {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub color: String,
    pub tle_line1: String,
    pub tle_line2: String,
    #[serde(default)]
    pub norad_id: Option<u64>,
    /// 传播器类型：`sgp4`（默认）或 `numerical`（Dormand-Prince 自适应积分）。
    #[serde(default = "default_propagator_type")]
    pub propagator_type: String,
    /// 姿态模式：`nadir`（对地）、`sun`（对日）或 `fixed`（轨道系固定偏置）。
    #[serde(default = "default_attitude_mode")]
    pub attitude_mode: String,
    /// `fixed` 姿态模式下相对 LVLH 的滚转角（度）。
    #[serde(default)]
    pub attitude_roll_deg: f64,
    /// `fixed` 姿态模式下相对 LVLH 的俯仰角（度）。
    #[serde(default)]
    pub attitude_pitch_deg: f64,
    /// `fixed` 姿态模式下相对 LVLH 的偏航角（度）。
    #[serde(default)]
    pub attitude_yaw_deg: f64,
    /// 质量（kg），用于大气阻力。
    #[serde(default = "default_mass")]
    pub mass_kg: f64,
    /// 迎风面积（m²），用于大气阻力。
    #[serde(default = "default_area")]
    pub area_m2: f64,
    /// 阻力系数。
    #[serde(default = "default_drag_coefficient")]
    pub drag_coefficient: f64,
    /// 数值积分相对容差（1e-12 ~ 1e-3）。
    #[serde(default = "default_integrator_tolerance")]
    pub integrator_tolerance: f64,
    /// 是否启用 J2 摄动（仅数值积分）。
    #[serde(default = "default_true")]
    pub enable_j2: bool,
    /// 是否启用大气阻力（仅数值积分）。
    #[serde(default = "default_true")]
    pub enable_drag: bool,
}

/// 地面站对象：大地坐标与最小仰角约束。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundStationObject {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub color: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    #[serde(default)]
    pub altitude_km: f64,
    #[serde(default = "default_min_elevation")]
    pub min_elevation_deg: f64,
}

fn default_true() -> bool {
    true
}

fn default_min_elevation() -> f64 {
    5.0
}

fn default_propagator_type() -> String {
    "sgp4".to_string()
}

fn default_attitude_mode() -> String {
    "nadir".to_string()
}

fn default_mass() -> f64 {
    500.0
}

fn default_area() -> f64 {
    2.0
}

fn default_drag_coefficient() -> f64 {
    2.2
}

fn default_integrator_tolerance() -> f64 {
    1e-9
}

fn default_body_boresight() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

fn default_sensor_shape() -> String {
    "conical".to_string()
}

fn default_pointing_mode() -> String {
    "nadir".to_string()
}

fn default_half_angle() -> f64 {
    30.0
}

fn default_rect_fov() -> f64 {
    20.0
}

fn default_sensor_range() -> f64 {
    3000.0
}

/// 卫星传感器：圆锥或矩形视场，可绑定到卫星并参与 Access 约束。
///
/// 指向模式：
/// - `nadir`：始终指向星下点（有姿态时使用本体 +Z 轴）；
/// - `fixed`：以星下点为基准，按方位角与离轴角固定指向；
/// - `target`：指向场景内指定地面站；
/// - `body`：按卫星真实姿态旋转本体视轴 `body_boresight`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorObject {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub color: String,
    pub satellite_id: String,
    #[serde(default = "default_sensor_shape")]
    pub shape: String,
    #[serde(default = "default_half_angle")]
    pub half_angle_deg: f64,
    #[serde(default = "default_rect_fov")]
    pub horizontal_fov_deg: f64,
    #[serde(default = "default_rect_fov")]
    pub vertical_fov_deg: f64,
    #[serde(default = "default_pointing_mode")]
    pub pointing_mode: String,
    /// `body` 指向模式下传感器在本体坐标系中的视轴方向。
    #[serde(default = "default_body_boresight")]
    pub body_boresight: [f64; 3],
    #[serde(default)]
    pub fixed_azimuth_deg: f64,
    #[serde(default)]
    pub fixed_off_nadir_deg: f64,
    #[serde(default)]
    pub target_station_id: Option<String>,
    #[serde(default = "default_sensor_range")]
    pub max_range_km: f64,
}

impl ScenarioData {
    /// 校验对象数量、标识唯一性、TLE 可解析性与地面站坐标范围。
    pub fn validate(&self) -> Result<(), AppError> {
        if self.satellites.len() > 100 {
            return Err(AppError::BadRequest("卫星数量不能超过 100 颗".into()));
        }
        if self.ground_stations.len() > 100 {
            return Err(AppError::BadRequest("地面站数量不能超过 100 个".into()));
        }

        let mut satellite_ids = HashSet::new();
        for satellite in &self.satellites {
            if satellite.id.trim().is_empty() {
                return Err(AppError::BadRequest("卫星缺少 id".into()));
            }
            if !satellite_ids.insert(satellite.id.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "卫星 id 重复：{}",
                    satellite.id
                )));
            }
            if satellite.name.trim().is_empty() {
                return Err(AppError::BadRequest("卫星名称不能为空".into()));
            }

            let line1 = satellite.tle_line1.trim();
            let line2 = satellite.tle_line2.trim();
            if line1.len() < 69 || line2.len() < 69 {
                return Err(AppError::BadRequest(format!(
                    "卫星 {} 的 TLE 行长度不足 69 字符",
                    satellite.name
                )));
            }
            sgp4::Elements::from_tle(None, line1.as_bytes(), line2.as_bytes()).map_err(|err| {
                AppError::BadRequest(format!("卫星 {} 的 TLE 解析失败：{err}", satellite.name))
            })?;

            match satellite.propagator_type.as_str() {
                "sgp4" | "numerical" => {}
                other => {
                    return Err(AppError::BadRequest(format!(
                        "卫星 {} 的传播器类型无效：{other}",
                        satellite.name
                    )));
                }
            }
            match satellite.attitude_mode.as_str() {
                "nadir" | "sun" | "fixed" => {}
                other => {
                    return Err(AppError::BadRequest(format!(
                        "卫星 {} 的姿态模式无效：{other}",
                        satellite.name
                    )));
                }
            }
            for (value, label) in [
                (satellite.attitude_roll_deg, "滚转角"),
                (satellite.attitude_pitch_deg, "俯仰角"),
                (satellite.attitude_yaw_deg, "偏航角"),
            ] {
                if !value.is_finite() || !(-180.0..=180.0).contains(&value) {
                    return Err(AppError::BadRequest(format!(
                        "卫星 {} 的{label}需在 [-180, 180] 度内",
                        satellite.name
                    )));
                }
            }
            if !satellite.mass_kg.is_finite() || !(0.001..=1_000_000.0).contains(&satellite.mass_kg) {
                return Err(AppError::BadRequest(format!(
                    "卫星 {} 的质量需为正数且不超过 1e6 kg",
                    satellite.name
                )));
            }
            if !satellite.area_m2.is_finite() || !(0.0001..=100_000.0).contains(&satellite.area_m2) {
                return Err(AppError::BadRequest(format!(
                    "卫星 {} 的迎风面积需为正数且不超过 1e5 m²",
                    satellite.name
                )));
            }
            if !satellite.drag_coefficient.is_finite()
                || !(0.01..=10.0).contains(&satellite.drag_coefficient)
            {
                return Err(AppError::BadRequest(format!(
                    "卫星 {} 的阻力系数需在 [0.01, 10] 内",
                    satellite.name
                )));
            }
            if !satellite.integrator_tolerance.is_finite()
                || !(1e-12..=1e-3).contains(&satellite.integrator_tolerance)
            {
                return Err(AppError::BadRequest(format!(
                    "卫星 {} 的积分容差需在 [1e-12, 1e-3] 内",
                    satellite.name
                )));
            }
        }

        let mut station_ids = HashSet::new();
        for station in &self.ground_stations {
            if station.id.trim().is_empty() {
                return Err(AppError::BadRequest("地面站缺少 id".into()));
            }
            if !station_ids.insert(station.id.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "地面站 id 重复：{}",
                    station.id
                )));
            }
            if station.name.trim().is_empty() {
                return Err(AppError::BadRequest("地面站名称不能为空".into()));
            }
            if !station.latitude_deg.is_finite() || !(-90.0..=90.0).contains(&station.latitude_deg)
            {
                return Err(AppError::BadRequest(format!(
                    "地面站 {} 的纬度需在 [-90, 90] 度内",
                    station.name
                )));
            }
            if !station.longitude_deg.is_finite()
                || !(-180.0..=180.0).contains(&station.longitude_deg)
            {
                return Err(AppError::BadRequest(format!(
                    "地面站 {} 的经度需在 [-180, 180] 度内",
                    station.name
                )));
            }
            if !station.altitude_km.is_finite() || !(-1.0..=10_000.0).contains(&station.altitude_km)
            {
                return Err(AppError::BadRequest(format!(
                    "地面站 {} 的海拔超出合理范围",
                    station.name
                )));
            }
            if !station.min_elevation_deg.is_finite()
                || !(0.0..=90.0).contains(&station.min_elevation_deg)
            {
                return Err(AppError::BadRequest(format!(
                    "地面站 {} 的最小仰角需在 [0, 90] 度内",
                    station.name
                )));
            }
        }

        if self.sensors.len() > 200 {
            return Err(AppError::BadRequest("传感器数量不能超过 200 个".into()));
        }

        let mut sensor_ids = HashSet::new();
        for sensor in &self.sensors {
            if sensor.id.trim().is_empty() {
                return Err(AppError::BadRequest("传感器缺少 id".into()));
            }
            if !sensor_ids.insert(sensor.id.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "传感器 id 重复：{}",
                    sensor.id
                )));
            }
            if sensor.name.trim().is_empty() {
                return Err(AppError::BadRequest("传感器名称不能为空".into()));
            }
            if !satellite_ids.contains(sensor.satellite_id.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "传感器 {} 绑定的卫星不存在：{}",
                    sensor.name, sensor.satellite_id
                )));
            }

            match sensor.shape.as_str() {
                "conical" => {
                    if !sensor.half_angle_deg.is_finite()
                        || !(0.1..=90.0).contains(&sensor.half_angle_deg)
                    {
                        return Err(AppError::BadRequest(format!(
                            "传感器 {} 的圆锥半角需在 (0, 90] 度内",
                            sensor.name
                        )));
                    }
                }
                "rectangular" => {
                    for (value, label) in [
                        (sensor.horizontal_fov_deg, "水平视场"),
                        (sensor.vertical_fov_deg, "垂直视场"),
                    ] {
                        if !value.is_finite() || !(0.1..=180.0).contains(&value) {
                            return Err(AppError::BadRequest(format!(
                                "传感器 {} 的{label}需在 (0, 180] 度内",
                                sensor.name
                            )));
                        }
                    }
                }
                other => {
                    return Err(AppError::BadRequest(format!(
                        "传感器 {} 的视场类型无效：{other}",
                        sensor.name
                    )));
                }
            }

            match sensor.pointing_mode.as_str() {
                "nadir" => {}
                "body" => {
                    let norm = (sensor.body_boresight[0] * sensor.body_boresight[0]
                        + sensor.body_boresight[1] * sensor.body_boresight[1]
                        + sensor.body_boresight[2] * sensor.body_boresight[2])
                        .sqrt();
                    if !norm.is_finite() || norm < 1e-6 {
                        return Err(AppError::BadRequest(format!(
                            "传感器 {} 的本体视轴不能为零向量",
                            sensor.name
                        )));
                    }
                }
                "fixed" => {
                    if !sensor.fixed_azimuth_deg.is_finite()
                        || !(0.0..=360.0).contains(&sensor.fixed_azimuth_deg)
                    {
                        return Err(AppError::BadRequest(format!(
                            "传感器 {} 的固定方位角需在 [0, 360] 度内",
                            sensor.name
                        )));
                    }
                    if !sensor.fixed_off_nadir_deg.is_finite()
                        || !(0.0..=180.0).contains(&sensor.fixed_off_nadir_deg)
                    {
                        return Err(AppError::BadRequest(format!(
                            "传感器 {} 的离轴角需在 [0, 180] 度内",
                            sensor.name
                        )));
                    }
                }
                "target" => {
                    let target = sensor
                        .target_station_id
                        .as_deref()
                        .ok_or_else(|| {
                            AppError::BadRequest(format!(
                                "传感器 {} 的指向模式为对目标，但未选择地面站",
                                sensor.name
                            ))
                        })?;
                    if !station_ids.contains(target) {
                        return Err(AppError::BadRequest(format!(
                            "传感器 {} 的目标地面站不存在：{}",
                            sensor.name, target
                        )));
                    }
                }
                other => {
                    return Err(AppError::BadRequest(format!(
                        "传感器 {} 的指向模式无效：{other}",
                        sensor.name
                    )));
                }
            }

            if !sensor.max_range_km.is_finite() || !(1.0..=100_000.0).contains(&sensor.max_range_km) {
                return Err(AppError::BadRequest(format!(
                    "传感器 {} 的最大作用距离需在 [1, 100000] km 内",
                    sensor.name
                )));
            }
        }

        Ok(())
    }

    pub fn satellite(&self, id: &str) -> Option<&SatelliteObject> {
        self.satellites.iter().find(|item| item.id == id)
    }

    pub fn ground_station(&self, id: &str) -> Option<&GroundStationObject> {
        self.ground_stations.iter().find(|item| item.id == id)
    }

    pub fn sensor(&self, id: &str) -> Option<&SensorObject> {
        self.sensors.iter().find(|item| item.id == id)
    }

    /// 绑定到指定卫星且启用的传感器。
    pub fn sensors_of<'a>(
        &'a self,
        satellite_id: &'a str,
    ) -> impl Iterator<Item = &'a SensorObject> + 'a {
        self.sensors
            .iter()
            .filter(move |sensor| sensor.enabled && sensor.satellite_id == satellite_id)
    }
}
