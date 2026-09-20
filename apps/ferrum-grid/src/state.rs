//! What the window is showing, and how it turns into something to draw.
//!
//! Two unit systems meet here and the split is deliberate.
//!
//! **Points** are the document's own unit. Scroll position, column widths and
//! row heights are all points, because a column is the same width when the
//! file is opened on a different screen.
//!
//! **Pixels** are the screen's, and appear only where something is drawn.
//! Every cell boundary is rounded to a whole pixel from its cumulative offset
//! rather than by adding up rounded widths, so a thousand columns along the
//! gridlines are still exactly where they belong.

use std::sync::OnceLock;

use ferrum_core::defaults::{COLUMN_WIDTH_PT, MIN_VISIBLE_PT, ROW_HEIGHT_PT};
use ferrum_core::{CellAddr, CellRef, MAX_COL, MAX_ROW, RangeRef, SheetId, Value};
use ferrum_sheet::Workbook;
use ferrum_sheet::axis::Axis;
use ferrum_theme::Theme;
use ferrum_theme::metrics::Metrics as GridMetrics;

/// How far past the last used row or column the view may scroll, in entries.
const SCROLL_MARGIN: u32 = 40;

/// What a keystroke asked for.
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Tab,
    Escape,
    Delete,
    Backspace,
    Edit,
    /// A printable character that starts an edit.
    Typed(String),
}

/// One visible row or column, positioned in whole pixels relative to the
/// first one on screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Span {
    pub index: u32,
    pub pos: f32,
    pub size: f32,
}

/// A resize in progress.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Drag {
    None,
    Column { index: u32, from: f64 },
    Row { index: u32, from: f64 },
}

pub struct App {
    pub book: Workbook,
    pub sheet: SheetId,
    pub theme: Theme,
    metrics: GridMetrics,

    /// Scroll position in points, from the top-left of the sheet.
    scroll_x: f64,
    scroll_y: f64,
    /// Viewport size in pixels, as the interface reports it.
    viewport_w: f32,
    viewport_h: f32,

    /// The fixed corner of the selection.
    anchor: CellRef,
    /// The moving corner. Equal to the anchor for a single cell.
    cursor: CellRef,

    editing: Option<String>,
    drag: Drag,
    /// Where a header drag began, in pixels along the header.
    press_at: f32,
    status: String,
}

impl App {
    pub fn new() -> Self {
        let book = Workbook::new();
        let sheet = book.first_sheet();
        Self {
            book,
            sheet,
            theme: Theme::default(),
            metrics: GridMetrics::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            viewport_w: 800.0,
            viewport_h: 600.0,
            anchor: CellRef::new(0, 0),
            cursor: CellRef::new(0, 0),
            editing: None,
            drag: Drag::None,
            press_at: 0.0,
            status: String::new(),
        }
    }

    // Units.

    pub fn metrics(&self) -> GridMetrics {
        self.metrics
    }

    fn to_px(&self, points: f64) -> f64 {
        self.metrics.points_to_pixels(points)
    }

    fn to_pt(&self, pixels: f64) -> f64 {
        self.metrics.pixels_to_points(pixels)
    }

    /// A cumulative offset, rounded to a whole pixel.
    ///
    /// Rounding the running total rather than each size is what keeps the
    /// gridlines true: rounded widths added together drift by a pixel every
    /// few columns.
    fn edge_px(&self, axis: &Axis, index: u32) -> f64 {
        self.to_px(axis.offset_of(index)).round()
    }

    /// The column widths of the sheet being shown.
    ///
    /// A workbook always has a sheet, so the fallback only exists to keep this
    /// total; it is never reached in practice.
    fn columns(&self) -> &Axis {
        static FALLBACK: OnceLock<Axis> = OnceLock::new();
        self.book.sheet(self.sheet).map_or_else(
            || FALLBACK.get_or_init(|| Axis::new(COLUMN_WIDTH_PT, MAX_COL)),
            ferrum_sheet::Sheet::columns,
        )
    }

    fn rows(&self) -> &Axis {
        static FALLBACK: OnceLock<Axis> = OnceLock::new();
        self.book.sheet(self.sheet).map_or_else(
            || FALLBACK.get_or_init(|| Axis::new(ROW_HEIGHT_PT, MAX_ROW)),
            ferrum_sheet::Sheet::rows,
        )
    }

    pub fn default_row_height_px(&self) -> f32 {
        self.to_px(self.rows().default_size()) as f32
    }

    pub fn default_col_width_px(&self) -> f32 {
        self.to_px(self.columns().default_size()) as f32
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_w = width.max(1.0);
        self.viewport_h = height.max(1.0);
        self.clamp_scroll();
    }

