//! What a new sheet looks like before anyone changes it.
//!
//! These are document values, not rendering values: a column width is stored
//! in the file and survives being opened on a different screen. They live here
//! rather than beside the palette because the document model needs them and
//! must not depend on anything about drawing.
//!
//! Every figure was measured from a live spreadsheet rather than recalled. The
//! measurement and its method are in `docs/design/theme.md`.

/// Default column width, in points. Measured: 48.0, which is 64 pixels at
/// 96 DPI.
pub const COLUMN_WIDTH_PT: f64 = 48.0;

/// Default row height, in points. Measured: 14.5, which is 29 pixels at
/// 144 DPI.
pub const ROW_HEIGHT_PT: f64 = 14.5;

/// Default column width as a spreadsheet's own dialog expresses it: in
/// characters of the standard font. Measured: 8.09.
pub const COLUMN_WIDTH_CHARS: f64 = 8.09;

/// Default body font size, in points. Measured: 11.0.
pub const FONT_SIZE_PT: f64 = 11.0;

/// Preferred body font, in order.
///
/// The first entry is what a current spreadsheet uses, and picking it up when
/// the machine already has it is what makes a sheet look right. It is not
/// redistributed here. Layout does not depend on which entry resolves, because
/// column widths are stored in points rather than in characters.
pub const FONT_STACK: &[&str] = &[
    "Aptos Narrow",
    "Calibri",
    "Segoe UI",
    "Liberation Sans",
    "DejaVu Sans",
];

/// Smallest a column or row may be dragged to before it counts as hidden.
///
/// Dragging an edge past its neighbour should collapse the row rather than
/// leave a sliver nobody can grab again.
pub const MIN_VISIBLE_PT: f64 = 1.5;
