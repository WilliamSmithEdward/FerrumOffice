//! Text manipulation.
//!
//! Positions and lengths count **characters**, one-based. Counting by
//! character rather than by encoding unit means an emoji or an accented
//! letter is one position, which is what a person reading the cell expects.

use ferrum_core::{CalcError, Value, compare_text};

use crate::eval::Ctx;
use crate::functions::stats::wildcard_matches;
use crate::functions::{finish, int_arg, logical_arg, text_arg};
use crate::operand::Operand;

/// Longest string a cell will hold, which caps what `REPT` may build.
const MAX_TEXT_LENGTH: usize = 32_767;

pub(super) fn len(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(text_arg(ctx, &args[0]).map(|s| Value::number(s.chars().count() as f64)))
}

pub(super) fn upper(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(text_arg(ctx, &args[0]).map(|s| Value::text(s.to_uppercase())))
}

pub(super) fn lower(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(text_arg(ctx, &args[0]).map(|s| Value::text(s.to_lowercase())))
}

/// Capitalise the first letter of every word, lowering the rest.
///
/// A word starts after anything that is not a letter, so `o'neill` becomes
/// `O'Neill` and `3rd` stays `3Rd`, which is what a spreadsheet does.
pub(super) fn proper(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(text_arg(ctx, &args[0]).map(|s| {
        let mut out = String::with_capacity(s.len());
        let mut starting = true;
        for c in s.chars() {
            if starting {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            starting = !c.is_alphabetic();
        }
        Value::text(out)
    }))
}

/// Strip the ends and collapse internal runs of spaces to one.
pub(super) fn trim(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish(text_arg(ctx, &args[0]).map(|s| {
        let collapsed: Vec<&str> = s.split(' ').filter(|part| !part.is_empty()).collect();
        Value::text(collapsed.join(" "))
    }))
}

pub(super) fn left(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        let count = optional_count(ctx, args.get(1))?;
        Ok(Value::text(text.chars().take(count).collect::<String>()))
    })())
}

pub(super) fn right(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        let count = optional_count(ctx, args.get(1))?;
        let total = text.chars().count();
        Ok(Value::text(
            text.chars()
                .skip(total.saturating_sub(count))
                .collect::<String>(),
        ))
    })())
}

fn optional_count(ctx: &Ctx, arg: Option<&Operand>) -> Result<usize, CalcError> {
    let count = match arg {
        None => 1,
        Some(operand) => int_arg(ctx, operand)?,
    };
    if count < 0 {
        return Err(CalcError::Value);
    }
    Ok(count as usize)
}

/// `MID(text, start, count)`, with `start` counting from one.
pub(super) fn mid(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        let start = int_arg(ctx, &args[1])?;
        let count = int_arg(ctx, &args[2])?;
        if start < 1 || count < 0 {
            return Err(CalcError::Value);
        }
        Ok(Value::text(
            text.chars()
                .skip(start as usize - 1)
                .take(count as usize)
                .collect::<String>(),
        ))
    })())
}

/// `REPT(text, count)`.
pub(super) fn rept(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        let count = int_arg(ctx, &args[1])?;
        if count < 0 {
            return Err(CalcError::Value);
        }
        // Check the size before building it, so a large count reports a fault
        // rather than exhausting memory trying to produce one.
        let length = text.chars().count().saturating_mul(count as usize);
        if length > MAX_TEXT_LENGTH {
            return Err(CalcError::Value);
        }
        Ok(Value::text(text.repeat(count as usize)))
    })())
}

/// `FIND(needle, haystack, [start])`: case-sensitive, no wildcards.
pub(super) fn find(ctx: &Ctx, args: &[Operand]) -> Operand {
    locate(ctx, args, false)
}

/// `SEARCH(needle, haystack, [start])`: case-insensitive, wildcards allowed.
pub(super) fn search(ctx: &Ctx, args: &[Operand]) -> Operand {
    locate(ctx, args, true)
}

