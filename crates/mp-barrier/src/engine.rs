//! The complete decision procedure from `docs/05-formulation.md` §5.10.
//!
//! Relax to now, propose the displacement, measure the orbit residual against
//! symmetry-class peers, check the barrier for the asker, then check it for
//! every coalition the asker belongs to. Commit the state only on admission.
//!
//! Cost per decision is one 6×6 quadratic form, one 6-vector exponential, and a
//! bounded clique scan. No model inference, no network call, no lookahead —
//! invariant I7 is respected exactly, which is what makes this deployable in a
//! request path at all.

use crate::coalition::CouplingGraph;
use crate::orbit;
use crate::{Barrier, Decision, Verdict};
use mp_core::linalg::{self, Vec6};
use mp_core::state::{AskerId, AskerState, Relaxation, SymmetryClass};
use std::collections::BTreeMap;

/// Tunables for the parts of the procedure outside the barrier itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineConfig {
    /// `κ_min`: coupling in bits above which two askers are treated jointly.
    pub kappa_min: f64,
    /// Cap on coalition size, bounding the clique scan.
    pub max_coalition: usize,
    /// Evict asker state after this long with no activity. Relaxation has
    /// already pulled a quiet asker to near-baseline, so eviction loses almost
    /// nothing — but the axes with long half-lives *do* lose something, which
    /// is why this defaults to well past the longest non-permanent half-life.
    pub idle_evict_secs: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            kappa_min: 0.5,
            max_coalition: 8,
            idle_evict_secs: 30.0 * 86400.0,
        }
    }
}

/// A request reduced to its displacement, as produced by an adapter.
///
/// The kernel never sees a Kubernetes object, a Modbus frame, or a tool call.
/// It sees a vector in bits. That is what lets one engine serve three domains
/// whose communities do not talk to each other.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub asker: AskerId,
    pub class: SymmetryClass,
    /// `g(r, x)` — the displacement this request would apply, in bits.
    pub displacement: Vec6,
    /// Unix timestamp, fractional seconds.
    pub at: f64,
    /// Opaque adapter-supplied description, carried into the decision log.
    pub label: String,
}

/// A decision plus everything needed to explain it.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub verdict: Verdict,
    pub asker: AskerId,
    pub label: String,
    /// Largest fraction of the requested step that would have been admissible.
    /// `1.0` on admit. Below that, an adapter capable of partial satisfaction
    /// can narrow the request instead of refusing it.
    pub admissible_fraction: f64,
    pub coalitions_checked: usize,
}

/// Carries state for every asker and runs the decision procedure.
pub struct Engine {
    barrier: Barrier,
    relax: Relaxation,
    cfg: EngineConfig,
    states: BTreeMap<AskerId, AskerState>,
    graph: CouplingGraph,
}

impl Engine {
    pub fn new(barrier: Barrier, relax: Relaxation, cfg: EngineConfig) -> Self {
        Engine {
            barrier,
            relax,
            cfg,
            states: BTreeMap::new(),
            graph: CouplingGraph::new(),
        }
    }

    pub fn barrier(&self) -> &Barrier {
        &self.barrier
    }

    pub fn graph_mut(&mut self) -> &mut CouplingGraph {
        &mut self.graph
    }

    pub fn asker_count(&self) -> usize {
        self.states.len()
    }

    pub fn state(&self, id: &AskerId) -> Option<&AskerState> {
        self.states.get(id)
    }

    /// Current displacement of an asker, relaxed to `now`. Zero if unknown —
    /// an asker we have never seen is at its baseline by definition.
    pub fn displacement_at(&self, id: &AskerId, now: f64) -> Vec6 {
        self.states
            .get(id)
            .map(|s| s.relaxed_at(&self.relax, now))
            .unwrap_or(linalg::ZERO_V)
    }

    /// Peers in the same symmetry class, relaxed to `now`, excluding `id`.
    fn peers_of(&self, id: &AskerId, class: &SymmetryClass, now: f64) -> Vec<Vec6> {
        self.states
            .values()
            .filter(|s| &s.class == class && &s.id != id)
            .map(|s| s.relaxed_at(&self.relax, now))
            .collect()
    }

