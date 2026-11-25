//! NRLMSISE-00 numerical implementation adapted from satkit.
//! Copyright (c) 2025 Steven Michael. MIT license: LICENSE-MIT.
//! Upstream commit: 30dd3fcb3bf7dbdfe9429c9d56ac6fc74b8ca77b.
//! Model by Picone, Hedin and Drob; C implementation by Dominik Brodowski.
//! Local changes and independent reference: docs/MODELS.md.
#![allow(
    clippy::possible_missing_else,
    clippy::needless_range_loop,
    clippy::too_many_arguments
)]
use super::coefficients::*;

struct NrlmsiseState {
    gsurf: f64,
    re: f64,
    dd: f64,
    dm04: f64,
    dm16: f64,
    dm28: f64,
    dm32: f64,
    dm40: f64,
    dm01: f64,
    dm14: f64,
    meso_tn1: [f64; 5],
    meso_tn2: [f64; 4],
    meso_tn3: [f64; 5],
    meso_tgn1: [f64; 2],
    meso_tgn2: [f64; 2],
    meso_tgn3: [f64; 2],
    dfa: f64,
    plg: [[f64; 9]; 4],
    ctloc: f64,
    stloc: f64,
    c2tloc: f64,
    s2tloc: f64,
    s3tloc: f64,
    c3tloc: f64,
    clong: f64,
    slong: f64,
    apdf: f64,
    apt: [f64; 4],
}

impl NrlmsiseState {
    fn new() -> Self {
        Self {
            gsurf: 0.0,
            re: 0.0,
            dd: 0.0,
            dm04: 0.0,
            dm16: 0.0,
            dm28: 0.0,
            dm32: 0.0,
            dm40: 0.0,
            dm01: 0.0,
            dm14: 0.0,
            meso_tn1: [0.0; 5],
            meso_tn2: [0.0; 4],
            meso_tn3: [0.0; 5],
            meso_tgn1: [0.0; 2],
            meso_tgn2: [0.0; 2],
            meso_tgn3: [0.0; 2],
            dfa: 0.0,
            plg: [[0.0; 9]; 4],
            ctloc: 0.0,
            stloc: 0.0,
            c2tloc: 0.0,
            s2tloc: 0.0,
            s3tloc: 0.0,
            c3tloc: 0.0,
            clong: 0.0,
            slong: 0.0,
            apdf: 0.0,
            apt: [0.0; 4],
        }
    }
}

struct NrlmsiseFlags {
    switches: [i32; 24],
    sw: [f64; 24],
    swc: [f64; 24],
}
struct NrlmsiseInput {
    doy: i32,
    sec: f64,
    alt: f64,
    g_lat: f64,
    g_long: f64,
    lst: f64,
    f107a: f64,
    f107: f64,
    ap: f64,
    ap_a: Option<[f64; 7]>,
}
struct NrlmsiseOutput {
    d: [f64; 9],
    t: [f64; 2],
}

fn tselec(flags: &mut NrlmsiseFlags) {
    for i in 0..24 {
        if i != 9 {
            flags.sw[i] = if flags.switches[i] == 1 { 1.0 } else { 0.0 };
            flags.swc[i] = if flags.switches[i] > 0 { 1.0 } else { 0.0 };
        } else {
            flags.sw[i] = flags.switches[i] as f64;
            flags.swc[i] = flags.switches[i] as f64;
        }
    }
}

fn glatf(lat: f64, gv: &mut f64, reff: &mut f64) {
    let dgtr = 1.74533E-2_f64;
    let c2 = (2.0 * dgtr * lat).cos();
    *gv = 980.616 * (1.0 - 0.0026373 * c2);
    *reff = 2.0 * (*gv) / (3.085462E-6 + 2.27E-9 * c2) * 1.0E-5;
}

fn ccor(alt: f64, r: f64, h1: f64, zh: f64) -> f64 {
    let e = (alt - zh) / h1;
    if e > 70.0 {
        return 1.0;
    }
    if e < -70.0 {
        return r.exp();
    }
    (r / (1.0 + e.exp())).exp()
}
fn ccor2(alt: f64, r: f64, h1: f64, zh: f64, h2: f64) -> f64 {
    let e1 = (alt - zh) / h1;
    let e2 = (alt - zh) / h2;
    if e1 > 70.0 || e2 > 70.0 {
        return 1.0;
    }
    if e1 < -70.0 && e2 < -70.0 {
        return r.exp();
    }
    (r / (1.0 + 0.5 * (e1.exp() + e2.exp()))).exp()
}
fn scalh(alt: f64, xm: f64, temp: f64, gsurf: f64, re: f64) -> f64 {
    let rgas = 831.4_f64;
    let r = 1.0 + alt / re;
    rgas * temp / (gsurf / (r * r) * xm)
}
fn dnet(dd: f64, dm: f64, zhm: f64, xmm: f64, xm: f64) -> f64 {
    let a = zhm / (xmm - xm);
    if !(dm > 0.0 && dd > 0.0) {
        if dd == 0.0 && dm == 0.0 {
            return 1.0;
        }
        if dm == 0.0 {
            return dd;
        }
        if dd == 0.0 {
            return dm;
        }
    }
    let ylog = a * (dm / dd).ln();
    if ylog < -10.0 {
        return dd;
    }
    if ylog > 10.0 {
        return dm;
    }
    dd * (1.0 + ylog.exp()).powf(1.0 / a)
}
#[inline]
fn zeta_fn(zz: f64, zl: f64, re: f64) -> f64 {
    (zz - zl) * (re + zl) / (re + zz)
}

