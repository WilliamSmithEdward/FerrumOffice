# Open questions

Decisions not yet made, and claims not yet verified. Each entry says what would
settle it. Entries are removed when they are settled, not ticked off.

## Decisions for the owner

**The code editor's product name.** `docs/design/code-editor.md` uses
**FerrumForge** as a working name for the VBA authoring surface. It fits the
iron theme and is not taken by the four application names. Nothing depends on
it yet.

**Whether the VBA engine is written fresh in Rust or ported from pyOpenVBA.**
pyOpenVBA reads and writes the code inside Office files; it is not an
evaluator, so the runtime is new work either way. The question is whether its
parser and its understanding of the VBA grammar are ported or rewritten.

**How far "a superset of VBA with modern conveniences" goes.** `AGENTS.md` asks
for one, behind an option that warns it will not open in Office 365. The
surface of that superset is a large design question in its own right and is
not started.

## The largest unresolved risk

**There is no Monaco.** The reference VBA editor gets folding, multi-cursor,
minimap, find and replace, bracket matching, semantic highlighting and a real
undo stack from Monaco, as configuration. In Slint all of that is ours to
build, on top of a text buffer, a layout engine and a syntax highlighter that
also do not exist yet.

This is the single largest piece of work in the project and the most likely
thing to be underestimated. It is recorded here so that it is taken
deliberately rather than discovered in the middle of building the editor.

Related: complex text shaping for right-to-left and Indic scripts is hard to do
well without a shaping library, and ADR 0002 rules those out of the shipped
product. There is no plan for it yet.

## Behaviour to verify against a live spreadsheet

The machine has Excel and `pyVBAharness` can drive it, so these are cheap to
settle and should not be guessed at. Each one is currently implemented from
recall, which ADR-wise is a claim typed as *reported*, not *observed*.

**Exponent associativity.** `ferrum-calc` will parse `^` as left-associative,
so `2^3^2` evaluates to 64 rather than 512. This matches the recollection that
spreadsheets differ from most programming languages here, and it has not been
checked.

**The general number format's switch to scientific notation.**
`ferrum_core::value::format_general` uses 15 significant digits, switching to
scientific at an exponent of 15 or above, or -10 or below. The upper bound
follows from the digit budget and is sound. The lower bound is a judgement
call: the constant `SCIENTIFIC_LOWER_EXP` is marked provisional in the source.
What is known is that `0.00001` renders plainly and that some smaller magnitude
switches over; where exactly is unverified.

**Whether a nine-error set is enough.** `CalcError` covers the seven classic
values plus `#SPILL!` and `#CALC!`. Newer hosts also produce `#FIELD!`,
`#BLOCKED!`, `#CONNECT!`, `#BUSY!`, `#UNKNOWN!`, `#EXTERNAL!` and `#PYTHON!`.
None are reachable yet, and each one costs nothing to add when its feature
arrives.

## Measurements not taken

**Row and column header chrome in Excel's light Office theme.** The theme
measurement was taken on a machine running a dark Office theme, so the header
band sampled near-black while the sheet stayed white. Ferrum derives its header
colours from its own palette instead. If matching Excel's light chrome exactly
matters, it needs a capture from a machine set to that theme.

**A performance budget.** The project's stated priority is performance and
there is still no budget attached to it. There is now a baseline, which is a
different thing: a baseline says where you are, a budget says where you must
stay.

Measured on this machine with `cargo run --release -p ferrum-sheet --example
workload`:

| Work | Time | Per item |
| --- | --- | --- |
| Write 20,000 literals | 9.1 ms | 0.45 us |
| Write 20,000 one-row formulas | 24.9 ms | 1.24 us |
| Write a 20,000-deep formula chain | 34.7 ms | 1.74 us |
| Edit the head of that chain, recalculating 20,001 formulas | 13.9 ms | 0.70 us |
| Recalculate a 40,000-formula workbook | 44.1 ms | 1.10 us |
| One edit under `SUM(A:A)` over 20,000 populated cells | 0.40 ms | |

The last row is the one worth watching. A whole-column reference addresses
1,048,576 cells; the evaluator clips to the populated bounding box, so the
cost tracks the data. If that clipping regresses, this figure jumps by
roughly fifty times and the example is where it shows.

The first version of that measurement was wrong, and the way it was wrong is
worth keeping: `SUM(A:A)` was timed on the same sheet as the 20,000-deep
chain, so every edit to column A cascaded through the chain and the figure
came out at 12.5 ms. That was a real cost, correctly computed, and had
nothing to do with what the label claimed. It now runs on its own sheet.

Still missing, and needed before the grid is tuned: a frame budget for
scrolling, a cold-open budget for a file of a stated size, and a decision
about what workbook size counts as the one to stay fast on.
