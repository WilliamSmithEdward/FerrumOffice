//! Editing a workbook and watching the right things recompute.

use ferrum_core::{CalcError, CellAddr, SheetId, Value};
use ferrum_sheet::Workbook;

/// Address a cell on the first sheet.
fn a(book: &Workbook, spot: &str) -> CellAddr {
    CellAddr::new(
        book.first_sheet(),
        ferrum_core::A1Ref::parse(spot).unwrap().cell,
    )
}

fn on(sheet: SheetId, spot: &str) -> CellAddr {
    CellAddr::new(sheet, ferrum_core::A1Ref::parse(spot).unwrap().cell)
}

/// Type into a cell on the first sheet.
fn set(book: &mut Workbook, spot: &str, typed: &str) {
    let addr = a(book, spot);
    book.set_input(addr, typed);
}

fn value(book: &Workbook, spot: &str) -> Value {
    let addr = a(book, spot);
    book.value(addr)
}

fn number(book: &Workbook, spot: &str) -> f64 {
    match value(book, spot) {
        Value::Number(n) => n,
        other => panic!("{spot} holds {other:?}, wanted a number"),
    }
}

#[test]
fn a_new_workbook_has_one_empty_sheet() {
    let book = Workbook::new();
    assert_eq!(book.sheet_order().len(), 1);
    assert_eq!(book.sheet(book.first_sheet()).unwrap().name(), "Sheet1");
    assert_eq!(value(&book, "A1"), Value::Blank);
}

#[test]
fn a_literal_is_stored_and_read_back() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "42");
    set(&mut book, "A2", "hello");
    set(&mut book, "A3", "TRUE");
    assert_eq!(value(&book, "A1"), Value::Number(42.0));
    assert_eq!(value(&book, "A2"), Value::text("hello"));
    assert_eq!(value(&book, "A3"), Value::Logical(true));
}

#[test]
fn a_formula_computes_when_it_is_entered() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "A2", "20");
    set(&mut book, "B1", "=A1+A2");
    assert_eq!(number(&book, "B1"), 30.0);
}

#[test]
fn editing_a_precedent_recalculates_what_reads_it() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "B1", "=A1*2");
    assert_eq!(number(&book, "B1"), 20.0);

    set(&mut book, "A1", "50");
    assert_eq!(number(&book, "B1"), 100.0);
}

#[test]
fn a_chain_recalculates_in_order() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    set(&mut book, "B1", "=A1+1");
    set(&mut book, "C1", "=B1+1");
    set(&mut book, "D1", "=C1+1");
    assert_eq!(number(&book, "D1"), 4.0);

    // One edit at the head has to reach the tail, and each step must see the
    // updated value of the one before it rather than the stale one.
    set(&mut book, "A1", "10");
    assert_eq!(number(&book, "B1"), 11.0);
    assert_eq!(number(&book, "C1"), 12.0);
    assert_eq!(number(&book, "D1"), 13.0);
}

#[test]
fn a_chain_entered_backwards_still_settles() {
    let mut book = Workbook::new();
    // The dependents exist before the cells they read.
    set(&mut book, "D1", "=C1+1");
    set(&mut book, "C1", "=B1+1");
    set(&mut book, "B1", "=A1+1");
    set(&mut book, "A1", "10");
    assert_eq!(number(&book, "D1"), 13.0);
}

#[test]
fn a_diamond_evaluates_each_cell_once() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "2");
    set(&mut book, "B1", "=A1*3");
    set(&mut book, "B2", "=A1*5");
    set(&mut book, "C1", "=B1+B2");
    assert_eq!(number(&book, "C1"), 16.0);

    let addr = a(&book, "A1");
    let report = book.set_input(addr, "10");
    assert_eq!(number(&book, "C1"), 80.0);
    // B1, B2 and C1, and no cell twice.
    assert_eq!(report.evaluated, 3);
}

#[test]
fn a_range_dependency_notices_a_change_inside_it() {
    let mut book = Workbook::new();
    for row in 1..=5 {
        set(&mut book, &format!("A{row}"), &row.to_string());
    }
    set(&mut book, "C1", "=SUM(A1:A5)");
    assert_eq!(number(&book, "C1"), 15.0);

    set(&mut book, "A3", "30");
    assert_eq!(number(&book, "C1"), 42.0);
}

#[test]
fn a_change_outside_a_range_leaves_it_alone() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    set(&mut book, "C1", "=SUM(A1:A5)");
    let addr = a(&book, "Z99");
    let report = book.set_input(addr, "999");
    assert_eq!(report.evaluated, 0);
    assert_eq!(number(&book, "C1"), 1.0);
}

