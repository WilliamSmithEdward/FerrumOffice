//! FerrumGrid.
//!
//! The window is a view over [`state::App`], which holds the workbook and
//! everything about what is on screen. Every interaction mutates that state
//! and then redraws from it, so there is one description of what is showing
//! rather than two that can drift apart.

// A spreadsheet is a window, not a console program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod state;

use std::cell::RefCell;
use std::rc::Rc;

use ferrum_core::{CellAddr, CellRef, SheetId, column_label};
use ferrum_theme::metrics::{DEFAULT_FONT_SIZE_PT, FONT_STACK};
use ferrum_theme::palette::Rgb;
use slint::{ModelRc, SharedString, VecModel};

use state::{App, Key};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    let app = Rc::new(RefCell::new(App::new()));

    let arguments: Vec<String> = std::env::args().collect();
    if arguments.iter().any(|arg| arg == "--demo") {
        seed_demo(&mut app.borrow_mut());
    }
    if arguments.iter().any(|arg| arg == "--dark") {
        app.borrow_mut().theme = ferrum_theme::Theme::Dark;
    }

    apply_theme(&window, &app.borrow());
    connect(&window, &app);
    refresh(&window, &app.borrow());

    window.run()
}

/// Wire every callback to the state and a redraw.
fn connect(window: &MainWindow, app: &Rc<RefCell<App>>) {
    /// Mutate the state, then redraw from it.
    ///
    /// The two borrows are separate statements on purpose: the mutable one
    /// has to be released before the redraw can read.
    macro_rules! on {
        ($setter:ident, |$state:ident $(, $arg:ident)* $(,)?| $body:block) => {{
            let weak = window.as_weak();
            let held = Rc::clone(app);
            window.$setter(move |$($arg),*| {
                {
                    let mut $state = held.borrow_mut();
                    $body
                }
                if let Some(window) = weak.upgrade() {
                    refresh(&window, &held.borrow());
                }
            });
        }};
    }

    on!(on_scrolled, |state, dx, dy| {
        state.scroll_by(dx, dy);
    });

    on!(on_scroll_to_fraction, |state, horizontal, fraction| {
        state.scroll_to_fraction(horizontal, fraction);
    });

    on!(on_pointer_down, |state, x, y, extend| {
        if state.is_resizing() {
            state.end_resize();
        }
        if state.is_editing() {
            state.commit_edit();
        }
        let cell = state.cell_at(x, y);
        state.select(cell, extend);
    });

    on!(on_pointer_move, |state, x, y| {
        let cell = state.cell_at(x, y);
        state.extend_to(cell);
    });

    on!(on_select_all, |state| {
        state.select_all();
    });

    on!(on_edit_changed, |state, text| {
        state.set_edit_text(text.to_string());
    });

    on!(on_edit_committed, |state| {
        state.commit_edit();
    });

    on!(on_formula_edited, |state, text| {
        if !state.is_editing() {
            state.begin_edit(Some(String::new()));
        }
        state.set_edit_text(text.to_string());
    });

    on!(on_formula_committed, |state| {
        state.commit_edit();
    });

    on!(on_undo, |state| {
        state.undo();
    });

    on!(on_redo, |state| {
        state.redo();
    });

    on!(on_add_sheet, |state| {
        let name = next_sheet_name(&state.book);
        if let Ok(id) = state.book.add_sheet(&name) {
            state.sheet = id;
            state.select(CellRef::new(0, 0), false);
        }
    });

    on!(on_tab_selected, |state, index| {
        if let Some(id) = state.book.sheet_order().get(index.max(0) as usize).copied() {
            state.sheet = id;
            state.select(CellRef::new(0, 0), false);
        }
    });

    on!(on_column_header_pressed, |state, x| {
        state.column_header_pressed(x);
    });

    on!(on_column_header_dragged, |state, x| {
        state.column_header_dragged(x);
    });

    on!(on_row_header_pressed, |state, y| {
        state.row_header_pressed(y);
    });

    on!(on_row_header_dragged, |state, y| {
        state.row_header_dragged(y);
    });

    on!(on_header_released, |state| {
        state.end_resize();
    });

    // These only answer a question, so they neither mutate nor redraw. A
    // hover moving across the header must not rebuild the sheet.
    {
        let held = Rc::clone(app);
        window.on_column_edge_near(move |x| held.borrow().column_edge_near(x).is_some());
    }
    {
        let held = Rc::clone(app);
        window.on_row_edge_near(move |y| held.borrow().row_edge_near(y).is_some());
    }

    // How much room the cells have is the interface's answer, not ours, and
    // it changes whenever the window is resized.
    {
        let weak = window.as_weak();
        let held = Rc::clone(app);
        window.on_viewport_changed(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            held.borrow_mut()
                .set_viewport(window.get_viewport_w(), window.get_viewport_h());
            refresh(&window, &held.borrow());
        });
    }

    // The theme has to be reapplied, not just redrawn, so it gets its own
    // handler rather than the macro's.
    {
        let weak = window.as_weak();
        let held = Rc::clone(app);
        window.on_toggle_theme(move || {
            held.borrow_mut().toggle_theme();
            if let Some(window) = weak.upgrade() {
                apply_theme(&window, &held.borrow());
                refresh(&window, &held.borrow());
            }
        });
    }

    // Keys return whether they were used, so an unused one falls through to
    // whatever else wants it.
    {
        let weak = window.as_weak();
        let held = Rc::clone(app);
        window.on_key_pressed(move |name, ctrl, shift| {
            let Some(key) = key_from(name.as_str()) else {
                return false;
            };
            let used = held.borrow_mut().handle_key(key, ctrl, shift);
            if let Some(window) = weak.upgrade() {
                refresh(&window, &held.borrow());
            }
            used
        });
    }
}

