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
> editing, undo and redo, sheet tabs, both themes. Behind it, a formula engine
> with 96 callable function names, sparse storage, a dependency graph, and
> ordered recalculation with cycle detection. It can write an `.xlsx` that
> Excel opens, but not read one back yet, and the application has no Save.
> 376 tests.

## What it is aiming at

Lock-step behavioural compatibility with the file formats and the formula
language people already use, on an interface built to a higher bar than the
products it replaces, with performance treated as a requirement rather than an
aspiration.

Three decisions shape everything else:

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
The goal those measurements serve, and how close is close enough, is in
[docs/design/fidelity.md](docs/design/fidelity.md).

**Drive the application, do not poke at it.** Everything FerrumGrid shows and
does lives in `ferrum-grid-view`, which has no toolkit in it; the window is
wiring. Tests click where a cell is drawn, press the keys the interface sends,
and read back what would be on screen. See
[ADR 0003](docs/adr/0003-drive-the-application-through-a-harness.md).

## Layout

```text
crates/
  ferrum-core     values, errors, coordinates. No dependencies, no I/O.
  ferrum-theme    colour tokens and grid metrics.
  ferrum-calc     formula lexer, parser, evaluator, function library.
  ferrum-sheet    workbook model, sparse storage, dependency-driven recalc.
  ferrum-xlsx     the ZIP container, the XML, and the spreadsheet package.
  ferrum-grid-view  what FerrumGrid shows, what a gesture does to it, and
                  the harness that drives it without a window.
apps/
  ferrum-grid     the window. Wiring, and nothing else.
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

The whole of FerrumGrid's behaviour is testable without a window, so the loop
worth staying inside is the one that does not build the toolkit at all, and
finishes in about a second:

```bash
cargo test -p ferrum-grid-view
```

A rough timing of the calculation engine, with the numbers recorded in
[open-questions.md](docs/open-questions.md):

```bash
cargo run --release -p ferrum-sheet --example workload
```

## Design

- [How close to Excel, and how we know](docs/design/fidelity.md)
- [Theme: colour, type and pixel alignment](docs/design/theme.md)
- [The spreadsheet file format](docs/design/file-format.md)
- [The Ferrum code editor](docs/design/code-editor.md)
- [Open questions](docs/open-questions.md)

## Decisions

- [ADR 0001: Workspace shape](docs/adr/0001-workspace-shape.md)
- [ADR 0002: No third-party dependencies in the shipped product](docs/adr/0002-no-runtime-dependencies.md)
- [ADR 0003: Drive the application through a harness](docs/adr/0003-drive-the-application-through-a-harness.md)

## Licence

MIT.
