//! Writing a workbook out as a spreadsheet file.
//!
//! The package is the handful of parts a reader insists on:
//!
//! ```text
//! [Content_Types].xml        what each part is
//! _rels/.rels                the way in
//! xl/workbook.xml            the list of sheets
//! xl/_rels/workbook.xml.rels where each sheet lives
//! xl/styles.xml              the minimum a reader will accept
//! xl/worksheets/sheetN.xml   one per sheet
//! ```
//!
//! Text is written inline rather than through a shared-strings table. Both
//! are valid; inline is one part fewer, and the table is an optimisation for
//! files with a great deal of repeated text rather than a requirement.

use ferrum_core::defaults::{COLUMN_WIDTH_CHARS, COLUMN_WIDTH_PT};
use ferrum_core::{CellRef, Value};
use ferrum_sheet::{Cell, Input, Sheet, Workbook};

use crate::xml::XmlWriter;
use crate::zip::{DosTime, ZipError, ZipWriter};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CONTENT_TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

const SHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const WORKBOOK_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
const STYLES_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";

/// Turn a width in points into the character count a spreadsheet shows.
fn width_in_characters(points: f64) -> f64 {
    points * COLUMN_WIDTH_CHARS / COLUMN_WIDTH_PT
}

/// Padding, in character units, that the stored width carries on top of the
/// width a spreadsheet reports.
///
/// The format stores `characters + 5 pixels`, expressed in character units,
/// so a column asked for 12.09 characters is written as 12.7265625. The
/// constant depends on the width of a digit in the default font and was
/// measured against Excel with that font: see `docs/design/file-format.md`.
const WIDTH_PADDING: f64 = 0.634_765_625;

/// Turn a width in points into the number the file stores.
fn stored_width(points: f64) -> f64 {
    width_in_characters(points) + WIDTH_PADDING
}

/// Write a workbook as spreadsheet-file bytes.
///
/// `time` stamps every entry. Passing [`DosTime::EPOCH`] makes the output
/// reproducible, which is what the tests rely on.
pub fn write_workbook(book: &Workbook, time: DosTime) -> Result<Vec<u8>, ZipError> {
    let sheets: Vec<&Sheet> = book
        .sheet_order()
        .iter()
        .filter_map(|id| book.sheet(*id))
        .collect();

    let mut zip = ZipWriter::new().with_time(time);
    zip.add("[Content_Types].xml", &content_types(sheets.len()))?;
    zip.add("_rels/.rels", &root_relationships())?;
    zip.add("xl/workbook.xml", &workbook_part(&sheets))?;
    zip.add(
        "xl/_rels/workbook.xml.rels",
        &workbook_relationships(sheets.len()),
    )?;
    zip.add("xl/styles.xml", &styles_part())?;
    for (index, sheet) in sheets.iter().enumerate() {
        zip.add(
            &format!("xl/worksheets/sheet{}.xml", index + 1),
            &sheet_part(sheet, index == 0),
        )?;
    }
    zip.finish()
}

fn content_types(sheet_count: usize) -> Vec<u8> {
    let mut xml = XmlWriter::new();
    xml.start("Types", &[("xmlns", CONTENT_TYPES)]);
    xml.empty(
        "Default",
        &[
            ("Extension", "rels"),
            (
                "ContentType",
                "application/vnd.openxmlformats-package.relationships+xml",
            ),
        ],
    );
    xml.empty(
        "Default",
        &[("Extension", "xml"), ("ContentType", "application/xml")],
    );
    xml.empty(
        "Override",
        &[
            ("PartName", "/xl/workbook.xml"),
            ("ContentType", WORKBOOK_TYPE),
        ],
    );
    xml.empty(
        "Override",
        &[("PartName", "/xl/styles.xml"), ("ContentType", STYLES_TYPE)],
    );
    for index in 1..=sheet_count {
        xml.empty(
            "Override",
            &[
                ("PartName", &format!("/xl/worksheets/sheet{index}.xml")),
                ("ContentType", SHEET_TYPE),
            ],
        );
    }
    xml.end("Types");
    xml.into_bytes()
}

fn root_relationships() -> Vec<u8> {
    let mut xml = XmlWriter::new();
    xml.start("Relationships", &[("xmlns", PACKAGE_RELATIONSHIPS)]);
    xml.empty(
        "Relationship",
        &[
            ("Id", "rId1"),
            ("Type", &format!("{RELATIONSHIPS}/officeDocument")),
            ("Target", "xl/workbook.xml"),
        ],
    );
    xml.end("Relationships");
    xml.into_bytes()
}

