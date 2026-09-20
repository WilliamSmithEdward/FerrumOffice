//! The function library, and the table that dispatches to it.
//!
//! Functions are looked up in a sorted static table by binary search. A test
//! keeps the table sorted and free of duplicates, so adding one in the wrong
//! place fails immediately rather than making a name unreachable.

mod info;
mod logic;
mod lookup;
mod math;
mod stats;
mod text;

use ferrum_core::{CalcError, Value};

use crate::ast::Expr;
use crate::eval::Ctx;
use crate::operand::Operand;

/// How a function receives its arguments.
#[derive(Clone, Copy)]
pub enum Dispatch {
    /// Arguments are evaluated first. Almost everything.
    Eager(fn(&Ctx, &[Operand]) -> Operand),
    /// Arguments arrive unevaluated, for the few that must not evaluate every
    /// branch: `IF` and the error-catching pair above all.
    Lazy(fn(&Ctx, &[Expr]) -> Operand),
}

pub struct Function {
    /// Upper case, because lookup folds case.
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    /// Whether an error passed as a whole argument short-circuits the call.
    ///
    /// True for almost everything: `SUM(#N/A)` is `#N/A`. False for the
    /// functions whose entire job is to inspect an error.
    pub propagates_errors: bool,
    pub dispatch: Dispatch,
}

/// No practical ceiling on argument count.
const MANY: usize = usize::MAX;

const fn eager(
    name: &'static str,
    min_args: usize,
    max_args: usize,
    call: fn(&Ctx, &[Operand]) -> Operand,
) -> Function {
    Function {
        name,
        min_args,
        max_args,
        propagates_errors: true,
        dispatch: Dispatch::Eager(call),
    }
}

/// An eager function that wants to see an error argument rather than
/// propagate it.
const fn inspecting(
    name: &'static str,
    min_args: usize,
    max_args: usize,
    call: fn(&Ctx, &[Operand]) -> Operand,
) -> Function {
    Function {
        name,
        min_args,
        max_args,
        propagates_errors: false,
        dispatch: Dispatch::Eager(call),
    }
}

const fn lazy(
    name: &'static str,
    min_args: usize,
    max_args: usize,
    call: fn(&Ctx, &[Expr]) -> Operand,
) -> Function {
    Function {
        name,
        min_args,
        max_args,
        propagates_errors: false,
        dispatch: Dispatch::Lazy(call),
    }
}

