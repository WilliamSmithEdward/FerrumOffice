//! Turning tokens into a tree.
//!
//! Precedence climbing, with the spreadsheet's own precedence table. Two
//! things about that table surprise people coming from other languages, and
//! both are deliberate here:
//!
//! - **Unary minus binds tighter than `^`.** `-2^2` is 4, not -4.
//! - **`^` is left-associative.** `2^3^2` is 64, not 512. This one is recall
//!   rather than measurement and is listed in `docs/open-questions.md` for
//!   checking against a live spreadsheet.
//!
//! The reference operators (`:`, a space, and `,` inside parentheses) bind
//! tighter than everything, including unary minus, which is why they are
//! handled structurally rather than through the binding-power loop.

use ferrum_core::{A1Ref, CellRef, MAX_ROW, ParseRefError, Value, parse_column_label};

use crate::ast::{BinaryOp, Bound, Expr, RefTarget, SheetRef, UnaryOp};
use crate::lexer::{LexError, Spanned, Token, tokenize};

/// Why a formula could not be parsed.
#[derive(Clone, PartialEq, Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    /// Byte offset into the formula text.
    pub at: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ParseErrorKind {
    Lex(LexError),
    /// The formula ended in the middle of something.
    UnexpectedEnd,
    /// A token that cannot appear here.
    UnexpectedToken,
    /// A `(` with no matching `)`, or the same for a brace.
    UnclosedBracket,
    /// A reference whose row or column is outside the grid.
    BadReference(ParseRefError),
    /// An array literal whose rows are not all the same length.
    RaggedArray,
    /// Text after the formula has already ended.
    TrailingInput,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match &self.kind {
            ParseErrorKind::Lex(e) => return write!(f, "{e}"),
            ParseErrorKind::UnexpectedEnd => "the formula ends here, unfinished".to_string(),
            ParseErrorKind::UnexpectedToken => "this cannot appear here".to_string(),
            ParseErrorKind::UnclosedBracket => "this bracket is never closed".to_string(),
            ParseErrorKind::BadReference(e) => format!("{e}"),
            ParseErrorKind::RaggedArray => "the rows of this array differ in length".to_string(),
            ParseErrorKind::TrailingInput => "the formula already ended before this".to_string(),
        };
        write!(f, "{what} (at {})", self.at)
    }
}

impl std::error::Error for ParseError {}

/// Parse a formula. A single leading `=` is accepted and ignored.
pub fn parse(formula: &str) -> Result<Expr, ParseError> {
    let body = formula.strip_prefix('=').unwrap_or(formula);
    let tokens = tokenize(body).map_err(|e| ParseError {
        at: e.at,
        kind: ParseErrorKind::Lex(e),
    })?;
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        end: body.len(),
    };
    let expr = parser.expression(0)?;
    parser.skip_space();
    if parser.peek().is_some() {
        return Err(parser.error_here(ParseErrorKind::TrailingInput));
    }
    Ok(expr)
}

/// What an atom turned out to be.
///
/// A bare column (`A`) or a bare row (`7`) is only meaningful as one end of a
/// range. Standing alone, `A` is a defined name and `7` is a number, so the
/// decision is deferred until the range operator has had its chance.
enum Atom {
    Done(Expr),
    BareColumn(Bound),
    BareRow(Bound),
}

impl Atom {
    /// Collapse to an expression, for a position where a range endpoint is
    /// not what was meant.
    fn into_expr(self) -> Expr {
        match self {
            Self::Done(expr) => expr,
            Self::BareColumn(bound) => Expr::Name {
                sheet: None,
                name: ferrum_core::column_label(bound.index),
            },
            Self::BareRow(bound) => Expr::Literal(Value::Number(f64::from(bound.index) + 1.0)),
        }
    }
}

