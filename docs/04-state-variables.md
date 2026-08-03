# Stage 4 — State-variable discovery

> New state variables are re-descriptions that make what's true easier to see.

Stage 3 measured six candidate quantities. This stage asks the harder question:
**are these the right state variables at all, and are they enough?**

A state variable earns its place only if knowing it, plus the incoming request, is
enough to compute the next state. Anything that fails that test is a derived
quantity masquerading as state, and carrying it is both wasteful and a source of
inconsistency.

## The test each candidate must pass

1. **Sufficiency.** Does `(x, r) → x'` close, or do we need history beyond `x`?
2. **Independence.** Is it recoverable from the others? If so, delete it.
3. **Distinct timescale.** Does it have its own half-life? (From I2 — two quantities
   with the same half-life that always move together are one quantity.)
4. **Measurability.** Stage 3 procedure exists. (All six passed.)
5. **Adversarial relevance.** Can an attacker's objective be expressed as moving it?
   If no attacker cares about it, it is telemetry, not state.

## Results

| Axis | Sufficient | Independent | Own timescale | Attacker cares | Verdict |
|---|---|---|---|---|---|
| `a` authority | ✓ | ✓ | hours–days | yes — the goal | **keep** |
| `h` reach | ✓ | ✓ (see below) | days | yes — lateral movement | **keep** |
| `ι` irreversibility | ✓ | ✓ | ∞ | yes — impact | **keep** |
| `ω` opacity | ✓ | ✓ | hours | yes — persistence | **keep** |
| `κ` coupling | ✗ → fixed | ✓ | minutes | yes — collusion | **keep, with care** |
| `τ` tempo | ✓ | ✓ | seconds | yes — exfil rate | **keep** |

### On `h` vs `a` — nearly dependent, but not

Reach and authority are strongly correlated: more permissions usually means more
reachable resources. The instinct is to collapse them.

**They must not be collapsed**, and the counterexample is the important case. A
single permission on a *bridge* resource — a service account token mounted into a
pod, a network route, an ICS gateway with two segments — adds ~1 bit of authority
and can add ten bits of reach. Conversely, a broad grant scoped to one namespace
adds many bits of authority and zero reach.

The correlation is high in benign traffic and *breaks* precisely on the escalation
step. Collapsing them would erase the signal exactly where it matters. This is a
general lesson: two axes correlated under benign traffic and decorrelated under
attack are the most valuable axes in the model, not redundant ones.

This is also what the metric tensor `M` in Stage 5 is for. Because `M` is fit as an
inverse covariance over benign trajectories, the strong benign correlation between
`a` and `h` produces a large off-diagonal term, which means the ellipsoid is
*narrow* in the direction that violates the correlation. Moving along benign
`a`↔`h` correlation is cheap; moving orthogonal to it is expensive. The geometry
learns "bridge resources are dangerous" without anyone encoding the concept.

### On `κ` — the one that failed sufficiency, and how it was fixed

Coupling is a *pairwise* quantity, but the state vector is *per-asker*. Mutual
information `I(A_p ; A_{-p})` cannot be updated from `(x_p, r)` alone — it depends
on other askers' streams.

Two options were considered:

- **Rejected:** lift the state to the joint space over all askers. Dimension
  `6n` for `n` askers, covariance estimation becomes hopeless, and it violates the
  Stage 1 constraint that the model not be indexed by an enumeration.
- **Adopted:** treat `κ` as a state component updated from a *shared, slowly
  varying* peer-group summary rather than from the full joint state. Each asker
  carries its own `κ`, computed against the group's action histogram, which is
  maintained once per group and read by all members.

This is a mean-field approximation. It is exact when peers are exchangeable (which
S1 asserts within an equivalence class) and degrades gracefully otherwise. The
approximation error is bounded by the within-group spread `σ_peer`, which is already
measured for Π₄ — so the model knows when its own approximation is getting bad. That
is worth more than exactness.

## Axes considered and rejected

**Sensitivity of data touched.** Fails independence: it is `ι` for reads (the
irreversibility of disclosure) plus `h` for scope. Adding it double-counts.

**Trust score / reputation.** Fails sufficiency and measurability. There is no
procedure that yields it from observables, which is exactly the fudge-term
diagnostic — it exists to absorb model error. Rejected on principle.

**Geographic or network origin.** Fails S1 relabeling under Stage 2 unless it enters
via the baseline. Belongs in `Reach_baseline`, not as an axis.

**Anomaly score from a learned classifier.** Fails independence (it is a function of
the others) and fails the symmetry requirement (learned features encode identity).
More importantly it would make the safety argument in Stage 6 unprovable, since a
neural function has no Lipschitz bound we can state honestly.

**Number of prior denials.** Tempting — it looks like probing. Fails distinct
timescale: it moves with `τ` and decays with it. Folded into tempo as a weighted
event class instead.

## The dynamics: how state evolves

Now the payoff. With six sufficient, independent, bit-valued components, the update
splits cleanly into two mechanisms — and the split is *forced* by I1 (approval only
pushes outward; only time pulls inward), not chosen.

**Relaxation.** Between events, each axis decays exponentially toward its baseline
at its own rate. Per-axis, not global — that is I2.

**Displacement.** An approved request pushes the state outward by an amount that
depends on the request *and on where the asker already is*.

That second dependence is not decoration. The same request means different things
from different positions: acquiring a credential when you already have broad reach
is worth more than acquiring it in isolation. State-dependent displacement is what
makes the composition problem (I6) live *inside* the dynamics rather than being
patched on afterward.

Formally, with `Λ = diag(1/T_½ⁱ · ln2)`:

    x(t⁻ᵢ₊₁) = x₀ + exp(-Λ Δt) · (x(tᵢ) - x₀)          relaxation
    x(tᵢ₊₁)  = x(t⁻ᵢ₊₁) + g(rᵢ, x(t⁻ᵢ₊₁))               displacement

The fixed point is `x₀`, the baseline — not the origin. That is N3 from Stage 2: the
space is a cone with a baseline fixed point, and an asker at rest returns to its own
normal, not to zero.

### Checking the rejected candidate C1

Stage 1 rejected "requests are discrete events" as an invariant, and required the
dynamics to degrade gracefully as sampling interval → 0. Check: as `Δt → 0` with
request rate `r`, the relaxation term → identity and displacements accumulate at
rate `r·ḡ`, giving the continuous limit

    ẋ = -Λ(x - x₀) + r·ḡ(x)

which is a well-posed ODE — an Ornstein–Uhlenbeck-like relaxation with a
state-dependent drive. The discrete model is its Euler discretization. No artifact
was baked in. C1's warning is discharged.

This continuous form is also what the ICS adapter should use directly, since setpoint
streams are genuinely continuous there, and it explains why the same model serves
all three domains: they differ only in sampling regime, not in structure.

## What we still do not have

We have a state space, a metric, units, and dynamics. We do **not** yet have an
admission rule, and the Stage 1 tension is untouched:

- **I6:** danger lives in sequences.
- **I7:** we may examine only one step from one position, with no lookahead.

Everything so far is description. Stage 5 has to produce a decision rule that
constrains a single step and yet says something true about every possible infinite
future sequence.
