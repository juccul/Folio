//! Pure-Rust statistical distributions and reference table solvers matching standard
//! university statistics tables:
//! - Standard & general Normal ($\Phi(z)$, $\varphi(z)$, $\Phi^{-1}(p)$, $z_\alpha$, $N(\mu, \sigma^2)$)
//! - Student's $t$-distribution ($t_{\text{df}, \alpha}$, $P(T \ge t)$, $P(T \le t)$, including $\text{df} = \infty$)
//! - Chi-square distribution ($\chi^2_{\text{df}, \alpha}$, $P(\chi^2 \ge c)$)
//! - $F$-distribution ($F_{\alpha}(f_1, f_2)$, $P(F \ge c)$, including $f_1 = \infty$ or $f_2 = \infty$)
//! - Shapiro-Wilk normality test (weights $a_{n-i+1}$, critical values $W_{n, \alpha}$, statistic $W$, p-value)
//! - Binomial distribution ($B(n, p)$ PMF, CDF, $\binom{n}{k}$, mean, variance)
//! - Poisson distribution ($\text{Pois}(\mu)$ PMF, CDF, mean, variance)
use std::f64::consts::{FRAC_1_SQRT_2, PI, SQRT_2};

const SQRT_2PI: f64 = 2.5066282746310002; // sqrt(2 * pi)

/// Supported probability distributions for `X ~ ...`, `P(...)`, `E(...)`, and `Var(...)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Distribution {
    /// Normal distribution parameterized by mean `mu` and standard deviation `sigma > 0`.
    /// Note: `N(mu, variance)` in textbook notation passes `sigma = variance.sqrt()`.
    Normal { mu: f64, sigma: f64 },
    /// Student's t-distribution with `df > 0` (may be `+inf`, which equals `N(0, 1)`).
    StudentT { df: f64 },
    /// Chi-square distribution with `df > 0`.
    ChiSquare { df: f64 },
    /// F-distribution with numerator `d1 > 0` and denominator `d2 > 0` (either may be `+inf`).
    FDist { d1: f64, d2: f64 },
    /// Binomial distribution `B(n, p)` with `n >= 1`, `0 <= p <= 1`.
    Binomial { n: u64, p: f64 },
    /// Poisson distribution `Pois(mu)` with `mu > 0`.
    Poisson { mu: f64 },
    /// Shapiro-Wilk `W` statistic under normality for sample size `n >= 3`.
    ShapiroWilk { n: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

impl Distribution {
    pub fn is_discrete(&self) -> bool {
        matches!(self, Self::Binomial { .. } | Self::Poisson { .. })
    }

    pub fn cdf(&self, x: f64) -> Option<f64> {
        if !x.is_finite() {
            return None;
        }
        match *self {
            Self::Normal { mu, sigma } if sigma > 0.0 => Some(normal_cdf((x - mu) / sigma)),
            Self::StudentT { df } => t_cdf(x, df),
            Self::ChiSquare { df } => chi2_cdf(x, df),
            Self::FDist { d1, d2 } => f_cdf(x, d1, d2),
            Self::Binomial { n, p } => binom_cdf(n, p, x.floor() as i64),
            Self::Poisson { mu } => poisson_cdf(mu, x.floor() as i64),
            Self::ShapiroWilk { n } => shapiro_wilk_cdf(n, x),
            _ => None,
        }
    }

    pub fn pmf(&self, x: f64) -> Option<f64> {
        if !x.is_finite() {
            return None;
        }
        if !self.is_discrete() {
            // For continuous distributions, P(X = x) = 0.
            return Some(0.0);
        }
        let k = x.round();
        if (x - k).abs() > 1e-9 {
            return Some(0.0);
        }
        match *self {
            Self::Binomial { n, p } => binom_pmf(n, p, k as i64),
            Self::Poisson { mu } => poisson_pmf(mu, k as i64),
            _ => None,
        }
    }

    pub fn prob_single(&self, op: RelOp, bound: f64) -> Option<f64> {
        if !bound.is_finite() {
            return None;
        }
        if self.is_discrete() {
            let eps = 1e-9;
            match op {
                RelOp::Le => self.cdf((bound + eps).floor()),
                RelOp::Lt => self.cdf((bound - eps).ceil() - 1.0),
                RelOp::Ge => Some((1.0 - self.cdf((bound - eps).ceil() - 1.0)?).clamp(0.0, 1.0)),
                RelOp::Gt => Some((1.0 - self.cdf((bound + eps).floor())?).clamp(0.0, 1.0)),
                RelOp::Eq => self.pmf(bound),
            }
        } else {
            match op {
                RelOp::Le | RelOp::Lt => self.cdf(bound),
                RelOp::Ge | RelOp::Gt => Some((1.0 - self.cdf(bound)?).clamp(0.0, 1.0)),
                RelOp::Eq => Some(0.0),
            }
        }
    }

    pub fn prob_interval(&self, lo: f64, lo_inclusive: bool, hi: f64, hi_inclusive: bool) -> Option<f64> {
        if !lo.is_finite() || !hi.is_finite() {
            return None;
        }
        if lo > hi || (lo == hi && (!lo_inclusive || !hi_inclusive)) {
            return Some(0.0);
        }
        let p_hi = self.prob_single(if hi_inclusive { RelOp::Le } else { RelOp::Lt }, hi)?;
        let p_below_lo = self.prob_single(if lo_inclusive { RelOp::Lt } else { RelOp::Le }, lo)?;
        Some((p_hi - p_below_lo).clamp(0.0, 1.0))
    }

    pub fn prob_abs(&self, op: RelOp, bound: f64) -> Option<f64> {
        if !bound.is_finite() {
            return None;
        }
        match op {
            RelOp::Le => {
                if bound < 0.0 {
                    Some(0.0)
                } else {
                    self.prob_interval(-bound, true, bound, true)
                }
            }
            RelOp::Lt => {
                if bound <= 0.0 {
                    Some(0.0)
                } else {
                    self.prob_interval(-bound, false, bound, false)
                }
            }
            RelOp::Ge => {
                if bound <= 0.0 {
                    Some(1.0)
                } else {
                    Some((1.0 - self.prob_interval(-bound, false, bound, false)?).clamp(0.0, 1.0))
                }
            }
            RelOp::Gt => {
                if bound < 0.0 {
                    Some(1.0)
                } else {
                    Some((1.0 - self.prob_interval(-bound, true, bound, true)?).clamp(0.0, 1.0))
                }
            }
            RelOp::Eq => {
                if bound < 0.0 {
                    Some(0.0)
                } else if bound == 0.0 {
                    self.pmf(0.0)
                } else {
                    Some((self.pmf(bound)? + self.pmf(-bound)?).clamp(0.0, 1.0))
                }
            }
        }
    }

    pub fn mean(&self) -> Option<f64> {
        match *self {
            Self::Normal { mu, .. } => Some(mu),
            Self::StudentT { df } if df > 1.0 => Some(0.0),
            Self::ChiSquare { df } if df > 0.0 => Some(df),
            Self::FDist { d2, .. } if d2.is_infinite() => Some(1.0),
            Self::FDist { d2, .. } if d2 > 2.0 => Some(d2 / (d2 - 2.0)),
            Self::Binomial { n, p } => Some(n as f64 * p),
            Self::Poisson { mu } => Some(mu),
            _ => None,
        }
    }

    pub fn variance(&self) -> Option<f64> {
        match *self {
            Self::Normal { sigma, .. } => Some(sigma * sigma),
            Self::StudentT { df } if df.is_infinite() => Some(1.0),
            Self::StudentT { df } if df > 2.0 => Some(df / (df - 2.0)),
            Self::ChiSquare { df } if df > 0.0 => Some(2.0 * df),
            Self::FDist { d1, d2 } if d1.is_finite() && d2 > 4.0 && d2.is_finite() => {
                Some(2.0 * d2 * d2 * (d1 + d2 - 2.0) / (d1 * (d2 - 2.0).powi(2) * (d2 - 4.0)))
            }
            Self::FDist { d1, d2 } if d1.is_finite() && d2.is_infinite() => Some(2.0 / d1),
            Self::Binomial { n, p } => Some(n as f64 * p * (1.0 - p)),
            Self::Poisson { mu } => Some(mu),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Core special functions (pure Rust, ~1e-13 accuracy)
// ---------------------------------------------------------------------------

/// Lanczos log-gamma function `ln(Gamma(z))` for `z > 0`.
pub fn ln_gamma(z: f64) -> f64 {
    const G: f64 = 7.0;
    const COEFFS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if z < 0.5 {
        return PI.ln() - (PI * z).sin().ln() - ln_gamma(1.0 - z);
    }
    let z = z - 1.0;
    let mut x = COEFFS[0];
    for (i, &c) in COEFFS.iter().enumerate().skip(1) {
        x += c / (z + i as f64);
    }
    let t = z + G + 0.5;
    0.5 * (2.0 * PI).ln() + (z + 0.5) * t.ln() - t + x.ln()
}

/// Complementary error function `erfc(x)` via the exact incomplete gamma identity
/// `erfc(x) = Q(1/2, x^2)` for `x >= 0` (`1 + P(1/2, x^2)` for `x < 0`).
pub fn erfc(x: f64) -> f64 {
    if x >= 0.0 {
        reg_gamma_q(0.5, x * x)
    } else {
        1.0 + reg_gamma_p(0.5, x * x)
    }
}

/// Regularized lower incomplete gamma function `P(a, x) = gamma(a, x) / Gamma(a)`.
pub fn reg_gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        gamma_series(a, x)
    } else {
        1.0 - gamma_cf(a, x)
    }
}

/// Regularized upper incomplete gamma function `Q(a, x) = Gamma(a, x) / Gamma(a)`.
pub fn reg_gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        1.0 - gamma_series(a, x)
    } else {
        gamma_cf(a, x)
    }
}