fn splini(xa: &[f64], ya: &[f64], y2a: &[f64], n: usize, x: f64) -> f64 {
    let mut yi = 0.0;
    let mut klo = 0;
    let mut khi = 1;
    while x > xa[klo] && khi < n {
        let xx = if khi < n - 1 {
            if x < xa[khi] {
                x
            } else {
                xa[khi]
            }
        } else {
            x
        };
        let h = xa[khi] - xa[klo];
        let a = (xa[khi] - xx) / h;
        let b = (xx - xa[klo]) / h;
        let a2 = a * a;
        let b2 = b * b;
        yi += ((1.0 - a2) * ya[klo] / 2.0
            + b2 * ya[khi] / 2.0
            + ((-(1.0 + a2 * a2) / 4.0 + a2 / 2.0) * y2a[klo]
                + (b2 * b2 / 4.0 - b2 / 2.0) * y2a[khi])
                * h
                * h
                / 6.0)
            * h;
        klo += 1;
        khi += 1;
    }
    yi
}
fn splint(xa: &[f64], ya: &[f64], y2a: &[f64], n: usize, x: f64) -> f64 {
    let mut klo = 0;
    let mut khi = n - 1;
    while (khi - klo) > 1 {
        let k = (khi + klo) / 2;
        if xa[k] > x {
            khi = k;
        } else {
            klo = k;
        }
    }
    let h = xa[khi] - xa[klo];
    let a = (xa[khi] - x) / h;
    let b = (x - xa[klo]) / h;
    a * ya[klo]
        + b * ya[khi]
        + ((a * a * a - a) * y2a[klo] + (b * b * b - b) * y2a[khi]) * h * h / 6.0
}
fn spline(x: &[f64], y: &[f64], n: usize, yp1: f64, ypn: f64, y2: &mut [f64]) {
    let mut u = vec![0.0_f64; n];
    if yp1 > 0.99E30 {
        y2[0] = 0.0;
        u[0] = 0.0;
    } else {
        y2[0] = -0.5;
        u[0] = (3.0 / (x[1] - x[0])) * ((y[1] - y[0]) / (x[1] - x[0]) - yp1);
    }
    for i in 1..(n - 1) {
        let sig = (x[i] - x[i - 1]) / (x[i + 1] - x[i - 1]);
        let p = sig * y2[i - 1] + 2.0;
        y2[i] = (sig - 1.0) / p;
        u[i] = (6.0
            * ((y[i + 1] - y[i]) / (x[i + 1] - x[i]) - (y[i] - y[i - 1]) / (x[i] - x[i - 1]))
            / (x[i + 1] - x[i - 1])
            - sig * u[i - 1])
            / p;
    }
    let (qn, un) = if ypn > 0.99E30 {
        (0.0, 0.0)
    } else {
        (
            0.5,
            (3.0 / (x[n - 1] - x[n - 2])) * (ypn - (y[n - 1] - y[n - 2]) / (x[n - 1] - x[n - 2])),
        )
    };
    y2[n - 1] = (un - qn * u[n - 2]) / (qn * y2[n - 2] + 1.0);
    for k in (0..=(n - 2)).rev() {
        y2[k] = y2[k] * y2[k + 1] + u[k];
    }
}

fn densm(
    alt: f64,
    d0: f64,
    xm: f64,
    tz: &mut f64,
    mn3: usize,
    zn3: &[f64],
    tn3: &[f64],
    tgn3: &[f64],
    mn2: usize,
    zn2: &[f64],
    tn2: &[f64],
    tgn2: &[f64],
    gsurf: f64,
    re: f64,
) -> f64 {
    let rgas = 831.4_f64;
    let mut densm_tmp = d0;
    if alt > zn2[0] {
        if xm == 0.0 {
            return *tz;
        } else {
            return d0;
        }
    }
    let z = if alt > zn2[mn2 - 1] {
        alt
    } else {
        zn2[mn2 - 1]
    };
    let mn = mn2;
    let z1 = zn2[0];
    let z2 = zn2[mn - 1];
    let t1 = tn2[0];
    let t2 = tn2[mn - 1];
    let zg = zeta_fn(z, z1, re);
    let zgdif = zeta_fn(z2, z1, re);
    let mut xs = [0.0_f64; 10];
    let mut ys = [0.0_f64; 10];
    let mut y2out = [0.0_f64; 10];
    for k in 0..mn {
        xs[k] = zeta_fn(zn2[k], z1, re) / zgdif;
        ys[k] = 1.0 / tn2[k];
    }
    let yd1 = -tgn2[0] / (t1 * t1) * zgdif;
    let yd2 = -tgn2[1] / (t2 * t2) * zgdif * ((re + z2) / (re + z1)).powi(2);
    spline(&xs[..mn], &ys[..mn], mn, yd1, yd2, &mut y2out[..mn]);
    let x = zg / zgdif;
    let y = splint(&xs[..mn], &ys[..mn], &y2out[..mn], mn, x);
    *tz = 1.0 / y;
    if xm != 0.0 {
        let glb = gsurf / (1.0 + z1 / re).powi(2);
        let gamm = xm * glb * zgdif / rgas;
        let yi = splini(&xs[..mn], &ys[..mn], &y2out[..mn], mn, x);
        let mut expl = gamm * yi;
        if expl > 50.0 {
            expl = 50.0;
        }
        densm_tmp = densm_tmp * (t1 / *tz) * (-expl).exp();
    }
    if alt > zn3[0] {
        if xm == 0.0 {
            return *tz;
        } else {
            return densm_tmp;
        }
    }
    let z = alt;
    let mn = mn3;
    let z1 = zn3[0];
    let z2 = zn3[mn - 1];
    let t1 = tn3[0];
    let t2 = tn3[mn - 1];
    let zg = zeta_fn(z, z1, re);
    let zgdif = zeta_fn(z2, z1, re);
    for k in 0..mn {
        xs[k] = zeta_fn(zn3[k], z1, re) / zgdif;
        ys[k] = 1.0 / tn3[k];
    }
    let yd1 = -tgn3[0] / (t1 * t1) * zgdif;
    let yd2 = -tgn3[1] / (t2 * t2) * zgdif * ((re + z2) / (re + z1)).powi(2);
    spline(&xs[..mn], &ys[..mn], mn, yd1, yd2, &mut y2out[..mn]);
    let x = zg / zgdif;
    let y = splint(&xs[..mn], &ys[..mn], &y2out[..mn], mn, x);
    *tz = 1.0 / y;
    if xm != 0.0 {
        let glb = gsurf / (1.0 + z1 / re).powi(2);
        let gamm = xm * glb * zgdif / rgas;
        let yi = splini(&xs[..mn], &ys[..mn], &y2out[..mn], mn, x);
        let mut expl = gamm * yi;
        if expl > 50.0 {
            expl = 50.0;
        }
        densm_tmp = densm_tmp * (t1 / *tz) * (-expl).exp();
    }
    if xm == 0.0 {
        *tz
    } else {
        densm_tmp
    }
}

