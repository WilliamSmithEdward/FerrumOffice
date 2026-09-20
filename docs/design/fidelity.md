# How close to Excel, and how we know

The standing goal for FerrumGrid, and for every application after it:

> **As close as possible to the Excel look and feel.** Somebody who works in
> Excel all day should be able to use FerrumGrid without being told anything,
> and should not be able to point at what moved.

That is the target for the shape of the thing: geometry, gestures, keyboard,
vocabulary, what happens when. It is not a ceiling on quality. Where Excel's
interface is slow, noisy or hard to read, this one is allowed to be better, as
long as somebody arriving from Excel is never wrong about what a thing does or
where it is. Polish sits on top of the familiar shape; it does not replace it.

Two instructions govern the boundary, and neither bends:

- **No copyrightable language, ever.** The reference is measured behaviour and
  geometry: a width in points, a colour a screenshot sampled, which key does
  what. Their wording is theirs. Every label, message and document in this
  repository is written here.
- **Lock-step where it counts.** File formats and the formula language behave
  as the products people already use behave, because a spreadsheet that
  computes a different total is not a spreadsheet.

## What "as close as possible" resolves to

In descending order of how strictly it binds:

1. **Results.** A formula returns what the reference returns, error values and
   edge cases included, or the difference is a recorded defect.
2. **Files.** A file written here opens there and reads back the same, which
   is verified by opening it there. See
   [file-format.md](file-format.md).
3. **Geometry.** Default column width, row height, header sizes, gridline
   placement, font and type size, all at the measured values. See
   [theme.md](theme.md).
4. **Gestures and keys.** The same key does the same thing: typing replaces,
   F2 amends, Enter commits and moves down, Tab commits and moves right,
   Escape abandons, dragging a boundary past its neighbour hides that row or
   column.
5. **Words on screen.** The same meaning in our own words, never theirs.
6. **Ornament.** Ours. Colour, spacing, focus treatment and motion are a
   project decision, held to the contrast and target-size floors in
   `ferrum-theme`.

## Measure, do not recall

A remembered value is a guess wearing a number. Anything in the first four
categories above is settled by measuring the thing being matched, and the
measurement is recorded next to what it produced.

This has already paid twice. A gridline contrast figure quoted from memory as
1.27:1 is 1.3201:1, and the floor invented around the wrong figure would have
rejected Excel's own gridline. A stored column width turned out to carry five
pixels of padding that no amount of reasoning about the format would have
produced, and unpadded columns came out 5% narrow.

The corollary is that a test of our own writer against our own reader proves
nothing. Interoperability claims are checked against the other program.

## Prove it on the screen, not next to it

A fidelity claim is a claim about what is displayed, so it is asserted against
what would be displayed. `ferrum-grid-view` is the whole of what FerrumGrid
shows and does with no toolkit in it, and its `Harness` drives the application
through gestures with no window. See
[ADR 0003](../adr/0003-drive-the-application-through-a-harness.md).

So "a fresh grid has 64-pixel columns and 19-pixel rows" is a test, and so is
"the two themes put every pixel in the same place".

## Known divergences

Recorded rather than quietly tolerated. Each one is either a gap to close or a
decision to defend.

| Divergence | Why, for now |
| --- | --- |
| Text longer than its column is clipped, not spilled into empty neighbours | Not written yet. It is a gap, not a decision. |
| Number formats are not applied; every number shows in the general format | Waiting on the format engine. |
| No copy and paste, no fill handle, no merged cells, no frozen panes | Not written yet. |
| The theme's accent and brand colours are ours | Deliberate. Ornament is a project decision. |

When one of these closes, the row goes and a test replaces it.
