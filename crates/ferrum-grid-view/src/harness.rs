//! Driving the application the way a person does.
//!
//! The harness performs gestures, not method calls: it clicks where a cell is
//! drawn, presses the keys the interface sends, drags the edge it can see.
//! After each one it rebuilds the [`View`], so a test always reads what would
//! be on screen and can never read a picture that is one gesture stale.
//!
//! ```
//! use ferrum_grid_view::Harness;
//!
//! let mut grid = Harness::new();
//! grid.enter("A1", "2");
//! grid.enter("A2", "3");
//! grid.enter("A3", "=SUM(A1:A2)");
//! assert_eq!(grid.text_at("A3"), "5");
//! ```
//!
//! Everything a gesture cannot express, a test sets up through
//! [`with_app`](Harness::with_app). Everything it can, it should: a test that
//! reaches past the gestures stops proving that the gestures work.

use ferrum_core::{A1Ref, CellRef, RangeRef};
use ferrum_theme::Theme;

use crate::app::App;
use crate::view::{Cell, Header, View};

/// How wide one cell is in [`Harness::screen`], in characters.
const CELL: usize = 9;

/// How wide the row-number gutter is in [`Harness::screen`].
const GUTTER: usize = 4;

/// An application to drive, and the picture it is showing.
pub struct Harness {
    app: App,
    view: View,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    /// A fresh workbook in a window of a stated size.
    ///
    /// The size is fixed rather than inherited so that what is on screen is a
    /// property of the test and not of whoever runs it. 800 by 600 pixels at
    /// the default metrics is about fifteen columns and thirty rows.
    pub fn new() -> Self {
        Self::with_viewport(800.0, 600.0)
    }

    pub fn with_viewport(width: f32, height: f32) -> Self {
        let mut app = App::new();
        app.set_viewport(width, height);
        let view = app.view();
        Self { app, view }
    }

    // What is on screen.

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    /// Set something up that no gesture can express, then redraw.
    pub fn with_app(&mut self, change: impl FnOnce(&mut App)) {
        change(&mut self.app);
        self.sync();
    }

    /// What is drawn in a cell. Panics if it is not on screen, because a test
    /// asserting on a cell it cannot see is asserting on nothing.
    pub fn text_at(&self, at: &str) -> &str {
        &self.drawn(cell_of(at)).text
    }

    /// A cell as it is drawn, for whatever has no reader of its own.
    pub fn cell(&self, at: &str) -> &Cell {
        self.drawn(cell_of(at))
    }

    /// Whether a cell is drawn against its right edge, as numbers are.
    pub fn is_right_aligned(&self, at: &str) -> bool {
        self.drawn(cell_of(at)).align_right
    }

    /// The selection in A1 form, `B2:C4` or `A1` for a single cell.
    pub fn selection(&self) -> String {
        self.view.selection.to_a1()
    }

    /// The active cell in A1 form.
    pub fn active(&self) -> String {
        self.view.active.to_a1()
    }

    /// Everything visible, as text.
    pub fn screen(&self) -> String {
        let first = CellRef::new(
            self.view.rows.first().map_or(0, |row| row.index),
            self.view.columns.first().map_or(0, |column| column.index),
        );
        let last = CellRef::new(
            self.view.rows.last().map_or(0, |row| row.index),
            self.view.columns.last().map_or(0, |column| column.index),
        );
        self.render(RangeRef::new(first, last))
    }

    /// A block of the screen, as text, so an assertion is the size of what it
    /// is about.
    ///
    /// ```text
    ///     |A        |B        |
    ///   1 |Region   |Units    |
    ///   2 |<North>  |      120|
    /// ```
    ///
    /// Angle brackets mark the active cell, a trailing `~` marks text too
    /// wide for the field, and numbers sit to the right exactly as they do on
    /// screen. Panics if any of the block is scrolled out of sight.
    pub fn screen_of(&self, range: &str) -> String {
        self.render(range_of(range))
    }

    // Pointer.

