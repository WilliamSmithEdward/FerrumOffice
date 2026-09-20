# The spreadsheet file format

A spreadsheet file is a ZIP archive of XML documents. `ferrum-xlsx` writes
both halves, because ADR 0002 rules out taking either from the registry.

Writing came first. It needs a ZIP writer, an XML writer and a CRC-32, all of
which are small. Reading a file written elsewhere additionally needs a DEFLATE
decoder, which is the larger piece and is not written yet.

## Entries are stored, not compressed

Every entry is written with method 0, stored. A stored ZIP is a valid ZIP that
every reader accepts, and it costs a DEFLATE encoder that would otherwise have
had to come first. A workbook of a few hundred kilobytes of XML is not worth
the compression until there is a decoder anyway.

Timestamps default to the earliest the format can express, which makes the
output byte-for-byte reproducible: two saves of one workbook compare equal, and
a test can rely on that.

## What the package contains

```text
[Content_Types].xml        what each part is
_rels/.rels                the way in
xl/workbook.xml            the list of sheets
xl/_rels/workbook.xml.rels where each sheet lives
xl/styles.xml              the minimum a reader accepts
xl/worksheets/sheetN.xml   one per sheet
```

Text is written inline, as `t="inlineStr"`, rather than through a shared
strings table. Both are valid. The table is an optimisation for files with a
lot of repeated text, not a requirement, and it is one part fewer to get wrong.

## Two things measured against Excel

The only way to know a file format is right is to hand the file to something
else. A test of our writer against our own reader would agree with itself
however wrong both were. Both of the following were found by writing a file,
opening it in Excel over COM, and reading the values back.

### A sheet must say how it is being looked at

**Omitting `<sheetViews>` makes a reader scale every custom row height by
exactly two thirds.** A row written at 20 points comes back as 13.4, at 30
points as 20, at 15 as 10. The ratio is 0.6667 across the range.

Nothing about the element suggests it has anything to do with row heights. It
looks like a cosmetic record of scroll position and selected tab. The fix is
one element:

```xml
<sheetViews><sheetView tabSelected="1" workbookViewId="0"/></sheetViews>
```

Found by bisection: substituting parts of our package into one Excel had
written, then building variants of our own sheet with and without each
element. `sheetFormatPr`, the obvious suspect because it carries
`defaultRowHeight`, made no difference at all.

### A stored column width carries five pixels of padding

The `width` attribute is not the number of characters a spreadsheet reports.
It is that number plus five pixels of cell padding, expressed in character
units:

```text
stored = characters + 5 / {width of a digit in the default font}
```

Measured: asked for a column of 12.135 characters, Excel set 12.09 (it
quantises to whole pixels) and stored `12.7265625`. The difference,
0.6365625, is the padding. Writing the unpadded number produced a column
about 5% too narrow.

The constant depends on the default font's digit width, so it is a measured
value for the font in `ferrum_core::defaults::FONT_STACK` rather than a
derivation. A different default font would need it measured again.

## Element order is fixed

The schema is a sequence, not a set. A worksheet's children must appear in
this order, and a reader rejects the file otherwise:

```text
dimension, sheetViews, sheetFormatPr, cols, sheetData
```

## What is verified

`cargo run -p ferrum-xlsx --example save -- out.xlsx` writes a sample
exercising the cases most likely to produce a file a reader rejects: markup
characters in text and in a formula, an error value, a logical, text outside
the basic multilingual plane, a resized row and column, and a second sheet.

Opening that file in Excel and reading it back confirms the sheet names, the
row height and column width in the units Excel reports them, the formulas with
their leading `=` restored, the recalculated values, and the text unmangled.

## Not written yet

- **Reading.** Needs INFLATE, a ZIP reader and an XML reader.
- **Shared strings**, which a file written elsewhere will use and a reader
  must therefore understand.
- **Number formats and styles.** The styles part is the minimum a reader
  accepts and carries nothing of the document's own.
- **Everything beyond cells**: merged ranges, frozen panes, defined names,
  charts, conditional formatting, tables.