fn gamma_series(a: f64, x: f64) -> f64 {
    let gln = ln_gamma(a);
    let mut ap = a;
    let mut term = 1.0 / a;
    let mut sum = term;
    for _ in 0..400 {
        ap += 1.0;
        term *= x / ap;
        sum += term;
        if term.abs() < sum.abs() * 1e-14 {
            break;
        }
    }
    (sum * (-x + a * x.ln() - gln).exp()).clamp(0.0, 1.0)
}

fn gamma_cf(a: f64, x: f64) -> f64 {
    const FPMIN: f64 = 1e-300;
    let gln = ln_gamma(a);
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / FPMIN;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=400 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = b + an / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    ((-x + a * x.ln() - gln).exp() * h).clamp(0.0, 1.0)
}

/// Regularized incomplete beta function `I_x(a, b)` for `0 <= x <= 1`, `a > 0`, `b > 0`.
pub fn reg_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        (bt * beta_cf(a, b, x) / a).clamp(0.0, 1.0)
    } else {
        (1.0 - bt * beta_cf(b, a, 1.0 - x) / b).clamp(0.0, 1.0)
    }
}

fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    const FPMIN: f64 = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=400 {
        let mf = m as f64;
        let m2 = 2.0 * mf;
        let aa = mf * (b - mf) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + mf) * (qab + mf) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    h
}

/// Invert `reg_beta(a, b, x) = p` for `x in (0, 1)` via safeguarded Newton + bisection.
fn inv_reg_beta(a: f64, b: f64, p: f64) -> Option<f64> {
    if !(a > 0.0 && b > 0.0 && p > 0.0 && p < 1.0) {
        return None;
    }
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    let mut x = (a / (a + b)).clamp(1e-12, 1.0 - 1e-12);
    let log_beta = ln_gamma(a) + ln_gamma(b) - ln_gamma(a + b);
    for _ in 0..80 {
        let fx = reg_beta(a, b, x) - p;
        if fx.abs() < 1e-13 {
            return Some(x);
        }
        if fx < 0.0 {
            lo = x;
        } else {
            hi = x;
        }
        if (hi - lo) <= 1e-14 * x.max(1e-12) {
            return Some(0.5 * (lo + hi));
        }
        let log_pdf = (a - 1.0) * x.ln() + (b - 1.0) * (1.0 - x).ln() - log_beta;
        let dfx = log_pdf.exp();
        let next = if dfx.is_finite() && dfx > 1e-300 {
            let cand = x - fx / dfx;
            if cand > lo && cand < hi {
                cand
            } else {
                0.5 * (lo + hi)
            }
        } else {
            0.5 * (lo + hi)
        };
        x = next;
    }
    Some(0.5 * (lo + hi))
}

// ---------------------------------------------------------------------------
// Tab-1: Standard Normal Distribution N(0, 1)
// ---------------------------------------------------------------------------

/// Standard normal CDF `Phi(z) = P(Z <= z)`.
pub fn normal_cdf(z: f64) -> f64 {
    0.5 * erfc(-z * FRAC_1_SQRT_2)
}

/// Standard normal PDF `varphi(z) = (1 / sqrt(2*pi)) * exp(-z^2 / 2)`.
pub fn normal_pdf(z: f64) -> f64 {
    (-0.5 * z * z).exp() / SQRT_2PI
}

