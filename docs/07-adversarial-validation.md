# Stage 7 — Adversarial validation

The first model of any real phenomenon is wrong in some specific, informative
way. This stage is the search for that way. It is a standing activity, not a
sign-off: findings are numbered and kept, including the ones that are still open.

## Threat model

The adversary:

- controls one or more askers, and knows it is being watched by this system;
- **knows the model, the code, the metric `M`, the budget `c`, and `α`** —
  Kerckhoffs applies, and a barrier whose safety depends on the attacker not
  knowing `α` would be worthless;
- can choose request content, magnitude, timing, and ordering freely;
- is patient — willing to spend weeks;
- cannot forge audit events for actions it did not take, and cannot modify the
  engine's carried state directly. If it can do either, it has already won by a
  route this system does not address, and saying so is more useful than
  pretending otherwise.

## Findings

### F1 — Denial charge escaped the safe set — **FIXED**

*Found by:* `engine::tests::the_slow_walk_attack_is_throttled_not_stopped_at_a_wall`.

The denial tempo charge (§5.5: "probing is not free") was applied directly to
the carried state, on a code path that never consulted the barrier condition. A
sustained probe therefore walked an asker out of `Ω` on **denials alone**,
measured at `V = 201` against a budget of `c = 100`.

T1 was not wrong. T1 covers admitted transitions and relaxation, and this was
neither — it was a third write path the proof never contemplated. The lesson
generalizes past this bug: **every write to the state must go through the
envelope, or the theorem's coverage has a hole shaped exactly like that write.**

*Fix:* the denial charge is scaled by `Barrier::max_admissible_scale`, the same
envelope every other displacement passes through. Probing stays expensive;
tempo now approaches the boundary asymptotically without crossing.

*Regression:* the test that found it, plus
`forward_invariance_holds_end_to_end_under_random_load` (20k randomized steps,
7 askers, all six axes).

### F2 — Baseline poisoning — **PARTIALLY MITIGATED, RESIDUAL RISK**

An adversary in control of an asker from the beginning shapes its own
`x₀`, `Ops_baseline`, `Reach_baseline`, and `r_baseline` during the calibration
window. Everything downstream is measured relative to those, so a compromise
present at calibration time is *defined* to be normal.

This is the sharpest limitation in the system and no amount of control theory
removes it. It is `docs/05` §5.11's "not claimed" made concrete.

*Mitigation:* the orbit residual (§5.8) does not use the asker's own history. It
compares against peers in the present, so poisoning it requires compromising a
majority of the symmetry class — and the median/MAD estimator means a *minority*
of compromised peers barely moves the reference (tested in `orbit.rs`).

*Residual risk:* an asker with no peers has no orbit residual, and falls back
entirely on its own baseline. Singleton symmetry classes are therefore the
weakest configuration, and a deployment that puts every asker in its own class
has silently disabled half the system. `mp-daemon` warns on singleton classes at
startup for this reason.

### F3 — Axis-shopping — **MITIGATED BY CONSTRUCTION**

An adversary that knows `M` picks the direction where `V` grows slowest per unit
of useful capability, and moves along it.

This is not a flaw, it is the design working. `M` is the inverse covariance of
benign traffic, so the cheapest directions are exactly the ones benign traffic
already occupies — moving there is, by construction, indistinguishable from
normal use and correspondingly low-value to an attacker. The expensive
directions are the ones benign traffic never explores.

*Residual:* the argument holds only as well as the benign corpus represents
reality. A corpus missing a legitimate but rare workload will price that
workload as an attack. `sim/calibration.py` reports the fraction of benign
variance captured, so under-representation is visible rather than silent.

### F4 — Coalition splitting — **PARTIALLY MITIGATED**

An adversary controlling many askers keeps pairwise mutual information below
`κ_min` by making them act on disjoint schedules, defeating the clique detection
in §5.7.

