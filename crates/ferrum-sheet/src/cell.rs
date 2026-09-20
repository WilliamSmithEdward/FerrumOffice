//! What one cell holds.
//!
//! A cell keeps both what was typed and what it came to: the text so that
//! editing shows exactly what the author wrote, the value so that reading is
//! free. Recalculation replaces the second and never the first.

use std::rc::Rc;

use ferrum_calc::Expr;
use ferrum_core::{CalcError, Value};

/// What the author put in a cell.
#[derive(Clone, Debug)]
pub enum Input {
    /// A value typed directly.
    Literal(Value),
    /// A formula, kept as text and as the tree it parsed to.
    ///
    /// The text is authoritative for display and editing, because a parsed
    /// tree cannot reproduce spacing or the author's choice of case. The tree
    /// is shared rather than cloned on every recalculation.
    Formula { text: Box<str>, expr: Rc<Expr> },
    /// A formula that does not parse. It is kept so the author can fix it
    /// rather than losing what they typed.
    Malformed { text: Box<str>, reason: Box<str> },
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub input: Input,
    /// The last computed result. For a literal this is the literal.
    pub value: Value,
}

impl Cell {
    pub const fn is_formula(&self) -> bool {
        matches!(self.input, Input::Formula { .. } | Input::Malformed { .. })
    }

    /// What an editor should show when the cell is opened for editing.
    pub fn edit_text(&self) -> String {
        match &self.input {
            Input::Literal(value) => value.display(),
            Input::Formula { text, .. } | Input::Malformed { text, .. } => text.to_string(),
        }
    }
}

/// Read what the author typed into an input.
///
/// A leading `=` means a formula. Everything else is read as the most specific
/// value it can be: a number, a logical, a written-out error, or text.
pub fn parse_input(typed: &str) -> Input {
    if let Some(body) = typed.strip_prefix('=') {
        // An `=` on its own is not a formula, it is the text "=".
        if body.trim().is_empty() {
            return Input::Literal(Value::text(typed));
        }
        return match ferrum_calc::parse(typed) {
            Ok(expr) => Input::Formula {
                text: typed.into(),
                expr: Rc::new(expr),
            },
            Err(error) => Input::Malformed {
                text: typed.into(),
                reason: error.to_string().into_boxed_str(),
            },
        };
    }

    Input::Literal(literal_from(typed))
}

/// Read a typed value that is not a formula.
pub fn literal_from(typed: &str) -> Value {
    if typed.is_empty() {
        return Value::Blank;
    }
    if typed.eq_ignore_ascii_case("TRUE") {
        return Value::Logical(true);
    }
    if typed.eq_ignore_ascii_case("FALSE") {
        return Value::Logical(false);
    }
    if let Some(error) = CalcError::from_display(typed) {
        return Value::Error(error);
    }
    // Only an exactly numeric entry becomes a number, so a code like "1-2"
    // or an identifier stays text.
    if let Some(n) = ferrum_core::parse_number(typed) {
        return Value::number(n);
    }
    Value::text(typed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_take_their_most_specific_kind() {
        assert_eq!(literal_from("42"), Value::Number(42.0));
        assert_eq!(literal_from("-1.5e3"), Value::Number(-1500.0));
        assert_eq!(literal_from("TRUE"), Value::Logical(true));
        assert_eq!(literal_from("true"), Value::Logical(true));
        assert_eq!(literal_from("#N/A"), Value::Error(CalcError::NotAvailable));
        assert_eq!(literal_from("hello"), Value::text("hello"));
        assert_eq!(literal_from(""), Value::Blank);
    }

    #[test]
    fn text_that_merely_contains_digits_stays_text() {
        assert_eq!(literal_from("1-2"), Value::text("1-2"));
        assert_eq!(literal_from("A1"), Value::text("A1"));
        assert_eq!(literal_from("1 2"), Value::text("1 2"));
    }

    #[test]
    fn a_leading_equals_makes_a_formula() {
        assert!(matches!(parse_input("=1+1"), Input::Formula { .. }));
        assert!(matches!(parse_input("1+1"), Input::Literal(_)));
    }

    #[test]
    fn a_bare_equals_sign_is_text() {
        assert!(matches!(parse_input("="), Input::Literal(Value::Text(_))));
    }

    #[test]
    fn a_broken_formula_keeps_what_was_typed() {
        let input = parse_input("=1+");
        let Input::Malformed { text, reason } = input else {
            panic!("expected the text to be kept");
        };
        assert_eq!(&*text, "=1+");
        assert!(!reason.is_empty());
    }

    #[test]
    fn editing_shows_the_formula_rather_than_its_result() {
        let cell = Cell {
            input: parse_input("=1+1"),
            value: Value::Number(2.0),
        };
        assert_eq!(cell.edit_text(), "=1+1");
        assert!(cell.is_formula());
    }
}
