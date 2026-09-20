//! The workbook: sheets, defined names, and recalculation.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ferrum_calc::eval::{Ctx, Resolver};
use ferrum_calc::operand::Operand;
use ferrum_calc::{Expr, Reference};
use ferrum_core::{CellAddr, CellRef, RangeRef, SheetId, Value};

use crate::cell::{Cell, Input, parse_input};
use crate::graph::{DependencyGraph, dependencies_of};
use crate::sheet::Sheet;

/// What a recalculation did.
#[derive(Clone, Debug, Default)]
pub struct RecalcReport {
    /// Formulas evaluated.
    pub evaluated: usize,
    /// Cells whose value actually changed, which is what a view needs to
    /// repaint. A formula that recalculated to the same answer is not here.
    pub changed: Vec<CellAddr>,
    /// Cells that take part in a circular reference.
    ///
    /// They are left showing zero, which is what a spreadsheet does, and the
    /// caller is expected to warn.
    pub circular: Vec<CellAddr>,
}

impl RecalcReport {
    pub fn is_circular(&self) -> bool {
        !self.circular.is_empty()
    }

    /// Record that a cell needs repainting, keeping the list tidy.
    fn note_changed(&mut self, addr: CellAddr) {
        if let Err(at) = self.changed.binary_search(&addr) {
            self.changed.insert(at, addr);
        }
    }
}

/// Why a sheet operation failed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SheetError {
    NameTaken,
    NameEmpty,
    NoSuchSheet,
    /// A workbook always has at least one sheet.
    LastSheet,
}

impl std::fmt::Display for SheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NameTaken => "a sheet with that name already exists",
            Self::NameEmpty => "a sheet needs a name",
            Self::NoSuchSheet => "no such sheet",
            Self::LastSheet => "a workbook needs at least one sheet",
        })
    }
}

impl std::error::Error for SheetError {}

pub struct Workbook {
    /// Indexed by sheet id. A removed sheet leaves a hole rather than
    /// renumbering the rest, so an id never comes to mean a different sheet.
    sheets: Vec<Option<Sheet>>,
    /// Tab order, which is what the user sees and what a sheet span follows.
    order: Vec<SheetId>,
    names: HashMap<String, Operand>,
    graph: DependencyGraph,
}

impl Default for Workbook {
    fn default() -> Self {
        Self::new()
    }
}

impl Workbook {
    /// A new workbook with one empty sheet.
    pub fn new() -> Self {
        let mut book = Self {
            sheets: Vec::new(),
            order: Vec::new(),
            names: HashMap::new(),
            graph: DependencyGraph::new(),
        };
        book.push_sheet(Sheet::new("Sheet1"));
        book
    }

    fn push_sheet(&mut self, sheet: Sheet) -> SheetId {
        let id = SheetId(self.sheets.len() as u32);
        self.sheets.push(Some(sheet));
        self.order.push(id);
        id
    }

    pub fn add_sheet(&mut self, name: &str) -> Result<SheetId, SheetError> {
        if name.trim().is_empty() {
            return Err(SheetError::NameEmpty);
        }
        if self.sheet_id(name).is_some() {
            return Err(SheetError::NameTaken);
        }
        Ok(self.push_sheet(Sheet::new(name)))
    }

    pub fn rename_sheet(&mut self, id: SheetId, name: &str) -> Result<(), SheetError> {
        if name.trim().is_empty() {
            return Err(SheetError::NameEmpty);
        }
        match self.sheet_id(name) {
            Some(existing) if existing != id => return Err(SheetError::NameTaken),
            _ => {}
        }
        self.sheet_mut(id)
            .ok_or(SheetError::NoSuchSheet)?
            .rename(name);
        Ok(())
    }