fn workbook_part(sheets: &[&Sheet]) -> Vec<u8> {
    let mut xml = XmlWriter::new();
    xml.start("workbook", &[("xmlns", MAIN), ("xmlns:r", RELATIONSHIPS)]);
    xml.start("sheets", &[]);
    for (index, sheet) in sheets.iter().enumerate() {
        let number = index + 1;
        xml.empty(
            "sheet",
            &[
                ("name", sheet.name()),
                ("sheetId", &number.to_string()),
                ("r:id", &format!("rId{number}")),
            ],
        );
    }
    xml.end("sheets");
    xml.end("workbook");
    xml.into_bytes()
}

fn workbook_relationships(sheet_count: usize) -> Vec<u8> {
    let mut xml = XmlWriter::new();
    xml.start("Relationships", &[("xmlns", PACKAGE_RELATIONSHIPS)]);
    for index in 1..=sheet_count {
        xml.empty(
            "Relationship",
            &[
                ("Id", &format!("rId{index}")),
                ("Type", &format!("{RELATIONSHIPS}/worksheet")),
                ("Target", &format!("worksheets/sheet{index}.xml")),
            ],
        );
    }
    xml.empty(
        "Relationship",
        &[
            ("Id", &format!("rId{}", sheet_count + 1)),
            ("Type", &format!("{RELATIONSHIPS}/styles")),
            ("Target", "styles.xml"),
        ],
    );
    xml.end("Relationships");
    xml.into_bytes()
}

/// The smallest style table a reader will accept.
///
/// Nothing here is used yet; a reader refuses the file without it. The two
/// fills are not a mistake: the format reserves the first two slots and the
/// second must be the grey pattern.
fn styles_part() -> Vec<u8> {
    let mut xml = XmlWriter::new();
    xml.start("styleSheet", &[("xmlns", MAIN)]);

    xml.start("fonts", &[("count", "1")]);
    xml.start("font", &[]);
    xml.empty("sz", &[("val", "11")]);
    xml.empty("name", &[("val", "Aptos Narrow")]);
    xml.end("font");
    xml.end("fonts");

    xml.start("fills", &[("count", "2")]);
    xml.start("fill", &[]);
    xml.empty("patternFill", &[("patternType", "none")]);
    xml.end("fill");
    xml.start("fill", &[]);
    xml.empty("patternFill", &[("patternType", "gray125")]);
    xml.end("fill");
    xml.end("fills");

    xml.start("borders", &[("count", "1")]);
    xml.start("border", &[]);
    for edge in ["left", "right", "top", "bottom", "diagonal"] {
        xml.empty(edge, &[]);
    }
    xml.end("border");
    xml.end("borders");

    let base = [
        ("numFmtId", "0"),
        ("fontId", "0"),
        ("fillId", "0"),
        ("borderId", "0"),
    ];
    xml.start("cellStyleXfs", &[("count", "1")]);
    xml.empty("xf", &base);
    xml.end("cellStyleXfs");
    xml.start("cellXfs", &[("count", "1")]);
    xml.empty("xf", &[base[0], base[1], base[2], base[3], ("xfId", "0")]);
    xml.end("cellXfs");

    xml.end("styleSheet");
    xml.into_bytes()
}

fn sheet_part(sheet: &Sheet, is_first: bool) -> Vec<u8> {
    let mut xml = XmlWriter::new();
    xml.start("worksheet", &[("xmlns", MAIN)]);

    // The order of these is fixed by the schema: extent, then the sheet's
    // own defaults, then the exceptions, then the data.
    if let Some(used) = sheet.used_bounds() {
        xml.empty("dimension", &[("ref", &used.to_a1())]);
    }
    write_sheet_views(&mut xml, is_first);
    write_sheet_format(&mut xml, sheet);
    write_columns(&mut xml, sheet);
    write_rows(&mut xml, sheet);

    xml.end("worksheet");
    xml.into_bytes()
}

/// How the sheet is being looked at.
///
/// This element looks purely cosmetic and is not. Without it a reader scales
/// every custom row height by two thirds: a row written at 20 points comes
/// back as 13.4, at 30 points as 20, exactly. Measured against Excel, and
/// found only by handing it the file. See `docs/design/file-format.md`.
fn write_sheet_views(xml: &mut XmlWriter, is_first: bool) {
    xml.start("sheetViews", &[]);
    let mut attributes = vec![];
    if is_first {
        attributes.push(("tabSelected", "1"));
    }
    attributes.push(("workbookViewId", "0"));
    xml.empty("sheetView", &attributes);
    xml.end("sheetViews");
}

