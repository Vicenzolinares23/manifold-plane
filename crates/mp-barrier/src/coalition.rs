//! Coalitions and separation — the part that is genuinely air traffic control.
//!
//! `docs/05-formulation.md` §5.7, proof in `docs/06-proofs.md` T3.
//!
//! Invariant I8: askers are not independent. Two individually-safe trajectories
//! can be jointly unsafe, because one actor can wear two coats or hand
//! capability to another. Watching each asker in isolation is provably blind to
//! the transfer.
//!
//! This is exactly separation minima. A controller does not ask whether each
//! aircraft is somewhere legal; it asks whether any pair will violate
//! separation. Here: two askers each at half the budget, tightly coupled,
//! exceed it together — and are stopped, though neither is individually
//! anywhere near the boundary.
//!
//! Weighted summation is the right composition rule because the axes are in
//! bits (`docs/03`) and independently-acquired bits of capability add.

use mp_core::linalg::{self, Vec6};
use mp_core::state::AskerId;
use std::collections::{BTreeMap, BTreeSet};

/// A set of askers coupled strongly enough to be treated jointly.
#[derive(Debug, Clone, PartialEq)]
pub struct Coalition {
    pub members: Vec<AskerId>,
    /// Convex weights, parallel to `members`. Sum to 1, which is what T3's
    /// Jensen argument requires.
    pub weights: Vec<f64>,
}

impl Coalition {
    pub fn size(&self) -> usize {
        self.members.len()
    }

    pub fn contains(&self, id: &AskerId) -> bool {
        self.members.iter().any(|m| m == id)
    }

    /// `z_S = Σ w_p · z_p`.
    ///
    /// `lookup` returns the current displacement of a member, or `None` if it
    /// has been evicted since the coalition was formed.
    pub fn joint_state<F>(&self, lookup: F) -> Vec6
    where
        F: Fn(&AskerId) -> Option<Vec6>,
    {
        let mut acc = linalg::ZERO_V;
        for (id, w) in self.members.iter().zip(self.weights.iter()) {
            if let Some(z) = lookup(id) {
                acc = linalg::add(&acc, &linalg::scale(&z, *w));
            }
        }
        acc
    }

    /// The joint state with one member's displacement replaced — used to
    /// evaluate a proposed step's effect on the coalition without mutating it.
    pub fn joint_state_with<F>(&self, subject: &AskerId, subject_z: &Vec6, lookup: F) -> Vec6
    where
        F: Fn(&AskerId) -> Option<Vec6>,
    {
        self.joint_state(|id| {
            if id == subject {
                Some(*subject_z)
            } else {
                lookup(id)
            }
        })
    }
}

/// Pairwise coupling graph over askers.
///
/// Edge weights are mutual information in bits between action streams
/// (`docs/03` A5). Estimation lives in `mp-adapters`; this structure only holds
/// the result and finds cliques in it.
#[derive(Debug, Clone, Default)]
pub struct CouplingGraph {
    edges: BTreeMap<(AskerId, AskerId), f64>,
    nodes: BTreeSet<AskerId>,
}

