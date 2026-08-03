# Stage 2 — Symmetry

> Symmetries are ways to transform the description without changing the truth.

If a transformation changes our verdict but does not change the underlying
situation, the model has encoded an accident of description as if it were a fact.
Every one of these is a bug, and several of them are exploitable bugs.

## Required symmetries — the model MUST be invariant

### S1. Relabeling of askers (permutation equivariance)

Rename every asker. Permute their identifiers arbitrarily. The verdict for the
situation must be unchanged, with verdicts permuted the same way.

**Why it matters concretely:** a model that violates this has learned something
about *names*. That is trivially defeated — the attacker picks a different name.
Any per-identity constant, any allowlist keyed on identifier, any embedding of the
service-account string is a violation.

**Design consequence:** identity may enter the model *only* through the state
carried under that identity, never as a feature of the identity itself.

### S2. Time translation

Shift every timestamp in the system by a constant. Nothing changes.

**Design consequence:** dynamics may depend on elapsed time between events (Δt),
never on absolute time. No wall-clock terms. Note this is a real constraint that
"business hours" heuristics violate — those are not invariant under time
translation, which is exactly why they generate the false positives they do. If
time-of-day genuinely matters in a deployment, it belongs in the *baseline* that
Stage 3 nondimensionalizes against, not in the dynamics.

### S3. Relabeling of resources within an equivalence class

Two pods running the same image with the same role are interchangeable. Two coils
on the same manifold with the same function are interchangeable. Renaming one to
the other changes nothing.

**Design consequence:** resources enter only through their *class* and their
position in the reachability structure — never through their names. This is C2 from
Stage 1 restated as a symmetry, which is a good sign that both are real.

### S4. Choice of units

Measuring time in seconds or hours, counts in items or thousands of items, must not
change any verdict. This one is doing more work than it appears to and Stage 3 is
entirely about cashing it out.

### S5. Additive constant on the potential

If a scalar "danger" function is built, adding a constant to it everywhere changes
nothing — only differences and level sets are meaningful. Any rule that reads an
absolute value of such a function without comparison to a calibrated reference is
reading a number that has no meaning.

## Broken symmetries — where the signal actually lives

Here is the move that pays for this entire document. A symmetry that *should* hold
but observably does not is not noise. It is the strongest available evidence.

### B1. Peer-orbit divergence

Consider the group of askers that are, by construction, interchangeable: replicas of
one deployment, identical operator instances, sensors of the same model on the same
segment. S1 and S3 say the system cannot distinguish them.

If their carried states diverge, one of three things is true: the workload is
genuinely heterogeneous (and the equivalence class was drawn wrong), a fault has
occurred, or one of them is compromised.

**This gives a detector for free, and it is a detector with no threshold.** It does
not ask "is this asker's state large." It asks "is this asker's state far from the
orbit of its own symmetry group." An attacker who compromises one replica of twenty
must now move *all twenty in the same way* to stay invisible — which is enormously
harder, observable in aggregate, and in the ICS setting often physically impossible.

Formally, this is distance from the group orbit, i.e. a residual in the quotient
space `state-space / symmetry-group`. Stage 5 gives it a symbol.

**Why this is not just anomaly detection:** conventional anomaly detection compares
an asker to its own past, which the slow attacker corrupts by moving the baseline.
Orbit residual compares an asker to its *peers in the present*, which the slow
attacker cannot corrupt without compromising the peers too. The baseline is
adversary-resistant because it is held by parties the adversary does not control.

### B2. Time-reversal asymmetry as the definition of irreversibility

Run the request log backwards. Most of it is meaningless in reverse — but exactly
*how* meaningless is the measurement of I3.

A read is nearly time-symmetric: the world after is the world before, plus the
reader's knowledge. A delete is maximally asymmetric: the forward map is not
injective, information is destroyed, and no inverse exists.

**This converts a fuzzy intuition into something measurable.** Irreversibility is
the failure of injectivity of the state transition. In Stage 3 this becomes a
count of preimages, and the logarithm of that count is measured in bits — which is
where the units for the whole model come from. Irreversibility being an *entropy*
is not a metaphor here; it is the definition we will use.

### B3. Scale asymmetry between grant and revoke

Granting is one request. Revoking the same ability is frequently many requests, or
impossible, or requires a human. The system is not symmetric under exchange of grant
and revoke, and the size of that asymmetry is a real, measurable, per-deployment
quantity: the ratio of cost-to-undo over cost-to-do.

**Design consequence:** this ratio is a *measured input* to the model, not a tuned
constant. Stage 3 requires every constant to have a measurement procedure; this one
has an obvious one, and `docs/03` specifies it.

## Symmetries we deliberately do NOT impose

### N1. Isotropy

We do **not** assume the axes are interchangeable — that moving one unit along
"privilege" is equivalent to one unit along "irreversibility." They are not. The
geometry is anisotropic and the anisotropy is exactly the deployment-specific
content of the model.

This is why the danger region will turn out to be an **ellipsoid and not a sphere.**
The user's original intuition was a "6-dimensional security sphere," and it is very
nearly right — it *is* a sphere, but only in the metric the system itself induces.
In raw coordinates it is a stretched ellipsoid, and the stretching is the physics.

### N2. Symmetry between askers of different type

A human operator and an automated agent are not interchangeable, and we should not
force a model that treats them so. They belong to different equivalence classes
under S1 — the permutation group acts *within* classes, not across them.

### N3. Reflection symmetry in the axes

Ability and its negation are not symmetric. Every axis is signed: more is worse,
less is better, and there is a floor. The state space is a cone, not all of ℝⁿ.
This matters in Stage 5 because it means the decay operator has a fixed point at
the baseline rather than at the origin.

## Summary of consequences carried forward

| Symmetry | Consequence for the model |
|---|---|
| S1 relabeling askers | Identity enters only via carried state |
| S2 time translation | Dynamics use Δt only; no wall-clock |
| S3 relabeling resources | Resources enter via class + reachability only |
| S4 units | Everything nondimensionalized (Stage 3) |
| S5 additive constant | Only level sets and differences are meaningful |
| B1 orbit divergence | A second, threshold-free, adversary-resistant detector |
| B2 time-reversal | Irreversibility ≔ log of preimage count, in bits |
| B3 grant/revoke | Measured ratio, not a tuned constant |
| N1 no isotropy | Danger region is an ellipsoid; metric is learned |
| N3 no reflection | State space is a cone with a baseline fixed point |