/// Standard normal quantile `Phi^{-1}(p)` for `0 < p < 1` (Acklam + Halley refinement).
pub fn normal_inv_cdf(p: f64) -> Option<f64> {
    if !(p > 0.0 && p < 1.0) {
        return None;
    }
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_690e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const P_LOW: f64 = 0.02425;
    let mut x = if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    // Two Halley refinement steps for full f64 precision.
    for _ in 0..2 {
        let err = normal_cdf(x) - p;
        let pdf = normal_pdf(x);
        if pdf > 1e-300 {
            let u = err / pdf;
            x -= u / (1.0 + 0.5 * x * u);
        }
    }
    x.is_finite().then_some(x)
}

/// Upper-tail critical value `z_alpha` such that `P(Z >= z_alpha) = alpha`.
pub fn normal_upper_crit(alpha: f64) -> Option<f64> {
    normal_inv_cdf(1.0 - alpha)
}

// ---------------------------------------------------------------------------
// Tab-2: Student's t-Distribution
// ---------------------------------------------------------------------------

/// Student's t CDF `P(T_df <= t)` (supports `df = +inf` as `N(0, 1)`).
pub fn t_cdf(t: f64, df: f64) -> Option<f64> {
    if !(df > 0.0) || !t.is_finite() {
        return None;
    }
    if df.is_infinite() {
        return Some(normal_cdf(t));
    }
    let x = df / (df + t * t);
    let tail = 0.5 * reg_beta(0.5 * df, 0.5, x);
    Some(if t >= 0.0 { 1.0 - tail } else { tail })
}

/// Student's t upper-tail critical value `t` such that `P(T_df >= t) = alpha` (Tab-2).
pub fn t_upper_crit(df: f64, alpha: f64) -> Option<f64> {
    if !(df > 0.0) || !(alpha > 0.0 && alpha < 1.0) {
        return None;
    }
    if df.is_infinite() {
        return normal_upper_crit(alpha);
    }
    if (alpha - 0.5).abs() < 1e-15 {
        return Some(0.0);
    }
    let sign = if alpha < 0.5 { 1.0 } else { -1.0 };
    let two_tail = 2.0 * alpha.min(1.0 - alpha);
    let u = inv_reg_beta(0.5 * df, 0.5, two_tail)?;
    let t = (df * (1.0 - u) / u).sqrt();
    (sign * t).is_finite().then_some(sign * t)
}

// ---------------------------------------------------------------------------
// Tab-3: Chi-Square Distribution
// ---------------------------------------------------------------------------

/// Chi-square CDF `P(chi^2_df <= x)`.
pub fn chi2_cdf(x: f64, df: f64) -> Option<f64> {
    if !(df > 0.0 && df.is_finite()) || !x.is_finite() {
        return None;
    }
    if x <= 0.0 {
        return Some(0.0);
    }
    Some(reg_gamma_p(0.5 * df, 0.5 * x))
}

/// Chi-square upper-tail critical value `c` such that `P(chi^2_df >= c) = alpha` (Tab-3).
pub fn chi2_upper_crit(df: f64, alpha: f64) -> Option<f64> {
    if !(df > 0.0 && df.is_finite()) || !(alpha > 0.0 && alpha < 1.0) {
        return None;
    }
    let a = 0.5 * df;
    let p_lower = 1.0 - alpha;
    // Wilson-Hilferty initial guess.
    let z = normal_inv_cdf(p_lower)?;
    let h = 2.0 / (9.0 * df);
    let wh = df * (1.0 - h + z * h.sqrt()).max(1e-3).powi(3);
    let mut y = (0.5 * wh).max(1e-12); // y = c / 2
    let mut lo = 0.0_f64;
    let mut hi = y * 4.0 + 10.0;
    while reg_gamma_p(a, hi) < p_lower {
        hi *= 2.0;
        if hi > 1e12 {
            return None;
        }
    }
    let gln = ln_gamma(a);
    for _ in 0..80 {
        let fy = reg_gamma_p(a, y) - p_lower;
        if fy.abs() < 1e-13 {
            return Some(2.0 * y);
        }
        if fy < 0.0 {
            lo = y;
        } else {
            hi = y;
        }
        if hi - lo <= 1e-13 * y.max(1.0) {
            return Some(lo + hi);
        }
        let log_pdf = (a - 1.0) * y.ln() - y - gln;
        let dfy = log_pdf.exp();
        let next = if dfy.is_finite() && dfy > 1e-300 {
            let cand = y - fy / dfy;
            if cand > lo && cand < hi {
                cand
            } else {
                0.5 * (lo + hi)
            }
        } else {
            0.5 * (lo + hi)
        };
        y = next;
    }
    Some(lo + hi)
}

// ---------------------------------------------------------------------------
// Tab-4..7: F-Distribution
// ---------------------------------------------------------------------------

/// F-distribution CDF `P(F_{d1, d2} <= x)` (supports `d1 = +inf` and/or `d2 = +inf`).
pub fn f_cdf(x: f64, d1: f64, d2: f64) -> Option<f64> {
    if !(d1 > 0.0 && d2 > 0.0) || !x.is_finite() {
        return None;
    }
    if x <= 0.0 {
        return Some(0.0);
    }
    match (d1.is_infinite(), d2.is_infinite()) {
        (true, true) => Some(if x >= 1.0 { 1.0 } else { 0.0 }),
        (false, true) => chi2_cdf(d1 * x, d1),
        (true, false) => Some(1.0 - chi2_cdf(d2 / x, d2)?),
        (false, false) => {
            let u = d2 / (d2 + d1 * x);
            Some(1.0 - reg_beta(0.5 * d2, 0.5 * d1, u))
        }
    }
}

/// F-distribution upper-tail critical value `c` such that `P(F_{d1, d2} >= c) = alpha` (Tabs 4-7).
pub fn f_upper_crit(d1: f64, d2: f64, alpha: f64) -> Option<f64> {
    if !(d1 > 0.0 && d2 > 0.0) || !(alpha > 0.0 && alpha < 1.0) {
        return None;
    }
    match (d1.is_infinite(), d2.is_infinite()) {
        (true, true) => Some(1.0),
        (false, true) => Some(chi2_upper_crit(d1, alpha)? / d1),
        (true, false) => Some(d2 / chi2_upper_crit(d2, 1.0 - alpha)?),
        (false, false) => {
            let u = inv_reg_beta(0.5 * d2, 0.5 * d1, alpha)?;
            let c = d2 * (1.0 - u) / (d1 * u);
            c.is_finite().then_some(c)
        }
    }
}

// ---------------------------------------------------------------------------
// Tab-8, Tab-9, Tab-10: Shapiro-Wilk Test for Normality
// ---------------------------------------------------------------------------

const SW_CRIT_ALPHAS: [f64; 9] = [0.01, 0.02, 0.05, 0.10, 0.50, 0.90, 0.95, 0.98, 0.99];