#[test]
fn a_whole_column_dependency_is_found() {
    let mut book = Workbook::new();
    set(&mut book, "C1", "=SUM(A:A)");
    set(&mut book, "A1", "5");
    assert_eq!(number(&book, "C1"), 5.0);

    // Far down the column, well past any tile the range could be bucketed in.
    let far = a(&book, "A500000");
    book.set_input(far, "7");
    assert_eq!(number(&book, "C1"), 12.0);
}

#[test]
fn clearing_a_cell_recalculates_what_read_it() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "B1", "=A1*2");
    assert_eq!(number(&book, "B1"), 20.0);

    let addr = a(&book, "A1");
    book.clear(addr);
    assert_eq!(value(&book, "A1"), Value::Blank);
    assert_eq!(number(&book, "B1"), 0.0);
}

#[test]
fn typing_nothing_clears_the_cell() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "A1", "");
    assert_eq!(value(&book, "A1"), Value::Blank);
}

#[test]
fn replacing_a_formula_drops_its_old_dependency() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    set(&mut book, "B1", "2");
    set(&mut book, "C1", "=A1");
    assert_eq!(number(&book, "C1"), 1.0);

    set(&mut book, "C1", "=B1");
    assert_eq!(number(&book, "C1"), 2.0);

    // C1 no longer reads A1, so changing A1 must not touch it.
    let addr = a(&book, "A1");
    let report = book.set_input(addr, "99");
    assert_eq!(report.evaluated, 0);
    assert_eq!(number(&book, "C1"), 2.0);
}

#[test]
fn the_report_lists_only_what_actually_changed() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "5");
    set(&mut book, "B1", "=A1*0");
    assert_eq!(number(&book, "B1"), 0.0);

    // B1 recalculates but lands on the same answer, so it is not a change a
    // view needs to repaint.
    let addr = a(&book, "A1");
    let report = book.set_input(addr, "7");
    assert_eq!(report.evaluated, 1);
    assert!(
        !report.changed.contains(&a(&book, "B1")),
        "B1 did not change value and should not be listed"
    );
    assert!(report.changed.contains(&addr));
}

#[test]
fn a_direct_self_reference_is_circular() {
    let mut book = Workbook::new();
    let addr = a(&book, "A1");
    let report = book.set_input(addr, "=A1+1");
    assert!(report.is_circular());
    assert_eq!(report.circular, vec![addr]);
    // A spreadsheet shows zero rather than an error.
    assert_eq!(value(&book, "A1"), Value::Number(0.0));
}

#[test]
fn a_cycle_through_several_cells_is_caught() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "=C1+1");
    set(&mut book, "B1", "=A1+1");
    let addr = a(&book, "C1");
    let report = book.set_input(addr, "=B1+1");

    assert!(report.is_circular());
    assert_eq!(report.circular.len(), 3);
    for spot in ["A1", "B1", "C1"] {
        assert!(
            report.circular.contains(&a(&book, spot)),
            "{spot} is in the cycle"
        );
    }
}

#[test]
fn breaking_a_cycle_lets_it_compute_again() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "=B1+1");
    set(&mut book, "B1", "=A1+1");
    assert!(book.recalculate_all().is_circular());

    set(&mut book, "A1", "10");
    let report = book.recalculate_all();
    assert!(!report.is_circular());
    assert_eq!(number(&book, "B1"), 11.0);
}

#[test]
fn a_cell_outside_the_cycle_still_computes() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "=B1");
    set(&mut book, "B1", "=A1");
    set(&mut book, "D1", "7");
    set(&mut book, "E1", "=D1*2");
    assert_eq!(number(&book, "E1"), 14.0);
}

#[test]
fn a_malformed_formula_keeps_the_text_and_reports_a_name_error() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "=1+");
    let addr = a(&book, "A1");
    assert_eq!(book.edit_text(addr), "=1+");
    assert!(matches!(book.value(addr), Value::Error(_)));
}

#[test]
fn editing_shows_the_formula_and_reading_shows_the_result() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "6");
    set(&mut book, "B1", "=A1*7");
    let b1 = a(&book, "B1");
    assert_eq!(book.edit_text(b1), "=A1*7");
    assert_eq!(book.value(b1), Value::Number(42.0));
}

#[test]
fn errors_travel_along_the_chain() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "0");
    set(&mut book, "B1", "=1/A1");
    set(&mut book, "C1", "=B1+1");
    assert_eq!(book.value(a(&book, "C1")), Value::Error(CalcError::Div0));

    set(&mut book, "A1", "2");
    assert_eq!(number(&book, "C1"), 1.5);
}

