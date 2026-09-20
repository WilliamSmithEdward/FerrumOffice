# Theme: colour, type and pixel alignment

The tokens themselves live in `crates/ferrum-theme` and are the single source
of truth. This document records where the numbers came from and why the rules
are the rules.

Two invariants hold everything together:

1. **Light is the default, dark is an option.**
2. **Geometry does not depend on the theme.** Sizing, pixel alignment and zoom
   are identical in both, so switching theme never moves a cell boundary and a
   layout bug cannot hide in one theme and not the other. Everything
   dimensional is in `metrics.rs`, which has no colours in it, and every colour
   is in `palette.rs`, which has no dimensions in it.

## 1. What was measured, and how

The light theme is meant to look like the Excel sheet surface, so the sheet
surface was measured rather than recalled. An owned Excel instance was created
over COM (a new process, never attaching to a running one), given a blank
workbook, read through the object model, screenshotted, and closed without
saving.

From the object model:

| Property | Value |
| --- | --- |
| `Application.StandardFont` | Aptos Narrow |
| `Application.StandardFontSize` | 11.0 pt |
| `Worksheet.StandardHeight` | 14.5 pt |
| `Worksheet.StandardWidth` | 8.09 characters |
| `Range("A1").Width` | 48.0 pt |
| `Range("A1").Height` | 14.5 pt |
| `Window.Zoom` | 100 |
| `Window.GridlineColor` | 0, meaning automatic |

The gridline reporting as automatic is why a screenshot was needed: Excel does
not say what it actually draws. Sampled from the capture, which was taken at
150% display scaling (144 DPI):

| Element | Value |
| --- | --- |
| Cell background | `#ffffff` |
| Gridline, horizontal and vertical | `#e0e0e0` |
| Column pitch | 96 px, exactly 8 columns measured |
| Row pitch | 29 px, exactly 10 rows measured |

The pitches confirm the point values exactly: 48pt at 144 DPI is 96px, and
14.5pt at 144 DPI is 29px. So the point figures are authoritative and the
pixel figures follow from the display density.

One thing the capture could not give: the row and column header colours. The
machine runs Excel with a dark Office theme, so the headers sampled near-black
while the sheet stayed white. Header chrome is an Office theme setting rather
than a property of the sheet surface, so Ferrum defines its own from the
palette instead of copying a value that varies. Changing the user's Office
theme to measure it was not worth doing.

### Correcting a figure

The first draft of this work asserted that `#e0e0e0` on `#ffffff` is 1.27:1 and
set a hairline floor of 1.5:1 from nothing in particular. Both were wrong. The
real ratio computes to **1.3201:1**, and the invented floor would have rejected
Excel's own gridline. The floor is now 1.30, derived from the measurement, and
a test computes the ratio rather than quoting it.

## 2. Type

The body font is **Aptos Narrow 11pt** where the machine has it, which is what
makes a sheet look right to someone coming from Excel. It is not redistributed
with Ferrum. The fallback chain is Calibri, Segoe UI, Liberation Sans, DejaVu
Sans.

Column widths are stored in points, not in characters, so the layout does not
move when the font falls back. The character figure (8.09) is a presentation
detail for the column-width dialog.

## 3. Pixel alignment

Cell boundaries land on whole device pixels, so a one-pixel gridline stays one
pixel instead of blurring across two. `Metrics::snap` does the rounding, and
nothing dimensional bypasses it.

Zoom multiplies before snapping, so the grid stays crisp at every zoom rather
than accumulating fractional error across a thousand columns. Zoom is clamped
to 10% to 400%.

A dimension never snaps to zero: a hidden row is a state, not a rounding
result.

## 4. Colour

The hues are **purple** and **rust**.

Purple carries interaction: selection, focus rings, the active cell outline,
the active tab. It was chosen for the job because it collides with none of the
semantic colours, so a selected cell never reads as an error or a success.

Rust is the brand, which suits a product named after iron. It appears in the
mark and in warm highlights, and it is used sparingly.

The neutrals are warm rather than blue-grey, because a cold grey sitting beside
a rust accent reads as an accident.

### The one awkward consequence

Rust and semantic red are neighbours on the colour wheel. They are held apart
by **hue**, not by lightness: the brand sits near 20 degrees and reads as burnt
orange, the error sits near 352 degrees and reads as crimson.

Contrast ratio is the wrong instrument for this question. It measures
lightness, and two colours can share a luminance exactly while looking nothing
alike. `hue_separation` is the right one, and a test keeps the two at least 20
degrees apart.

That 20 degree bar is **this project's own choice, not a borrowed standard.**
No specification says how far apart two hues must sit. It is a backstop against
drift. The real defence is that an error is always an icon and a message as
well as a colour.

## 5. The contrast rules, and where they are enforced

| Kind | Floor | Source |
| --- | --- | --- |
| Body text | 4.5:1 | WCAG 2.x AA, 1.4.3 |
| Icons, borders, focus rings | 3:1 | WCAG 2.x AA, 1.4.11 |
| Gridlines and dividers | 1.30:1 | This project, calibrated to Excel's measured 1.3201:1 |

Every text token is checked against every surface it can land on, including
the selection wash and the debugger's stopped-line tint, which are the two
backgrounds that quietly break a palette. The check is a unit test in
`ferrum-theme`, so a token that fails cannot be committed, and the failure
names the token, the surface and the ratio it managed.

Run it with:

```bash
cargo test -p ferrum-theme
```

This check lives in Rust rather than in a script because the repository is pure
Rust and because a check beside the values it checks cannot drift from them.

## 6. Rules that a test cannot catch

- Never colour alone. A breakpoint is a shape, an error is an icon plus text, a
  dirty tab is a dot rather than a tint, a negative number is a sign rather
  than only red.
- A focus ring is never removed without an equivalent replacement.
- Motion honours the reduced-motion preference.
- Pointer targets are at least 24x24 px, with the gridline's resize handles a
  deliberate exception that gets extra hit area rather than extra pixels.

## 7. Open

Header chrome colours for the grid's row and column headers are derived from
the palette rather than measured, for the reason in section 1. If matching
Excel's light Office theme exactly turns out to matter, it needs a measurement
on a machine set to that theme.