/// Exact Shapiro-Wilk critical values `c` such that `P(W <= c | normal) = alpha` (Tab-10, n = 3..=50).
#[rustfmt::skip]
const SW_CRIT_TABLE: [[f64; 9]; 48] = [
    [0.753, 0.756, 0.767, 0.789, 0.959, 0.998, 0.999, 1.000, 1.000], // n=3
    [0.687, 0.707, 0.748, 0.792, 0.935, 0.987, 0.992, 0.996, 0.997], // n=4
    [0.686, 0.715, 0.762, 0.806, 0.927, 0.979, 0.986, 0.991, 0.993], // n=5
    [0.713, 0.743, 0.788, 0.826, 0.927, 0.974, 0.981, 0.986, 0.989], // n=6
    [0.730, 0.760, 0.803, 0.838, 0.928, 0.972, 0.979, 0.985, 0.988], // n=7
    [0.749, 0.778, 0.818, 0.851, 0.932, 0.972, 0.978, 0.984, 0.987], // n=8
    [0.764, 0.791, 0.829, 0.859, 0.935, 0.972, 0.978, 0.984, 0.986], // n=9
    [0.781, 0.806, 0.842, 0.869, 0.938, 0.972, 0.978, 0.983, 0.986], // n=10
    [0.792, 0.817, 0.850, 0.876, 0.940, 0.973, 0.979, 0.984, 0.986], // n=11
    [0.805, 0.828, 0.859, 0.883, 0.943, 0.973, 0.979, 0.984, 0.986], // n=12
    [0.814, 0.837, 0.866, 0.889, 0.945, 0.974, 0.979, 0.984, 0.986], // n=13
    [0.825, 0.846, 0.874, 0.895, 0.947, 0.975, 0.980, 0.984, 0.986], // n=14
    [0.835, 0.855, 0.881, 0.901, 0.950, 0.975, 0.980, 0.984, 0.987], // n=15
    [0.844, 0.863, 0.887, 0.906, 0.952, 0.976, 0.981, 0.985, 0.987], // n=16
    [0.851, 0.869, 0.892, 0.910, 0.954, 0.977, 0.981, 0.985, 0.987], // n=17
    [0.858, 0.874, 0.897, 0.914, 0.956, 0.978, 0.982, 0.986, 0.988], // n=18
    [0.863, 0.879, 0.901, 0.917, 0.957, 0.978, 0.982, 0.986, 0.988], // n=19
    [0.868, 0.884, 0.905, 0.920, 0.959, 0.979, 0.983, 0.986, 0.988], // n=20
    [0.873, 0.888, 0.908, 0.923, 0.960, 0.980, 0.983, 0.987, 0.989], // n=21
    [0.878, 0.892, 0.911, 0.926, 0.961, 0.980, 0.984, 0.987, 0.989], // n=22
    [0.881, 0.895, 0.914, 0.928, 0.962, 0.981, 0.984, 0.987, 0.989], // n=23
    [0.884, 0.898, 0.916, 0.930, 0.963, 0.981, 0.984, 0.987, 0.989], // n=24
    [0.888, 0.901, 0.918, 0.931, 0.964, 0.981, 0.985, 0.988, 0.989], // n=25
    [0.891, 0.904, 0.920, 0.933, 0.965, 0.982, 0.985, 0.988, 0.989], // n=26
    [0.894, 0.906, 0.923, 0.935, 0.965, 0.982, 0.985, 0.988, 0.990], // n=27
    [0.896, 0.908, 0.924, 0.936, 0.966, 0.982, 0.985, 0.988, 0.990], // n=28
    [0.898, 0.910, 0.926, 0.937, 0.966, 0.982, 0.985, 0.988, 0.990], // n=29
    [0.900, 0.912, 0.927, 0.939, 0.967, 0.983, 0.985, 0.988, 0.990], // n=30
    [0.902, 0.914, 0.929, 0.940, 0.967, 0.983, 0.986, 0.988, 0.990], // n=31
    [0.904, 0.915, 0.930, 0.941, 0.968, 0.983, 0.986, 0.988, 0.990], // n=32
    [0.906, 0.917, 0.931, 0.942, 0.968, 0.983, 0.986, 0.989, 0.990], // n=33
    [0.908, 0.919, 0.933, 0.943, 0.969, 0.983, 0.986, 0.989, 0.990], // n=34
    [0.910, 0.920, 0.934, 0.944, 0.969, 0.984, 0.986, 0.989, 0.990], // n=35
    [0.912, 0.922, 0.935, 0.945, 0.970, 0.984, 0.986, 0.989, 0.990], // n=36
    [0.914, 0.924, 0.936, 0.946, 0.970, 0.984, 0.987, 0.989, 0.990], // n=37
    [0.916, 0.925, 0.938, 0.947, 0.971, 0.984, 0.987, 0.989, 0.990], // n=38
    [0.917, 0.927, 0.939, 0.948, 0.971, 0.984, 0.987, 0.989, 0.991], // n=39
    [0.919, 0.928, 0.940, 0.949, 0.972, 0.985, 0.987, 0.989, 0.991], // n=40
    [0.920, 0.929, 0.941, 0.950, 0.972, 0.985, 0.987, 0.989, 0.991], // n=41
    [0.922, 0.930, 0.942, 0.951, 0.972, 0.985, 0.987, 0.989, 0.991], // n=42
    [0.923, 0.932, 0.943, 0.951, 0.973, 0.985, 0.987, 0.990, 0.991], // n=43
    [0.924, 0.933, 0.944, 0.952, 0.973, 0.985, 0.987, 0.990, 0.991], // n=44
    [0.926, 0.934, 0.945, 0.953, 0.973, 0.985, 0.988, 0.990, 0.991], // n=45
    [0.927, 0.935, 0.945, 0.953, 0.974, 0.985, 0.988, 0.990, 0.991], // n=46
    [0.928, 0.936, 0.946, 0.954, 0.974, 0.985, 0.988, 0.990, 0.991], // n=47
    [0.929, 0.937, 0.947, 0.954, 0.974, 0.985, 0.988, 0.990, 0.991], // n=48
    [0.929, 0.937, 0.947, 0.955, 0.974, 0.985, 0.988, 0.990, 0.991], // n=49
    [0.930, 0.938, 0.947, 0.955, 0.974, 0.985, 0.988, 0.990, 0.991], // n=50
];

