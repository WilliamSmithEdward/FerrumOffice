# The Ferrum code editor

Working name **FerrumForge**: the VBA authoring surface inside FerrumGrid, and
later inside the other three applications. Naming is not settled; see
[open-questions.md](../open-questions.md).

The reference is `F:\GitHub\xlide\xlide_vbide`, the author's own product, so
its design may be reused directly. This document is the analysis of what that
surface does well, what it had to fight, and which of its conclusions transfer
to a Rust and Slint implementation that has no native editor underneath it.

## 1. Why the reference is worth copying

xlide replaces the 1998 Visual Basic Editor surface with the VS Code shell:
docked tool panes, a tab strip per editor group, an editor with a gutter and a
minimap, and a bottom panel of diagnostics and debug views. It keeps every
native engine and replaces every native surface.

The layout, read off `shot.png` and confirmed against the source:

```text
+--------------------------------------------------------------------------+
| title bar        FerrumGrid - Book1.xlsm - Shapes            _  []  X     |
+--------------------------------------------------------------------------+
| toolbar   open save | undo redo | run pause stop | step in/over/out | ... |
+------------------+-------------------------------------------------------+
| Explorer      [x]| tab strip: Module1 | Sheet1 | ThisWorkbook | ...       |
|  v Book1.xlsm    +-------------------------------------------------------+
|    Sheet1    doc |  1 | Option Explicit                          |minimap|
|    ThisWorkbook  |  2 |                                          |       |
|    EntryForm form|  3 | ' a comment                              |       |
|    Module1   std |> 4 |     TopLeft            <- current stmt   |       |
|    Module2   std (2) 5 |     TopRight          <- breakpoint     |       |
|                  |  6 | End Enum                                 |       |
+------------------+                                                       |
| Properties    [x]|                                                       |
|  Shapes   Module |                                                       |
|  (Name)   Shapes |                                                       |
+------------------+-------------------------------------------------------+
| Problems | Immediate | Locals | Watch | Tests | Changes | Source Control  |
| (x)14 Errors  (!)1 Warning  (i)0 Messages        [All Modules (15) v]     |
|  x Missing 'End Sub' for 'Sub Broken'.                                    |
|      Module2 (3, 8) missing-block-closer                                  |
+--------------------------------------------------------------------------+
| Ln 1, Col 1    Shapes                                                     |
+--------------------------------------------------------------------------+
```

Density is the point. Nine tool panes, a full diagnostics list and twenty lines
of code are on screen at once without the layout feeling crowded, because the
chrome is nearly flat: no gradients, no borders where spacing will do, one
accent colour, and roughly 24px of vertical space per row of chrome.

## 2. The ergonomic decisions worth inheriting

These are the findings from `docs/ui-lessons.md`, restated as rules for
FerrumForge. Each one cost the reference project real debugging time, so each
is cheaper to adopt than to rediscover.

### 2.1 Docking

- **A five-zone drop compass, not a guess from pointer position.** Deriving
  intent from where the pointer sits in a region fails on real geometry: over a
  wide, short panel, "near the left edge" and "just left of centre" are a few
  pixels apart, and an if-chain that tests x before y sends a point near the
  top edge but left of centre to the left. Draw the compass and make the user
  aim at a zone. It is more motion and it converts a guess into an aim.
- **Hit-test the compass geometrically.** During a drag the dragged element
  holds the pointer capture, so nothing else receives pointer events. Compare
  the pointer position against zone rectangles.
- **Offer only zones the drop will honour.** A group's only tab cannot split
  against its own group. A lit zone that does nothing is reported as a bug,
  correctly. Compute the offered zones per region.
- **The preview describes the real outcome.** Dropping on an edge where a
  section already exists joins that section, so outline the section rather than
  half the editor. A new section gets a dashed edge, because it is a proposal
  rather than a place. Use a light wash with a definite edge; a heavy
  translucent slab reads as "your editor has been replaced".
- **Reordering uses the strip itself as feedback.** Move the dragged tab past
  its neighbours' midpoints in the live strip. The answer to "where will this
  land" is the strip already showing it.
- **A drag ends on more than pointerup.** Window blur, visibility change and
  Escape all end a gesture without producing a pointer event. Without them the
  dim and the compass outlive the drag.

### 2.2 Identity and state

- **State lives on the document, not on the view.** Undo stack, markers,
  decorations, scroll position and selection hang off the open document. Tab
  switching then costs nothing (the reference measures a 2.5ms median) and a
  background module's diagnostics keep updating.
- **The stopped-statement highlight and breakpoint dots are document state**,
  so they are right in every view showing that document.
- **Identity is a pair, never a name.** Two workbooks can each hold a
  `Module1`. Every key (document id, tab key, view-state key, baseline key) is
  the (workbook, module) pair. A name-only key is a latent corruption that
  surfaces the day both are open.
- **Rebuild only when something drawn changed.** Give each strip a render key
  covering everything it draws: identity, order, active item, badge counts,
  dirty flags. An echo that changes nothing rebuilds nothing.

### 2.3 Undo

This is the deepest theme across the reference's documents.

- **One undo gives back one gesture.** A whole drag, a deleted page with all
  its children, a Replace All across a module, a rename across modules. Not
  five steps because the gesture touched five properties.
- **No machine-authored step in the user's chain.** An undo that first walks
  through a step differing only by the machine's own reformatting is an undo
  the user did not make and cannot predict.