    /// Press and release inside the cell area, in viewport pixels.
    pub fn click_at(&mut self, x: f32, y: f32) {
        self.app.pointer_down(x, y, false);
        self.sync();
    }

    /// Click the middle of a cell.
    pub fn click(&mut self, at: &str) {
        let (x, y) = self.middle_of(cell_of(at));
        self.click_at(x, y);
    }

    /// Click a cell holding shift, which extends the selection to it.
    pub fn shift_click(&mut self, at: &str) {
        let (x, y) = self.middle_of(cell_of(at));
        self.app.pointer_down(x, y, true);
        self.sync();
    }

    /// Sweep the pointer to a cell with the button down.
    pub fn drag_to(&mut self, at: &str) {
        let (x, y) = self.middle_of(cell_of(at));
        self.app.pointer_move(x, y);
        self.sync();
    }

    /// Select a cell without clicking it, as the name box does. The view
    /// scrolls to it if it is not already showing.
    pub fn go_to(&mut self, at: &str) {
        self.app.select(cell_of(at), false);
        self.sync();
    }

    // Keyboard.

    /// Press a key, named as the interface names it.
    ///
    /// Modifiers go in front: `ctrl+z`, `ctrl+shift+z`, `shift+tab`. `f2` is
    /// spelled that way here and sent as the interface's `edit`, because a
    /// test reads better with the key that is on the keyboard. Returns
    /// whether the application used it.
    pub fn press(&mut self, chord: &str) -> bool {
        let (modifiers, name) = match chord.rsplit_once('+') {
            Some((modifiers, name)) if !name.is_empty() => (modifiers, name),
            _ => ("", chord),
        };

        let mut ctrl = false;
        let mut shift = false;
        for modifier in modifiers.split('+').filter(|part| !part.is_empty()) {
            match modifier {
                "ctrl" | "control" => ctrl = true,
                "shift" => shift = true,
                other => panic!("no modifier called {other} in {chord}"),
            }
        }

        let name = if name.eq_ignore_ascii_case("f2") {
            "edit"
        } else {
            name
        };
        // A name that is neither the interface's nor a single character is a
        // typo in the test. Left alone it would arrive as text and quietly
        // type itself into a cell.
        match crate::key_from(name) {
            Some(crate::Key::Typed(text)) if text.chars().count() != 1 => panic!(
                "no key called {name}: press one of the interface's names, \
                 a single character, or f2"
            ),
            None => panic!("no key called {name}"),
            Some(_) => {}
        }

        let used = self.app.key_pressed(name, ctrl, shift);
        self.sync();
        used
    }

