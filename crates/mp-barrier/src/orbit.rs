//! Orbit residual: distance from an asker to its symmetry group's orbit.
//!
//! `docs/02-symmetry.md` B1, `docs/05-formulation.md` §5.8.
//!
//! Symmetries S1 and S3 say the system cannot distinguish members of an
//! equivalence class — replicas of one deployment, identical operator
//! instances, sensors of the same model on the same segment. If their carried
//! states diverge anyway, the symmetry that *should* hold observably does not,
//! and that broken symmetry is evidence.
//!
//! Why this is not just anomaly detection: conventional anomaly detection
//! compares an asker to its own past, which a patient attacker corrupts by
//! moving the baseline slowly enough. The orbit residual compares an asker to
//! its *peers in the present*. An attacker cannot poison that baseline without
//! also compromising the peers, whom it does not control. The reference is held
//! by parties outside the adversary's reach, which is what makes it durable.

use mp_core::linalg::{Vec6, N};
use mp_core::metric::Metric;

/// Residual of one asker against its peer group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitResidual {
    /// `ρ_p = ‖z_p − median(z_G)‖_M`.
    pub distance: f64,
    /// `σ_G`, the group's median absolute deviation in the same metric.
    pub spread: f64,
    /// `Π₄ = ρ_p / σ_G`. Dimensionless, in units of the group's own spread.
    pub ratio: f64,
    /// Peers the estimate was computed from, excluding the subject.
    pub peers: usize,
}

impl OrbitResidual {
    /// Neutral residual, used when a group is too small to say anything.
    pub fn none() -> Self {
        OrbitResidual {
            distance: 0.0,
            spread: 0.0,
            ratio: 0.0,
            peers: 0,
        }
    }
}

/// Minimum peers required before a residual is meaningful.
///
/// With fewer than this, the median has no breakdown-point advantage over the
/// mean and the MAD is not estimable. Returning a neutral residual rather than
/// a noisy one matters: the residual *tightens* α, so a spurious value would
/// throttle legitimate askers for no reason.
pub const MIN_PEERS: usize = 4;

/// Coordinate-wise median.
fn median_vec(states: &[Vec6]) -> Vec6 {
    let mut out = [0.0; N];
    let mut buf: Vec<f64> = Vec::with_capacity(states.len());
    for (i, slot) in out.iter_mut().enumerate() {
        buf.clear();
        buf.extend(states.iter().map(|s| s[i]));
        buf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        *slot = median_sorted(&buf);
    }
    out
}

fn median_sorted(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// Compute the orbit residual of `subject` against `peers`.
///
/// Median and MAD rather than mean and standard deviation, because the
/// adversary is *in* the sample. A mean-based reference moves toward whichever
/// members are compromised; the median tolerates up to half the group being
/// compromised before it does. Breakdown point is a security property here, not
/// a statistical nicety.
pub fn residual(metric: &Metric, subject: &Vec6, peers: &[Vec6]) -> OrbitResidual {
    if peers.len() < MIN_PEERS {
        return OrbitResidual::none();
    }

    let center = median_vec(peers);
    let distance = metric.distance(subject, &center);

    let mut devs: Vec<f64> = peers.iter().map(|p| metric.distance(p, &center)).collect();
    devs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let spread = median_sorted(&devs);

    // A perfectly uniform group has zero spread. Any deviation from it is then
    // infinitely many "spreads" away, which would deny on the first bit of
    // legitimate variation. Floor the denominator at a small fraction of the
    // distance scale so a uniform group yields a large but finite ratio.
    let denom = if spread > 1e-9 {
        spread
    } else {
        1e-9_f64.max(distance * 1e-3)
    };
    let ratio = if distance > 0.0 {
        distance / denom
    } else {
        0.0
    };

    OrbitResidual {
        distance,
        spread,
        ratio,
        peers: peers.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: f64) -> Vec6 {
        let mut z = [0.0; N];
        z[0] = a;
        z
    }

    #[test]
    fn a_group_that_is_too_small_yields_a_neutral_residual() {
        let m = Metric::identity();
        let r = residual(&m, &v(100.0), &[v(0.0), v(0.0)]);
        assert_eq!(r.ratio, 0.0);
        assert_eq!(r.peers, 0);
    }

    #[test]
    fn a_conforming_member_has_a_small_residual() {
        let m = Metric::identity();
        let peers: Vec<Vec6> = (0..10).map(|i| v(1.0 + i as f64 * 0.01)).collect();
        let r = residual(&m, &v(1.05), &peers);
        assert!(r.ratio < 5.0, "conforming member scored {}", r.ratio);
    }

    #[test]
    fn a_divergent_member_has_a_large_residual() {
        let m = Metric::identity();
        let peers: Vec<Vec6> = (0..10).map(|i| v(1.0 + i as f64 * 0.01)).collect();
        let r = residual(&m, &v(50.0), &peers);
        assert!(r.ratio > 100.0, "divergent member only scored {}", r.ratio);
    }

    #[test]
    fn the_median_reference_survives_a_compromised_minority() {
        // The reason for median-and-MAD over mean-and-stddev. Four of ten peers
        // are compromised and far out; the reference must barely move, so the
        // honest subject stays unpunished and the compromised ones stay visible.
        let m = Metric::identity();
        let mut peers: Vec<Vec6> = (0..6).map(|i| v(1.0 + i as f64 * 0.01)).collect();
        peers.extend((0..4).map(|_| v(500.0)));

        let honest = residual(&m, &v(1.02), &peers);
        let compromised = residual(&m, &v(500.0), &peers);

        assert!(honest.ratio < compromised.ratio / 100.0);
        assert!(
            honest.distance < 1.0,
            "honest member dragged to {}",
            honest.distance
        );
    }

    #[test]
    fn a_perfectly_uniform_group_does_not_divide_by_zero() {
        let m = Metric::identity();
        let peers = vec![v(1.0); 8];
        let r = residual(&m, &v(1.0), &peers);
        assert!(r.ratio.is_finite());
        let r2 = residual(&m, &v(9.0), &peers);
        assert!(r2.ratio.is_finite() && r2.ratio > 0.0);
    }
}
