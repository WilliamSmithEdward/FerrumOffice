//! FerrumGrid, driven the way a person drives it.
//!
//! Every test here performs gestures and then reads what would be on screen.
//! Nothing reaches past the gestures into the state, because a test that does
//! that stops proving that the gestures work.

use ferrum_grid_view::{Harness, Theme};

/// A small table, typed in rather than installed, so that getting it onto the
/// screen is itself part of what is being tested.
fn table() -> Harness {
    let mut grid = Harness::new();
    grid.enter("A1", "Region");
    grid.enter("B1", "Units");
    grid.enter("C1", "Price");
    grid.enter("A2", "North");
    grid.enter("B2", "120");
    grid.enter("C2", "4.5");
    grid.enter("A3", "South");
    grid.enter("B3", "85");
    grid.enter("C3", "4.5");
    grid.go_to("A1");
    grid
}

fn screen(lines: &[&str]) -> String {
    lines.join("\n")
}

#[test]
fn what_was_typed_is_what_is_on_screen() {
    let grid = table();
    assert_eq!(
        grid.screen_of("A1:C3"),
        screen(&[
            "    |    A    |    B    |    C    |",
            "  1 |<Region> |Units    |Price    |",
            "  2 |North    |      120|      4.5|",
            "  3 |South    |       85|      4.5|",
        ])
    );
}

#[test]
fn a_formula_shows_its_result_and_the_bar_shows_the_formula() {
    let mut grid = table();
    grid.enter("D2", "=B2*C2");

    assert_eq!(grid.text_at("D2"), "540");

    grid.go_to("D2");
    assert_eq!(grid.view().formula_text, "=B2*C2");
    assert_eq!(grid.view().name_box, "D2");
}

#[test]
fn a_formula_follows_what_it_depends_on() {
    let mut grid = table();
    grid.enter("D2", "=B2*C2");
    grid.enter("B2", "200");
    assert_eq!(grid.text_at("D2"), "900");
}

#[test]
fn numbers_sit_against_the_right_edge_and_text_against_the_left() {
    let grid = table();
    assert!(grid.is_right_aligned("B2"), "a number aligns right");
    assert!(!grid.is_right_aligned("A2"), "text aligns left");
}

#[test]
fn an_error_says_which_one_and_is_marked_for_the_theme_to_colour() {
    let mut grid = table();
    grid.enter("D1", "=1/0");
    assert_eq!(grid.text_at("D1"), "#DIV/0!");
    assert!(grid.cell("D1").is_error);
}

#[test]
fn typing_over_a_cell_replaces_it_and_f2_amends_it() {
    let mut grid = table();

    grid.go_to("A2");
    grid.type_text("W");
    assert_eq!(grid.view().edit_text, "W", "typing discards what was there");
    grid.press("escape");

    grid.press("f2");
    assert_eq!(grid.view().edit_text, "North", "F2 keeps it to be amended");
    grid.type_text("ern");
    grid.press("enter");
    assert_eq!(grid.text_at("A2"), "Northern");
}

#[test]
fn escape_leaves_the_cell_as_it_was() {
    let mut grid = table();
    grid.go_to("A2");
    grid.type_text("Southern");
    grid.press("escape");
    assert_eq!(grid.text_at("A2"), "North");
    assert!(!grid.view().editing);
}

#[test]
fn committing_moves_down_so_a_column_can_be_typed_without_the_pointer() {
    let mut grid = Harness::new();
    grid.go_to("A1");
    for value in ["1", "2", "3"] {
        grid.type_text(value);
        grid.press("enter");
    }
    assert_eq!(grid.active(), "A4");
    assert_eq!(grid.text_at("A3"), "3");
}

#[test]
fn the_formula_bar_writes_the_cell_that_is_selected() {
    let mut grid = table();
    grid.go_to("D3");
    grid.type_in_formula_bar("=B3*C3");
    grid.commit_formula_bar();
    assert_eq!(grid.text_at("D3"), "382.5");
}

#[test]
fn undo_puts_the_screen_back_exactly_as_it_was() {
    let mut grid = table();
    // Standing where the change is about to happen, because an undo returns
    // the cursor there too and the picture includes the cursor.
    grid.go_to("B2");
    let before = grid.screen_of("A1:C3");

    grid.type_text("999");
    grid.press("enter");
    assert_ne!(grid.screen_of("A1:C3"), before);

    grid.undo();
    assert_eq!(grid.screen_of("A1:C3"), before);

    grid.redo();
    assert_eq!(grid.text_at("B2"), "999");
}

#[test]
fn clearing_a_block_takes_one_undo() {
    let mut grid = table();
    grid.click("A2");
    let before = grid.screen_of("A1:C3");

    grid.shift_click("C3");
    grid.press("delete");
    assert_eq!(grid.text_at("B2"), "");

    grid.undo();
    assert_eq!(grid.screen_of("A1:C3"), before);
}

#[test]
fn clicking_and_sweeping_selects_a_block() {
    let mut grid = table();
    grid.click("A1");
    grid.drag_to("C3");
    assert_eq!(grid.selection(), "A1:C3");
    assert_eq!(grid.view().name_box, "3 x 3");
}

#[test]
fn the_status_bar_totals_the_numbers_in_the_selection() {
    let mut grid = table();
    grid.click("B2");
    grid.shift_click("B3");
    assert_eq!(
        grid.view().selection_summary,
        "Average: 102.5   Count: 2   Sum: 205"
    );
}

