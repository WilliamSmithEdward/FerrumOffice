//! Arithmetic, rounding and trigonometry.

use ferrum_core::value::{SIGNIFICANT_DIGITS, round_to_significant_digits};
use ferrum_core::{CalcError, Value};

use crate::eval::Ctx;
use crate::functions::{finish, int_arg, number_arg};
use crate::operand::Operand;

/// Apply a plain one-argument numeric function.
fn unary(ctx: &Ctx, args: &[Operand], f: impl Fn(f64) -> Result<f64, CalcError>) -> Operand {
    finish((|| {
        let n = number_arg(ctx, &args[0])?;
        Ok(Value::number(f(n)?))
    })())
}

pub(super) fn abs(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.abs()))
}

pub(super) fn sign(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| {
        Ok(if n > 0.0 {
            1.0
        } else if n < 0.0 {
            -1.0
        } else {
            0.0
        })
    })
}

pub(super) fn sqrt(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| {
        if n < 0.0 {
            Err(CalcError::Num)
        } else {
            Ok(n.sqrt())
        }
    })
}

pub(super) fn exp(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.exp()))
}

pub(super) fn ln(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| {
        if n <= 0.0 {
            Err(CalcError::Num)
        } else {
            Ok(n.ln())
        }
    })
}

pub(super) fn log10(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| {
        if n <= 0.0 {
            Err(CalcError::Num)
        } else {
            Ok(n.log10())
        }
    })
}

/// `LOG(number, [base])`, base ten by default.
pub(super) fn log(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let n = number_arg(ctx, &args[0])?;
        let base = match args.get(1) {
            None => 10.0,
            Some(arg) => number_arg(ctx, arg)?,
        };
        if n <= 0.0 || base <= 0.0 || base == 1.0 {
            return Err(CalcError::Num);
        }
        Ok(Value::number(n.log(base)))
    })())
}

pub(super) fn power(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let base = number_arg(ctx, &args[0])?;
        let exponent = number_arg(ctx, &args[1])?;
        // Same edge cases as the `^` operator, so the two never disagree.
        Ok(crate::eval::apply_binary(
            crate::ast::BinaryOp::Power,
            &Value::Number(base),
            &Value::Number(exponent),
        ))
    })())
}

/// `MOD(number, divisor)`.
///
/// The result takes the sign of the divisor, not of the dividend, so
/// `MOD(-3, 2)` is 1 rather than -1. That follows from defining it as
/// `n - d * FLOOR(n / d)`, which is what a spreadsheet does and what the
/// remainder operator in most languages does not.
pub(super) fn mod_(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let n = number_arg(ctx, &args[0])?;
        let d = number_arg(ctx, &args[1])?;
        if d == 0.0 {
            return Err(CalcError::Div0);
        }
        Ok(Value::number(n - d * (n / d).floor()))
    })())
}

/// Round towards negative infinity, so `INT(-1.5)` is -2.
pub(super) fn int(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.floor()))
}

/// How a rounding function treats the discarded part.
#[derive(Clone, Copy)]
enum Rounding {
    /// Half away from zero, the schoolbook rule.
    Nearest,
    /// Always away from zero.
    Up,
    /// Always towards zero.
    Down,
}

fn round_at(value: f64, digits: i64, mode: Rounding) -> Result<f64, CalcError> {
    if !value.is_finite() {
        return Err(CalcError::Num);
    }
    let digits = i32::try_from(digits).map_err(|_| CalcError::Num)?;
    // Beyond this the scale factor stops being representable and the answer
    // is the input either way.
    if digits > 300 {
        return Ok(value);
    }
    if digits < -300 {
        return Ok(0.0);
    }

    let factor = 10f64.powi(digits);
    let scaled = value * factor;
    if !scaled.is_finite() {
        return Ok(value);
    }

    // Snap away the binary representation error before deciding, or
    // ROUND(2.675, 2) answers 2.67 because the product lands a hair below the
    // halfway point.
    let scaled = round_to_significant_digits(scaled, SIGNIFICANT_DIGITS);

    let rounded = match mode {
        // Rust rounds halves away from zero, which is the rule wanted here.
        Rounding::Nearest => scaled.round(),
        Rounding::Up => {
            if scaled < 0.0 {
                scaled.floor()
            } else {
                scaled.ceil()
            }
        }
        Rounding::Down => scaled.trunc(),
    };
    Ok(rounded / factor)
}