fn densu(
    alt: f64,
    dlb: f64,
    tinf: f64,
    tlb: f64,
    xm: f64,
    alpha: f64,
    tz: &mut f64,
    zlb: f64,
    s2: f64,
    mn1: usize,
    zn1: &[f64],
    tn1: &mut [f64],
    tgn1: &mut [f64],
    gsurf: f64,
    re: f64,
) -> f64 {
    let rgas = 831.4_f64;
    let za = zn1[0];
    let z = if alt > za { alt } else { za };
    let zg2 = zeta_fn(z, zlb, re);
    let tt = tinf - (tinf - tlb) * (-s2 * zg2).exp();
    let ta = tt;
    *tz = tt;
    let mut x = 0.0_f64;
    let mut z1 = 0.0_f64;
    let mut zgdif = 0.0_f64;
    let mut xs = [0.0_f64; 5];
    let mut ys = [0.0_f64; 5];
    let mut y2out = [0.0_f64; 5];
    let mut mn = 0_usize;
    if alt < za {
        let dta = (tinf - ta) * s2 * ((re + zlb) / (re + za)).powi(2);
        tgn1[0] = dta;
        tn1[0] = ta;
        let z_local = if alt > zn1[mn1 - 1] {
            alt
        } else {
            zn1[mn1 - 1]
        };
        mn = mn1;
        z1 = zn1[0];
        let z2 = zn1[mn - 1];
        let t1 = tn1[0];
        let t2 = tn1[mn - 1];
        let zg = zeta_fn(z_local, z1, re);
        zgdif = zeta_fn(z2, z1, re);
        for k in 0..mn {
            xs[k] = zeta_fn(zn1[k], z1, re) / zgdif;
            ys[k] = 1.0 / tn1[k];
        }
        let yd1 = -tgn1[0] / (t1 * t1) * zgdif;
        let yd2 = -tgn1[1] / (t2 * t2) * zgdif * ((re + z2) / (re + z1)).powi(2);
        spline(&xs[..mn], &ys[..mn], mn, yd1, yd2, &mut y2out[..mn]);
        x = zg / zgdif;
        let y = splint(&xs[..mn], &ys[..mn], &y2out[..mn], mn, x);
        *tz = 1.0 / y;
    }
    if xm == 0.0 {
        return *tz;
    }
    let glb = gsurf / (1.0 + zlb / re).powi(2);
    let gamma = xm * glb / (s2 * rgas * tinf);
    let mut expl = (-s2 * gamma * zg2).exp();
    if expl > 50.0 {
        expl = 50.0;
    }
    if tt <= 0.0 {
        expl = 50.0;
    }
    let densa = dlb * (tlb / tt).powf(1.0 + alpha + gamma) * expl;
    let mut densu_temp = densa;
    if alt >= za {
        return densu_temp;
    }
    let glb = gsurf / (1.0 + z1 / re).powi(2);
    let gamm = xm * glb * zgdif / rgas;
    let yi = splini(&xs[..mn], &ys[..mn], &y2out[..mn], mn, x);
    let mut expl = gamm * yi;
    if expl > 50.0 {
        expl = 50.0;
    }
    if *tz <= 0.0 {
        expl = 50.0;
    }
    densu_temp = densu_temp * (tn1[0] / *tz).powf(1.0 + alpha) * (-expl).exp();
    densu_temp
}

#[inline]
fn g0_fn(a: f64, p: &[f64]) -> f64 {
    a - 4.0 + (p[25] - 1.0) * (a - 4.0 + ((-p[24].abs() * (a - 4.0)).exp() - 1.0) / p[24].abs())
}
#[inline]
fn sumex(ex: f64) -> f64 {
    1.0 + (1.0 - ex.powi(19)) / (1.0 - ex) * ex.sqrt()
}
#[inline]
fn sg0(ex: f64, p: &[f64], ap: &[f64]) -> f64 {
    (g0_fn(ap[1], p)
        + (g0_fn(ap[2], p) * ex
            + g0_fn(ap[3], p) * ex * ex
            + g0_fn(ap[4], p) * ex.powi(3)
            + (g0_fn(ap[5], p) * ex.powi(4) + g0_fn(ap[6], p) * ex.powi(12)) * (1.0 - ex.powi(8))
                / (1.0 - ex)))
        / sumex(ex)
}

/// Terms of `globe7` and `glob7s` that depend only on the input, computed
/// once per density evaluation rather than in each of their calls.
fn globe_setup(input: &NrlmsiseInput, flags: &NrlmsiseFlags, state: &mut NrlmsiseState) {
    let dgtr = 1.74533E-2_f64;
    let hr = 0.2618_f64;
    let tloc = input.lst;
    let c = (input.g_lat * dgtr).sin();
    let s = (input.g_lat * dgtr).cos();
    let c2 = c * c;
    let c4 = c2 * c2;
    let s2 = s * s;
    state.plg[0][1] = c;
    state.plg[0][2] = 0.5 * (3.0 * c2 - 1.0);
    state.plg[0][3] = 0.5 * (5.0 * c * c2 - 3.0 * c);
    state.plg[0][4] = (35.0 * c4 - 30.0 * c2 + 3.0) / 8.0;
    state.plg[0][5] = (63.0 * c2 * c2 * c - 70.0 * c2 * c + 15.0 * c) / 8.0;
    state.plg[0][6] = (11.0 * c * state.plg[0][5] - 5.0 * state.plg[0][4]) / 6.0;
    state.plg[1][1] = s;
    state.plg[1][2] = 3.0 * c * s;
    state.plg[1][3] = 1.5 * (5.0 * c2 - 1.0) * s;
    state.plg[1][4] = 2.5 * (7.0 * c2 * c - 3.0 * c) * s;
    state.plg[1][5] = 1.875 * (21.0 * c4 - 14.0 * c2 + 1.0) * s;
    state.plg[1][6] = (11.0 * c * state.plg[1][5] - 6.0 * state.plg[1][4]) / 5.0;
    state.plg[2][2] = 3.0 * s2;
    state.plg[2][3] = 15.0 * s2 * c;
    state.plg[2][4] = 7.5 * (7.0 * c2 - 1.0) * s2;
    state.plg[2][5] = 3.0 * c * state.plg[2][4] - 2.0 * state.plg[2][3];
    state.plg[2][6] = (11.0 * c * state.plg[2][5] - 7.0 * state.plg[2][4]) / 4.0;
    state.plg[2][7] = (13.0 * c * state.plg[2][6] - 8.0 * state.plg[2][5]) / 5.0;
    state.plg[3][3] = 15.0 * s2 * s;
    state.plg[3][4] = 105.0 * s2 * s * c;
    state.plg[3][5] = (9.0 * c * state.plg[3][4] - 7.0 * state.plg[3][3]) / 2.0;
    state.plg[3][6] = (11.0 * c * state.plg[3][5] - 8.0 * state.plg[3][4]) / 3.0;
    if !((flags.sw[7] == 0.0 && flags.sw[8] == 0.0) && flags.sw[14] == 0.0) {
        state.stloc = (hr * tloc).sin();
        state.ctloc = (hr * tloc).cos();
        state.s2tloc = (2.0 * hr * tloc).sin();
        state.c2tloc = (2.0 * hr * tloc).cos();
        state.s3tloc = (3.0 * hr * tloc).sin();
        state.c3tloc = (3.0 * hr * tloc).cos();
    }
    state.clong = (dgtr * input.g_long).cos();
    state.slong = (dgtr * input.g_long).sin();
}

