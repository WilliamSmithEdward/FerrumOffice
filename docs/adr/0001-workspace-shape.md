# ADR 0001: Workspace shape

**Status:** accepted
**Date:** 2026-09-19

## Context

FerrumOffice recreates four applications. FerrumGrid (the spreadsheet) comes
first, then FerrumWrite, FerrumStage and FerrumBase. A VBA engine and a code
editor are shared by all four.

The layout has to keep three things apart from the start, because retrofitting
the separation later is expensive:

1. The calculation engine must be testable without a window. Almost all of the
   correctness risk in a spreadsheet lives there, and a test suite that needs
   a GUI will not be run often enough.
2. The document model must not know about the renderer, so that a headless
   read and write path exists for tests and for file conversion.
3. The VBA engine must reach the document model without the model depending on
   the VBA engine.

## Decision

A Cargo workspace of small library crates with one application binary per
product.

```text
crates/
  ferrum-core     values, errors, coordinates. No dependencies, no I/O.
  ferrum-theme    colour tokens and grid metrics. No dependencies.
  ferrum-calc     formula lexer, parser, evaluator, function library.
  ferrum-sheet    workbook model, sparse storage, dependency-driven recalc.
apps/
  ferrum-grid     the spreadsheet application.
```

Crates are added when a boundary is real, not in advance. The xlsx reader, the
VBA engine, the code editor core and the Slint UI layer each get one when they
are started.

Two of those have since arrived: `ferrum-xlsx` for the file format, and
`ferrum-grid-view` for the interaction layer, which is the "Slint UI layer"
boundary turned inside out. See
[ADR 0003](0003-drive-the-application-through-a-harness.md).

The dependency direction is strictly downward: `core` knows nothing, `calc`
knows `core`, `sheet` knows `core` and `calc`, and the application knows all of
them.

### How `calc` stays independent of `sheet`

The evaluator needs to read cells, and the cells live in `sheet`, which would
be a cycle. `calc` therefore defines a resolver trait describing what it needs
from a workbook, and `sheet` implements it.

That inversion buys two things beyond breaking the cycle: the engine can be
tested against a fixture workbook that is twenty lines of test code, and a
future consumer that is not a spreadsheet at all can drive the same evaluator.

The trait includes a way to ask for a sheet's populated bounds, because
`SUM(A:A)` must not iterate 1,048,576 cells to add up four of them. That is a
performance requirement expressed in the type system rather than in a comment.

## Consequences

- Most of the test suite runs in under a second with no window, which is what
  makes it worth running on every change.
- Compile times stay reasonable, since a change to the UI does not rebuild the
  evaluator.
- There is a small tax: a type shared by two crates has to live in `core` or be
  re-exported, and getting that wrong shows up as a borrowed type that cannot
  cross a boundary.

## Alternatives rejected

**One crate.** Simpler until the UI and the engine are in the same compilation
unit and every experiment costs a full rebuild. It also makes it too easy for
the evaluator to reach into the renderer, which is the coupling this whole
layout exists to prevent.

**A crate per feature.** Fragments the code past the point where a reader can
follow one behaviour without opening six files, which the project's own
guidelines warn against as loudly as they warn against monoliths.
