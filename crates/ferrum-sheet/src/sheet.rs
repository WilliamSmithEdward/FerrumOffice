//! Sparse storage for one sheet.
//!
//! A sheet addresses 17 billion cells and holds a few thousand, so storage is
//! a map rather than a grid. The bounding box of what is populated is kept
//! alongside, because the calculation engine uses it to avoid walking a whole
//! column to add up four numbers.

use std::collections::HashMap;

use ferrum_core::defaults::{COLUMN_WIDTH_PT, ROW_HEIGHT_PT};
use ferrum_core::{CellRef, MAX_COL, MAX_ROW, RangeRef, Value};

use crate::axis::Axis;
use crate::cell::Cell;

pub struct Sheet {
    name: String,
    cells: HashMap<CellRef, Cell>,
    /// Bounding box of the populated cells.
    ///
    /// May be larger than the true extent after a deletion that did not touch
    /// an edge, which is harmless: the contract is that it never excludes a
    /// populated cell.
    used: Option<RangeRef>,
    /// Column widths and row heights, in points.
    columns: Axis,
    rows: Axis,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cells: HashMap::new(),
            used: None,
            columns: Axis::new(COLUMN_WIDTH_PT, MAX_COL),
            rows: Axis::new(ROW_HEIGHT_PT, MAX_ROW),
        }
    }

    /// Column widths, in points.
    pub fn columns(&self) -> &Axis {
        &self.columns
    }

    pub fn columns_mut(&mut self) -> &mut Axis {
        &mut self.columns
    }

    /// Row heights, in points.
    pub fn rows(&self) -> &Axis {
        &self.rows
    }

    pub fn rows_mut(&mut self) -> &mut Axis {
        &mut self.rows
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn get(&self, cell: CellRef) -> Option<&Cell> {
        self.cells.get(&cell)
    }

    pub fn get_mut(&mut self, cell: CellRef) -> Option<&mut Cell> {
        self.cells.get_mut(&cell)
    }

    /// The value in a cell. An empty cell reads as blank.
    pub fn value(&self, cell: CellRef) -> Value {
        self.cells
            .get(&cell)
            .map_or(Value::Blank, |c| c.value.clone())
    }

    pub fn insert(&mut self, cell: CellRef, content: Cell) {
        self.grow_to(cell);
        self.cells.insert(cell, content);
    }

    /// Remove a cell, narrowing the bounding box only when an edge went with it.
    pub fn remove(&mut self, cell: CellRef) -> Option<Cell> {
        let removed = self.cells.remove(&cell)?;
        if let Some(used) = self.used
            && (cell.row == used.start.row
                || cell.row == used.end.row
                || cell.col == used.start.col
                || cell.col == used.end.col)
        {
            // Only a cell on the boundary can shrink the box, so the scan is
            // rare rather than a cost on every clear.
            self.recompute_used();
        }
        Some(removed)
    }

    pub fn used_bounds(&self) -> Option<RangeRef> {
        self.used
    }

    pub fn populated(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Every populated cell, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (CellRef, &Cell)> {
        self.cells.iter().map(|(at, cell)| (*at, cell))
    }

    /// Every populated cell whose position lies inside `range`.
    ///
    /// Picks whichever way round is cheaper: walking the range when it is
    /// small, or filtering the populated cells when the range is large. A
    /// whole-column range takes the second path and costs the data rather than
    /// the address space.
    pub fn cells_in(&self, range: RangeRef) -> Vec<(CellRef, &Cell)> {
        let populated = self.cells.len() as u64;
        if range.area() <= populated.saturating_mul(4) {
            range
                .cells()
                .filter_map(|at| self.cells.get(&at).map(|cell| (at, cell)))
                .collect()
        } else {
            self.cells
                .iter()
                .filter(|(at, _)| range.contains(**at))
                .map(|(at, cell)| (*at, cell))
                .collect()
        }
    }

    fn grow_to(&mut self, cell: CellRef) {
        let one = RangeRef::single(cell);
        self.used = Some(match self.used {
            None => one,
            Some(used) => used.union_bounds(&one),
        });
    }

    fn recompute_used(&mut self) {
        let mut bounds: Option<RangeRef> = None;
        for at in self.cells.keys() {
            let one = RangeRef::single(*at);
            bounds = Some(match bounds {
                None => one,
                Some(acc) => acc.union_bounds(&one),
            });
        }
        self.used = bounds;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Input, parse_input};

    fn literal(text: &str) -> Cell {
        let input = parse_input(text);
        let value = match &input {
            Input::Literal(v) => v.clone(),
            _ => Value::Blank,
        };
        Cell { input, value }
    }

    fn at(a1: &str) -> CellRef {
        ferrum_core::A1Ref::parse(a1).unwrap().cell
    }

    #[test]
    fn an_empty_sheet_has_no_bounds() {
        let sheet = Sheet::new("Sheet1");
        assert_eq!(sheet.used_bounds(), None);
        assert_eq!(sheet.value(at("A1")), Value::Blank);
        assert!(sheet.is_empty());
    }

    #[test]
    fn the_bounding_box_grows_to_cover_what_is_written() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.insert(at("C3"), literal("1"));
        assert_eq!(sheet.used_bounds().unwrap().to_a1(), "C3");
        sheet.insert(at("A5"), literal("2"));
        assert_eq!(sheet.used_bounds().unwrap().to_a1(), "A3:C5");
    }

    #[test]
    fn removing_an_interior_cell_leaves_the_box_alone() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.insert(at("A1"), literal("1"));
        sheet.insert(at("B2"), literal("2"));
        sheet.insert(at("C3"), literal("3"));
        sheet.remove(at("B2"));
        // B2 was not on an edge, so no rescan happened and the box stands.
        assert_eq!(sheet.used_bounds().unwrap().to_a1(), "A1:C3");
    }

    #[test]
    fn removing_an_edge_cell_shrinks_the_box() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.insert(at("A1"), literal("1"));
        sheet.insert(at("C3"), literal("3"));
        sheet.remove(at("C3"));
        assert_eq!(sheet.used_bounds().unwrap().to_a1(), "A1");
    }

    #[test]
    fn emptying_a_sheet_clears_its_bounds() {
        let mut sheet = Sheet::new("Sheet1");
        sheet.insert(at("A1"), literal("1"));
        sheet.remove(at("A1"));
        assert_eq!(sheet.used_bounds(), None);
    }

    #[test]
    fn range_iteration_finds_the_same_cells_either_way_round() {
        let mut sheet = Sheet::new("Sheet1");
        for row in 0..10u32 {
            sheet.insert(CellRef::new(row, 0), literal("1"));
        }

        // A small range walks the range; a whole column filters the map. Both
        // must agree.
        let small = sheet.cells_in(RangeRef::new(at("A1"), at("A5")));
        assert_eq!(small.len(), 5);

        let whole_column = sheet.cells_in(RangeRef::whole_columns(0, 0));
        assert_eq!(whole_column.len(), 10);

        let elsewhere = sheet.cells_in(RangeRef::whole_columns(5, 5));
        assert!(elsewhere.is_empty());
    }
}