fn globe7(
    p: &[f64],
    input: &NrlmsiseInput,
    flags: &NrlmsiseFlags,
    state: &mut NrlmsiseState,
) -> f64 {
    let sr = 7.2722E-5_f64;
    let dgtr = 1.74533E-2_f64;
    let dr = 1.72142E-2_f64;
    let hr = 0.2618_f64;
    let mut t = [0.0_f64; 15];
    let tloc = input.lst;
    let cd32 = (dr * (input.doy as f64 - p[31])).cos();
    let cd18 = (2.0 * dr * (input.doy as f64 - p[17])).cos();
    let cd14 = (dr * (input.doy as f64 - p[13])).cos();
    let cd39 = (2.0 * dr * (input.doy as f64 - p[38])).cos();
    let df = input.f107 - input.f107a;
    state.dfa = input.f107a - 150.0;
    t[0] = p[19] * df * (1.0 + p[59] * state.dfa)
        + p[20] * df * df
        + p[21] * state.dfa
        + p[29] * state.dfa * state.dfa;
    let f1 = 1.0 + (p[47] * state.dfa + p[19] * df + p[20] * df * df) * flags.swc[1];
    let f2 = 1.0 + (p[49] * state.dfa + p[19] * df + p[20] * df * df) * flags.swc[1];
    t[1] = (p[1] * state.plg[0][2] + p[2] * state.plg[0][4] + p[22] * state.plg[0][6])
        + p[14] * state.plg[0][2] * state.dfa * flags.swc[1]
        + p[26] * state.plg[0][1];
    t[2] = p[18] * cd32;
    t[3] = (p[15] + p[16] * state.plg[0][2]) * cd18;
    t[4] = f1 * (p[9] * state.plg[0][1] + p[10] * state.plg[0][3]) * cd14;
    t[5] = p[37] * state.plg[0][1] * cd39;
    if flags.sw[7] != 0.0 {
        let t71 = p[11] * state.plg[1][2] * cd14 * flags.swc[5];
        let t72 = p[12] * state.plg[1][2] * cd14 * flags.swc[5];
        t[6] = f2
            * ((p[3] * state.plg[1][1] + p[4] * state.plg[1][3] + p[27] * state.plg[1][5] + t71)
                * state.ctloc
                + (p[6] * state.plg[1][1]
                    + p[7] * state.plg[1][3]
                    + p[28] * state.plg[1][5]
                    + t72)
                    * state.stloc);
    }
    if flags.sw[8] != 0.0 {
        let t81 = (p[23] * state.plg[2][3] + p[35] * state.plg[2][5]) * cd14 * flags.swc[5];
        let t82 = (p[33] * state.plg[2][3] + p[36] * state.plg[2][5]) * cd14 * flags.swc[5];
        t[7] = f2
            * ((p[5] * state.plg[2][2] + p[41] * state.plg[2][4] + t81) * state.c2tloc
                + (p[8] * state.plg[2][2] + p[42] * state.plg[2][4] + t82) * state.s2tloc);
    }
    if flags.sw[14] != 0.0 {
        t[13] = f2
            * ((p[39] * state.plg[3][3]
                + (p[93] * state.plg[3][4] + p[46] * state.plg[3][6]) * cd14 * flags.swc[5])
                * state.s3tloc
                + (p[40] * state.plg[3][3]
                    + (p[94] * state.plg[3][4] + p[48] * state.plg[3][6]) * cd14 * flags.swc[5])
                    * state.c3tloc);
    }
    if flags.sw[9] as i32 == -1 {
        if let Some(ref ap_a) = input.ap_a {
            if p[51] != 0.0 {
                let exp1_arg = -10800.0 * (p[51] * p[51]).sqrt()
                    / (1.0 + p[138] * (45.0 - (input.g_lat * input.g_lat).sqrt()));
                let mut exp1 = exp1_arg.exp();
                if exp1 > 0.99999 {
                    exp1 = 0.99999;
                }
                state.apt[0] = sg0(exp1, p, ap_a);
                if flags.sw[9] != 0.0 {
                    t[8] = state.apt[0]
                        * (p[50]
                            + p[96] * state.plg[0][2]
                            + p[54] * state.plg[0][4]
                            + (p[125] * state.plg[0][1]
                                + p[126] * state.plg[0][3]
                                + p[127] * state.plg[0][5])
                                * cd14
                                * flags.swc[5]
                            + (p[128] * state.plg[1][1]
                                + p[129] * state.plg[1][3]
                                + p[130] * state.plg[1][5])
                                * flags.swc[7]
                                * (hr * (tloc - p[131])).cos());
                }
            }
        }
    } else {
        let apd = input.ap - 4.0;
        let mut p44 = p[43];
        let p45 = p[44];
        if p44 < 0.0 {
            p44 = 1.0E-5;
        }
        state.apdf = apd + (p45 - 1.0) * (apd + ((-p44 * apd).exp() - 1.0) / p44);
        if flags.sw[9] != 0.0 {
            t[8] = state.apdf
                * (p[32]
                    + p[45] * state.plg[0][2]
                    + p[34] * state.plg[0][4]
                    + (p[100] * state.plg[0][1]
                        + p[101] * state.plg[0][3]
                        + p[102] * state.plg[0][5])
                        * cd14
                        * flags.swc[5]
                    + (p[121] * state.plg[1][1]
                        + p[122] * state.plg[1][3]
                        + p[123] * state.plg[1][5])
                        * flags.swc[7]
                        * (hr * (tloc - p[124])).cos());
        }
    }
    if flags.sw[10] != 0.0 && input.g_long > -1000.0 {
        if flags.sw[11] != 0.0 {
            t[10] = (1.0 + p[80] * state.dfa * flags.swc[1])
                * ((p[64] * state.plg[1][2]
                    + p[65] * state.plg[1][4]
                    + p[66] * state.plg[1][6]
                    + p[103] * state.plg[1][1]
                    + p[104] * state.plg[1][3]
                    + p[105] * state.plg[1][5]
                    + flags.swc[5]
                        * (p[109] * state.plg[1][1]
                            + p[110] * state.plg[1][3]
                            + p[111] * state.plg[1][5])
                        * cd14)
                    * state.clong
                    + (p[90] * state.plg[1][2]
                        + p[91] * state.plg[1][4]
                        + p[92] * state.plg[1][6]
                        + p[106] * state.plg[1][1]
                        + p[107] * state.plg[1][3]
                        + p[108] * state.plg[1][5]
                        + flags.swc[5]
                            * (p[112] * state.plg[1][1]
                                + p[113] * state.plg[1][3]
                                + p[114] * state.plg[1][5])
                            * cd14)
                        * state.slong);
        }
        if flags.sw[12] != 0.0 {
            t[11] = (1.0 + p[95] * state.plg[0][1])
                * (1.0 + p[81] * state.dfa * flags.swc[1])
                * (1.0 + p[119] * state.plg[0][1] * flags.swc[5] * cd14)
                * ((p[68] * state.plg[0][1] + p[69] * state.plg[0][3] + p[70] * state.plg[0][5])
                    * (sr * (input.sec - p[71])).cos());
            t[11] += flags.swc[11]
                * (p[76] * state.plg[2][3] + p[77] * state.plg[2][5] + p[78] * state.plg[2][7])
                * (sr * (input.sec - p[79]) + 2.0 * dgtr * input.g_long).cos()
                * (1.0 + p[137] * state.dfa * flags.swc[1]);
        }
        if flags.sw[13] != 0.0 {
            if flags.sw[9] as i32 == -1 {
                if let Some(ref _ap_a) = input.ap_a {
                    if p[51] != 0.0 {
                        t[12] = state.apt[0]
                            * flags.swc[11]
                            * (1.0 + p[132] * state.plg[0][1])
                            * ((p[52] * state.plg[1][2]
                                + p[98] * state.plg[1][4]
                                + p[67] * state.plg[1][6])
                                * (dgtr * (input.g_long - p[97])).cos())
                            + state.apt[0]
                                * flags.swc[11]
                                * flags.swc[5]
                                * (p[133] * state.plg[1][1]
                                    + p[134] * state.plg[1][3]
                                    + p[135] * state.plg[1][5])
                                * cd14
                                * (dgtr * (input.g_long - p[136])).cos()
                            + state.apt[0]
                                * flags.swc[12]
                                * (p[55] * state.plg[0][1]
                                    + p[56] * state.plg[0][3]
                                    + p[57] * state.plg[0][5])
                                * (sr * (input.sec - p[58])).cos();
                    }
                }
            } else {
                t[12] = state.apdf
                    * flags.swc[11]
                    * (1.0 + p[120] * state.plg[0][1])
                    * ((p[60] * state.plg[1][2]
                        + p[61] * state.plg[1][4]
                        + p[62] * state.plg[1][6])
                        * (dgtr * (input.g_long - p[63])).cos())
                    + state.apdf
                        * flags.swc[11]
                        * flags.swc[5]
                        * (p[115] * state.plg[1][1]
                            + p[116] * state.plg[1][3]
                            + p[117] * state.plg[1][5])
                        * cd14
                        * (dgtr * (input.g_long - p[118])).cos()
                    + state.apdf
                        * flags.swc[12]
                        * (p[83] * state.plg[0][1]
                            + p[84] * state.plg[0][3]
                            + p[85] * state.plg[0][5])
                        * (sr * (input.sec - p[75])).cos();
            }
        }
    }
    let mut tinf = p[30];
    for i in 0..14 {
        tinf += flags.sw[i + 1].abs() * t[i];
    }
    tinf
}

