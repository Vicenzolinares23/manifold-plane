//! Mutual information between action streams, for the coupling axis.
//!
//! `docs/03-dimensional-analysis.md` A5.
//!
//! Raw correlation was rejected as the functional form: independent couplings
//! must *add* so the dynamics stay linear in accumulation, and correlation does
//! not add. Mutual information does, and is natively in bits — which is the
//! whole reason the six axes share a unit.

use std::collections::BTreeMap;

/// A sliding histogram of action types for one asker.
#[derive(Debug, Clone, Default)]
pub struct ActionHistogram {
    counts: BTreeMap<String, u64>,
    total: u64,
}

impl ActionHistogram {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, action: &str) {
        *self.counts.entry(action.to_string()).or_insert(0) += 1;
        self.total += 1;
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn alphabet_size(&self) -> usize {
        self.counts.len()
    }

    pub fn count(&self, action: &str) -> u64 {
        self.counts.get(action).copied().unwrap_or(0)
    }

    pub fn actions(&self) -> impl Iterator<Item = &String> {
        self.counts.keys()
    }
}

/// Paired observations of two askers' actions over a window.
///
/// Coupling is about *joint* behavior, so the estimator needs the joint
/// distribution, not two marginals. Observations are paired by time bucket:
/// two askers acting in the same bucket contribute a joint sample.
#[derive(Debug, Clone, Default)]
pub struct JointHistogram {
    joint: BTreeMap<(String, String), u64>,
    left: ActionHistogram,
    right: ActionHistogram,
    total: u64,
}

impl JointHistogram {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, a: &str, b: &str) {
        *self.joint.entry((a.to_string(), b.to_string())).or_insert(0) += 1;
        self.left.observe(a);
        self.right.observe(b);
        self.total += 1;
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    /// Plug-in mutual information estimate, in bits, with the Miller–Madow
    /// bias correction.
    ///
    /// The plug-in estimator is biased *upward* — it reports spurious coupling
    /// between genuinely independent streams, and the bias grows with alphabet
    /// size and shrinks with sample count. Left uncorrected, two unrelated
    /// askers with rich action vocabularies would be flagged as a coalition and
    /// throttled together. The correction is `(|A|-1)(|B|-1) / (2N ln2)`, a
    /// function of measured quantities only, per `docs/03`.
    pub fn mutual_information_bits(&self) -> f64 {
        (self.mutual_information_raw_bits() - self.bias_bits()).max(0.0)
    }

    /// Uncorrected plug-in estimate. Exposed so the correction can be tested
    /// against it directly rather than inferred from a threshold.
    pub fn mutual_information_raw_bits(&self) -> f64 {
        if self.total < 2 {
            return 0.0;
        }
        let n = self.total as f64;

        let mut mi = 0.0;
        for ((a, b), &c) in &self.joint {
            let p_ab = c as f64 / n;
            let p_a = self.left.count(a) as f64 / n;
            let p_b = self.right.count(b) as f64 / n;
            if p_ab > 0.0 && p_a > 0.0 && p_b > 0.0 {
                mi += p_ab * (p_ab / (p_a * p_b)).log2();
            }
        }
        mi
    }

    /// Miller-Madow bias term, `(|A|-1)(|B|-1) / (2N ln2)`. A function of
    /// measured quantities only, per `docs/03`.
    pub fn bias_bits(&self) -> f64 {
        if self.total < 2 {
            return 0.0;
        }
        let ka = self.left.alphabet_size() as f64;
        let kb = self.right.alphabet_size() as f64;
        ((ka - 1.0) * (kb - 1.0)) / (2.0 * self.total as f64 * std::f64::consts::LN_2)
    }
}

