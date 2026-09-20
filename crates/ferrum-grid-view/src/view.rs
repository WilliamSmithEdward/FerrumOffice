//! The picture the application draws.
//!
//! Everything on screen is described here as plain data, produced in one pass
//! by [`App::view`]. The interface copies it onto the window and sends
//! gestures back; it decides nothing. That is what lets
//! [`Harness`](crate::Harness) drive the whole application without a window
//! and still see what a person would see.

use ferrum_core::{CellRef, RangeRef, column_label};

use crate::app::{App, Span};

/// A rectangle in viewport pixels, relative to the first visible cell.
///
/// The drawn position is this plus the view's offset, which is how scrolling
/// part of a row moves the content without rebuilding it.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Where a scrollbar thumb sits and how big it is, both as fractions of the
/// bar.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Thumb {
    pub start: f32,
    pub size: f32,
}

/// One column letter or row number.
#[derive(Clone, PartialEq, Debug)]
pub struct Header {
    pub index: u32,
    pub pos: f32,
    pub size: f32,
    pub label: String,
}

impl Header {
    /// The far edge, which is where the resize handle sits.
    pub fn end(&self) -> f32 {
        self.pos + self.size
    }

    fn span(&self) -> Span {
        Span {
            index: self.index,
            pos: self.pos,
            size: self.size,
        }
    }
}

/// One cell, as it is drawn.
#[derive(Clone, PartialEq, Debug)]
pub struct Cell {
    pub at: CellRef,
    pub rect: Rect,
    /// The value, formatted. A formula shows its result, never its text.
    pub text: String,
    /// Numbers sit against the right edge, everything else against the left.
    pub align_right: bool,
    pub is_error: bool,
}

/// One sheet tab along the bottom.
#[derive(Clone, PartialEq, Debug)]
pub struct Tab {
    pub name: String,
    pub active: bool,
}

/// Everything the window shows at one moment.
#[derive(Clone, PartialEq, Debug)]
pub struct View {
    pub columns: Vec<Header>,
    pub rows: Vec<Header>,
    /// Row-major, one per visible cell.
    pub cells: Vec<Cell>,
    pub offset_x: f32,
    pub offset_y: f32,

    pub selection: RangeRef,
    pub active: CellRef,
    /// Absent when the active cell has been scrolled out of sight.
    pub active_box: Option<Rect>,

    pub horizontal_thumb: Thumb,
    pub vertical_thumb: Thumb,

    pub row_header_width: f32,
    pub column_header_height: f32,
    pub default_column_width: f32,
    pub default_row_height: f32,

    pub name_box: String,
    /// What the formula bar shows: the text being typed, or the active cell's.
    pub formula_text: String,
    pub status: String,
    pub selection_summary: String,

    pub editing: bool,
    pub edit_text: String,

    pub can_undo: bool,
    pub undo_hint: String,
    pub can_redo: bool,
    pub redo_hint: String,

    pub tabs: Vec<Tab>,
    pub dark: bool,
}

impl View {
    pub fn column(&self, index: u32) -> Option<&Header> {
        self.columns.iter().find(|header| header.index == index)
    }

    pub fn row(&self, index: u32) -> Option<&Header> {
        self.rows.iter().find(|header| header.index == index)
    }

    /// The cell drawn at a grid position, or `None` when it is off screen.
    pub fn cell(&self, at: CellRef) -> Option<&Cell> {
        self.cells.iter().find(|cell| cell.at == at)
    }

    /// Every number that decides where something is drawn.
    ///
    /// Two views with equal geometry put every pixel in the same place
    /// whatever their colours, which is how the requirement that the two
    /// themes align identically is checked rather than asserted.
    pub fn geometry(&self) -> Geometry {
        Geometry {
            columns: self.columns.iter().map(Header::span).collect(),
            rows: self.rows.iter().map(Header::span).collect(),
            cells: self.cells.iter().map(|cell| (cell.at, cell.rect)).collect(),
            offset: (self.offset_x, self.offset_y),
            active_box: self.active_box,
            row_header_width: self.row_header_width,
            column_header_height: self.column_header_height,
            default_column_width: self.default_column_width,
            default_row_height: self.default_row_height,
            thumbs: (self.horizontal_thumb, self.vertical_thumb),
        }
    }
}

