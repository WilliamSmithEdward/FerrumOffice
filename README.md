# FerrumOffice

A productivity suite written from scratch in Rust, with a Slint interface.

| Application | Does what |
| --- | --- |
| **FerrumGrid** | Spreadsheets. In progress, and the only one started. |
| **FerrumWrite** | Documents. Not started. |
| **FerrumStage** | Presentations. Not started. |
| **FerrumBase** | Databases. Not started. |

> **Status: early, and it opens.** FerrumGrid runs: a virtualised grid over a
> million rows, resizable rows and columns, selection, in-cell and formula-bar
> editing, sheet tabs, both themes. Behind it, a formula engine with 96
> callable function names, sparse storage, a dependency graph, and ordered
> recalculation with cycle detection. 272 tests. No file format yet, so
> nothing can be saved or opened.

## What it is aiming at

Lock-step behavioural compatibility with the file formats and the formula
language people already use, on an interface built to a higher bar than the
products it replaces, with performance treated as a requirement rather than an
aspiration.

Two decisions shape everything else:

**No third-party dependencies in the shipped product.** The standard library
and the GUI toolkit, and nothing else. The ZIP codec, the XML reader, the date
arithmetic, the number formatting and the engines are all written here.
Development tooling is exempt. See
[ADR 0002](docs/adr/0002-no-runtime-dependencies.md).

**Measure, do not recall.** The light theme matches the Excel sheet surface
because that surface was measured, not remembered: `#ffffff` cells, `#e0e0e0`
gridlines, 48pt columns and 14.5pt rows in Aptos Narrow 11pt. The method and
the numbers are in [docs/design/theme.md](docs/design/theme.md), including a
figure that was wrong in the first draft and how the measurement caught it.

## Layout

```text
crates/
  ferrum-core     values, errors, coordinates. No dependencies, no I/O.
  ferrum-theme    colour tokens and grid metrics.
  ferrum-calc     formula lexer, parser, evaluator, function library.
  ferrum-sheet    workbook model, sparse storage, dependency-driven recalc.
apps/
  ferrum-grid     the spreadsheet application.
assets/
  ferrum_grid.png        the FerrumGrid mark, as drawn.
  ferrum_office_icon.png the suite mark.
  icons/                 sizes derived from those, generated once and committed.
docs/
  adr/            decisions that would be expensive to reverse.
  design/         how a surface should look and behave, and why.
  open-questions.md  what is undecided and what would settle it.
```

The files under `assets/icons/` are downscaled from the artwork beside them:
a 256px PNG that the window and taskbar use, and a seven-size `.ico` that the
build embeds so the executable has a picture in Explorer. They are committed
rather than generated at build time, because generating them would mean a
build dependency on an image library for something that changes once a year.

## Running it

```bash
cargo run --release -p ferrum-grid -- --demo
```

`--demo` opens on a small worked sheet; without it you get an empty workbook.
`--dark` starts in the dark theme, which the toolbar also toggles.

Arrow keys move, typing starts an edit, Enter and Tab commit, Escape abandons,
F2 opens a cell for amending, Delete empties the selection. Clicking a column
letter or row number selects it, dragging across several extends the
selection, and dragging the boundary between two resizes; the corner selects
everything. Dragging a boundary past its neighbour hides that row or column.

On Windows with the GNU toolchain, read
[building-on-windows.md](docs/building-on-windows.md) first: there is a missing
import library, and the failure it causes is thoroughly misleading.

## Building

```bash
cargo test
```

```bash
cargo clippy --all-targets
```

A rough timing of the calculation engine, with the numbers recorded in
[open-questions.md](docs/open-questions.md):

```bash
cargo run --release -p ferrum-sheet --example workload
```

## Design

- [Theme: colour, type and pixel alignment](docs/design/theme.md)
- [The Ferrum code editor](docs/design/code-editor.md)
- [Open questions](docs/open-questions.md)

## Licence

MIT.
