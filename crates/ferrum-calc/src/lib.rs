//! Formula parsing and evaluation.
//!
//! The crate knows nothing about how a workbook stores its cells. It asks a
//! [`Resolver`] for whatever it needs, which keeps the engine testable
//! without a document and lets the document model change underneath it.
//!
//! ```text
//! text ->  lexer  -> tokens
//!      ->  parser -> Expr
//!      ->  eval   -> Operand -> Value
//! ```

pub mod ast;
pub mod eval;
pub mod functions;
pub mod lexer;
pub mod operand;
pub mod parser;

pub use ast::{BinaryOp, Expr, RefTarget, SheetRef, UnaryOp};
pub use eval::{Ctx, Resolver};
pub use operand::{Array, Operand, Reference};
pub use parser::{ParseError, ParseErrorKind, parse};
