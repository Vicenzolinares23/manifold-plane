//! The metric tensor `M`, the potential `V`, and the barrier `h`.
//!
//! `docs/05-formulation.md` §5.2, §5.3, §5.6.
//!
//! `M` is fit as the shrinkage-regularized inverse covariance of benign
//! trajectory displacements, which makes `‖z‖_M` a Mahalanobis distance:
//! directions benign traffic explores freely are cheap, directions it never
//! explores are expensive. The escalation-relevant structure — that a single
//! permission on a bridge resource buys far more reach than authority — is
//! learned as an off-diagonal term rather than encoded by hand.

use crate::linalg::{
    self, frobenius, inv_spd, mat_add, mat_scale, matmul, min_eigenvalue, spectral_map, symmetrize,
    trace, Mat6, Vec6, N,
};

/// Numerical floor for eigenvalues of the metric.
const PD_FLOOR: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq)]
pub enum MetricError {
    /// Covariance was singular or near-singular: too few samples, or an axis
    /// with no variation in the calibration corpus.
    SingularCovariance,
    /// The feasibility projection could not satisfy both constraints. In
    /// practice this means the fitted correlation structure is deeply
    /// incompatible with the measured half-lives.
    InfeasibleProjection { residual: f64 },
    /// Fewer samples than dimensions; the covariance would be rank-deficient.
    InsufficientSamples { got: usize, need: usize },
}

impl std::fmt::Display for MetricError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricError::SingularCovariance => {
                write!(f, "benign covariance is singular; calibration corpus lacks variation")
            }
            MetricError::InfeasibleProjection { residual } => {
                write!(f, "metric feasibility projection failed, residual {residual:.3e}")
            }
            MetricError::InsufficientSamples { got, need } => {
                write!(f, "need at least {need} calibration samples, got {got}")
            }
        }
    }
}

impl std::error::Error for MetricError {}

/// A validated metric tensor.
///
/// Construction is fallible on purpose. `docs/06` T5 shows that a metric
/// violating the Lyapunov condition lets an asker leave the safe set while
/// issuing no requests at all, which voids the forward-invariance theorem
/// entirely. An unvalidated `M` is therefore not a degraded metric, it is a
/// silently broken safety argument — so it cannot be constructed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metric {
    m: Mat6,
    /// How far the feasibility projection had to move the fitted matrix.
    /// Surfaced rather than swallowed: a large value is a finding about the
    /// deployment, per `docs/05` §5.6.
    projection_distance: f64,
}

impl Metric {
    /// Identity metric. Only for tests and bootstrapping — it asserts all six
    /// axes are interchangeable and uncorrelated, which `docs/02` N1 rejects.
    pub fn identity() -> Self {
        Metric { m: linalg::identity(), projection_distance: 0.0 }
    }

    /// Build from a raw matrix, enforcing positive-definiteness and the
    /// Lyapunov feasibility condition `ΛM + MΛ ⪰ 0`.
    pub fn new(raw: Mat6, rates: &Vec6) -> Result<Self, MetricError> {
        let sym = symmetrize(&raw);
        let (feasible, dist) = project_feasible(&sym, rates);

        let residual = -min_eigenvalue(&lyapunov(&feasible, rates)).max(0.0);
        if residual > 1e-6 || min_eigenvalue(&feasible) <= 0.0 {
            return Err(MetricError::InfeasibleProjection { residual });
        }
        Ok(Metric { m: feasible, projection_distance: dist })
    }

    pub fn as_matrix(&self) -> &Mat6 {
        &self.m
    }

    pub fn projection_distance(&self) -> f64 {
        self.projection_distance
    }

    /// `V(x) = zᵀ M z`, in bits², where `z = x - x₀`.
    pub fn potential(&self, z: &Vec6) -> f64 {
        linalg::quad(&self.m, z)
    }

    /// `⟨u, v⟩_M`.
    pub fn inner(&self, u: &Vec6, v: &Vec6) -> f64 {
        linalg::dot(u, &linalg::matvec(&self.m, v))
    }

    /// `‖u‖_M`.
    pub fn norm(&self, u: &Vec6) -> f64 {
        self.potential(u).max(0.0).sqrt()
    }

    /// Distance between two states in the metric.
    pub fn distance(&self, a: &Vec6, b: &Vec6) -> f64 {
        self.norm(&linalg::sub(a, b))
    }

    /// Verify the Lyapunov condition holds. Cheap enough to assert in tests
    /// and on config reload.
    pub fn is_feasible(&self, rates: &Vec6) -> bool {
        min_eigenvalue(&lyapunov(&self.m, rates)) >= -1e-9 && min_eigenvalue(&self.m) > 0.0
    }
}