    /// Remove a sheet and recalculate whatever was reading it.
    ///
    /// References to the sheet are not rewritten. They stop resolving, so they
    /// report `#REF!`, which is what the author needs to see.
    pub fn remove_sheet(&mut self, id: SheetId) -> Result<RecalcReport, SheetError> {
        if self.order.len() <= 1 {
            return Err(SheetError::LastSheet);
        }
        if self.sheet(id).is_none() {
            return Err(SheetError::NoSuchSheet);
        }

        // Everything that read a cell on this sheet has to be told.
        let readers: Vec<CellAddr> = self
            .sheet(id)
            .map(|sheet| {
                sheet
                    .iter()
                    .flat_map(|(at, _)| self.graph.dependents_of(CellAddr::new(id, at)))
                    .collect()
            })
            .unwrap_or_default();

        // Drop the formulas that lived here from the graph.
        let living: Vec<CellAddr> = self
            .sheet(id)
            .map(|sheet| {
                sheet
                    .iter()
                    .filter(|(_, cell)| cell.is_formula())
                    .map(|(at, _)| CellAddr::new(id, at))
                    .collect()
            })
            .unwrap_or_default();
        for addr in living {
            self.graph.clear(addr);
        }

        self.sheets[id.0 as usize] = None;
        self.order.retain(|held| *held != id);

        Ok(self.recalculate_from(readers))
    }

    pub fn sheet(&self, id: SheetId) -> Option<&Sheet> {
        self.sheets.get(id.0 as usize)?.as_ref()
    }

    pub fn sheet_mut(&mut self, id: SheetId) -> Option<&mut Sheet> {
        self.sheets.get_mut(id.0 as usize)?.as_mut()
    }

