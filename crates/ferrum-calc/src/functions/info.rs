//! Asking what a value is.
//!
//! Every function here is registered as inspecting rather than eager, so an
//! error argument arrives intact instead of short-circuiting the call. That
//! is the whole point of `ISERROR`.

use ferrum_core::{CalcError, Value};

use crate::eval::Ctx;
use crate::operand::Operand;

/// The value an inspecting function sees, after a reference is read but
/// before anything is coerced.
fn inspected(ctx: &Ctx, arg: &Operand) -> Value {
    ctx.to_scalar(arg.clone())
}

pub(super) fn isblank(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(matches!(inspected(ctx, &args[0]), Value::Blank))
}

pub(super) fn isnumber(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(matches!(inspected(ctx, &args[0]), Value::Number(_)))
}

pub(super) fn istext(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(matches!(inspected(ctx, &args[0]), Value::Text(_)))
}

/// True for anything that is not text, blanks included.
pub(super) fn isnontext(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(!matches!(inspected(ctx, &args[0]), Value::Text(_)))
}

pub(super) fn islogical(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(matches!(inspected(ctx, &args[0]), Value::Logical(_)))
}

/// True for any error at all.
pub(super) fn iserror(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(inspected(ctx, &args[0]).is_error())
}

/// True for every error except `#N/A`, which is the pair's whole reason for
/// existing: a missing lookup is usually expected, a fault is not.
pub(super) fn iserr(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(matches!(
        inspected(ctx, &args[0]),
        Value::Error(e) if e != CalcError::NotAvailable
    ))
}

pub(super) fn isna(ctx: &Ctx, args: &[Operand]) -> Operand {
    Operand::logical(inspected(ctx, &args[0]).as_error() == Some(CalcError::NotAvailable))
}

/// `NA()`: the deliberate absence of a value.
pub(super) fn na(_ctx: &Ctx, _args: &[Operand]) -> Operand {
    Operand::error(CalcError::NotAvailable)
}

/// `ERROR.TYPE(value)`: the ordinal of an error, or `#N/A` for a non-error.
pub(super) fn error_type(ctx: &Ctx, args: &[Operand]) -> Operand {
    match inspected(ctx, &args[0])
        .as_error()
        .and_then(CalcError::error_type)
    {
        Some(ordinal) => Operand::number(f64::from(ordinal)),
        None => Operand::error(CalcError::NotAvailable),
    }
}

/// `TYPE(value)`: 1 number, 2 text, 4 logical, 16 error, 64 array.
pub(super) fn type_of(ctx: &Ctx, args: &[Operand]) -> Operand {
    // An array answers 64 whatever it holds, so the check comes first.
    if matches!(args[0], Operand::Array(_)) {
        return Operand::number(64.0);
    }
    let code = match inspected(ctx, &args[0]) {
        // An empty cell reports as a number, which is consistent with a blank
        // counting as zero everywhere else.
        Value::Number(_) | Value::Blank => 1,
        Value::Text(_) => 2,
        Value::Logical(_) => 4,
        Value::Error(_) => 16,
    };
    Operand::number(f64::from(code))
}
