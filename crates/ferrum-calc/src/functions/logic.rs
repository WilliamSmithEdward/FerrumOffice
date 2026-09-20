//! Conditions, and the functions that choose without evaluating everything.
//!
//! The lazy ones here take unevaluated expressions on purpose. `IF` must not
//! evaluate the branch it does not take, and `IFERROR` must not report an
//! error it was asked to swallow.

use ferrum_core::{CalcError, Value};

use crate::ast::Expr;
use crate::eval::Ctx;
use crate::functions::{finish, logical_arg};
use crate::operand::Operand;

/// `IF(condition, then, [otherwise])`.
///
/// With the third argument omitted, a false condition gives `FALSE`.
pub(super) fn if_(ctx: &Ctx, args: &[Expr]) -> Operand {
    let condition = ctx.eval(&args[0]);
    if let Some(error) = condition.as_error() {
        return Operand::error(error);
    }
    match ctx.to_scalar(condition).to_logical() {
        Err(e) => Operand::error(e),
        Ok(true) => ctx.eval(&args[1]),
        Ok(false) => args
            .get(2)
            .map_or_else(|| Operand::logical(false), |otherwise| ctx.eval(otherwise)),
    }
}

/// `IFS(condition1, value1, condition2, value2, ...)`.
pub(super) fn ifs(ctx: &Ctx, args: &[Expr]) -> Operand {
    if args.len() % 2 != 0 {
        return Operand::error(CalcError::Value);
    }
    for pair in args.chunks_exact(2) {
        let condition = ctx.eval(&pair[0]);
        if let Some(error) = condition.as_error() {
            return Operand::error(error);
        }
        match ctx.to_scalar(condition).to_logical() {
            Err(e) => return Operand::error(e),
            Ok(true) => return ctx.eval(&pair[1]),
            Ok(false) => {}
        }
    }
    // Nothing matched, and there is no else.
    Operand::error(CalcError::NotAvailable)
}

/// `IFERROR(value, fallback)`: the fallback for any error at all.
pub(super) fn iferror(ctx: &Ctx, args: &[Expr]) -> Operand {
    let value = ctx.eval(&args[0]);
    if value.is_error() {
        return ctx.eval(&args[1]);
    }
    value
}

/// `IFNA(value, fallback)`: the fallback for `#N/A` only, so a genuine fault
/// is not hidden along with a missing lookup.
pub(super) fn ifna(ctx: &Ctx, args: &[Expr]) -> Operand {
    let value = ctx.eval(&args[0]);
    if value.as_error() == Some(CalcError::NotAvailable) {
        return ctx.eval(&args[1]);
    }
    value
}

/// `CHOOSE(index, first, second, ...)`, counting from one.
pub(super) fn choose(ctx: &Ctx, args: &[Expr]) -> Operand {
    let index = ctx.eval(&args[0]);
    if let Some(error) = index.as_error() {
        return Operand::error(error);
    }
    let index = match ctx.to_scalar(index).to_number() {
        Ok(n) => n.trunc(),
        Err(e) => return Operand::error(e),
    };
    let choices = &args[1..];
    if index < 1.0 || index as usize > choices.len() {
        return Operand::error(CalcError::Value);
    }
    ctx.eval(&choices[index as usize - 1])
}

pub(super) fn not(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(logical_arg(ctx, &args[0]).map(|b| Value::Logical(!b)))
}

pub(super) fn and(ctx: &Ctx, args: &[Operand]) -> Operand {
    fold_logicals(ctx, args, |acc, b| acc && b, true)
}

pub(super) fn or(ctx: &Ctx, args: &[Operand]) -> Operand {
    fold_logicals(ctx, args, |acc, b| acc || b, false)
}

/// True when an odd number of the conditions are true.
pub(super) fn xor(ctx: &Ctx, args: &[Operand]) -> Operand {
    fold_logicals(ctx, args, |acc, b| acc ^ b, false)
}

/// The shared body of the multi-condition logical functions.
///
/// A value written directly must be readable as a condition, so `AND("x")` is
/// `#VALUE!`. Text and blanks sitting inside a range are skipped instead,
/// because a column of labels beside a column of flags is ordinary. If no
/// condition turns up anywhere, there is nothing to decide and the answer is
/// `#VALUE!`.
fn fold_logicals(
    ctx: &Ctx,
    args: &[Operand],
    combine: impl Fn(bool, bool) -> bool,
    identity: bool,
) -> Operand {
    let mut accumulator = identity;
    let mut seen = false;

    for arg in args {
        match arg {
            Operand::Value(value) => match value.to_logical() {
                Ok(b) => {
                    accumulator = combine(accumulator, b);
                    seen = true;
                }
                Err(e) => return Operand::error(e),
            },
            indirect => {
                for value in ctx.flatten(indirect) {
                    match value {
                        Value::Logical(b) => {
                            accumulator = combine(accumulator, b);
                            seen = true;
                        }
                        Value::Number(n) => {
                            accumulator = combine(accumulator, n != 0.0);
                            seen = true;
                        }
                        Value::Error(e) => return Operand::error(e),
                        // Text and blanks in a range are not conditions.
                        Value::Text(_) | Value::Blank => {}
                    }
                }
            }
        }
    }

    if seen {
        Operand::logical(accumulator)
    } else {
        Operand::error(CalcError::Value)
    }
}