    pub fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.order.iter().copied().find(|id| {
            self.sheet(*id)
                .is_some_and(|s| s.name().eq_ignore_ascii_case(name))
        })
    }

    /// Sheet ids in tab order.
    pub fn sheet_order(&self) -> &[SheetId] {
        &self.order
    }

    pub fn first_sheet(&self) -> SheetId {
        self.order[0]
    }

    /// Define a name for the whole workbook.
    pub fn define_name(&mut self, name: &str, target: Operand) {
        self.names.insert(name.to_ascii_uppercase(), target);
    }

    /// Define a name standing for a range.
    pub fn define_range(&mut self, name: &str, sheet: SheetId, range: RangeRef) {
        self.define_name(name, Operand::Reference(Reference::single(sheet, range)));
    }

    pub fn value(&self, addr: CellAddr) -> Value {
        self.sheet(addr.sheet)
            .map_or(Value::Blank, |sheet| sheet.value(addr.cell))
    }

    pub fn cell(&self, addr: CellAddr) -> Option<&Cell> {
        self.sheet(addr.sheet)?.get(addr.cell)
    }

    /// What an editor should show for a cell.
    pub fn edit_text(&self, addr: CellAddr) -> String {
        self.cell(addr).map(Cell::edit_text).unwrap_or_default()
    }

    /// Write what the author typed, then recalculate what depended on it.
    pub fn set_input(&mut self, addr: CellAddr, typed: &str) -> RecalcReport {
        if typed.is_empty() {
            return self.clear(addr);
        }
        if self.sheet(addr.sheet).is_none() {
            return RecalcReport::default();
        }

        let previous = self.value(addr);
        let input = parse_input(typed);

        // Register what the new formula reads before anything is evaluated,
        // so the ordering pass sees the new shape of the graph.
        match &input {
            Input::Formula { expr, .. } => {
                let dependencies = self.dependencies_for(expr, addr.sheet);
                self.graph.set_precedents(addr, dependencies);
            }
            Input::Literal(_) | Input::Malformed { .. } => self.graph.clear(addr),
        }

        let seed_value = match &input {
            Input::Literal(value) => value.clone(),
            // Until it is evaluated. A malformed formula stays this way.
            Input::Formula { .. } => Value::Blank,
            Input::Malformed { .. } => Value::Error(ferrum_core::CalcError::Name),
        };

        if let Some(sheet) = self.sheet_mut(addr.sheet) {
            sheet.insert(
                addr.cell,
                Cell {
                    input,
                    value: seed_value,
                },
            );
        }

        let mut report = self.recalculate_from(vec![addr]);
        if self.value(addr) != previous {
            report.note_changed(addr);
        }
        report
    }

    /// Empty a cell and recalculate what read it.
    pub fn clear(&mut self, addr: CellAddr) -> RecalcReport {
        let previous = self.value(addr);
        self.graph.clear(addr);
        if let Some(sheet) = self.sheet_mut(addr.sheet) {
            sheet.remove(addr.cell);
        }
        let mut report = self.recalculate_from(vec![addr]);
        if previous != Value::Blank {
            report.note_changed(addr);
        }
        report
    }

    /// Recalculate every formula in the workbook.
    pub fn recalculate_all(&mut self) -> RecalcReport {
        let everything: Vec<CellAddr> = self
            .order
            .iter()
            .filter_map(|id| self.sheet(*id).map(|sheet| (*id, sheet)))
            .flat_map(|(id, sheet)| {
                sheet
                    .iter()
                    .map(move |(at, _)| CellAddr::new(id, at))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.recalculate_from(everything)
    }

    fn dependencies_for(&self, expr: &Expr, current: SheetId) -> Vec<crate::graph::Dependency> {
        dependencies_of(
            expr,
            current,
            &|name| self.sheet_id(name),
            &|first, last| self.sheets_between_named(first, last),
        )
    }

    fn sheets_between_named(&self, first: &str, last: &str) -> Option<Vec<SheetId>> {
        let a = self.order.iter().position(|id| {
            self.sheet(*id)
                .is_some_and(|s| s.name().eq_ignore_ascii_case(first))
        })?;
        let b = self.order.iter().position(|id| {
            self.sheet(*id)
                .is_some_and(|s| s.name().eq_ignore_ascii_case(last))
        })?;
        let (lo, hi) = (a.min(b), a.max(b));
        Some(self.order[lo..=hi].to_vec())
    }

    /// The heart of it: work out what changed, in what order, and evaluate.
    fn recalculate_from(&mut self, seeds: Vec<CellAddr>) -> RecalcReport {
        let affected = self.affected_by(&seeds);
        if affected.is_empty() {
            return RecalcReport::default();
        }

        let (order, circular) = self.evaluation_order(&affected);

        let mut report = RecalcReport {
            circular,
            ..Default::default()
        };

        for addr in order {
            // Only a formula needs evaluating. A literal in the set is there
            // because something reads it, not because it recomputes.
            let Some(expr) = self.formula_at(addr) else {
                continue;
            };
            report.evaluated += 1;

            let computed = {
                let view = View {
                    sheets: &self.sheets,
                    order: &self.order,
                    names: &self.names,
                    current: addr.sheet,
                };
                Ctx::new(&view, addr).eval_to_value(&expr)
            };

            if self.write_value(addr, computed) {
                report.note_changed(addr);
            }
        }

        // A cycle has no answer, so the cells in it show zero and the caller
        // is told which they were.
        for addr in report.circular.clone() {
            if self.write_value(addr, Value::Number(0.0)) {
                report.note_changed(addr);
            }
        }

        report
    }

    /// The formula in a cell, if it holds one.
    fn formula_at(&self, addr: CellAddr) -> Option<Rc<Expr>> {
        match &self.cell(addr)?.input {
            Input::Formula { expr, .. } => Some(Rc::clone(expr)),
            _ => None,
        }
    }

    /// Store a value, reporting whether it differed from what was there.
    fn write_value(&mut self, addr: CellAddr, value: Value) -> bool {
        let Some(sheet) = self.sheet_mut(addr.sheet) else {
            return false;
        };
        match sheet.get_mut(addr.cell) {
            Some(cell) => {
                if cell.value == value {
                    return false;
                }
                cell.value = value;
                true
            }
            None => false,
        }
    }

    /// Every cell reached from the seeds by following who-reads-whom.
    fn affected_by(&self, seeds: &[CellAddr]) -> HashSet<CellAddr> {
        let mut seen: HashSet<CellAddr> = HashSet::new();
        let mut queue: Vec<CellAddr> = seeds.to_vec();

        while let Some(addr) = queue.pop() {
            if !seen.insert(addr) {
                continue;
            }
            for dependent in self.graph.dependents_of(addr) {
                if !seen.contains(&dependent) {
                    queue.push(dependent);
                }
            }
        }

        seen
    }

    /// Order the affected cells so that everything is evaluated after what it
    /// reads, and report any that cannot be ordered because they form a cycle.
    fn evaluation_order(&self, affected: &HashSet<CellAddr>) -> (Vec<CellAddr>, Vec<CellAddr>) {
        // Edges point from a cell to the formulas that read it, so a
        // topological order over them puts each formula after its inputs.
        let mut successors: HashMap<CellAddr, Vec<CellAddr>> = HashMap::new();
        for addr in affected {
            let mut next: Vec<CellAddr> = self
                .graph
                .dependents_of(*addr)
                .into_iter()
                .filter(|d| affected.contains(d))
                .collect();
            next.sort_unstable();
            successors.insert(*addr, next);
        }

        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Open,
            Done,
        }

        let mut marks: HashMap<CellAddr, Mark> = HashMap::new();
        let mut finished: Vec<CellAddr> = Vec::with_capacity(affected.len());
        let mut circular: HashSet<CellAddr> = HashSet::new();

        // Sorted roots keep the result stable run to run, which matters for
        // tests and for anyone reading a diff of the output.
        let mut roots: Vec<CellAddr> = affected.iter().copied().collect();
        roots.sort_unstable();

        for root in roots {
            if marks.contains_key(&root) {
                continue;
            }
            let mut stack: Vec<(CellAddr, usize)> = vec![(root, 0)];
            marks.insert(root, Mark::Open);

            while let Some(&mut (node, ref mut visited)) = stack.last_mut() {
                let children = &successors[&node];
                if *visited < children.len() {
                    let child = children[*visited];
                    *visited += 1;
                    match marks.get(&child) {
                        Some(Mark::Done) => {}
                        Some(Mark::Open) => {
                            // Met a cell already on the stack: everything from
                            // it upward is in the cycle.
                            if let Some(from) = stack.iter().position(|(n, _)| *n == child) {
                                for (member, _) in &stack[from..] {
                                    circular.insert(*member);
                                }
                            }
                        }
                        None => {
                            marks.insert(child, Mark::Open);
                            stack.push((child, 0));
                        }
                    }
                } else {
                    marks.insert(node, Mark::Done);
                    finished.push(node);
                    stack.pop();
                }
            }
        }

        // Post-order reversed is the order in which to evaluate.
        finished.reverse();
        finished.retain(|addr| !circular.contains(addr));

        let mut circular: Vec<CellAddr> = circular.into_iter().collect();
        circular.sort_unstable();
        (finished, circular)
    }
}