/// Every function, sorted by name.
static TABLE: &[Function] = &[
    eager("ABS", 1, 1, math::abs),
    eager("ACOS", 1, 1, math::acos),
    eager("AND", 1, MANY, logic::and),
    eager("ASIN", 1, 1, math::asin),
    eager("ATAN", 1, 1, math::atan),
    eager("ATAN2", 2, 2, math::atan2),
    eager("AVERAGE", 1, MANY, stats::average),
    eager("AVERAGEIF", 2, 3, stats::averageif),
    eager("CEILING", 2, 2, math::ceiling),
    eager("CHAR", 1, 1, text::char_of),
    lazy("CHOOSE", 2, MANY, logic::choose),
    eager("CODE", 1, 1, text::code),
    eager("COLUMN", 0, 1, lookup::column),
    eager("COLUMNS", 1, 1, lookup::columns),
    eager("CONCAT", 1, MANY, text::concat),
    eager("CONCATENATE", 1, MANY, text::concat),
    eager("COS", 1, 1, math::cos),
    eager("COUNT", 1, MANY, stats::count),
    eager("COUNTA", 1, MANY, stats::counta),
    eager("COUNTBLANK", 1, 1, stats::countblank),
    eager("COUNTIF", 2, 2, stats::countif),
    eager("DEGREES", 1, 1, math::degrees),
    inspecting("ERROR.TYPE", 1, 1, info::error_type),
    eager("EVEN", 1, 1, math::even),
    eager("EXACT", 2, 2, text::exact),
    eager("EXP", 1, 1, math::exp),
    eager("FIND", 2, 3, text::find),
    eager("FLOOR", 2, 2, math::floor),
    eager("HLOOKUP", 3, 4, lookup::hlookup),
    lazy("IF", 2, 3, logic::if_),
    lazy("IFERROR", 2, 2, logic::iferror),
    lazy("IFNA", 2, 2, logic::ifna),
    lazy("IFS", 2, MANY, logic::ifs),
    eager("INDEX", 2, 3, lookup::index),
    eager("INT", 1, 1, math::int),
    inspecting("ISBLANK", 1, 1, info::isblank),
    inspecting("ISERR", 1, 1, info::iserr),
    inspecting("ISERROR", 1, 1, info::iserror),
    inspecting("ISLOGICAL", 1, 1, info::islogical),
    inspecting("ISNA", 1, 1, info::isna),
    inspecting("ISNONTEXT", 1, 1, info::isnontext),
    inspecting("ISNUMBER", 1, 1, info::isnumber),
    inspecting("ISTEXT", 1, 1, info::istext),
    eager("LARGE", 2, 2, stats::large),
    eager("LEFT", 1, 2, text::left),
    eager("LEN", 1, 1, text::len),
    eager("LN", 1, 1, math::ln),
    eager("LOG", 1, 2, math::log),
    eager("LOG10", 1, 1, math::log10),
    eager("LOWER", 1, 1, text::lower),
    eager("MATCH", 2, 3, lookup::match_),
    eager("MAX", 1, MANY, stats::max),
    eager("MEDIAN", 1, MANY, stats::median),
    eager("MID", 3, 3, text::mid),
    eager("MIN", 1, MANY, stats::min),
    eager("MOD", 2, 2, math::mod_),
    inspecting("NA", 0, 0, info::na),
    eager("NOT", 1, 1, logic::not),
    eager("ODD", 1, 1, math::odd),
    eager("OR", 1, MANY, logic::or),
    eager("PI", 0, 0, math::pi),
    eager("POWER", 2, 2, math::power),
    eager("PRODUCT", 1, MANY, stats::product),
    eager("PROPER", 1, 1, text::proper),
    eager("RADIANS", 1, 1, math::radians),
    eager("REPLACE", 4, 4, text::replace),
    eager("REPT", 2, 2, text::rept),
    eager("RIGHT", 1, 2, text::right),
    eager("ROUND", 2, 2, math::round),
    eager("ROUNDDOWN", 2, 2, math::rounddown),
    eager("ROUNDUP", 2, 2, math::roundup),
    eager("ROW", 0, 1, lookup::row),
    eager("ROWS", 1, 1, lookup::rows),
    eager("SEARCH", 2, 3, text::search),
    eager("SIGN", 1, 1, math::sign),
    eager("SIN", 1, 1, math::sin),
    eager("SMALL", 2, 2, stats::small),
    eager("SQRT", 1, 1, math::sqrt),
    eager("STDEV.P", 1, MANY, stats::stdev_p),
    eager("STDEV.S", 1, MANY, stats::stdev_s),
    eager("SUBSTITUTE", 3, 4, text::substitute),
    eager("SUM", 1, MANY, stats::sum),
    eager("SUMIF", 2, 3, stats::sumif),
    eager("SUMPRODUCT", 1, MANY, stats::sumproduct),
    eager("SUMSQ", 1, MANY, stats::sumsq),
    eager("TAN", 1, 1, math::tan),
    eager("TEXTJOIN", 3, MANY, text::textjoin),
    eager("TRIM", 1, 1, text::trim),
    eager("TRUNC", 1, 2, math::trunc),
    inspecting("TYPE", 1, 1, info::type_of),
    eager("UPPER", 1, 1, text::upper),
    eager("VALUE", 1, 1, text::value),
    eager("VAR.P", 1, MANY, stats::var_p),
    eager("VAR.S", 1, MANY, stats::var_s),
    eager("VLOOKUP", 3, 4, lookup::vlookup),
    eager("XOR", 1, MANY, logic::xor),
];

/// Find a function by name, folding case.
pub fn lookup(name: &str) -> Option<&'static Function> {
    let wanted = name.to_ascii_uppercase();
    // Strip the compatibility prefix newer functions carry in stored files.
    let wanted = wanted.strip_prefix("_XLFN.").unwrap_or(&wanted);
    TABLE
        .binary_search_by(|f| f.name.cmp(wanted))
        .ok()
        .map(|index| &TABLE[index])
}

/// Whether a name is a function this engine knows.
pub fn is_known(name: &str) -> bool {
    lookup(name).is_some()
}

/// Every function name, for completion and for documentation.
pub fn names() -> impl Iterator<Item = &'static str> {
    TABLE.iter().map(|f| f.name)
}

