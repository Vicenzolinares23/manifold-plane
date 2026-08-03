# Stage 6 — Proof strategy and results

Notation from `05-formulation.md`. `h(x) = c - V(x)`, `V(x) = ‖x-x₀‖²_M`,
`Ω = {h ≥ 0}`, admission rule `h(x⁺) ≥ (1-α)h(x)` with `α ∈ (0,1]`.

## T1 — Forward invariance of the safe set

**Theorem.** Let `x_0 ∈ Ω`. If every admitted transition satisfies the barrier
condition and every relaxation satisfies (★) from 5.6, then `x_k ∈ Ω` for all
`k ≥ 0`, and moreover

    h(x_k) ≥ (1-α)^k · h(x_0) ≥ 0.

*Proof.* Induction on `k`. Base case `k=0` holds since `x_0 ∈ Ω` gives `h ≥ 0`.
Inductive step: relaxation gives `h(x⁻_{k+1}) ≥ h(x_k)` by (★), since (★) makes `V`
non-increasing under `R_Δt`. The admitted displacement gives
`h(x_{k+1}) ≥ (1-α)h(x⁻_{k+1}) ≥ (1-α)h(x_k)`. With `α ≤ 1` the factor `(1-α)` is
non-negative, so by the inductive hypothesis `h(x_{k+1}) ≥ (1-α)^{k+1}h(x_0) ≥ 0`. ∎

**Why this is the result that matters.** The quantifier order is the point. It is
*for every adversary, for every request sequence, for every length*, not "for
sequences drawn from the distribution we trained on." The rule inspects one step and
constrains all futures. That is what Stage 1 demanded and what a per-request policy
cannot provide.

The proof is elementary. That is a feature: the safety argument of a security
control should be checkable by hand, and the entire burden lives in hypotheses that
`docs/07` attacks directly.

## T2 — Adversarial approach bound

**Theorem.** An adversary maximizing `V` under the admission rule, from `x_0`, after
`N` admitted requests, achieves at most

    V(x_N) ≤ c - (1-α)^N · (c - V(x_0))

with equality iff every step saturates the barrier condition and no relaxation
occurs. In particular `V(x_N) < c` strictly for every finite `N`.

*Proof.* Immediate from T1 with `h = c - V`, saturating each inequality. ∎

**Corollary (time-to-boundary).** To reach margin `h_target` from `h_0`:

    N ≥ ln(h_0 / h_target) / ln(1/(1-α))  ≈  (1/α)·ln(h_0/h_target)   for small α

**This is exactly the scaling law that Stage 3 derived from dimensional analysis
alone, before the dynamics were written.** Two independent routes agreeing is the
strongest evidence available that the formulation is not arbitrary. `sim/` measures
it numerically against a real optimizer as a third, adversarial check (`docs/07`).

**Operational reading.** `α` is not a knob to be tuned until the demo looks good.
Invert the corollary: an operator states "an attacker must need at least `N_min`
admitted requests to get from nominal to 99% of budget," and `α` follows:

    α ≤ 1 - (0.01·c / h_0)^{1/N_min}

That is the measurement procedure Stage 3 promised for `α`.

## T3 — Coalition safety

**Theorem.** If the barrier condition holds for every maximal clique `S` in the
coupling graph, and the weights `w_p` form a convex combination, then the coalition
state `z_S` satisfies T1 with the same constant `α`.

*Proof.* `V` is a quadratic form, hence convex. `z_S = Σ w_p z_p` with
`Σ w_p = 1, w_p ≥ 0`, so by Jensen `V(z_S) ≤ Σ w_p V(z_p)`. The barrier condition
applied directly to `z_S` (as the procedure in 5.10 does) then satisfies the
hypotheses of T1 with `x_S` in place of `x`. ∎

Convexity of `V` is doing real work: it is why summed capability can be bounded from
the coalition state without enumerating subsets of actions. Had we chosen a
non-convex potential, coalition safety would require combinatorial search and would
not be computable in the request path.

## T4 — The step-size envelope

**Proposition.** At state `x`, the admitted displacement `g` must satisfy

    ‖g‖²_M + 2⟨z, g⟩_M ≤ α·h(x)

*Proof.* `h(x+g) - h(x) = V(x) - V(x+g) = -(2zᵀMg + gᵀMg)`. Substituting into
`h(x+g) ≥ (1-α)h(x)` gives the result. ∎

Two readings. The admissible set is a ball in the `M`-metric, centered at `-z`, of
radius `√(V(x) + α·h(x))` — so it is genuinely a *sphere*, per-step, shrinking as
`h → 0`. And the cross-term `⟨z,g⟩_M` means steps pointing back toward baseline are
cheap or free: an asker that reduces its own capability is never throttled. That
behavior was not designed in. It fell out.

## T5 — What breaks without (★)

**Proposition.** If `ΛM + MΛ ⋡ 0`, there exists `z` and `Δt > 0` with
`V(R_Δt(z)) > V(z)`, and hence an asker issuing **no requests at all** can exit `Ω`.

*Proof.* By 5.6, (★) failing means `dP/dt ⋠ 0` at `t=0`, so some `z` has
`d/dt[zᵀP(t)z] > 0`. Integrate. ∎

T1's hypothesis is therefore not decorative. `mp-core` refuses to construct a metric
that violates (★), rather than accepting it and losing the theorem silently.

## Proof obligations discharged in code

| Result | Test |
|---|---|
| T1 | `mp-barrier` property test: random adversarial sequences never exit Ω |
| T2 | `sim/adversarial.py`: measured approach vs bound |
| T3 | `mp-barrier` coalition property test |
| T4 | `mp-barrier` unit test on the envelope |
| T5 | `mp-core` rejects infeasible (Λ, M) pairs |
| (★) | `mp-core::metric::project_feasible` + eigenvalue assertion |
