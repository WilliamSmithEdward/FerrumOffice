//! Aggregates, and the conditional forms of them.

use std::cmp::Ordering;

use ferrum_core::{CalcError, CellAddr, RangeAddr, Value, compare_text};

use crate::eval::Ctx;
use crate::functions::{finish, int_arg, numbers_of};
use crate::operand::Operand;

pub(super) fn sum(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(numbers_of(ctx, args).map(|ns| Value::number(ns.iter().sum())))
}

pub(super) fn sumsq(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(numbers_of(ctx, args).map(|ns| Value::number(ns.iter().map(|n| n * n).sum())))
}

pub(super) fn product(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(numbers_of(ctx, args).map(|ns| {
        // No numbers at all gives zero rather than the empty product.
        if ns.is_empty() {
            Value::Number(0.0)
        } else {
            Value::number(ns.iter().product())
        }
    }))
}

pub(super) fn average(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let ns = numbers_of(ctx, args)?;
        if ns.is_empty() {
            return Err(CalcError::Div0);
        }
        Ok(Value::number(ns.iter().sum::<f64>() / ns.len() as f64))
    })())
}

pub(super) fn min(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(numbers_of(ctx, args).map(|ns| extreme(&ns, Ordering::Less)))
}

pub(super) fn max(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(numbers_of(ctx, args).map(|ns| extreme(&ns, Ordering::Greater)))
}

/// The smallest or largest of a set, which is zero when the set is empty.
fn extreme(numbers: &[f64], wanted: Ordering) -> Value {
    let mut best: Option<f64> = None;
    for &n in numbers {
        best = Some(match best {
            None => n,
            Some(current) => {
                if n.partial_cmp(&current) == Some(wanted) {
                    n
                } else {
                    current
                }
            }
        });
    }
    Value::number(best.unwrap_or(0.0))
}

pub(super) fn median(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let mut ns = numbers_of(ctx, args)?;
        if ns.is_empty() {
            return Err(CalcError::Num);
        }
        ns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let middle = ns.len() / 2;
        Ok(Value::number(if ns.len() % 2 == 1 {
            ns[middle]
        } else {
            (ns[middle - 1] + ns[middle]) / 2.0
        }))
    })())
}

/// `LARGE(values, k)`: the kth largest, counting from one.
pub(super) fn large(ctx: &Ctx, args: &[Operand]) -> Operand {
    nth_ranked(ctx, args, true)
}

/// `SMALL(values, k)`: the kth smallest, counting from one.
pub(super) fn small(ctx: &Ctx, args: &[Operand]) -> Operand {
    nth_ranked(ctx, args, false)
}

fn nth_ranked(ctx: &Ctx, args: &[Operand], largest: bool) -> Operand {
    finish((|| {
        let mut ns = numbers_of(ctx, &args[..1])?;
        let k = int_arg(ctx, &args[1])?;
        if ns.is_empty() || k < 1 || k as usize > ns.len() {
            return Err(CalcError::Num);
        }
        ns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let index = if largest {
            ns.len() - k as usize
        } else {
            k as usize - 1
        };
        Ok(Value::number(ns[index]))
    })())
}

/// Sum of squared deviations from the mean, shared by the variance pair.
fn sum_of_squares(numbers: &[f64]) -> f64 {
    let mean = numbers.iter().sum::<f64>() / numbers.len() as f64;
    numbers.iter().map(|n| (n - mean) * (n - mean)).sum()
}

fn variance(ctx: &Ctx, args: &[Operand], sample: bool) -> Result<f64, CalcError> {
    let ns = numbers_of(ctx, args)?;
    let divisor = if sample {
        ns.len() as i64 - 1
    } else {
        ns.len() as i64
    };
    if divisor < 1 {
        return Err(CalcError::Div0);
    }
    Ok(sum_of_squares(&ns) / divisor as f64)
}

pub(super) fn var_s(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(variance(ctx, args, true).map(Value::number))
}

pub(super) fn var_p(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(variance(ctx, args, false).map(Value::number))
}

