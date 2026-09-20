//! FerrumGrid.
//!
//! The window is a view over [`ferrum_grid_view::App`], which holds the
//! workbook and everything about what is on screen. Every interaction calls
//! one method on it and then redraws from the [`View`] it produces, so there
//! is one description of what is showing rather than two that can drift
//! apart.
//!
//! Nothing is decided here. This file is the wiring between the interface's
//! callbacks and the application's gestures, which is why the test harness
//! can drive the same gestures without a window.

// A spreadsheet is a window, not a console program.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::rc::Rc;

use ferrum_core::{A1Ref, CellAddr, SheetId};
use ferrum_grid_view::{App, View};
use ferrum_theme::metrics::{DEFAULT_FONT_SIZE_PT, FONT_STACK};
use ferrum_theme::palette::Rgb;
use slint::{ModelRc, VecModel};

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
        state.pointer_down(x, y, extend);
    });

    on!(on_pointer_move, |state, x, y| {
        state.pointer_move(x, y);
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
        state.formula_edited(text.to_string());
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
        state.add_sheet();
    });

    on!(on_tab_selected, |state, index| {
        state.select_tab(index);
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
            let used = held.borrow_mut().key_pressed(name.as_str(), ctrl, shift);
            if let Some(window) = weak.upgrade() {
                refresh(&window, &held.borrow());
            }
            used
        });
    }
}

/// Push the palette and the type into the interface.
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

    let geometry = window.global::<Metrics>();
    geometry.set_font_size(app.metrics().points_to_pixels(DEFAULT_FONT_SIZE_PT) as f32);
    // The first entry is what a spreadsheet uses when the machine has it.
    // Slint falls back on its own when it does not.
    geometry.set_font_family(FONT_STACK[0].into());
}

/// Copy the picture onto the window.
///
/// The view is taken apart field by field rather than with `..`, so adding
/// something to the picture that nothing draws will not compile.
fn refresh(window: &MainWindow, app: &App) {
    let View {
        columns,
        rows,
        cells,
        offset_x,
        offset_y,
        selection,
        active,
        active_box,
        horizontal_thumb,
        vertical_thumb,
        row_header_width,
        column_header_height,
        default_column_width,
        default_row_height,
        name_box,
        formula_text,
        status,
        selection_summary,
        editing,
        edit_text,
        can_undo,
        undo_hint,
        can_redo,
        redo_hint,
        tabs,
        dark,
    } = app.view();

    let boxes: Vec<CellBox> = cells
        .into_iter()
        .map(|cell| CellBox {
            row: cell.at.row as i32,
            col: cell.at.col as i32,
            x: cell.rect.x,
            y: cell.rect.y,
            w: cell.rect.width,
            h: cell.rect.height,
            text: cell.text.into(),
            align_right: cell.align_right,
            is_error: cell.is_error,
        })
        .collect();
    window.set_cells(ModelRc::new(VecModel::from(boxes)));

    window.set_columns(ModelRc::new(VecModel::from(headers(columns))));
    window.set_rows(ModelRc::new(VecModel::from(headers(rows))));
    window.set_offset_x(offset_x);
    window.set_offset_y(offset_y);

    window.set_sel_top(selection.start.row as i32);
    window.set_sel_left(selection.start.col as i32);
    window.set_sel_bottom(selection.end.row as i32);
    window.set_sel_right(selection.end.col as i32);

    window.set_active_row(active.row as i32);
    window.set_active_col(active.col as i32);
    match active_box {
        Some(rect) => {
            window.set_active_visible(true);
            window.set_active_x(rect.x);
            window.set_active_y(rect.y);
            window.set_active_w(rect.width);
            window.set_active_h(rect.height);
        }
        None => window.set_active_visible(false),
    }

    window.set_h_thumb_start(horizontal_thumb.start);
    window.set_h_thumb_size(horizontal_thumb.size);
    window.set_v_thumb_start(vertical_thumb.start);
    window.set_v_thumb_size(vertical_thumb.size);

    let geometry = window.global::<Metrics>();
    geometry.set_row_header_width(row_header_width);
    geometry.set_col_header_height(column_header_height);
    geometry.set_col_width(default_column_width);
    geometry.set_row_height(default_row_height);

    window.set_can_undo(can_undo);
    window.set_can_redo(can_redo);
    window.set_undo_hint(undo_hint.into());
    window.set_redo_hint(redo_hint.into());

    window.set_name_box(name_box.into());
    window.set_status_text(status.into());
    window.set_selection_summary(selection_summary.into());
    window.set_editing(editing);
    window.set_edit_text(edit_text.into());
    window.set_formula_text(formula_text.into());

    window.set_tabs(ModelRc::new(VecModel::from(
        tabs.into_iter()
            .map(|tab| SheetTab {
                name: tab.name.into(),
                active: tab.active,
            })
            .collect::<Vec<_>>(),
    )));

    window.set_dark(dark);
}

fn headers(from: Vec<ferrum_grid_view::Header>) -> Vec<HeaderBox> {
    from.into_iter()
        .map(|header| HeaderBox {
            index: header.index as i32,
            pos: header.pos,
            size: header.size,
            label: header.label.into(),
        })
        .collect()
}

/// A small table, so `--demo` opens on something to look at.
fn seed_demo(app: &mut App) {
    let sheet: SheetId = app.sheet;
    let write = |app: &mut App, spot: &str, text: &str| {
        let cell = A1Ref::parse(spot)
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