    /// Type, one keystroke at a time.
    ///
    /// The first character opens the editor and the rest go to the text field
    /// that is now on screen, which reports the whole string back. That is
    /// what the interface does, so it is what this does.
    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            if self.app.is_editing() {
                let mut current = self.app.edit_text();
                current.push(character);
                self.app.set_edit_text(current);
            } else {
                self.app.key_pressed(&character.to_string(), false, false);
            }
        }
        self.sync();
    }

    /// Go to a cell, type, and press Enter. The commonest gesture there is.
    pub fn enter(&mut self, at: &str, text: &str) {
        self.go_to(at);
        self.type_text(text);
        self.press("enter");
    }

    // The formula bar.

    /// Type into the formula bar, replacing what it held.
    pub fn type_in_formula_bar(&mut self, text: &str) {
        self.app.formula_edited(text.to_string());
        self.sync();
    }

    pub fn commit_formula_bar(&mut self) {
        self.app.commit_edit();
        self.sync();
    }

    // Headers.

    /// Click a column letter, which selects the column.
    pub fn click_column_header(&mut self, column: u32) {
        let x = self.middle_of_column(column);
        self.app.column_header_pressed(x);
        assert!(
            !self.app.is_resizing(),
            "pressing at x={x} grabbed an edge instead of selecting column {column}"
        );
        self.app.end_resize();
        self.sync();
    }

    /// Click a row number, which selects the row.
    pub fn click_row_header(&mut self, row: u32) {
        let y = self.middle_of_row(row);
        self.app.row_header_pressed(y);
        assert!(
            !self.app.is_resizing(),
            "pressing at y={y} grabbed an edge instead of selecting row {row}"
        );
        self.app.end_resize();
        self.sync();
    }

    /// Press on a column letter and sweep across to another, selecting both
    /// and everything between.
    pub fn drag_column_header(&mut self, from: u32, to: u32) {
        let start = self.middle_of_column(from);
        self.app.column_header_pressed(start);
        let end = self.middle_of_column(to);
        self.app.column_header_dragged(end);
        self.app.end_resize();
        self.sync();
    }

    /// Drag a column's right edge by a number of pixels.
    ///
    /// The press lands where the edge is drawn, and the harness checks that
    /// it actually grabbed the edge: a resize gesture that quietly turns into
    /// a selection is exactly the failure this is here to catch.
    pub fn drag_column_edge(&mut self, column: u32, by: f32) {
        let x = self.header(column).end() + self.view.offset_x;
        self.app.column_header_pressed(x);
        assert!(
            self.app.is_resizing(),
            "pressing at x={x} did not grab the right edge of column {column}"
        );
        self.app.column_header_dragged(x + by);
        self.app.end_resize();
        self.sync();
    }

    /// Drag a row's bottom edge by a number of pixels.
    pub fn drag_row_edge(&mut self, row: u32, by: f32) {
        let y = self.row_header(row).end() + self.view.offset_y;
        self.app.row_header_pressed(y);
        assert!(
            self.app.is_resizing(),
            "pressing at y={y} did not grab the bottom edge of row {row}"
        );
        self.app.row_header_dragged(y + by);
        self.app.end_resize();
        self.sync();
    }

    /// The corner above the row numbers, which selects the whole sheet.
    pub fn click_corner(&mut self) {
        self.app.select_all();
        self.sync();
    }

    // The rest of the chrome.

    /// A wheel gesture, with the interface's own signs: a delta is where the
    /// content goes, so scrolling down the sheet arrives as a negative `dy`.
    pub fn wheel(&mut self, dx: f32, dy: f32) {
        self.app.scroll_by(dx, dy);
        self.sync();
    }

    /// Move down the sheet by a number of pixels. Negative goes back up.
    pub fn scroll_down(&mut self, pixels: f32) {
        self.wheel(0.0, -pixels);
    }

    /// Move across the sheet by a number of pixels. Negative goes back left.
    pub fn scroll_right(&mut self, pixels: f32) {
        self.wheel(-pixels, 0.0);
    }

    /// Drag a scrollbar thumb to a fraction of its travel.
    pub fn scroll_to_fraction(&mut self, horizontal: bool, fraction: f32) {
        self.app.scroll_to_fraction(horizontal, fraction);
        self.sync();
    }

    /// Resize the window, which is the one gesture the interface starts
    /// rather than the person.
    pub fn resize_window(&mut self, width: f32, height: f32) {
        self.app.set_viewport(width, height);
        self.sync();
    }

    pub fn undo(&mut self) {
        self.app.undo();
        self.sync();
    }

    pub fn redo(&mut self) {
        self.app.redo();
        self.sync();
    }

    pub fn toggle_theme(&mut self) {
        self.app.toggle_theme();
        self.sync();
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.app.theme = theme;
        self.sync();
    }

    pub fn add_sheet(&mut self) {
        self.app.add_sheet();
        self.sync();
    }

    pub fn click_tab(&mut self, index: i32) {
        self.app.select_tab(index);
        self.sync();
    }

    // Inside.

    /// Rebuild the picture. Every gesture ends here, so the view is never one
    /// gesture behind, and building it on every gesture is itself a check:
    /// anything that would make the window panic panics in the test instead.
    fn sync(&mut self) {
        self.view = self.app.view();
    }

    fn drawn(&self, at: CellRef) -> &Cell {
        self.view
            .cell(at)
            .unwrap_or_else(|| panic!("{} is not on screen", at.to_a1()))
    }

    fn header(&self, column: u32) -> &Header {
        self.view
            .column(column)
            .unwrap_or_else(|| panic!("column {column} is not on screen"))
    }

    fn row_header(&self, row: u32) -> &Header {
        self.view
            .row(row)
            .unwrap_or_else(|| panic!("row {row} is not on screen"))
    }

    /// The middle of a cell, in the viewport pixels a pointer reports.
    fn middle_of(&self, at: CellRef) -> (f32, f32) {
        let rect = self.drawn(at).rect;
        (
            rect.x + self.view.offset_x + rect.width / 2.0,
            rect.y + self.view.offset_y + rect.height / 2.0,
        )
    }

    fn middle_of_column(&self, column: u32) -> f32 {
        let header = self.header(column);
        header.pos + header.size / 2.0 + self.view.offset_x
    }

    fn middle_of_row(&self, row: u32) -> f32 {
        let header = self.row_header(row);
        header.pos + header.size / 2.0 + self.view.offset_y
    }

    fn render(&self, range: RangeRef) -> String {
        let mut out = String::new();

        out.push_str(&" ".repeat(GUTTER));
        for column in range.start.col..=range.end.col {
            out.push('|');
            out.push_str(&centred(&self.header(column).label, CELL));
        }
        out.push('|');

        for row in range.start.row..=range.end.row {
            out.push('\n');
            let label = self.row_header(row).label.clone();
            out.push_str(&format!("{label:>width$} ", width = GUTTER - 1));
            for column in range.start.col..=range.end.col {
                let at = CellRef::new(row, column);
                let cell = self.drawn(at);
                let text = if at == self.view.active {
                    format!("<{}>", cell.text)
                } else {
                    cell.text.clone()
                };
                out.push('|');
                out.push_str(&field(&text, cell.align_right, CELL));
            }
            out.push('|');
        }

        out
    }
}