*Mitigation:* MI is estimated over a sliding window, and driving it near zero
requires genuinely uncorrelated behavior — which costs the adversary the
coordination that made multiple askers useful. There is a real trade here and
the adversary pays for it.

*Residual risk:* an adversary willing to accept full independence between its
askers defeats coalition detection entirely, and then each asker is bounded only
individually. `n` independent askers can therefore accumulate `n` budgets. This
is **open**, and it is the most important unsolved problem in the design.
Sketched direction: a per-symmetry-class aggregate budget, which bounds the
class regardless of how the adversary partitions its identities — but that
couples unrelated askers' fates and needs its own analysis before it goes in.

### F5 — Recalibration as an attack surface — **OPEN**

`M` and `c` are refit periodically from recent traffic. An adversary that has
been active during the fitting window shifts both in its favour, and because
refits are incremental the shift can be spread across many windows, each
individually unremarkable. This is the slow-walk attack applied to the
calibration procedure rather than to the request stream.

The system's own defence would be to apply a barrier condition to the *parameter*
trajectory `(M, c)` — the same mathematics one level up. That is the right
answer and it is not implemented. Currently `mp-daemon` refuses a refit that
moves `c` by more than a configured fraction per window, which is a blunt
version of the same idea and is honestly labelled as such in the config.

### F6 — Clock manipulation — **FIXED**

Relaxation depends on `Δt`. An adversary that influences timestamps supplies a
large `Δt` and relaxes its own state to baseline for free.

*Fix:* `Relaxation::relax` clamps `Δt` at zero, so backwards time is the identity
rather than an amplifier. Timestamps come from the interception point's clock,
never from the request. Tested by
`state::tests::negative_elapsed_time_is_clamped_not_amplified`.

*Residual:* an adversary that can move the *server's* clock forward still gets
free relaxation. That is a host compromise, and out of scope per the threat model.

### F7 — Hold-band flooding — **OPEN, LOW SEVERITY**

Requests landing in the review band (§5.5) escalate to a human. An adversary can
deliberately aim for that band to generate review fatigue, then slip a real
request through an inattentive approver.

Unfixed in the mathematics; this is a human-factors problem. `mp-daemon` rate-
limits holds per asker and per class, and collapses identical held requests, but
the underlying issue is real and belongs in operational guidance rather than in
the kernel.

## Validation that is not adversarial

**The scaling law.** `docs/03` predicted `N ~ (1/α)·ln(h₀/h_target)` from
dimensional analysis alone, before any dynamics existed. `docs/06` T2 derived
the same expression from the barrier condition. `sim/adversarial.py` measures it
with a numerical optimizer that is free to choose direction and magnitude at
every step. Three independent routes; agreement to numerical tolerance.

Had the measured scaling come out polynomial rather than logarithmic, the
formulation would have been wrong, and this was pre-registered as such in Stage 3
rather than being noticed afterward and explained away.

**Unit consistency.** Every axis in bits, `V` in bits², `c` in bits², `α` and all
Π-groups dimensionless. Checked in `sim/units.py`.

**Benign false-positive rate.** Measured on held-out benign trajectories. Since
`c` is calibrated at the 0.999 quantile of `V`, the expected rate is ~0.1% *by
construction* — which makes it a check that calibration ran correctly, not
evidence that the model is good. Reported as such.

## What would falsify the whole approach

Stated in advance, because a model that cannot be falsified is not doing work:

1. Real benign traffic whose displacement distribution is so heavy-tailed that no
   `c` separates it from attack traffic. The ellipsoid would be the wrong shape,
   and the fix would be a different potential, not a different constant.
2. Attack sequences whose per-step displacement is genuinely indistinguishable
   from benign on all six axes. Then the axes are incomplete and Stage 4 has to
   be reopened for a seventh.
3. Half-lives that turn out not to be separable — a single timescale governing
   everything — which would mean the vector state was unnecessary and a scalar
   score would have done.