/// Exact Shapiro-Wilk coefficients `a_{n-i+1}` for `n = 2..=50`, `i = 1..=n/2` (Tabs 8-9).
fn sw_table_weights(n: usize) -> Option<&'static [f64]> {
    let slice: &'static [f64] = match n {
        2 => &[0.7071],
        3 => &[0.7071],
        4 => &[0.6872, 0.1677],
        5 => &[0.6646, 0.2413],
        6 => &[0.6431, 0.2806, 0.0875],
        7 => &[0.6233, 0.3031, 0.1401],
        8 => &[0.6052, 0.3164, 0.1743, 0.0561],
        9 => &[0.5888, 0.3244, 0.1976, 0.0947],
        10 => &[0.5739, 0.3291, 0.2141, 0.1224, 0.0399],
        11 => &[0.5601, 0.3315, 0.2260, 0.1429, 0.0695],
        12 => &[0.5475, 0.3325, 0.2347, 0.1586, 0.0922, 0.0303],
        13 => &[0.5359, 0.3325, 0.2412, 0.1707, 0.1099, 0.0539],
        14 => &[0.5251, 0.3318, 0.2460, 0.1802, 0.1240, 0.0727, 0.0240],
        15 => &[0.5150, 0.3306, 0.2495, 0.1878, 0.1353, 0.0880, 0.0433],
        16 => &[0.5056, 0.3290, 0.2521, 0.1939, 0.1447, 0.1005, 0.0593, 0.0196],
        17 => &[0.4968, 0.3273, 0.2540, 0.1988, 0.1524, 0.1109, 0.0725, 0.0359],
        18 => &[0.4886, 0.3253, 0.2553, 0.2027, 0.1587, 0.1197, 0.0837, 0.0496, 0.0163],
        19 => &[0.4808, 0.3232, 0.2561, 0.2059, 0.1641, 0.1271, 0.0932, 0.0612, 0.0303],
        20 => &[0.4734, 0.3211, 0.2565, 0.2085, 0.1686, 0.1334, 0.1013, 0.0711, 0.0422, 0.0140],
        21 => &[0.4643, 0.3185, 0.2578, 0.2119, 0.1736, 0.1399, 0.1092, 0.0804, 0.0530, 0.0263],
        22 => &[0.4590, 0.3156, 0.2571, 0.2131, 0.1764, 0.1443, 0.1150, 0.0878, 0.0618, 0.0368, 0.0122],
        23 => &[0.4542, 0.3126, 0.2563, 0.2139, 0.1787, 0.1480, 0.1201, 0.0941, 0.0696, 0.0459, 0.0228],
        24 => &[0.4493, 0.3098, 0.2554, 0.2145, 0.1807, 0.1512, 0.1245, 0.0997, 0.0764, 0.0539, 0.0321, 0.0107],
        25 => &[0.4450, 0.3069, 0.2543, 0.2148, 0.1822, 0.1539, 0.1283, 0.1046, 0.0823, 0.0610, 0.0403, 0.0200],
        26 => &[0.4407, 0.3043, 0.2533, 0.2151, 0.1836, 0.1563, 0.1316, 0.1089, 0.0876, 0.0672, 0.0476, 0.0284, 0.0094],
        27 => &[0.4366, 0.3018, 0.2522, 0.2152, 0.1848, 0.1584, 0.1346, 0.1128, 0.0923, 0.0728, 0.0540, 0.0358, 0.0178],
        28 => &[0.4328, 0.2992, 0.2510, 0.2151, 0.1857, 0.1601, 0.1372, 0.1162, 0.0965, 0.0778, 0.0598, 0.0424, 0.0253, 0.0084],
        29 => &[0.4291, 0.2968, 0.2499, 0.2150, 0.1864, 0.1616, 0.1395, 0.1192, 0.1002, 0.0822, 0.0650, 0.0483, 0.0320, 0.0159],
        30 => &[0.4254, 0.2944, 0.2487, 0.2148, 0.1870, 0.1630, 0.1415, 0.1219, 0.1036, 0.0862, 0.0697, 0.0537, 0.0381, 0.0227, 0.0076],
        31 => &[0.4220, 0.2921, 0.2475, 0.2145, 0.1874, 0.1641, 0.1433, 0.1243, 0.1066, 0.0899, 0.0739, 0.0585, 0.0435, 0.0289, 0.0144],
        32 => &[0.4188, 0.2898, 0.2463, 0.2141, 0.1878, 0.1651, 0.1449, 0.1265, 0.1093, 0.0931, 0.0777, 0.0629, 0.0485, 0.0344, 0.0206, 0.0068],
        33 => &[0.4156, 0.2876, 0.2451, 0.2137, 0.1880, 0.1660, 0.1463, 0.1284, 0.1118, 0.0961, 0.0812, 0.0669, 0.0530, 0.0395, 0.0262, 0.0131],
        34 => &[0.4127, 0.2854, 0.2439, 0.2132, 0.1882, 0.1667, 0.1475, 0.1301, 0.1140, 0.0988, 0.0844, 0.0706, 0.0572, 0.0441, 0.0314, 0.0187, 0.0062],
        35 => &[0.4096, 0.2834, 0.2427, 0.2127, 0.1883, 0.1673, 0.1487, 0.1317, 0.1160, 0.1013, 0.0873, 0.0739, 0.0610, 0.0484, 0.0361, 0.0239, 0.0119],
        36 => &[0.4068, 0.2813, 0.2415, 0.2121, 0.1883, 0.1678, 0.1496, 0.1331, 0.1179, 0.1036, 0.0900, 0.0770, 0.0645, 0.0523, 0.0404, 0.0287, 0.0172, 0.0057],
        37 => &[0.4040, 0.2794, 0.2403, 0.2116, 0.1883, 0.1683, 0.1505, 0.1344, 0.1196, 0.1056, 0.0924, 0.0798, 0.0677, 0.0559, 0.0444, 0.0331, 0.0220, 0.0110],
        38 => &[0.4015, 0.2774, 0.2391, 0.2110, 0.1881, 0.1686, 0.1513, 0.1356, 0.1211, 0.1075, 0.0947, 0.0824, 0.0706, 0.0592, 0.0481, 0.0372, 0.0264, 0.0158, 0.0053],
        39 => &[0.3989, 0.2755, 0.2380, 0.2104, 0.1880, 0.1689, 0.1520, 0.1366, 0.1225, 0.1092, 0.0967, 0.0848, 0.0733, 0.0622, 0.0515, 0.0409, 0.0305, 0.0203, 0.0101],
        40 => &[0.3964, 0.2737, 0.2368, 0.2098, 0.1878, 0.1691, 0.1526, 0.1376, 0.1237, 0.1108, 0.0986, 0.0870, 0.0759, 0.0651, 0.0546, 0.0444, 0.0343, 0.0244, 0.0146, 0.0049],
        41 => &[0.3940, 0.2719, 0.2357, 0.2091, 0.1876, 0.1693, 0.1531, 0.1384, 0.1249, 0.1123, 0.1004, 0.0891, 0.0782, 0.0677, 0.0575, 0.0476, 0.0379, 0.0283, 0.0188, 0.0094],
        42 => &[0.3917, 0.2701, 0.2345, 0.2085, 0.1874, 0.1694, 0.1535, 0.1392, 0.1259, 0.1136, 0.1020, 0.0909, 0.0804, 0.0701, 0.0602, 0.0506, 0.0411, 0.0318, 0.0227, 0.0136, 0.0045],
        43 => &[0.3894, 0.2684, 0.2334, 0.2078, 0.1871, 0.1695, 0.1539, 0.1398, 0.1269, 0.1149, 0.1035, 0.0927, 0.0824, 0.0724, 0.0628, 0.0534, 0.0442, 0.0352, 0.0263, 0.0175, 0.0087],
        44 => &[0.3872, 0.2667, 0.2323, 0.2072, 0.1868, 0.1695, 0.1542, 0.1405, 0.1278, 0.1160, 0.1049, 0.0943, 0.0842, 0.0745, 0.0651, 0.0560, 0.0471, 0.0383, 0.0296, 0.0211, 0.0126, 0.0042],
        45 => &[0.3850, 0.2651, 0.2313, 0.2065, 0.1865, 0.1695, 0.1545, 0.1410, 0.1286, 0.1170, 0.1062, 0.0959, 0.0860, 0.0765, 0.0673, 0.0584, 0.0497, 0.0412, 0.0328, 0.0245, 0.0163, 0.0081],
        46 => &[0.3830, 0.2635, 0.2302, 0.2058, 0.1862, 0.1695, 0.1548, 0.1415, 0.1293, 0.1180, 0.1073, 0.0972, 0.0876, 0.0783, 0.0694, 0.0607, 0.0522, 0.0439, 0.0357, 0.0277, 0.0197, 0.0118, 0.0039],
        47 => &[0.3808, 0.2620, 0.2291, 0.2052, 0.1859, 0.1695, 0.1550, 0.1420, 0.1300, 0.1189, 0.1085, 0.0986, 0.0892, 0.0801, 0.0713, 0.0628, 0.0546, 0.0465, 0.0385, 0.0307, 0.0229, 0.0153, 0.0076],
        48 => &[0.3789, 0.2604, 0.2281, 0.2045, 0.1855, 0.1693, 0.1551, 0.1423, 0.1306, 0.1197, 0.1095, 0.0998, 0.0906, 0.0817, 0.0731, 0.0648, 0.0568, 0.0489, 0.0411, 0.0335, 0.0259, 0.0185, 0.0111, 0.0037],
        49 => &[0.3770, 0.2589, 0.2271, 0.2038, 0.1851, 0.1692, 0.1553, 0.1427, 0.1312, 0.1205, 0.1105, 0.1010, 0.0919, 0.0832, 0.0748, 0.0667, 0.0588, 0.0511, 0.0436, 0.0361, 0.0288, 0.0215, 0.0143, 0.0071],
        50 => &[0.3751, 0.2574, 0.2260, 0.2032, 0.1847, 0.1691, 0.1554, 0.1430, 0.1317, 0.1212, 0.1113, 0.1020, 0.0932, 0.0846, 0.0764, 0.0685, 0.0608, 0.0532, 0.0459, 0.0386, 0.0314, 0.0244, 0.0174, 0.0104, 0.0035],
        _ => return None,
    };
    Some(slice)
}

