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
                write!(
                    f,
                    "benign covariance is singular; calibration corpus lacks variation"
                )
            }
            MetricError::InfeasibleProjection { residual } => {
                write!(
                    f,
                    "metric feasibility projection failed, residual {residual:.3e}"
                )
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
        Metric {
            m: linalg::identity(),
            projection_distance: 0.0,
        }
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
        Ok(Metric {
            m: feasible,
            projection_distance: dist,
        })
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

/// Inverse of the Lyapunov operator, `L⁻¹(S)[i][j] = S[i][j] / (λᵢ + λⱼ)`.
///
/// Kept for diagnostics and not used by `project_feasible`. It is retained
/// precisely because its conditioning is the finding: with half-lives spanning
/// nine orders, the slow-slow denominator is ~4e-13 and this map amplifies by
/// ~10¹². Anything reaching for it should see that first.
pub fn lyapunov_inv(s: &Mat6, rates: &Vec6) -> Mat6 {
    let mut out = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            let denom = rates[i] + rates[j];
            out[i][j] = if denom.abs() > 1e-300 {
                s[i][j] / denom
            } else {
                0.0
            };
        }
    }
    symmetrize(&out)
}

/// Make a symmetric matrix positive definite and Lyapunov-feasible, in
/// closed form, returning it with the Frobenius distance moved.
///
/// Damp each entry by the ratio of the geometric to the arithmetic mean of its
/// two decay rates:
///
/// ```text
///     C[i][j] = 2·√(λᵢλⱼ) / (λᵢ + λⱼ),     M' = M ∘ C
/// ```
///
/// Then `(ΛM' + M'Λ)[i][j] = 2·√(λᵢλⱼ)·M[i][j]`, i.e. `ΛM' + M'Λ = 2·Λ^½ M Λ^½`,
/// which is PSD by congruence whenever `M` is. `M'` stays PSD too: `C` is a
/// Hadamard product of the rank-one kernel `√λᵢ√λⱼ` with the Cauchy kernel
/// `1/(λᵢ+λⱼ)`, both PSD for positive rates, so `C` is PSD with unit diagonal
/// and Schur's product theorem gives `M ∘ C ⪰ 0`. Single pass, no iteration.
///
/// `C` is 1 on the diagonal and shrinks as two axes' rates diverge, so **an
/// axis that never decays cannot stay correlated with a fast one.** That is the
/// real content of the Lyapunov condition rather than an artifact of the
/// method, and it is why `docs/05` §5.6 warned this constraint binds here.
/// Authority and reach differ by a factor of four in rate and keep ~0.8 of
/// their fitted correlation; irreversibility, nine orders slower than tempo,
/// is decorrelated from it almost entirely.
///
/// Two alternatives were implemented and rejected, both on measured failures:
///
/// - Alternating projections through `L⁻¹`. `L⁻¹` scales entry `(i,j)` by
///   `1/(λᵢ+λⱼ)` — about `4e-13` for the slow-slow entry — so it amplifies by
///   `~10¹²`. On a fitted metric it produced a budget of `3.1e8` bits²: valid
///   arithmetic, meaningless geometry.
/// - Bisecting one global damping factor toward the diagonal. Stable, but far
///   too blunt: it crushed the authority-reach correlation, which was never the
///   problem, along with the irreversibility coupling, which was — discarding
///   exactly the structure `docs/04` identifies as carrying the escalation
///   signal.
pub fn project_feasible(raw: &Mat6, rates: &Vec6) -> (Mat6, f64) {
    let original = symmetrize(raw);
    let mut m = spectral_map(&original, |l| l.max(PD_FLOOR));

    let mut out = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            let denom = rates[i] + rates[j];
            let c = if denom > 0.0 {
                2.0 * (rates[i] * rates[j]).sqrt() / denom
            } else if i == j {
                1.0
            } else {
                0.0
            };
            out[i][j] = m[i][j] * c;
        }
    }

    m = symmetrize(&out);
    if min_eigenvalue(&m) < PD_FLOOR {
        m = spectral_map(&m, |l| l.max(PD_FLOOR));
    }

    let dist = frobenius(&linalg::mat_sub(&m, &original));
    (m, dist)
}