#[test]
fn clicking_a_column_letter_takes_the_whole_column() {
    let mut grid = table();
    grid.click_column_header(1);
    assert_eq!(grid.selection(), "B1:B1048576");

    grid.drag_column_header(1, 2);
    assert_eq!(grid.selection(), "B1:C1048576");
}

#[test]
fn dragging_a_column_edge_widens_it_and_moves_what_follows() {
    let mut grid = table();
    let before = grid.view().column(0).unwrap().size;
    let neighbour = grid.view().column(1).unwrap().pos;

    grid.drag_column_edge(0, 40.0);

    let after = grid.view().column(0).unwrap().size;
    assert_eq!(after, before + 40.0);
    assert_eq!(grid.view().column(1).unwrap().pos, neighbour + 40.0);
}

#[test]
fn the_columns_tile_the_window_with_no_gap_and_no_overlap() {
    let mut grid = table();
    // Awkward widths, because equal ones hide a rounding error.
    grid.drag_column_edge(0, 37.0);
    grid.drag_column_edge(2, -13.0);

    let view = grid.view();
    for pair in view.columns.windows(2) {
        assert_eq!(
            pair[0].end(),
            pair[1].pos,
            "column {} ends where column {} begins",
            pair[0].index,
            pair[1].index
        );
    }
    for pair in view.rows.windows(2) {
        assert_eq!(pair[0].end(), pair[1].pos);
    }
}

#[test]
fn a_column_dragged_past_its_neighbour_is_hidden_rather_than_left_as_a_sliver() {
    let mut grid = table();
    grid.drag_column_edge(1, -200.0);

    let view = grid.view();
    assert!(
        view.column(1).is_none(),
        "a hidden column is not drawn at all"
    );
    assert_eq!(
        view.column(0).unwrap().end(),
        view.column(2).unwrap().pos,
        "and leaves no gap where it was"
    );
}

#[test]
fn the_two_themes_put_every_pixel_in_the_same_place() {
    // A stated requirement: the dark theme changes colours and nothing else.
    let mut grid = table();
    grid.drag_column_edge(0, 23.0);
    grid.drag_row_edge(2, 11.0);
    grid.scroll_down(40.0);

    grid.set_theme(Theme::Light);
    let light = grid.view().geometry();

    grid.toggle_theme();
    assert!(grid.view().dark, "the toggle changed the theme");
    let dark = grid.view().geometry();

    assert_eq!(light, dark);
}

#[test]
fn scrolling_shows_later_rows_without_moving_the_selection() {
    let mut grid = table();
    grid.go_to("A1");
    assert_eq!(grid.view().rows[0].label, "1");

    grid.scroll_down(400.0);

    assert_ne!(grid.view().rows[0].label, "1", "the view moved");
    assert_eq!(grid.active(), "A1", "the selection did not");
    assert!(
        grid.view().active_box.is_none(),
        "and the cell outline is not drawn where the cell is not"
    );
}

#[test]
fn the_cursor_cannot_be_left_off_screen() {
    let mut grid = table();
    grid.go_to("A1");
    grid.scroll_down(4000.0);
    grid.press("down");
    assert!(
        grid.view().active_box.is_some(),
        "moving the cursor brings it back into view"
    );
}

#[test]
fn a_smaller_window_shows_less_of_the_sheet() {
    let mut grid = table();
    let wide = grid.view().columns.len();

    grid.resize_window(400.0, 300.0);
    let narrow = grid.view().columns.len();

    assert!(
        narrow < wide,
        "{narrow} columns fit in 400px against {wide} in 800px"
    );
    assert_eq!(grid.text_at("A1"), "Region", "and A1 is still drawn");
}

#[test]
fn a_new_sheet_is_empty_and_the_first_one_keeps_its_table() {
    let mut grid = table();
    grid.add_sheet();

    assert_eq!(grid.text_at("A1"), "");
    assert_eq!(
        grid.view().tabs.iter().filter(|tab| tab.active).count(),
        1,
        "exactly one tab is the current one"
    );
    assert!(grid.view().tabs[1].active);

    grid.click_tab(0);
    assert_eq!(grid.text_at("A1"), "Region");
}

#[test]
fn undo_says_what_it_would_take_back() {
    let mut grid = table();
    assert!(grid.view().can_undo);
    assert_eq!(grid.view().undo_hint, "Undo edit");

    grid.drag_column_edge(0, 20.0);
    assert_eq!(grid.view().undo_hint, "Undo resize column");
}

#[test]
fn a_circular_formula_is_reported_rather_than_looping() {
    let mut grid = Harness::new();
    grid.enter("A1", "=A2");
    grid.enter("A2", "=A1");
    assert!(
        grid.view().status.contains("Circular"),
        "the status bar said: {}",
        grid.view().status
    );
}

#[test]
fn a_fresh_grid_has_the_measured_default_sizes() {
    // Measured from the surface being matched, at 96 dpi and no zoom:
    // 48pt columns and 14.5pt rows, which round to these pixels. The numbers
    // are in docs/design/theme.md.
    let grid = Harness::new();
    let view = grid.view();

    assert_eq!(view.column(0).unwrap().size, 64.0);
    assert_eq!(view.row(0).unwrap().size, 19.0);
    assert_eq!(view.columns[0].pos, 0.0);
    assert_eq!(view.rows[0].pos, 0.0);
    assert_eq!(view.name_box, "A1");
    assert_eq!(view.status, "Ready");
    assert_eq!(grid.active(), "A1");
}
