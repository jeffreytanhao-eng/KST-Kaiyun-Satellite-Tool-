//! Neutral atmosphere models with explicit model-specific inputs and SI outputs.
//!
//! 本模块代码取自 https://github.com/spacelibres/STKCraft-SpaceEnv（MIT / LGPL-3.0-or-later），
//! 仅保留大气部分并在项目内以 `spaceenv` 模块编译，避免依赖尚未发布的 crate。
#![allow(dead_code)]

mod nrlmsise00;
pub use nrlmsise00::{
    nrlmsise00, Nrlmsise00Activity, Nrlmsise00Ap, Nrlmsise00Input,
};

/// Thermodynamic state of dry air in SI units.
#[derive(Debug, Clone, Copy)]
pub struct AtmosphereState {
    /// Kinetic temperature, K.
    pub temperature_k: f64,
    /// Static pressure, Pa.
    pub pressure_pa: f64,
    /// Mass density, kg/m³.
    pub density_kg_m3: f64,
}

/// Input is not a finite geometric height within the implemented range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAltitude;

impl std::fmt::Display for InvalidAltitude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("geometric altitude must be finite and in 0..=70000 m")
    }
}
impl std::error::Error for InvalidAltitude {}

/// Evaluate geometric height in metres, restricted to 0 through 70 km.
/// The upper range is excluded until molecular-weight corrections are implemented.
pub fn us1976(altitude_m: f64) -> Result<AtmosphereState, InvalidAltitude> {
    if !altitude_m.is_finite() || !(0.0..=70_000.0).contains(&altitude_m) {
        return Err(InvalidAltitude);
    }
    const R: f64 = 6356766.0;
    const GMR: f64 = 9.80665 * 28.9644 / 8314.32;
    const BOUNDS: [f64; 7] = [0.0, 11000.0, 20000.0, 32000.0, 47000.0, 51000.0, 71000.0];
    const LAPSE: [f64; 6] = [-0.0065, 0.0, 0.001, 0.0028, 0.0, -0.0028];
    let h = R * altitude_m / (R + altitude_m);
    let mut t = 288.15;
    let mut p = 101325.0;
    for i in 0..LAPSE.len() {
        let dh = h.min(BOUNDS[i + 1]) - BOUNDS[i];
        if dh <= 0.0 {
            break;
        }
        let next = t + LAPSE[i] * dh;
        p *= if LAPSE[i] == 0.0 {
            (-GMR * dh / t).exp()
        } else {
            (t / next).powf(GMR / LAPSE[i])
        };
        t = next;
        if h <= BOUNDS[i + 1] {
            break;
        }
    }
    Ok(AtmosphereState {
        temperature_k: t,
        pressure_pa: p,
        density_kg_m3: p * 28.9644 / (8314.32 * t),
    })
}
