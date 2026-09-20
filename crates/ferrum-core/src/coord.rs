//! Addressing the grid.
//!
//! Rows and columns are zero-based everywhere inside the engine and one-based
//! only where a human sees them, so that indexing never needs an adjustment at
//! the point of use. [`A1Ref`] is the bridge between the two.

use std::fmt;

/// Highest zero-based row index. The grid holds 1,048,576 rows.
pub const MAX_ROW: u32 = 1_048_575;

/// Highest zero-based column index. The grid holds 16,384 columns, `A` to `XFD`.
pub const MAX_COL: u32 = 16_383;

/// Longest column label the grid can produce.
const MAX_COLUMN_LABEL_LEN: usize = 3;

/// Identifies a sheet within one workbook.
///
/// Stable for the lifetime of the workbook: deleting a sheet retires its id
/// rather than renumbering the others, so references held elsewhere cannot
/// silently start pointing at a different sheet.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SheetId(pub u32);

impl fmt::Display for SheetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sheet#{}", self.0)
    }
}

/// A cell together with the sheet it lives on.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CellAddr {
    pub sheet: SheetId,
    pub cell: CellRef,
}

impl CellAddr {
    pub const fn new(sheet: SheetId, cell: CellRef) -> Self {
        Self { sheet, cell }
    }
}

/// A range together with the sheet it lives on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RangeAddr {
    pub sheet: SheetId,
    pub range: RangeRef,
}

impl RangeAddr {
    pub const fn new(sheet: SheetId, range: RangeRef) -> Self {
        Self { sheet, range }
    }

    pub const fn contains(&self, addr: CellAddr) -> bool {
        self.sheet.0 == addr.sheet.0 && self.range.contains(addr.cell)
    }
}

/// A cell position. Zero-based on both axes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct CellRef {
    pub row: u32,
    pub col: u32,
}

impl CellRef {
    pub const fn new(row: u32, col: u32) -> Self {
        Self { row, col }
    }

    /// True when both axes are inside the grid.
    pub const fn is_valid(self) -> bool {
        self.row <= MAX_ROW && self.col <= MAX_COL
    }

    /// Move by a signed offset, returning `None` if that leaves the grid.
    ///
    /// Used when a formula is copied: a relative reference shifts by the same
    /// delta as the cell holding it.
    pub fn offset(self, rows: i64, cols: i64) -> Option<Self> {
        let row = u32::try_from(i64::from(self.row).checked_add(rows)?).ok()?;
        let col = u32::try_from(i64::from(self.col).checked_add(cols)?).ok()?;
        let moved = Self { row, col };
        moved.is_valid().then_some(moved)
    }

    /// Render as `A1`.
    pub fn to_a1(self) -> String {
        format!("{}{}", column_label(self.col), self.row + 1)
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_a1())
    }
}

/// A cell reference as written in a formula, keeping the `$` markers.
///
/// The markers do not affect which cell is addressed. They decide whether the
/// reference moves when the formula is copied.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct A1Ref {
    pub cell: CellRef,
    pub col_absolute: bool,
    pub row_absolute: bool,
}

impl A1Ref {
    pub const fn relative(cell: CellRef) -> Self {
        Self {
            cell,
            col_absolute: false,
            row_absolute: false,
        }
    }

    pub const fn absolute(cell: CellRef) -> Self {
        Self {
            cell,
            col_absolute: true,
            row_absolute: true,
        }
    }

