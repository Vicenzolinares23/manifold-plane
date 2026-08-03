//! The six capability axes.
//!
//! Derived in `docs/04-state-variables.md`. Each survived a five-part test:
//! sufficiency, independence, distinct decay timescale, a measurement
//! procedure, and adversarial relevance. Five candidate axes were rejected;
//! they are listed in that document with the reason.
//!
//! Every axis is measured in **bits** (`docs/03-dimensional-analysis.md`). That
//! is what makes a quadratic form over them arithmetic rather than a category
//! error, and it is why the danger budget has units of bits².

use crate::linalg::{Vec6, N};

/// A capability axis. The discriminants are stable and index into [`Vec6`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(usize)]
pub enum Axis {
    /// `a` — accumulated privilege. `log2(|Ops| / |Ops_baseline|)`.
    Authority = 0,
    /// `h` — blast radius. `log2(|Reach| / |Reach_baseline|)`, over the
    /// transitive closure of currently-permitted operations, not direct grants.
    Reach = 1,
    /// `ι` — permanence. `log2(preimage count)` of the applied operation: a
    /// literal entropy, being the bits about the prior world that are gone.
    Irreversibility = 2,
    /// `ω` — unobservability. `-log2(recorded_fraction)`. Unbounded as
    /// recording approaches zero, which is the correct behavior.
    Opacity = 3,
    /// `κ` — correlation with peers. Mutual information, natively in bits,
    /// chosen over raw correlation because independent couplings must add.
    Coupling = 4,
    /// `τ` — rate against the asker's own habit. `log2(r / r_baseline)`.
    /// Never absolute rate: `docs/01` I5 rules that out.
    Tempo = 5,
}

pub const ALL_AXES: [Axis; N] = [
    Axis::Authority,
    Axis::Reach,
    Axis::Irreversibility,
    Axis::Opacity,
    Axis::Coupling,
    Axis::Tempo,
];

impl Axis {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn short(self) -> &'static str {
        match self {
            Axis::Authority => "a",
            Axis::Reach => "h",
            Axis::Irreversibility => "iota",
            Axis::Opacity => "omega",
            Axis::Coupling => "kappa",
            Axis::Tempo => "tau",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Axis::Authority => "authority",
            Axis::Reach => "reach",
            Axis::Irreversibility => "irreversibility",
            Axis::Opacity => "opacity",
            Axis::Coupling => "coupling",
            Axis::Tempo => "tempo",
        }
    }

    /// Nominal half-life in seconds, from `docs/03`.
    ///
    /// These are defaults for bootstrapping only. A real deployment measures
    /// them by fitting decay on its own benign corpus — `docs/03` makes that a
    /// requirement, because a half-life is a property of an environment and not
    /// of this software.
    ///
    /// The spread across these is roughly nine orders of magnitude, and that
    /// spread is the whole argument against a scalar risk score: a scalar has
    /// exactly one half-life, so any choice of it is wrong by up to nine orders
    /// on some axis.
    pub const fn nominal_half_life_secs(self) -> f64 {
        match self {
            Axis::Tempo => 30.0,
            Axis::Coupling => 300.0,
            Axis::Opacity => 3_600.0,
            Axis::Authority => 43_200.0,
            Axis::Reach => 172_800.0,
            // Destroyed information does not come back. Represented as an
            // enormous but finite half-life so the dynamics stay uniform;
            // `is_permanent` is the honest predicate for anything that needs
            // to branch on it.
            Axis::Irreversibility => 3.15e12,
        }
    }

    /// True for axes that do not meaningfully decay.
    pub const fn is_permanent(self) -> bool {
        matches!(self, Axis::Irreversibility)
    }
}

/// Nominal half-lives for all axes, in seconds.
pub fn nominal_half_lives() -> Vec6 {
    let mut v = [0.0; N];
    for ax in ALL_AXES {
        v[ax.index()] = ax.nominal_half_life_secs();
    }
    v
}

/// Decay rates `Λ = ln2 / T½`, in inverse seconds.
///
/// This is the diagonal of the relaxation generator in `docs/05` §5.4.
pub fn rates_from_half_lives(half_lives: &Vec6) -> Vec6 {
    let mut v = [0.0; N];
    for i in 0..N {
        v[i] = if half_lives[i].is_finite() && half_lives[i] > 0.0 {
            std::f64::consts::LN_2 / half_lives[i]
        } else {
            0.0
        };
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_indices_are_dense_and_ordered() {
        for (i, ax) in ALL_AXES.iter().enumerate() {
            assert_eq!(ax.index(), i);
        }
    }

    #[test]
    fn half_life_spread_spans_at_least_nine_orders() {
        // This is not a style assertion. `docs/03` argues that the spread is
        // what forces a vector state, and `docs/05` §5.6 shows the same spread
        // is what makes the Lyapunov feasibility condition bind. If the spread
        // ever collapses, both arguments need revisiting, so the test fails
        // loudly rather than letting the docs quietly go stale.
        let hl = nominal_half_lives();
        let min = hl.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = hl.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (max / min).log10() >= 9.0,
            "half-life spread collapsed to {} orders",
            (max / min).log10()
        );
    }

    #[test]
    fn rates_are_positive_and_ordered_inversely_to_half_lives() {
        let rates = rates_from_half_lives(&nominal_half_lives());
        assert!(rates.iter().all(|&r| r > 0.0));
        assert!(rates[Axis::Tempo.index()] > rates[Axis::Authority.index()]);
        assert!(rates[Axis::Authority.index()] > rates[Axis::Irreversibility.index()]);
    }

    #[test]
    fn only_irreversibility_is_permanent() {
        let permanent: Vec<_> = ALL_AXES.iter().filter(|a| a.is_permanent()).collect();
        assert_eq!(permanent, vec![&Axis::Irreversibility]);
    }
}