pub(super) fn stdev_s(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(variance(ctx, args, true).map(|v| Value::number(v.sqrt())))
}

pub(super) fn stdev_p(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(variance(ctx, args, false).map(|v| Value::number(v.sqrt())))
}

pub(super) fn count(ctx: &Ctx, args: &[Operand]) -> Operand {
    let mut total = 0u64;
    for arg in args {
        match arg {
            // A value written directly counts if it can be read as a number.
            Operand::Value(value) => {
                if matches!(value, Value::Number(_)) || value.to_number().is_ok() {
                    total += 1;
                }
            }
            indirect => {
                total += ctx
                    .flatten(indirect)
                    .iter()
                    .filter(|v| matches!(v, Value::Number(_)))
                    .count() as u64;
            }
        }
    }
    Operand::number(total as f64)
}

pub(super) fn counta(ctx: &Ctx, args: &[Operand]) -> Operand {
    let mut total = 0u64;
    for arg in args {
        match arg {
            // An argument written directly is present whatever it holds.
            Operand::Value(Value::Blank) => {}
            Operand::Value(_) => total += 1,
            indirect => {
                total += ctx
                    .flatten(indirect)
                    .iter()
                    .filter(|v| !matches!(v, Value::Blank))
                    .count() as u64;
            }
        }
    }
    Operand::number(total as f64)
}

/// `COUNTBLANK(range)`.
///
/// Counts the whole addressed area rather than the populated part, so a whole
/// column answers with the million cells it really addresses. The populated
/// cells are the only ones that need visiting.
pub(super) fn countblank(ctx: &Ctx, args: &[Operand]) -> Operand {
    let Operand::Reference(reference) = &args[0] else {
        // A literal argument is blank or it is not.
        let value = ctx.to_scalar(args[0].clone());
        return Operand::number(f64::from(u8::from(matches!(value, Value::Blank))));
    };
    let addressed = reference.area();
    let populated = ctx
        .reference_values(reference)
        .iter()
        .filter(|v| !matches!(v, Value::Blank))
        .count() as u64;
    Operand::number((addressed - populated.min(addressed)) as f64)
}

pub(super) fn sumif(ctx: &Ctx, args: &[Operand]) -> Operand {
    conditional(ctx, args, Conditional::Sum)
}

pub(super) fn averageif(ctx: &Ctx, args: &[Operand]) -> Operand {
    conditional(ctx, args, Conditional::Average)
}

pub(super) fn countif(ctx: &Ctx, args: &[Operand]) -> Operand {
    conditional(ctx, args, Conditional::Count)
}

#[derive(Clone, Copy, PartialEq)]
enum Conditional {
    Sum,
    Average,
    Count,
}

/// The shared body of `SUMIF`, `AVERAGEIF` and `COUNTIF`.
///
/// The value tested and the value totalled can come from different ranges, so
/// each matching cell is located by its offset from the tested range's own
/// top-left corner. Taking the offset from the clipped range instead would
/// shift every answer the moment the first rows of a column were empty.
fn conditional(ctx: &Ctx, args: &[Operand], mode: Conditional) -> Operand {
    let Operand::Reference(tested) = &args[0] else {
        return Operand::error(CalcError::Value);
    };
    let Some(tested_area) = tested.as_single_area() else {
        return Operand::error(CalcError::Value);
    };

    let criterion = match Criterion::parse(&ctx.to_scalar(args[1].clone())) {
        Ok(c) => c,
        Err(e) => return Operand::error(e),
    };

    // The third argument, where there is one, says where the totalled values
    // live. Without it they are the tested values themselves.
    let totalled_area: Option<RangeAddr> = match args.get(2) {
        None => None,
        Some(Operand::Reference(r)) => match r.as_single_area() {
            Some(area) => Some(area),
            None => return Operand::error(CalcError::Value),
        },
        Some(_) => return Operand::error(CalcError::Value),
    };

    let mut matched = 0u64;
    let mut total = 0.0;

    for area in ctx.clipped_areas(tested) {
        for cell in area.range.cells() {
            let value = ctx.cell(CellAddr::new(area.sheet, cell));
            if !criterion.matches(&value) {
                continue;
            }
            matched += 1;

            if mode == Conditional::Count {
                continue;
            }

            let contribution = match totalled_area {
                None => value,
                Some(target) => {
                    let row_offset = cell.row - tested_area.range.start.row;
                    let col_offset = cell.col - tested_area.range.start.col;
                    let Some(moved) = target
                        .range
                        .start
                        .offset(i64::from(row_offset), i64::from(col_offset))
                    else {
                        continue;
                    };
                    ctx.cell(CellAddr::new(target.sheet, moved))
                }
            };

            match contribution {
                Value::Number(n) => total += n,
                Value::Error(e) => return Operand::error(e),
                // Text and logicals contribute nothing, as in any aggregate.
                _ => {}
            }
        }
    }

    match mode {
        Conditional::Count => Operand::number(matched as f64),
        Conditional::Sum => Operand::number(total),
        Conditional::Average => {
            if matched == 0 {
                Operand::error(CalcError::Div0)
            } else {
                Operand::number(total / matched as f64)
            }
        }
    }
}

