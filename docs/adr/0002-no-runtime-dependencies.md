# ADR 0002: No third-party dependencies in the shipped product

**Status:** accepted
**Date:** 2026-09-19

## Context

The owner's other libraries set this standard already: pyOpenVBA ships with
"no dependencies beyond the standard library", and pyOfficeEditor with "no
dependencies" at all. The instruction for this repository is the same, and adds
that the repository should be pure Rust as far as is practical.

A spreadsheet needs a lot of machinery that the crates ecosystem would happily
supply: a fast hash, a ZIP reader, a DEFLATE codec, an XML parser, date
arithmetic, number formatting, a text shaper, a regular-expression engine.
Taking those is the default path and it is not the one being taken.

## Decision

**The shipped product depends on the Rust standard library and on the GUI
toolkit, and on nothing else.**

The GUI toolkit is the single exception, and it is not a loophole: `AGENTS.md`
names Slint as the interface technology, so a reading of "no dependencies" that
excluded it would contradict a standing instruction and leave no way to build
the product at all. Every other capability is written here.

That exception is not small, and the number belongs in this document rather
than in a footnote: the application crate resolves **246 crates**, essentially
all of them beneath Slint. The four library crates resolve none. So the rule
holds where it was aimed, at the engine, the document model and the file
formats, and the interface is a single large boundary that was chosen rather
than accumulated. `renderer-femtovg` is used instead of `renderer-skia`, which
would bring 291 and needs a source download and symlink privileges to build.

That means writing, over the life of the project:

- the hash used by the internal maps
- INFLATE and DEFLATE, because an `.xlsx` is a ZIP
- an XML reader and writer
- the serial date arithmetic, including the 1900 leap-year quirk that
  interoperability requires
- number-format parsing and rendering
- the formula engine, the VBA engine, and the code editor's text core

**Development and testing may use whatever is useful.** Build scripts,
measurement tools, fixtures, benchmarking harnesses and `dev-dependencies` are
not the product and are not covered by this rule. The Excel measurement behind
`docs/design/theme.md` was taken with Python and COM, which is exactly the sort
of use this exemption is for.

**Prefer Rust even for the tooling.** Where a dev tool is going to be kept and
rerun, it belongs in the test suite rather than in a script in another
language. The palette's contrast check started as a Python script and was moved
into `ferrum-theme`'s tests for this reason: a check beside the values it
checks cannot drift from them, and it runs in CI without a second toolchain.

## Consequences

Accepted costs:

- More code to write, and more to get right. A DEFLATE decoder is a weekend
  that a dependency would have made an afternoon.
- Correctness burden moves onto us. A hand-written XML reader has to be
  fuzzed; a borrowed one has been.
- Some things are genuinely hard to do well alone. Complex text shaping for
  right-to-left and Indic scripts is the clearest example, and it is listed in
  `docs/open-questions.md` rather than waved at.

Bought in exchange:

- No supply chain. Nothing to audit, nothing to update, no transitive crate
  pulling in a build script, and no exposure to the class of attack where a
  package name is registered because a model hallucinated it.
- A build that will still work in ten years.
- Binary size and startup time under our own control, which matters for a
  product whose stated priority is performance.
- Every performance-critical path is ours to profile and change, with no
  dependency's design deciding our data layout.

## Enforcement

`Cargo.toml` at the workspace root lists only path dependencies. A review that
sees a registry dependency added to a shipped crate should reject it or expect
this document to change in the same commit.

`unsafe_code = "forbid"` is set workspace-wide for the same family of reasons:
the product should be memory-safe by construction, not by inspection.

## Alternatives rejected

**Take dependencies for the boring parts only.** Reasonable, and it is what
most projects do. Rejected because the line between boring and load-bearing
moves: `zip` is boring until the spreadsheet's load time is dominated by it.

**Vendor the source of a few crates.** Gets the code without the supply chain,
but inherits the maintenance with none of the upstream fixes, and muddies the
licensing story that a clean-room implementation depends on.