/// The positional half of a [`View`], with nothing that carries a colour or a
/// word.
#[derive(Clone, PartialEq, Debug)]
pub struct Geometry {
    pub columns: Vec<Span>,
    pub rows: Vec<Span>,
    pub cells: Vec<(CellRef, Rect)>,
    pub offset: (f32, f32),
    pub active_box: Option<Rect>,
    pub row_header_width: f32,
    pub column_header_height: f32,
    pub default_column_width: f32,
    pub default_row_height: f32,
    pub thumbs: (Thumb, Thumb),
}

impl App {
    /// Describe everything on screen.
    ///
    /// One pass over the visible window. The application calls this after
    /// every gesture, so there is one description of what is showing rather
    /// than two that can drift apart.
    pub fn view(&self) -> View {
        let columns = self.visible_columns();
        let rows = self.visible_rows();

        let mut cells = Vec::with_capacity(columns.len() * rows.len());
        for row in &rows {
            for column in &columns {
                let at = CellRef::new(row.index, column.index);
                let (text, align_right, is_error) = self.cell_display(at);
                cells.push(Cell {
                    at,
                    rect: Rect {
                        x: column.pos,
                        y: row.pos,
                        width: column.size,
                        height: row.size,
                    },
                    text,
                    align_right,
                    is_error,
                });
            }
        }

        // The row gutter widens with the largest row number on screen, so the
        // digits never clip and the grid does not shift on every scroll.
        let widest_row = rows.last().map_or(1, |span| span.index + 1);
        let metrics = self.metrics();

        let (horizontal_start, horizontal_size) = self.thumb(true);
        let (vertical_start, vertical_size) = self.thumb(false);

        View {
            columns: columns
                .iter()
                .map(|span| Header {
                    index: span.index,
                    pos: span.pos,
                    size: span.size,
                    label: column_label(span.index),
                })
                .collect(),
            rows: rows
                .iter()
                .map(|span| Header {
                    index: span.index,
                    pos: span.pos,
                    size: span.size,
                    label: (span.index + 1).to_string(),
                })
                .collect(),
            cells,
            offset_x: self.offset_x(),
            offset_y: self.offset_y(),

            selection: self.selection(),
            active: self.active(),
            active_box: self.active_box().map(|(x, y, width, height)| Rect {
                x,
                y,
                width,
                height,
            }),

            horizontal_thumb: Thumb {
                start: horizontal_start,
                size: horizontal_size,
            },
            vertical_thumb: Thumb {
                start: vertical_start,
                size: vertical_size,
            },

            row_header_width: metrics.row_header_width_px(widest_row) as f32,
            column_header_height: metrics.column_header_height_px() as f32,
            default_column_width: self.default_col_width_px(),
            default_row_height: self.default_row_height_px(),

            name_box: self.name_box(),
            formula_text: if self.is_editing() {
                self.edit_text()
            } else {
                self.formula_text()
            },
            status: self.status(),
            selection_summary: self.selection_summary(),

            editing: self.is_editing(),
            edit_text: self.edit_text(),

            can_undo: self.book.can_undo(),
            undo_hint: self
                .book
                .undo_label()
                .map_or_else(String::new, |what| format!("Undo {what}")),
            can_redo: self.book.can_redo(),
            redo_hint: self
                .book
                .redo_label()
                .map_or_else(String::new, |what| format!("Redo {what}")),

            tabs: self
                .book
                .sheet_order()
                .iter()
                .filter_map(|id| self.book.sheet(*id).map(|sheet| (*id, sheet)))
                .map(|(id, sheet)| Tab {
                    name: sheet.name().to_string(),
                    active: id == self.sheet,
                })
                .collect(),
            dark: self.theme == ferrum_theme::Theme::Dark,
        }
    }
}
