# Stage 3 — Dimensional analysis

> Dimensional analysis finds truths that don't depend on your choice of units.

The discipline this stage enforces: **no constant enters the model without a
measurement procedure.** If we cannot say how to measure it in a real deployment,
it is a fudge term, and per the fudge-term diagnostic it means the structure is
wrong rather than the coefficient.

## The problem this stage solves

We have candidate quantities that are natively incommensurable:

- accumulated privilege — natively a *set of permitted operations*
- reach — natively a *count of resources*
- irreversibility — natively a *cost to restore*, in currency or in seconds
- opacity — natively a *fraction of behavior recorded*
- coupling — natively a *correlation*, dimensionless in [-1, 1]
- tempo — natively a *rate*, dimension 1/time

You cannot form a meaningful ellipsoid over axes with incompatible dimensions. The
expression `privilege² + rate²` is not a number; it is a category error. Any model
that writes it down has silently assumed a unit conversion it never stated, and the
conversion factor becomes an untunable fudge parameter.

Most "risk scoring" systems in production do exactly this. It is why their weights
never transfer between deployments and have to be re-tuned by hand forever. That
re-tuning burden is not an operational inconvenience; it is the diagnostic symptom
of a dimensional error.

## The resolution: everything is measured in bits

Every one of the six quantities can be written as the logarithm of a *ratio of
counts* or a *ratio of a quantity to its own baseline*. Logarithms of ratios are
dimensionless. Choosing base 2 names the unit: **bits**.

This is not a cosmetic rescaling. It is the claim that the natural content of each
axis is *how many binary choices' worth of capability has been accumulated*, and
that claim has to be checked axis by axis.

### A1 — Authority

Natively: the set `Ops(p)` of operations the asker can currently invoke.

Take the base-2 log of its cardinality:

    a = log2( |Ops(p)| / |Ops_baseline(p)| )

Dimensionless. In bits. Interpretation: how many doublings of the asker's operation
set have been accumulated beyond its established baseline. `a = 3` means the asker
can now do eight times as many distinct things as it normally can.

The baseline is *measured*, not chosen: it is the asker's own steady-state operation
set over a calibration window, or its peer group's median under S1.

*Why a log and not the raw count:* the raw count is not invariant under how finely
the API happens to be carved up. Splitting one verb into two doubles the count and
should not double the danger. Ratios against baseline cancel the carving; the log
makes accumulation additive so composition of steps adds rather than multiplies.

### A2 — Reach

Natively: the number of distinct resources reachable, where "reachable" means
attainable by a chain of currently-permitted operations. This is a closure over the
resource graph, not a direct-permission count — the difference is precisely what
privilege-escalation chains exploit.

    h = log2( |Reach(p)| / |Reach_baseline(p)| )

Bits. Same argument as A1.

### A3 — Irreversibility

From B2 in Stage 2, irreversibility is the failure of injectivity of the transition
map: how much information about the prior world state is destroyed.

    ι = log2( |preimages of the post-state under the applied operation| )

Bits, and this is a *literal* entropy, not an analogy — it is the number of bits
about the prior state that can no longer be recovered.

Measurement in practice, per operation class:

| Operation | Preimage structure | ι |
|---|---|---|
| Read | Bijective on world state | 0 |
| Idempotent write to a versioned store | Prior version retained | 0 (recoverable) |
| Overwrite, no versioning | All prior contents map here | log2(state space of that field) |
| Delete with tombstone + backup | Recoverable at cost | log2(restore cost / unit cost) |
| Hard delete | Unbounded preimage, capped at field entropy | high |
| ICS setpoint write on a physical process | Physical state not restorable | capped at process entropy |
| Outbound network send | Recipient's knowledge is not retractable | log2(recipients × payload entropy) |

Where the true entropy is unbounded we cap at the measured entropy of the field
from a sample of the data store. That cap is a measurement, not a guess.

The `restore cost / unit cost` form is exactly the B3 grant/revoke asymmetry ratio,
which is why B3 required no separate treatment: it *is* the irreversibility axis.

### A4 — Opacity

Natively: fraction of the asker's actions that leave a durable record.

    ω = -log2( recorded_fraction )

Bits. `ω = 0` under complete recording. Each bit is a halving of the evidence that
will survive. Unbounded as recording → 0, which is correct behavior: an asker that
has made itself fully unobservable should be at infinite opacity, and the numerics
in Stage 5 must handle the pole rather than clip it silently.

Measured directly by comparing emitted audit events against invoked operations —
both are countable at the interception point, so this needs no estimation at all.

### A5 — Coupling

Natively a correlation, already dimensionless in [0,1]. But raw correlation is the
wrong functional form: we need a quantity that *adds* when independent couplings
compose, so that Stage 5's dynamics stay linear in accumulation.

Mutual information between the asker's action stream and its peers' does exactly
this, and is natively in bits:

    κ = I(A_p ; A_{-p})