/// The sheet's own default row height and column width.
///
/// Omitting this is not harmless. Without it a reader falls back to the
/// schema's default row height rather than this sheet's, and then rescales
/// every custom height against it: a row written at 20 points came back as
/// 13.4. Measured against Excel, which always writes this element.
fn write_sheet_format(xml: &mut XmlWriter, sheet: &Sheet) {
    let height = format!("{}", sheet.rows().default_size());
    let width = format!("{:.4}", stored_width(sheet.columns().default_size()));
    xml.empty(
        "sheetFormatPr",
        &[
            ("defaultRowHeight", height.as_str()),
            ("defaultColWidth", width.as_str()),
        ],
    );
}

/// Only the columns that are not the default width need recording.
fn write_columns(xml: &mut XmlWriter, sheet: &Sheet) {
    let exceptions: Vec<(u32, f64)> = sheet.columns().exceptions().collect();
    if exceptions.is_empty() {
        return;
    }
    xml.start("cols", &[]);
    for (index, points) in exceptions {
        let number = (index + 1).to_string();
        let mut attributes = vec![
            ("min", number.as_str()),
            ("max", number.as_str()),
            ("customWidth", "1"),
        ];
        let width = format!("{:.4}", stored_width(points));
        if points <= 0.0 {
            attributes.push(("hidden", "1"));
        }
        attributes.insert(2, ("width", width.as_str()));
        xml.empty("col", &attributes);
    }
    xml.end("cols");
}

fn write_rows(xml: &mut XmlWriter, sheet: &Sheet) {
    xml.start("sheetData", &[]);

    // Gather the populated cells by row. A sheet stores them unordered, and
    // the format wants them in reading order.
    //
    // A sheet with no cells can still have rows worth writing, because a
    // height is worth keeping on its own.
    let mut by_row: Vec<(u32, Vec<(CellRef, &Cell)>)> = Vec::new();
    if let Some(used) = sheet.used_bounds() {
        let mut cells: Vec<(CellRef, &Cell)> = sheet.cells_in(used);
        cells.sort_by_key(|(at, _)| (at.row, at.col));
        for (at, cell) in cells {
            match by_row.last_mut() {
                Some((row, group)) if *row == at.row => group.push((at, cell)),
                _ => by_row.push((at.row, vec![(at, cell)])),
            }
        }
    }

    // A row can also need writing because its height was changed, even with
    // nothing in it.
    let mut sized_rows: Vec<(u32, f64)> = sheet.rows().exceptions().collect();
    sized_rows.sort_by_key(|(index, _)| *index);

    let mut sized = sized_rows.into_iter().peekable();
    for (row, cells) in by_row {
        // Any empty-but-resized rows that come before this one.
        while let Some((index, points)) = sized.peek().copied() {
            if index >= row {
                break;
            }
            sized.next();
            write_row(xml, index, Some(points), &[]);
        }
        let height = match sized.peek().copied() {
            Some((index, points)) if index == row => {
                sized.next();
                Some(points)
            }
            _ => None,
        };
        write_row(xml, row, height, &cells);
    }
    for (index, points) in sized {
        write_row(xml, index, Some(points), &[]);
    }

    xml.end("sheetData");
}

fn write_row(xml: &mut XmlWriter, row: u32, height: Option<f64>, cells: &[(CellRef, &Cell)]) {
    let number = (row + 1).to_string();
    let mut attributes = vec![("r", number.as_str())];
    let height_text;
    if let Some(points) = height {
        height_text = format!("{points:.4}");
        attributes.push(("ht", height_text.as_str()));
        attributes.push(("customHeight", "1"));
        if points <= 0.0 {
            attributes.push(("hidden", "1"));
        }
    }

    if cells.is_empty() {
        xml.empty("row", &attributes);
        return;
    }

    xml.start("row", &attributes);
    for (at, cell) in cells {
        write_cell(xml, *at, cell);
    }
    xml.end("row");
}