fn locate(ctx: &Ctx, args: &[Operand], loose: bool) -> Operand {
    finish((|| {
        let needle = text_arg(ctx, &args[0])?;
        let haystack = text_arg(ctx, &args[1])?;
        let start = match args.get(2) {
            None => 1,
            Some(arg) => int_arg(ctx, arg)?,
        };

        let characters: Vec<char> = haystack.chars().collect();
        if start < 1 || start as usize > characters.len() + 1 {
            return Err(CalcError::Value);
        }
        let from = start as usize - 1;

        // An empty needle is found where the search began.
        if needle.is_empty() {
            return Ok(Value::number(start as f64));
        }

        let needle_chars: Vec<char> = needle.chars().collect();
        for at in from..=characters.len().saturating_sub(needle_chars.len()) {
            let window: String = characters[at..at + needle_chars.len()].iter().collect();
            let hit = if loose {
                compare_text(&window, &needle) == std::cmp::Ordering::Equal
            } else {
                window == needle
            };
            if hit {
                return Ok(Value::number(at as f64 + 1.0));
            }
        }

        // SEARCH also accepts wildcards, which need a scan of every length.
        if loose && (needle.contains('*') || needle.contains('?')) {
            for at in from..characters.len() {
                for end in at..=characters.len() {
                    let window: String = characters[at..end].iter().collect();
                    if wildcard_matches(&needle, &window) {
                        return Ok(Value::number(at as f64 + 1.0));
                    }
                }
            }
        }

        Err(CalcError::Value)
    })())
}

/// `SUBSTITUTE(text, old, new, [instance])`, case-sensitive.
///
/// Without `instance` every occurrence is replaced; with it, only the nth.
pub(super) fn substitute(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        let old = text_arg(ctx, &args[1])?;
        let new = text_arg(ctx, &args[2])?;

        if old.is_empty() {
            return Ok(Value::text(text));
        }

        let Some(arg) = args.get(3) else {
            return Ok(Value::text(text.replace(&old, &new)));
        };

        let wanted = int_arg(ctx, arg)?;
        if wanted < 1 {
            return Err(CalcError::Value);
        }

        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        let mut seen = 0i64;
        while let Some(at) = rest.find(&old) {
            seen += 1;
            out.push_str(&rest[..at]);
            if seen == wanted {
                out.push_str(&new);
            } else {
                out.push_str(&old);
            }
            rest = &rest[at + old.len()..];
        }
        out.push_str(rest);
        Ok(Value::text(out))
    })())
}

/// `REPLACE(text, start, count, new)`, by character position.
pub(super) fn replace(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        let start = int_arg(ctx, &args[1])?;
        let count = int_arg(ctx, &args[2])?;
        let new = text_arg(ctx, &args[3])?;
        if start < 1 || count < 0 {
            return Err(CalcError::Value);
        }
        let characters: Vec<char> = text.chars().collect();
        let from = (start as usize - 1).min(characters.len());
        let to = from.saturating_add(count as usize).min(characters.len());
        let mut out: String = characters[..from].iter().collect();
        out.push_str(&new);
        out.extend(&characters[to..]);
        Ok(Value::text(out))
    })())
}

/// Case-sensitive equality, which the `=` operator does not provide.
pub(super) fn exact(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let a = text_arg(ctx, &args[0])?;
        let b = text_arg(ctx, &args[1])?;
        Ok(Value::Logical(a == b))
    })())
}

/// `CHAR(code)` for a code point.
pub(super) fn char_of(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let code = int_arg(ctx, &args[0])?;
        let code = u32::try_from(code).map_err(|_| CalcError::Value)?;
        char::from_u32(code)
            .map(|c| Value::text(c.to_string()))
            .ok_or(CalcError::Value)
    })())
}

/// `CODE(text)`: the code point of the first character.
pub(super) fn code(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        text.chars()
            .next()
            .map(|c| Value::number(f64::from(c as u32)))
            .ok_or(CalcError::Value)
    })())
}

/// `VALUE(text)`: read text as a number.
pub(super) fn value(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let text = text_arg(ctx, &args[0])?;
        ferrum_core::value::parse_number(&text)
            .map(Value::number)
            .ok_or(CalcError::Value)
    })())
}

/// `CONCAT(...)`: join everything, ranges included.
pub(super) fn concat(ctx: &Ctx, args: &[Operand]) -> Operand {
    let mut out = String::new();
    for arg in args {
        for value in ctx.flatten(arg) {
            match value.to_text() {
                Ok(text) => out.push_str(&text),
                Err(e) => return Operand::error(e),
            }
            if out.len() > MAX_TEXT_LENGTH {
                return Operand::error(CalcError::Value);
            }
        }
    }
    Operand::text(out)
}

/// `TEXTJOIN(delimiter, ignore_empty, ...)`.
pub(super) fn textjoin(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let delimiter = text_arg(ctx, &args[0])?;
        let skip_empty = logical_arg(ctx, &args[1])?;

        let mut parts: Vec<String> = Vec::new();
        for arg in &args[2..] {
            for value in ctx.flatten(arg) {
                let text = value.to_text()?;
                if skip_empty && text.is_empty() {
                    continue;
                }
                parts.push(text);
            }
        }

        let joined = parts.join(&delimiter);
        if joined.len() > MAX_TEXT_LENGTH {
            return Err(CalcError::Value);
        }
        Ok(Value::text(joined))
    })())
}