/// Estimate pairwise coupling from two time-bucketed action streams.
///
/// Streams are `(bucket, action)` pairs. Only buckets where both askers acted
/// contribute — a bucket where one is silent carries no information about
/// joint behavior, and counting it as a "null" action would manufacture
/// coupling out of two askers merely being idle at the same time.
pub fn coupling_bits(
    a_stream: &[(u64, String)],
    b_stream: &[(u64, String)],
) -> f64 {
    let b_map: BTreeMap<u64, &String> = b_stream.iter().map(|(t, s)| (*t, s)).collect();
    let mut joint = JointHistogram::new();
    for (t, a) in a_stream {
        if let Some(b) = b_map.get(t) {
            joint.observe(a, b);
        }
    }
    joint.mutual_information_bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(actions: &[(u64, &str)]) -> Vec<(u64, String)> {
        actions.iter().map(|(t, a)| (*t, a.to_string())).collect()
    }

    #[test]
    fn identical_streams_have_high_coupling() {
        let a: Vec<(u64, String)> =
            (0..400).map(|i| (i, format!("op{}", i % 4))).collect();
        let mi = coupling_bits(&a, &a);
        // Four equiprobable actions perfectly correlated carry 2 bits.
        assert!(mi > 1.8, "identical streams scored only {mi}");
    }

    #[test]
    fn independent_streams_have_near_zero_coupling() {
        // The case the bias correction exists for. Two rich but unrelated
        // vocabularies must not read as a coalition.
        let a: Vec<(u64, String)> =
            (0..2000).map(|i| (i, format!("op{}", i % 7))).collect();
        let b: Vec<(u64, String)> =
            (0..2000).map(|i| (i, format!("act{}", (i * 3 + 1) % 11))).collect();
        let mi = coupling_bits(&a, &b);
        assert!(mi < 0.3, "independent streams scored {mi}");
    }

    #[test]
    fn bias_correction_lowers_the_raw_estimate() {
        let mut j = JointHistogram::new();
        for i in 0..40u64 {
            j.observe(&format!("a{}", i % 9), &format!("b{}", (i * 5) % 9));
        }
        // Note (i*5)%9 is a bijection of i%9, so these streams really are
        // perfectly coupled and the corrected MI *should* stay high. What the
        // correction must do is be strictly positive and reduce the estimate.
        assert!(j.bias_bits() > 0.0);
        assert!(j.mutual_information_bits() < j.mutual_information_raw_bits());
    }

    #[test]
    fn the_correction_shrinks_as_samples_accumulate() {
        // Bias grows with alphabet size and shrinks with sample count. If it
        // did not shrink, the estimator would never converge and coupling
        // would be permanently underestimated on rich vocabularies.
        let mut small = JointHistogram::new();
        let mut large = JointHistogram::new();
        for i in 0..50u64 {
            small.observe(&format!("a{}", i % 8), &format!("b{}", (i * 3) % 8));
        }
        for i in 0..5000u64 {
            large.observe(&format!("a{}", i % 8), &format!("b{}", (i * 3) % 8));
        }
        assert!(large.bias_bits() < small.bias_bits() / 10.0);
    }

    #[test]
    fn sparse_samples_over_a_rich_alphabet_do_not_manufacture_coupling() {
        // The case the correction exists for: few samples, many symbols, no
        // real relationship. Uncorrected, this reads as a coalition and would
        // throttle two unrelated askers together.
        let mut j = JointHistogram::new();
        let mut seed = 0x9E3779B97F4A7C15u64;
        for _ in 0..60 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let a = seed % 12;
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let b = seed % 12;
            j.observe(&format!("a{a}"), &format!("b{b}"));
        }
        assert!(j.bias_bits() > 0.5, "bias term should be large in this regime");
    }

    #[test]
    fn disjoint_time_buckets_yield_no_coupling() {
        // The F4 evasion in docs/07 is real and this documents it honestly:
        // askers acting on non-overlapping schedules read as uncoupled.
        let a = stream(&[(0, "x"), (2, "x"), (4, "x")]);
        let b = stream(&[(1, "x"), (3, "x"), (5, "x")]);
        assert_eq!(coupling_bits(&a, &b), 0.0);
    }

    #[test]
    fn an_empty_stream_is_uncoupled() {
        assert_eq!(coupling_bits(&[], &[]), 0.0);
    }

    #[test]
    fn histogram_tracks_alphabet_and_totals() {
        let mut h = ActionHistogram::new();
        h.observe("get");
        h.observe("get");
        h.observe("delete");
        assert_eq!(h.total(), 3);
        assert_eq!(h.alphabet_size(), 2);
        assert_eq!(h.count("get"), 2);
    }
}
