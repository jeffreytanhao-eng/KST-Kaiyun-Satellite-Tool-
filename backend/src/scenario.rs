use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// 场景内容：卫星与地面站对象集合，整体以 JSONB 存储。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScenarioData {
    #[serde(default)]
    pub satellites: Vec<SatelliteObject>,
    #[serde(default)]
    pub ground_stations: Vec<GroundStationObject>,
}

/// 卫星对象：使用 TLE 描述，由后端通过 SGP4 传播。
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

        Ok(())
    }

    pub fn satellite(&self, id: &str) -> Option<&SatelliteObject> {
        self.satellites.iter().find(|item| item.id == id)
    }

    pub fn ground_station(&self, id: &str) -> Option<&GroundStationObject> {
        self.ground_stations.iter().find(|item| item.id == id)
    }
}