#[test]
fn sheets_can_be_added_renamed_and_referenced() {
    let mut book = Workbook::new();
    let data = book.add_sheet("Data").unwrap();
    book.set_input(on(data, "A1"), "99");
    set(&mut book, "A1", "=Data!A1+1");
    assert_eq!(number(&book, "A1"), 100.0);

    book.rename_sheet(data, "Figures").unwrap();
    // The formula still names the old sheet, so it stops resolving.
    book.recalculate_all();
    assert_eq!(value(&book, "A1"), Value::Error(CalcError::Ref));
}

#[test]
fn a_duplicate_sheet_name_is_refused() {
    let mut book = Workbook::new();
    book.add_sheet("Data").unwrap();
    assert_eq!(
        book.add_sheet("data").unwrap_err(),
        ferrum_sheet::SheetError::NameTaken
    );
    assert_eq!(
        book.add_sheet("  ").unwrap_err(),
        ferrum_sheet::SheetError::NameEmpty
    );
}

#[test]
fn a_cross_sheet_edit_recalculates_the_other_sheet() {
    let mut book = Workbook::new();
    let data = book.add_sheet("Data").unwrap();
    book.set_input(on(data, "A1"), "5");
    set(&mut book, "B1", "=Data!A1*2");
    assert_eq!(number(&book, "B1"), 10.0);

    book.set_input(on(data, "A1"), "6");
    assert_eq!(number(&book, "B1"), 12.0);
}

#[test]
fn removing_a_sheet_breaks_the_references_into_it() {
    let mut book = Workbook::new();
    let data = book.add_sheet("Data").unwrap();
    book.set_input(on(data, "A1"), "5");
    set(&mut book, "B1", "=Data!A1*2");
    assert_eq!(number(&book, "B1"), 10.0);

    book.remove_sheet(data).unwrap();
    assert_eq!(value(&book, "B1"), Value::Error(CalcError::Ref));
}

#[test]
fn the_last_sheet_cannot_be_removed() {
    let mut book = Workbook::new();
    assert_eq!(
        book.remove_sheet(book.first_sheet()).unwrap_err(),
        ferrum_sheet::SheetError::LastSheet
    );
}

#[test]
fn a_defined_name_is_usable_in_a_formula() {
    let mut book = Workbook::new();
    let sheet = book.first_sheet();
    set(&mut book, "A1", "10");
    set(&mut book, "A2", "20");
    book.define_range(
        "Sales",
        sheet,
        ferrum_core::RangeRef::new(
            ferrum_core::A1Ref::parse("A1").unwrap().cell,
            ferrum_core::A1Ref::parse("A2").unwrap().cell,
        ),
    );
    set(&mut book, "C1", "=SUM(Sales)");
    assert_eq!(number(&book, "C1"), 30.0);
}

#[test]
fn recalculating_everything_gives_the_same_answers() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "3");
    set(&mut book, "A2", "4");
    set(&mut book, "B1", "=A1*A2");
    set(&mut book, "B2", "=B1+A1");
    set(&mut book, "C1", "=SUM(A1:B2)");

    let before: Vec<Value> = ["A1", "A2", "B1", "B2", "C1"]
        .iter()
        .map(|spot| value(&book, spot))
        .collect();

    let report = book.recalculate_all();
    assert!(!report.is_circular());

    let after: Vec<Value> = ["A1", "A2", "B1", "B2", "C1"]
        .iter()
        .map(|spot| value(&book, spot))
        .collect();
    assert_eq!(before, after);
    // Nothing moved, so nothing needs repainting.
    assert!(report.changed.is_empty());
}

#[test]
fn a_deep_chain_does_not_overflow_the_stack() {
    // The ordering pass walks the graph iteratively for exactly this reason.
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    for row in 2..=5_000 {
        set(&mut book, &format!("A{row}"), &format!("=A{}+1", row - 1));
    }
    assert_eq!(number(&book, "A5000"), 5000.0);

    set(&mut book, "A1", "0");
    assert_eq!(number(&book, "A5000"), 4999.0);
}

#[test]
fn a_wide_fan_out_recalculates_every_reader() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "2");
    for row in 1..=500 {
        set(&mut book, &format!("B{row}"), "=A1*2");
    }
    let addr = a(&book, "A1");
    let report = book.set_input(addr, "3");
    assert_eq!(report.evaluated, 500);
    assert_eq!(number(&book, "B250"), 6.0);
}

#[test]
fn a_formula_referring_to_itself_through_a_range_is_circular() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    set(&mut book, "A2", "2");
    // The sum covers the cell holding it.
    let addr = a(&book, "A3");
    let report = book.set_input(addr, "=SUM(A1:A3)");
    assert!(report.is_circular());
    assert!(report.circular.contains(&addr));
}

// Undo and redo.