Bits. Zero for a genuinely independent asker. Additive over independent peer
groups. Estimated from the action-type histograms over a sliding window, which is
a standard plug-in MI estimate with a known bias correction (Miller–Madow), and the
bias correction is a function of the alphabet size, so it too is measured.

### A6 — Tempo

Natively a rate, dimension 1/time. From I5, absolute rate is meaningless. Nondimensionalize
against the asker's own measured baseline rate:

    τ = log2( r(t) / r_baseline )

Bits. `τ = 0` is normal, `τ = 4` is sixteen-fold. Invariant under S4 (choice of time
unit cancels in the ratio) and under S2.

## Result: the state is a vector of bit-valued components

    x = (a, h, ι, ω, κ, τ) ∈ ℝ⁶,  every component in bits

All six axes now share a unit. Sums, quadratic forms, and distances between them are
now legitimate arithmetic rather than category errors. The metric tensor of Stage 5
is dimensionless, and the danger boundary constant is measured in **bits²** — a
quantity a deployment can actually calibrate rather than tune.

Note what happened here: we did not *choose* six dimensions to match an initial
intuition about a "6D sphere." We enumerated the distinct kinds of accumulation
forced by the Stage 1 invariants, checked each admitted a bit-valued measurement,
and got six. If a seventh with an independent decay timescale and an independent
measurement procedure is found, the model takes seven. The dimension is an output.

## Dimensionless groups (Π-groups)

With everything in bits, the model's behavior is governed by these dimensionless
ratios — and *only* these. Two deployments agreeing on all Π-groups behave
identically regardless of scale, traffic volume, or industry.

| Group | Definition | Meaning | How measured |
|---|---|---|---|
| Π₁ | `α` | Barrier aggressiveness: fraction of remaining margin spendable per step | Set from tolerated worst-case approach rate; see 07 |
| Π₂ⁱ | `Δt / T_½ⁱ` | Elapsed time over axis *i*'s half-life | Half-lives measured from decay of each axis in benign logs |
| Π₃ | `V(x) / c` | Fraction of the danger budget consumed | Direct |
| Π₄ | `ρ_orbit / σ_peer` | Orbit residual over peer-group spread | Direct from peer states |
| Π₅ | `‖g‖_M / √c` | Step size relative to budget scale | Direct |

Π₂ deserves emphasis. The half-lives span roughly nine orders of magnitude:

    T_½(τ)  ~ seconds        tempo forgets almost immediately
    T_½(κ)  ~ minutes        coupling is a windowed statistic
    T_½(ω)  ~ hours          logging is usually restored
    T_½(a)  ~ hours-days     credential rotation
    T_½(h)  ~ days           topology change
    T_½(ι)  ~ ∞              destroyed information does not come back

This nine-order spread is the quantitative statement of Stage 1's I2, and it is the
proof that no scalar risk score can work. A scalar has exactly one half-life. Any
choice of it is wrong by up to nine orders of magnitude on some axis.

## The constants, and how each is measured

The model has exactly these free constants. Per the discipline, each gets a
procedure — none is a knob to be tuned until the demo looks good.

| Constant | Symbol | Measurement procedure |
|---|---|---|
| Per-axis half-lives | `T_½ⁱ` | Fit exponential decay to each axis on a benign-traffic corpus |
| Metric tensor | `M` | Inverse covariance of benign trajectories (Mahalanobis); see 05 |
| Danger budget | `c` | High quantile (e.g. 0.999) of `V` over the benign corpus |
| Barrier aggressiveness | `α` | From the worst-case approach bound in 06: choose the largest α meeting the operator's required time-to-boundary |
| Baselines | `Ops_b, Reach_b, r_b` | Per-asker medians over the calibration window, with peer-group fallback |
| MI window | `W` | Longest window whose MI estimate is stable under bootstrap |

Six constants, six measurement procedures, zero tuning knobs. If a deployment cannot
run these procedures, it cannot run the model — and that is the correct behavior, not
a limitation to paper over with defaults.

## Sanity check by dimensional reasoning alone

Before any code: what does dimensional analysis predict about time-to-boundary?

The only quantities available are the remaining margin (bits²), the step size in the
metric (bits²/step), and α (dimensionless). The only dimensionally consistent
combination giving a step count is

    N ~ (1/α) · log( h_0 / h_target )

So the model predicts time-to-boundary grows **logarithmically** in how close the
attacker wants to get, and **linearly in 1/α**. An attacker wanting to reach 99.9%
of the budget needs ~7/α steps beyond one wanting 50%.

We have not derived the dynamics yet. Dimensional analysis alone gave us the scaling
law. Stage 6 proves this exact expression from the barrier condition, and Stage 7
measures it against an adversarial optimizer. If the measured scaling disagrees with
this prediction, the formulation is wrong — and that is a real, pre-registered
falsification test rather than a post-hoc rationalization.
