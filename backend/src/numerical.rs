//! Dormand-Prince (DOPRI5) 自适应数值积分传播器。
//!
//! 力模型包含：
//! - 中心引力；
//! - J2 非球形摄动（`J2 = 1.08262668e-3`，`Re = 6378.1366 km`）；
//! - 大气阻力（复用项目内嵌的 NRLMSISE-00 密度，相对速度扣除地球自转牵连速度）。
//!
//! 积分在惯性系完成，输出与 SGP4 一致的地固系状态序列。步长由 5 阶/4 阶嵌入对
//! 的误差估计自适应控制，并设置步数、步长上下限保护，避免长时间任务阻塞。
//!
//! 积分器与力模型结构参考 `ssmichael1/satkit` 与 `duncaneddy/brahe` 的公开实现，
//! 本项目未直接依赖这两个库，评估结论见 `doc/KST_Phase3_开源评估.md`。

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::orbit::{ecef_to_geodetic, gmst_radians, prepare, propagate_state};
use crate::scenario::SatelliteObject;
use crate::sensor::dot;
use crate::spaceenv::{nrlmsise00, Nrlmsise00Activity, Nrlmsise00Ap, Nrlmsise00Input};

/// 地球引力常数（km³/s²）。
const EARTH_MU: f64 = 398_600.441_8;
/// J2 项。
const EARTH_J2: f64 = 1.082_626_68e-3;
/// 地球赤道半径（km）。
const EARTH_RADIUS_KM: f64 = 6_378.136_6;
/// 地球自转角速度（rad/s）。
const EARTH_ROTATION_RAD_S: f64 = 7.292_115_9e-5;

/// 积分配置上限。
const MAX_STEPS: usize = 500_000;
const MIN_STEP_SECONDS: f64 = 0.05;
const MAX_STEP_SECONDS: f64 = 300.0;
const INITIAL_STEP_SECONDS: f64 = 60.0;
const ABS_TOLERANCE_POSITION_KM: f64 = 1e-6;
const ABS_TOLERANCE_VELOCITY_KM_S: f64 = 1e-9;

/// 数值积分输出状态（地固系，速度仅做轴向旋转，与 SGP4 输出一致）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NumericalState {
    pub t_seconds: f64,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
}

/// 积分过程统计，用于任务结果与前端展示。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IntegrationStats {
    pub accepted_steps: u64,
    pub rejected_steps: u64,
    pub min_step_seconds: f64,
    pub max_step_seconds: f64,
    pub mean_step_seconds: f64,
}

/// 数值积分结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericalTrack {
    pub states: Vec<NumericalState>,
    pub stats: IntegrationStats,
    pub model: String,
    pub j2_enabled: bool,
    pub drag_enabled: bool,
}

/// 地固系 → 惯性系（绕 Z 轴转 GMST）。
fn ecef_to_inertial(vector: [f64; 3], gmst: f64) -> [f64; 3] {
    let (sin, cos) = gmst.sin_cos();
    [
        cos * vector[0] - sin * vector[1],
        sin * vector[0] + cos * vector[1],
        vector[2],
    ]
}

/// 惯性系 → 地固系。
fn inertial_to_ecef(vector: [f64; 3], gmst: f64) -> [f64; 3] {
    let (sin, cos) = gmst.sin_cos();
    [
        cos * vector[0] + sin * vector[1],
        -sin * vector[0] + cos * vector[1],
        vector[2],
    ]
}