    /// Parse one reference, such as `A1`, `$B$7` or `c$3`.
    pub fn parse(text: &str) -> Result<Self, ParseRefError> {
        let bytes = text.as_bytes();
        let mut at = 0;

        let col_absolute = bytes.first() == Some(&b'$');
        if col_absolute {
            at += 1;
        }

        let letters_start = at;
        while at < bytes.len() && bytes[at].is_ascii_alphabetic() {
            at += 1;
        }
        if at == letters_start {
            return Err(ParseRefError::Malformed);
        }
        let col = parse_column_label(&text[letters_start..at]).ok_or(ParseRefError::ColumnRange)?;

        let row_absolute = bytes.get(at) == Some(&b'$');
        if row_absolute {
            at += 1;
        }

        let digits_start = at;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        if at == digits_start || at != bytes.len() {
            return Err(ParseRefError::Malformed);
        }
        let one_based: u32 = text[digits_start..at]
            .parse()
            .map_err(|_| ParseRefError::RowRange)?;
        let row = one_based
            .checked_sub(1)
            .filter(|r| *r <= MAX_ROW)
            .ok_or(ParseRefError::RowRange)?;

        Ok(Self {
            cell: CellRef { row, col },
            col_absolute,
            row_absolute,
        })
    }

    /// Render with the `$` markers the reference was written with.
    pub fn to_a1(self) -> String {
        let col_marker = if self.col_absolute { "$" } else { "" };
        let row_marker = if self.row_absolute { "$" } else { "" };
        format!(
            "{col_marker}{}{row_marker}{}",
            column_label(self.cell.col),
            self.cell.row + 1
        )
    }

    /// Shift the parts of this reference that are relative.
    ///
    /// Returns `None` when the result would leave the grid, which is the
    /// condition that turns a copied reference into `#REF!`.
    pub fn offset(self, rows: i64, cols: i64) -> Option<Self> {
        let moved = self.cell.offset(
            if self.row_absolute { 0 } else { rows },
            if self.col_absolute { 0 } else { cols },
        )?;
        Some(Self {
            cell: moved,
            ..self
        })
    }
}

impl fmt::Display for A1Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_a1())
    }
}

/// A rectangular block of cells, inclusive of both corners.
///
/// Always normalised so that `start` is the top-left corner. A whole column is
/// a range spanning every row; a whole row spans every column.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RangeRef {
    pub start: CellRef,
    pub end: CellRef,
}

impl RangeRef {
    /// Build a range from two opposite corners, in either order.
    pub fn new(a: CellRef, b: CellRef) -> Self {
        Self {
            start: CellRef {
                row: a.row.min(b.row),
                col: a.col.min(b.col),
            },
            end: CellRef {
                row: a.row.max(b.row),
                col: a.col.max(b.col),
            },
        }
    }

    pub const fn single(cell: CellRef) -> Self {
        Self {
            start: cell,
            end: cell,
        }
    }

    /// Every row of the given columns.
    pub const fn whole_columns(first: u32, last: u32) -> Self {
        Self {
            start: CellRef::new(0, first),
            end: CellRef::new(MAX_ROW, last),
        }
    }

    /// Every column of the given rows.
    pub const fn whole_rows(first: u32, last: u32) -> Self {
        Self {
            start: CellRef::new(first, 0),
            end: CellRef::new(last, MAX_COL),
        }
    }

    pub const fn height(&self) -> u32 {
        self.end.row - self.start.row + 1
    }

    pub const fn width(&self) -> u32 {
        self.end.col - self.start.col + 1
    }

    /// Number of cells covered. A whole column alone exceeds `u32`.
    pub const fn area(&self) -> u64 {
        self.height() as u64 * self.width() as u64
    }

    pub const fn contains(&self, cell: CellRef) -> bool {
        cell.row >= self.start.row
            && cell.row <= self.end.row
            && cell.col >= self.start.col
            && cell.col <= self.end.col
    }

    pub const fn intersects(&self, other: &Self) -> bool {
        self.start.row <= other.end.row
            && other.start.row <= self.end.row
            && self.start.col <= other.end.col
            && other.start.col <= self.end.col
    }