/// Map a key name from the interface onto something the state understands.
fn key_from(name: &str) -> Option<Key> {
    Some(match name {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "home" => Key::Home,
        "end" => Key::End,
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "escape" => Key::Escape,
        "delete" => Key::Delete,
        "backspace" => Key::Backspace,
        "edit" => Key::Edit,
        // Anything else is a keystroke only if it is something a person typed.
        typed => {
            if typed.is_empty() || typed.chars().any(char::is_control) {
                return None;
            }
            Key::Typed(typed.to_string())
        }
    })
}

/// Push the palette and the metrics into the interface.
fn apply_theme(window: &MainWindow, app: &App) {
    fn colour(c: Rgb) -> slint::Color {
        slint::Color::from_rgb_u8(c.r, c.g, c.b)
    }

    let palette = app.theme.palette();
    let target = window.global::<Palette>();
    target.set_canvas(colour(palette.canvas));
    target.set_surface(colour(palette.surface));
    target.set_chrome(colour(palette.chrome));
    target.set_header(colour(palette.header));
    target.set_elevated(colour(palette.elevated));
    target.set_selection(colour(palette.selection));
    target.set_text_primary(colour(palette.text_primary));
    target.set_text_secondary(colour(palette.text_secondary));
    target.set_text_muted(colour(palette.text_muted));
    target.set_danger(colour(palette.danger));
    target.set_accent(colour(palette.accent));
    target.set_brand(colour(palette.brand));
    target.set_focus_ring(colour(palette.focus_ring));
    target.set_border_strong(colour(palette.border_strong));
    target.set_gridline(colour(palette.gridline));
    target.set_divider(colour(palette.divider));

    let metrics = app.metrics();
    let geometry = window.global::<Metrics>();
    // The row and column sizes belong to the sheet and are pushed by the
    // redraw, which knows which rows are on screen.
    geometry.set_col_header_height(metrics.column_header_height_px() as f32);
    geometry.set_font_size(metrics.points_to_pixels(DEFAULT_FONT_SIZE_PT) as f32);
    // The first entry is what a spreadsheet uses when the machine has it.
    // Slint falls back on its own when it does not.
    geometry.set_font_family(FONT_STACK[0].into());

    window.set_dark(app.theme == ferrum_theme::Theme::Dark);
}

