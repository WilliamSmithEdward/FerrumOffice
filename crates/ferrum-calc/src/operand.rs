//! What an expression evaluates to before it becomes a cell's value.
//!
//! Three kinds, because the language distinguishes them:
//!
//! - a **value** is one scalar,
//! - an **array** is a rectangle of scalars with no home on a sheet,
//! - a **reference** is one or more rectangles that do have a home.
//!
//! The distinction matters. `ROW(A5)` needs to know it was handed a
//! reference, not the number sitting in A5, and `SUM((A1:A2,C1:C2))` needs the
//! union to survive as two areas rather than collapsing into a bounding box
//! that would also sweep up B1 and B2.

use ferrum_core::{CalcError, CellRef, RangeAddr, RangeRef, SheetId, Value};

/// A rectangle of values with no position on a sheet.
///
/// Stored row-major. Always at least one cell: an empty array is not a thing
/// the language can express.
#[derive(Clone, PartialEq, Debug)]
pub struct Array {
    rows: u32,
    cols: u32,
    cells: Vec<Value>,
}

impl Array {
    /// Build from row-major cells. Returns `None` if the length does not
    /// match the stated shape, or if either dimension is zero.
    pub fn new(rows: u32, cols: u32, cells: Vec<Value>) -> Option<Self> {
        if rows == 0 || cols == 0 || cells.len() as u64 != u64::from(rows) * u64::from(cols) {
            return None;
        }
        Some(Self { rows, cols, cells })
    }

    /// A one-by-one array, which is how a scalar enters array arithmetic.
    pub fn scalar(value: Value) -> Self {
        Self {
            rows: 1,
            cols: 1,
            cells: vec![value],
        }
    }

    /// Build from rows of values, padding nothing: the rows must match.
    pub fn from_rows(rows: Vec<Vec<Value>>) -> Option<Self> {
        let height = rows.len() as u32;
        let width = rows.first()?.len() as u32;
        if rows.iter().any(|r| r.len() as u32 != width) {
            return None;
        }
        Self::new(height, width, rows.into_iter().flatten().collect())
    }

    /// A single column, which is the shape most lookups return.
    pub fn column(values: Vec<Value>) -> Option<Self> {
        let height = values.len() as u32;
        Self::new(height, 1, values)
    }

    pub const fn rows(&self) -> u32 {
        self.rows
    }

    pub const fn cols(&self) -> u32 {
        self.cols
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// The value at a position, or `None` when it is outside the rectangle.
    pub fn get(&self, row: u32, col: u32) -> Option<&Value> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.cells
            .get((row as usize) * (self.cols as usize) + col as usize)
    }

    /// The value at a position, stretching a single row or column the way
    /// broadcasting does.
    ///
    /// A one-row array read at any row gives its only row, and the same for a
    /// one-column array. Anything else outside the rectangle is `#N/A`, which
    /// is what a mismatched pair of arrays produces in the corners neither
    /// covers.
    pub fn get_broadcast(&self, row: u32, col: u32) -> Value {
        let r = if self.rows == 1 { 0 } else { row };
        let c = if self.cols == 1 { 0 } else { col };
        self.get(r, c)
            .cloned()
            .unwrap_or(Value::Error(CalcError::NotAvailable))
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.cells.iter()
    }

    /// The single value, when the array holds exactly one.
    pub fn as_scalar(&self) -> Option<&Value> {
        (self.cells.len() == 1).then(|| &self.cells[0])
    }

    /// The shape two arrays broadcast to: the larger of each dimension.
    pub fn broadcast_shape(a: &Self, b: &Self) -> (u32, u32) {
        (a.rows.max(b.rows), a.cols.max(b.cols))
    }
}

/// One or more rectangles on one or more sheets.
///
/// A plain `A1:B2` is one area. A union is several. A three-dimensional
/// reference such as `Sheet1:Sheet3!A1` is the same rectangle on several
/// sheets, which is also several areas.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reference {
    areas: Vec<RangeAddr>,
}

