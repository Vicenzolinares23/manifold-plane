# Stage 5 — Formulation

Four stages of structure, no equations until now. Here is the model.

## 5.1 The state space

    x = (a, h, ι, ω, κ, τ) ∈ ℝ⁶     all components in bits (Stage 3)
    x₀ = per-asker baseline          the relaxation fixed point (Stage 2, N3)
    z = x - x₀                       displacement from baseline

## 5.2 The metric

Stage 2 (N1) forbade isotropy. The axes are not interchangeable, and the
deployment-specific content of the model is exactly *how* they are not.

    M ∈ ℝ⁶ˣ⁶,  M = M ᵀ ≻ 0
    ⟨u, v⟩_M = uᵀ M v
    ‖u‖_M = √(uᵀ M u)

`M` is fit as the **inverse covariance of benign trajectory displacements**:

    M = (Cov_benign[z])⁻¹     (shrinkage-regularized; see 5.8)

This makes `‖z‖_M` a Mahalanobis distance. Directions that benign traffic explores
freely are cheap; directions benign traffic never explores are expensive. Nobody
writes down "bridge resources are dangerous" — the off-diagonal `M_{ah}` term
produced by the benign `a`↔`h` correlation makes violating that correlation
expensive automatically (Stage 4).

## 5.3 The potential and the safe set

    V(x) = zᵀ M z = ‖x - x₀‖²_M        [bits²]
    Ω = { x : V(x) ≤ c }               the safe set
    c = Quantile_{0.999}( V over benign corpus )     [bits²]

`Ω` is the set `{ zᵀMz ≤ c }`: an **ellipsoid in raw coordinates, and exactly a
sphere of radius √c in the metric induced by `M`.** The original intuition — a
six-dimensional security sphere — is recovered, but as a *derived* object. It is a
sphere in the only geometry the system actually has.

The barrier function is the signed margin:

    h(x) = c - V(x)        h ≥ 0 ⟺ safe,  h = 0 ⟺ on the boundary

## 5.4 The dynamics

Between events, relaxation toward baseline (Stage 4):

    R_Δt(x) = x₀ + D_Δt · z,      D_Δt = exp(-Λ Δt),  Λ = diag(ln2 / T_½ⁱ)

On an approved request `r`, displacement:

    x⁺ = R_Δt(x) + g(r, R_Δt(x))

`g` is state-dependent (Stage 4): the same request is worth more from a position of
existing reach. The adapters in `crates/mp-adapters` implement `g` per domain; the
kernel never sees a request, only its displacement.

Continuous limit (Stage 4, discharging C1):

    ẋ = -Λ(x - x₀) + r·ḡ(x)

## 5.5 The admission rule

This is the resolution of the Stage 1 tension. **I6** says danger lives in
sequences; **I7** forbids lookahead. The rule that satisfies both is a
**discrete-time exponential control barrier function** condition:

    ADMIT  r  ⟺  h(x⁺) ≥ (1 - α) · h(x)          α ∈ (0, 1]

Equivalently: `h(x⁺) - h(x) ≥ -α·h(x)`. The margin may shrink, but only by a
fraction `α` of what remains.

**What this does that a threshold cannot.** A threshold rule (`admit iff V(x⁺) ≤ c`)
permits a full-speed walk right up to the boundary and admits the largest possible
step at every point. It is exactly the greedy rule the slow attacker wants.

The barrier condition instead makes the **admissible step size shrink in proportion
to the remaining margin**. Far from the boundary, `h` is large and big steps pass.
Near the boundary, `h → 0`, and the allowed displacement → 0. The attacker is not
stopped by a wall; the attacker is subjected to a *speed limit that tightens as it
approaches*, asymptotically to zero.

Because the constraint is on the step relative to the current margin, and because it
is enforced at every step, it says something about **every possible infinite future
sequence** while examining only one step from one position. That is the whole trick,
and Stage 6 proves it.