None of the three is ruled out by anything in this repository. They are ruled out
or in by data from a real deployment, which is the next piece of work.

---

## Findings from implementation

F1–F7 came from the derivation. These two came from running it, and both were
invisible until a metric was fit from actual data — the identity metric used
during development hid them completely. That is worth noting on its own: a
security control validated only against its own defaults is validated against
the case that cannot fail.

### F8 — Iterative Lyapunov projection is numerically unusable — **FIXED**

*Found by:* `sim/calibration.py` on the first fitted metric.

`docs/05` §5.6 specified enforcing `ΛM + MΛ ⪰ 0` by alternating projections,
mapping through `L⁻¹`. `L⁻¹` scales entry `(i,j)` by `1/(λᵢ+λⱼ)`. With
half-lives spanning nine orders (`docs/03`), the slow-slow denominator is about
`4×10⁻¹³`, so the map amplifies by `~10¹²`. The result was a calibrated budget of
`c = 3.1×10⁸ bits²` and a diagonal condition number of `3.9×10⁸` — arithmetically
valid, geometrically meaningless, and no operator could reason about it.

The irony is exact: the nine-order spread that *justifies* a vector state
(`docs/01` I2) is what breaks the projection that keeps the vector state safe.

*Rejected first fix.* Bisecting a single global damping factor toward the
diagonal. Stable, and far too blunt — it crushed the authority–reach correlation,
which was never the problem, along with the irreversibility coupling, which was.
It left `M[a][h] = −0.007` and a near-isotropic geometry, discarding exactly the
structure `docs/04` identifies as carrying the escalation signal. Worth recording
because it *passed* every check that existed at the time; the checks were what
had to improve.

*Fix.* Closed form, single pass. Damp entry `(i,j)` by the ratio of the geometric
to the arithmetic mean of its two rates:

    C[i][j] = 2·√(λᵢλⱼ) / (λᵢ + λⱼ),     M' = M ∘ C

Then `ΛM' + M'Λ = 2·Λ^½ M Λ^½`, PSD by congruence; and `M' ⪰ 0` because `C` is a
Hadamard product of the rank-one kernel `√λᵢ√λⱼ` with the Cauchy kernel
`1/(λᵢ+λⱼ)`, both PSD, so Schur's product theorem applies.

**It also says something true rather than merely being convenient.** `C` is unity
on the diagonal and shrinks with rate disparity, so *an axis that never decays
cannot stay correlated with a fast one*. That is the actual content of the
Lyapunov condition, and it was not visible in the algebra of §5.6. Authority and
reach (rates within 4×) retain ~0.8 of their fitted correlation; irreversibility
and tempo (nine orders apart) are decorrelated entirely.

Post-fix calibration: `c = 111 bits²`, anisotropy 83×, and moving against the
benign authority–reach correlation costs 6× moving along it.

### F9 — Tail-sparse axes explode the inverse covariance — **FIXED**

Irreversibility is exactly zero for ~92% of benign requests. Its sample variance
approaches zero, and `M = Cov⁻¹` then assigns it an enormous eigenvalue. Ledoit–
Wolf shrinkage does not rescue this: the computed intensity was `γ = 0.0014`,
correctly small for a well-sampled covariance and useless against a near-
degenerate axis.

*Fix, and it is dimensional rather than numerical.* Every axis is a log-ratio of
counted things (`docs/03`), and counts have finite resolution — you cannot
resolve a displacement smaller than the smallest countable change. So the
covariance cannot legitimately be below the measurement quantum, and flooring the
diagonal at `(0.01 bits)²` is a statement about the instrument, not a
regularization knob. It therefore does not violate the no-constant-without-a-
procedure rule: the procedure is "measure your telemetry's resolution."

*Open residual.* The floor is currently a compiled-in default rather than a
measured per-deployment quantity. It should be derived from the actual counting
resolution of the telemetry source. Small, and real.