    /// The overlap of two ranges, or `None` when they are disjoint.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        self.intersects(other).then(|| Self {
            start: CellRef {
                row: self.start.row.max(other.start.row),
                col: self.start.col.max(other.start.col),
            },
            end: CellRef {
                row: self.end.row.min(other.end.row),
                col: self.end.col.min(other.end.col),
            },
        })
    }

    /// The smallest range covering both operands.
    pub fn union_bounds(&self, other: &Self) -> Self {
        Self {
            start: CellRef {
                row: self.start.row.min(other.start.row),
                col: self.start.col.min(other.start.col),
            },
            end: CellRef {
                row: self.end.row.max(other.end.row),
                col: self.end.col.max(other.end.col),
            },
        }
    }

    /// Walk the range in reading order: left to right, then top to bottom.
    pub fn cells(&self) -> impl Iterator<Item = CellRef> + use<> {
        let (start, end) = (self.start, self.end);
        (start.row..=end.row)
            .flat_map(move |row| (start.col..=end.col).map(move |col| CellRef { row, col }))
    }

    /// Render as `A1:B2`, collapsing a single cell to `A1`.
    pub fn to_a1(&self) -> String {
        if self.start == self.end {
            self.start.to_a1()
        } else {
            format!("{}:{}", self.start.to_a1(), self.end.to_a1())
        }
    }
}

impl fmt::Display for RangeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_a1())
    }
}

/// Why a reference could not be read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseRefError {
    /// The text is not shaped like a reference at all.
    Malformed,
    /// The column letters name a column past the last one.
    ColumnRange,
    /// The row number is zero, or past the last row.
    RowRange,
}

impl fmt::Display for ParseRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "not a cell reference",
            Self::ColumnRange => "column is outside the grid",
            Self::RowRange => "row is outside the grid",
        })
    }
}

impl std::error::Error for ParseRefError {}

/// Turn a zero-based column index into its letters: 0 gives `A`, 16383 gives `XFD`.
pub fn column_label(col: u32) -> String {
    let mut remaining = col + 1;
    let mut letters = [0u8; MAX_COLUMN_LABEL_LEN];
    let mut written = 0;
    while remaining > 0 && written < MAX_COLUMN_LABEL_LEN {
        let digit = (remaining - 1) % 26;
        letters[written] = b'A' + digit as u8;
        written += 1;
        remaining = (remaining - 1) / 26;
    }
    letters[..written].reverse();
    // Every byte written is an ASCII letter.
    String::from_utf8_lossy(&letters[..written]).into_owned()
}