/// Rebuild everything the window draws from the state.
fn refresh(window: &MainWindow, app: &App) {
    let columns = app.visible_columns();
    let rows = app.visible_rows();

    let mut cells = Vec::with_capacity(columns.len() * rows.len());
    for row in &rows {
        for column in &columns {
            let (text, align_right, is_error) =
                app.cell_display(CellRef::new(row.index, column.index));
            cells.push(CellBox {
                row: row.index as i32,
                col: column.index as i32,
                x: column.pos,
                y: row.pos,
                w: column.size,
                h: row.size,
                text: text.into(),
                align_right,
                is_error,
            });
        }
    }
    window.set_cells(ModelRc::new(VecModel::from(cells)));

    let column_headers: Vec<HeaderBox> = columns
        .iter()
        .map(|span| HeaderBox {
            index: span.index as i32,
            pos: span.pos,
            size: span.size,
            label: column_label(span.index).into(),
        })
        .collect();
    window.set_columns(ModelRc::new(VecModel::from(column_headers)));

    let row_headers: Vec<HeaderBox> = rows
        .iter()
        .map(|span| HeaderBox {
            index: span.index as i32,
            pos: span.pos,
            size: span.size,
            label: (span.index + 1).to_string().into(),
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(row_headers)));

    window.set_offset_x(app.offset_x());
    window.set_offset_y(app.offset_y());

    let selection = app.selection();
    window.set_sel_top(selection.start.row as i32);
    window.set_sel_left(selection.start.col as i32);
    window.set_sel_bottom(selection.end.row as i32);
    window.set_sel_right(selection.end.col as i32);

    let active = app.active();
    window.set_active_row(active.row as i32);
    window.set_active_col(active.col as i32);
    match app.active_box() {
        Some((x, y, w, h)) => {
            window.set_active_visible(true);
            window.set_active_x(x);
            window.set_active_y(y);
            window.set_active_w(w);
            window.set_active_h(h);
        }
        None => window.set_active_visible(false),
    }

    let (h_start, h_size) = app.thumb(true);
    let (v_start, v_size) = app.thumb(false);
    window.set_h_thumb_start(h_start);
    window.set_h_thumb_size(h_size);
    window.set_v_thumb_start(v_start);
    window.set_v_thumb_size(v_size);

    // The row gutter widens with the largest row number on screen, so the
    // digits never clip and the grid does not shift on every scroll.
    let widest_row = rows.last().map_or(1, |span| span.index + 1);
    window
        .global::<Metrics>()
        .set_row_header_width(app.metrics().row_header_width_px(widest_row) as f32);
    window
        .global::<Metrics>()
        .set_col_width(app.default_col_width_px());
    window
        .global::<Metrics>()
        .set_row_height(app.default_row_height_px());

    window.set_can_undo(app.book.can_undo());
    window.set_can_redo(app.book.can_redo());
    window.set_undo_hint(
        app.book
            .undo_label()
            .map_or_else(String::new, |what| format!("Undo {what}"))
            .into(),
    );
    window.set_redo_hint(
        app.book
            .redo_label()
            .map_or_else(String::new, |what| format!("Redo {what}"))
            .into(),
    );

    window.set_name_box(app.name_box().into());
    window.set_status_text(app.status().into());
    window.set_selection_summary(app.selection_summary().into());
    window.set_editing(app.is_editing());
    window.set_edit_text(app.edit_text().into());
    // While editing, the formula bar shows what is being typed.
    window.set_formula_text(if app.is_editing() {
        app.edit_text().into()
    } else {
        SharedString::from(app.formula_text())
    });

    let tabs: Vec<SheetTab> = app
        .book
        .sheet_order()
        .iter()
        .filter_map(|id| app.book.sheet(*id).map(|sheet| (*id, sheet)))
        .map(|(id, sheet)| SheetTab {
            name: sheet.name().into(),
            active: id == app.sheet,
        })
        .collect();
    window.set_tabs(ModelRc::new(VecModel::from(tabs)));
}

fn next_sheet_name(book: &ferrum_sheet::Workbook) -> String {
    for n in 1..1000 {
        let candidate = format!("Sheet{n}");
        if book.sheet_id(&candidate).is_none() {
            return candidate;
        }
    }
    "Sheet".to_string()
}

/// A small table, so `--demo` opens on something to look at.
fn seed_demo(app: &mut App) {
    let sheet: SheetId = app.sheet;
    let write = |app: &mut App, spot: &str, text: &str| {
        let cell = ferrum_core::A1Ref::parse(spot)
            .expect("the demo uses real addresses")
            .cell;
        app.book.set_input(CellAddr::new(sheet, cell), text);
    };

    write(app, "A1", "Region");
    write(app, "B1", "Units");
    write(app, "C1", "Price");
    write(app, "D1", "Revenue");

    let rows = [
        ("North", 120, "4.50"),
        ("South", 85, "4.50"),
        ("East", 210, "3.95"),
        ("West", 64, "5.25"),
        ("Central", 143, "4.10"),
    ];
    for (i, (region, units, price)) in rows.iter().enumerate() {
        let row = i + 2;
        write(app, &format!("A{row}"), region);
        write(app, &format!("B{row}"), &units.to_string());
        write(app, &format!("C{row}"), price);
        write(app, &format!("D{row}"), &format!("=B{row}*C{row}"));
    }

    write(app, "A8", "Total");
    write(app, "B8", "=SUM(B2:B6)");
    write(app, "D8", "=SUM(D2:D6)");
    write(app, "A10", "Best region");
    write(app, "B10", "=INDEX(A2:A6,MATCH(MAX(D2:D6),D2:D6,0))");
    write(app, "A11", "Average price");
    write(app, "B11", "=ROUND(AVERAGE(C2:C6),2)");
    write(app, "A12", "Above average");
    write(app, "B12", "=COUNTIF(D2:D6,\">\"&AVERAGE(D2:D6))");
    write(app, "A14", "An error, on purpose");
    write(app, "B14", "=1/0");

    // Wide enough for the labels, and a demonstration that columns resize.
    if let Some(sheet) = app.book.sheet_mut(sheet) {
        sheet.columns_mut().set_size(0, Some(96.0));
        sheet.rows_mut().set_size(0, Some(20.0));
    }
}