/// Shapiro-Wilk weight `a_{n-i+1}` for `1 <= i <= (n + 1) / 2` (exact Tabs 8-9 for `n <= 50`,
/// Royston AS R94 for `n > 50`).
pub fn shapiro_wilk_weight(n: usize, i: usize) -> Option<f64> {
    if n < 2 || i < 1 || i > n.div_ceil(2) {
        return None;
    }
    if n % 2 == 1 && i == n.div_ceil(2) {
        return Some(0.0);
    }
    if let Some(row) = sw_table_weights(n) {
        return row.get(i - 1).copied();
    }
    let weights = royston_weights(n)?;
    weights.get(i - 1).copied()
}

fn royston_weights(n: usize) -> Option<Vec<f64>> {
    if !(3..=5000).contains(&n) {
        return None;
    }
    let nf = n as f64;
    let mut m = Vec::with_capacity(n);
    let mut m_sq = 0.0_f64;
    for k in 1..=n {
        let mk = normal_inv_cdf((k as f64 - 0.375) / (nf + 0.25))?;
        m_sq += mk * mk;
        m.push(mk);
    }
    let u = 1.0 / nf.sqrt();
    let poly = |c: [f64; 5], base: f64| {
        ((((c[0] * u + c[1]) * u + c[2]) * u + c[3]) * u + c[4]) * u + base
    };
    let a_n = poly([-2.706056, 4.434685, -2.071190, -0.147981, 0.221157], m[n - 1] / m_sq.sqrt());
    let half = n / 2;
    let mut out = vec![0.0_f64; half];
    if n <= 5 {
        let phi = (m_sq - 2.0 * m[n - 1] * m[n - 1]) / (1.0 - 2.0 * a_n * a_n);
        let inv_s = 1.0 / phi.sqrt();
        out[0] = a_n;
        for idx in 1..half {
            out[idx] = m[n - 1 - idx] * inv_s;
        }
    } else {
        let a_n1 = poly([-3.582633, 5.682633, -1.752461, -0.293762, 0.042981], m[n - 2] / m_sq.sqrt());
        let phi = (m_sq - 2.0 * m[n - 1] * m[n - 1] - 2.0 * m[n - 2] * m[n - 2])
            / (1.0 - 2.0 * a_n * a_n - 2.0 * a_n1 * a_n1);
        let inv_s = 1.0 / phi.sqrt();
        out[0] = a_n;
        if half > 1 {
            out[1] = a_n1;
        }
        for idx in 2..half {
            out[idx] = m[n - 1 - idx] * inv_s;
        }
    }
    Some(out)
}