- **Two views of one document share one stack.** The form canvas and its text
  view edit the same document, and undo from either half takes back the same
  gesture.

### 2.4 Layout persistence

- **Membership belongs to the model; geography belongs to the user.** Which
  documents are open is the workbook's answer. Where each pane sits and how big
  it is belongs to the arrangement and must survive every model change.
- **Arrangement is per-machine state, not a product setting.** It describes one
  screen, not a preference about behaviour.
- **A stored arrangement must tolerate a changed product.** Drop panes that no
  longer exist; place panes the stored layout has never heard of somewhere
  sensible rather than letting them vanish.
- **Every closable pane needs a route back**, through a View menu that lists
  it. One pane, the explorer, cannot be closed at all, because with every tab
  shut it is the only way back to a document. Say so rather than silently
  refusing.
- **The split tree is a pure module, tested on its own.** Pruning empty groups,
  collapsing single-child splits, absorbing same-axis splits and keeping sizes
  a partition of one is the most error-prone code in a docking layout and the
  slowest to test through drags. As a pure function it is a dozen assertions
  that name the case they broke.

### 2.5 Authoring behaviour

- Diagnostics as you type, with severity filter chips and a scope selector,
  and a click that navigates to the line and column.
- A diagnostic is shown only when it can be proven. Ambiguity produces
  silence, and red means the compiler will reject this.
- Typing follows the language's own conventions: auto-casing of known
  identifiers, automatic block closers, smart Enter and Tab, auto-indent.
- Completion, hover and signature help, answered for the document being asked
  about rather than for whichever one is focused.
- Search is one floating widget, scoped to the document, the workbook, or every
  open workbook, with Find All showing a preview per match.
- The status bar names the procedure the caret is in, beside the document name,
  so position reads off the screen without scrolling.

### 2.6 Debugging

Break mode is a first-class visual state, not a dialog:

- The current statement gets a highlighted line and a gutter arrow.
- Breakpoints are dots in the gutter, toggled by clicking it.
- Locals and Watch track every step; Immediate evaluates against live state.
- Run to Cursor and Set Next Statement are on the toolbar, not buried.
- The title bar carries a break marker.

The reference notes one thing the native editor never had and we should keep:
**breakpoints that survive a session.**

## 3. Where FerrumForge differs

The reference is a surface drawn over a native editor it does not own. Three
consequences of that do not apply to us, and one new problem does.

**We own the engine, so there is no synchronisation layer.** xlide spends
significant machinery keeping Monaco converged with the native `CodeModule`:
revision numbers, edit coalescing, echo suppression, wholesale resynchronisation
on divergence. FerrumGrid holds the only copy of the text. That entire category
of bug does not exist here, and we should not invent an equivalent.

**We own the debugger**, so break state is a value we publish rather than a
condition we detect from a window caption and a change in command availability.

**We own the process model.** xlide runs its analyzer out of process because
the VBE is single threaded and owns the typing thread. We have the same
requirement for a different reason: analysis must not block the frame. The
answer in Rust is a worker thread with a channel, not a separate executable.

**The new problem: there is no Monaco.** Monaco supplied folding, multi-cursor,
minimap, find and replace, bracket matching, semantic highlighting and a real
undo stack as configuration. In Slint those are ours to build. That is the
single largest piece of work in this document and the main risk to the
schedule. It is recorded as a decision to take deliberately rather than to
discover; see [open-questions.md](../open-questions.md).

## 4. Improvements over the reference

The user asked for liberties. These are the ones worth taking.

- **One theme across the suite.** The editor and the grid share the token set
  in [theme.md](theme.md), so a workbook and its code look like one product
  rather than two. The reference is dark only; Ferrum ships light and dark, and
  both meet the contrast floor in section 5.
- **The command palette as the primary route.** The reference reaches
  everything through a toolbar and menus. A palette makes the surface
  self-documenting and lets the toolbar carry fewer, larger targets.
- **Diagnostics in the explorer and the tab strip, not only the panel.** The
  reference already badges modules with error counts in the explorer, which is
  the single best idea in its navigation. Extend it to the tab strip so a
  problem in a background document is visible without opening the panel.
- **A structure view of the open document** (procedures, types, constants),
  which the reference does not have, and which is how a 2,000-line module
  becomes navigable.
- **Inline values during break.** Locals answers "what is `i`" in a pane; the
  editor can answer it at the end of the line, which is where the eye already
  is.
- **Keyboard-first docking.** Drag is the discoverable route; every dock,
  split and focus move also needs a binding, because a docking layout that can
  only be arranged with a pointer is inaccessible.

## 5. The bar this surface is held to

From the UI/UX guidelines this project adopted:

- Text contrast at least 4.5:1, and 3:1 for icons, borders and focus rings.
- Never colour alone: a breakpoint is a shape, an error is an icon plus text,
  a dirty tab is a dot and not a tint.
- Every control reachable by keyboard, focus order matching reading order, a
  visible focus ring that is never removed without a replacement.
- Motion honours the reduced-motion preference.
- Targets at least 24x24px, with the gutter's breakpoint strip a deliberate
  exception that gets extra hit area rather than extra pixels.
- Empty states teach: an editor with no document open offers the recent list
  and a new-module action, never a blank panel.

## 6. What is not decided

Recorded in [open-questions.md](../open-questions.md): the product name, the
text-editing core, and whether the VBA engine is written fresh in Rust or
ported from `pyOpenVBA`.
