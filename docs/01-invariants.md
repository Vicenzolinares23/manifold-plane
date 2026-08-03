# Stage 1 — Invariants

> What is actually true about this system, independent of how I choose to describe it?

Still no equations. We are looking for statements that survive rewriting the whole
system in a different vocabulary, on different hardware, in a different decade.

## Candidate invariants, and whether they survive

### I1. Ability is monotone under approval, absent decay

If a request is approved, the asker's ability afterward is never *less* than before,
except through the passage of time.

**Survives.** This is close to definitional but it is load-bearing, because it means
ability is an accumulator and not a fluctuating measurement. Approvals only ever
push outward. The only inward force is time. Any model where a granted approval
spontaneously reduces ability is describing something else.

**Consequence:** the state cannot be estimated from the current request. It must be
carried.

### I2. Decay is real, and it is not one rate

Ability fades, and different kinds fade at rates separated by many orders of
magnitude. A burst of traffic is over in seconds. A credential lives until rotation.
A deleted row is gone for good — that one does not fade at all.

**Survives, and it is sharper than it first looks.** The existence of *several*
decay rates in the same system is the reason a single scalar "risk score" cannot
work. A scalar has one decay rate. Collapse a fast-decaying quantity and a
never-decaying quantity into one number and you get a number that is wrong at every
timescale: it either forgets deletions or it never forgets a traffic spike.

**Consequence:** the state is a vector, and the number of components is bounded
below by the number of distinct decay timescales.

### I3. Doing and undoing are not symmetric

There is no operation that reverses every operation. Reads reverse for free.
Deletes do not reverse. Physical actuation does not reverse — the valve can be
closed again but the fluid already moved and the vessel already heated.

**Survives, and it is the invariant most systems get wrong.** Existing policy
engines score a request by *what it touches*, not by *whether the world can be put
back*. Those are different quantities and they are not correlated as strongly as
people assume: a read of a secret is low-impact by resource sensitivity and
permanently irreversible by information disclosure.

**Consequence:** irreversibility is an independent axis, not a weight on the others.

### I4. Observability is a resource that can be spent and acquired

An asker can take actions that reduce how much of its own future behavior is
recorded. Once taken, subsequent measurements of that asker are less reliable.

**Survives, and it has a nasty corollary:** the state estimate degrades as a
function of the asker's own prior actions. This is a feedback loop *against* the
defender, and it is the mechanism by which slow attacks become invisible attacks.

**Consequence:** the model must include its own measurement quality as a state
component, or it will silently become confident about nothing.

### I5. Rate is meaningless in absolute terms

There is no universal number of requests per second that is suspicious.

**Survives.** Any threshold on absolute rate is a statement about a specific
deployment, not about the system. It is therefore not an invariant of the system and
must not appear as a constant in the model — it must appear as a ratio against a
measured baseline.

### I6. Individually-acceptable steps compose into unacceptable outcomes

The central observation from Stage 0.

**Survives, and it is the reason the project exists.** Note carefully what it
implies: the acceptable region is **not closed under composition of acceptable
steps.** If it were, memoryless per-request policy would be sound and there would be
no defect to fix. Every deployed authorization system in production today implicitly
assumes closure under composition. The assumption is false.

This is the single most important line in this document.

### I7. The decision is irrevocable and made under a deadline

The receiver cannot defer, cannot batch, and cannot recall an approval.

**Survives.** This kills any formulation requiring lookahead over the actual future
request distribution, because that distribution is not available at decision time
and the decision cannot wait for it. Whatever we build must decide from the current
state and the proposed step alone — no simulation of what the asker will ask next.

This is a severe constraint and it eliminates most of the obvious approaches
(planning, model-predictive control with a learned request model, anything
requiring a rollout). It will drive the entire formulation in Stage 5.

### I8. Askers are not independent, and independence cannot be assumed away

Two askers may be one actor. Capability can be handed between them.

**Survives.** The joint state is not the product of the marginals. A model that
tracks each asker in isolation is provably blind to the transfer.

### C1. "Requests arrive as discrete events" — candidate, does NOT survive cleanly

In the factory-floor setting, a sequence of setpoint writes is better described as a
continuously varying commanded value than as discrete events. Discreteness is an
artifact of how we sample, not a property of the system.

**Rejected as an invariant.** Kept as a modeling convenience, flagged as such. The
dynamics we write in Stage 5 must degrade gracefully as the sampling interval goes to
zero, or we have baked in an artifact.

### C2. "There is a fixed set of resources" — candidate, does NOT survive

Resources are created and destroyed constantly, often by the very requests being
judged. A model indexed by a fixed resource set breaks immediately.

**Rejected.** The state must be dimensioned by *kinds* of ability, not by an
enumeration of things. This is a strong constraint and it rules out the obvious
"one dimension per resource" design that most graph-based approaches reach for.

## What the surviving invariants force

Reading I1–I8 together, without having chosen any formalism:

- The state is **carried**, not computed per request. (I1)
- The state is a **vector**, with at least as many components as there are distinct
  decay timescales. (I2)
- Its components include at minimum: something about accumulated privilege,
  something about permanence, something about self-observability, something about
  rate relative to habit, and something about cross-asker coupling. (I2, I3, I4,
  I5, I8)
- The components are **kinds of ability**, never an enumeration of resources. (C2)
- Dynamics have a **decay term with per-component rates** and a **displacement term
  from the approved request**. (I1, I2)
- The acceptable region is **not closed under composition**, so the admission rule
  must constrain something about the *step*, not only about the resulting position.
  (I6)
- The rule may use only the current state and the proposed step. No lookahead over
  future requests. (I7)
- All constants must be ratios against measured baselines, never absolute
  magnitudes. (I5)

That last set is essentially a specification, and we derived it without writing a
single symbol. This is the point of the exercise.

## The hard one

I6 and I7 are in tension and that tension is the actual research problem.

I6 says the danger lives in composition — in sequences. I7 says we may not look at
sequences, only at one step from one position.

A rule that only sees one step cannot, in general, know where a sequence ends up. So
either the problem is impossible, or there is some property of the *step* that
guarantees something about *every possible future sequence* without enumerating any
of them.

There is. It is not obvious, it comes from control theory rather than from security,
and it is the subject of Stage 5. Naming it here would violate the discipline of
this stage, so it stays unnamed for two more documents.