/// 计算给定时刻、惯性系状态下的加速度（km/s²）。
#[allow(clippy::too_many_arguments)]
fn acceleration(
    position: [f64; 3],
    velocity: [f64; 3],
    time: DateTime<Utc>,
    options: &SatelliteObject,
    j2_enabled: bool,
    drag_enabled: bool,
) -> Result<[f64; 3], String> {
    let radius_sq = dot(position, position);
    let radius = radius_sq.sqrt();
    if !radius.is_finite() || radius < 1.0 {
        return Err("数值积分出现无效轨道半径".into());
    }

    // 中心引力。
    let factor = -EARTH_MU / (radius_sq * radius);
    let mut acceleration = [
        factor * position[0],
        factor * position[1],
        factor * position[2],
    ];

    // J2 摄动。
    if j2_enabled {
        let z_sq_over_r_sq = position[2] * position[2] / radius_sq;
        let scale = -1.5 * EARTH_J2 * EARTH_MU * EARTH_RADIUS_KM * EARTH_RADIUS_KM
            / (radius_sq * radius_sq * radius);
        acceleration[0] += scale * position[0] * (1.0 - 5.0 * z_sq_over_r_sq);
        acceleration[1] += scale * position[1] * (1.0 - 5.0 * z_sq_over_r_sq);
        acceleration[2] += scale * position[2] * (3.0 - 5.0 * z_sq_over_r_sq);
    }

    // 大气阻力。
    if drag_enabled {
        let gmst = gmst_radians(time);
        let position_ecef = inertial_to_ecef(position, gmst);
        let [longitude_deg, latitude_deg, altitude_km] = ecef_to_geodetic(position_ecef)?;

        if (0.0..=1_000.0).contains(&altitude_km) {
            let day_of_year = time.ordinal() as u16;
            let utc_seconds = f64::from(time.num_seconds_from_midnight());
            let solar_hours = (utc_seconds / 3600.0 + longitude_deg / 15.0).rem_euclid(24.0);
            let input = Nrlmsise00Input {
                day_of_year,
                utc_seconds,
                altitude_m: altitude_km * 1000.0,
                latitude_rad: latitude_deg.to_radians(),
                longitude_rad: longitude_deg.to_radians(),
                local_apparent_solar_time_hours: solar_hours,
                activity: Nrlmsise00Activity {
                    daily_f107_sfu: 150.0,
                    mean81_f107_sfu: 150.0,
                    ap: Nrlmsise00Ap::Daily(4.0),
                },
            };
            let density = nrlmsise00(&input)
                .map_err(|err| format!("大气密度计算失败：{err}"))?
                .effective_density_kg_m3;

            // 相对速度：扣除地球自转牵连速度（惯性系）。
            let relative_velocity = [
                velocity[0] + EARTH_ROTATION_RAD_S * position[1],
                velocity[1] - EARTH_ROTATION_RAD_S * position[0],
                velocity[2],
            ];
            let speed = dot(relative_velocity, relative_velocity).sqrt();
            if density > 0.0 && speed > 0.0 {
                let coefficient = -0.5 * density * options.drag_coefficient * options.area_m2
                    / options.mass_kg
                    * 1000.0
                    * speed;
                acceleration[0] += coefficient * relative_velocity[0];
                acceleration[1] += coefficient * relative_velocity[1];
                acceleration[2] += coefficient * relative_velocity[2];
            }
        }
    }

    Ok(acceleration)
}

/// 单步 DOPRI5 结果：新状态与误差范数。
struct StepResult {
    state: [f64; 6],
    error: f64,
}

fn derivative(
    state: &[f64; 6],
    t_seconds: f64,
    offset: f64,
    start: DateTime<Utc>,
    satellite: &SatelliteObject,
    j2_enabled: bool,
    drag_enabled: bool,
) -> Result<[f64; 6], String> {
    let position = [state[0], state[1], state[2]];
    let velocity = [state[3], state[4], state[5]];
    let time =
        start + chrono::Duration::milliseconds(((t_seconds + offset) * 1000.0).round() as i64);
    let acceleration =
        acceleration(position, velocity, time, satellite, j2_enabled, drag_enabled)?;
    Ok([
        velocity[0],
        velocity[1],
        velocity[2],
        acceleration[0],
        acceleration[1],
        acceleration[2],
    ])
}

fn combine(
    state: &[f64; 6],
    h: f64,
    k: &[[f64; 6]; 7],
    coefficients: &[(usize, f64)],
) -> [f64; 6] {
    let mut next = *state;
    for (stage_index, coefficient) in coefficients {
        for component in 0..6 {
            next[component] += h * coefficient * k[*stage_index][component];
        }
    }
    next
}