struct Parser<'a> {
    tokens: &'a [Spanned],
    at: usize,
    /// Offset just past the formula, for errors that point at the end.
    end: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.at).map(|s| &s.token)
    }

    fn peek_at(&self, ahead: usize) -> Option<&'a Token> {
        self.tokens.get(self.at + ahead).map(|s| &s.token)
    }

    fn position(&self) -> usize {
        self.tokens.get(self.at).map_or(self.end, |s| s.start)
    }

    fn advance(&mut self) -> Option<&'a Token> {
        let token = self.peek();
        if token.is_some() {
            self.at += 1;
        }
        token
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(Token::Space)) {
            self.at += 1;
        }
    }

    fn error_here(&self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            kind,
            at: self.position(),
        }
    }

    fn expect(&mut self, expected: &Token, kind: ParseErrorKind) -> Result<(), ParseError> {
        self.skip_space();
        if self.peek() == Some(expected) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error_here(kind))
        }
    }

    /// The binding-power loop over the infix operators.
    fn expression(&mut self, min_power: u8) -> Result<Expr, ParseError> {
        let mut left = self.prefix()?;

        loop {
            self.skip_space();
            let Some(op) = self.peek().and_then(binary_op) else {
                break;
            };
            let power = op.binding_power();
            if power < min_power {
                break;
            }
            self.advance();
            // Every operator here is left-associative, `^` included, so the
            // right side is parsed at one power higher.
            let right = self.expression(power + 1)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Unary signs, then any trailing `%`.
    ///
    /// Percent sits between unary and `^` in the table, so it is applied
    /// outside the sign: `-2%` is -0.02.
    fn prefix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.signed()?;
        loop {
            self.skip_space();
            if self.peek() == Some(&Token::Percent) {
                self.advance();
                expr = Expr::Percent(Box::new(expr));
            } else {
                return Ok(expr);
            }
        }
    }

    fn signed(&mut self) -> Result<Expr, ParseError> {
        self.skip_space();
        let op = match self.peek() {
            Some(Token::Minus) => UnaryOp::Negate,
            Some(Token::Plus) => UnaryOp::Plus,
            _ => return self.reference_chain(),
        };
        self.advance();
        let operand = self.signed()?;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
        })
    }

    /// Intersection: two references separated by a space.
    ///
    /// Looser than `:`, so `A1:A5 A3:B3` intersects two ranges rather than
    /// building a range out of an intersection.
    fn reference_chain(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.range_chain()?;

        // A space is the intersection operator only when an operand follows
        // it. Otherwise it is spacing around an infix operator.
        while self.peek() == Some(&Token::Space)
            && self
                .peek_at(1)
                .is_some_and(super::lexer::Token::can_begin_operand)
        {
            self.advance();
            let right = self.range_chain()?;
            left = Atom::Done(Expr::Intersect {
                left: Box::new(left.into_expr()),
                right: Box::new(right.into_expr()),
            });
        }

        Ok(left.into_expr())
    }

    /// The range operator, which binds tighter than everything else.
    fn range_chain(&mut self) -> Result<Atom, ParseError> {
        let mut left = self.atom()?;
        while self.peek() == Some(&Token::Colon) {
            self.advance();
            let right = self.atom()?;
            left = Atom::Done(join_range(left, right, self.position())?);
        }
        Ok(left)
    }

    fn atom(&mut self) -> Result<Atom, ParseError> {
        self.skip_space();
        let start = self.position();
        let Some(token) = self.peek() else {
            return Err(self.error_here(ParseErrorKind::UnexpectedEnd));
        };

        match token {
            Token::Number(n) => {
                let value = *n;
                self.advance();
                // An integer in range may be one end of a whole-row range.
                if value.fract() == 0.0 && value >= 1.0 && value <= f64::from(MAX_ROW) + 1.0 {
                    return Ok(Atom::BareRow(Bound {
                        index: value as u32 - 1,
                        absolute: false,
                    }));
                }
                Ok(Atom::Done(Expr::Literal(Value::number(value))))
            }

            Token::Text(s) => {
                let value = Value::text(s.as_str());
                self.advance();
                Ok(Atom::Done(Expr::Literal(value)))
            }

            Token::Error(e) => {
                let error = *e;
                self.advance();
                Ok(Atom::Done(Expr::Literal(Value::Error(error))))
            }

            Token::OpenParen => {
                self.advance();
                self.parenthesised(start)
            }

            Token::OpenBrace => {
                self.advance();
                self.array_literal(start)
            }

            Token::Word(_) | Token::QuotedName(_) => self.word_atom(),

            _ => Err(self.error_here(ParseErrorKind::UnexpectedToken)),
        }
    }

    /// A parenthesised group, which is either grouping or a union.
    fn parenthesised(&mut self, start: usize) -> Result<Atom, ParseError> {
        let first = self.expression(0)?;
        self.skip_space();

        if self.peek() != Some(&Token::Comma) {
            self.expect(&Token::CloseParen, ParseErrorKind::UnclosedBracket)
                .map_err(|_| ParseError {
                    kind: ParseErrorKind::UnclosedBracket,
                    at: start,
                })?;
            return Ok(Atom::Done(first));
        }

        // Commas at the top level of a group make it a union of references.
        let mut parts = vec![first];
        while self.peek() == Some(&Token::Comma) {
            self.advance();
            parts.push(self.expression(0)?);
            self.skip_space();
        }
        self.expect(&Token::CloseParen, ParseErrorKind::UnclosedBracket)
            .map_err(|_| ParseError {
                kind: ParseErrorKind::UnclosedBracket,
                at: start,
            })?;
        Ok(Atom::Done(Expr::Union(parts)))
    }

    /// `{1,2;3,4}`: commas separate columns, semicolons separate rows.
    fn array_literal(&mut self, start: usize) -> Result<Atom, ParseError> {
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row: Vec<Expr> = Vec::new();

        loop {
            self.skip_space();
            if self.peek() == Some(&Token::CloseBrace) {
                self.advance();
                break;
            }

            row.push(self.expression(0)?);
            self.skip_space();

            match self.peek() {
                Some(Token::Comma) => {
                    self.advance();
                }
                Some(Token::Semicolon) => {
                    self.advance();
                    rows.push(std::mem::take(&mut row));
                }
                Some(Token::CloseBrace) => {
                    self.advance();
                    break;
                }
                Some(_) => return Err(self.error_here(ParseErrorKind::UnexpectedToken)),
                None => {
                    return Err(ParseError {
                        kind: ParseErrorKind::UnclosedBracket,
                        at: start,
                    });
                }
            }
        }

        if !row.is_empty() {
            rows.push(row);
        }

        let width = rows.first().map_or(0, Vec::len);
        if rows.iter().any(|r| r.len() != width) {
            return Err(ParseError {
                kind: ParseErrorKind::RaggedArray,
                at: start,
            });
        }

        Ok(Atom::Done(Expr::Array(rows)))
    }

    /// A word: a function call, a sheet-qualified reference, a plain
    /// reference, or a defined name.
    fn word_atom(&mut self) -> Result<Atom, ParseError> {
        let start = self.position();

        // A sheet prefix is a name, or two names joined by a colon, followed
        // by `!`.
        if let Some(sheet) = self.try_sheet_prefix() {
            let target = self.local_reference()?;
            return Ok(Atom::Done(match target {
                Some(target) => Expr::Ref {
                    sheet: Some(sheet),
                    target,
                },
                None => {
                    // `Sheet1!SomeName` is a sheet-scoped defined name.
                    let name = match self.advance() {
                        Some(Token::Word(w)) => w.clone(),
                        _ => {
                            return Err(ParseError {
                                kind: ParseErrorKind::UnexpectedToken,
                                at: start,
                            });
                        }
                    };
                    Expr::Name {
                        sheet: Some(sheet),
                        name,
                    }
                }
            }));
        }

        let Some(Token::Word(word)) = self.peek() else {
            return Err(self.error_here(ParseErrorKind::UnexpectedToken));
        };
        let word = word.clone();

        // A word followed by `(` is a call, whatever it otherwise looks like.
        if self.peek_at(1) == Some(&Token::OpenParen) {
            self.advance();
            self.advance();
            let args = self.argument_list(start)?;
            return Ok(Atom::Done(Expr::Call { name: word, args }));
        }

        self.advance();

        if word.eq_ignore_ascii_case("TRUE") {
            return Ok(Atom::Done(Expr::Literal(Value::Logical(true))));
        }
        if word.eq_ignore_ascii_case("FALSE") {
            return Ok(Atom::Done(Expr::Literal(Value::Logical(false))));
        }

        match classify_word(&word) {
            WordKind::Cell(cell) => Ok(Atom::Done(Expr::Ref {
                sheet: None,
                target: RefTarget::Cell(cell),
            })),
            WordKind::Column(bound) => Ok(Atom::BareColumn(bound)),
            WordKind::OutOfGrid(e) => Err(ParseError {
                kind: ParseErrorKind::BadReference(e),
                at: start,
            }),
            WordKind::Name => Ok(Atom::Done(Expr::Name {
                sheet: None,
                name: word,
            })),
        }
    }

    /// Consume `Name!` or `First:Last!` if that is what comes next.
    fn try_sheet_prefix(&mut self) -> Option<SheetRef> {
        let name_at = |token: Option<&Token>| match token {
            Some(Token::Word(w)) => Some(w.clone()),
            Some(Token::QuotedName(w)) => Some(w.clone()),
            _ => None,
        };

        let first = name_at(self.peek())?;

        if self.peek_at(1) == Some(&Token::Bang) {
            self.at += 2;
            return Some(SheetRef::One(first));
        }

        if self.peek_at(1) == Some(&Token::Colon)
            && self.peek_at(3) == Some(&Token::Bang)
            && let Some(last) = name_at(self.peek_at(2))
        {
            self.at += 4;
            return Some(SheetRef::Span { first, last });
        }

        None
    }

    /// The reference part after a sheet prefix, if there is one.
    ///
    /// Returns `None` when what follows is a defined name rather than a
    /// reference, leaving the token unconsumed.
    fn local_reference(&mut self) -> Result<Option<RefTarget>, ParseError> {
        if self.peek() == Some(&Token::Error(ferrum_core::CalcError::Ref)) {
            self.advance();
            return Ok(Some(RefTarget::Invalid));
        }

        let start = self.position();
        let first = match self.peek() {
            Some(Token::Word(w)) => classify_word(w),
            Some(Token::Number(n)) if n.fract() == 0.0 && *n >= 1.0 => {
                WordKind::Column(Bound {
                    // Reused below only for the row case; the index is the
                    // zero-based row.
                    index: *n as u32 - 1,
                    absolute: false,
                })
            }
            _ => return Ok(None),
        };

        // A bare number after `!` can only be a row range.
        if let Some(Token::Number(n)) = self.peek() {
            let row = *n as u32 - 1;
            if self.peek_at(1) != Some(&Token::Colon) {
                return Ok(None);
            }
            self.advance();
            self.advance();
            let Some(Token::Number(m)) = self.peek() else {
                return Err(self.error_here(ParseErrorKind::UnexpectedToken));
            };
            let last = *m as u32 - 1;
            self.advance();
            if row > MAX_ROW || last > MAX_ROW {
                return Err(ParseError {
                    kind: ParseErrorKind::BadReference(ParseRefError::RowRange),
                    at: start,
                });
            }
            return Ok(Some(rows_target(
                Bound {
                    index: row,
                    absolute: false,
                },
                Bound {
                    index: last,
                    absolute: false,
                },
            )));
        }

        match first {
            WordKind::Cell(cell) => {
                self.advance();
                // `Sheet1!A1:B2` keeps the sheet across the whole range.
                if self.peek() == Some(&Token::Colon)
                    && let Some(Token::Word(next)) = self.peek_at(1)
                    && let WordKind::Cell(end) = classify_word(next)
                    && self.peek_at(2) != Some(&Token::Bang)
                {
                    self.advance();
                    self.advance();
                    return Ok(Some(RefTarget::Range { start: cell, end }));
                }
                Ok(Some(RefTarget::Cell(cell)))
            }
            WordKind::Column(bound) => {
                // Only a range makes sense: `Sheet1!A:C`.
                if self.peek_at(1) == Some(&Token::Colon)
                    && let Some(Token::Word(next)) = self.peek_at(2)
                    && let WordKind::Column(last) = classify_word(next)
                {
                    self.advance();
                    self.advance();
                    self.advance();
                    return Ok(Some(columns_target(bound, last)));
                }
                Ok(None)
            }
            WordKind::OutOfGrid(e) => Err(ParseError {
                kind: ParseErrorKind::BadReference(e),
                at: start,
            }),
            WordKind::Name => Ok(None),
        }
    }

    fn argument_list(&mut self, start: usize) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_space();

        if self.peek() == Some(&Token::CloseParen) {
            self.advance();
            return Ok(args);
        }

        loop {
            self.skip_space();
            // An omitted argument, as in `IF(A1,,0)`, is a blank.
            if matches!(self.peek(), Some(Token::Comma | Token::CloseParen)) {
                args.push(Expr::Literal(Value::Blank));
            } else {
                args.push(self.expression(0)?);
            }
            self.skip_space();

            match self.peek() {
                Some(Token::Comma) => {
                    self.advance();
                }
                Some(Token::CloseParen) => {
                    self.advance();
                    return Ok(args);
                }
                Some(_) => return Err(self.error_here(ParseErrorKind::UnexpectedToken)),
                None => {
                    return Err(ParseError {
                        kind: ParseErrorKind::UnclosedBracket,
                        at: start,
                    });
                }
            }
        }
    }
}

