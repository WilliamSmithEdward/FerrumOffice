//! End-to-end tests: formula text in, value out, against a fixture workbook.
//!
//! These exercise the whole chain rather than any one stage, which is what
//! makes them worth having alongside the unit tests inside the crate.

use std::collections::HashMap;

use ferrum_calc::eval::{Ctx, Resolver};
use ferrum_calc::operand::Operand;
use ferrum_calc::parse;
use ferrum_core::{A1Ref, CalcError, CellAddr, CellRef, RangeRef, SheetId, Value};

/// A workbook that exists only for these tests.
#[derive(Default)]
struct Book {
    names: Vec<String>,
    cells: Vec<HashMap<CellRef, Value>>,
    defined: HashMap<String, Operand>,
}

impl Book {
    fn new() -> Self {
        Self::default()
    }

    /// Add a sheet. `cells` are written as they would be typed.
    fn sheet(mut self, name: &str, cells: &[(&str, Value)]) -> Self {
        let mut map = HashMap::new();
        for (address, value) in cells {
            map.insert(A1Ref::parse(address).unwrap().cell, value.clone());
        }
        self.names.push(name.to_string());
        self.cells.push(map);
        self
    }

    fn define(mut self, name: &str, target: Operand) -> Self {
        self.defined.insert(name.to_ascii_uppercase(), target);
        self
    }

    fn id_of(&self, name: &str) -> Option<SheetId> {
        self.names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| SheetId(i as u32))
    }
}

impl Resolver for Book {
    fn current_sheet(&self) -> SheetId {
        SheetId(0)
    }

    fn sheet_by_name(&self, name: &str) -> Option<SheetId> {
        self.id_of(name)
    }

    fn sheets_between(&self, first: &str, last: &str) -> Option<Vec<SheetId>> {
        let (a, b) = (self.id_of(first)?.0, self.id_of(last)?.0);
        let (lo, hi) = (a.min(b), a.max(b));
        Some((lo..=hi).map(SheetId).collect())
    }

    fn cell(&self, addr: CellAddr) -> Value {
        self.cells
            .get(addr.sheet.0 as usize)
            .and_then(|sheet| sheet.get(&addr.cell))
            .cloned()
            .unwrap_or(Value::Blank)
    }

    fn used_bounds(&self, sheet: SheetId) -> Option<RangeRef> {
        let cells = self.cells.get(sheet.0 as usize)?;
        let mut bounds: Option<RangeRef> = None;
        for cell in cells.keys() {
            let one = RangeRef::single(*cell);
            bounds = Some(match bounds {
                None => one,
                Some(acc) => acc.union_bounds(&one),
            });
        }
        bounds
    }

    fn defined_name(&self, _sheet: Option<SheetId>, name: &str) -> Option<Operand> {
        self.defined.get(&name.to_ascii_uppercase()).cloned()
    }
}

/// Evaluate a formula as though it sat in `origin` on the first sheet.
fn at(book: &Book, origin: &str, formula: &str) -> Value {
    let cell = A1Ref::parse(origin).unwrap().cell;
    let ctx = Ctx::new(book, CellAddr::new(SheetId(0), cell));
    let expr = parse(formula).unwrap_or_else(|e| panic!("{formula}: {e}"));
    ctx.eval_to_value(&expr)
}

/// The usual fixture: a little table with numbers, text and a gap.
fn fixture() -> Book {
    Book::new()
        .sheet(
            "Sheet1",
            &[
                ("A1", Value::Number(10.0)),
                ("A2", Value::Number(20.0)),
                ("A3", Value::Number(30.0)),
                ("A4", Value::Number(40.0)),
                ("B1", Value::text("north")),
                ("B2", Value::text("south")),
                ("B3", Value::text("north")),
                ("B4", Value::text("east")),
                ("C1", Value::Logical(true)),
                ("C2", Value::Error(CalcError::Div0)),
                ("D2", Value::Number(2.5)),
            ],
        )
        .sheet(
            "Data",
            &[("A1", Value::Number(7.0)), ("B1", Value::Number(8.0))],
        )
        .sheet("Extra", &[("A1", Value::Number(100.0))])
}