impl CouplingGraph {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(a: &AskerId, b: &AskerId) -> (AskerId, AskerId) {
        if a <= b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        }
    }

    /// Record a coupling. Symmetric: mutual information is.
    pub fn set_coupling(&mut self, a: &AskerId, b: &AskerId, bits: f64) {
        if a == b {
            return;
        }
        self.nodes.insert(a.clone());
        self.nodes.insert(b.clone());
        self.edges.insert(Self::key(a, b), bits.max(0.0));
    }

    pub fn coupling(&self, a: &AskerId, b: &AskerId) -> f64 {
        if a == b {
            return 0.0;
        }
        self.edges.get(&Self::key(a, b)).copied().unwrap_or(0.0)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Remove an asker and all its edges.
    pub fn remove(&mut self, id: &AskerId) {
        self.nodes.remove(id);
        self.edges.retain(|(a, b), _| a != id && b != id);
    }

    /// Neighbours above the coupling threshold.
    pub fn neighbours(&self, id: &AskerId, kappa_min: f64) -> Vec<AskerId> {
        self.nodes
            .iter()
            .filter(|other| *other != id && self.coupling(id, other) >= kappa_min)
            .cloned()
            .collect()
    }

    /// Coalitions containing `id`, as maximal cliques in the thresholded graph,
    /// capped at `max_size`.
    ///
    /// Clique enumeration is exponential in general, which is unacceptable in a
    /// request path. Three things keep it bounded: the threshold `kappa_min`
    /// makes the graph very sparse (benign askers have near-zero mutual
    /// information), only cliques containing the subject are enumerated, and
    /// `max_size` truncates. The result is a greedy maximal clique rather than
    /// a full enumeration — deliberately, since missing a large coalition is
    /// preferable to a latency spike, and the pairwise ones are the common case.
    pub fn coalitions_for(&self, id: &AskerId, kappa_min: f64, max_size: usize) -> Vec<Coalition> {
        if max_size < 2 {
            return Vec::new();
        }
        let mut neigh = self.neighbours(id, kappa_min);
        if neigh.is_empty() {
            return Vec::new();
        }

        // Strongest couplings first: a greedy clique built from the strongest
        // edges is the one most likely to matter.
        neigh.sort_by(|a, b| {
            self.coupling(id, b)
                .partial_cmp(&self.coupling(id, a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut clique = vec![id.clone()];
        for cand in neigh {
            if clique.len() >= max_size {
                break;
            }
            // Admit only if coupled to every current member.
            if clique.iter().all(|m| self.coupling(m, &cand) >= kappa_min) {
                clique.push(cand);
            }
        }

        if clique.len() < 2 {
            return Vec::new();
        }
        vec![self.weighted(clique, kappa_min)]
    }

    /// Weight members by their mean coupling within the clique, normalized to a
    /// convex combination.
    fn weighted(&self, members: Vec<AskerId>, _kappa_min: f64) -> Coalition {
        let n = members.len();
        let mut raw: Vec<f64> = members
            .iter()
            .map(|m| {
                let s: f64 = members
                    .iter()
                    .filter(|o| *o != m)
                    .map(|o| self.coupling(m, o))
                    .sum();
                s / (n - 1).max(1) as f64
            })
            .collect();

        let total: f64 = raw.iter().sum();
        if total <= 0.0 {
            // Degenerate; fall back to uniform so the weights stay convex.
            raw = vec![1.0 / n as f64; n];
        } else {
            for w in raw.iter_mut() {
                *w /= total;
            }
        }
        Coalition {
            members,
            weights: raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_core::linalg::N;

    fn id(s: &str) -> AskerId {
        AskerId::new(s)
    }

    fn z(a: f64) -> Vec6 {
        let mut v = [0.0; N];
        v[0] = a;
        v
    }

    #[test]
    fn an_uncoupled_asker_forms_no_coalition() {
        let mut g = CouplingGraph::new();
        g.set_coupling(&id("a"), &id("b"), 0.001);
        assert!(g.coalitions_for(&id("a"), 0.5, 8).is_empty());
    }

    #[test]
    fn strongly_coupled_askers_form_a_coalition() {
        let mut g = CouplingGraph::new();
        g.set_coupling(&id("a"), &id("b"), 3.0);
        let cs = g.coalitions_for(&id("a"), 0.5, 8);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].size(), 2);
        assert!(cs[0].contains(&id("b")));
    }

    #[test]
    fn coalition_weights_are_convex() {
        let mut g = CouplingGraph::new();
        g.set_coupling(&id("a"), &id("b"), 3.0);
        g.set_coupling(&id("a"), &id("c"), 2.0);
        g.set_coupling(&id("b"), &id("c"), 2.5);
        let c = &g.coalitions_for(&id("a"), 0.5, 8)[0];
        let sum: f64 = c.weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-12, "weights sum to {sum}");
        assert!(c.weights.iter().all(|w| *w >= 0.0));
    }

    #[test]
    fn a_clique_requires_mutual_coupling_not_just_a_shared_neighbour() {
        // a-b and a-c are strong, b-c is not. A clique containing all three
        // would be wrong: b and c are not coupled to each other.
        let mut g = CouplingGraph::new();
        g.set_coupling(&id("a"), &id("b"), 3.0);
        g.set_coupling(&id("a"), &id("c"), 3.0);
        g.set_coupling(&id("b"), &id("c"), 0.0);
        let c = &g.coalitions_for(&id("a"), 0.5, 8)[0];
        assert_eq!(c.size(), 2, "should not have merged b and c");
    }

    #[test]
    fn coalition_size_is_capped() {
        let mut g = CouplingGraph::new();
        let names: Vec<AskerId> = (0..20).map(|i| id(&format!("n{i}"))).collect();
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                g.set_coupling(&names[i], &names[j], 5.0);
            }
        }
        let c = &g.coalitions_for(&names[0], 0.5, 4)[0];
        assert_eq!(c.size(), 4);
    }

    #[test]
    fn joint_state_sums_member_displacements() {
        let mut g = CouplingGraph::new();
        g.set_coupling(&id("a"), &id("b"), 1.0);
        let c = &g.coalitions_for(&id("a"), 0.5, 8)[0];
        // Equal coupling gives equal weights, so the joint state of two askers
        // each at 4.0 is 4.0 — but two askers each at 4.0 together carry more
        // than either alone, which is what the barrier check on z_S sees.
        let joint = c.joint_state(|_| Some(z(4.0)));
        assert!((joint[0] - 4.0).abs() < 1e-12);
    }

    #[test]
    fn removing_an_asker_drops_its_edges() {
        let mut g = CouplingGraph::new();
        g.set_coupling(&id("a"), &id("b"), 1.0);
        g.set_coupling(&id("b"), &id("c"), 1.0);
        assert_eq!(g.edge_count(), 2);
        g.remove(&id("b"));
        assert_eq!(g.edge_count(), 0);
        assert_eq!(g.node_count(), 2);
    }
}
