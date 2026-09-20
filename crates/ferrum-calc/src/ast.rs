//! The shape a parsed formula takes.
//!
//! The tree keeps what the author wrote, including the `$` markers and the
//! distinction between a range written `A1:B2` and one computed by an
//! expression. That fidelity is what lets a formula be rewritten when it is
//! copied, and printed back in the form it was typed.

use ferrum_core::{A1Ref, Value};

/// A parsed formula.
#[derive(Clone, PartialEq, Debug)]
pub enum Expr {
    Literal(Value),

    /// A reference written directly, optionally qualified by a sheet.
    Ref {
        sheet: Option<SheetRef>,
        target: RefTarget,
    },

    /// A defined name, optionally qualified by a sheet.
    Name {
        sheet: Option<SheetRef>,
        name: String,
    },

    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// The postfix `%`, which divides by a hundred.
    Percent(Box<Expr>),

    Call {
        name: String,
        args: Vec<Expr>,
    },

    /// A literal array, `{1,2;3,4}`, in row-major order.
    Array(Vec<Vec<Expr>>),

    /// `a:b` where the endpoints are not both plain references, as in
    /// `INDEX(A:A,1):B5`. The simple case collapses into [`Expr::Ref`] at
    /// parse time instead.
    RangeOp {
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Two references separated by a space: the cells they share.
    Intersect {
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Comma-separated references inside parentheses, as in `(A1:A2,C1:C2)`.
    Union(Vec<Expr>),
}

/// The sheet part of a qualified reference.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SheetRef {
    One(String),
    /// A span across consecutive sheets, `Sheet1:Sheet3!A1`.
    Span {
        first: String,
        last: String,
    },
}

/// What a written reference points at.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RefTarget {
    Cell(A1Ref),
    Range {
        start: A1Ref,
        end: A1Ref,
    },
    /// Whole columns, `B:D`.
    Columns {
        first: Bound,
        last: Bound,
    },
    /// Whole rows, `2:5`.
    Rows {
        first: Bound,
        last: Bound,
    },
    /// `#REF!` written in the formula, usually left by a deletion.
    Invalid,
}

/// One edge of a whole-column or whole-row range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bound {
    /// Zero-based column or row index.
    pub index: u32,
    /// Whether the author pinned it with `$`.
    pub absolute: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    Negate,
    /// A leading `+`. It coerces to a number, so it is not a no-op.
    Plus,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
    Concat,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl BinaryOp {
    /// True for the six comparisons, which yield a logical rather than a
    /// value of the operands' kind.
    pub const fn is_comparison(self) -> bool {
        matches!(
            self,
            Self::Equal
                | Self::NotEqual
                | Self::Less
                | Self::LessOrEqual
                | Self::Greater
                | Self::GreaterOrEqual
        )
    }

    /// How tightly this operator binds. Larger wins.
    pub const fn binding_power(self) -> u8 {
        match self {
            Self::Equal
            | Self::NotEqual
            | Self::Less
            | Self::LessOrEqual
            | Self::Greater
            | Self::GreaterOrEqual => 1,
            Self::Concat => 2,
            Self::Add | Self::Subtract => 3,
            Self::Multiply | Self::Divide => 4,
            Self::Power => 5,
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Power => "^",
            Self::Concat => "&",
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
        }
    }
}