    /// The viewport in points, which is what the axes are measured in.
    fn viewport_pt(&self) -> (f64, f64) {
        (
            self.to_pt(f64::from(self.viewport_w)),
            self.to_pt(f64::from(self.viewport_h)),
        )
    }

    // The visible window.

    pub fn visible_columns(&self) -> Vec<Span> {
        let (width_pt, _) = self.viewport_pt();
        self.spans(self.columns(), self.scroll_x, width_pt)
    }

    pub fn visible_rows(&self) -> Vec<Span> {
        let (_, height_pt) = self.viewport_pt();
        self.spans(self.rows(), self.scroll_y, height_pt)
    }

    fn spans(&self, axis: &Axis, scroll: f64, extent: f64) -> Vec<Span> {
        let (first, last) = axis.visible_range(scroll, extent);
        let origin = self.edge_px(axis, first);
        let mut out = Vec::with_capacity((last - first + 1) as usize);
        let mut edge = origin;
        for index in first..=last {
            let next = self.edge_px(axis, index.saturating_add(1));
            let size = next - edge;
            // A hidden entry occupies nothing and is not drawn.
            if size > 0.0 {
                out.push(Span {
                    index,
                    pos: (edge - origin) as f32,
                    size: size as f32,
                });
            }
            edge = next;
        }
        out
    }

    /// How far the first visible entry is scrolled off the edge, in pixels.
    ///
    /// Zero or negative, and applied as a translation so that scrolling part
    /// of a row rebuilds nothing.
    pub fn offset_x(&self) -> f32 {
        let (first, _) = self.columns().index_at(self.scroll_x);
        (self.edge_px(self.columns(), first) - self.to_px(self.scroll_x).round()) as f32
    }

    pub fn offset_y(&self) -> f32 {
        let (first, _) = self.rows().index_at(self.scroll_y);
        (self.edge_px(self.rows(), first) - self.to_px(self.scroll_y).round()) as f32
    }

    /// Where the active cell sits inside the visible window, in pixels, or
    /// `None` when it is scrolled out of sight.
    pub fn active_box(&self) -> Option<(f32, f32, f32, f32)> {
        let active = self.anchor;
        let column = self
            .visible_columns()
            .into_iter()
            .find(|s| s.index == active.col)?;
        let row = self
            .visible_rows()
            .into_iter()
            .find(|s| s.index == active.row)?;
        Some((column.pos, row.pos, column.size, row.size))
    }

    // Scrolling.

    fn scroll_limit(&self, axis: &Axis, furthest: u32, limit: u32, viewport: f64) -> f64 {
        let end = furthest.saturating_add(SCROLL_MARGIN).min(limit);
        (axis.offset_of(end.saturating_add(1)) - viewport).max(0.0)
    }

    fn max_scroll_y(&self) -> f64 {
        let used = self
            .book
            .sheet(self.sheet)
            .and_then(ferrum_sheet::Sheet::used_bounds)
            .map_or(0, |b| b.end.row);
        // Past the data and past the selection, so the view can always follow
        // the cursor wherever it was moved.
        let furthest = used.max(self.anchor.row).max(self.cursor.row.min(MAX_ROW));
        let (_, viewport) = self.viewport_pt();
        self.scroll_limit(self.rows(), furthest, MAX_ROW, viewport)
    }

    fn max_scroll_x(&self) -> f64 {
        let used = self
            .book
            .sheet(self.sheet)
            .and_then(ferrum_sheet::Sheet::used_bounds)
            .map_or(0, |b| b.end.col);
        let furthest = used.max(self.anchor.col).max(self.cursor.col.min(MAX_COL));
        let (viewport, _) = self.viewport_pt();
        self.scroll_limit(self.columns(), furthest, MAX_COL, viewport)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_x = self.scroll_x.clamp(0.0, self.max_scroll_x());
        self.scroll_y = self.scroll_y.clamp(0.0, self.max_scroll_y());
    }

    /// Scroll by a wheel or trackpad delta, which arrives in pixels.
    pub fn scroll_by(&mut self, dx: f32, dy: f32) {
        self.scroll_x -= self.to_pt(f64::from(dx));
        self.scroll_y -= self.to_pt(f64::from(dy));
        self.clamp_scroll();
    }

    pub fn scroll_to_fraction(&mut self, horizontal: bool, fraction: f32) {
        let fraction = f64::from(fraction.clamp(0.0, 1.0));
        if horizontal {
            self.scroll_x = fraction * self.max_scroll_x();
        } else {
            self.scroll_y = fraction * self.max_scroll_y();
        }
        self.clamp_scroll();
    }