/// Read column letters into a zero-based index. Case-insensitive.
///
/// Returns `None` for anything that is not one to three letters naming a
/// column inside the grid.
pub fn parse_column_label(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > MAX_COLUMN_LABEL_LEN {
        return None;
    }
    let mut value: u32 = 0;
    for byte in letters.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a',
            _ => return None,
        };
        value = value * 26 + u32::from(digit) + 1;
    }
    let col = value.checked_sub(1)?;
    (col <= MAX_COL).then_some(col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_labels_cover_the_whole_grid() {
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
        assert_eq!(column_label(51), "AZ");
        assert_eq!(column_label(52), "BA");
        assert_eq!(column_label(701), "ZZ");
        assert_eq!(column_label(702), "AAA");
        assert_eq!(column_label(MAX_COL), "XFD");
    }

    #[test]
    fn column_labels_round_trip() {
        for col in [0, 1, 25, 26, 27, 701, 702, 16_000, MAX_COL] {
            assert_eq!(
                parse_column_label(&column_label(col)),
                Some(col),
                "column {col}"
            );
        }
    }

    #[test]
    fn column_labels_are_case_insensitive() {
        assert_eq!(parse_column_label("xfd"), Some(MAX_COL));
        assert_eq!(parse_column_label("XfD"), Some(MAX_COL));
    }

    #[test]
    fn columns_past_the_last_are_rejected() {
        assert_eq!(parse_column_label("XFE"), None);
        assert_eq!(parse_column_label("ZZZ"), None);
        assert_eq!(parse_column_label("AAAA"), None);
        assert_eq!(parse_column_label(""), None);
        assert_eq!(parse_column_label("A1"), None);
    }

    #[test]
    fn a1_references_parse_with_their_markers() {
        let plain = A1Ref::parse("B3").unwrap();
        assert_eq!(plain.cell, CellRef::new(2, 1));
        assert!(!plain.col_absolute && !plain.row_absolute);

        let pinned = A1Ref::parse("$B$3").unwrap();
        assert_eq!(pinned.cell, CellRef::new(2, 1));
        assert!(pinned.col_absolute && pinned.row_absolute);

        let mixed = A1Ref::parse("B$3").unwrap();
        assert!(!mixed.col_absolute && mixed.row_absolute);

        let other_mixed = A1Ref::parse("$B3").unwrap();
        assert!(other_mixed.col_absolute && !other_mixed.row_absolute);
    }

    #[test]
    fn a1_references_round_trip() {
        for text in ["A1", "$A1", "A$1", "$A$1", "XFD1048576", "Z99"] {
            assert_eq!(A1Ref::parse(text).unwrap().to_a1(), text);
        }
    }

    #[test]
    fn malformed_references_are_rejected() {
        assert_eq!(A1Ref::parse("1A"), Err(ParseRefError::Malformed));
        assert_eq!(A1Ref::parse("A"), Err(ParseRefError::Malformed));
        assert_eq!(A1Ref::parse("A1x"), Err(ParseRefError::Malformed));
        assert_eq!(A1Ref::parse(""), Err(ParseRefError::Malformed));
        assert_eq!(A1Ref::parse("A0"), Err(ParseRefError::RowRange));
        assert_eq!(A1Ref::parse("A1048577"), Err(ParseRefError::RowRange));
        assert_eq!(A1Ref::parse("XFE1"), Err(ParseRefError::ColumnRange));
    }

    #[test]
    fn offsets_move_only_the_relative_parts() {
        let mixed = A1Ref::parse("$B3").unwrap();
        assert_eq!(mixed.offset(2, 5).unwrap().to_a1(), "$B5");

        let relative = A1Ref::parse("B3").unwrap();
        assert_eq!(relative.offset(2, 5).unwrap().to_a1(), "G5");

        let pinned = A1Ref::parse("$B$3").unwrap();
        assert_eq!(pinned.offset(2, 5).unwrap().to_a1(), "$B$3");
    }

    #[test]
    fn offsets_off_the_grid_fail() {
        let corner = A1Ref::parse("A1").unwrap();
        assert_eq!(corner.offset(-1, 0), None);
        assert_eq!(corner.offset(0, -1), None);
        assert_eq!(corner.offset(MAX_ROW as i64 + 1, 0), None);
    }

    #[test]
    fn ranges_normalise_their_corners() {
        let range = RangeRef::new(CellRef::new(5, 5), CellRef::new(1, 2));
        assert_eq!(range.start, CellRef::new(1, 2));
        assert_eq!(range.end, CellRef::new(5, 5));
        assert_eq!(range.to_a1(), "C2:F6");
    }

    #[test]
    fn a_single_cell_range_renders_without_a_colon() {
        assert_eq!(RangeRef::single(CellRef::new(0, 0)).to_a1(), "A1");
    }

    #[test]
    fn whole_column_area_exceeds_a_u32() {
        let column = RangeRef::whole_columns(0, 0);
        assert_eq!(column.area(), 1_048_576);
        assert_eq!(RangeRef::whole_columns(0, MAX_COL).area(), 17_179_869_184);
    }

    #[test]
    fn range_intersection_is_the_overlap() {
        let left = RangeRef::new(CellRef::new(0, 0), CellRef::new(4, 4));
        let right = RangeRef::new(CellRef::new(3, 3), CellRef::new(9, 9));
        assert_eq!(
            left.intersection(&right),
            Some(RangeRef::new(CellRef::new(3, 3), CellRef::new(4, 4)))
        );

        let apart = RangeRef::new(CellRef::new(20, 20), CellRef::new(21, 21));
        assert_eq!(left.intersection(&apart), None);
    }

    #[test]
    fn ranges_walk_in_reading_order() {
        let range = RangeRef::new(CellRef::new(0, 0), CellRef::new(1, 1));
        let visited: Vec<_> = range.cells().map(|c| c.to_a1()).collect();
        assert_eq!(visited, ["A1", "B1", "A2", "B2"]);
    }
}