fn rounding_call(ctx: &Ctx, args: &[Operand], mode: Rounding) -> Operand {
    finish((|| {
        let value = number_arg(ctx, &args[0])?;
        let digits = match args.get(1) {
            None => 0,
            Some(arg) => int_arg(ctx, arg)?,
        };
        Ok(Value::number(round_at(value, digits, mode)?))
    })())
}

pub(super) fn round(ctx: &Ctx, args: &[Operand]) -> Operand {
    rounding_call(ctx, args, Rounding::Nearest)
}

pub(super) fn roundup(ctx: &Ctx, args: &[Operand]) -> Operand {
    rounding_call(ctx, args, Rounding::Up)
}

pub(super) fn rounddown(ctx: &Ctx, args: &[Operand]) -> Operand {
    rounding_call(ctx, args, Rounding::Down)
}

/// `TRUNC(number, [digits])`, which discards rather than rounds.
pub(super) fn trunc(ctx: &Ctx, args: &[Operand]) -> Operand {
    rounding_call(ctx, args, Rounding::Down)
}

/// `CEILING(number, significance)`: away from zero to a multiple.
pub(super) fn ceiling(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let value = number_arg(ctx, &args[0])?;
        let step = number_arg(ctx, &args[1])?;
        if step == 0.0 {
            return Ok(Value::Number(0.0));
        }
        // Rounding away from zero towards a multiple of the opposite sign has
        // no answer.
        if value.signum() != step.signum() && value != 0.0 {
            return Err(CalcError::Num);
        }
        let quotient = round_to_significant_digits(value / step, SIGNIFICANT_DIGITS);
        Ok(Value::number(quotient.ceil() * step))
    })())
}

/// `FLOOR(number, significance)`: towards zero to a multiple.
pub(super) fn floor(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let value = number_arg(ctx, &args[0])?;
        let step = number_arg(ctx, &args[1])?;
        if step == 0.0 {
            return Err(CalcError::Div0);
        }
        if value.signum() != step.signum() && value != 0.0 {
            return Err(CalcError::Num);
        }
        let quotient = round_to_significant_digits(value / step, SIGNIFICANT_DIGITS);
        Ok(Value::number(quotient.floor() * step))
    })())
}

/// Away from zero to the next even integer.
pub(super) fn even(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(to_parity(n, 2.0, 0.0)))
}

/// Away from zero to the next odd integer.
pub(super) fn odd(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(to_parity(n, 2.0, 1.0)))
}

/// Step away from zero to the next integer congruent to `offset` mod `step`.
fn to_parity(value: f64, step: f64, offset: f64) -> f64 {
    if value == 0.0 {
        // Zero is already even; the next odd number away from it is one.
        return offset;
    }
    let sign = value.signum();
    let magnitude = value.abs();
    let steps = ((magnitude - offset) / step).ceil().max(0.0);
    sign * (steps * step + offset)
}

pub(super) fn pi(_ctx: &Ctx, _args: &[Operand]) -> Operand {
    Operand::number(std::f64::consts::PI)
}

pub(super) fn sin(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.sin()))
}

pub(super) fn cos(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.cos()))
}

pub(super) fn tan(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.tan()))
}

pub(super) fn asin(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| {
        if !(-1.0..=1.0).contains(&n) {
            Err(CalcError::Num)
        } else {
            Ok(n.asin())
        }
    })
}

pub(super) fn acos(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| {
        if !(-1.0..=1.0).contains(&n) {
            Err(CalcError::Num)
        } else {
            Ok(n.acos())
        }
    })
}

pub(super) fn atan(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.atan()))
}

/// `ATAN2(x, y)`, which takes its arguments the other way round from the
/// library function of the same name.
pub(super) fn atan2(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let x = number_arg(ctx, &args[0])?;
        let y = number_arg(ctx, &args[1])?;
        if x == 0.0 && y == 0.0 {
            return Err(CalcError::Div0);
        }
        Ok(Value::number(y.atan2(x)))
    })())
}

pub(super) fn degrees(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.to_degrees()))
}

pub(super) fn radians(ctx: &Ctx, args: &[Operand]) -> Operand {
    unary(ctx, args, |n| Ok(n.to_radians()))
}