/// Put text in a field of a fixed width, the way the cell does: against one
/// edge, and clipped if it does not fit.
fn field(text: &str, align_right: bool, width: usize) -> String {
    let count = text.chars().count();
    if count > width {
        let mut out: String = text.chars().take(width - 1).collect();
        out.push('~');
        return out;
    }
    let padding = " ".repeat(width - count);
    if align_right {
        padding + text
    } else {
        text.to_string() + &padding
    }
}

fn centred(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count >= width {
        return text.chars().take(width).collect();
    }
    let left = (width - count) / 2;
    format!(
        "{}{text}{}",
        " ".repeat(left),
        " ".repeat(width - count - left)
    )
}

fn cell_of(text: &str) -> CellRef {
    A1Ref::parse(text)
        .unwrap_or_else(|_| panic!("{text} is not a cell reference"))
        .cell
}

fn range_of(text: &str) -> RangeRef {
    match text.split_once(':') {
        Some((first, last)) => RangeRef::new(cell_of(first), cell_of(last)),
        None => RangeRef::single(cell_of(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_pads_on_the_side_the_text_is_not_against() {
        assert_eq!(field("ab", false, 5), "ab   ");
        assert_eq!(field("ab", true, 5), "   ab");
    }

    #[test]
    fn a_field_marks_where_it_clipped() {
        assert_eq!(field("abcdef", false, 4), "abc~");
    }

    #[test]
    fn a_chord_without_a_modifier_is_just_the_key() {
        let mut grid = Harness::new();
        grid.press("down");
        assert_eq!(grid.active(), "A2");
    }

    #[test]
    #[should_panic(expected = "no key called f9")]
    fn a_key_the_interface_cannot_send_fails_the_test_rather_than_typing_itself() {
        Harness::new().press("f9");
    }

    #[test]
    #[should_panic(expected = "no modifier called alt")]
    fn a_modifier_that_is_not_handled_fails_the_test() {
        Harness::new().press("alt+down");
    }

    #[test]
    #[should_panic(expected = "ZZ1 is not on screen")]
    fn asserting_on_a_cell_that_is_not_showing_fails_rather_than_passing() {
        Harness::new().text_at("ZZ1");
    }
}