/// `SUMPRODUCT(a, b, ...)`: multiply matching positions, then add.
pub(super) fn sumproduct(ctx: &Ctx, args: &[Operand]) -> Operand {
    let mut arrays = Vec::with_capacity(args.len());
    for arg in args {
        match ctx.to_array(arg) {
            Ok(array) => arrays.push(array),
            Err(e) => return Operand::error(e),
        }
    }

    let first = &arrays[0];
    let (rows, cols) = (first.rows(), first.cols());
    if arrays.iter().any(|a| a.rows() != rows || a.cols() != cols) {
        return Operand::error(CalcError::Value);
    }

    let mut total = 0.0;
    for row in 0..rows {
        for col in 0..cols {
            let mut product = 1.0;
            for array in &arrays {
                match array.get(row, col) {
                    Some(Value::Number(n)) => product *= n,
                    Some(Value::Error(e)) => return Operand::error(*e),
                    // Anything that is not a number counts as zero here.
                    _ => {
                        product = 0.0;
                    }
                }
            }
            total += product;
        }
    }
    Operand::number(total)
}

/// A test written as a criteria argument: `">5"`, `"<>x"`, `"a*"`, or a plain
/// value to match.
pub(crate) struct Criterion {
    comparison: Comparison,
    target: Value,
    /// Set when the target is text holding a wildcard.
    pattern: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl Criterion {
    pub(crate) fn parse(value: &Value) -> Result<Self, CalcError> {
        if let Value::Error(e) = value {
            return Err(*e);
        }

        let Value::Text(text) = value else {
            // A number or a logical matches by equality.
            return Ok(Self {
                comparison: Comparison::Equal,
                target: value.clone(),
                pattern: None,
            });
        };

        // Longest operator first, or ">=" would read as ">".
        const OPERATORS: &[(&str, Comparison)] = &[
            (">=", Comparison::GreaterOrEqual),
            ("<=", Comparison::LessOrEqual),
            ("<>", Comparison::NotEqual),
            (">", Comparison::Greater),
            ("<", Comparison::Less),
            ("=", Comparison::Equal),
        ];

        let (comparison, rest) = OPERATORS
            .iter()
            .find_map(|(prefix, op)| text.strip_prefix(prefix).map(|rest| (*op, rest)))
            .unwrap_or((Comparison::Equal, text.as_ref()));

        // A comparison against something numeric is numeric.
        let target = match ferrum_core::value::parse_number(rest) {
            Some(n) => Value::Number(n),
            None if rest.is_empty() => Value::Blank,
            None => Value::text(rest),
        };

        let pattern = matches!(comparison, Comparison::Equal | Comparison::NotEqual)
            .then(|| rest.to_string())
            .filter(|s| s.contains('*') || s.contains('?'));

        Ok(Self {
            comparison,
            target,
            pattern,
        })
    }

    pub(crate) fn matches(&self, value: &Value) -> bool {
        if let Some(pattern) = &self.pattern {
            let Ok(text) = value.to_text() else {
                return false;
            };
            let hit = wildcard_matches(pattern, &text);
            return if self.comparison == Comparison::NotEqual {
                !hit
            } else {
                hit
            };
        }

        // A blank cell is not zero for this purpose, or every empty cell in a
        // column would satisfy "=0".
        if matches!(value, Value::Blank) && !matches!(self.target, Value::Blank) {
            return self.comparison == Comparison::NotEqual;
        }

        let Ok(order) = value.compare(&self.target) else {
            return false;
        };
        match self.comparison {
            Comparison::Equal => order.is_eq(),
            Comparison::NotEqual => order.is_ne(),
            Comparison::Less => order.is_lt(),
            Comparison::LessOrEqual => order.is_le(),
            Comparison::Greater => order.is_gt(),
            Comparison::GreaterOrEqual => order.is_ge(),
        }
    }
}

/// Match text against a pattern where `*` stands for any run, `?` for one
/// character, and `~` escapes either. Case is ignored.
pub(crate) fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    // Iterative backtracking, so a pattern full of stars cannot blow the
    // stack or take exponential time.
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut resume) = (None, 0usize);

    while t < text.len() {
        let literal = match pattern.get(p) {
            Some('*') => {
                star = Some(p);
                p += 1;
                resume = t;
                continue;
            }
            Some('?') => {
                p += 1;
                t += 1;
                continue;
            }
            Some('~') => pattern.get(p + 1).copied(),
            Some(c) => Some(*c),
            None => None,
        };

        let escaped = matches!(pattern.get(p), Some('~')) && literal.is_some();
        let matched = literal
            .is_some_and(|c| compare_text(&c.to_string(), &text[t].to_string()) == Ordering::Equal);

        if matched {
            p += if escaped { 2 } else { 1 };
            t += 1;
        } else if let Some(at) = star {
            // Give the last star one more character and try again.
            p = at + 1;
            resume += 1;
            t = resume;
        } else {
            return false;
        }
    }

    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_the_way_a_spreadsheet_expects() {
        assert!(wildcard_matches("a*", "apple"));
        assert!(wildcard_matches("*e", "apple"));
        assert!(wildcard_matches("a*e", "apple"));
        assert!(wildcard_matches("?pple", "apple"));
        assert!(wildcard_matches("*", "anything"));
        assert!(wildcard_matches("apple", "APPLE"));
        assert!(!wildcard_matches("a?", "apple"));
        assert!(!wildcard_matches("b*", "apple"));
        assert!(wildcard_matches("", ""));
        assert!(!wildcard_matches("", "x"));
    }

    #[test]
    fn a_tilde_escapes_a_wildcard() {
        assert!(wildcard_matches("a~*b", "a*b"));
        assert!(!wildcard_matches("a~*b", "axb"));
        assert!(wildcard_matches("a~?b", "a?b"));
    }

    #[test]
    fn many_stars_do_not_take_forever() {
        // The pathological case for a naive recursive matcher.
        let pattern = "*a*a*a*a*a*a*a*a*b";
        let text = "a".repeat(64);
        assert!(!wildcard_matches(pattern, &text));
    }

    #[test]
    fn criteria_parse_their_operator() {
        let greater = Criterion::parse(&Value::text(">5")).unwrap();
        assert!(greater.matches(&Value::Number(6.0)));
        assert!(!greater.matches(&Value::Number(5.0)));

        let not_equal = Criterion::parse(&Value::text("<>x")).unwrap();
        assert!(not_equal.matches(&Value::text("y")));
        assert!(!not_equal.matches(&Value::text("x")));

        let plain = Criterion::parse(&Value::Number(3.0)).unwrap();
        assert!(plain.matches(&Value::Number(3.0)));
        assert!(!plain.matches(&Value::Number(4.0)));
    }

    #[test]
    fn a_blank_cell_does_not_satisfy_a_zero_test() {
        let zero = Criterion::parse(&Value::text("=0")).unwrap();
        assert!(zero.matches(&Value::Number(0.0)));
        assert!(!zero.matches(&Value::Blank));
    }
}
