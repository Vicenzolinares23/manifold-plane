# manifold-plane

**Trajectory-based admission control.** Every deployed authorization system
decides one request at a time against a fixed policy. That is sound only if the
set of acceptable states is closed under composition of acceptable steps — and
it is not. This bounds what a *sequence* of individually-acceptable requests can
compose into, using a control barrier function over a six-dimensional capability
state.

One kernel, three adapters: Kubernetes admission, ICS (Modbus/DNP3), and LLM
agent tool-calls.

```
docs/00 … docs/07   the derivation, written before the code
crates/mp-core      state space, metric geometry, relaxation dynamics (zero deps)
crates/mp-barrier   the admission kernel and the decision procedure
crates/mp-adapters  Kubernetes / ICS / agent → displacement in bits
crates/mp-daemon    manifold-planed
crates/mp-operator  ManifoldPolicy reconciliation
sim/                independent reimplementation used to check crates/
```

## The defect

An asker requests permission to read a list. Fine. Read a second list. Fine.
Copy one small thing. Fine. Replicate itself somewhere quieter. Fine. Turn off
the thing that records what it did — also fine in isolation, plenty of
legitimate software does that.

Nobody would have approved the sequence. Everybody approved it one piece at a
time, because the receiver has no memory: each answer is given as if it were the
first question ever asked.

The same shape appears in a Kubernetes cluster, on a factory floor, and inside
an LLM agent's tool loop. Three communities that do not talk to each other, one
defect.

## The idea

Stop treating an asker as a name on a list. Treat it as a **position** —
somewhere it has walked to, one approved request at a time — and ask whether any
sequence of individually-acceptable moves leads somewhere unacceptable.

Six axes, all measured in **bits** (`docs/03`):

| | | decays in |
|---|---|---|
| `a` | authority — accumulated privilege | hours–days |
| `h` | reach — blast radius over the resource graph | days |
| `ι` | irreversibility — bits of world state destroyed | never |
| `ω` | opacity — evidence that will not survive | hours |
| `κ` | coupling — mutual information with peers | minutes |
| `τ` | tempo — rate against the asker's own habit | seconds |

Those half-lives span **nine orders of magnitude**, which is the quantitative
proof that a scalar risk score cannot work: a scalar has exactly one half-life,
so any choice of it is wrong by up to nine orders on some axis.

The safe set is `Ω = {z : zᵀMz ≤ c}` — an ellipsoid in raw coordinates and
exactly a sphere in the metric the system itself induces. `M` is the inverse
covariance of benign traffic, so the geometry learns which directions matter
without anyone encoding them.

## The rule

```
ADMIT r  ⟺  h(x⁺) ≥ (1 − α)·h(x)        h(x) = c − V(x)
```

Not `V(x⁺) ≤ c`. A threshold admits the largest possible step everywhere and
lets a patient attacker sprint to the wall. The barrier condition makes the
admissible step shrink in proportion to the margin remaining, so approach speed
decays geometrically toward zero. **The attacker is not stopped by a wall; it is
subjected to a speed limit that tightens as it approaches.**

Because the constraint is relative to the current margin and enforced every
step, it says something about every possible infinite future sequence while
examining only one step from one position — which is what makes it deployable in
a request path where no lookahead exists.

**Theorem (forward invariance).** If `x₀ ∈ Ω` and every admitted step satisfies
the condition, then `h(x_k) ≥ (1−α)^k·h(x₀) ≥ 0` for all `k`. For every
adversary, every request sequence, every length. Proof in `docs/06`; verified
over 100k adversarial steps in `mp-barrier` and 200k in `sim/`.

Two things fall out that nobody designed:

- Steps *back toward* baseline are never throttled — an asker reducing its own
  capability is always allowed to.
- Two askers each at half the budget, tightly coupled, exceed it together and
  are stopped, though neither is individually near the boundary. This is the
  air-traffic-control part, and no deployed authorization system detects it.

## Three routes to one law

`docs/03` predicted, from dimensional analysis alone and before any dynamics
existed:

```
N ~ (1/α) · ln(h₀ / h_target)
```

`docs/06` T2 derived the same expression from the barrier condition.
`sim/adversarial.py` measures it with an optimizer free to choose direction and
magnitude, told the metric, the budget and `α`.

Measured: **45 steps per decade of approach at both decades tested**, matching
theory to 0.24%. Logarithmic, not polynomial. Had it come out otherwise the
formulation would have been wrong, and `docs/03` said so in advance rather than
leaving it to be explained away.

## Cost

Measured by `cargo bench -p mp-barrier`:

| | |
|---|---|
| barrier kernel alone | 38 ns |
| full decision, no peers | 270 ns |
| full decision, 20 peers in class | 1.7 µs |
| full decision, 2000 peers in class | 138 µs |

The orbit residual takes a median and a MAD over the whole symmetry class on
every decision, so it is `O(peers)` and dominates past ~20 members. That is a
real limitation, not fixed here; the fix is caching the class median between
decisions rather than dropping the detector, which is half the system's value.

## Run it

```bash
cargo test --all                 # 139 tests
python3 sim/run_all.py           # independent validation, no dependencies
```

With a real agent driving it:

```bash
DOCKER_BUILDKIT=1 docker compose -f docker-compose.dev.yml build
docker compose -f docker-compose.dev.yml up -d
docker compose -f docker-compose.dev.yml --profile demo run --rm agent-harness
```

`ollama` carries `llama3:8b`, pulled at build time. The harness sets up a
situation and lets the model choose its own tool sequence — it does not script
the attack, and it reports honestly when the model never attempts one.

## What is not claimed

`Ω` is calibrated from benign traffic. If that corpus contains an ongoing
compromise, `Ω` is wrong, and no amount of control theory fixes it. The
displacement functions are the softest part of the system and the kernel's
guarantee is conditional on them.

Three findings in `docs/07` are **open**:

- **F4** — an adversary willing to accept full independence between its askers
  defeats coalition detection, and then `n` identities accumulate `n` budgets.
  The most important unsolved problem here.
- **F5** — recalibration is itself slow-walkable. The principled fix is to apply
  the same barrier one level up, to the parameter trajectory. Currently there is
  only a blunt drift clamp, labelled as blunt.
- **F7** — review-band flooding is a human-factors problem the mathematics does
  not address.

`docs/07` also states in advance what would falsify the whole approach. None of
the three is ruled out by anything in this repository; they are ruled out or in
by data from a real deployment, which is the next piece of work.

## Method

The mathematics was derived before the implementation, in order: observation
without jargon, invariants, symmetry, dimensional analysis, state-variable
discovery, formulation, proofs, adversarial validation. `docs/` is that record,
including the candidates that were rejected and why.

Running it found real problems the derivation had not:

- The denial charge wrote to the state on a path that never consulted the
  barrier, walking an asker out of `Ω` on denials alone. The theorem was
  correct; the escape was on a path outside its scope.
- The iterative Lyapunov projection is numerically unusable at a nine-order
  half-life spread — it produced `c = 3.1×10⁸ bits²`. Replaced with a closed
  form that also says something true: an axis that never decays cannot stay
  correlated with a fast one.

Both are in the commit history with the measurements that exposed them.

## License

MIT.