fn num(book: &Book, formula: &str) -> f64 {
    match at(book, "F1", formula) {
        Value::Number(n) => n,
        other => panic!("{formula} gave {other:?}, wanted a number"),
    }
}

fn text(book: &Book, formula: &str) -> String {
    match at(book, "F1", formula) {
        Value::Text(s) => s.to_string(),
        other => panic!("{formula} gave {other:?}, wanted text"),
    }
}

fn error(book: &Book, formula: &str) -> CalcError {
    match at(book, "F1", formula) {
        Value::Error(e) => e,
        other => panic!("{formula} gave {other:?}, wanted an error"),
    }
}

// Operators

#[test]
fn arithmetic_evaluates() {
    let book = fixture();
    assert_eq!(num(&book, "=1+2*3"), 7.0);
    assert_eq!(num(&book, "=(1+2)*3"), 9.0);
    assert_eq!(num(&book, "=10/4"), 2.5);
    assert_eq!(num(&book, "=2^10"), 1024.0);
    assert_eq!(num(&book, "=-2^2"), 4.0);
    assert_eq!(num(&book, "=2^3^2"), 64.0);
    assert_eq!(num(&book, "=50%"), 0.5);
    assert_eq!(num(&book, "=-2%"), -0.02);
}

#[test]
fn division_by_zero_is_an_error_not_an_infinity() {
    let book = fixture();
    assert_eq!(error(&book, "=1/0"), CalcError::Div0);
    assert_eq!(error(&book, "=0/0"), CalcError::Div0);
}

#[test]
fn exponentiation_has_the_spreadsheet_edge_cases() {
    let book = fixture();
    assert_eq!(error(&book, "=0^0"), CalcError::Num);
    assert_eq!(error(&book, "=0^-1"), CalcError::Div0);
    // A negative base under a fractional power has no real answer.
    assert_eq!(error(&book, "=(-8)^(1/3)"), CalcError::Num);
    assert_eq!(num(&book, "=(-8)^2"), 64.0);
}

