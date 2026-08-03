# Stage 0 — Observation before formalism

No equations in this document. No terms of art from security, control theory, or
mathematics. If a specialized word appears here, it is a bug, and it means an
unexamined model has been smuggled in.

## The system, described plainly

Something asks something else to do a thing.

The thing receiving the request has to answer yes or no. It has to answer *now* —
it cannot wait to see what happens next. And once it says yes, the thing gets done,
and often it cannot be undone.

Requests keep arriving. Many different askers. The same asker comes back again and
again over hours or days. The receiver answers each one.

Here is what we actually observe, and it is the whole reason this project exists:

> Every single request, looked at by itself, is fine.
> The sequence of them is not.

An asker requests permission to read a list. Fine. Then to read a second list.
Fine. Then to copy one small thing. Fine. Then to make a copy of itself somewhere
quieter. Fine. Then to turn off the thing that writes down what it did. That last
one is *also* fine in isolation — plenty of legitimate askers turn off noisy
record-keeping. Nobody would have approved the whole sequence if asked up front.
Everybody approved it one piece at a time.

## What the receiver currently does

It compares each request against a list of what that asker is allowed to do, and
answers from the list. The list does not change based on what the asker did five
minutes ago. The receiver has no memory. Each answer is given as if it were the
first question ever asked.

This is the defect. Not a bug in the list — the list can be perfect and this still
happens. The defect is that **the receiver answers questions about single requests
when the dangerous object is the sequence.**

## What else we observe

**Undoing is not symmetric with doing.** Reading a thing costs nothing to reverse.
Deleting a thing may be impossible to reverse. Turning a physical valve may be
impossible to reverse in the way that matters, because the liquid already moved.
The receiver treats both as one yes.

**Ability accumulates, but it also fades.** An asker who got a key an hour ago
still effectively has it. An asker who got one three months ago probably does not —
the key rotated, the session ended, the machine was rebuilt. Different kinds of
gained ability fade at wildly different speeds. A deleted record never comes back;
a burst of fast requests is over the moment it stops.

**Askers are not independent.** Two askers, each behaving unremarkably, can be the
same actor wearing two coats. Or one can hand the other something. Watching each in
isolation loses this entirely.

**Being unwatched is itself something you can acquire.** Some requests make the
asker's future behavior less visible. That is not a side effect. For an adversary
it is the whole point, and it is a thing that can be gained, held, and spent, just
like any other ability.

**Speed only means something relative to a habit.** Forty requests a second is
alarming from a thing that has made four requests a day for a year, and completely
unremarkable from a thing that has always made forty a second.

## The three places we see this

The same shape appears in three settings that are normally considered unrelated,
which is the first real hint that there is a structure underneath worth finding.

1. A machine that decides whether to accept changes to a cluster of running
   programs. Individually valid changes, applied in order, end in an asker that
   can do anything.

2. A machine on a factory floor that accepts instructions to move physical
   equipment. Each instruction is within the equipment's stated range. The
   *sequence* walks the equipment somewhere it should never be, and the physical
   world does not offer an undo.

3. A program that decides what a piece of automated software is allowed to do on
   someone's behalf — read this, write that, send this outward. Each action is one
   the operator would have approved. The chain of them ends with private data
   leaving the building.

These three have completely different vocabularies and completely different
communities of people working on them. They have the same defect.

## The question this raises

If ability accumulates, fades, and comes in kinds that behave differently, then an
asker is not a name on a list. An asker is a **position** — somewhere it has walked
to, one approved request at a time.

And if it is a position, then the question stops being "is this request allowed"
and starts being:

> Where is this asker now, where would this request move it,
> and is there any sequence of individually-acceptable moves
> from there to somewhere unacceptable?

That is not a filtering question. That is a question about motion.

## Deliberately not said yet

We have not said how many kinds of ability there are. We have not said how to
measure any of them. We have not said what "unacceptable" means as a boundary. We
have not chosen units, and we have not written a single symbol.

All of that is Stage 1 onward. Writing it here would freeze assumptions we have not
earned yet.

## The one analogy we are allowing ourselves, and its leash

Airspace control has the same shape: many independent movers, decisions that must
be made now, no undo, and safety that is a property of *trajectories* rather than
of positions. Controllers do not ask "is this aircraft currently somewhere bad."
They ask "given where everything is and how it is moving, can anything reach a bad
place within the time I would need to prevent it."

This is a hypothesis to stress-test, not a template to fill in. Where it fails:
aircraft have physics that bound their next move, and requests do not. An asker can
in principle request anything at any time. So any borrowed structure has to survive
the loss of a bounded-motion assumption, and Stage 5 has to address that directly
or the analogy has to be dropped.