fn glob7s(p: &[f64], input: &NrlmsiseInput, flags: &NrlmsiseFlags, state: &NrlmsiseState) -> f64 {
    let dr = 1.72142E-2_f64;
    let _hr = 0.2618_f64;
    let mut t = [0.0_f64; 14];
    let cd32 = (dr * (input.doy as f64 - p[31])).cos();
    let cd18 = (2.0 * dr * (input.doy as f64 - p[17])).cos();
    let cd14 = (dr * (input.doy as f64 - p[13])).cos();
    let cd39 = (2.0 * dr * (input.doy as f64 - p[38])).cos();
    t[0] = p[21] * state.dfa;
    t[1] = p[1] * state.plg[0][2]
        + p[2] * state.plg[0][4]
        + p[22] * state.plg[0][6]
        + p[26] * state.plg[0][1]
        + p[14] * state.plg[0][3]
        + p[59] * state.plg[0][5];
    t[2] = (p[18] + p[47] * state.plg[0][2] + p[29] * state.plg[0][4]) * cd32;
    t[3] = (p[15] + p[16] * state.plg[0][2] + p[30] * state.plg[0][4]) * cd18;
    t[4] = (p[9] * state.plg[0][1] + p[10] * state.plg[0][3] + p[20] * state.plg[0][5]) * cd14;
    t[5] = p[37] * state.plg[0][1] * cd39;
    if flags.sw[7] != 0.0 {
        let t71 = p[11] * state.plg[1][2] * cd14 * flags.swc[5];
        let t72 = p[12] * state.plg[1][2] * cd14 * flags.swc[5];
        t[6] = (p[3] * state.plg[1][1] + p[4] * state.plg[1][3] + t71) * state.ctloc
            + (p[6] * state.plg[1][1] + p[7] * state.plg[1][3] + t72) * state.stloc;
    }
    if flags.sw[8] != 0.0 {
        let t81 = (p[23] * state.plg[2][3] + p[35] * state.plg[2][5]) * cd14 * flags.swc[5];
        let t82 = (p[33] * state.plg[2][3] + p[36] * state.plg[2][5]) * cd14 * flags.swc[5];
        t[7] = (p[5] * state.plg[2][2] + p[41] * state.plg[2][4] + t81) * state.c2tloc
            + (p[8] * state.plg[2][2] + p[42] * state.plg[2][4] + t82) * state.s2tloc;
    }
    if flags.sw[14] != 0.0 {
        t[13] = p[39] * state.plg[3][3] * state.s3tloc + p[40] * state.plg[3][3] * state.c3tloc;
    }
    if flags.sw[9] != 0.0 {
        if flags.sw[9] as i32 == 1 {
            t[8] = state.apdf * (p[32] + p[45] * state.plg[0][2] * flags.swc[2]);
        }
        if flags.sw[9] as i32 == -1 {
            t[8] = p[50] * state.apt[0] + p[96] * state.plg[0][2] * state.apt[0] * flags.swc[2];
        }
    }
    if flags.sw[10] != 0.0 && flags.sw[11] != 0.0 && input.g_long > -1000.0 {
        t[10] = (1.0
            + state.plg[0][1]
                * (p[80] * flags.swc[5] * (dr * (input.doy as f64 - p[81])).cos()
                    + p[85] * flags.swc[6] * (2.0 * dr * (input.doy as f64 - p[86])).cos())
            + p[83] * flags.swc[3] * (dr * (input.doy as f64 - p[84])).cos()
            + p[87] * flags.swc[4] * (2.0 * dr * (input.doy as f64 - p[88])).cos())
            * ((p[64] * state.plg[1][2]
                + p[65] * state.plg[1][4]
                + p[66] * state.plg[1][6]
                + p[74] * state.plg[1][1]
                + p[75] * state.plg[1][3]
                + p[76] * state.plg[1][5])
                * state.clong
                + (p[90] * state.plg[1][2]
                    + p[91] * state.plg[1][4]
                    + p[92] * state.plg[1][6]
                    + p[77] * state.plg[1][1]
                    + p[78] * state.plg[1][3]
                    + p[79] * state.plg[1][5])
                    * state.slong);
    }
    let mut tt = 0.0_f64;
    for i in 0..14 {
        tt += flags.sw[i + 1].abs() * t[i];
    }
    tt
}