/// The Lyapunov operator `L(M) = ΛM + MΛ`, with `Λ = diag(rates)`.
///
/// Entrywise this is `(λᵢ + λⱼ)·Mᵢⱼ`, which is why it is invertible whenever
/// no two rates sum to zero — true here since all rates are non-negative and
/// not both zero.
pub fn lyapunov(m: &Mat6, rates: &Vec6) -> Mat6 {
    let mut out = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            out[i][j] = (rates[i] + rates[j]) * m[i][j];
        }
    }
    symmetrize(&out)
}

/// Inverse of the Lyapunov operator.
fn lyapunov_inv(s: &Mat6, rates: &Vec6) -> Mat6 {
    let mut out = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            let denom = rates[i] + rates[j];
            out[i][j] = if denom.abs() > 1e-300 { s[i][j] / denom } else { 0.0 };
        }
    }
    symmetrize(&out)
}

/// Project a symmetric matrix onto `{M ≻ 0} ∩ {ΛM + MΛ ⪰ 0}` by alternating
/// projections, returning the projected matrix and the Frobenius distance moved.
///
/// The second step is an approximate projection: the Lyapunov operator is
/// linear and invertible but not orthogonal, so mapping through it, clipping
/// the spectrum, and mapping back is not the exact metric projection onto that
/// set. It is a contraction toward feasibility, alternation converges to a
/// point in the intersection, and — this being the part that matters —
/// feasibility of the *result* is verified directly by `Metric::new` rather
/// than assumed from the procedure.
pub fn project_feasible(raw: &Mat6, rates: &Vec6) -> (Mat6, f64) {
    let original = symmetrize(raw);
    let mut m = original;

    for _ in 0..256 {
        // Onto the positive-definite cone.
        m = spectral_map(&m, |l| l.max(PD_FLOOR));

        // Onto the Lyapunov-feasible set.
        let s = lyapunov(&m, rates);
        if min_eigenvalue(&s) >= 0.0 && min_eigenvalue(&m) >= PD_FLOOR {
            break;
        }
        let s_clipped = spectral_map(&s, |l| l.max(0.0));
        m = lyapunov_inv(&s_clipped, rates);
    }

    m = spectral_map(&m, |l| l.max(PD_FLOOR));
    let dist = frobenius(&linalg::mat_sub(&m, &original));
    (m, dist)
}

/// Sample covariance of benign displacement vectors.
pub fn covariance(samples: &[Vec6]) -> Result<Mat6, MetricError> {
    if samples.len() <= N {
        return Err(MetricError::InsufficientSamples { got: samples.len(), need: N + 1 });
    }
    let n = samples.len() as f64;

    let mut mean = [0.0; N];
    for s in samples {
        for i in 0..N {
            mean[i] += s[i];
        }
    }
    for i in 0..N {
        mean[i] /= n;
    }

    let mut cov = [[0.0; N]; N];
    for s in samples {
        for i in 0..N {
            let di = s[i] - mean[i];
            for j in 0..N {
                cov[i][j] += di * (s[j] - mean[j]);
            }
        }
    }
    // Bessel correction.
    for i in 0..N {
        for j in 0..N {
            cov[i][j] /= n - 1.0;
        }
    }
    Ok(symmetrize(&cov))
}

/// Ledoit–Wolf shrinkage intensity toward a scaled identity.
///
/// Computed in closed form from the sample, not tuned. `docs/03` requires every
/// constant to have a measurement procedure, and this is that procedure for the
/// one regularization parameter the fit needs.
pub fn ledoit_wolf_intensity(samples: &[Vec6], cov: &Mat6) -> f64 {
    let n = samples.len() as f64;
    if n <= 1.0 {
        return 1.0;
    }

    let mut mean = [0.0; N];
    for s in samples {
        for i in 0..N {
            mean[i] += s[i];
        }
    }
    for i in 0..N {
        mean[i] /= n;
    }

    let mu = trace(cov) / N as f64;

    // Dispersion of the sample covariance around the shrinkage target.
    let mut d2 = 0.0;
    for i in 0..N {
        for j in 0..N {
            let target = if i == j { mu } else { 0.0 };
            let d = cov[i][j] - target;
            d2 += d * d;
        }
    }

    // Expected estimation error of the sample covariance itself.
    let mut b2 = 0.0;
    for s in samples {
        let mut outer_err = 0.0;
        for i in 0..N {
            let di = s[i] - mean[i];
            for j in 0..N {
                let e = di * (s[j] - mean[j]) - cov[i][j];
                outer_err += e * e;
            }
        }
        b2 += outer_err;
    }
    b2 /= n * n;

    if d2 <= 0.0 {
        return 1.0;
    }
    (b2 / d2).clamp(0.0, 1.0)
}