#[test]
fn concatenation_coerces_both_sides() {
    let book = fixture();
    assert_eq!(text(&book, r#"="a"&"b""#), "ab");
    assert_eq!(text(&book, r#"="n="&1/4"#), "n=0.25");
    assert_eq!(text(&book, "=TRUE&\"\""), "TRUE");
    // A blank cell concatenates as nothing.
    assert_eq!(text(&book, r#"="x"&Z99"#), "x");
}

#[test]
fn comparison_sorts_across_kinds() {
    let book = fixture();
    assert_eq!(at(&book, "F1", "=1<2"), Value::Logical(true));
    assert_eq!(at(&book, "F1", r#"="a"="A""#), Value::Logical(true));
    // A number sorts below any text.
    assert_eq!(at(&book, "F1", r#"=1000<"a""#), Value::Logical(true));
    // A blank equals zero and equals empty text.
    assert_eq!(at(&book, "F1", "=Z99=0"), Value::Logical(true));
}

#[test]
fn an_error_travels_through_every_operator() {
    let book = fixture();
    assert_eq!(error(&book, "=C2+1"), CalcError::Div0);
    assert_eq!(error(&book, "=C2&\"x\""), CalcError::Div0);
    assert_eq!(error(&book, "=C2>1"), CalcError::Div0);
    assert_eq!(error(&book, "=-C2"), CalcError::Div0);
}

// References

#[test]
fn references_read_their_cells() {
    let book = fixture();
    assert_eq!(num(&book, "=A1"), 10.0);
    assert_eq!(num(&book, "=$A$1"), 10.0);
    assert_eq!(num(&book, "=A1+A2"), 30.0);
    assert_eq!(at(&book, "F1", "=Z99"), Value::Blank);
}

#[test]
fn sheet_qualified_references_reach_other_sheets() {
    let book = fixture();
    assert_eq!(num(&book, "=Data!A1"), 7.0);
    assert_eq!(num(&book, "=Data!A1+Data!B1"), 15.0);
    assert_eq!(error(&book, "=Nowhere!A1"), CalcError::Ref);
}

#[test]
fn a_three_dimensional_reference_covers_every_sheet_in_the_span() {
    let book = fixture();
    // Sheet1!A1 is 10, Data!A1 is 7, Extra!A1 is 100.
    assert_eq!(num(&book, "=SUM(Sheet1:Extra!A1)"), 117.0);
}

#[test]
fn intersection_takes_the_shared_cells() {
    let book = fixture();
    // Row 2 of columns A to D, intersected with the whole of column A.
    assert_eq!(num(&book, "=SUM(A2:D2 A1:A4)"), 20.0);
    // Ranges that share nothing.
    assert_eq!(error(&book, "=SUM(A1:A2 B3:B4)"), CalcError::Null);
}

#[test]
fn a_union_counts_both_areas_and_not_the_gap() {
    let book = fixture();
    // A1:A2 is 30, and C1 is TRUE which adds nothing to a sum.
    assert_eq!(num(&book, "=SUM((A1:A2,A4))"), 70.0);
}

#[test]
fn implicit_intersection_picks_the_aligned_cell() {
    let book = fixture();
    // A formula in row 3 reading a column picks row 3.
    assert_eq!(num(&book, "=A1:A4"), 10.0);
    let cell = A1Ref::parse("F3").unwrap().cell;
    let ctx = Ctx::new(&book, CellAddr::new(SheetId(0), cell));
    let expr = parse("=A1:A4").unwrap();
    assert_eq!(ctx.eval_to_value(&expr), Value::Number(30.0));
}

#[test]
fn a_range_that_does_not_line_up_cannot_collapse() {
    let book = fixture();
    // Row 1 of A to D, read from F1, has no column F.
    assert_eq!(error(&book, "=A1:D1"), CalcError::Value);
}

#[test]
fn a_defined_name_resolves() {
    let book = fixture().define("Rate", Operand::number(0.2));
    assert_eq!(num(&book, "=Rate*100"), 20.0);
    assert_eq!(error(&book, "=Missing"), CalcError::Name);
}

// Aggregates

#[test]
fn sum_skips_text_in_a_range_but_coerces_a_direct_argument() {
    let book = fixture();
    assert_eq!(num(&book, "=SUM(A1:A4)"), 100.0);
    // B1:B4 is all text, which a range contributes nothing from.
    assert_eq!(num(&book, "=SUM(B1:B4)"), 0.0);
    // Written directly, the same kinds are coerced.
    assert_eq!(num(&book, r#"=SUM("2",TRUE)"#), 3.0);
    // And a logical inside a range still adds nothing.
    assert_eq!(num(&book, "=SUM(C1)"), 0.0);
}

#[test]
fn an_error_in_a_range_propagates_out_of_an_aggregate() {
    let book = fixture();
    assert_eq!(error(&book, "=SUM(C1:C2)"), CalcError::Div0);
}

#[test]
fn a_whole_column_reference_only_costs_the_data() {
    let book = fixture();
    // Correctness here; the point of clipping is that this does not walk a
    // million rows to find four numbers.
    assert_eq!(num(&book, "=SUM(A:A)"), 100.0);
    assert_eq!(num(&book, "=COUNT(A:A)"), 4.0);
}

#[test]
fn the_averages_and_extremes_work() {
    let book = fixture();
    assert_eq!(num(&book, "=AVERAGE(A1:A4)"), 25.0);
    assert_eq!(num(&book, "=MIN(A1:A4)"), 10.0);
    assert_eq!(num(&book, "=MAX(A1:A4)"), 40.0);
    assert_eq!(num(&book, "=MEDIAN(A1:A4)"), 25.0);
    assert_eq!(num(&book, "=MEDIAN(1,2,3)"), 2.0);
    assert_eq!(num(&book, "=PRODUCT(2,3,4)"), 24.0);
    assert_eq!(error(&book, "=AVERAGE(B1:B4)"), CalcError::Div0);
}

#[test]
fn counting_distinguishes_numbers_from_presence() {
    let book = fixture();
    assert_eq!(num(&book, "=COUNT(A1:B4)"), 4.0);
    assert_eq!(num(&book, "=COUNTA(A1:B4)"), 8.0);
    // A1:B4 is eight cells and all are filled.
    assert_eq!(num(&book, "=COUNTBLANK(A1:B4)"), 0.0);
    assert_eq!(num(&book, "=COUNTBLANK(A1:A6)"), 2.0);
}

#[test]
fn rank_selection_counts_from_one() {
    let book = fixture();
    assert_eq!(num(&book, "=LARGE(A1:A4,1)"), 40.0);
    assert_eq!(num(&book, "=LARGE(A1:A4,4)"), 10.0);
    assert_eq!(num(&book, "=SMALL(A1:A4,1)"), 10.0);
    assert_eq!(error(&book, "=SMALL(A1:A4,9)"), CalcError::Num);
}

#[test]
fn the_deviation_pair_differ_by_their_divisor() {
    let book = fixture();
    // Values 10, 20, 30, 40: population variance 125, sample variance 500/3.
    assert!((num(&book, "=VAR.P(A1:A4)") - 125.0).abs() < 1e-9);
    assert!((num(&book, "=VAR.S(A1:A4)") - 500.0 / 3.0).abs() < 1e-9);
    assert!((num(&book, "=STDEV.P(A1:A4)") - 125.0f64.sqrt()).abs() < 1e-9);
    assert_eq!(error(&book, "=VAR.S(A1)"), CalcError::Div0);
}

#[test]
fn sumproduct_multiplies_matching_positions() {
    let book = fixture();
    // 10*10 + 20*20 + 30*30 + 40*40
    assert_eq!(num(&book, "=SUMPRODUCT(A1:A4,A1:A4)"), 3000.0);
    assert_eq!(num(&book, "=SUMPRODUCT({1,2,3},{4,5,6})"), 32.0);
    // Shapes have to agree.
    assert_eq!(error(&book, "=SUMPRODUCT(A1:A4,A1:A2)"), CalcError::Value);
}

// Conditional aggregates

#[test]
fn the_conditional_aggregates_test_and_total() {
    let book = fixture();
    assert_eq!(num(&book, r#"=COUNTIF(B1:B4,"north")"#), 2.0);
    assert_eq!(num(&book, r#"=SUMIF(B1:B4,"north",A1:A4)"#), 40.0);
    assert_eq!(num(&book, r#"=AVERAGEIF(B1:B4,"north",A1:A4)"#), 20.0);
    assert_eq!(num(&book, "=SUMIF(A1:A4,\">15\")"), 90.0);
    assert_eq!(num(&book, "=COUNTIF(A1:A4,\">=30\")"), 2.0);
}

#[test]
fn criteria_take_wildcards() {
    let book = fixture();
    assert_eq!(num(&book, r#"=COUNTIF(B1:B4,"n*")"#), 2.0);
    assert_eq!(num(&book, r#"=COUNTIF(B1:B4,"?outh")"#), 1.0);
    assert_eq!(num(&book, r#"=COUNTIF(B1:B4,"<>north")"#), 2.0);
}

// Logic

#[test]
fn the_conditional_picks_one_branch() {
    let book = fixture();
    assert_eq!(num(&book, "=IF(TRUE,1,2)"), 1.0);
    assert_eq!(num(&book, "=IF(FALSE,1,2)"), 2.0);
    assert_eq!(num(&book, "=IF(A1>5,1,2)"), 1.0);
    assert_eq!(at(&book, "F1", "=IF(FALSE,1)"), Value::Logical(false));
}

#[test]
fn the_branch_not_taken_is_never_evaluated() {
    let book = fixture();
    // The dead branch divides by zero. A strict evaluator would report it.
    assert_eq!(num(&book, "=IF(TRUE,1,1/0)"), 1.0);
    assert_eq!(num(&book, "=IF(FALSE,1/0,2)"), 2.0);
}

#[test]
fn the_error_catchers_differ_in_what_they_catch() {
    let book = fixture();
    assert_eq!(num(&book, "=IFERROR(1/0,99)"), 99.0);
    assert_eq!(num(&book, "=IFERROR(5,99)"), 5.0);
    assert_eq!(num(&book, "=IFNA(NA(),99)"), 99.0);
    // IFNA lets a real fault through rather than hiding it.
    assert_eq!(error(&book, "=IFNA(1/0,99)"), CalcError::Div0);
}

#[test]
fn ifs_returns_the_first_match_and_complains_when_none_do() {
    let book = fixture();
    assert_eq!(num(&book, "=IFS(FALSE,1,TRUE,2)"), 2.0);
    assert_eq!(
        error(&book, "=IFS(FALSE,1,FALSE,2)"),
        CalcError::NotAvailable
    );
}

#[test]
fn the_logical_folds_skip_text_in_ranges() {
    let book = fixture();
    assert_eq!(at(&book, "F1", "=AND(TRUE,TRUE)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=AND(TRUE,FALSE)"), Value::Logical(false));
    assert_eq!(at(&book, "F1", "=OR(FALSE,TRUE)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=NOT(TRUE)"), Value::Logical(false));
    assert_eq!(at(&book, "F1", "=XOR(TRUE,TRUE)"), Value::Logical(false));
    assert_eq!(at(&book, "F1", "=XOR(TRUE,FALSE)"), Value::Logical(true));
    // C1 is TRUE and the text beside it is not a condition.
    assert_eq!(at(&book, "F1", "=AND(C1,B1:B2)"), Value::Logical(true));
    // Text written directly is not a condition at all.
    assert_eq!(error(&book, r#"=AND("x")"#), CalcError::Value);
}

#[test]
fn choose_evaluates_only_the_chosen_branch() {
    let book = fixture();
    assert_eq!(num(&book, "=CHOOSE(2,1/0,42,1/0)"), 42.0);
    assert_eq!(error(&book, "=CHOOSE(9,1,2)"), CalcError::Value);
}

// Maths

#[test]
fn rounding_survives_binary_representation() {
    let book = fixture();
    // The classic: 2.675 * 100 lands below the halfway point in binary.
    assert_eq!(num(&book, "=ROUND(2.675,2)"), 2.68);
    assert_eq!(num(&book, "=ROUND(2.5,0)"), 3.0);
    assert_eq!(num(&book, "=ROUND(-2.5,0)"), -3.0);
    assert_eq!(num(&book, "=ROUND(1234.5678,-2)"), 1200.0);
    assert_eq!(num(&book, "=ROUNDUP(1.001,2)"), 1.01);
    assert_eq!(num(&book, "=ROUNDDOWN(1.999,2)"), 1.99);
    assert_eq!(num(&book, "=TRUNC(-1.9)"), -1.0);
    assert_eq!(num(&book, "=INT(-1.9)"), -2.0);
}

#[test]
fn modulo_follows_the_divisor_sign() {
    let book = fixture();
    assert_eq!(num(&book, "=MOD(5,3)"), 2.0);
    assert_eq!(num(&book, "=MOD(-3,2)"), 1.0);
    assert_eq!(num(&book, "=MOD(3,-2)"), -1.0);
    assert_eq!(error(&book, "=MOD(1,0)"), CalcError::Div0);
}

#[test]
fn the_step_functions_move_away_from_zero() {
    let book = fixture();
    assert_eq!(num(&book, "=CEILING(4.2,1)"), 5.0);
    assert_eq!(num(&book, "=CEILING(4.2,0.5)"), 4.5);
    assert_eq!(num(&book, "=FLOOR(4.7,1)"), 4.0);
    assert_eq!(num(&book, "=EVEN(1.5)"), 2.0);
    assert_eq!(num(&book, "=EVEN(-1.5)"), -2.0);
    assert_eq!(num(&book, "=ODD(1.5)"), 3.0);
    assert_eq!(num(&book, "=ODD(2)"), 3.0);
    // Stepping away from zero towards a multiple of the other sign cannot work.
    assert_eq!(error(&book, "=CEILING(-4.2,1)"), CalcError::Num);
}

#[test]
fn the_domain_limited_functions_refuse_bad_input() {
    let book = fixture();
    assert_eq!(num(&book, "=SQRT(9)"), 3.0);
    assert_eq!(error(&book, "=SQRT(-1)"), CalcError::Num);
    assert_eq!(error(&book, "=LN(0)"), CalcError::Num);
    assert_eq!(error(&book, "=ASIN(2)"), CalcError::Num);
    assert_eq!(num(&book, "=LOG(8,2)"), 3.0);
    assert_eq!(num(&book, "=LOG10(1000)"), 3.0);
    assert_eq!(num(&book, "=ABS(-3)"), 3.0);
    assert_eq!(num(&book, "=SIGN(-3)"), -1.0);
}

#[test]
fn trigonometry_is_in_radians() {
    let book = fixture();
    assert!((num(&book, "=SIN(PI()/2)") - 1.0).abs() < 1e-12);
    assert!((num(&book, "=DEGREES(PI())") - 180.0).abs() < 1e-12);
    assert!((num(&book, "=RADIANS(180)") - std::f64::consts::PI).abs() < 1e-12);
}

// Text

#[test]
fn the_text_functions_count_characters_from_one() {
    let book = fixture();
    assert_eq!(text(&book, r#"=LEFT("spreadsheet",6)"#), "spread");
    assert_eq!(text(&book, r#"=RIGHT("spreadsheet",5)"#), "sheet");
    assert_eq!(text(&book, r#"=MID("spreadsheet",7,5)"#), "sheet");
    assert_eq!(num(&book, r#"=LEN("spreadsheet")"#), 11.0);
    assert_eq!(text(&book, r#"=LEFT("abc")"#), "a");
}

#[test]
fn text_positions_count_characters_not_bytes() {
    let book = fixture();
    // Four characters, more than four bytes.
    assert_eq!(num(&book, r#"=LEN("naïve")"#), 5.0);
    assert_eq!(text(&book, r#"=MID("naïve",3,1)"#), "ï");
}

#[test]
fn case_and_spacing_functions_behave() {
    let book = fixture();
    assert_eq!(text(&book, r#"=UPPER("aBc")"#), "ABC");
    assert_eq!(text(&book, r#"=LOWER("aBc")"#), "abc");
    assert_eq!(text(&book, r#"=PROPER("hello world")"#), "Hello World");
    assert_eq!(text(&book, r#"=TRIM("  a   b  ")"#), "a b");
    assert_eq!(text(&book, r#"=REPT("ab",3)"#), "ababab");
}

#[test]
fn searching_differs_in_case_sensitivity() {
    let book = fixture();
    assert_eq!(num(&book, r#"=FIND("s","spreadsheet")"#), 1.0);
    assert_eq!(num(&book, r#"=FIND("s","spreadsheet",2)"#), 7.0);
    assert_eq!(
        error(&book, r#"=FIND("S","spreadsheet")"#),
        CalcError::Value
    );
    // SEARCH ignores case and takes wildcards.
    assert_eq!(num(&book, r#"=SEARCH("S","spreadsheet")"#), 1.0);
    assert_eq!(num(&book, r#"=SEARCH("sh??t","spreadsheet")"#), 7.0);
}

#[test]
fn substitution_can_target_one_occurrence() {
    let book = fixture();
    assert_eq!(text(&book, r#"=SUBSTITUTE("a-b-c","-","+")"#), "a+b+c");
    assert_eq!(text(&book, r#"=SUBSTITUTE("a-b-c","-","+",2)"#), "a-b+c");
    assert_eq!(text(&book, r#"=REPLACE("abcdef",2,3,"XY")"#), "aXYef");
}

#[test]
fn joining_handles_ranges_and_empties() {
    let book = fixture();
    assert_eq!(text(&book, "=CONCAT(B1:B2)"), "northsouth");
    assert_eq!(text(&book, r#"=CONCATENATE("a","b")"#), "ab");
    assert_eq!(text(&book, r#"=TEXTJOIN("-",TRUE,"a","","b")"#), "a-b");
    assert_eq!(text(&book, r#"=TEXTJOIN("-",FALSE,"a","","b")"#), "a--b");
}

#[test]
fn exact_is_case_sensitive_where_the_operator_is_not() {
    let book = fixture();
    assert_eq!(at(&book, "F1", r#"=EXACT("a","A")"#), Value::Logical(false));
    assert_eq!(at(&book, "F1", r#"="a"="A""#), Value::Logical(true));
}

#[test]
fn characters_and_codes_round_trip() {
    let book = fixture();
    assert_eq!(text(&book, "=CHAR(65)"), "A");
    assert_eq!(num(&book, r#"=CODE("A")"#), 65.0);
    assert_eq!(num(&book, r#"=VALUE("3.5")"#), 3.5);
    assert_eq!(error(&book, r#"=VALUE("x")"#), CalcError::Value);
}

// Lookup

#[test]
fn the_position_functions_report_one_based_positions() {
    let book = fixture();
    assert_eq!(num(&book, "=ROW(A5)"), 5.0);
    assert_eq!(num(&book, "=COLUMN(C1)"), 3.0);
    assert_eq!(num(&book, "=ROWS(A1:A4)"), 4.0);
    assert_eq!(num(&book, "=COLUMNS(A1:D1)"), 4.0);
    // With no argument they answer for the formula's own cell.
    assert_eq!(num(&book, "=ROW()"), 1.0);
    assert_eq!(num(&book, "=COLUMN()"), 6.0);
}

#[test]
fn match_finds_a_position_three_ways() {
    let book = fixture();
    // Exact.
    assert_eq!(num(&book, r#"=MATCH("south",B1:B4,0)"#), 2.0);
    assert_eq!(num(&book, "=MATCH(30,A1:A4,0)"), 3.0);
    // Approximate over an ascending column: the last value at or below 25.
    assert_eq!(num(&book, "=MATCH(25,A1:A4,1)"), 2.0);
    assert_eq!(error(&book, "=MATCH(99,B1:B4,0)"), CalcError::NotAvailable);
}

#[test]
fn index_addresses_a_range() {
    let book = fixture();
    assert_eq!(num(&book, "=INDEX(A1:A4,3)"), 30.0);
    assert_eq!(text(&book, "=INDEX(A1:B4,2,2)"), "south");
    assert_eq!(error(&book, "=INDEX(A1:A4,9)"), CalcError::Ref);
}

#[test]
fn index_and_match_compose() {
    let book = fixture();
    assert_eq!(num(&book, r#"=INDEX(A1:A4,MATCH("east",B1:B4,0))"#), 40.0);
}

#[test]
fn vlookup_finds_across_a_table() {
    let book = Book::new().sheet(
        "Sheet1",
        &[
            ("A1", Value::Number(1.0)),
            ("B1", Value::text("one")),
            ("A2", Value::Number(5.0)),
            ("B2", Value::text("five")),
            ("A3", Value::Number(9.0)),
            ("B3", Value::text("nine")),
        ],
    );
    assert_eq!(text(&book, "=VLOOKUP(5,A1:B3,2,FALSE)"), "five");
    // Approximate takes the largest key at or below the target.
    assert_eq!(text(&book, "=VLOOKUP(7,A1:B3,2,TRUE)"), "five");
    assert_eq!(
        error(&book, "=VLOOKUP(7,A1:B3,2,FALSE)"),
        CalcError::NotAvailable
    );
    assert_eq!(error(&book, "=VLOOKUP(5,A1:B3,3,FALSE)"), CalcError::Ref);
}

#[test]
fn hlookup_searches_the_first_row() {
    let book = Book::new().sheet(
        "Sheet1",
        &[
            ("A1", Value::text("a")),
            ("B1", Value::text("b")),
            ("A2", Value::Number(1.0)),
            ("B2", Value::Number(2.0)),
        ],
    );
    assert_eq!(num(&book, r#"=HLOOKUP("b",A1:B2,2,FALSE)"#), 2.0);
}

// Information

#[test]
fn the_type_tests_see_an_error_rather_than_propagating_it() {
    let book = fixture();
    assert_eq!(at(&book, "F1", "=ISERROR(C2)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISERROR(A1)"), Value::Logical(false));
    assert_eq!(at(&book, "F1", "=ISERR(NA())"), Value::Logical(false));
    assert_eq!(at(&book, "F1", "=ISERR(1/0)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISNA(NA())"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISBLANK(Z99)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISBLANK(A1)"), Value::Logical(false));
    assert_eq!(at(&book, "F1", "=ISNUMBER(A1)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISTEXT(B1)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISLOGICAL(C1)"), Value::Logical(true));
    assert_eq!(at(&book, "F1", "=ISNONTEXT(A1)"), Value::Logical(true));
}

#[test]
fn type_and_error_type_report_ordinals() {
    let book = fixture();
    assert_eq!(num(&book, "=TYPE(A1)"), 1.0);
    assert_eq!(num(&book, "=TYPE(B1)"), 2.0);
    assert_eq!(num(&book, "=TYPE(C1)"), 4.0);
    assert_eq!(num(&book, "=TYPE(C2)"), 16.0);
    assert_eq!(num(&book, "=TYPE({1,2})"), 64.0);
    assert_eq!(num(&book, "=ERROR.TYPE(C2)"), 2.0);
    assert_eq!(error(&book, "=ERROR.TYPE(A1)"), CalcError::NotAvailable);
}

// Arrays

#[test]
fn array_literals_feed_aggregates() {
    let book = fixture();
    assert_eq!(num(&book, "=SUM({1,2;3,4})"), 10.0);
    assert_eq!(num(&book, "=ROWS({1,2;3,4})"), 2.0);
    assert_eq!(num(&book, "=COLUMNS({1,2;3,4})"), 2.0);
    assert_eq!(num(&book, "=INDEX({10,20,30},2)"), 20.0);
}

#[test]
fn arithmetic_broadcasts_across_an_array() {
    let book = fixture();
    // The multiplication happens element by element before the sum.
    assert_eq!(num(&book, "=SUM({1,2,3}*2)"), 12.0);
    assert_eq!(num(&book, "=SUM(A1:A4*2)"), 200.0);
    assert_eq!(num(&book, "=SUM({1,2}+{10,20})"), 33.0);
}

// Unknown names

#[test]
fn an_unknown_function_is_a_name_error() {
    let book = fixture();
    assert_eq!(error(&book, "=NOSUCHFUNCTION(1)"), CalcError::Name);
}

#[test]
fn the_wrong_number_of_arguments_is_rejected() {
    let book = fixture();
    assert_eq!(error(&book, "=ABS(1,2)"), CalcError::Value);
    assert_eq!(error(&book, "=PI(1)"), CalcError::Value);
}

#[test]
fn function_names_are_case_insensitive() {
    let book = fixture();
    assert_eq!(num(&book, "=sum(A1:A2)"), 30.0);
    assert_eq!(num(&book, "=SuM(A1:A2)"), 30.0);
}

// A realistic sheet

#[test]
fn a_realistic_formula_evaluates_end_to_end() {
    let book = fixture();
    let formula = r#"=IFERROR(ROUND(SUMIF(B1:B4,"north",A1:A4)/COUNTIF(B1:B4,"north"),1),"n/a")"#;
    assert_eq!(num(&book, formula), 20.0);
}

#[test]
fn a_formula_over_an_empty_sheet_does_not_panic() {
    let book = Book::new().sheet("Sheet1", &[]);
    assert_eq!(num(&book, "=SUM(A:A)"), 0.0);
    assert_eq!(num(&book, "=COUNT(A1:Z100)"), 0.0);
    assert_eq!(at(&book, "F1", "=A1"), Value::Blank);
    assert_eq!(error(&book, "=AVERAGE(A1:A9)"), CalcError::Div0);
}
