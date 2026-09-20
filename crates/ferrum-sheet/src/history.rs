//! Undo and redo.
//!
//! The rule this is built around: **one undo gives back one gesture.** A drag
//! that resizes a column fires a hundred size changes and is one step. A paste
//! over fifty cells is one step. Typing in a cell is one step. Nothing the
//! machine did on its own account, such as recalculating a dependent formula,
//! appears in the chain at all, because the user did not do it and cannot
//! predict it.
//!
//! A gesture is bracketed by [`History::begin`] and [`History::end`]. Anything
//! recorded outside a bracket is a step of its own, which is the right answer
//! for a single keystroke and means a caller cannot forget to open one.

use ferrum_core::{CellAddr, SheetId};

use crate::cell::Input;

/// How many gestures are kept. Beyond this the oldest is forgotten.
///
/// Each step holds only the cells it touched, so a long session of single-cell
/// edits costs little; a hundred is well past what anyone reaches back through.
pub const DEPTH: usize = 100;

/// One cell, before and after. `None` means the cell was empty.
#[derive(Clone, Debug)]
pub struct CellEdit {
    pub addr: CellAddr,
    pub before: Option<Input>,
    pub after: Option<Input>,
}

/// One row height or column width, before and after. `None` means it had no
/// size of its own and used the default.
#[derive(Clone, Copy, Debug)]
pub struct SizeEdit {
    pub sheet: SheetId,
    pub index: u32,
    pub is_column: bool,
    pub before: Option<f64>,
    pub after: Option<f64>,
}

/// Everything one gesture changed.
#[derive(Clone, Debug)]
pub struct Change {
    /// What the gesture was, for an "Undo typing" label.
    pub label: String,
    pub cells: Vec<CellEdit>,
    pub sizes: Vec<SizeEdit>,
}

impl Change {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            cells: Vec::new(),
            sizes: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() && self.sizes.is_empty()
    }

    /// Record a cell changing.
    ///
    /// When the same cell is touched twice in one gesture the earliest
    /// `before` and the latest `after` are kept, so undo returns to the state
    /// before the gesture began rather than to somewhere in the middle of it.
    fn record_cell(&mut self, addr: CellAddr, before: Option<Input>, after: Option<Input>) {
        if let Some(existing) = self.cells.iter_mut().find(|e| e.addr == addr) {
            existing.after = after;
        } else {
            self.cells.push(CellEdit {
                addr,
                before,
                after,
            });
        }
    }

    fn record_size(
        &mut self,
        sheet: SheetId,
        index: u32,
        is_column: bool,
        before: Option<f64>,
        after: Option<f64>,
    ) {
        if let Some(existing) = self
            .sizes
            .iter_mut()
            .find(|e| e.sheet == sheet && e.index == index && e.is_column == is_column)
        {
            existing.after = after;
        } else {
            self.sizes.push(SizeEdit {
                sheet,
                index,
                is_column,
                before,
                after,
            });
        }
    }
}

/// The undo and redo stacks.
#[derive(Default)]
pub struct History {
    past: Vec<Change>,
    future: Vec<Change>,
    /// The gesture currently being collected, if one is open.
    open: Option<Change>,
    /// How many times `begin` has been called without a matching `end`.
    ///
    /// Nesting is counted rather than refused, so a routine that brackets its
    /// own work can be called from inside a larger gesture and still produce
    /// one step.
    depth: u32,
    /// Set while an undo or a redo is being applied, so the edits it makes are
    /// not recorded as new history.
    replaying: bool,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a gesture. Everything recorded until the matching [`Self::end`] is
    /// one undo step.
    pub fn begin(&mut self, label: &str) {
        if self.depth == 0 {
            self.open = Some(Change::new(label));
        }
        self.depth += 1;
    }

    /// Close a gesture and push it, unless it changed nothing.
    pub fn end(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        if self.depth > 0 {
            return;
        }
        if let Some(change) = self.open.take()
            && !change.is_empty()
        {
            self.push(change);
        }
    }

    fn push(&mut self, change: Change) {
        self.past.push(change);
        if self.past.len() > DEPTH {
            self.past.remove(0);
        }
        // A new change makes the old future unreachable.
        self.future.clear();
    }

    pub const fn is_replaying(&self) -> bool {
        self.replaying
    }

    /// Note a cell changing. Ignored while an undo is being applied.
    pub fn record_cell(&mut self, addr: CellAddr, before: Option<Input>, after: Option<Input>) {
        if self.replaying {
            return;
        }
        match &mut self.open {
            Some(change) => change.record_cell(addr, before, after),
            None => {
                // Outside a gesture, one edit is one step.
                let mut change = Change::new("edit");
                change.record_cell(addr, before, after);
                self.push(change);
            }
        }
    }