fn gts7(
    input: &mut NrlmsiseInput,
    flags: &NrlmsiseFlags,
    output: &mut NrlmsiseOutput,
    state: &mut NrlmsiseState,
) {
    let dgtr = 1.74533E-2_f64;
    let dr = 1.72142E-2_f64;
    let alpha = [-0.38, 0.0, 0.0, 0.0, 0.17, 0.0, -0.38, 0.0, 0.0_f64];
    let altl = [200.0, 300.0, 160.0, 250.0, 240.0, 450.0, 320.0, 450.0_f64];
    let za = PDL[1][15];
    let zn1 = [za, 110.0, 100.0, 90.0, 72.5];
    let mn1 = 5_usize;
    for j in 0..9 {
        output.d[j] = 0.0;
    }
    let tinf = if input.alt > zn1[0] {
        PTM[0] * PT[0] * (1.0 + flags.sw[16] * globe7(&PT, input, flags, state))
    } else {
        PTM[0] * PT[0]
    };
    output.t[0] = tinf;
    let g0 = if input.alt > zn1[4] {
        PTM[3] * PS[0] * (1.0 + flags.sw[19] * globe7(&PS, input, flags, state))
    } else {
        PTM[3] * PS[0]
    };
    let tlb = PTM[1] * (1.0 + flags.sw[17] * globe7(&PD[3], input, flags, state)) * PD[3][0];
    let s = g0 / (tinf - tlb);
    if input.alt < 300.0 {
        state.meso_tn1[1] =
            PTM[6] * PTL[0][0] / (1.0 - flags.sw[18] * glob7s(&PTL[0], input, flags, state));
        state.meso_tn1[2] =
            PTM[2] * PTL[1][0] / (1.0 - flags.sw[18] * glob7s(&PTL[1], input, flags, state));
        state.meso_tn1[3] =
            PTM[7] * PTL[2][0] / (1.0 - flags.sw[18] * glob7s(&PTL[2], input, flags, state));
        state.meso_tn1[4] = PTM[4] * PTL[3][0]
            / (1.0 - flags.sw[18] * flags.sw[20] * glob7s(&PTL[3], input, flags, state));
        state.meso_tgn1[1] = PTM[8]
            * PMA[8][0]
            * (1.0 + flags.sw[18] * flags.sw[20] * glob7s(&PMA[8], input, flags, state))
            * state.meso_tn1[4]
            * state.meso_tn1[4]
            / ((PTM[4] * PTL[3][0]) * (PTM[4] * PTL[3][0]));
    } else {
        state.meso_tn1[1] = PTM[6] * PTL[0][0];
        state.meso_tn1[2] = PTM[2] * PTL[1][0];
        state.meso_tn1[3] = PTM[7] * PTL[2][0];
        state.meso_tn1[4] = PTM[4] * PTL[3][0];
        state.meso_tgn1[1] = PTM[8] * PMA[8][0] * state.meso_tn1[4] * state.meso_tn1[4]
            / ((PTM[4] * PTL[3][0]) * (PTM[4] * PTL[3][0]));
    }
    let g28 = flags.sw[21] * globe7(&PD[2], input, flags, state);
    let zhf = PDL[1][24]
        * (1.0
            + flags.sw[5]
                * PDL[0][24]
                * (dgtr * input.g_lat).sin()
                * (dr * (input.doy as f64 - PT[13])).cos());
    output.t[0] = tinf;
    let xmm = PDM[2][4];
    let z = input.alt;
    let db28 = PDM[2][0] * g28.exp() * PD[2][0];
    output.d[2] = densu(
        z,
        db28,
        tinf,
        tlb,
        28.0,
        alpha[2],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[2];
    let zh28 = PDM[2][2] * zhf;
    let zhm28 = PDM[2][3] * PDL[1][5];
    let xmd = 28.0 - xmm;
    let b28 = densu(
        zh28,
        db28,
        tinf,
        tlb,
        xmd,
        alpha[2] - 1.0,
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    if flags.sw[15] != 0.0 && z <= altl[2] {
        state.dm28 = densu(
            z,
            b28,
            tinf,
            tlb,
            xmm,
            alpha[2],
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        output.d[2] = dnet(output.d[2], state.dm28, zhm28, xmm, 28.0);
    }
    let g4 = flags.sw[21] * globe7(&PD[0], input, flags, state);
    let db04 = PDM[0][0] * g4.exp() * PD[0][0];
    output.d[0] = densu(
        z,
        db04,
        tinf,
        tlb,
        4.0,
        alpha[0],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[0];
    if flags.sw[15] != 0.0 && z < altl[0] {
        let zh04 = PDM[0][2];
        let b04 = densu(
            zh04,
            db04,
            tinf,
            tlb,
            4.0 - xmm,
            alpha[0] - 1.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        state.dm04 = densu(
            z,
            b04,
            tinf,
            tlb,
            xmm,
            0.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        output.d[0] = dnet(output.d[0], state.dm04, zhm28, xmm, 4.0);
        let rl = (b28 * PDM[0][1] / b04).ln();
        let zc04 = PDM[0][4] * PDL[1][0];
        let hc04 = PDM[0][5] * PDL[1][1];
        output.d[0] *= ccor(z, rl, hc04, zc04);
    }
    let g16 = flags.sw[21] * globe7(&PD[1], input, flags, state);
    let db16 = PDM[1][0] * g16.exp() * PD[1][0];
    output.d[1] = densu(
        z,
        db16,
        tinf,
        tlb,
        16.0,
        alpha[1],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[1];
    if flags.sw[15] != 0.0 && z <= altl[1] {
        let zh16 = PDM[1][2];
        let b16 = densu(
            zh16,
            db16,
            tinf,
            tlb,
            16.0 - xmm,
            alpha[1] - 1.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        state.dm16 = densu(
            z,
            b16,
            tinf,
            tlb,
            xmm,
            0.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        output.d[1] = dnet(output.d[1], state.dm16, zhm28, xmm, 16.0);
        let rl = PDM[1][1] * PDL[1][16] * (1.0 + flags.sw[1] * PDL[0][23] * (input.f107a - 150.0));
        let hc16 = PDM[1][5] * PDL[1][3];
        let zc16 = PDM[1][4] * PDL[1][2];
        let hc216 = PDM[1][5] * PDL[1][4];
        output.d[1] *= ccor2(z, rl, hc16, zc16, hc216);
        let hcc16 = PDM[1][7] * PDL[1][13];
        let zcc16 = PDM[1][6] * PDL[1][12];
        let rc16 = PDM[1][3] * PDL[1][14];
        output.d[1] *= ccor(z, rc16, hcc16, zcc16);
    }
    let g32 = flags.sw[21] * globe7(&PD[4], input, flags, state);
    let db32 = PDM[3][0] * g32.exp() * PD[4][0];
    output.d[3] = densu(
        z,
        db32,
        tinf,
        tlb,
        32.0,
        alpha[3],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[3];
    if flags.sw[15] != 0.0 {
        if z <= altl[3] {
            let zh32 = PDM[3][2];
            let b32 = densu(
                zh32,
                db32,
                tinf,
                tlb,
                32.0 - xmm,
                alpha[3] - 1.0,
                &mut output.t[1],
                PTM[5],
                s,
                mn1,
                &zn1,
                &mut state.meso_tn1,
                &mut state.meso_tgn1,
                state.gsurf,
                state.re,
            );
            state.dm32 = densu(
                z,
                b32,
                tinf,
                tlb,
                xmm,
                0.0,
                &mut output.t[1],
                PTM[5],
                s,
                mn1,
                &zn1,
                &mut state.meso_tn1,
                &mut state.meso_tgn1,
                state.gsurf,
                state.re,
            );
            output.d[3] = dnet(output.d[3], state.dm32, zhm28, xmm, 32.0);
            let rl = (b28 * PDM[3][1] / b32).ln();
            let hc32 = PDM[3][5] * PDL[1][7];
            let zc32 = PDM[3][4] * PDL[1][6];
            output.d[3] *= ccor(z, rl, hc32, zc32);
        }
        let hcc32 = PDM[3][7] * PDL[1][22];
        let hcc232 = PDM[3][7] * PDL[0][22];
        let zcc32 = PDM[3][6] * PDL[1][21];
        let rc32 =
            PDM[3][3] * PDL[1][23] * (1.0 + flags.sw[1] * PDL[0][23] * (input.f107a - 150.0));
        output.d[3] *= ccor2(z, rc32, hcc32, zcc32, hcc232);
    }
    let g40 = flags.sw[21] * globe7(&PD[5], input, flags, state);
    let db40 = PDM[4][0] * g40.exp() * PD[5][0];
    output.d[4] = densu(
        z,
        db40,
        tinf,
        tlb,
        40.0,
        alpha[4],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[4];
    if flags.sw[15] != 0.0 && z <= altl[4] {
        let zh40 = PDM[4][2];
        let b40 = densu(
            zh40,
            db40,
            tinf,
            tlb,
            40.0 - xmm,
            alpha[4] - 1.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        state.dm40 = densu(
            z,
            b40,
            tinf,
            tlb,
            xmm,
            0.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        output.d[4] = dnet(output.d[4], state.dm40, zhm28, xmm, 40.0);
        let rl = (b28 * PDM[4][1] / b40).ln();
        let hc40 = PDM[4][5] * PDL[1][9];
        let zc40 = PDM[4][4] * PDL[1][8];
        output.d[4] *= ccor(z, rl, hc40, zc40);
    }
    let g1 = flags.sw[21] * globe7(&PD[6], input, flags, state);
    let db01 = PDM[5][0] * g1.exp() * PD[6][0];
    output.d[6] = densu(
        z,
        db01,
        tinf,
        tlb,
        1.0,
        alpha[6],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[6];
    if flags.sw[15] != 0.0 && z <= altl[6] {
        let zh01 = PDM[5][2];
        let b01 = densu(
            zh01,
            db01,
            tinf,
            tlb,
            1.0 - xmm,
            alpha[6] - 1.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        state.dm01 = densu(
            z,
            b01,
            tinf,
            tlb,
            xmm,
            0.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        output.d[6] = dnet(output.d[6], state.dm01, zhm28, xmm, 1.0);
        let rl = (b28 * PDM[5][1] * (PDL[1][17] * PDL[1][17]).sqrt() / b01).ln();
        let hc01 = PDM[5][5] * PDL[1][11];
        let zc01 = PDM[5][4] * PDL[1][10];
        output.d[6] *= ccor(z, rl, hc01, zc01);
        let hcc01 = PDM[5][7] * PDL[1][19];
        let zcc01 = PDM[5][6] * PDL[1][18];
        let rc01 = PDM[5][3] * PDL[1][20];
        output.d[6] *= ccor(z, rc01, hcc01, zcc01);
    }
    let g14 = flags.sw[21] * globe7(&PD[7], input, flags, state);
    let db14 = PDM[6][0] * g14.exp() * PD[7][0];
    output.d[7] = densu(
        z,
        db14,
        tinf,
        tlb,
        14.0,
        alpha[7],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    state.dd = output.d[7];
    if flags.sw[15] != 0.0 && z <= altl[7] {
        let zh14 = PDM[6][2];
        let b14 = densu(
            zh14,
            db14,
            tinf,
            tlb,
            14.0 - xmm,
            alpha[7] - 1.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        state.dm14 = densu(
            z,
            b14,
            tinf,
            tlb,
            xmm,
            0.0,
            &mut output.t[1],
            PTM[5],
            s,
            mn1,
            &zn1,
            &mut state.meso_tn1,
            &mut state.meso_tgn1,
            state.gsurf,
            state.re,
        );
        output.d[7] = dnet(output.d[7], state.dm14, zhm28, xmm, 14.0);
        let rl = (b28 * PDM[6][1] * (PDL[0][2] * PDL[0][2]).sqrt() / b14).ln();
        let hc14 = PDM[6][5] * PDL[0][1];
        let zc14 = PDM[6][4] * PDL[0][0];
        output.d[7] *= ccor(z, rl, hc14, zc14);
        let hcc14 = PDM[6][7] * PDL[0][4];
        let zcc14 = PDM[6][6] * PDL[0][3];
        let rc14 = PDM[6][3] * PDL[0][5];
        output.d[7] *= ccor(z, rc14, hcc14, zcc14);
    }
    let g16h = flags.sw[21] * globe7(&PD[8], input, flags, state);
    let db16h = PDM[7][0] * g16h.exp() * PD[8][0];
    let tho = PDM[7][9] * PDL[0][6];
    let dd_anom = densu(
        z,
        db16h,
        tho,
        tho,
        16.0,
        alpha[8],
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    let zsht = PDM[7][5];
    let zmho = PDM[7][4];
    let zsho = scalh(zmho, 16.0, tho, state.gsurf, state.re);
    output.d[8] = dd_anom * (-zsht / zsho * ((-(z - zmho) / zsht).exp() - 1.0)).exp();
    output.d[5] = 1.66E-24
        * (4.0 * output.d[0]
            + 16.0 * output.d[1]
            + 28.0 * output.d[2]
            + 32.0 * output.d[3]
            + 40.0 * output.d[4]
            + output.d[6]
            + 14.0 * output.d[7]);
    let z_abs = (input.alt * input.alt).sqrt();
    let _ddum = densu(
        z_abs,
        1.0,
        tinf,
        tlb,
        0.0,
        0.0,
        &mut output.t[1],
        PTM[5],
        s,
        mn1,
        &zn1,
        &mut state.meso_tn1,
        &mut state.meso_tgn1,
        state.gsurf,
        state.re,
    );
    if flags.sw[0] != 0.0 {
        for i in 0..9 {
            output.d[i] *= 1.0E6;
        }
        output.d[5] /= 1000.0;
    }
}

fn gtd7(
    input: &mut NrlmsiseInput,
    flags: &mut NrlmsiseFlags,
    output: &mut NrlmsiseOutput,
    state: &mut NrlmsiseState,
) {
    let mn3 = 5_usize;
    let zn3 = [32.5, 20.0, 15.0, 10.0, 0.0_f64];
    let mn2 = 4_usize;
    let zn2 = [72.5, 55.0, 45.0, 32.5_f64];
    let zmix = 62.5_f64;
    tselec(flags);
    globe_setup(input, flags, state);
    let xlat = if flags.sw[2] == 0.0 {
        45.0
    } else {
        input.g_lat
    };
    glatf(xlat, &mut state.gsurf, &mut state.re);
    let xmm = PDM[2][4];
    let altt = if input.alt > zn2[0] {
        input.alt
    } else {
        zn2[0]
    };
    let tmp = input.alt;
    input.alt = altt;
    let mut soutput = NrlmsiseOutput {
        d: [0.0; 9],
        t: [0.0; 2],
    };
    gts7(input, flags, &mut soutput, state);
    input.alt = tmp;
    let dm28m = if flags.sw[0] != 0.0 {
        state.dm28 * 1.0E6
    } else {
        state.dm28
    };
    output.t[0] = soutput.t[0];
    output.t[1] = soutput.t[1];
    if input.alt >= zn2[0] {
        for i in 0..9 {
            output.d[i] = soutput.d[i];
        }
        return;
    }
    state.meso_tgn2[0] = state.meso_tgn1[1];
    state.meso_tn2[0] = state.meso_tn1[4];
    state.meso_tn2[1] =
        PMA[0][0] * PAVGM[0] / (1.0 - flags.sw[20] * glob7s(&PMA[0], input, flags, state));
    state.meso_tn2[2] =
        PMA[1][0] * PAVGM[1] / (1.0 - flags.sw[20] * glob7s(&PMA[1], input, flags, state));
    state.meso_tn2[3] = PMA[2][0] * PAVGM[2]
        / (1.0 - flags.sw[20] * flags.sw[22] * glob7s(&PMA[2], input, flags, state));
    state.meso_tgn2[1] = PAVGM[8]
        * PMA[9][0]
        * (1.0 + flags.sw[20] * flags.sw[22] * glob7s(&PMA[9], input, flags, state))
        * state.meso_tn2[3]
        * state.meso_tn2[3]
        / ((PMA[2][0] * PAVGM[2]) * (PMA[2][0] * PAVGM[2]));
    state.meso_tn3[0] = state.meso_tn2[3];
    if input.alt <= zn3[0] {
        state.meso_tgn3[0] = state.meso_tgn2[1];
        state.meso_tn3[1] =
            PMA[3][0] * PAVGM[3] / (1.0 - flags.sw[22] * glob7s(&PMA[3], input, flags, state));
        state.meso_tn3[2] =
            PMA[4][0] * PAVGM[4] / (1.0 - flags.sw[22] * glob7s(&PMA[4], input, flags, state));
        state.meso_tn3[3] =
            PMA[5][0] * PAVGM[5] / (1.0 - flags.sw[22] * glob7s(&PMA[5], input, flags, state));
        state.meso_tn3[4] =
            PMA[6][0] * PAVGM[6] / (1.0 - flags.sw[22] * glob7s(&PMA[6], input, flags, state));
        state.meso_tgn3[1] = PMA[7][0]
            * PAVGM[7]
            * (1.0 + flags.sw[22] * glob7s(&PMA[7], input, flags, state))
            * state.meso_tn3[4]
            * state.meso_tn3[4]
            / ((PMA[6][0] * PAVGM[6]) * (PMA[6][0] * PAVGM[6]));
    }
    let dmc = if input.alt > zmix {
        1.0 - (zn2[0] - input.alt) / (zn2[0] - zmix)
    } else {
        0.0
    };
    let dz28 = soutput.d[2];
    let mut tz = 0.0_f64;
    let dmr = soutput.d[2] / dm28m - 1.0;
    output.d[2] = densm(
        input.alt,
        dm28m,
        xmm,
        &mut tz,
        mn3,
        &zn3,
        &state.meso_tn3,
        &state.meso_tgn3,
        mn2,
        &zn2,
        &state.meso_tn2,
        &state.meso_tgn2,
        state.gsurf,
        state.re,
    );
    output.d[2] *= 1.0 + dmr * dmc;
    let dmr = soutput.d[0] / (dz28 * PDM[0][1]) - 1.0;
    output.d[0] = output.d[2] * PDM[0][1] * (1.0 + dmr * dmc);
    output.d[1] = 0.0;
    output.d[8] = 0.0;
    let dmr = soutput.d[3] / (dz28 * PDM[3][1]) - 1.0;
    output.d[3] = output.d[2] * PDM[3][1] * (1.0 + dmr * dmc);
    let dmr = soutput.d[4] / (dz28 * PDM[4][1]) - 1.0;
    output.d[4] = output.d[2] * PDM[4][1] * (1.0 + dmr * dmc);
    output.d[6] = 0.0;
    output.d[7] = 0.0;
    output.d[5] = 1.66E-24
        * (4.0 * output.d[0]
            + 16.0 * output.d[1]
            + 28.0 * output.d[2]
            + 32.0 * output.d[3]
            + 40.0 * output.d[4]
            + output.d[6]
            + 14.0 * output.d[7]);
    if flags.sw[0] != 0.0 {
        output.d[5] /= 1000.0;
    }
    let _dd = densm(
        input.alt,
        1.0,
        0.0,
        &mut tz,
        mn3,
        &zn3,
        &state.meso_tn3,
        &state.meso_tgn3,
        mn2,
        &zn2,
        &state.meso_tn2,
        &state.meso_tgn2,
        state.gsurf,
        state.re,
    );
    output.t[1] = tz;
}

pub(super) fn evaluate(
    doy: u16,
    sec: f64,
    alt_km: f64,
    lat_deg: f64,
    lon_deg: f64,
    lst: f64,
    f107a: f64,
    f107: f64,
    ap: f64,
    ap_a: Option<[f64; 7]>,
) -> ([f64; 9], [f64; 2]) {
    let mut input = NrlmsiseInput {
        doy: i32::from(doy),
        sec,
        alt: alt_km,
        g_lat: lat_deg,
        g_long: lon_deg,
        lst,
        f107a,
        f107,
        ap,
        ap_a,
    };
    let mut flags = NrlmsiseFlags {
        switches: [1; 24],
        sw: [0.0; 24],
        swc: [0.0; 24],
    };
    flags.switches[0] = 0; // native CGS; the public adapter converts to SI.
    if input.ap_a.is_some() {
        flags.switches[9] = -1;
    }
    let mut output = NrlmsiseOutput {
        d: [0.0; 9],
        t: [0.0; 2],
    };
    gtd7(
        &mut input,
        &mut flags,
        &mut output,
        &mut NrlmsiseState::new(),
    );
    (output.d, output.t)
}