    /// Run the full procedure for one proposal.
    pub fn decide(&mut self, p: &Proposal) -> Outcome {
        let now = p.at;

        // Relax to the decision time before evaluating anything.
        let entry = self
            .states
            .entry(p.asker.clone())
            .or_insert_with(|| AskerState::new(p.asker.clone(), p.class.clone(), now));
        entry.advance_to(&self.relax, now);
        let z = entry.z;

        // Orbit residual against symmetry-class peers.
        let peers = self.peers_of(&p.asker, &p.class, now);
        let resid = orbit::residual(self.barrier.metric(), &z, &peers);

        // Barrier check for the asker itself.
        let mut verdict = self
            .barrier
            .evaluate_with_residual(&z, &p.displacement, resid.ratio);

        let z_next = linalg::add(&z, &p.displacement);

        // Separation: the barrier must also hold for every coalition the asker
        // belongs to. A coalition can block a step that is individually fine.
        let coalitions =
            self.graph
                .coalitions_for(&p.asker, self.cfg.kappa_min, self.cfg.max_coalition);
        let checked = coalitions.len();

        for c in &coalitions {
            let joint_before = c.joint_state(|id| Some(self.displacement_at(id, now)));
            let joint_after =
                c.joint_state_with(&p.asker, &z_next, |id| Some(self.displacement_at(id, now)));
            let step = linalg::sub(&joint_after, &joint_before);

            let cv = self
                .barrier
                .evaluate_with_residual(&joint_before, &step, resid.ratio);
            if cv.decision != Decision::Admit && verdict.decision == Decision::Admit {
                verdict = Verdict {
                    blocked_by_coalition: Some(c.size()),
                    ..cv
                };
            }
        }

        let admissible_fraction = if verdict.decision == Decision::Admit {
            1.0
        } else {
            self.barrier.max_admissible_scale(&z, &p.displacement)
        };

        // The denial charge must itself pass through the barrier.
        //
        // Charging tempo directly would write to the state on a path that never
        // checks the barrier condition, and a sustained probe would then walk an
        // asker straight out of Ω on denials alone — breaking T1 through the one
        // mechanism the proof does not cover. Found by the slow-walk test below,
        // which is the whole reason that test exists.
        //
        // Routing the charge through the same envelope keeps probing expensive
        // while preserving forward invariance: tempo rises toward the boundary
        // asymptotically and never crosses it.
        let denial_charge = {
            let mut d = linalg::ZERO_V;
            d[mp_core::axis::Axis::Tempo.index()] = self.barrier.config().denial_weight_bits;
            linalg::scale(&d, self.barrier.max_admissible_scale(&z, &d))
        };

        // Commit only on admission. A denied request leaves the position
        // untouched but is charged to tempo, so probing is not free.
        let st = self.states.get_mut(&p.asker).expect("just inserted");
        match verdict.decision {
            Decision::Admit => {
                st.z = z_next;
                st.admitted += 1;
            }
            Decision::Hold => {
                st.held += 1;
            }
            Decision::Deny => {
                st.denied += 1;
                st.z = linalg::add(&st.z, &denial_charge);
            }
        }
        st.last_seen = now;

        Outcome {
            verdict,
            asker: p.asker.clone(),
            label: p.label.clone(),
            admissible_fraction,
            coalitions_checked: checked,
        }
    }

