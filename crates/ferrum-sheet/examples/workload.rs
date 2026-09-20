//! A rough timing of the calculation engine on workbook-shaped work.
//!
//! Not a benchmark harness, and not a budget. It exists so that the numbers
//! in `docs/open-questions.md` come from a measurement rather than a guess,
//! and so that a change which makes recalculation ten times slower is noticed
//! the same day.
//!
//! Run it with optimisation, or the figures mean nothing:
//!
//! ```text
//! cargo run --release -p ferrum-sheet --example workload
//! ```

use std::time::Instant;

use ferrum_core::{CellAddr, CellRef, SheetId, Value};
use ferrum_sheet::Workbook;

fn addr(sheet: SheetId, row: u32, col: u32) -> CellAddr {
    CellAddr::new(sheet, CellRef::new(row, col))
}

fn report(what: &str, elapsed: std::time::Duration, items: usize) {
    let micros = elapsed.as_secs_f64() * 1e6;
    let each = if items > 0 {
        format!("{:.2} us each", micros / items as f64)
    } else {
        String::new()
    };
    println!(
        "{what:<44} {:>9.1} ms   {each}",
        elapsed.as_secs_f64() * 1e3
    );
}

fn main() {
    if cfg!(debug_assertions) {
        println!("NOTE: built without optimisation, so these figures are not meaningful.\n");
    }

    const ROWS: u32 = 20_000;

    let mut book = Workbook::new();
    let sheet = book.first_sheet();

    // Column A: literals. Column B: a formula per row reading its own row.
    // Column C: a running chain, which is the worst shape for ordering.
    let start = Instant::now();
    for row in 0..ROWS {
        book.set_input(addr(sheet, row, 0), &(row + 1).to_string());
    }
    report("write 20,000 literals", start.elapsed(), ROWS as usize);

    let start = Instant::now();
    for row in 0..ROWS {
        book.set_input(addr(sheet, row, 1), &format!("=A{}*2", row + 1));
    }
    report("write 20,000 row formulas", start.elapsed(), ROWS as usize);

    let start = Instant::now();
    book.set_input(addr(sheet, 0, 2), "=B1");
    for row in 1..ROWS {
        book.set_input(addr(sheet, row, 2), &format!("=C{}+B{}", row, row + 1));
    }
    report("write a 20,000 deep chain", start.elapsed(), ROWS as usize);

    // One edit at the head of the chain must reach the tail.
    let start = Instant::now();
    let edit = book.set_input(addr(sheet, 0, 0), "999");
    let elapsed = start.elapsed();
    report("edit the head of the chain", elapsed, edit.evaluated);
    println!("    {} formulas recalculated", edit.evaluated);

    let start = Instant::now();
    let all = book.recalculate_all();
    report("recalculate everything", start.elapsed(), all.evaluated);

    let total = book.sheet(sheet).map_or(0, ferrum_sheet::Sheet::populated);
    println!("    populated cells: {total}");

    whole_column_aggregate(ROWS);
}

/// An aggregate over a whole column, measured on its own.
///
/// This has to be a fresh workbook. Measuring it on the one above timed the
/// 20,000-deep chain instead, because editing a cell in column A cascades
/// into the chain and recalculates everything: a real cost, but not the one
/// the label claimed.
///
/// What is being checked is that `SUM(A:A)` costs the populated cells rather
/// than the 1,048,576 the reference addresses. If the clipping in the
/// evaluator ever regresses, this figure jumps by orders of magnitude.
fn whole_column_aggregate(rows: u32) {
    println!("\nwhole-column aggregate, on a sheet with nothing else on it");

    let mut book = Workbook::new();
    let sheet = book.first_sheet();
    for row in 0..rows {
        book.set_input(addr(sheet, row, 0), &(row + 1).to_string());
    }
    book.set_input(addr(sheet, 0, 2), "=SUM(A:A)");

    let start = Instant::now();
    const EDITS: usize = 100;
    for n in 0..EDITS {
        book.set_input(addr(sheet, 1, 0), &n.to_string());
    }
    report("100 edits under SUM(A:A)", start.elapsed(), EDITS);

    match book.value(addr(sheet, 0, 2)) {
        Value::Number(n) => println!("    SUM(A:A) = {n}"),
        other => println!("    SUM(A:A) = {other}"),
    }
}
