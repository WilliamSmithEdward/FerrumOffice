//! The value a cell can hold, and the conversions between the kinds.
//!
//! A spreadsheet is weakly typed: almost every operator accepts almost every
//! kind of value and coerces on the spot. The coercion rules live here rather
//! than in the evaluator so that the document model, the function library and
//! the user interface all agree on them.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

/// The error values a calculation can produce.
///
/// These are values, not Rust errors: a cell holding one is a perfectly valid
/// cell, and most operators propagate it to their result.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum CalcError {
    /// Two ranges were intersected and share no cells.
    Null,
    /// Division by zero.
    Div0,
    /// An operand had the wrong kind and could not be coerced.
    Value,
    /// A reference no longer points at anything.
    Ref,
    /// A function or defined name was not recognised.
    Name,
    /// The arithmetic is defined but the result is not representable.
    Num,
    /// A lookup found nothing, or a value is deliberately absent.
    NotAvailable,
    /// A dynamic array had no room to write its result.
    Spill,
    /// A calculation could not be carried out at all.
    Calc,
}

impl CalcError {
    /// The text a cell displays for this error.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Null => "#NULL!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::NotAvailable => "#N/A",
            Self::Spill => "#SPILL!",
            Self::Calc => "#CALC!",
        }
    }

    /// Recognise an error written out in a formula, such as `#DIV/0!`.
    pub fn from_display(text: &str) -> Option<Self> {
        const ALL: [CalcError; 9] = [
            CalcError::Null,
            CalcError::Div0,
            CalcError::Value,
            CalcError::Ref,
            CalcError::Name,
            CalcError::Num,
            CalcError::NotAvailable,
            CalcError::Spill,
            CalcError::Calc,
        ];
        ALL.into_iter()
            .find(|e| e.as_str().eq_ignore_ascii_case(text))
    }

    /// The ordinal `ERROR.TYPE` reports, or `None` for errors it does not cover.
    pub const fn error_type(self) -> Option<u8> {
        match self {
            Self::Null => Some(1),
            Self::Div0 => Some(2),
            Self::Value => Some(3),
            Self::Ref => Some(4),
            Self::Name => Some(5),
            Self::Num => Some(6),
            Self::NotAvailable => Some(7),
            Self::Spill => Some(9),
            Self::Calc => Some(14),
        }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single scalar value.
///
/// [`Value::Number`] never holds a NaN or an infinity. Construct numbers with
/// [`Value::number`], which maps a non-finite result to [`CalcError::Num`], so
/// that the rest of the engine can compare and format numbers without guarding
/// every use.
#[derive(Clone, Debug)]
pub enum Value {
    /// An empty cell, or an omitted argument.
    Blank,
    Number(f64),
    Text(Arc<str>),
    Logical(bool),
    Error(CalcError),
}

/// Significant decimal digits the engine keeps. IEEE-754 doubles carry a little
/// under 16, and spreadsheets have long settled on 15 so that arithmetic on
/// decimal input reads back the way it was typed.
pub const SIGNIFICANT_DIGITS: usize = 15;

/// Decimal exponent at or above which the general format switches to
/// scientific notation. At this point a plain rendering would need more digits
/// than the engine actually holds.
const SCIENTIFIC_UPPER_EXP: i32 = SIGNIFICANT_DIGITS as i32;

/// Decimal exponent at or below which the general format switches to
/// scientific notation.
///
/// Provisional: the upper bound follows from the digit budget, but the lower
/// one is a judgement call that has not yet been checked against a real
/// spreadsheet. See `docs/open-questions.md`.
const SCIENTIFIC_LOWER_EXP: i32 = -10;

impl Value {
    /// Build a number, mapping a non-finite result to `#NUM!`.
    pub fn number(n: f64) -> Self {
        if n.is_finite() {
            Self::Number(n)
        } else {
            Self::Error(CalcError::Num)
        }
    }

    /// Build a text value.
    pub fn text(s: impl Into<Arc<str>>) -> Self {
        Self::Text(s.into())
    }

    /// True when this value is an error, which most operators propagate.
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    pub const fn as_error(&self) -> Option<CalcError> {
        match self {
            Self::Error(e) => Some(*e),
            _ => None,
        }
    }

    /// Coerce to a number the way an arithmetic operator does.
    ///
    /// Blank counts as zero, logicals as one and zero, and text is parsed.
    /// Text that does not parse gives `#VALUE!`.
    pub fn to_number(&self) -> Result<f64, CalcError> {
        match self {
            Self::Blank => Ok(0.0),
            Self::Number(n) => Ok(*n),
            Self::Logical(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Self::Text(s) => parse_number(s).ok_or(CalcError::Value),
            Self::Error(e) => Err(*e),
        }
    }

    /// Coerce to text the way concatenation does.
    pub fn to_text(&self) -> Result<String, CalcError> {
        match self {
            Self::Blank => Ok(String::new()),
            Self::Number(n) => Ok(format_general(*n)),
            Self::Text(s) => Ok(s.to_string()),
            Self::Logical(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_string()),
            Self::Error(e) => Err(*e),
        }
    }

    /// Coerce to a logical the way a condition does.
    pub fn to_logical(&self) -> Result<bool, CalcError> {
        match self {
            Self::Blank => Ok(false),
            Self::Number(n) => Ok(*n != 0.0),
            Self::Logical(b) => Ok(*b),
            Self::Text(s) => {
                if s.eq_ignore_ascii_case("TRUE") {
                    Ok(true)
                } else if s.eq_ignore_ascii_case("FALSE") {
                    Ok(false)
                } else {
                    Err(CalcError::Value)
                }
            }
            Self::Error(e) => Err(*e),
        }
    }

    /// The text a cell shows when no explicit number format applies.
    pub fn display(&self) -> String {
        match self {
            Self::Error(e) => e.as_str().to_string(),
            // The other kinds all convert without failing.
            other => other.to_text().unwrap_or_default(),
        }
    }

    /// Rank used when values of different kinds are compared.
    ///
    /// Comparison operators sort numbers below text below logicals, rather
    /// than coercing one kind into the other.
    const fn kind_rank(&self) -> u8 {
        match self {
            Self::Blank | Self::Number(_) => 0,
            Self::Text(_) => 1,
            Self::Logical(_) => 2,
            Self::Error(_) => 3,
        }
    }

    /// Order two values the way a comparison operator does.
    ///
    /// An error on either side propagates. A blank takes the kind of whatever
    /// it is compared against: zero against a number, empty text against text,
    /// false against a logical.
    pub fn compare(&self, other: &Self) -> Result<Ordering, CalcError> {
        if let Self::Error(e) = self {
            return Err(*e);
        }
        if let Self::Error(e) = other {
            return Err(*e);
        }

        let (left, right) = match (self, other) {
            (Self::Blank, Self::Blank) => return Ok(Ordering::Equal),
            (Self::Blank, r) => (blank_as(r), r.clone()),
            (l, Self::Blank) => (l.clone(), blank_as(l)),
            (l, r) => (l.clone(), r.clone()),
        };

        match left.kind_rank().cmp(&right.kind_rank()) {
            Ordering::Equal => {}
            unequal => return Ok(unequal),
        }

        Ok(match (&left, &right) {
            (Self::Number(a), Self::Number(b)) => {
                // Neither can be NaN, so the comparison is total.
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (Self::Text(a), Self::Text(b)) => compare_text(a, b),
            (Self::Logical(a), Self::Logical(b)) => a.cmp(b),
            // kind_rank already established that the kinds match.
            _ => Ordering::Equal,
        })
    }
}

/// The value a blank takes on when compared against `other`.
fn blank_as(other: &Value) -> Value {
    match other {
        Value::Text(_) => Value::Text(Arc::from("")),
        Value::Logical(_) => Value::Logical(false),
        _ => Value::Number(0.0),
    }
}

/// Compare two strings the way a spreadsheet does: case-insensitively.
pub fn compare_text(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().flat_map(char::to_uppercase);
    let mut right = b.chars().flat_map(char::to_uppercase);
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match x.cmp(&y) {
                Ordering::Equal => continue,
                unequal => return unequal,
            },
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Blank, Self::Blank) => true,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Logical(a), Self::Logical(b)) => a == b,
            (Self::Error(a), Self::Error(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Self::number(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Logical(b)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Text(Arc::from(s))
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Text(Arc::from(s))
    }
}

impl From<CalcError> for Value {
    fn from(e: CalcError) -> Self {
        Self::Error(e)
    }
}

/// Parse text that a formula used where a number was expected.
///
/// Accepts a leading sign, a decimal point, scientific notation and
/// surrounding whitespace. Deliberately strict otherwise: thousands
/// separators, currency symbols and percent signs are locale-dependent and
/// belong to the number-format layer, not here.
pub fn parse_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Rust accepts "inf" and "NaN"; a spreadsheet does not.
    if trimmed
        .bytes()
        .any(|b| !(b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E')))
    {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Render a number the way the general format does.
///
/// Keeps at most [`SIGNIFICANT_DIGITS`] digits and drops trailing zeros, so
/// `1/3` reads back as `0.333333333333333` rather than as a full double.
pub fn format_general(n: f64) -> String {
    if n == 0.0 {
        // Covers negative zero, which a cell shows as plain zero.
        return "0".to_string();
    }
    if !n.is_finite() {
        return CalcError::Num.as_str().to_string();
    }

    // Round to the digit budget first, then decide how to lay the result out.
    let rounded = format!("{:.*e}", SIGNIFICANT_DIGITS - 1, n);
    let (mantissa, exponent) = rounded
        .split_once('e')
        .expect("Rust's exponential format always emits an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("Rust's exponential format always emits a valid exponent");

    if exponent >= SCIENTIFIC_UPPER_EXP || exponent <= SCIENTIFIC_LOWER_EXP {
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!(
            "{}E{sign}{:02}",
            trim_trailing_zeros(mantissa),
            exponent.abs()
        );
    }

    let decimals = (SIGNIFICANT_DIGITS as i32 - 1 - exponent).max(0) as usize;
    trim_trailing_zeros(&format!("{n:.decimals$}"))
}

/// Round to a number of significant decimal digits.
///
/// Needed because binary doubles do not hold decimal fractions exactly, and a
/// spreadsheet is expected to behave as though they do. Scaling 2.675 by a
/// hundred lands on 267.49999999999997, so rounding it gives 267 where a
/// person reading the digits expects 268. Snapping to the engine's
/// [`SIGNIFICANT_DIGITS`] budget first restores the answer the digits imply.
pub fn round_to_significant_digits(value: f64, digits: usize) -> f64 {
    if value == 0.0 || !value.is_finite() || digits == 0 {
        return value;
    }
    // Rendering and reparsing is exact for this purpose and needs no decimal
    // arithmetic of our own.
    format!("{:.*e}", digits - 1, value)
        .parse()
        .unwrap_or(value)
}

/// Drop trailing zeros from a fixed-point rendering, and the point with them.
fn trim_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_finite_numbers_become_num_errors() {
        assert_eq!(Value::number(f64::NAN), Value::Error(CalcError::Num));
        assert_eq!(Value::number(f64::INFINITY), Value::Error(CalcError::Num));
        assert_eq!(Value::number(1.5), Value::Number(1.5));
    }

    #[test]
    fn numbers_coerce_from_every_kind() {
        assert_eq!(Value::Blank.to_number(), Ok(0.0));
        assert_eq!(Value::Logical(true).to_number(), Ok(1.0));
        assert_eq!(Value::Logical(false).to_number(), Ok(0.0));
        assert_eq!(Value::text(" 42 ").to_number(), Ok(42.0));
        assert_eq!(Value::text("1e3").to_number(), Ok(1000.0));
        assert_eq!(Value::text("abc").to_number(), Err(CalcError::Value));
        assert_eq!(Value::text("").to_number(), Err(CalcError::Value));
        assert_eq!(
            Value::Error(CalcError::Ref).to_number(),
            Err(CalcError::Ref)
        );
    }

    #[test]
    fn infinity_spellings_are_not_numbers() {
        assert_eq!(parse_number("inf"), None);
        assert_eq!(parse_number("NaN"), None);
        assert_eq!(parse_number("infinity"), None);
    }

    #[test]
    fn logicals_render_in_upper_case() {
        assert_eq!(Value::Logical(true).to_text(), Ok("TRUE".to_string()));
        assert_eq!(Value::Logical(false).to_text(), Ok("FALSE".to_string()));
    }

    #[test]
    fn general_format_keeps_fifteen_digits() {
        assert_eq!(format_general(1.0 / 3.0), "0.333333333333333");
        assert_eq!(format_general(2.0 / 3.0), "0.666666666666667");
        assert_eq!(format_general(1.0), "1");
        assert_eq!(format_general(-0.0), "0");
        assert_eq!(format_general(1.5), "1.5");
        assert_eq!(format_general(0.1 + 0.2), "0.3");
        assert_eq!(format_general(1234.5678), "1234.5678");
    }

    #[test]
    fn general_format_switches_to_scientific_past_the_digit_budget() {
        assert_eq!(format_general(1e15), "1E+15");
        assert_eq!(format_general(1e21), "1E+21");
        assert_eq!(format_general(-1e21), "-1E+21");
        // One below the switch still reads plainly.
        assert_eq!(format_general(1e14), "100000000000000");
        assert_eq!(format_general(0.00001), "0.00001");
    }

    #[test]
    fn comparison_sorts_numbers_below_text_below_logicals() {
        let number = Value::Number(1000.0);
        let text = Value::text("a");
        let logical = Value::Logical(false);
        assert_eq!(number.compare(&text), Ok(Ordering::Less));
        assert_eq!(text.compare(&logical), Ok(Ordering::Less));
        assert_eq!(logical.compare(&number), Ok(Ordering::Greater));
    }

    #[test]
    fn text_comparison_ignores_case() {
        assert_eq!(
            Value::text("abc").compare(&Value::text("ABC")),
            Ok(Ordering::Equal)
        );
        assert_eq!(
            Value::text("abc").compare(&Value::text("abd")),
            Ok(Ordering::Less)
        );
    }

    #[test]
    fn blank_takes_the_kind_it_is_compared_against() {
        assert_eq!(
            Value::Blank.compare(&Value::Number(0.0)),
            Ok(Ordering::Equal)
        );
        assert_eq!(Value::Blank.compare(&Value::text("")), Ok(Ordering::Equal));
        assert_eq!(
            Value::Blank.compare(&Value::Logical(false)),
            Ok(Ordering::Equal)
        );
        assert_eq!(
            Value::Blank.compare(&Value::Logical(true)),
            Ok(Ordering::Less)
        );
        assert_eq!(
            Value::Blank.compare(&Value::Number(-1.0)),
            Ok(Ordering::Greater)
        );
    }

    #[test]
    fn errors_propagate_out_of_comparisons() {
        let err = Value::Error(CalcError::NotAvailable);
        assert_eq!(
            err.compare(&Value::Number(1.0)),
            Err(CalcError::NotAvailable)
        );
        assert_eq!(
            Value::Number(1.0).compare(&err),
            Err(CalcError::NotAvailable)
        );
    }

    #[test]
    fn errors_round_trip_through_their_display_text() {
        for error in [
            CalcError::Null,
            CalcError::Div0,
            CalcError::Value,
            CalcError::Ref,
            CalcError::Name,
            CalcError::Num,
            CalcError::NotAvailable,
            CalcError::Spill,
            CalcError::Calc,
        ] {
            assert_eq!(CalcError::from_display(error.as_str()), Some(error));
        }
        assert_eq!(CalcError::from_display("#NOPE!"), None);
    }
}