impl Reference {
    pub fn new(areas: Vec<RangeAddr>) -> Option<Self> {
        (!areas.is_empty()).then_some(Self { areas })
    }

    pub fn single(sheet: SheetId, range: RangeRef) -> Self {
        Self {
            areas: vec![RangeAddr::new(sheet, range)],
        }
    }

    pub fn cell(sheet: SheetId, cell: CellRef) -> Self {
        Self::single(sheet, RangeRef::single(cell))
    }

    pub fn areas(&self) -> &[RangeAddr] {
        &self.areas
    }

    /// The one area, when there is exactly one.
    pub fn as_single_area(&self) -> Option<RangeAddr> {
        (self.areas.len() == 1).then(|| self.areas[0])
    }

    /// The one cell, when this reference is exactly one cell.
    pub fn as_single_cell(&self) -> Option<ferrum_core::CellAddr> {
        let area = self.as_single_area()?;
        (area.range.start == area.range.end)
            .then(|| ferrum_core::CellAddr::new(area.sheet, area.range.start))
    }

    /// Join two references into a union.
    pub fn union(mut self, other: Self) -> Self {
        self.areas.extend(other.areas);
        self
    }

    /// The cells the two references share.
    ///
    /// Returns `None` when they share none, which is the `#NULL!` case.
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let mut shared = Vec::new();
        for left in &self.areas {
            for right in &other.areas {
                if left.sheet == right.sheet
                    && let Some(overlap) = left.range.intersection(&right.range)
                {
                    shared.push(RangeAddr::new(left.sheet, overlap));
                }
            }
        }
        Self::new(shared)
    }

    /// Total cells addressed, counting overlaps in a union twice, the way the
    /// language does.
    pub fn area(&self) -> u64 {
        self.areas.iter().map(|a| a.range.area()).sum()
    }
}

/// The result of evaluating an expression.
#[derive(Clone, PartialEq, Debug)]
pub enum Operand {
    Value(Value),
    Array(Array),
    Reference(Reference),
}

impl Operand {
    pub fn error(error: CalcError) -> Self {
        Self::Value(Value::Error(error))
    }

    pub fn number(n: f64) -> Self {
        Self::Value(Value::number(n))
    }

    pub fn text(s: impl Into<std::sync::Arc<str>>) -> Self {
        Self::Value(Value::Text(s.into()))
    }

    pub fn logical(b: bool) -> Self {
        Self::Value(Value::Logical(b))
    }

    /// The error this operand carries, if it is a scalar error.
    ///
    /// An array containing an error is not itself an error: the error travels
    /// in whichever element holds it.
    pub const fn as_error(&self) -> Option<CalcError> {
        match self {
            Self::Value(Value::Error(e)) => Some(*e),
            _ => None,
        }
    }

    pub const fn is_error(&self) -> bool {
        self.as_error().is_some()
    }
}