fn dopri5_step(
    state: &[f64; 6],
    h: f64,
    t_seconds: f64,
    start: DateTime<Utc>,
    satellite: &SatelliteObject,
    j2_enabled: bool,
    drag_enabled: bool,
    rtol: f64,
) -> Result<StepResult, String> {
    let mut k = [[0.0_f64; 6]; 7];
    k[0] = derivative(
        state,
        t_seconds,
        0.0,
        start,
        satellite,
        j2_enabled,
        drag_enabled,
    )?;

    let first = combine(state, h, &k, &[(0, 1.0 / 5.0)]);
    k[1] = derivative(&first, t_seconds, h / 5.0, start, satellite, j2_enabled, drag_enabled)?;

    let second = combine(state, h, &k, &[(0, 3.0 / 40.0), (1, 9.0 / 40.0)]);
    k[2] = derivative(
        &second,
        t_seconds,
        3.0 * h / 10.0,
        start,
        satellite,
        j2_enabled,
        drag_enabled,
    )?;

    let third = combine(
        state,
        h,
        &k,
        &[(0, 44.0 / 45.0), (1, -56.0 / 15.0), (2, 32.0 / 9.0)],
    );
    k[3] = derivative(
        &third,
        t_seconds,
        4.0 * h / 5.0,
        start,
        satellite,
        j2_enabled,
        drag_enabled,
    )?;

    let fourth = combine(
        state,
        h,
        &k,
        &[
            (0, 19372.0 / 6561.0),
            (1, -25360.0 / 2187.0),
            (2, 64448.0 / 6561.0),
            (3, -212.0 / 729.0),
        ],
    );
    k[4] = derivative(
        &fourth,
        t_seconds,
        8.0 * h / 9.0,
        start,
        satellite,
        j2_enabled,
        drag_enabled,
    )?;

    let fifth_stage = combine(
        state,
        h,
        &k,
        &[
            (0, 9017.0 / 3168.0),
            (1, -355.0 / 33.0),
            (2, 46732.0 / 5247.0),
            (3, 49.0 / 176.0),
            (4, -5103.0 / 18656.0),
        ],
    );
    k[5] = derivative(
        &fifth_stage,
        t_seconds,
        h,
        start,
        satellite,
        j2_enabled,
        drag_enabled,
    )?;

    // 5 阶解。
    let next = combine(
        state,
        h,
        &k,
        &[
            (0, 35.0 / 384.0),
            (2, 500.0 / 1113.0),
            (3, 125.0 / 192.0),
            (4, -2187.0 / 6784.0),
            (5, 11.0 / 84.0),
        ],
    );

    k[6] = derivative(
        &next,
        t_seconds,
        h,
        start,
        satellite,
        j2_enabled,
        drag_enabled,
    )?;

    // 4 阶解用于误差估计。
    let alternative = combine(
        state,
        h,
        &k,
        &[
            (0, 5179.0 / 57_600.0),
            (2, 7571.0 / 16_695.0),
            (3, 393.0 / 640.0),
            (4, -92_097.0 / 339_200.0),
            (5, 187.0 / 2_100.0),
            (6, 1.0 / 40.0),
        ],
    );

    let mut sum = 0.0_f64;
    for component in 0..6 {
        let absolute = if component < 3 {
            ABS_TOLERANCE_POSITION_KM
        } else {
            ABS_TOLERANCE_VELOCITY_KM_S
        };
        let scale = absolute + rtol * state[component].abs().max(next[component].abs());
        let difference = (next[component] - alternative[component]) / scale;
        sum += difference * difference;
    }
    let error = (sum / 6.0).sqrt();

    Ok(StepResult { state: next, error })
}

