//! Which cells depend on which.
//!
//! Editing a cell has to find every formula that reads it, and has to do so
//! without asking every formula in the workbook. Single-cell reads get a plain
//! reverse index. Range reads are the interesting case: a formula reading
//! `A1:A100` must be found by an edit to any of those hundred cells, and
//! registering it under each one would be wasteful and wrong for a range that
//! covers a whole column.
//!
//! So ranges are bucketed into tiles of the grid, and an edit checks only the
//! tile it falls in. A range too wide to bucket sensibly, which in practice
//! means a whole column or row, goes on a short per-sheet list that is scanned
//! instead. One threshold, two paths, no special cases beyond that.

use std::collections::{HashMap, HashSet};

use ferrum_calc::{Expr, RefTarget, SheetRef};
use ferrum_core::{CellAddr, RangeAddr, RangeRef, SheetId};

/// Side of a tile, in cells.
const TILE: u32 = 256;

/// A range covering more tiles than this is watched by the per-sheet list
/// instead of being registered in each one.
const MAX_TILES_PER_RANGE: u64 = 64;

/// One region of the grid that range watchers are bucketed into.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Tile {
    sheet: SheetId,
    row: u32,
    col: u32,
}

fn tile_of(addr: CellAddr) -> Tile {
    Tile {
        sheet: addr.sheet,
        row: addr.cell.row / TILE,
        col: addr.cell.col / TILE,
    }
}

/// What a formula reads.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dependency {
    Cell(CellAddr),
    Range(RangeAddr),
}

#[derive(Default)]
pub struct DependencyGraph {
    /// What each formula cell reads, kept so its edges can be withdrawn when
    /// the formula changes.
    precedents: HashMap<CellAddr, Vec<Dependency>>,
    /// Formulas that read one particular cell.
    cell_watchers: HashMap<CellAddr, HashSet<CellAddr>>,
    /// Formulas that read a range, bucketed by the tiles it covers.
    tile_watchers: HashMap<Tile, Vec<(RangeAddr, CellAddr)>>,
    /// Formulas reading a range too wide to bucket.
    wide_watchers: HashMap<SheetId, Vec<(RangeAddr, CellAddr)>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what `formula` reads, replacing whatever it read before.
    pub fn set_precedents(&mut self, formula: CellAddr, dependencies: Vec<Dependency>) {
        self.clear(formula);
        if dependencies.is_empty() {
            return;
        }

        for dependency in &dependencies {
            match *dependency {
                Dependency::Cell(target) => {
                    self.cell_watchers
                        .entry(target)
                        .or_default()
                        .insert(formula);
                }
                Dependency::Range(area) => {
                    if let Some(tiles) = tiles_covering(area) {
                        for tile in tiles {
                            self.tile_watchers
                                .entry(tile)
                                .or_default()
                                .push((area, formula));
                        }
                    } else {
                        self.wide_watchers
                            .entry(area.sheet)
                            .or_default()
                            .push((area, formula));
                    }
                }
            }
        }

        self.precedents.insert(formula, dependencies);
    }

    /// Forget everything `formula` read.
    pub fn clear(&mut self, formula: CellAddr) {
        let Some(previous) = self.precedents.remove(&formula) else {
            return;
        };

        for dependency in previous {
            match dependency {
                Dependency::Cell(target) => {
                    if let Some(watchers) = self.cell_watchers.get_mut(&target) {
                        watchers.remove(&formula);
                        if watchers.is_empty() {
                            self.cell_watchers.remove(&target);
                        }
                    }
                }
                Dependency::Range(area) => {
                    if let Some(tiles) = tiles_covering(area) {
                        for tile in tiles {
                            if let Some(bucket) = self.tile_watchers.get_mut(&tile) {
                                bucket.retain(|(r, f)| !(*r == area && *f == formula));
                                if bucket.is_empty() {
                                    self.tile_watchers.remove(&tile);
                                }
                            }
                        }
                    } else if let Some(bucket) = self.wide_watchers.get_mut(&area.sheet) {
                        bucket.retain(|(r, f)| !(*r == area && *f == formula));
                        if bucket.is_empty() {
                            self.wide_watchers.remove(&area.sheet);
                        }
                    }
                }
            }
        }
    }

    /// What `formula` reads.
    pub fn precedents_of(&self, formula: CellAddr) -> &[Dependency] {
        self.precedents.get(&formula).map_or(&[], Vec::as_slice)
    }

    /// Every formula that reads `addr`, directly or through a range.
    pub fn dependents_of(&self, addr: CellAddr) -> Vec<CellAddr> {
        let mut out: Vec<CellAddr> = Vec::new();

        if let Some(direct) = self.cell_watchers.get(&addr) {
            out.extend(direct.iter().copied());
        }

        if let Some(bucket) = self.tile_watchers.get(&tile_of(addr)) {
            out.extend(
                bucket
                    .iter()
                    .filter(|(area, _)| area.contains(addr))
                    .map(|(_, formula)| *formula),
            );
        }

        if let Some(bucket) = self.wide_watchers.get(&addr.sheet) {
            out.extend(
                bucket
                    .iter()
                    .filter(|(area, _)| area.contains(addr))
                    .map(|(_, formula)| *formula),
            );
        }

        out.sort_unstable();
        out.dedup();
        out
    }