/// What a bare word turned out to name.
enum WordKind {
    Cell(A1Ref),
    /// Letters only, which is one end of a column range or else a name.
    Column(Bound),
    /// Shaped like a reference but outside the grid.
    OutOfGrid(ParseRefError),
    Name,
}

fn classify_word(word: &str) -> WordKind {
    match A1Ref::parse(word) {
        Ok(cell) => return WordKind::Cell(cell),
        Err(ParseRefError::ColumnRange | ParseRefError::RowRange) => {
            // `XFE1` and `A1048577` are meant as references and missed.
            if looks_like_a_reference(word) {
                return WordKind::OutOfGrid(A1Ref::parse(word).unwrap_err());
            }
        }
        Err(ParseRefError::Malformed) => {}
    }

    let (letters, absolute) = match word.strip_prefix('$') {
        Some(rest) => (rest, true),
        None => (word, false),
    };
    if !letters.is_empty() && letters.bytes().all(|b| b.is_ascii_alphabetic()) {
        if let Some(index) = parse_column_label(letters) {
            return WordKind::Column(Bound { index, absolute });
        }
    }

    WordKind::Name
}

/// Whether a word is shaped `letters digits`, so that being out of range is a
/// reference problem rather than the word being an ordinary name.
fn looks_like_a_reference(word: &str) -> bool {
    let trimmed = word.replace('$', "");
    let letters = trimmed.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = &trimmed[letters.len()..];
    !letters.is_empty()
        && !digits.is_empty()
        && letters.bytes().all(|b| b.is_ascii_alphabetic())
        && letters.len() <= 3
}