/// A read-only view of the workbook, which is what the evaluator is given.
///
/// It exists so that evaluating one cell borrows the sheets immutably while
/// the loop around it still writes results back between cells.
struct View<'a> {
    sheets: &'a [Option<Sheet>],
    order: &'a [SheetId],
    names: &'a HashMap<String, Operand>,
    current: SheetId,
}

impl View<'_> {
    fn sheet(&self, id: SheetId) -> Option<&Sheet> {
        self.sheets.get(id.0 as usize)?.as_ref()
    }

    fn position_of(&self, name: &str) -> Option<usize> {
        self.order.iter().position(|id| {
            self.sheet(*id)
                .is_some_and(|s| s.name().eq_ignore_ascii_case(name))
        })
    }
}

impl Resolver for View<'_> {
    fn current_sheet(&self) -> SheetId {
        self.current
    }

    fn sheet_by_name(&self, name: &str) -> Option<SheetId> {
        self.position_of(name).map(|at| self.order[at])
    }

    fn sheets_between(&self, first: &str, last: &str) -> Option<Vec<SheetId>> {
        let a = self.position_of(first)?;
        let b = self.position_of(last)?;
        let (lo, hi) = (a.min(b), a.max(b));
        Some(self.order[lo..=hi].to_vec())
    }

    fn cell(&self, addr: CellAddr) -> Value {
        self.sheet(addr.sheet)
            .map_or(Value::Blank, |sheet| sheet.value(addr.cell))
    }

    fn used_bounds(&self, sheet: SheetId) -> Option<RangeRef> {
        self.sheet(sheet)?.used_bounds()
    }

    fn defined_name(&self, _sheet: Option<SheetId>, name: &str) -> Option<Operand> {
        self.names.get(&name.to_ascii_uppercase()).cloned()
    }
}

/// Convenience for addressing a cell by its A1 spelling.
pub fn addr(sheet: SheetId, a1: &str) -> Option<CellAddr> {
    Some(CellAddr::new(
        sheet,
        ferrum_core::A1Ref::parse(a1).ok()?.cell,
    ))
}

/// Convenience for a cell position from its A1 spelling.
pub fn cell_ref(a1: &str) -> Option<CellRef> {
    Some(ferrum_core::A1Ref::parse(a1).ok()?.cell)
}