/// 使用 DOPRI5 自适应积分，按固定采样间隔输出地固系状态。
pub fn propagate(
    satellite: &SatelliteObject,
    start: DateTime<Utc>,
    total_seconds: f64,
    step_seconds: f64,
) -> Result<NumericalTrack, String> {
    if !total_seconds.is_finite() || total_seconds <= 0.0 {
        return Err("结束时间必须晚于开始时间".into());
    }
    if !step_seconds.is_finite() || step_seconds <= 0.0 {
        return Err("采样步长必须为正数".into());
    }

    let count = (total_seconds / step_seconds).floor() as usize + 1;
    if count > 20_000 {
        return Err(format!("采样点过多（{count}），请增大步长或缩短时间范围"));
    }

    let (elements, constants) = prepare(satellite)?;
    let (position_ecef, velocity_ecef) = propagate_state(&elements, &constants, start)?;
    let gmst_start = gmst_radians(start);
    let mut state = [
        ecef_to_inertial(position_ecef, gmst_start)[0],
        ecef_to_inertial(position_ecef, gmst_start)[1],
        ecef_to_inertial(position_ecef, gmst_start)[2],
        ecef_to_inertial(velocity_ecef, gmst_start)[0],
        ecef_to_inertial(velocity_ecef, gmst_start)[1],
        ecef_to_inertial(velocity_ecef, gmst_start)[2],
    ];

    let j2_enabled = satellite.enable_j2;
    let drag_enabled = satellite.enable_drag;
    let rtol = satellite.integrator_tolerance.clamp(1e-12, 1e-3);

    let mut states = Vec::with_capacity(count);
    let push_state = |t: f64, state: &[f64; 6], out: &mut Vec<NumericalState>| {
        let time = start + chrono::Duration::milliseconds((t * 1000.0).round() as i64);
        let gmst = gmst_radians(time);
        out.push(NumericalState {
            t_seconds: t,
            position: inertial_to_ecef([state[0], state[1], state[2]], gmst),
            velocity: inertial_to_ecef([state[3], state[4], state[5]], gmst),
        });
    };

    push_state(0.0, &state, &mut states);

    let mut t = 0.0_f64;
    let mut current_step = INITIAL_STEP_SECONDS.min(step_seconds.max(MIN_STEP_SECONDS));
    let mut accepted = 0u64;
    let mut rejected = 0u64;
    let mut min_step = f64::INFINITY;
    let mut max_step: f64 = 0.0;
    let mut total_step = 0.0_f64;
    let mut total_steps = 0u64;

    for sample_index in 1..count {
        let target = sample_index as f64 * step_seconds;
        while t < target - 1e-9 {
            if total_steps as usize >= MAX_STEPS {
                return Err("数值积分步数超过上限，请缩短时间范围或放宽容差".into());
            }

            let remaining = target - t;
            let h = current_step.min(remaining).clamp(MIN_STEP_SECONDS, MAX_STEP_SECONDS);
            let attempt = dopri5_step(
                &state,
                h,
                t,
                start,
                satellite,
                j2_enabled,
                drag_enabled,
                rtol,
            )?;
            total_steps += 1;

            if attempt.error.is_finite() && attempt.error <= 1.0 {
                state = attempt.state;
                t += h;
                accepted += 1;
                min_step = min_step.min(h);
                max_step = max_step.max(h);
                total_step += h;

                let growth = if attempt.error > 1e-12 {
                    0.9 * attempt.error.powf(-0.2)
                } else {
                    5.0
                };
                current_step = (h * growth.clamp(0.2, 5.0)).clamp(MIN_STEP_SECONDS, MAX_STEP_SECONDS);
            } else {
                rejected += 1;
                let shrink = if attempt.error.is_finite() && attempt.error > 0.0 {
                    0.9 * attempt.error.powf(-0.2)
                } else {
                    0.1
                };
                current_step = (h * shrink.clamp(0.1, 0.5)).max(MIN_STEP_SECONDS);
                if current_step < MIN_STEP_SECONDS * 1.01 && h <= MIN_STEP_SECONDS * 1.01 {
                    return Err("数值积分步长低于下限，无法满足容差要求".into());
                }
            }
        }

        push_state(target, &state, &mut states);
    }

    let stats = IntegrationStats {
        accepted_steps: accepted,
        rejected_steps: rejected,
        min_step_seconds: if min_step.is_finite() { min_step } else { 0.0 },
        max_step_seconds: max_step,
        mean_step_seconds: if accepted > 0 {
            total_step / accepted as f64
        } else {
            0.0
        },
    };

    Ok(NumericalTrack {
        states,
        stats,
        model: "DOPRI5 + 中心引力 + J2 + NRLMSISE-00 阻力".to_string(),
        j2_enabled,
        drag_enabled,
    })
}

/// Hermite 插值：给定数值积分序列，在任意时刻取地固系状态。
pub fn interpolate(states: &[NumericalState], t_seconds: f64) -> Option<NumericalState> {
    if states.is_empty() {
        return None;
    }
    let first = &states[0];
    let last = &states[states.len() - 1];
    if t_seconds <= first.t_seconds {
        return Some(*first);
    }
    if t_seconds >= last.t_seconds {
        return Some(*last);
    }
    let index = states
        .windows(2)
        .position(|pair| t_seconds >= pair[0].t_seconds && t_seconds <= pair[1].t_seconds)?;
    let a = &states[index];
    let b = &states[index + 1];
    let span = b.t_seconds - a.t_seconds;
    if span <= 0.0 {
        return Some(*a);
    }
    let s = (t_seconds - a.t_seconds) / span;
    let s2 = s * s;
    let s3 = s2 * s;
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    let dh00 = 6.0 * s2 - 6.0 * s;
    let dh10 = 3.0 * s2 - 4.0 * s + 1.0;
    let dh01 = -6.0 * s2 + 6.0 * s;
    let dh11 = 3.0 * s2 - 2.0 * s;

    let mut position = [0.0_f64; 3];
    let mut velocity = [0.0_f64; 3];
    for component in 0..3 {
        position[component] = h00 * a.position[component]
            + h10 * span * a.velocity[component]
            + h01 * b.position[component]
            + h11 * span * b.velocity[component];
        velocity[component] = (dh00 * a.position[component]
            + dh10 * span * a.velocity[component]
            + dh01 * b.position[component]
            + dh11 * span * b.velocity[component])
            / span;
    }

    Some(NumericalState {
        t_seconds,
        position,
        velocity,
    })
}