    /// Where a scrollbar thumb sits and how big it is, as fractions.
    pub fn thumb(&self, horizontal: bool) -> (f32, f32) {
        let (width_pt, height_pt) = self.viewport_pt();
        let (position, max, viewport) = if horizontal {
            (self.scroll_x, self.max_scroll_x(), width_pt)
        } else {
            (self.scroll_y, self.max_scroll_y(), height_pt)
        };
        let total = max + viewport;
        let size = if total > 0.0 {
            (viewport / total).clamp(0.02, 1.0)
        } else {
            1.0
        };
        let start = if max > 0.0 {
            (position / max) * (1.0 - size)
        } else {
            0.0
        };
        (start as f32, size as f32)
    }

    /// Which cell a point inside the cell area falls on.
    pub fn cell_at(&self, x: f32, y: f32) -> CellRef {
        let at_x = self.scroll_x + self.to_pt(f64::from(x.max(0.0)));
        let at_y = self.scroll_y + self.to_pt(f64::from(y.max(0.0)));
        CellRef::new(
            self.rows().index_at(at_y).0,
            self.columns().index_at(at_x).0,
        )
    }

    /// Scroll the smallest amount that brings a cell fully into view.
    pub fn scroll_into_view(&mut self, cell: CellRef) {
        let (width_pt, height_pt) = self.viewport_pt();

        let top = self.rows().offset_of(cell.row);
        let bottom = top + self.rows().size_of(cell.row);
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if bottom > self.scroll_y + height_pt {
            self.scroll_y = bottom - height_pt;
        }

        let left = self.columns().offset_of(cell.col);
        let right = left + self.columns().size_of(cell.col);
        if left < self.scroll_x {
            self.scroll_x = left;
        } else if right > self.scroll_x + width_pt {
            self.scroll_x = right - width_pt;
        }

        self.clamp_scroll();
    }

    // Resizing.
    //
    // The header has one pointer area rather than one per column, so nothing
    // is rebuilt as the sheet scrolls and there is no stack of overlapping
    // handlers to get the ordering wrong. Whether a press means "resize this
    // edge" or "select this column" is decided here, where it can be tested.

    /// How close to an edge a pointer must be to grab it, in pixels.
    pub const GRAB_TOLERANCE: f32 = 5.0;

    /// The column whose right edge is under `x`, if one is close enough.
    ///
    /// `x` is measured along the header band, which shares its origin with the
    /// cell area.
    pub fn column_edge_near(&self, x: f32) -> Option<u32> {
        let local = x - self.offset_x();
        self.visible_columns()
            .into_iter()
            .find(|span| (local - (span.pos + span.size)).abs() <= Self::GRAB_TOLERANCE)
            .map(|span| span.index)
    }

    /// The row whose bottom edge is under `y`.
    pub fn row_edge_near(&self, y: f32) -> Option<u32> {
        let local = y - self.offset_y();
        self.visible_rows()
            .into_iter()
            .find(|span| (local - (span.pos + span.size)).abs() <= Self::GRAB_TOLERANCE)
            .map(|span| span.index)
    }

    /// A press on the column header: resize an edge, or select a column.
    pub fn column_header_pressed(&mut self, x: f32) {
        self.press_at = x;
        match self.column_edge_near(x) {
            Some(index) => self.begin_column_resize(index),
            None => self.select_column(self.cell_at(x, 0.0).col),
        }
    }

    pub fn row_header_pressed(&mut self, y: f32) {
        self.press_at = y;
        match self.row_edge_near(y) {
            Some(index) => self.begin_row_resize(index),
            None => self.select_row(self.cell_at(0.0, y).row),
        }
    }

    /// Continue a header drag: resize if one is under way, otherwise extend
    /// the selection across columns or rows.
    pub fn column_header_dragged(&mut self, x: f32) {
        if self.is_resizing() {
            self.drag_resize(x - self.press_at);
        } else {
            let col = self.cell_at(x, 0.0).col;
            self.cursor = CellRef::new(MAX_ROW, col);
        }
    }

    pub fn row_header_dragged(&mut self, y: f32) {
        if self.is_resizing() {
            self.drag_resize(y - self.press_at);
        } else {
            let row = self.cell_at(0.0, y).row;
            self.cursor = CellRef::new(row, MAX_COL);
        }
    }

    pub fn begin_column_resize(&mut self, index: u32) {
        let from = self.columns().size_of(index);
        self.drag = Drag::Column { index, from };
    }

    pub fn begin_row_resize(&mut self, index: u32) {
        let from = self.rows().size_of(index);
        self.drag = Drag::Row { index, from };
    }