/// Evaluate a call.
pub fn call(ctx: &Ctx, name: &str, args: &[Expr]) -> Operand {
    let Some(function) = lookup(name) else {
        return Operand::error(CalcError::Name);
    };

    if args.len() < function.min_args || args.len() > function.max_args {
        return Operand::error(CalcError::Value);
    }

    match function.dispatch {
        Dispatch::Lazy(call) => call(ctx, args),
        Dispatch::Eager(call) => {
            let mut evaluated = Vec::with_capacity(args.len());
            for arg in args {
                let operand = ctx.eval(arg);
                if function.propagates_errors
                    && let Some(error) = operand.as_error()
                {
                    return Operand::error(error);
                }
                evaluated.push(operand);
            }
            call(ctx, &evaluated)
        }
    }
}

// Shared argument handling. Every function reaches for these rather than
// re-deriving the coercion rules, so the rules stay in one place.

/// One argument as a number.
pub(crate) fn number_arg(ctx: &Ctx, arg: &Operand) -> Result<f64, CalcError> {
    ctx.to_scalar(arg.clone()).to_number()
}

/// One argument as a number truncated to an integer, which is what the
/// position and count arguments want.
pub(crate) fn int_arg(ctx: &Ctx, arg: &Operand) -> Result<i64, CalcError> {
    let n = number_arg(ctx, arg)?;
    if !n.is_finite() {
        return Err(CalcError::Num);
    }
    Ok(n.trunc() as i64)
}

/// One argument as text.
pub(crate) fn text_arg(ctx: &Ctx, arg: &Operand) -> Result<String, CalcError> {
    ctx.to_scalar(arg.clone()).to_text()
}

/// One argument as a condition.
pub(crate) fn logical_arg(ctx: &Ctx, arg: &Operand) -> Result<bool, CalcError> {
    ctx.to_scalar(arg.clone()).to_logical()
}

/// The numbers an argument contributes to an aggregate.
///
/// The rule a spreadsheet uses is not obvious and catches people out: a value
/// written **directly** as an argument is coerced, so `SUM(TRUE)` is 1 and
/// `SUM("2")` is 2, while text and logicals sitting **inside a range** are
/// skipped, so a column holding `TRUE` adds nothing. Errors propagate from
/// either place.
pub(crate) fn collect_numbers(
    ctx: &Ctx,
    args: &[Operand],
    out: &mut Vec<f64>,
) -> Result<(), CalcError> {
    for arg in args {
        match arg {
            Operand::Value(value) => out.push(value.to_number()?),
            indirect => {
                for value in ctx.flatten(indirect) {
                    match value {
                        Value::Number(n) => out.push(n),
                        Value::Error(e) => return Err(e),
                        // Text, logicals and blanks are not data here.
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

/// Shorthand for the many aggregates that want every number and nothing else.
pub(crate) fn numbers_of(ctx: &Ctx, args: &[Operand]) -> Result<Vec<f64>, CalcError> {
    let mut out = Vec::new();
    collect_numbers(ctx, args, &mut out)?;
    Ok(out)
}

/// Turn a `Result` into an operand, so a function body can use `?`.
pub(crate) fn finish(result: Result<Value, CalcError>) -> Operand {
    match result {
        Ok(value) => Operand::Value(value),
        Err(error) => Operand::error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_unique() {
        for pair in TABLE.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "{} must sort before {}; binary search needs the table ordered",
                pair[0].name,
                pair[1].name
            );
        }
    }

    #[test]
    fn every_name_is_upper_case_and_findable() {
        for function in TABLE {
            assert_eq!(
                function.name,
                function.name.to_ascii_uppercase(),
                "{} should be upper case",
                function.name
            );
            assert!(
                lookup(function.name).is_some(),
                "{} is unreachable",
                function.name
            );
        }
    }

    #[test]
    fn lookup_folds_case_and_strips_the_compatibility_prefix() {
        assert!(lookup("sum").is_some());
        assert!(lookup("SuM").is_some());
        assert!(lookup("_xlfn.SUM").is_some());
        assert!(lookup("NOPE").is_none());
    }

    #[test]
    fn argument_counts_are_sane() {
        for function in TABLE {
            assert!(
                function.min_args <= function.max_args,
                "{} has min above max",
                function.name
            );
        }
    }
}