impl From<Value> for Operand {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<Array> for Operand {
    fn from(array: Array) -> Self {
        Self::Array(array)
    }
}

impl From<Reference> for Operand {
    fn from(reference: Reference) -> Self {
        Self::Reference(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: SheetId = SheetId(0);

    fn range(a: &str, b: &str) -> RangeRef {
        RangeRef::new(
            ferrum_core::A1Ref::parse(a).unwrap().cell,
            ferrum_core::A1Ref::parse(b).unwrap().cell,
        )
    }

    #[test]
    fn an_array_needs_a_matching_length() {
        assert!(Array::new(2, 2, vec![Value::Blank; 4]).is_some());
        assert!(Array::new(2, 2, vec![Value::Blank; 3]).is_none());
        assert!(Array::new(0, 2, vec![]).is_none());
    }

    #[test]
    fn arrays_read_row_major() {
        let array = Array::from_rows(vec![
            vec![Value::Number(1.0), Value::Number(2.0)],
            vec![Value::Number(3.0), Value::Number(4.0)],
        ])
        .unwrap();
        assert_eq!(array.get(0, 0), Some(&Value::Number(1.0)));
        assert_eq!(array.get(0, 1), Some(&Value::Number(2.0)));
        assert_eq!(array.get(1, 0), Some(&Value::Number(3.0)));
        assert_eq!(array.get(1, 1), Some(&Value::Number(4.0)));
        assert_eq!(array.get(2, 0), None);
    }

    #[test]
    fn a_ragged_array_is_refused() {
        assert!(
            Array::from_rows(vec![
                vec![Value::Number(1.0), Value::Number(2.0)],
                vec![Value::Number(3.0)],
            ])
            .is_none()
        );
    }

    #[test]
    fn a_single_row_stretches_down_and_a_single_column_across() {
        let row = Array::from_rows(vec![vec![Value::Number(1.0), Value::Number(2.0)]]).unwrap();
        assert_eq!(row.get_broadcast(5, 1), Value::Number(2.0));

        let column = Array::column(vec![Value::Number(7.0), Value::Number(8.0)]).unwrap();
        assert_eq!(column.get_broadcast(1, 9), Value::Number(8.0));
    }

    #[test]
    fn a_corner_neither_array_covers_is_not_available() {
        // A 2x1 and a 1x2 broadcast to 2x2, and the 2x1 has no column 1.
        let tall = Array::column(vec![Value::Number(1.0), Value::Number(2.0)]).unwrap();
        let wide = Array::from_rows(vec![vec![Value::Number(3.0), Value::Number(4.0)]]).unwrap();
        assert_eq!(Array::broadcast_shape(&tall, &wide), (2, 2));
        // The tall one is one column, so it stretches; both are covered here.
        assert_eq!(tall.get_broadcast(1, 1), Value::Number(2.0));

        // A genuinely short array reports #N/A past its end.
        let short = Array::from_rows(vec![
            vec![Value::Number(1.0), Value::Number(2.0)],
            vec![Value::Number(3.0), Value::Number(4.0)],
        ])
        .unwrap();
        assert_eq!(
            short.get_broadcast(5, 0),
            Value::Error(CalcError::NotAvailable)
        );
    }

    #[test]
    fn a_union_keeps_its_areas_apart() {
        let left = Reference::single(SHEET, range("A1", "A2"));
        let right = Reference::single(SHEET, range("C1", "C2"));
        let joined = left.union(right);
        assert_eq!(joined.areas().len(), 2);
        // Four cells, not the six a bounding box would sweep up.
        assert_eq!(joined.area(), 4);
    }

    #[test]
    fn intersection_finds_the_shared_cells() {
        let across = Reference::single(SHEET, range("A3", "E3"));
        let down = Reference::single(SHEET, range("C1", "C9"));
        let shared = across.intersect(&down).unwrap();
        assert_eq!(shared.as_single_cell().unwrap().cell.to_a1(), "C3");
    }

    #[test]
    fn disjoint_references_share_nothing() {
        let left = Reference::single(SHEET, range("A1", "A2"));
        let right = Reference::single(SHEET, range("C1", "C2"));
        assert!(left.intersect(&right).is_none());
    }

    #[test]
    fn references_on_different_sheets_never_intersect() {
        let here = Reference::single(SheetId(0), range("A1", "Z99"));
        let there = Reference::single(SheetId(1), range("A1", "Z99"));
        assert!(here.intersect(&there).is_none());
    }

    #[test]
    fn an_array_holding_an_error_is_not_itself_an_error() {
        let array = Array::column(vec![Value::Number(1.0), Value::Error(CalcError::Div0)]).unwrap();
        assert!(!Operand::Array(array).is_error());
        assert!(Operand::error(CalcError::Div0).is_error());
    }
}