fn columns_target(first: Bound, last: Bound) -> RefTarget {
    let (first, last) = if first.index <= last.index {
        (first, last)
    } else {
        (last, first)
    };
    RefTarget::Columns { first, last }
}

fn rows_target(first: Bound, last: Bound) -> RefTarget {
    let (first, last) = if first.index <= last.index {
        (first, last)
    } else {
        (last, first)
    };
    RefTarget::Rows { first, last }
}

/// Build the range that `left:right` means.
///
/// The common cases collapse into a single reference. Anything else stays as
/// an operator for the evaluator to work out, which is what makes
/// `INDEX(...):B5` possible.
fn join_range(left: Atom, right: Atom, at: usize) -> Result<Expr, ParseError> {
    Ok(match (left, right) {
        (Atom::BareColumn(a), Atom::BareColumn(b)) => Expr::Ref {
            sheet: None,
            target: columns_target(a, b),
        },
        (Atom::BareRow(a), Atom::BareRow(b)) => Expr::Ref {
            sheet: None,
            target: rows_target(a, b),
        },
        (
            Atom::Done(Expr::Ref {
                sheet,
                target: RefTarget::Cell(start),
            }),
            Atom::Done(Expr::Ref {
                sheet: None,
                target: RefTarget::Cell(end),
            }),
        ) => {
            let (start, end) = normalise_corners(start, end);
            Expr::Ref {
                sheet,
                target: RefTarget::Range { start, end },
            }
        }
        (left, right) => {
            let _ = at;
            Expr::RangeOp {
                left: Box::new(left.into_expr()),
                right: Box::new(right.into_expr()),
            }
        }
    })
}

