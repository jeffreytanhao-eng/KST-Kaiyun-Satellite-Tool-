//! NRLMSISE-00 neutral atmosphere, standard physical switches.
//!
//! Inputs are explicit: no time conversion, ephemeris, activity lookup or
//! fallback is performed here. The validated altitude range is 0–1000 km.
//! See docs/MODELS.md for provenance and limitations.
mod coefficients;
mod engine;

/// Geomagnetic input: current daily Ap or the author's seven-slot history.
/// History order is daily, current, -3/-6/-9 h, mean -12..-33 h, mean -36..-57 h.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nrlmsise00Ap {
    Daily(f64),
    History([f64; 7]),
}

/// Actual Sun–Earth distance fluxes, not flux normalized to 1 AU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nrlmsise00Activity {
    /// Observed F10.7 of the previous UTC calendar day, sfu.
    pub daily_f107_sfu: f64,
    /// Observed 81-day mean centred on the current UTC day, sfu.
    pub mean81_f107_sfu: f64,
    pub ap: Nrlmsise00Ap,
}

/// Model-native independent variables, with public geometry in SI/radians.
///
/// The caller supplies a UTC day/second and consistent activity input. The
/// year is not used by this model; a day number of 366 is accepted. UTC leap
/// seconds require an explicit caller policy: seconds >= 86400 are rejected.
/// Apparent solar time is independent of longitude and UT; it is not derived
/// from mean solar time. Below 80 km the author recommends 150/150/4 activity
/// inputs; this function does not silently replace supplied measurements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nrlmsise00Input {
    pub day_of_year: u16,
    pub utc_seconds: f64,
    /// Geodetic height above the caller's reference ellipsoid, m.
    pub altitude_m: f64,
    /// Geodetic latitude, rad, in [-π/2, π/2].
    pub latitude_rad: f64,
    /// East longitude, rad, in [-π, π].
    pub longitude_rad: f64,
    /// Local apparent solar time, hours, in [0, 24).
    pub local_apparent_solar_time_hours: f64,
    pub activity: Nrlmsise00Activity,
}

/// Number densities in m⁻³. Anomalous oxygen is reported separately.
/// Some species are identically zero in the model's lower-atmosphere branch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nrlmsise00Species {
    pub he_m3: f64,
    pub o_m3: f64,
    pub n2_m3: f64,
    pub o2_m3: f64,
    pub ar_m3: f64,
    pub h_m3: f64,
    pub n_m3: f64,
    pub anomalous_o_m3: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nrlmsise00State {
    pub species: Nrlmsise00Species,
    /// GTD7 total mass density excluding anomalous oxygen, kg/m³.
    pub density_kg_m3: f64,
    /// GTD7D effective mass density including anomalous oxygen, kg/m³.
    /// This is a model output, not a calculation of spacecraft drag.
    pub effective_density_kg_m3: f64,
    pub exospheric_temperature_k: f64,
    pub temperature_k: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nrlmsise00Error {
    InvalidInput(&'static str),
    /// The model could not produce finite, physically signed SI outputs.
    Numeric,
}

impl std::fmt::Display for Nrlmsise00Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(field) => write!(f, "invalid NRLMSISE-00 {field}"),
            Self::Numeric => f.write_str("NRLMSISE-00 output is not physically representable"),
        }
    }
}
impl std::error::Error for Nrlmsise00Error {}

/// Evaluate the standard model with all physical switches enabled; switch 9
/// selects daily Ap or history according to the explicit input enum.
///
/// Each evaluation owns its numerical state. No shared mutable cache, global
/// activity defaults or ambient data source is used. The outputs represent an
/// empirical atmosphere, not a guarantee of current atmospheric conditions.
pub fn nrlmsise00(input: &Nrlmsise00Input) -> Result<Nrlmsise00State, Nrlmsise00Error> {
    use std::f64::consts::{FRAC_PI_2, PI};
    use Nrlmsise00Error::InvalidInput;
    if !(1..=366).contains(&input.day_of_year) {
        return Err(InvalidInput("day of year"));
    }
    for (value, low, high, inclusive, field) in [
        (input.utc_seconds, 0.0, 86400.0, false, "UTC seconds"),
        (input.altitude_m, 0.0, 1000000.0, true, "geodetic altitude"),
        (
            input.latitude_rad,
            -FRAC_PI_2,
            FRAC_PI_2,
            true,
            "geodetic latitude",
        ),
        (input.longitude_rad, -PI, PI, true, "east longitude"),
        (
            input.local_apparent_solar_time_hours,
            0.0,
            24.0,
            false,
            "apparent solar time",
        ),
    ] {
        if !value.is_finite() || value < low || value > high || (!inclusive && value == high) {
            return Err(InvalidInput(field));
        }
    }
    for (value, field) in [
        (input.activity.daily_f107_sfu, "daily F10.7"),
        (input.activity.mean81_f107_sfu, "centred mean F10.7"),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(InvalidInput(field));
        }
    }
    let (ap, history) = match input.activity.ap {
        Nrlmsise00Ap::Daily(ap) => (ap, None),
        Nrlmsise00Ap::History(history) => (history[0], Some(history)),
    };
    if !ap.is_finite()
        || ap < 0.0
        || history.is_some_and(|h| h.iter().any(|v| !v.is_finite() || *v < 0.0))
    {
        return Err(InvalidInput("Ap"));
    }
    let (d, t) = engine::evaluate(
        input.day_of_year,
        input.utc_seconds,
        input.altitude_m / 1000.0,
        input.latitude_rad.to_degrees(),
        input.longitude_rad.to_degrees(),
        input.local_apparent_solar_time_hours,
        input.activity.mean81_f107_sfu,
        input.activity.daily_f107_sfu,
        ap,
        history,
    );
    let species = Nrlmsise00Species {
        he_m3: d[0] * 1e6,
        o_m3: d[1] * 1e6,
        n2_m3: d[2] * 1e6,
        o2_m3: d[3] * 1e6,
        ar_m3: d[4] * 1e6,
        h_m3: d[6] * 1e6,
        n_m3: d[7] * 1e6,
        anomalous_o_m3: d[8] * 1e6,
    };
    let density = d[5] * 1000.0;
    // Algebraically the author's GTD7D definition: add 16 u per anomalous O.
    // Keep GTD7's original lower-atmosphere mixture rather than reconstructing it.
    let effective = density + 16.0 * 1.66e-27 * species.anomalous_o_m3;
    if [
        species.he_m3,
        species.o_m3,
        species.n2_m3,
        species.o2_m3,
        species.ar_m3,
        species.h_m3,
        species.n_m3,
        species.anomalous_o_m3,
    ]
    .iter()
    .any(|v| !v.is_finite() || *v < 0.0)
        || [density, effective, t[0], t[1]]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
    {
        return Err(Nrlmsise00Error::Numeric);
    }
    Ok(Nrlmsise00State {
        species,
        density_kg_m3: density,
        effective_density_kg_m3: effective,
        exospheric_temperature_k: t[0],
        temperature_k: t[1],
    })
}
