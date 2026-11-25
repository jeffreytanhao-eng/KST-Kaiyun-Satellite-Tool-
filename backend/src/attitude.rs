//! 卫星姿态模型。
//!
//! 姿态计算全部在 Rust 后端完成，输出本体坐标系三轴在地固系中的单位向量与四元数，
//! 供传感器指向、Access 约束与三维可视化复用。
//!
//! 本体坐标系约定（右手系）：
//! - `+Z`：对地模式为星下点方向，对日模式为太阳方向；
//! - `+X`：沿迹方向（速度在垂直于 `+Z` 平面内的投影）；
//! - `+Y`：`Z × X`。
//!
//! 参考实现思路来自 `ssmichael1/satkit` 与 `duncaneddy/brahe` 的 LVLH/姿态坐标系定义，
//! 本项目按其轴序约定在 Rust 中独立实现，未直接链接这两个库（原因见
//! `doc/KST_Phase3_开源评估.md`）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::orbit::gmst_radians;
use crate::scenario::SatelliteObject;
use crate::sensor::{cross, dot, normalize};

/// 本体坐标系三轴（地固系单位向量）与姿态四元数 `[x, y, z, w]`。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AttitudeFrame {
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub z: [f64; 3],
    pub quaternion: [f64; 4],
}

/// 绕单位轴旋转向量（Rodrigues 公式）。
fn rotate_about(vector: [f64; 3], axis: [f64; 3], angle_rad: f64) -> [f64; 3] {
    let (sin, cos) = angle_rad.sin_cos();
    let axis_dot = dot(axis, vector);
    let axis_cross = cross(axis, vector);
    [
        vector[0] * cos + axis_cross[0] * sin + axis[0] * axis_dot * (1.0 - cos),
        vector[1] * cos + axis_cross[1] * sin + axis[1] * axis_dot * (1.0 - cos),
        vector[2] * cos + axis_cross[2] * sin + axis[2] * axis_dot * (1.0 - cos),
    ]
}

/// 太阳方向单位向量（地固系）。
///
/// 采用低精度太阳历表（精度约 0.01°），满足姿态指向与三维显示需求；
/// 高精度星历可在后续阶段接入 SPICE（见开源评估文档）。
pub fn sun_direction_ecef(time: DateTime<Utc>) -> [f64; 3] {
    let j2000 = DateTime::from_timestamp(946_728_000, 0).unwrap_or(time);
    let days = (time - j2000).num_milliseconds() as f64 / 86_400_000.0;

    let mean_longitude = (280.460 + 0.985_647_4 * days).to_radians();
    let mean_anomaly = (357.528 + 0.985_600_3 * days).to_radians();
    let ecliptic_longitude = mean_longitude
        + 1.915_f64.to_radians() * mean_anomaly.sin()
        + 0.020_f64.to_radians() * (2.0 * mean_anomaly).sin();
    let obliquity = (23.439 - 0.000_000_4 * days).to_radians();

    // 黄道 → 赤道（惯性系）。
    let (sin_lon, cos_lon) = ecliptic_longitude.sin_cos();
    let (sin_obl, cos_obl) = obliquity.sin_cos();
    let inertial = [cos_lon, sin_lon * cos_obl, sin_lon * sin_obl];

    // 惯性系 → 地固系（绕 Z 轴转 -GMST）。
    let gmst = gmst_radians(time);
    let (sin_g, cos_g) = gmst.sin_cos();
    normalize([
        cos_g * inertial[0] + sin_g * inertial[1],
        -sin_g * inertial[0] + cos_g * inertial[1],
        inertial[2],
    ])
    .unwrap_or([1.0, 0.0, 0.0])
}

