//! Write a sample workbook, so the file can be opened in a real spreadsheet.
//!
//! The only way to know a file format is right is to hand the file to
//! something else and see whether it agrees.
//!
//! ```text
//! cargo run -p ferrum-xlsx --example save -- out.xlsx
//! ```

use ferrum_core::{A1Ref, CellAddr};
use ferrum_sheet::Workbook;
use ferrum_xlsx::{DosTime, write_workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "out.xlsx".into());

    let mut book = Workbook::new();
    let sheet = book.first_sheet();

    {
        let mut write = |spot: &str, text: &str| {
            let cell = A1Ref::parse(spot)
                .expect("the sample uses real addresses")
                .cell;
            book.set_input(CellAddr::new(sheet, cell), text);
        };

        write("A1", "Region");
        write("B1", "Units");
        write("C1", "Price");
        write("D1", "Revenue");
        for (row, region, units, price) in [
            (2, "North", "120", "4.5"),
            (3, "South", "85", "4.5"),
            (4, "East", "210", "3.95"),
            (5, "West", "64", "5.25"),
        ] {
            write(&format!("A{row}"), region);
            write(&format!("B{row}"), units);
            write(&format!("C{row}"), price);
            write(&format!("D{row}"), &format!("=B{row}*C{row}"));
        }

        write("A7", "Total");
        write("D7", "=SUM(D2:D5)");
        write("A8", "Best region");
        write("B8", "=INDEX(A2:A5,MATCH(MAX(D2:D5),D2:D5,0))");
        // The cases most likely to produce a file a reader rejects.
        write("A9", "Markup");
        write("B9", "a & b < c > d");
        write("A10", "An error");
        write("B10", "=1/0");
        write("A11", "A logical");
        write("B11", "=1>0");
        write("A12", "Unicode");
        write("B12", "naïve 日本語");
    }

    book.resize_column(sheet, 0, Some(72.0));
    book.resize_row(sheet, 0, Some(20.0));
    book.add_sheet("Second")?;

    let bytes = write_workbook(&book, DosTime::new(2026, 9, 20, 12, 0, 0))?;
    std::fs::write(&path, &bytes)?;
    println!("wrote {} bytes to {path}", bytes.len());
    Ok(())
}