/// Put the top-left corner first while keeping each edge's own `$` marker.
fn normalise_corners(a: A1Ref, b: A1Ref) -> (A1Ref, A1Ref) {
    let start = A1Ref {
        cell: CellRef::new(a.cell.row.min(b.cell.row), a.cell.col.min(b.cell.col)),
        row_absolute: if a.cell.row <= b.cell.row {
            a.row_absolute
        } else {
            b.row_absolute
        },
        col_absolute: if a.cell.col <= b.cell.col {
            a.col_absolute
        } else {
            b.col_absolute
        },
    };
    let end = A1Ref {
        cell: CellRef::new(a.cell.row.max(b.cell.row), a.cell.col.max(b.cell.col)),
        row_absolute: if a.cell.row > b.cell.row {
            a.row_absolute
        } else {
            b.row_absolute
        },
        col_absolute: if a.cell.col > b.cell.col {
            a.col_absolute
        } else {
            b.col_absolute
        },
    };
    (start, end)
}

fn binary_op(token: &Token) -> Option<BinaryOp> {
    Some(match token {
        Token::Plus => BinaryOp::Add,
        Token::Minus => BinaryOp::Subtract,
        Token::Star => BinaryOp::Multiply,
        Token::Slash => BinaryOp::Divide,
        Token::Caret => BinaryOp::Power,
        Token::Ampersand => BinaryOp::Concat,
        Token::Equal => BinaryOp::Equal,
        Token::NotEqual => BinaryOp::NotEqual,
        Token::Less => BinaryOp::Less,
        Token::LessEqual => BinaryOp::LessOrEqual,
        Token::Greater => BinaryOp::Greater,
        Token::GreaterEqual => BinaryOp::GreaterOrEqual,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_core::CalcError;

    /// A compact rendering, so a test can state the tree it expects on one line.
    fn sexpr(expr: &Expr) -> String {
        match expr {
            Expr::Literal(Value::Number(n)) => ferrum_core::value::format_general(*n),
            Expr::Literal(Value::Text(s)) => format!("{s:?}"),
            Expr::Literal(Value::Logical(b)) => {
                if *b {
                    "TRUE".into()
                } else {
                    "FALSE".into()
                }
            }
            Expr::Literal(Value::Blank) => "blank".into(),
            Expr::Literal(Value::Error(e)) => e.as_str().to_string(),
            Expr::Ref { sheet, target } => {
                let prefix = match sheet {
                    None => String::new(),
                    Some(SheetRef::One(s)) => format!("{s}!"),
                    Some(SheetRef::Span { first, last }) => format!("{first}:{last}!"),
                };
                let body = match target {
                    RefTarget::Cell(c) => c.to_a1(),
                    RefTarget::Range { start, end } => format!("{}:{}", start.to_a1(), end.to_a1()),
                    RefTarget::Columns { first, last } => format!(
                        "{}:{}",
                        ferrum_core::column_label(first.index),
                        ferrum_core::column_label(last.index)
                    ),
                    RefTarget::Rows { first, last } => {
                        format!("{}:{}", first.index + 1, last.index + 1)
                    }
                    RefTarget::Invalid => "#REF!".into(),
                };
                format!("{prefix}{body}")
            }
            Expr::Name { sheet, name } => match sheet {
                Some(SheetRef::One(s)) => format!("name({s}!{name})"),
                _ => format!("name({name})"),
            },
            Expr::Unary { op, operand } => {
                let sign = match op {
                    UnaryOp::Negate => "neg",
                    UnaryOp::Plus => "pos",
                };
                format!("({sign} {})", sexpr(operand))
            }
            Expr::Binary { op, left, right } => {
                format!("({} {} {})", op.symbol(), sexpr(left), sexpr(right))
            }
            Expr::Percent(inner) => format!("(% {})", sexpr(inner)),
            Expr::Call { name, args } => {
                let rendered: Vec<_> = args.iter().map(sexpr).collect();
                format!("{name}({})", rendered.join(" "))
            }
            Expr::Array(rows) => {
                let rendered: Vec<String> = rows
                    .iter()
                    .map(|r| r.iter().map(sexpr).collect::<Vec<_>>().join(" "))
                    .collect();
                format!("{{{}}}", rendered.join(" ; "))
            }
            Expr::RangeOp { left, right } => format!("(: {} {})", sexpr(left), sexpr(right)),
            Expr::Intersect { left, right } => {
                format!("(isect {} {})", sexpr(left), sexpr(right))
            }
            Expr::Union(parts) => {
                let rendered: Vec<_> = parts.iter().map(sexpr).collect();
                format!("(union {})", rendered.join(" "))
            }
        }
    }

    fn tree(formula: &str) -> String {
        sexpr(&parse(formula).unwrap())
    }

    #[test]
    fn arithmetic_follows_the_usual_precedence() {
        assert_eq!(tree("=1+2*3"), "(+ 1 (* 2 3))");
        assert_eq!(tree("=(1+2)*3"), "(* (+ 1 2) 3)");
        assert_eq!(tree("=1+2-3"), "(- (+ 1 2) 3)");
        assert_eq!(tree("=8/4/2"), "(/ (/ 8 4) 2)");
    }

    #[test]
    fn concatenation_binds_looser_than_arithmetic() {
        assert_eq!(tree(r#"="a"&1+2"#), r#"(& "a" (+ 1 2))"#);
    }

    #[test]
    fn comparison_binds_loosest() {
        assert_eq!(tree("=1+2=3"), "(= (+ 1 2) 3)");
        assert_eq!(tree(r#"=A1&"x"<>"yx""#), r#"(<> (& A1 "x") "yx")"#);
    }

    #[test]
    fn unary_minus_binds_tighter_than_power() {
        // This is the one that surprises people: -2^2 is 4.
        assert_eq!(tree("=-2^2"), "(^ (neg 2) 2)");
        // A sign on the right of the operator still works.
        assert_eq!(tree("=2^-1"), "(^ 2 (neg 1))");
    }

    #[test]
    fn power_is_left_associative() {
        // Recall rather than measurement: see docs/open-questions.md.
        assert_eq!(tree("=2^3^2"), "(^ (^ 2 3) 2)");
    }

    #[test]
    fn percent_applies_outside_the_sign() {
        assert_eq!(tree("=-2%"), "(% (neg 2))");
        assert_eq!(tree("=50%*2"), "(* (% 50) 2)");
    }

    #[test]
    fn repeated_signs_are_allowed() {
        assert_eq!(tree("=--1"), "(neg (neg 1))");
        assert_eq!(tree("=+-1"), "(pos (neg 1))");
    }

    #[test]
    fn references_collapse_into_a_single_node() {
        assert_eq!(tree("=A1"), "A1");
        assert_eq!(tree("=$A$1"), "$A$1");
        assert_eq!(tree("=A1:B2"), "A1:B2");
        // Written backwards, the corners normalise.
        assert_eq!(tree("=B2:A1"), "A1:B2");
    }

    #[test]
    fn whole_columns_and_rows_parse() {
        assert_eq!(tree("=A:A"), "A:A");
        assert_eq!(tree("=B:D"), "B:D");
        assert_eq!(tree("=2:5"), "2:5");
        // Backwards spans normalise too.
        assert_eq!(tree("=D:B"), "B:D");
    }

    #[test]
    fn a_lone_column_letter_is_a_defined_name() {
        assert_eq!(tree("=A"), "name(A)");
    }

    #[test]
    fn sheet_qualified_references_keep_their_sheet() {
        assert_eq!(tree("=Sheet1!A1"), "Sheet1!A1");
        assert_eq!(tree("=Sheet1!A1:B2"), "Sheet1!A1:B2");
        assert_eq!(tree("='My Sheet'!A1"), "My Sheet!A1");
        assert_eq!(tree("=Sheet1:Sheet3!A1"), "Sheet1:Sheet3!A1");
        assert_eq!(tree("=Sheet1!Total"), "name(Sheet1!Total)");
    }

    #[test]
    fn calls_take_their_arguments() {
        assert_eq!(tree("=SUM(A1:B2,3)"), "SUM(A1:B2 3)");
        assert_eq!(tree("=NOW()"), "NOW()");
        assert_eq!(tree("=IF(A1>0,\"y\",\"n\")"), r#"IF((> A1 0) "y" "n")"#);
    }

    #[test]
    fn an_omitted_argument_is_a_blank() {
        assert_eq!(tree("=IF(A1,,0)"), "IF(A1 blank 0)");
        assert_eq!(tree("=SUM(,1)"), "SUM(blank 1)");
    }

    #[test]
    fn a_word_before_a_paren_is_a_call_even_if_it_reads_as_a_reference() {
        assert_eq!(tree("=LOG10(100)"), "LOG10(100)");
    }

    #[test]
    fn booleans_are_literals_not_names() {
        assert_eq!(tree("=TRUE"), "TRUE");
        assert_eq!(tree("=false"), "FALSE");
    }

    #[test]
    fn a_space_between_references_is_an_intersection() {
        assert_eq!(tree("=A1:A5 A3:B3"), "(isect A1:A5 A3:B3)");
    }

    #[test]
    fn a_space_before_a_sign_is_just_spacing() {
        // The case that makes naive intersection handling wrong.
        assert_eq!(tree("=A1 -B1"), "(- A1 B1)");
        assert_eq!(tree("=A1 + B1"), "(+ A1 B1)");
        assert_eq!(tree("= 1 + 2 "), "(+ 1 2)");
    }

    #[test]
    fn commas_inside_a_group_make_a_union() {
        assert_eq!(tree("=(A1:A2,C1:C2)"), "(union A1:A2 C1:C2)");
        assert_eq!(tree("=SUM((A1:A2,C1:C2))"), "SUM((union A1:A2 C1:C2))");
    }

    #[test]
    fn array_literals_keep_their_shape() {
        assert_eq!(tree("={1,2;3,4}"), "{1 2 ; 3 4}");
        assert_eq!(tree("={1;2;3}"), "{1 ; 2 ; 3}");
        assert_eq!(tree("={1,2,3}"), "{1 2 3}");
    }

    #[test]
    fn a_ragged_array_is_rejected() {
        assert_eq!(
            parse("={1,2;3}").unwrap_err().kind,
            ParseErrorKind::RaggedArray
        );
    }

    #[test]
    fn an_error_value_can_be_written_out() {
        assert_eq!(tree("=#N/A"), "#N/A");
        assert_eq!(tree("=IFERROR(A1,#N/A)"), "IFERROR(A1 #N/A)");
        assert_eq!(tree("=#REF!+1"), "(+ #REF! 1)");
    }

    #[test]
    fn a_dynamic_range_stays_an_operator() {
        assert_eq!(tree("=INDEX(A:A,1):B5"), "(: INDEX(A:A 1) B5)");
    }

    #[test]
    fn a_reference_outside_the_grid_is_rejected() {
        assert!(matches!(
            parse("=XFE1").unwrap_err().kind,
            ParseErrorKind::BadReference(ParseRefError::ColumnRange)
        ));
        assert!(matches!(
            parse("=A1048577").unwrap_err().kind,
            ParseErrorKind::BadReference(ParseRefError::RowRange)
        ));
    }

    #[test]
    fn unfinished_formulas_report_where_they_stopped() {
        assert_eq!(
            parse("=1+").unwrap_err().kind,
            ParseErrorKind::UnexpectedEnd
        );
        assert_eq!(
            parse("=SUM(1").unwrap_err().kind,
            ParseErrorKind::UnclosedBracket
        );
        assert_eq!(
            parse("=(1").unwrap_err().kind,
            ParseErrorKind::UnclosedBracket
        );
        assert_eq!(
            parse("=1 2 3)").unwrap_err().kind,
            ParseErrorKind::TrailingInput
        );
    }

    #[test]
    fn the_leading_equals_is_optional() {
        assert_eq!(parse("1+1").unwrap(), parse("=1+1").unwrap());
    }

    #[test]
    fn a_realistic_formula_parses() {
        let expected = concat!(
            "IF((> (isect Sales!A1:A100 B1:B100) 0) ",
            "(/ SUM($A$1:$A$10) 2) ",
            "IFERROR(VLOOKUP(A1 Sheet2!A:C 3 FALSE) \"\"))"
        );
        assert_eq!(
            tree(
                "=IF(Sales!A1:A100 B1:B100>0,SUM($A$1:$A$10)/2,\
                 IFERROR(VLOOKUP(A1,Sheet2!A:C,3,FALSE),\"\"))"
            ),
            expected
        );
    }

    #[test]
    fn errors_carry_a_position_inside_the_text() {
        let err = parse("=1+*2").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
        // The `*` sits at offset 2 of the body, after the `=` is stripped.
        assert_eq!(err.at, 2);
    }

    #[test]
    fn an_error_value_is_not_mistaken_for_a_reference_target() {
        assert_eq!(tree("=Sheet1!#REF!"), "Sheet1!#REF!");
        let _ = CalcError::Ref;
    }
}