    /// Continue a resize. `delta` is the whole distance dragged so far, in
    /// pixels, measured from where the drag started.
    ///
    /// Taking the total rather than a per-event increment means a dropped
    /// event cannot leave the size drifting behind the pointer.
    pub fn drag_resize(&mut self, delta: f32) {
        let change = self.to_pt(f64::from(delta));
        let (index, size, is_column) = match self.drag {
            Drag::None => return,
            Drag::Column { index, from } => (index, snap_to_hidden(from + change), true),
            Drag::Row { index, from } => (index, snap_to_hidden(from + change), false),
        };
        if let Some(sheet) = self.book.sheet_mut(self.sheet) {
            if is_column {
                sheet.columns_mut().set_size(index, Some(size));
            } else {
                sheet.rows_mut().set_size(index, Some(size));
            }
        }
        self.clamp_scroll();
    }

    pub fn end_resize(&mut self) {
        self.drag = Drag::None;
    }

    pub fn is_resizing(&self) -> bool {
        self.drag != Drag::None
    }

    // Selection.

    pub fn active(&self) -> CellRef {
        self.anchor
    }

    pub fn selection(&self) -> RangeRef {
        RangeRef::new(self.anchor, self.cursor)
    }

    pub fn active_addr(&self) -> CellAddr {
        CellAddr::new(self.sheet, self.anchor)
    }

    pub fn select(&mut self, cell: CellRef, extend: bool) {
        if extend {
            self.cursor = cell;
        } else {
            self.anchor = cell;
            self.cursor = cell;
        }
        self.scroll_into_view(if extend { self.cursor } else { self.anchor });
    }

    pub fn extend_to(&mut self, cell: CellRef) {
        self.cursor = cell;
    }

    pub fn select_column(&mut self, col: u32) {
        self.anchor = CellRef::new(0, col);
        self.cursor = CellRef::new(MAX_ROW, col);
    }

    pub fn select_row(&mut self, row: u32) {
        self.anchor = CellRef::new(row, 0);
        self.cursor = CellRef::new(row, MAX_COL);
    }

    pub fn select_all(&mut self) {
        self.anchor = CellRef::new(0, 0);
        self.cursor = CellRef::new(MAX_ROW, MAX_COL);
    }

    // Editing.

    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    pub fn edit_text(&self) -> String {
        self.editing.clone().unwrap_or_default()
    }

    pub fn set_edit_text(&mut self, text: String) {
        if self.editing.is_some() {
            self.editing = Some(text);
        }
    }

