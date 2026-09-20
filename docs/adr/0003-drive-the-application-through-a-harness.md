# ADR 0003: Drive the application through a harness

**Status:** accepted
**Date:** 2026-09-19

## Context

FerrumGrid's stated goal is to sit as close to the Excel look and feel as it
can get, and to be quick. Both are claims about what happens on screen, and
neither can be settled by a unit test on a function.

Until this decision the application was one crate: the interaction model, the
toolkit wiring and the binary together. That arrangement has two costs. The
first is that nothing above the model could be tested at all, because reaching
it meant opening a window and an event loop. The second is quieter and worse:
with no boundary in the way, decisions drift into the callbacks, where they
cannot be reached by anything except a person clicking. Three bugs in this
project were found by a human noticing something on screen, and one of them, a
resize strip that received no press because of how hit testing resolved, would
have been caught in a second by a test that asked whether the drag started.

Automating the real window is the obvious alternative and is the wrong size of
tool: it needs a display, it is slow enough that nobody runs it before pushing,
and it is famously flaky. Screenshot comparison additionally fails on a font
change three layers down.

## Decision

**The application is split so that everything except the drawing lives in
`ferrum-grid-view`, and every user-visible behaviour is proved by driving that
crate through `Harness`.**

Three rules follow, and they are the dev strategy going forward:

**1. The interface decides nothing.** `apps/ferrum-grid` is wiring: each
callback calls one method on `App` and copies the resulting `View` onto the
window. A callback that contains a decision is a bug, because that decision
cannot be tested. `View` is destructured field by field in `refresh`, so adding
something to the picture that nothing draws will not compile.

**2. Tests perform gestures, not method calls.** `Harness` clicks where a cell
is drawn, presses the keys the interface sends by the names the interface sends
them under, and drags the edge it can see. It rebuilds the `View` after every
gesture, so a test always reads what would be on screen and can never read a
picture that is one gesture stale. A test that reaches past the gestures into
the state has stopped proving that the gestures work.

**3. A claim about the screen is asserted against the screen.**
`Harness::screen_of` renders a block of the grid as text, cursor and alignment
and clipping included, so an assertion looks like the thing it is about:

```text
    |    A    |    B    |
  1 |<Region> |Units    |
  2 |North    |      120|
```

`View::geometry` strips out everything that carries a colour or a word, which
is what makes "both themes put every pixel in the same place" a test rather
than an intention.

The new crate takes no interface dependency, so the boundary is enforced by the
compiler rather than by discipline, and the tests run on Linux in the fast CI
job: 25 integration tests in about ten milliseconds, against roughly twenty
minutes for a cold build of the application.

## Consequences

It found a defect within minutes of existing. Every special key the interface
does not name for itself arrives from the toolkit as a private use character:
F9 is `U+F70C`. The old key handler admitted anything that was not a control
character, so pressing F9 opened a cell editor holding something invisible. The
fix is four lines; the point is that nothing else was going to find it.

Accepted costs:

- One more crate, and a `View` that has to be kept in step with what the window
  draws. The exhaustive destructuring is what keeps that honest.
- Building the picture on every gesture in a test is more work than reading a
  field. It is also a check, since anything that would make the window panic
  panics in the test instead.
- The harness is shipped in the library rather than hidden behind a feature,
  because a gate that has to be remembered is a gate that gets forgotten. It
  costs nothing at run time; the binary never names it.

What it does not cover, and what still needs a person or another instrument:

- Colour, type rendering and anything about how a pixel actually looks. The
  theme's numbers are checked in `ferrum-theme`'s own tests, against contrast
  floors.
- Whether the toolkit does what the view asked. The hit-testing bug lived
  exactly there.
- Performance. That belongs in the workload example and in profiling.

## Alternatives rejected

**Keep it in the application crate behind a feature.** Smaller diff. Rejected
because the tests would then need the toolkit to build, which puts them in the
twenty-minute job, and because a boundary that is a convention rather than a
crate erodes.

**Automate the real window.** Kept in reserve for the handful of questions that
genuinely need a compositor. It is not where the behaviour of a spreadsheet
should be pinned down.

**Compare screenshots.** Rejected. A screenshot test fails on a font update and
passes on a wrong total.