/// Fit a metric from a corpus of benign displacement vectors.
///
/// `M = shrunk(Cov)⁻¹`, then projected onto the feasible cone.
pub fn fit(samples: &[Vec6], rates: &Vec6) -> Result<Metric, MetricError> {
    let cov = covariance(samples)?;
    let gamma = ledoit_wolf_intensity(samples, &cov);
    let mu = trace(&cov) / N as f64;

    let shrunk = mat_add(
        &mat_scale(&cov, 1.0 - gamma),
        &mat_scale(&linalg::identity(), gamma * mu),
    );

    let m = inv_spd(&shrunk, PD_FLOOR).ok_or(MetricError::SingularCovariance)?;
    Metric::new(m, rates)
}

/// Calibrate the danger budget `c` as a high quantile of `V` over the benign
/// corpus (`docs/03`). Not a chosen threshold — a measured one.
pub fn calibrate_budget(metric: &Metric, samples: &[Vec6], quantile: f64) -> f64 {
    let mut vs: Vec<f64> = samples.iter().map(|z| metric.potential(z)).collect();
    vs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if vs.is_empty() {
        return 0.0;
    }
    let q = quantile.clamp(0.0, 1.0);
    let idx = ((vs.len() - 1) as f64 * q).round() as usize;
    vs[idx]
}

/// `M ⪰ 0` check used by tests.
pub fn is_psd(m: &Mat6) -> bool {
    min_eigenvalue(m) >= -1e-9
}

/// Verify empirically that relaxation does not increase `V`.
///
/// This is the direct check of `docs/05` §5.6 — the property that the Lyapunov
/// condition exists to guarantee. Used in tests to confirm the algebra matches
/// the numerics rather than trusting the derivation alone.
pub fn relaxation_increases_potential(m: &Mat6, rates: &Vec6, z: &Vec6, dt: f64) -> bool {
    let mut d = [0.0; N];
    for i in 0..N {
        d[i] = (-rates[i] * dt).exp();
    }
    let dm = matmul(&matmul(&linalg::diag(&d), m), &linalg::diag(&d));
    linalg::quad(&dm, z) > linalg::quad(m, z) + 1e-12
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::{nominal_half_lives, rates_from_half_lives};

    fn rates() -> Vec6 {
        rates_from_half_lives(&nominal_half_lives())
    }

    #[test]
    fn identity_metric_is_feasible() {
        assert!(Metric::identity().is_feasible(&rates()));
    }

    #[test]
    fn the_counterexample_from_docs_05_is_real() {
        // docs/05 §5.6 claims a diagonal contraction can *increase* a quadratic
        // form when M has off-diagonal structure, with a ~13x example. If that
        // claim were false the whole feasibility apparatus would be
        // unnecessary, so pin it down.
        let mut m = linalg::identity();
        m[0][1] = -0.99;
        m[1][0] = -0.99;
        for i in 2..N {
            m[i][i] = 1.0;
        }
        let r = {
            let mut r = [0.0; N];
            r[1] = 1.0; // axis 1 decays, axis 0 does not
            r
        };
        let mut z = [0.0; N];
        z[0] = 1.0;
        z[1] = 1.0;

        assert!(
            relaxation_increases_potential(&m, &r, &z, std::f64::consts::LN_2),
            "the counterexample in docs/05 should reproduce"
        );
        assert!(!is_psd(&lyapunov(&m, &r)), "and it should be Lyapunov-infeasible");
    }

    #[test]
    fn projection_repairs_the_counterexample() {
        let mut m = linalg::identity();
        m[0][1] = -0.99;
        m[1][0] = -0.99;
        let mut r = [0.0; N];
        r[1] = 1.0;
        r[0] = 1e-6;

        let (fixed, dist) = project_feasible(&m, &r);
        assert!(dist > 0.0, "projection should have moved the matrix");
        assert!(is_psd(&lyapunov(&fixed, &r)), "result must be Lyapunov-feasible");
        assert!(min_eigenvalue(&fixed) > 0.0, "result must stay positive definite");
    }

    #[test]
    fn feasible_metric_never_gains_potential_under_relaxation() {
        let m = Metric::identity();
        let r = rates();
        assert!(m.is_feasible(&r));
        let z = [1.0, -2.0, 0.5, 3.0, -1.5, 2.0];
        for dt in [0.1, 1.0, 60.0, 3600.0, 86400.0] {
            assert!(!relaxation_increases_potential(m.as_matrix(), &r, &z, dt));
        }
    }

    #[test]
    fn covariance_rejects_too_few_samples() {
        let s = vec![[0.0; N]; 3];
        assert!(matches!(covariance(&s), Err(MetricError::InsufficientSamples { .. })));
    }

    #[test]
    fn budget_calibration_is_monotone_in_quantile() {
        let m = Metric::identity();
        let samples: Vec<Vec6> = (0..100)
            .map(|i| {
                let mut z = [0.0; N];
                z[0] = i as f64 * 0.1;
                z
            })
            .collect();
        let c50 = calibrate_budget(&m, &samples, 0.5);
        let c99 = calibrate_budget(&m, &samples, 0.99);
        assert!(c99 > c50);
    }
}