    /// Open the active cell for editing.
    ///
    /// `replace` is what a printable keystroke passes: typing over a cell
    /// discards what was there, while F2 keeps it to be amended.
    pub fn begin_edit(&mut self, replace: Option<String>) {
        self.editing = Some(match replace {
            Some(text) => text,
            None => self.book.edit_text(self.active_addr()),
        });
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    pub fn commit_edit(&mut self) {
        let Some(text) = self.editing.take() else {
            return;
        };
        let addr = self.active_addr();
        let report = self.book.set_input(addr, &text);
        self.status = if report.is_circular() {
            format!(
                "Circular reference: {} cells refer back to themselves",
                report.circular.len()
            )
        } else {
            String::new()
        };
    }

    /// Empty every cell in the selection.
    pub fn clear_selection(&mut self) {
        let cells: Vec<CellRef> = self.clipped_selection().cells().collect();
        for cell in cells {
            self.book.clear(CellAddr::new(self.sheet, cell));
        }
    }

    /// The selection, narrowed to the cells the sheet actually holds.
    ///
    /// Selecting a whole column addresses a million cells. Anything that walks
    /// the selection walks this instead.
    fn clipped_selection(&self) -> RangeRef {
        let selection = self.selection();
        self.book
            .sheet(self.sheet)
            .and_then(ferrum_sheet::Sheet::used_bounds)
            .and_then(|used| selection.intersection(&used))
            .unwrap_or(RangeRef::single(self.anchor))
    }

    // Keyboard.

    /// Returns true when the key was used.
    pub fn handle_key(&mut self, key: Key, ctrl: bool, shift: bool) -> bool {
        if self.is_editing() {
            return self.handle_key_while_editing(key);
        }

        let page = self.visible_rows().len().saturating_sub(2).max(1) as i64;
        match key {
            Key::Up => self.step(-1, 0, shift, ctrl),
            Key::Down => self.step(1, 0, shift, ctrl),
            Key::Left => self.step(0, -1, shift, ctrl),
            Key::Right => self.step(0, 1, shift, ctrl),
            Key::PageUp => self.step(-page, 0, shift, false),
            Key::PageDown => self.step(page, 0, shift, false),
            Key::Home => {
                let row = if ctrl { 0 } else { self.anchor.row };
                self.select(CellRef::new(row, 0), shift);
            }
            Key::End => {
                let bounds = self
                    .book
                    .sheet(self.sheet)
                    .and_then(ferrum_sheet::Sheet::used_bounds);
                let target = bounds.map_or(CellRef::new(0, 0), |b| {
                    if ctrl {
                        b.end
                    } else {
                        CellRef::new(self.anchor.row, b.end.col)
                    }
                });
                self.select(target, shift);
            }
            Key::Enter => self.step(1, 0, false, false),
            Key::Tab => self.step(0, if shift { -1 } else { 1 }, false, false),
            Key::Escape => {}
            Key::Delete | Key::Backspace => self.clear_selection(),
            Key::Edit => self.begin_edit(None),
            Key::Typed(text) => self.begin_edit(Some(text)),
        }
        true
    }

    fn handle_key_while_editing(&mut self, key: Key) -> bool {
        match key {
            Key::Escape => {
                self.cancel_edit();
                true
            }
            Key::Enter => {
                self.commit_edit();
                self.step(1, 0, false, false);
                true
            }
            Key::Tab => {
                self.commit_edit();
                self.step(0, 1, false, false);
                true
            }
            // Everything else belongs to the text field.
            _ => false,
        }
    }

    /// Move the active cell, or the selection's moving corner.
    fn step(&mut self, rows: i64, cols: i64, extend: bool, jump: bool) {
        let from = if extend { self.cursor } else { self.anchor };
        let target = if jump {
            self.jump_target(from, rows, cols)
        } else {
            from.offset(rows, cols).unwrap_or(from)
        };
        self.select(target, extend);
    }

    /// The far edge of the populated block in a direction.
    fn jump_target(&self, from: CellRef, rows: i64, cols: i64) -> CellRef {
        let Some(bounds) = self
            .book
            .sheet(self.sheet)
            .and_then(ferrum_sheet::Sheet::used_bounds)
        else {
            return from;
        };
        match (rows.signum(), cols.signum()) {
            (-1, 0) => CellRef::new(bounds.start.row.min(from.row), from.col),
            (1, 0) => CellRef::new(bounds.end.row.max(from.row), from.col),
            (0, -1) => CellRef::new(from.row, bounds.start.col.min(from.col)),
            (0, 1) => CellRef::new(from.row, bounds.end.col.max(from.col)),
            _ => from,
        }
    }

    // What the chrome shows.

    pub fn name_box(&self) -> String {
        let selection = self.selection();
        if selection.start == selection.end {
            selection.start.to_a1()
        } else {
            format!("{} x {}", selection.height(), selection.width())
        }
    }

    pub fn formula_text(&self) -> String {
        self.book.edit_text(self.active_addr())
    }

    pub fn status(&self) -> String {
        if self.status.is_empty() {
            "Ready".to_string()
        } else {
            self.status.clone()
        }
    }

    /// The count, sum and average of the numbers in the selection.
    pub fn selection_summary(&self) -> String {
        let range = self.clipped_selection();
        if range.area() > 1_000_000 {
            return String::new();
        }

        let mut count = 0u64;
        let mut filled = 0u64;
        let mut total = 0.0;
        for cell in range.cells() {
            match self.book.value(CellAddr::new(self.sheet, cell)) {
                Value::Blank => {}
                Value::Number(n) => {
                    count += 1;
                    filled += 1;
                    total += n;
                }
                _ => filled += 1,
            }
        }

        if filled == 0 {
            return String::new();
        }
        if count == 0 {
            return format!("Count: {filled}");
        }
        format!(
            "Average: {}   Count: {filled}   Sum: {}",
            ferrum_core::format_general(total / count as f64),
            ferrum_core::format_general(total)
        )
    }

    pub fn toggle_theme(&mut self) {
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
    }

    /// The text and alignment one cell should draw with.
    pub fn cell_display(&self, cell: CellRef) -> (String, bool, bool) {
        let value = self.book.value(CellAddr::new(self.sheet, cell));
        let right = matches!(value, Value::Number(_));
        let error = matches!(value, Value::Error(_));
        let text = match value {
            Value::Blank => String::new(),
            Value::Error(e) => e.as_str().to_string(),
            other => other.display(),
        };
        (text, right, error)
    }
}

/// A size dragged below the visible minimum becomes hidden outright.
///
/// Leaving a one-point sliver would produce a row nobody can grab again.
fn snap_to_hidden(size: f64) -> f64 {
    if size < MIN_VISIBLE_PT { 0.0 } else { size }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.set_viewport(800.0, 600.0);
        app
    }

    fn set_column_width(app: &mut App, col: u32, points: f64) {
        let sheet = app.sheet;
        app.book
            .sheet_mut(sheet)
            .unwrap()
            .columns_mut()
            .set_size(col, Some(points));
    }

    #[test]
    fn a_fresh_view_starts_at_the_first_cell() {
        let app = app();
        assert_eq!(app.active(), CellRef::new(0, 0));
        assert_eq!(app.visible_rows()[0].index, 0);
        assert_eq!(app.visible_columns()[0].index, 0);
        assert_eq!(app.name_box(), "A1");
        assert_eq!(app.offset_x(), 0.0);
        assert_eq!(app.offset_y(), 0.0);
    }