**This also closes the leash Stage 0 put on the ATC analogy.** The objection was
that aircraft have bounded motion and requests do not — an asker can request
anything at any time. Correct, and the barrier rule does not assume otherwise. It
*manufactures* the bound. Requests are unbounded; admitted requests are not. The
analogy survives because we supplied the missing physics rather than assuming it.

### Three outcomes, not two

Binary admit/deny is wrong for the LLM-agent domain and often wrong elsewhere:

    h(x⁺) ≥ (1-α)h(x)                    → ADMIT
    h(x⁺) ≥ (1-α)h(x) - δ,  h(x⁺) > 0    → HOLD  (escalate to human / step-up auth)
    otherwise                             → DENY

`δ` is the review band, expressed as a fraction of `c`. A denial does not change
`x`, but it *is* an event: denials feed `τ` as a weighted event class (Stage 4),
so probing is not free.

## 5.6 Relaxation is not automatically safe — a real constraint on M and Λ

An easy error here, worth stating because the obvious claim is false.

One wants to say: relaxation decays toward baseline, so it decreases `V`, so it
can only help. **This is not true for non-diagonal `M`.** A diagonal contraction can
*increase* a quadratic form when the form has off-diagonal structure. Concretely,
with

    M = [[1, -0.99], [-0.99, 1]],  z = (1,1),  D = diag(1, 0.5)

we get `V(z) = 0.02` but `V(Dz) = 0.26` — a thirteen-fold increase from pure decay.
If that goes unnoticed, an asker can drift out of `Ω` while making no requests at
all, and every safety claim in Stage 6 is void.

**Condition.** `V(R_Δt(x)) ≤ V(x)` for all `x` and all `Δt ≥ 0` **iff**

    Λ M + M Λ ⪰ 0                                    (★)

*Proof.* Let `P(t) = D_t M D_t`. Since `D_t` and `Λ` are both diagonal they commute,
so `dP/dt = -(Λ P + P Λ) = -D_t (Λ M + M Λ) D_t`. Congruence by the invertible `D_t`
preserves positive semidefiniteness, so `dP/dt ⪯ 0` for all `t` iff `ΛM + MΛ ⪰ 0`.
`V(R_t(x)) = zᵀP(t)z`, monotone non-increasing in `t` exactly under that condition. ∎

Entrywise, `(ΛM + MΛ)_{ij} = (λᵢ + λⱼ) M_{ij}`. For diagonal `M` this is `2ΛM ⪰ 0`,
always true. Trouble comes only from off-diagonal terms coupling axes with very
different rates — and Stage 3 measured a **nine-order-of-magnitude spread** in the
half-lives. So (★) is a *binding* constraint in this system, not a formality. The
same half-life spread that justified a vector state is what makes the geometry
delicate.