/// 由本体三轴构造四元数（本体 → 地固系）。
fn quaternion_from_axes(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> [f64; 4] {
    let m00 = x[0];
    let m01 = y[0];
    let m02 = z[0];
    let m10 = x[1];
    let m11 = y[1];
    let m12 = z[1];
    let m20 = x[2];
    let m21 = y[2];
    let m22 = z[2];

    let trace = m00 + m11 + m22;
    if trace > 0.0 {
        let scale = (trace + 1.0).sqrt() * 2.0;
        [
            (m21 - m12) / scale,
            (m02 - m20) / scale,
            (m10 - m01) / scale,
            0.25 * scale,
        ]
    } else if m00 > m11 && m00 > m22 {
        let scale = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [
            0.25 * scale,
            (m01 + m10) / scale,
            (m02 + m20) / scale,
            (m21 - m12) / scale,
        ]
    } else if m11 > m22 {
        let scale = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [
            (m01 + m10) / scale,
            0.25 * scale,
            (m12 + m21) / scale,
            (m02 - m20) / scale,
        ]
    } else {
        let scale = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [
            (m02 + m20) / scale,
            (m12 + m21) / scale,
            0.25 * scale,
            (m10 - m01) / scale,
        ]
    }
}

fn build(x: [f64; 3], y: [f64; 3], z: [f64; 3]) -> Option<AttitudeFrame> {
    let x = normalize(x)?;
    let y = normalize(y)?;
    let z = normalize(z)?;
    Some(AttitudeFrame {
        x,
        y,
        z,
        quaternion: quaternion_from_axes(x, y, z),
    })
}

/// 计算卫星在给定状态下的姿态。
///
/// `velocity` 为惯性速度在地固系轴上的分量（与 `orbit::propagate_state` 输出一致）。
pub fn compute(
    satellite: &SatelliteObject,
    position_ecef: [f64; 3],
    velocity_ecef: [f64; 3],
    time: DateTime<Utc>,
) -> Option<AttitudeFrame> {
    let nadir = normalize([-position_ecef[0], -position_ecef[1], -position_ecef[2]])?;

    match satellite.attitude_mode.as_str() {
        "sun" => {
            let sun = sun_direction_ecef(time);
            let along = normalize([
                velocity_ecef[0] - sun[0] * dot(velocity_ecef, sun),
                velocity_ecef[1] - sun[1] * dot(velocity_ecef, sun),
                velocity_ecef[2] - sun[2] * dot(velocity_ecef, sun),
            ])?;
            let y = normalize(cross(sun, along))?;
            build(along, y, sun)
        }
        "fixed" => {
            // LVLH：Z=星下点，X=沿迹，Y=Z×X。
            let along = normalize([
                velocity_ecef[0] - nadir[0] * dot(velocity_ecef, nadir),
                velocity_ecef[1] - nadir[1] * dot(velocity_ecef, nadir),
                velocity_ecef[2] - nadir[2] * dot(velocity_ecef, nadir),
            ])
            .unwrap_or_else(|| normalize(cross(nadir, [0.0, 0.0, 1.0])).unwrap_or([1.0, 0.0, 0.0]));
            let cross_track = normalize(cross(nadir, along))?;

            // 依次绕本体系 X(roll)、Y(pitch)、Z(yaw) 旋转。
            let roll = satellite.attitude_roll_deg.to_radians();
            let pitch = satellite.attitude_pitch_deg.to_radians();
            let yaw = satellite.attitude_yaw_deg.to_radians();

            let x1 = along;
            let y1 = rotate_about(cross_track, along, roll);
            let z1 = rotate_about(nadir, along, roll);

            let x2 = rotate_about(x1, y1, pitch);
            let z2 = rotate_about(z1, y1, pitch);

            let x3 = rotate_about(x2, z2, yaw);
            let y3 = rotate_about(y1, z2, yaw);

            build(x3, y3, z2)
        }
        _ => {
            // Nadir：Z 对地，X 沿迹。
            let along = normalize([
                velocity_ecef[0] - nadir[0] * dot(velocity_ecef, nadir),
                velocity_ecef[1] - nadir[1] * dot(velocity_ecef, nadir),
                velocity_ecef[2] - nadir[2] * dot(velocity_ecef, nadir),
            ])
            .unwrap_or_else(|| normalize(cross(nadir, [0.0, 0.0, 1.0])).unwrap_or([1.0, 0.0, 0.0]));
            let y = normalize(cross(nadir, along))?;
            build(along, y, nadir)
        }
    }
}