    pub fn record_size(
        &mut self,
        sheet: SheetId,
        index: u32,
        is_column: bool,
        before: Option<f64>,
        after: Option<f64>,
    ) {
        if self.replaying {
            return;
        }
        match &mut self.open {
            Some(change) => change.record_size(sheet, index, is_column, before, after),
            None => {
                let mut change = Change::new(if is_column {
                    "resize column"
                } else {
                    "resize row"
                });
                change.record_size(sheet, index, is_column, before, after);
                self.push(change);
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// What the next undo would take back.
    pub fn undo_label(&self) -> Option<&str> {
        self.past.last().map(|c| c.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.future.last().map(|c| c.label.as_str())
    }

    /// Take the next gesture to undo. The caller applies it and then calls
    /// [`Self::finish_replay`].
    pub fn take_undo(&mut self) -> Option<Change> {
        let change = self.past.pop()?;
        self.replaying = true;
        Some(change)
    }

    pub fn take_redo(&mut self) -> Option<Change> {
        let change = self.future.pop()?;
        self.replaying = true;
        Some(change)
    }

    /// Put a replayed gesture on the other stack and start recording again.
    pub fn finish_undo(&mut self, change: Change) {
        self.replaying = false;
        self.future.push(change);
    }

    pub fn finish_redo(&mut self, change: Change) {
        self.replaying = false;
        self.past.push(change);
    }

    /// Forget everything, as when a different file is opened.
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
        self.open = None;
        self.depth = 0;
        self.replaying = false;
    }

    pub fn undo_depth(&self) -> usize {
        self.past.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.future.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_core::CellRef;

    const SHEET: SheetId = SheetId(0);

    fn at(row: u32, col: u32) -> CellAddr {
        CellAddr::new(SHEET, CellRef::new(row, col))
    }

    fn literal(text: &str) -> Option<Input> {
        Some(crate::cell::parse_input(text))
    }

    #[test]
    fn a_lone_edit_is_its_own_step() {
        let mut history = History::new();
        history.record_cell(at(0, 0), None, literal("1"));
        assert_eq!(history.undo_depth(), 1);
        assert!(history.can_undo());
    }

    #[test]
    fn a_bracketed_gesture_is_one_step_however_many_edits() {
        let mut history = History::new();
        history.begin("paste");
        for row in 0..50 {
            history.record_cell(at(row, 0), None, literal("1"));
        }
        history.end();
        assert_eq!(history.undo_depth(), 1);
        assert_eq!(history.undo_label(), Some("paste"));
    }

    #[test]
    fn touching_one_cell_twice_keeps_the_state_before_the_gesture() {
        let mut history = History::new();
        history.begin("drag");
        history.record_cell(at(0, 0), literal("original"), literal("first"));
        history.record_cell(at(0, 0), literal("first"), literal("second"));
        history.end();

        let change = history.take_undo().unwrap();
        assert_eq!(change.cells.len(), 1);
        // Undo must return to what was there before the drag began, not to
        // the halfway point.
        let before = change.cells[0].before.as_ref().unwrap();
        assert!(matches!(before, Input::Literal(v) if v.display() == "original"));
        let after = change.cells[0].after.as_ref().unwrap();
        assert!(matches!(after, Input::Literal(v) if v.display() == "second"));
    }

    #[test]
    fn a_resize_drag_collapses_to_one_step() {
        let mut history = History::new();
        history.begin("resize column");
        for width in 1..100 {
            history.record_size(SHEET, 3, true, Some(48.0), Some(f64::from(width)));
        }
        history.end();

        assert_eq!(history.undo_depth(), 1);
        let change = history.take_undo().unwrap();
        assert_eq!(change.sizes.len(), 1);
        assert_eq!(change.sizes[0].before, Some(48.0));
        assert_eq!(change.sizes[0].after, Some(99.0));
    }

    #[test]
    fn a_gesture_that_changed_nothing_is_not_recorded() {
        let mut history = History::new();
        history.begin("nothing happened");
        history.end();
        assert!(!history.can_undo());
    }

    #[test]
    fn nested_gestures_produce_one_step() {
        let mut history = History::new();
        history.begin("outer");
        history.begin("inner");
        history.record_cell(at(0, 0), None, literal("1"));
        history.end();
        history.record_cell(at(1, 0), None, literal("2"));
        history.end();

        assert_eq!(history.undo_depth(), 1);
        assert_eq!(history.undo_label(), Some("outer"));
    }

    #[test]
    fn edits_made_while_replaying_are_not_recorded() {
        let mut history = History::new();
        history.record_cell(at(0, 0), None, literal("1"));
        let change = history.take_undo().unwrap();
        assert!(history.is_replaying());

        // Applying the undo writes cells, and none of that is new history.
        history.record_cell(at(0, 0), literal("1"), None);
        history.finish_undo(change);

        assert!(!history.can_undo());
        assert!(history.can_redo());
    }

    #[test]
    fn undo_and_redo_move_between_the_stacks() {
        let mut history = History::new();
        history.record_cell(at(0, 0), None, literal("1"));
        history.record_cell(at(1, 0), None, literal("2"));
        assert_eq!(history.undo_depth(), 2);

        let change = history.take_undo().unwrap();
        history.finish_undo(change);
        assert_eq!(history.undo_depth(), 1);
        assert_eq!(history.redo_depth(), 1);

        let change = history.take_redo().unwrap();
        history.finish_redo(change);
        assert_eq!(history.undo_depth(), 2);
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn a_new_edit_discards_the_redo_stack() {
        let mut history = History::new();
        history.record_cell(at(0, 0), None, literal("1"));
        let change = history.take_undo().unwrap();
        history.finish_undo(change);
        assert!(history.can_redo());

        history.record_cell(at(5, 5), None, literal("new"));
        assert!(!history.can_redo(), "the old future is unreachable now");
    }

    #[test]
    fn the_stack_is_bounded() {
        let mut history = History::new();
        for row in 0..(DEPTH as u32 + 50) {
            history.record_cell(at(row, 0), None, literal("1"));
        }
        assert_eq!(history.undo_depth(), DEPTH);
    }

    #[test]
    fn clearing_forgets_everything() {
        let mut history = History::new();
        history.record_cell(at(0, 0), None, literal("1"));
        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