    /// How many formulas the graph is tracking.
    pub fn tracked(&self) -> usize {
        self.precedents.len()
    }
}

/// The tiles a range covers, or `None` when it covers too many to be worth
/// registering individually.
fn tiles_covering(area: RangeAddr) -> Option<Vec<Tile>> {
    let range = area.range;
    let first_row = range.start.row / TILE;
    let last_row = range.end.row / TILE;
    let first_col = range.start.col / TILE;
    let last_col = range.end.col / TILE;

    let count = u64::from(last_row - first_row + 1) * u64::from(last_col - first_col + 1);
    if count > MAX_TILES_PER_RANGE {
        return None;
    }

    let mut tiles = Vec::with_capacity(count as usize);
    for row in first_row..=last_row {
        for col in first_col..=last_col {
            tiles.push(Tile {
                sheet: area.sheet,
                row,
                col,
            });
        }
    }
    Some(tiles)
}

/// Walk a formula and list what it reads.
///
/// `resolve_sheet` turns a written sheet name into an id. A name that does not
/// resolve contributes no dependency, because there is nothing to depend on;
/// the formula will report `#REF!` when it is evaluated.
pub fn dependencies_of(
    expr: &Expr,
    current: SheetId,
    resolve_sheet: &dyn Fn(&str) -> Option<SheetId>,
    span: &dyn Fn(&str, &str) -> Option<Vec<SheetId>>,
) -> Vec<Dependency> {
    let mut out = Vec::new();
    walk(expr, current, resolve_sheet, span, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

fn walk(
    expr: &Expr,
    current: SheetId,
    resolve_sheet: &dyn Fn(&str) -> Option<SheetId>,
    span: &dyn Fn(&str, &str) -> Option<Vec<SheetId>>,
    out: &mut Vec<Dependency>,
) {
    match expr {
        Expr::Literal(_) | Expr::Name { .. } => {}

        Expr::Ref { sheet, target } => {
            let sheets: Vec<SheetId> = match sheet {
                None => vec![current],
                Some(SheetRef::One(name)) => resolve_sheet(name).into_iter().collect(),
                Some(SheetRef::Span { first, last }) => span(first, last).unwrap_or_default(),
            };
            let Some(range) = range_of(target) else {
                return;
            };
            for id in sheets {
                out.push(if range.start == range.end {
                    Dependency::Cell(CellAddr::new(id, range.start))
                } else {
                    Dependency::Range(RangeAddr::new(id, range))
                });
            }
        }

        Expr::Unary { operand, .. } | Expr::Percent(operand) => {
            walk(operand, current, resolve_sheet, span, out);
        }

        Expr::Binary { left, right, .. }
        | Expr::RangeOp { left, right }
        | Expr::Intersect { left, right } => {
            walk(left, current, resolve_sheet, span, out);
            walk(right, current, resolve_sheet, span, out);
        }

        Expr::Call { args, .. } | Expr::Union(args) => {
            for arg in args {
                walk(arg, current, resolve_sheet, span, out);
            }
        }

        Expr::Array(rows) => {
            for row in rows {
                for cell in row {
                    walk(cell, current, resolve_sheet, span, out);
                }
            }
        }
    }
}

fn range_of(target: &RefTarget) -> Option<RangeRef> {
    Some(match target {
        RefTarget::Cell(a1) => RangeRef::single(a1.cell),
        RefTarget::Range { start, end } => RangeRef::new(start.cell, end.cell),
        RefTarget::Columns { first, last } => RangeRef::whole_columns(first.index, last.index),
        RefTarget::Rows { first, last } => RangeRef::whole_rows(first.index, last.index),
        // A broken reference reads nothing.
        RefTarget::Invalid => return None,
    })
}

/// Ordering so dependencies can be sorted and deduplicated.
impl PartialOrd for Dependency {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Dependency {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn key(d: &Dependency) -> (u32, u32, u32, u32, u32) {
            match d {
                Dependency::Cell(a) => (a.sheet.0, a.cell.row, a.cell.col, a.cell.row, a.cell.col),
                Dependency::Range(a) => (
                    a.sheet.0,
                    a.range.start.row,
                    a.range.start.col,
                    a.range.end.row,
                    a.range.end.col,
                ),
            }
        }
        key(self).cmp(&key(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHEET: SheetId = SheetId(0);

    fn at(a1: &str) -> CellAddr {
        CellAddr::new(SHEET, ferrum_core::A1Ref::parse(a1).unwrap().cell)
    }

    fn range(a: &str, b: &str) -> RangeAddr {
        RangeAddr::new(
            SHEET,
            RangeRef::new(
                ferrum_core::A1Ref::parse(a).unwrap().cell,
                ferrum_core::A1Ref::parse(b).unwrap().cell,
            ),
        )
    }

    fn deps(formula: &str) -> Vec<Dependency> {
        let expr = ferrum_calc::parse(formula).unwrap();
        dependencies_of(
            &expr,
            SHEET,
            &|name| match name {
                "Sheet1" => Some(SheetId(0)),
                "Data" => Some(SheetId(1)),
                _ => None,
            },
            &|_, _| Some(vec![SheetId(0), SheetId(1)]),
        )
    }

    #[test]
    fn a_single_reference_is_a_cell_dependency() {
        assert_eq!(deps("=A1+1"), [Dependency::Cell(at("A1"))]);
    }

    #[test]
    fn a_range_reference_is_a_range_dependency() {
        assert_eq!(deps("=SUM(A1:B2)"), [Dependency::Range(range("A1", "B2"))]);
    }

    #[test]
    fn dependencies_are_deduplicated() {
        assert_eq!(deps("=A1+A1+A1"), [Dependency::Cell(at("A1"))]);
    }

    #[test]
    fn a_sheet_span_depends_on_every_sheet_in_it() {
        let found = deps("=SUM(Sheet1:Data!A1)");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn an_unresolvable_sheet_contributes_nothing() {
        assert!(deps("=Nowhere!A1").is_empty());
    }

    #[test]
    fn a_broken_reference_contributes_nothing() {
        assert!(deps("=#REF!+1").is_empty());
    }

    #[test]
    fn nested_expressions_are_walked() {
        let found = deps("=IF(A1>0,SUM(B1:B3),{1,2})");
        assert_eq!(
            found,
            [
                Dependency::Cell(at("A1")),
                Dependency::Range(range("B1", "B3"))
            ]
        );
    }

    #[test]
    fn an_edit_finds_the_formula_that_reads_it_directly() {
        let mut graph = DependencyGraph::new();
        graph.set_precedents(at("C1"), vec![Dependency::Cell(at("A1"))]);
        assert_eq!(graph.dependents_of(at("A1")), [at("C1")]);
        assert!(graph.dependents_of(at("A2")).is_empty());
    }

    #[test]
    fn an_edit_inside_a_range_finds_the_formula_reading_it() {
        let mut graph = DependencyGraph::new();
        graph.set_precedents(at("C1"), vec![Dependency::Range(range("A1", "A100"))]);
        assert_eq!(graph.dependents_of(at("A50")), [at("C1")]);
        assert!(graph.dependents_of(at("B50")).is_empty());
        assert!(graph.dependents_of(at("A101")).is_empty());
    }

    #[test]
    fn a_whole_column_watcher_is_found_without_being_bucketed() {
        let wide = RangeAddr::new(SHEET, RangeRef::whole_columns(0, 0));
        // Too many tiles to register one by one.
        assert!(tiles_covering(wide).is_none());

        let mut graph = DependencyGraph::new();
        graph.set_precedents(at("C1"), vec![Dependency::Range(wide)]);
        assert_eq!(graph.dependents_of(at("A500000")), [at("C1")]);
        assert!(graph.dependents_of(at("B1")).is_empty());
    }

    #[test]
    fn replacing_a_formula_withdraws_its_old_edges() {
        let mut graph = DependencyGraph::new();
        graph.set_precedents(at("C1"), vec![Dependency::Cell(at("A1"))]);
        graph.set_precedents(at("C1"), vec![Dependency::Cell(at("B1"))]);
        assert!(graph.dependents_of(at("A1")).is_empty());
        assert_eq!(graph.dependents_of(at("B1")), [at("C1")]);
    }

    #[test]
    fn clearing_a_formula_removes_every_trace_of_it() {
        let mut graph = DependencyGraph::new();
        graph.set_precedents(
            at("C1"),
            vec![
                Dependency::Cell(at("A1")),
                Dependency::Range(range("B1", "B9")),
                Dependency::Range(RangeAddr::new(SHEET, RangeRef::whole_columns(3, 3))),
            ],
        );
        graph.clear(at("C1"));
        assert!(graph.dependents_of(at("A1")).is_empty());
        assert!(graph.dependents_of(at("B5")).is_empty());
        assert!(graph.dependents_of(at("D7")).is_empty());
        assert_eq!(graph.tracked(), 0);
        // And the internal buckets did not leak.
        assert!(graph.tile_watchers.is_empty());
        assert!(graph.wide_watchers.is_empty());
        assert!(graph.cell_watchers.is_empty());
    }

    #[test]
    fn several_formulas_reading_one_cell_are_all_found() {
        let mut graph = DependencyGraph::new();
        graph.set_precedents(at("C1"), vec![Dependency::Cell(at("A1"))]);
        graph.set_precedents(at("C2"), vec![Dependency::Range(range("A1", "A9"))]);
        let mut found = graph.dependents_of(at("A1"));
        found.sort_unstable();
        assert_eq!(found, [at("C1"), at("C2")]);
    }
}