    #[test]
    fn visible_spans_tile_without_gaps_or_overlaps() {
        let app = app();
        let columns = app.visible_columns();
        assert!(columns.len() > 5);
        for pair in columns.windows(2) {
            assert_eq!(
                pair[0].pos + pair[0].size,
                pair[1].pos,
                "column {} should end where {} begins",
                pair[0].index,
                pair[1].index
            );
        }
    }

    #[test]
    fn spans_stay_seamless_with_awkward_widths() {
        // Rounding each width on its own would drift; rounding the running
        // total does not.
        let mut app = app();
        for col in 0..12 {
            set_column_width(&mut app, col, 33.3);
        }
        for pair in app.visible_columns().windows(2) {
            assert_eq!(pair[0].pos + pair[0].size, pair[1].pos);
        }
    }

    #[test]
    fn a_resized_column_reports_its_new_width() {
        let mut app = app();
        set_column_width(&mut app, 1, 120.0);
        let columns = app.visible_columns();
        let second = columns.iter().find(|s| s.index == 1).unwrap();
        assert_eq!(second.size, app.to_px(120.0).round() as f32);
        let third = columns.iter().find(|s| s.index == 2).unwrap();
        assert_eq!(third.pos, second.pos + second.size);
    }

    #[test]
    fn a_hidden_column_is_not_drawn_at_all() {
        let mut app = app();
        set_column_width(&mut app, 1, 0.0);
        let columns = app.visible_columns();
        assert!(columns.iter().all(|s| s.index != 1));
        let first = columns.iter().find(|s| s.index == 0).unwrap();
        let third = columns.iter().find(|s| s.index == 2).unwrap();
        assert_eq!(third.pos, first.pos + first.size);
    }

    #[test]
    fn a_point_maps_back_to_the_cell_under_it() {
        let app = app();
        assert_eq!(app.cell_at(0.0, 0.0), CellRef::new(0, 0));
        let width = app.default_col_width_px();
        let height = app.default_row_height_px();
        assert_eq!(app.cell_at(width * 2.5, height * 3.5), CellRef::new(3, 2));
    }

    #[test]
    fn a_point_maps_back_across_a_resized_column() {
        let mut app = app();
        set_column_width(&mut app, 0, 200.0);
        let inside = app.to_px(190.0) as f32;
        assert_eq!(app.cell_at(inside, 0.0).col, 0);
        let past = app.to_px(210.0) as f32;
        assert_eq!(app.cell_at(past, 0.0).col, 1);
    }