#[test]
fn undo_takes_back_an_edit_and_redo_puts_it_again() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "A1", "20");
    assert_eq!(number(&book, "A1"), 20.0);

    book.undo().unwrap();
    assert_eq!(number(&book, "A1"), 10.0);

    book.redo().unwrap();
    assert_eq!(number(&book, "A1"), 20.0);
}

#[test]
fn undoing_the_first_edit_empties_the_cell_again() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    book.undo().unwrap();
    assert_eq!(value(&book, "A1"), Value::Blank);
}

#[test]
fn undo_recalculates_what_depended_on_the_change() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "B1", "=A1*2");
    set(&mut book, "A1", "50");
    assert_eq!(number(&book, "B1"), 100.0);

    book.undo().unwrap();
    assert_eq!(number(&book, "A1"), 10.0);
    assert_eq!(
        number(&book, "B1"),
        20.0,
        "the dependent should have followed"
    );
}

#[test]
fn undo_restores_a_formula_rather_than_its_result() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "6");
    set(&mut book, "B1", "=A1*7");
    set(&mut book, "B1", "plain text");

    book.undo().unwrap();
    let b1 = a(&book, "B1");
    assert_eq!(book.edit_text(b1), "=A1*7");
    assert_eq!(number(&book, "B1"), 42.0);
}

#[test]
fn a_bracketed_gesture_undoes_in_one_step() {
    let mut book = Workbook::new();
    book.begin_change("fill");
    for row in 1..=20 {
        set(&mut book, &format!("A{row}"), &row.to_string());
    }
    book.end_change();

    assert_eq!(number(&book, "A20"), 20.0);
    book.undo().unwrap();
    for row in 1..=20 {
        assert_eq!(
            value(&book, &format!("A{row}")),
            Value::Blank,
            "row {row} should have gone back with the rest"
        );
    }
    assert!(!book.can_undo(), "one gesture, one step");
}

#[test]
fn a_resize_can_be_undone() {
    let mut book = Workbook::new();
    let sheet = book.first_sheet();
    let original = book.sheet(sheet).unwrap().columns().size_of(2);

    book.resize_column(sheet, 2, Some(150.0));
    assert_eq!(book.sheet(sheet).unwrap().columns().size_of(2), 150.0);

    book.undo().unwrap();
    assert_eq!(book.sheet(sheet).unwrap().columns().size_of(2), original);

    book.redo().unwrap();
    assert_eq!(book.sheet(sheet).unwrap().columns().size_of(2), 150.0);
}

#[test]
fn a_resize_drag_is_one_undo_step() {
    let mut book = Workbook::new();
    let sheet = book.first_sheet();
    let original = book.sheet(sheet).unwrap().columns().size_of(1);

    book.begin_change("resize column");
    for width in 50..200 {
        book.resize_column(sheet, 1, Some(f64::from(width)));
    }
    book.end_change();

    book.undo().unwrap();
    assert_eq!(book.sheet(sheet).unwrap().columns().size_of(1), original);
    assert!(!book.can_undo());
}

#[test]
fn undoing_past_the_beginning_is_refused_rather_than_wrong() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    assert!(book.undo().is_some());
    assert!(book.undo().is_none());
    assert_eq!(value(&book, "A1"), Value::Blank);
}

#[test]
fn a_new_edit_after_an_undo_discards_the_redo() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    set(&mut book, "A1", "2");
    book.undo().unwrap();
    assert!(book.can_redo());

    set(&mut book, "B1", "something else");
    assert!(!book.can_redo());
    assert_eq!(number(&book, "A1"), 1.0);
}

#[test]
fn the_recalculation_that_an_undo_causes_is_not_itself_undoable() {
    // B1 recomputes when A1 is taken back. That is the machine's doing, not
    // the user's, and must not become a step they have to undo twice.
    let mut book = Workbook::new();
    set(&mut book, "A1", "1");
    set(&mut book, "B1", "=A1+1");
    let before = book.can_undo();
    assert!(before);

    book.undo().unwrap();
    book.undo().unwrap();
    assert!(!book.can_undo(), "two edits, two undos, and no extra steps");
}

#[test]
fn undo_reports_which_cells_need_repainting() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "10");
    set(&mut book, "B1", "=A1*2");
    set(&mut book, "A1", "50");

    let report = book.undo().unwrap();
    assert!(report.changed.contains(&a(&book, "A1")));
    assert!(report.changed.contains(&a(&book, "B1")));
}

#[test]
fn a_cycle_created_and_then_undone_leaves_nothing_behind() {
    let mut book = Workbook::new();
    set(&mut book, "A1", "5");
    let addr = a(&book, "A1");
    let report = book.set_input(addr, "=A1+1");
    assert!(report.is_circular());

    let report = book.undo().unwrap();
    assert!(!report.is_circular());
    assert_eq!(number(&book, "A1"), 5.0);
}