/// The damping matrix `C`. Exposed so a deployment can see which axis pairs its
/// measured half-lives forbid from being correlated — a large off-diagonal
/// suppression is a fact about the environment worth surfacing.
pub fn damping_matrix(rates: &Vec6) -> Mat6 {
    let mut c = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            let denom = rates[i] + rates[j];
            c[i][j] = if denom > 0.0 {
                2.0 * (rates[i] * rates[j]).sqrt() / denom
            } else if i == j {
                1.0
            } else {
                0.0
            };
        }
    }
    c
}

/// Sample covariance of benign displacement vectors.
pub fn covariance(samples: &[Vec6]) -> Result<Mat6, MetricError> {
    if samples.len() <= N {
        return Err(MetricError::InsufficientSamples {
            got: samples.len(),
            need: N + 1,
        });
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
        assert!(
            !is_psd(&lyapunov(&m, &r)),
            "and it should be Lyapunov-infeasible"
        );
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
        assert!(
            is_psd(&lyapunov(&fixed, &r)),
            "result must be Lyapunov-feasible"
        );
        assert!(
            min_eigenvalue(&fixed) > 0.0,
            "result must stay positive definite"
        );
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
        assert!(matches!(
            covariance(&s),
            Err(MetricError::InsufficientSamples { .. })
        ));
    }

    #[test]
    fn damping_preserves_similar_rates_and_crushes_disparate_ones() {
        // The behavior the closed form exists for. Authority and reach have
        // rates within a factor of four and must keep their correlation, since
        // docs/04 identifies it as carrying the escalation signal.
        // Irreversibility is nine orders slower than tempo and cannot stay
        // correlated with it under the Lyapunov condition.
        use crate::axis::Axis;
        let r = rates();
        let c = damping_matrix(&r);

        let ah = c[Axis::Authority.index()][Axis::Reach.index()];
        let it = c[Axis::Irreversibility.index()][Axis::Tempo.index()];

        assert!(ah > 0.7, "authority-reach correlation over-damped: {ah}");
        assert!(
            it < 1e-4,
            "irreversibility-tempo correlation under-damped: {it}"
        );
        for i in 0..N {
            assert!((c[i][i] - 1.0).abs() < 1e-12, "diagonal must be unity");
        }
    }

    #[test]
    fn the_closed_form_makes_an_arbitrary_correlated_metric_feasible() {
        let r = rates();
        let mut m = linalg::identity();
        // Correlate every pair, including the ones the condition forbids.
        for i in 0..N {
            for j in 0..N {
                if i != j {
                    m[i][j] = 0.6;
                }
            }
        }
        let (fixed, dist) = project_feasible(&m, &r);
        assert!(dist > 0.0);
        assert!(is_psd(&lyapunov(&fixed, &r)), "must be Lyapunov-feasible");
        assert!(min_eigenvalue(&fixed) > 0.0, "must stay positive definite");
    }

    #[test]
    fn a_feasible_fitted_metric_never_gains_potential_under_relaxation() {
        // End-to-end check of what the whole apparatus is for: after
        // projection, decay toward baseline can only reduce V, at every
        // timescale from a second to a decade.
        let r = rates();
        let mut m = linalg::identity();
        m[0][1] = 0.9;
        m[1][0] = 0.9;
        m[2][5] = 0.8;
        m[5][2] = 0.8;
        let (fixed, _) = project_feasible(&m, &r);
        let z = [2.0, -3.0, 1.5, 0.5, -1.0, 4.0];
        for dt in [1.0, 60.0, 3600.0, 86400.0, 3.15e8] {
            assert!(
                !relaxation_increases_potential(&fixed, &r, &z, dt),
                "potential grew under relaxation at dt={dt}"
            );
        }
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