fn write_cell(xml: &mut XmlWriter, at: CellRef, cell: &Cell) {
    let reference = at.to_a1();
    let formula = match &cell.input {
        // The stored formula has no leading `=`.
        Input::Formula { text, .. } => Some(text.trim_start_matches('=').to_string()),
        // A formula that does not parse is still the author's text and is
        // kept, so that opening the file elsewhere shows what they wrote.
        Input::Malformed { text, .. } => Some(text.trim_start_matches('=').to_string()),
        Input::Literal(_) => None,
    };

    // A blank cell with no formula carries nothing worth writing.
    if formula.is_none() && matches!(cell.value, Value::Blank) {
        return;
    }

    let kind = match &cell.value {
        Value::Text(_) => Some("inlineStr"),
        Value::Logical(_) => Some("b"),
        Value::Error(_) => Some("e"),
        Value::Number(_) | Value::Blank => None,
    };

    let mut attributes = vec![("r", reference.as_str())];
    if let Some(kind) = kind {
        attributes.push(("t", kind));
    }
    xml.start("c", &attributes);

    if let Some(formula) = &formula {
        xml.text_element("f", &[], formula);
    }

    match &cell.value {
        Value::Blank => {}
        // The shortest form that reads back as the same double, so a
        // round trip is exact.
        Value::Number(n) => {
            xml.text_element("v", &[], &n.to_string());
        }
        Value::Logical(b) => {
            xml.text_element("v", &[], if *b { "1" } else { "0" });
        }
        Value::Error(e) => {
            xml.text_element("v", &[], e.as_str());
        }
        Value::Text(text) => {
            xml.start("is", &[]);
            // Leading and trailing spaces survive only when the part says so.
            let needs_space =
                text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace);
            let attributes: &[(&str, &str)] = if needs_space {
                &[("xml:space", "preserve")]
            } else {
                &[]
            };
            xml.text_element("t", attributes, text);
            xml.end("is");
        }
    }

    xml.end("c");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_core::CellAddr;

    /// The bytes of one part, found by walking the archive's local headers.
    fn part(archive: &[u8], wanted: &str) -> Option<String> {
        let mut at = 0usize;
        while at + 30 <= archive.len() {
            let signature = u32::from_le_bytes(archive[at..at + 4].try_into().ok()?);
            if signature != 0x0403_4B50 {
                break;
            }
            let size = u32::from_le_bytes(archive[at + 18..at + 22].try_into().ok()?) as usize;
            let name_len = u16::from_le_bytes(archive[at + 26..at + 28].try_into().ok()?) as usize;
            let extra = u16::from_le_bytes(archive[at + 28..at + 30].try_into().ok()?) as usize;
            let name = std::str::from_utf8(&archive[at + 30..at + 30 + name_len]).ok()?;
            let data_at = at + 30 + name_len + extra;
            if name == wanted {
                return String::from_utf8(archive[data_at..data_at + size].to_vec()).ok();
            }
            at = data_at + size;
        }
        None
    }

    fn book_with(cells: &[(&str, &str)]) -> Workbook {
        let mut book = Workbook::new();
        let sheet = book.first_sheet();
        for (spot, text) in cells {
            let cell = ferrum_core::A1Ref::parse(spot).unwrap().cell;
            book.set_input(CellAddr::new(sheet, cell), text);
        }
        book
    }

    fn write(book: &Workbook) -> Vec<u8> {
        write_workbook(book, DosTime::EPOCH).unwrap()
    }

    #[test]
    fn the_package_carries_the_parts_a_reader_needs() {
        let archive = write(&Workbook::new());
        for wanted in [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/styles.xml",
            "xl/worksheets/sheet1.xml",
        ] {
            assert!(part(&archive, wanted).is_some(), "{wanted} is missing");
        }
    }

    #[test]
    fn every_sheet_is_declared_once_in_each_place_it_must_be() {
        let mut book = Workbook::new();
        book.add_sheet("Data").unwrap();
        book.add_sheet("Notes").unwrap();
        let archive = write(&book);

        let workbook = part(&archive, "xl/workbook.xml").unwrap();
        for name in ["Sheet1", "Data", "Notes"] {
            assert!(workbook.contains(&format!("name=\"{name}\"")), "{name}");
        }

        let types = part(&archive, "[Content_Types].xml").unwrap();
        let rels = part(&archive, "xl/_rels/workbook.xml.rels").unwrap();
        for index in 1..=3 {
            assert!(types.contains(&format!("/xl/worksheets/sheet{index}.xml")));
            assert!(rels.contains(&format!("worksheets/sheet{index}.xml")));
            assert!(part(&archive, &format!("xl/worksheets/sheet{index}.xml")).is_some());
        }
    }

    #[test]
    fn each_kind_of_value_is_written_in_its_own_form() {
        let book = book_with(&[
            ("A1", "42.5"),
            ("A2", "hello"),
            ("A3", "TRUE"),
            ("A4", "=1/0"),
        ]);
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();

        assert!(sheet.contains(r#"<c r="A1"><v>42.5</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="A2" t="inlineStr"><is><t>hello</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="A3" t="b"><v>1</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="A4" t="e"><f>1/0</f><v>#DIV/0!</v></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn a_formula_is_written_with_its_cached_result_and_no_equals() {
        let book = book_with(&[("A1", "6"), ("B1", "=A1*7")]);
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="B1"><f>A1*7</f><v>42</v></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn markup_in_a_formula_is_escaped() {
        // The case that produces a file no reader will open, if missed.
        let book = book_with(&[("A1", r#"=IF(1<2,"a & b","c")"#)]);
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains("&lt;2"), "{sheet}");
        assert!(sheet.contains("&amp; b"), "{sheet}");
        assert!(!sheet.contains("<2,"), "the raw form must not appear");
    }

    #[test]
    fn cells_come_out_in_reading_order() {
        let book = book_with(&[("C3", "3"), ("A1", "1"), ("B2", "2"), ("A3", "0")]);
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        let order: Vec<usize> = ["A1", "B2", "A3", "C3"]
            .iter()
            .map(|spot| sheet.find(&format!("r=\"{spot}\"")).unwrap())
            .collect();
        assert!(
            order.windows(2).all(|pair| pair[0] < pair[1]),
            "cells should be sorted by row then column: {order:?}"
        );
    }

    #[test]
    fn empty_cells_are_left_out() {
        let book = book_with(&[("A1", "1"), ("C1", "3")]);
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        assert!(!sheet.contains(r#"r="B1""#), "B1 holds nothing");
    }

    #[test]
    fn a_resized_column_is_recorded_in_the_units_the_format_uses() {
        let mut book = Workbook::new();
        let id = book.first_sheet();
        // Twice the default width should be twice the default character count.
        book.resize_column(id, 2, Some(COLUMN_WIDTH_PT * 2.0));
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();

        assert!(sheet.contains(r#"<col min="3" max="3""#), "{sheet}");
        let expected = format!("{:.4}", COLUMN_WIDTH_CHARS * 2.0 + WIDTH_PADDING);
        assert!(
            sheet.contains(&expected),
            "expected width {expected}: {sheet}"
        );
    }

    #[test]
    fn a_resized_row_is_recorded_in_points() {
        let mut book = Workbook::new();
        let id = book.first_sheet();
        book.resize_row(id, 0, Some(31.5));
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"ht="31.5000""#), "{sheet}");
        assert!(sheet.contains(r#"customHeight="1""#), "{sheet}");
    }

    #[test]
    fn a_hidden_column_says_so() {
        let mut book = Workbook::new();
        let id = book.first_sheet();
        book.resize_column(id, 0, Some(0.0));
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"hidden="1""#), "{sheet}");
    }

    #[test]
    fn surrounding_spaces_in_text_are_preserved_explicitly() {
        let book = book_with(&[("A1", "  padded  ")]);
        let sheet = part(&write(&book), "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"xml:space="preserve""#), "{sheet}");
    }

    #[test]
    fn every_sheet_says_how_it_is_being_looked_at() {
        // Omitting this makes a reader rescale every row height by two
        // thirds, which is not a thing anybody would guess.
        let mut book = Workbook::new();
        book.add_sheet("Second").unwrap();
        let archive = write(&book);

        let first = part(&archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(first.contains("<sheetViews>"), "{first}");
        assert!(first.contains(r#"tabSelected="1""#), "{first}");

        let second = part(&archive, "xl/worksheets/sheet2.xml").unwrap();
        assert!(second.contains("<sheetViews>"), "{second}");
        assert!(
            !second.contains("tabSelected"),
            "only one sheet is the selected tab"
        );
    }

    #[test]
    fn an_empty_workbook_still_produces_a_valid_package() {
        let archive = write(&Workbook::new());
        let sheet = part(&archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains("<sheetData"), "{sheet}");
        assert!(sheet.contains("</worksheet>"), "{sheet}");
    }

    #[test]
    fn two_saves_of_one_workbook_are_byte_for_byte_equal() {
        let book = book_with(&[("A1", "1"), ("B2", "=A1+1")]);
        assert_eq!(write(&book), write(&book));
    }

    #[test]
    fn a_sheet_name_with_markup_in_it_is_escaped() {
        let mut book = Workbook::new();
        book.rename_sheet(book.first_sheet(), "P&L <2026>").unwrap();
        let workbook = part(&write(&book), "xl/workbook.xml").unwrap();
        assert!(workbook.contains("P&amp;L &lt;2026&gt;"), "{workbook}");
    }
}