**Enforcement.** After fitting `M` from data, project onto the feasible cone:

    M ← argmin_{M'} ‖M' - M‖_F   s.t.  M' ≻ 0,  Λ M' + M' Λ ⪰ 0

Implemented by alternating projections (`mp-core::metric::project_feasible`). The
projection is reported: if it moves `M` substantially, the fitted correlation
structure is incompatible with the measured half-lives, and that is a finding about
the deployment worth surfacing rather than silently absorbing.

## 5.7 Coalitions — the actual air-traffic-control part

I8: askers are not independent. Two individually-safe trajectories can be jointly
unsafe. This is precisely separation minima: ATC does not ask whether each aircraft
is somewhere legal, it asks whether any *pair* will violate separation.

Build the coupling graph `G` on askers with edge weight `κ_{pq}` (pairwise mutual
information), threshold at `κ_min`. For each maximal clique `S` (capped at size
`s_max` for tractability), define the coalition state

    z_S = Σ_{p∈S} w_p · z_p ,     w_p = κ̄_p / Σ_{q∈S} κ̄_q

and require the barrier condition to hold for `S` as well:

    ADMIT r from p  ⟺  h(x⁺) ≥ (1-α)h(x)  for p  AND for every clique S ∋ p

Weighted summation of displacements is the right composition rule because the axes
are in bits (Stage 3) and bits of independently-acquired capability add. Two askers
each at half the budget, tightly coupled, exceed it together — and are stopped,
though neither is individually anywhere near the boundary.

This is the case no deployed authorization system detects, and it falls out of the
formulation rather than being bolted on.

## 5.8 Orbit residual — the free second detector

From Stage 2 (B1). For asker `p` in symmetry class `G`:

    ρ_p = ‖ x_p - median_{q∈G}(x_q) ‖_M
    σ_G = median_{q∈G} ‖ x_q - median(x_G) ‖_M          (MAD, in the M-metric)
    Π₄ = ρ_p / σ_G

Median and MAD rather than mean and standard deviation because the estimator must
survive a minority of compromised peers — the breakdown point matters here, since
the adversary is *in* the sample.

Π₄ is threshold-free in the sense that matters: it is measured in units of the
group's own present spread, so it needs no absolute calibration and does not drift
as workloads change. And unlike self-baseline anomaly detection, a slow attacker
cannot poison it — poisoning requires moving the peers, whom the adversary does not
control.

Π₄ enters the decision as a multiplier on the effective aggressiveness:

    α_eff = α / (1 + Π₄)

An asker that has drifted from its peers is held to a proportionally tighter speed
limit. No new threshold, no new constant.

## 5.9 Shrinkage for the covariance fit

`Cov_benign[z]` is a 6×6 estimate; with `n` trajectory samples it is well-conditioned
for `n ≫ 6`, but tail-sparse axes (`ι` is near-zero for the overwhelming majority of
benign requests) make the raw inverse unstable. Ledoit–Wolf shrinkage toward a
scaled identity:

    Σ̂ = (1-γ)·S + γ·(tr(S)/6)·I,   γ chosen by the Ledoit–Wolf estimator
    M = Σ̂⁻¹, then projected per (★)

`γ` is not tuned; it is computed in closed form from the sample.

## 5.10 The complete decision procedure

    on request r from asker p at time t:
      Δt   ← t - t_last(p)
      x⁻   ← R_Δt(x_p)                                  relax
      x⁺   ← x⁻ + g(r, x⁻)                              proposed displacement
      Π₄   ← orbit_residual(p)                          peer comparison
      α_eff← α / (1 + Π₄)
      ok   ← h(x⁺) ≥ (1-α_eff)·h(x⁻)                    self barrier
      for each coalition S ∋ p:
        ok ← ok ∧ [ h(x⁺_S) ≥ (1-α_eff)·h(x⁻_S) ]       separation
      decide ADMIT / HOLD / DENY per 5.5
      if ADMIT: x_p ← x⁺  else: x_p ← x⁻ ; τ_p += denial weight
      t_last(p) ← t

Cost per decision: one 6×6 quadratic form, one 6-vector exponential, and a bounded
clique scan. Sub-microsecond in Rust. No model inference, no network call, no
lookahead — I7 is respected exactly.

## 5.11 What is claimed and what is not

**Claimed:** if the asker starts inside `Ω` and every admitted request satisfies the
barrier condition, the asker's state never leaves `Ω` — for any request sequence, of
any length, chosen by any adversary. Proven in Stage 6.

**Not claimed:** that `Ω` contains only safe worlds. `Ω` is calibrated from benign
traffic; if the calibration corpus contains an ongoing compromise, `Ω` is wrong.
Garbage in, garbage out, and no amount of control theory fixes it. Mitigations and
their limits are in `docs/07`.

**Not claimed:** that displacement functions `g` are correct. They are the domain
adapters' responsibility and they are the softest part of the system. The kernel's
guarantee is conditional on them, and `docs/07` measures sensitivity to `g` error.