/// Shapiro-Wilk critical value `c` such that `P(W <= c | normal) = alpha` (exact Tab-10 for
/// `n = 3..=50` at tabulated `alpha`, with smooth Royston AS R94 for any other `n` or `alpha`).
pub fn shapiro_wilk_crit(n: usize, alpha: f64) -> Option<f64> {
    if n < 3 || !(alpha > 0.0 && alpha < 1.0) {
        return None;
    }
    if (3..=50).contains(&n) {
        let row = &SW_CRIT_TABLE[n - 3];
        for (col, &a) in SW_CRIT_ALPHAS.iter().enumerate() {
            if (alpha - a).abs() < 1e-6 {
                return Some(row[col]);
            }
        }
    }
    // Royston (1992) AS R94 continuous quantile.
    if n == 3 {
        let s = (PI / 6.0 * alpha + PI / 3.0).sin();
        return Some((s * s).clamp(0.75, 1.0));
    }
    let nf = n as f64;
    let z_hi = normal_inv_cdf(1.0 - alpha)?;
    if n <= 11 {
        let gamma = 0.459 * nf - 2.273;
        let mu = -0.0006714 * nf.powi(3) + 0.025054 * nf.powi(2) - 0.39978 * nf + 0.5440;
        let sigma = (-0.0020322 * nf.powi(3) + 0.062767 * nf.powi(2) - 0.77857 * nf + 1.3822).exp();
        let w_prime = mu + sigma * z_hi;
        let w = 1.0 - (gamma - (-w_prime).exp()).exp();
        return Some(w.clamp(0.0, 1.0));
    }
    let ln_n = nf.ln();
    let mu = 0.0038915 * ln_n.powi(3) - 0.083751 * ln_n.powi(2) - 0.31082 * ln_n - 1.5861;
    let sigma = (0.0030302 * ln_n.powi(2) - 0.082676 * ln_n - 0.4803).exp();
    let w = 1.0 - (mu + sigma * z_hi).exp();
    Some(w.clamp(0.0, 1.0))
}

/// Shapiro-Wilk CDF / p-value `P(W <= w | normal)` via Royston AS R94.
pub fn shapiro_wilk_cdf(n: usize, w: f64) -> Option<f64> {
    if n < 3 || !w.is_finite() {
        return None;
    }
    if w >= 1.0 {
        return Some(1.0);
    }
    if w <= 0.0 {
        return Some(0.0);
    }
    if n == 3 {
        if w <= 0.75 {
            return Some(0.0);
        }
        return Some(((6.0 / PI) * (w.sqrt().asin() - 0.75_f64.sqrt().asin())).clamp(0.0, 1.0));
    }
    let nf = n as f64;
    let y = (1.0 - w).ln();
    let z = if n <= 11 {
        let gamma = 0.459 * nf - 2.273;
        if y >= gamma {
            return Some(0.0);
        }
        let w_prime = -(gamma - y).ln();
        let mu = -0.0006714 * nf.powi(3) + 0.025054 * nf.powi(2) - 0.39978 * nf + 0.5440;
        let sigma = (-0.0020322 * nf.powi(3) + 0.062767 * nf.powi(2) - 0.77857 * nf + 1.3822).exp();
        (w_prime - mu) / sigma
    } else {
        let ln_n = nf.ln();
        let mu = 0.0038915 * ln_n.powi(3) - 0.083751 * ln_n.powi(2) - 0.31082 * ln_n - 1.5861;
        let sigma = (0.0030302 * ln_n.powi(2) - 0.082676 * ln_n - 0.4803).exp();
        (y - mu) / sigma
    };
    Some((0.5 * erfc(z / SQRT_2)).clamp(0.0, 1.0))
}

/// Compute the Shapiro-Wilk test statistic `W` directly for a sample of `n >= 3` values.
pub fn shapiro_wilk_stat(values: &[f64]) -> Option<f64> {
    let n = values.len();
    if n < 3 || values.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let ss: f64 = sorted.iter().map(|&x| (x - mean).powi(2)).sum();
    if ss <= 1e-18 {
        return None;
    }
    let half = n / 2;
    let mut num = 0.0_f64;
    for i in 1..=half {
        let a = shapiro_wilk_weight(n, i)?;
        num += a * (sorted[n - i] - sorted[i - 1]);
    }
    Some(((num * num) / ss).clamp(0.0, 1.0))
}

// ---------------------------------------------------------------------------
// Tab-11..13: Binomial Distribution B(n, p)
// ---------------------------------------------------------------------------

/// Binomial coefficient `C(n, k)` for non-negative integers.
pub fn binom_coeff(n: u64, k: u64) -> Option<f64> {
    if k > n {
        return Some(0.0);
    }
    if k == 0 || k == n {
        return Some(1.0);
    }
    let k = k.min(n - k);
    let log_c = ln_gamma(n as f64 + 1.0) - ln_gamma(k as f64 + 1.0) - ln_gamma((n - k) as f64 + 1.0);
    let v = log_c.exp();
    v.is_finite().then_some(if v < 1e15 { v.round() } else { v })
}

/// Binomial PMF `P(X = k)` for `X ~ B(n, p)`.
pub fn binom_pmf(n: u64, p: f64, k: i64) -> Option<f64> {
    if !(0.0..=1.0).contains(&p) || !p.is_finite() {
        return None;
    }
    if k < 0 || k as u64 > n {
        return Some(0.0);
    }
    let ku = k as u64;
    if p == 0.0 {
        return Some(if ku == 0 { 1.0 } else { 0.0 });
    }
    if p == 1.0 {
        return Some(if ku == n { 1.0 } else { 0.0 });
    }
    let log_p = ln_gamma(n as f64 + 1.0)
        - ln_gamma(ku as f64 + 1.0)
        - ln_gamma((n - ku) as f64 + 1.0)
        + (ku as f64) * p.ln()
        + ((n - ku) as f64) * (1.0 - p).ln();
    Some(log_p.exp().clamp(0.0, 1.0))
}

/// Binomial CDF `P(X <= k)` for `X ~ B(n, p)` (Tabs 11-13).
pub fn binom_cdf(n: u64, p: f64, k: i64) -> Option<f64> {
    if !(0.0..=1.0).contains(&p) || !p.is_finite() {
        return None;
    }
    if k < 0 {
        return Some(0.0);
    }
    let ku = k as u64;
    if ku >= n {
        return Some(1.0);
    }
    if p == 0.0 {
        return Some(1.0);
    }
    if p == 1.0 {
        return Some(0.0);
    }
    // Exact identity: P(B(n, p) <= k) = I_{1 - p}(n - k, k + 1).
    // For moderate n, summing the smaller tail avoids any subtraction loss; for large n, beta is O(1).
    if n > 2000 {
        return Some(reg_beta((n - ku) as f64, (ku + 1) as f64, 1.0 - p));
    }
    if ku <= n / 2 {
        let mut sum = 0.0_f64;
        for i in 0..=ku {
            sum += binom_pmf(n, p, i as i64)?;
        }
        Some(sum.clamp(0.0, 1.0))
    } else {
        let mut upper = 0.0_f64;
        for i in (ku + 1)..=n {
            upper += binom_pmf(n, p, i as i64)?;
        }
        Some((1.0 - upper).clamp(0.0, 1.0))
    }
}

// ---------------------------------------------------------------------------
// Tab-14..15: Poisson Distribution Pois(mu)
// ---------------------------------------------------------------------------