    #[test]
    fn a_point_maps_back_correctly_after_scrolling() {
        let mut app = app();
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(500, 0)), "x");
        let height = app.default_row_height_px();
        app.scroll_by(0.0, -height * 10.0);
        assert_eq!(app.cell_at(0.0, 0.0), CellRef::new(10, 0));
    }

    #[test]
    fn dragging_an_edge_resizes_from_where_the_drag_began() {
        let mut app = app();
        let original = app.columns().size_of(2);
        app.begin_column_resize(2);
        // The delta is the whole distance so far, so repeating it is not
        // cumulative.
        let delta = app.to_px(30.0) as f32;
        app.drag_resize(delta);
        app.drag_resize(delta);
        assert!((app.columns().size_of(2) - (original + 30.0)).abs() < 0.01);
        app.end_resize();
        assert!(!app.is_resizing());
    }

    #[test]
    fn dragging_an_edge_past_its_neighbour_hides_the_column() {
        let mut app = app();
        let original = app.columns().size_of(3);
        app.begin_column_resize(3);
        app.drag_resize(-app.to_px(original + 50.0) as f32);
        assert!(app.columns().is_hidden(3));
    }

    #[test]
    fn rows_resize_the_same_way() {
        let mut app = app();
        let original = app.rows().size_of(4);
        app.begin_row_resize(4);
        app.drag_resize(app.to_px(12.0) as f32);
        assert!((app.rows().size_of(4) - (original + 12.0)).abs() < 0.01);
    }

    #[test]
    fn a_press_near_a_column_edge_starts_a_resize() {
        let mut app = app();
        let columns = app.visible_columns();
        let second = columns.iter().find(|s| s.index == 1).unwrap();
        let edge = second.pos + second.size;

        assert_eq!(app.column_edge_near(edge), Some(1));
        app.column_header_pressed(edge);
        assert!(app.is_resizing());

        let original = app.columns().size_of(1);
        app.column_header_dragged(edge + app.to_px(24.0) as f32);
        assert!((app.columns().size_of(1) - (original + 24.0)).abs() < 0.01);
    }

    #[test]
    fn a_press_away_from_an_edge_selects_the_column() {
        let mut app = app();
        let columns = app.visible_columns();
        let second = columns.iter().find(|s| s.index == 1).unwrap();
        let middle = second.pos + second.size / 2.0;

        assert_eq!(app.column_edge_near(middle), None);
        app.column_header_pressed(middle);
        assert!(!app.is_resizing());
        assert_eq!(app.selection().to_a1(), "B1:B1048576");
    }

    #[test]
    fn dragging_the_column_header_extends_the_selection() {
        let mut app = app();
        let columns = app.visible_columns();
        let first = columns.iter().find(|s| s.index == 0).unwrap();
        let fourth = columns.iter().find(|s| s.index == 3).unwrap();

        app.column_header_pressed(first.pos + first.size / 2.0);
        app.column_header_dragged(fourth.pos + fourth.size / 2.0);
        assert_eq!(app.selection().to_a1(), "A1:D1048576");
    }

    #[test]
    fn a_press_near_a_row_edge_starts_a_resize() {
        let mut app = app();
        let rows = app.visible_rows();
        let second = rows.iter().find(|s| s.index == 1).unwrap();
        let edge = second.pos + second.size;

        assert_eq!(app.row_edge_near(edge), Some(1));
        app.row_header_pressed(edge);
        assert!(app.is_resizing());
    }

    #[test]
    fn a_press_away_from_a_row_edge_selects_the_row() {
        let mut app = app();
        let rows = app.visible_rows();
        let third = rows.iter().find(|s| s.index == 2).unwrap();
        app.row_header_pressed(third.pos + third.size / 2.0);
        assert!(!app.is_resizing());
        assert_eq!(app.selection().to_a1(), "A3:XFD3");
    }

    #[test]
    fn the_grab_zone_reaches_both_sides_of_an_edge() {
        let app = app();
        let columns = app.visible_columns();
        let second = columns.iter().find(|s| s.index == 1).unwrap();
        let edge = second.pos + second.size;
        let tolerance = App::GRAB_TOLERANCE;

        assert_eq!(app.column_edge_near(edge - tolerance + 0.5), Some(1));
        assert_eq!(app.column_edge_near(edge + tolerance - 0.5), Some(1));
        assert_eq!(app.column_edge_near(edge - tolerance * 3.0), None);
    }

    #[test]
    fn edges_are_found_correctly_after_scrolling() {
        // The header is translated as the sheet scrolls, so a hit test that
        // forgot the offset would drift by up to a column.
        let mut app = app();
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(0, 40)), "x");
        app.scroll_by(-app.default_col_width_px() * 3.5, 0.0);

        let columns = app.visible_columns();
        let span = &columns[1];
        let edge = span.pos + span.size + app.offset_x();
        assert_eq!(app.column_edge_near(edge), Some(span.index));
    }

    #[test]
    fn a_resize_with_no_drag_in_progress_does_nothing() {
        let mut app = app();
        let original = app.columns().size_of(0);
        app.drag_resize(500.0);
        assert_eq!(app.columns().size_of(0), original);
    }

    #[test]
    fn arrow_keys_move_the_active_cell() {
        let mut app = app();
        app.handle_key(Key::Down, false, false);
        app.handle_key(Key::Right, false, false);
        assert_eq!(app.active(), CellRef::new(1, 1));
        assert_eq!(app.name_box(), "B2");
    }

    #[test]
    fn the_active_cell_cannot_leave_the_grid() {
        let mut app = app();
        app.handle_key(Key::Up, false, false);
        app.handle_key(Key::Left, false, false);
        assert_eq!(app.active(), CellRef::new(0, 0));
    }

    #[test]
    fn shift_extends_the_selection_without_moving_the_anchor() {
        let mut app = app();
        app.handle_key(Key::Down, false, true);
        app.handle_key(Key::Right, false, true);
        assert_eq!(app.active(), CellRef::new(0, 0));
        assert_eq!(app.selection().to_a1(), "A1:B2");
    }

    #[test]
    fn typing_starts_an_edit_holding_what_was_typed() {
        let mut app = app();
        app.handle_key(Key::Typed("7".to_string()), false, false);
        assert!(app.is_editing());
        assert_eq!(app.edit_text(), "7");
    }

    #[test]
    fn committing_writes_the_cell_and_moves_down() {
        let mut app = app();
        app.handle_key(Key::Typed("7".to_string()), false, false);
        app.handle_key(Key::Enter, false, false);
        assert!(!app.is_editing());
        assert_eq!(
            app.book.value(CellAddr::new(app.sheet, CellRef::new(0, 0))),
            Value::Number(7.0)
        );
        assert_eq!(app.active(), CellRef::new(1, 0));
    }

    #[test]
    fn escape_abandons_the_edit() {
        let mut app = app();
        app.handle_key(Key::Typed("7".to_string()), false, false);
        app.handle_key(Key::Escape, false, false);
        assert!(!app.is_editing());
        assert_eq!(
            app.book.value(CellAddr::new(app.sheet, CellRef::new(0, 0))),
            Value::Blank
        );
    }

    #[test]
    fn opening_an_existing_cell_keeps_its_formula() {
        let mut app = app();
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(0, 0)), "=1+1");
        app.handle_key(Key::Edit, false, false);
        assert_eq!(app.edit_text(), "=1+1");
    }

    #[test]
    fn delete_empties_the_selection() {
        let mut app = app();
        for row in 0..3 {
            app.book
                .set_input(CellAddr::new(app.sheet, CellRef::new(row, 0)), "5");
        }
        app.select(CellRef::new(0, 0), false);
        app.select(CellRef::new(2, 0), true);
        app.handle_key(Key::Delete, false, false);
        for row in 0..3 {
            assert_eq!(
                app.book
                    .value(CellAddr::new(app.sheet, CellRef::new(row, 0))),
                Value::Blank
            );
        }
    }

    #[test]
    fn moving_off_screen_scrolls_the_view() {
        let mut app = app();
        for _ in 0..60 {
            app.handle_key(Key::Down, false, false);
        }
        assert!(app.visible_rows()[0].index > 0);
        assert!(
            app.active_box().is_some(),
            "the active cell should be on screen"
        );
    }

    #[test]
    fn the_view_can_scroll_to_wherever_the_cursor_went() {
        let mut app = app();
        for _ in 0..200 {
            app.handle_key(Key::Down, false, false);
        }
        assert!(app.active_box().is_some());
    }

    #[test]
    fn the_active_box_disappears_when_it_scrolls_away() {
        let mut app = app();
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(900, 0)), "x");
        app.select(CellRef::new(0, 0), false);
        app.scroll_by(0.0, -app.default_row_height_px() * 300.0);
        assert!(app.active_box().is_none());
    }

    #[test]
    fn a_tall_row_still_scrolls_into_view() {
        let mut app = app();
        let sheet = app.sheet;
        app.book
            .sheet_mut(sheet)
            .unwrap()
            .rows_mut()
            .set_size(40, Some(400.0));
        app.select(CellRef::new(40, 0), false);
        assert!(app.active_box().is_some());
    }

    #[test]
    fn the_summary_totals_the_numbers_in_the_selection() {
        let mut app = app();
        for (row, value) in [(0u32, "10"), (1, "20"), (2, "text")] {
            app.book
                .set_input(CellAddr::new(app.sheet, CellRef::new(row, 0)), value);
        }
        app.select(CellRef::new(0, 0), false);
        app.select(CellRef::new(2, 0), true);
        let summary = app.selection_summary();
        assert!(summary.contains("Sum: 30"), "{summary}");
        assert!(summary.contains("Count: 3"), "{summary}");
        assert!(summary.contains("Average: 15"), "{summary}");
    }

    #[test]
    fn selecting_a_whole_column_does_not_sweep_a_million_cells() {
        let mut app = app();
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(0, 0)), "5");
        app.select_column(0);
        assert!(app.selection_summary().contains("Sum: 5"));
    }

    #[test]
    fn the_name_box_shows_the_size_of_a_block() {
        let mut app = app();
        app.select(CellRef::new(0, 0), false);
        app.select(CellRef::new(2, 1), true);
        assert_eq!(app.name_box(), "3 x 2");
    }

    #[test]
    fn a_circular_entry_is_reported_in_the_status_bar() {
        let mut app = app();
        app.handle_key(Key::Typed("=A1+1".to_string()), false, false);
        app.commit_edit();
        assert!(app.status().contains("Circular"), "{}", app.status());
    }

    #[test]
    fn the_theme_toggles_between_two() {
        let mut app = app();
        assert_eq!(app.theme, Theme::Light);
        app.toggle_theme();
        assert_eq!(app.theme, Theme::Dark);
        app.toggle_theme();
        assert_eq!(app.theme, Theme::Light);
    }

    #[test]
    fn numbers_align_right_and_text_left() {
        let mut app = app();
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(0, 0)), "42");
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(1, 0)), "hello");
        app.book
            .set_input(CellAddr::new(app.sheet, CellRef::new(2, 0)), "=1/0");

        assert_eq!(
            app.cell_display(CellRef::new(0, 0)),
            ("42".into(), true, false)
        );
        assert_eq!(
            app.cell_display(CellRef::new(1, 0)),
            ("hello".into(), false, false)
        );
        assert_eq!(
            app.cell_display(CellRef::new(2, 0)),
            ("#DIV/0!".into(), false, true)
        );
    }
}
