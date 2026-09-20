//! Turning formula text into tokens.
//!
//! The lexer stays deliberately ignorant. It does not know whether `A1` is a
//! cell or a defined name, nor whether `SUM` is a function; both are just
//! words. Deciding that needs the context of what follows, which is the
//! parser's job.
//!
//! One oddity is worth stating up front: **whitespace is significant**. A
//! space between two references is the intersection operator, so runs of
//! whitespace are emitted as a token rather than skipped.

use std::fmt;

use ferrum_core::CalcError;

/// A token together with where it came from, so an error can point at it.
#[derive(Clone, PartialEq, Debug)]
pub struct Spanned {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Token {
    Number(f64),
    /// A quoted string literal, with doubled quotes already collapsed.
    Text(String),
    /// An error value written out in the formula, such as `#N/A`.
    Error(CalcError),
    /// A run of name characters: a function, a defined name, a cell
    /// reference, or an unquoted sheet name.
    Word(String),
    /// A sheet name that needed quoting, with doubled quotes collapsed.
    QuotedName(String),

    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Ampersand,

    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,

    OpenParen,
    CloseParen,
    OpenBrace,
    CloseBrace,

    Comma,
    Semicolon,
    Colon,
    /// Separates a sheet name from the reference on it.
    Bang,
    /// One or more whitespace characters. Between two references this is the
    /// intersection operator; everywhere else the parser discards it.
    Space,
}

impl Token {
    /// Whether this token could begin an operand.
    ///
    /// Used to decide whether a space is an intersection operator or just
    /// spacing. Signs are excluded on purpose: `=A1 -B1` subtracts, it does
    /// not intersect `A1` with `-B1`.
    pub const fn can_begin_operand(&self) -> bool {
        matches!(
            self,
            Self::Number(_)
                | Self::Text(_)
                | Self::Error(_)
                | Self::Word(_)
                | Self::QuotedName(_)
                | Self::OpenParen
                | Self::OpenBrace
        )
    }
}

/// Why a formula could not be tokenised.
#[derive(Clone, PartialEq, Debug)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub at: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum LexErrorKind {
    /// A string literal ran to the end of the formula without closing.
    UnterminatedText,
    /// A quoted sheet name ran to the end of the formula without closing.
    UnterminatedName,
    /// A `#` that does not begin any known error value.
    UnknownError,
    /// A character that cannot appear in a formula.
    UnexpectedCharacter(char),
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            LexErrorKind::UnterminatedText => write!(f, "unclosed text at {}", self.at),
            LexErrorKind::UnterminatedName => write!(f, "unclosed sheet name at {}", self.at),
            LexErrorKind::UnknownError => write!(f, "unrecognised error value at {}", self.at),
            LexErrorKind::UnexpectedCharacter(c) => {
                write!(f, "unexpected character {c:?} at {}", self.at)
            }
        }
    }
}

impl std::error::Error for LexError {}

/// Every error value, longest first so that a prefix match cannot pick a
/// shorter one by accident.
const ERROR_SPELLINGS: &[(&str, CalcError)] = &[
    ("#GETTING_DATA", CalcError::Calc),
    ("#DIV/0!", CalcError::Div0),
    ("#VALUE!", CalcError::Value),
    ("#SPILL!", CalcError::Spill),
    ("#NAME?", CalcError::Name),
    ("#NULL!", CalcError::Null),
    ("#CALC!", CalcError::Calc),
    ("#REF!", CalcError::Ref),
    ("#NUM!", CalcError::Num),
    ("#N/A", CalcError::NotAvailable),
];

/// Characters that may appear inside a word after its first character.
fn is_word_continuation(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '?' | '\\')
}

/// Characters that may begin a word.
fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '_' | '$' | '\\')
}

/// Tokenise a formula. The leading `=` is not expected here; strip it first.
pub fn tokenize(input: &str) -> Result<Vec<Spanned>, LexError> {
    Lexer::new(input).run()
}

struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    at: usize,
    out: Vec<Spanned>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            at: 0,
            out: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.at..].chars().next()
    }

    fn push(&mut self, token: Token, start: usize) {
        self.out.push(Spanned {
            token,
            start,
            end: self.at,
        });
    }

    fn run(mut self) -> Result<Vec<Spanned>, LexError> {
        while let Some(c) = self.peek() {
            let start = self.at;
            match c {
                c if c.is_whitespace() => {
                    while self.peek().is_some_and(char::is_whitespace) {
                        self.at += self.peek().map_or(0, char::len_utf8);
                    }
                    self.push(Token::Space, start);
                }
                '"' => self.lex_text(start)?,
                '\'' => self.lex_quoted_name(start)?,
                '#' => self.lex_error(start)?,
                c if c.is_ascii_digit() => self.lex_number(start),
                '.' if self
                    .byte_at(self.at + 1)
                    .is_some_and(|b| b.is_ascii_digit()) =>
                {
                    self.lex_number(start);
                }
                c if is_word_start(c) => self.lex_word(start),
                _ => self.lex_operator(start, c)?,
            }
        }
        Ok(self.out)
    }

    fn byte_at(&self, index: usize) -> Option<u8> {
        self.bytes.get(index).copied()
    }

    /// A string literal. Two quote characters in a row mean one quote.
    fn lex_text(&mut self, start: usize) -> Result<(), LexError> {
        self.at += 1; // opening quote
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(LexError {
                        kind: LexErrorKind::UnterminatedText,
                        at: start,
                    });
                }
                Some('"') => {
                    self.at += 1;
                    if self.peek() == Some('"') {
                        value.push('"');
                        self.at += 1;
                    } else {
                        self.push(Token::Text(value), start);
                        return Ok(());
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    /// A sheet name in single quotes, which is how a name with a space or a
    /// punctuation character is written.
    fn lex_quoted_name(&mut self, start: usize) -> Result<(), LexError> {
        self.at += 1;
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(LexError {
                        kind: LexErrorKind::UnterminatedName,
                        at: start,
                    });
                }
                Some('\'') => {
                    self.at += 1;
                    if self.peek() == Some('\'') {
                        value.push('\'');
                        self.at += 1;
                    } else {
                        self.push(Token::QuotedName(value), start);
                        return Ok(());
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }

    fn lex_error(&mut self, start: usize) -> Result<(), LexError> {
        let rest = &self.input[start..];
        for (spelling, error) in ERROR_SPELLINGS {
            if rest.len() >= spelling.len() && rest[..spelling.len()].eq_ignore_ascii_case(spelling)
            {
                self.at = start + spelling.len();
                self.push(Token::Error(*error), start);
                return Ok(());
            }
        }
        Err(LexError {
            kind: LexErrorKind::UnknownError,
            at: start,
        })
    }

    fn lex_number(&mut self, start: usize) {
        while self.byte_at(self.at).is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        if self.byte_at(self.at) == Some(b'.') {
            self.at += 1;
            while self.byte_at(self.at).is_some_and(|b| b.is_ascii_digit()) {
                self.at += 1;
            }
        }
        // An exponent only counts if digits actually follow it, so that the
        // `E` in a word like `A1E` is not swallowed.
        if matches!(self.byte_at(self.at), Some(b'e' | b'E')) {
            let mut lookahead = self.at + 1;
            if matches!(self.byte_at(lookahead), Some(b'+' | b'-')) {
                lookahead += 1;
            }
            if self.byte_at(lookahead).is_some_and(|b| b.is_ascii_digit()) {
                self.at = lookahead;
                while self.byte_at(self.at).is_some_and(|b| b.is_ascii_digit()) {
                    self.at += 1;
                }
            }
        }

        // The slice is digits, at most one point and an optional exponent, so
        // it parses. A magnitude too large for a double saturates to infinity,
        // which the evaluator turns into #NUM! when the value is used.
        let value = self.input[start..self.at]
            .parse::<f64>()
            .unwrap_or(f64::MAX);
        self.push(Token::Number(value), start);
    }

    fn lex_word(&mut self, start: usize) {
        while self.peek().is_some_and(is_word_continuation) {
            self.at += self.peek().map_or(0, char::len_utf8);
        }
        self.push(Token::Word(self.input[start..self.at].to_string()), start);
    }

    fn lex_operator(&mut self, start: usize, c: char) -> Result<(), LexError> {
        let two = &self.input.as_bytes()[start..];
        let (token, width) = match c {
            '+' => (Token::Plus, 1),
            '-' => (Token::Minus, 1),
            '*' => (Token::Star, 1),
            '/' => (Token::Slash, 1),
            '^' => (Token::Caret, 1),
            '%' => (Token::Percent, 1),
            '&' => (Token::Ampersand, 1),
            '=' => (Token::Equal, 1),
            '(' => (Token::OpenParen, 1),
            ')' => (Token::CloseParen, 1),
            '{' => (Token::OpenBrace, 1),
            '}' => (Token::CloseBrace, 1),
            ',' => (Token::Comma, 1),
            ';' => (Token::Semicolon, 1),
            ':' => (Token::Colon, 1),
            '!' => (Token::Bang, 1),
            '<' => match two.get(1) {
                Some(b'=') => (Token::LessEqual, 2),
                Some(b'>') => (Token::NotEqual, 2),
                _ => (Token::Less, 1),
            },
            '>' => match two.get(1) {
                Some(b'=') => (Token::GreaterEqual, 2),
                _ => (Token::Greater, 1),
            },
            other => {
                return Err(LexError {
                    kind: LexErrorKind::UnexpectedCharacter(other),
                    at: start,
                });
            }
        };
        self.at = start + width;
        self.push(token, start);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: &str) -> Vec<Token> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|s| s.token)
            .collect()
    }

    #[test]
    fn numbers_cover_the_usual_spellings() {
        assert_eq!(tokens("1"), [Token::Number(1.0)]);
        assert_eq!(tokens("1.5"), [Token::Number(1.5)]);
        assert_eq!(tokens(".5"), [Token::Number(0.5)]);
        assert_eq!(tokens("1e3"), [Token::Number(1000.0)]);
        assert_eq!(tokens("1E+3"), [Token::Number(1000.0)]);
        assert_eq!(tokens("1.5e-3"), [Token::Number(0.0015)]);
    }

    #[test]
    fn an_exponent_marker_without_digits_is_not_part_of_the_number() {
        // `1E` is the number 1 followed by the word E, not a broken number.
        assert_eq!(
            tokens("1E"),
            [Token::Number(1.0), Token::Word("E".to_string())]
        );
    }

    #[test]
    fn text_collapses_doubled_quotes() {
        assert_eq!(tokens(r#""hi""#), [Token::Text("hi".to_string())]);
        assert_eq!(tokens(r#""a""b""#), [Token::Text(r#"a"b"#.to_string())]);
        assert_eq!(tokens(r#""""#), [Token::Text(String::new())]);
    }

    #[test]
    fn unterminated_text_is_an_error_pointing_at_the_quote() {
        let err = tokenize(r#"1+"oops"#).unwrap_err();
        assert_eq!(err.kind, LexErrorKind::UnterminatedText);
        assert_eq!(err.at, 2);
    }

    #[test]
    fn every_error_value_is_recognised() {
        assert_eq!(tokens("#N/A"), [Token::Error(CalcError::NotAvailable)]);
        assert_eq!(tokens("#DIV/0!"), [Token::Error(CalcError::Div0)]);
        assert_eq!(tokens("#REF!"), [Token::Error(CalcError::Ref)]);
        assert_eq!(tokens("#NAME?"), [Token::Error(CalcError::Name)]);
        assert_eq!(tokens("#NULL!"), [Token::Error(CalcError::Null)]);
        assert_eq!(tokens("#NUM!"), [Token::Error(CalcError::Num)]);
        assert_eq!(tokens("#VALUE!"), [Token::Error(CalcError::Value)]);
        assert_eq!(tokens("#SPILL!"), [Token::Error(CalcError::Spill)]);
    }

    #[test]
    fn an_unknown_hash_word_is_rejected() {
        assert_eq!(
            tokenize("#NOPE!").unwrap_err().kind,
            LexErrorKind::UnknownError
        );
    }

    #[test]
    fn references_keep_their_dollar_markers_in_one_word() {
        assert_eq!(tokens("$A$1"), [Token::Word("$A$1".to_string())]);
        assert_eq!(
            tokens("XFD1048576"),
            [Token::Word("XFD1048576".to_string())]
        );
    }

    #[test]
    fn function_names_may_contain_dots_and_underscores() {
        assert_eq!(tokens("NORM.DIST"), [Token::Word("NORM.DIST".to_string())]);
        assert_eq!(
            tokens("_xlfn.XLOOKUP"),
            [Token::Word("_xlfn.XLOOKUP".to_string())]
        );
    }

    #[test]
    fn two_character_comparisons_beat_their_prefixes() {
        assert_eq!(tokens("<="), [Token::LessEqual]);
        assert_eq!(tokens(">="), [Token::GreaterEqual]);
        assert_eq!(tokens("<>"), [Token::NotEqual]);
        assert_eq!(tokens("<"), [Token::Less]);
        assert_eq!(tokens(">"), [Token::Greater]);
    }

    #[test]
    fn whitespace_survives_as_a_token() {
        assert_eq!(
            tokens("A1 B2"),
            [
                Token::Word("A1".to_string()),
                Token::Space,
                Token::Word("B2".to_string())
            ]
        );
        // A run collapses to one token.
        assert_eq!(
            tokens("A1   B2"),
            [
                Token::Word("A1".to_string()),
                Token::Space,
                Token::Word("B2".to_string())
            ]
        );
    }

    #[test]
    fn a_sign_cannot_begin_an_operand_after_a_space() {
        // This is what makes `=A1 -B1` a subtraction.
        assert!(!Token::Minus.can_begin_operand());
        assert!(!Token::Plus.can_begin_operand());
        assert!(Token::Word("B1".to_string()).can_begin_operand());
        assert!(Token::Number(1.0).can_begin_operand());
        assert!(Token::OpenParen.can_begin_operand());
    }

    #[test]
    fn quoted_sheet_names_collapse_doubled_quotes() {
        assert_eq!(
            tokens("'My Sheet'!A1"),
            [
                Token::QuotedName("My Sheet".to_string()),
                Token::Bang,
                Token::Word("A1".to_string())
            ]
        );
        assert_eq!(
            tokens("'It''s'!A1"),
            [
                Token::QuotedName("It's".to_string()),
                Token::Bang,
                Token::Word("A1".to_string())
            ]
        );
    }

    #[test]
    fn spans_point_at_the_original_text() {
        let spans = tokenize("1+23").unwrap();
        assert_eq!((spans[0].start, spans[0].end), (0, 1));
        assert_eq!((spans[1].start, spans[1].end), (1, 2));
        assert_eq!((spans[2].start, spans[2].end), (2, 4));
    }

    #[test]
    fn a_stray_character_is_rejected_with_its_position() {
        let err = tokenize("1 ~ 2").unwrap_err();
        assert_eq!(err.kind, LexErrorKind::UnexpectedCharacter('~'));
        assert_eq!(err.at, 2);
    }

    #[test]
    fn a_whole_formula_tokenises() {
        assert_eq!(
            tokens("SUM(A1:B2,3)*-1%"),
            [
                Token::Word("SUM".to_string()),
                Token::OpenParen,
                Token::Word("A1".to_string()),
                Token::Colon,
                Token::Word("B2".to_string()),
                Token::Comma,
                Token::Number(3.0),
                Token::CloseParen,
                Token::Star,
                Token::Minus,
                Token::Number(1.0),
                Token::Percent,
            ]
        );
    }
}