/// Poisson PMF `P(X = k) = mu^k * e^{-mu} / k!`.
pub fn poisson_pmf(mu: f64, k: i64) -> Option<f64> {
    if !(mu >= 0.0 && mu.is_finite()) {
        return None;
    }
    if k < 0 {
        return Some(0.0);
    }
    if mu == 0.0 {
        return Some(if k == 0 { 1.0 } else { 0.0 });
    }
    let kf = k as f64;
    let log_p = kf * mu.ln() - mu - ln_gamma(kf + 1.0);
    Some(log_p.exp().clamp(0.0, 1.0))
}

/// Poisson CDF `P(X <= k) = sum_{i=0..k} mu^i e^{-mu} / i!` (Tabs 14-15).
pub fn poisson_cdf(mu: f64, k: i64) -> Option<f64> {
    if !(mu >= 0.0 && mu.is_finite()) {
        return None;
    }
    if k < 0 {
        return Some(0.0);
    }
    if mu == 0.0 {
        return Some(1.0);
    }
    // Exact identity: P(Pois(mu) <= k) = Q(k + 1, mu).
    Some(reg_gamma_q((k + 1) as f64, mu).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) {
        assert!(
            (a - b).abs() <= tol,
            "expected {a} ≈ {b} (diff = {}, tol = {tol})",
            (a - b).abs()
        );
    }

    #[test]
    fn matches_all_15_statistics_reference_tables() {
        // Tab-1: Standard normal CDF & PDF
        approx(normal_cdf(0.0), 0.5000, 1e-4);
        approx(normal_cdf(1.96), 0.9750, 1e-4);
        approx(normal_cdf(1.64), 0.9495, 1e-4);
        approx(normal_cdf(-1.96), 0.0250, 1e-4);
        approx(normal_cdf(3.69), 0.9999, 1e-4);
        approx(normal_pdf(0.0), 0.3989, 1e-4);
        approx(normal_pdf(1.0), 0.2420, 1e-4);
        approx(normal_pdf(2.5), 0.0175, 1e-4);
        approx(normal_inv_cdf(0.975).unwrap(), 1.9600, 1e-4);
        approx(normal_upper_crit(0.025).unwrap(), 1.9600, 1e-4);

        // Tab-2: Student's t-distribution upper-tail critical values
        approx(t_upper_crit(1.0, 0.025).unwrap(), 12.706, 1e-3);
        approx(t_upper_crit(1.0, 0.0005).unwrap(), 636.62, 1e-2);
        approx(t_upper_crit(10.0, 0.05).unwrap(), 1.812, 1e-3);
        approx(t_upper_crit(10.0, 0.025).unwrap(), 2.228, 1e-3);
        approx(t_upper_crit(20.0, 0.01).unwrap(), 2.528, 1e-3);
        approx(t_upper_crit(120.0, 0.025).unwrap(), 1.980, 1e-3);
        approx(t_upper_crit(f64::INFINITY, 0.025).unwrap(), 1.960, 1e-3);
        approx(1.0 - t_cdf(2.2281, 10.0).unwrap(), 0.025, 1e-4);

        // Tab-3: Chi-square upper-tail critical values
        approx(chi2_upper_crit(1.0, 0.05).unwrap(), 3.84, 1e-2);
        approx(chi2_upper_crit(10.0, 0.05).unwrap(), 18.31, 1e-2);
        approx(chi2_upper_crit(10.0, 0.95).unwrap(), 3.94, 1e-2);
        approx(chi2_upper_crit(20.0, 0.025).unwrap(), 34.17, 1e-2);
        approx(chi2_upper_crit(100.0, 0.005).unwrap(), 140.17, 1e-2);
        approx(1.0 - chi2_cdf(18.307, 10.0).unwrap(), 0.05, 1e-4);

        // Tab-4..7: F-distribution upper-tail critical values (alpha = 0.05 and 0.025)
        approx(f_upper_crit(1.0, 1.0, 0.05).unwrap(), 161.4, 0.1);
        approx(f_upper_crit(5.0, 10.0, 0.05).unwrap(), 3.33, 1e-2);
        approx(f_upper_crit(12.0, 20.0, 0.05).unwrap(), 2.28, 1e-2);
        approx(f_upper_crit(f64::INFINITY, 10.0, 0.05).unwrap(), 2.54, 1e-2);
        approx(f_upper_crit(5.0, f64::INFINITY, 0.05).unwrap(), 2.21, 1e-2);
        approx(f_upper_crit(f64::INFINITY, f64::INFINITY, 0.05).unwrap(), 1.00, 1e-4);
        approx(f_upper_crit(5.0, 10.0, 0.025).unwrap(), 4.24, 1e-2);
        approx(f_upper_crit(15.0, 25.0, 0.025).unwrap(), 2.41, 1e-2);
        approx(f_upper_crit(f64::INFINITY, 10.0, 0.025).unwrap(), 3.08, 1e-2);
        approx(f_upper_crit(10.0, f64::INFINITY, 0.025).unwrap(), 2.05, 1e-2);

        // Tab-8..10: Shapiro-Wilk weights, critical values, and W statistic
        approx(shapiro_wilk_weight(30, 1).unwrap(), 0.4254, 1e-5);
        approx(shapiro_wilk_weight(30, 2).unwrap(), 0.2944, 1e-5);
        approx(shapiro_wilk_weight(30, 15).unwrap(), 0.0076, 1e-5);
        approx(shapiro_wilk_weight(50, 15).unwrap(), 0.0764, 1e-5);
        approx(shapiro_wilk_crit(30, 0.05).unwrap(), 0.927, 1e-4);
        approx(shapiro_wilk_crit(3, 0.05).unwrap(), 0.767, 1e-4);
        approx(shapiro_wilk_crit(50, 0.01).unwrap(), 0.930, 1e-4);
        assert!(shapiro_wilk_stat(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap() > 0.95);

        // Tab-11..13: Binomial cumulative probabilities
        approx(binom_cdf(5, 0.30, 2).unwrap(), 0.837, 1e-3);
        approx(binom_cdf(10, 1.0 / 6.0, 3).unwrap(), 0.930, 1e-3);
        approx(binom_cdf(15, 0.25, 4).unwrap(), 0.686, 1e-3);
        approx(binom_cdf(20, 0.50, 10).unwrap(), 0.588, 1e-3);
        approx(binom_cdf(25, 1.0 / 3.0, 8).unwrap(), 0.538, 1e-3);

        // Tab-14..15: Poisson cumulative probabilities
        approx(poisson_cdf(0.02, 0).unwrap(), 0.980, 1e-3);
        approx(poisson_cdf(2.0, 3).unwrap(), 0.857, 1e-3);
        approx(poisson_cdf(5.0, 4).unwrap(), 0.440, 1e-3);
        approx(poisson_cdf(10.0, 15).unwrap(), 0.951, 1e-3);
        approx(poisson_cdf(10.0, 18).unwrap(), 0.993, 1e-3);
    }
}