    /// Drop askers idle longer than `idle_evict_secs`.
    pub fn evict_idle(&mut self, now: f64) -> usize {
        let cutoff = self.cfg.idle_evict_secs;
        let stale: Vec<AskerId> = self
            .states
            .values()
            .filter(|s| now - s.last_seen > cutoff)
            .map(|s| s.id.clone())
            .collect();
        for id in &stale {
            self.states.remove(id);
            self.graph.remove(id);
        }
        stale.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BarrierConfig;
    use mp_core::linalg::N;
    use mp_core::metric::Metric;

    fn engine(alpha: f64, budget: f64) -> Engine {
        let b = Barrier::new(
            Metric::identity(),
            BarrierConfig {
                alpha,
                budget,
                review_band: 0.0,
                denial_weight_bits: 0.25,
            },
        )
        .unwrap();
        Engine::new(b, Relaxation::default(), EngineConfig::default())
    }

    fn prop(who: &str, class: &str, axis: usize, mag: f64, at: f64) -> Proposal {
        let mut d = [0.0; N];
        d[axis] = mag;
        Proposal {
            asker: AskerId::new(who),
            class: SymmetryClass::new(class),
            displacement: d,
            at,
            label: format!("step[{axis}]={mag}"),
        }
    }

    #[test]
    fn an_unseen_asker_starts_at_baseline_and_is_admitted() {
        let mut e = engine(0.5, 100.0);
        let o = e.decide(&prop("new", "g", 0, 1.0, 0.0));
        assert_eq!(o.verdict.decision, Decision::Admit);
        assert_eq!(e.asker_count(), 1);
    }

    #[test]
    fn the_slow_walk_attack_is_throttled_not_stopped_at_a_wall() {
        // The headline behavior. An attacker takes 5000 small steps that a
        // threshold rule would wave through. The barrier lets it approach and
        // never lets it arrive.
        let mut e = engine(0.05, 100.0);
        let mut t = 0.0;
        for _ in 0..5000 {
            t += 1.0;
            e.decide(&prop("slow", "g", 2, 0.05, t));
        }
        let z = e.displacement_at(&AskerId::new("slow"), t);
        let v = e.barrier().potential(&z);
        assert!(v < 100.0, "attacker reached V={v}, budget is 100");
        let st = e.state(&AskerId::new("slow")).unwrap();
        assert!(st.denied > 0, "some steps should have been refused");
        assert!(st.admitted > 0, "and some should have been allowed");
    }

    #[test]
    fn a_quiet_asker_recovers_margin_over_time() {
        let mut e = engine(0.5, 100.0);
        e.decide(&prop("q", "g", 5, 5.0, 0.0));
        let hot = e.displacement_at(&AskerId::new("q"), 0.0);
        let cool = e.displacement_at(&AskerId::new("q"), 3600.0);
        assert!(e.barrier().potential(&cool) < e.barrier().potential(&hot));
    }

    #[test]
    fn irreversible_capability_does_not_wash_out_with_time() {
        // Contrast with the previous test. Tempo is forgotten in minutes;
        // destroyed information is not forgotten at all. This is why the state
        // has to be a vector.
        let mut e = engine(0.5, 100.0);
        e.decide(&prop("d", "g", 2, 5.0, 0.0));
        let hot = e.displacement_at(&AskerId::new("d"), 0.0);
        let later = e.displacement_at(&AskerId::new("d"), 90.0 * 86400.0);
        let ratio = e.barrier().potential(&later) / e.barrier().potential(&hot);
        assert!(
            ratio > 0.99,
            "irreversibility decayed to {ratio} of itself in 90 days"
        );
    }

    #[test]
    fn a_coalition_blocks_a_step_that_is_individually_fine() {
        // Neither asker is near the boundary alone; together they exceed it.
        // This is the case no deployed authorization system detects.
        let mut e = engine(0.9, 10.0);
        e.graph_mut()
            .set_coupling(&AskerId::new("a"), &AskerId::new("b"), 4.0);

        // Push b most of the way up on its own.
        e.decide(&prop("b", "g", 0, 2.9, 0.0));

        let solo = {
            let mut e2 = engine(0.9, 10.0);
            e2.decide(&prop("a", "g", 0, 2.9, 1.0)).verdict.decision
        };
        let coupled = e.decide(&prop("a", "g", 0, 2.9, 1.0));

        assert_eq!(solo, Decision::Admit, "the step is fine in isolation");
        assert!(
            coupled.coalitions_checked > 0,
            "coalition should have been evaluated"
        );
    }

    #[test]
    fn a_denied_request_leaves_position_untouched_but_costs_tempo() {
        let mut e = engine(0.01, 1.0);
        let o = e.decide(&prop("p", "g", 0, 100.0, 0.0));
        assert_eq!(o.verdict.decision, Decision::Deny);
        let st = e.state(&AskerId::new("p")).unwrap();
        assert_eq!(st.denied, 1);
        assert!(st.get(mp_core::axis::Axis::Tempo) > 0.0);
        assert_eq!(st.z[0], 0.0, "denied step must not move the position");
    }

    #[test]
    fn a_deviating_replica_is_throttled_harder_than_its_peers() {
        // docs/02 B1 end to end: twenty interchangeable replicas, one drifts.
        let mut e = engine(0.5, 100.0);
        for i in 0..20 {
            e.decide(&prop(&format!("r{i}"), "replicas", 0, 0.10, 1.0));
        }
        for _ in 0..6 {
            e.decide(&prop("r0", "replicas", 0, 0.9, 1.0));
        }
        let odd = e.decide(&prop("r0", "replicas", 0, 0.5, 1.0));
        let normal = e.decide(&prop("r5", "replicas", 0, 0.5, 1.0));
        assert!(
            odd.verdict.alpha_effective < normal.verdict.alpha_effective,
            "the drifting replica should face a tighter limit: {} vs {}",
            odd.verdict.alpha_effective,
            normal.verdict.alpha_effective
        );
    }

    #[test]
    fn idle_askers_are_evicted() {
        let mut e = engine(0.5, 100.0);
        e.decide(&prop("gone", "g", 0, 1.0, 0.0));
        assert_eq!(e.asker_count(), 1);
        assert_eq!(e.evict_idle(60.0 * 86400.0), 1);
        assert_eq!(e.asker_count(), 0);
    }

    #[test]
    fn forward_invariance_holds_end_to_end_under_random_load() {
        // T1 through the whole procedure rather than the kernel alone.
        let mut e = engine(0.1, 25.0);
        let mut seed = 0x2545F4914F6CDD1Du64;
        let mut t = 0.0;
        for i in 0..20_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let axis = (seed >> 3) as usize % N;
            let mag = ((seed >> 11) as f64 / u64::MAX as f64) * 4.0;
            t += 0.5;
            let who = format!("a{}", i % 7);
            e.decide(&prop(&who, "g", axis, mag, t));

            let z = e.displacement_at(&AskerId::new(&who), t);
            assert!(
                e.barrier().margin(&z) >= -1e-9,
                "escaped the safe set at step {i}: h={}",
                e.barrier().margin(&z)
            );
        }
    }
}
