//! Finding things: position functions, `INDEX`, `MATCH` and the lookup pair.
//!
//! Every position here counts from one, the way the language does, and every
//! offset is measured from the **stated** range rather than from the part of
//! it that happens to hold data. Measuring from the clipped part would shift
//! every answer the moment a column's first rows were empty.

use std::cmp::Ordering;

use ferrum_core::{CalcError, CellAddr, CellRef, RangeAddr, RangeRef, Value};

use crate::eval::Ctx;
use crate::functions::stats::wildcard_matches;
use crate::functions::{finish, int_arg, logical_arg};
use crate::operand::{Operand, Reference};

/// `ROW([reference])`: the row of the reference, or of the formula itself.
pub(super) fn row(ctx: &Ctx, args: &[Operand]) -> Operand {
    match args.first() {
        None => Operand::number(f64::from(ctx.origin().cell.row) + 1.0),
        Some(Operand::Reference(reference)) => match reference.as_single_area() {
            Some(area) => Operand::number(f64::from(area.range.start.row) + 1.0),
            None => Operand::error(CalcError::Ref),
        },
        Some(_) => Operand::error(CalcError::Value),
    }
}

/// `COLUMN([reference])`.
pub(super) fn column(ctx: &Ctx, args: &[Operand]) -> Operand {
    match args.first() {
        None => Operand::number(f64::from(ctx.origin().cell.col) + 1.0),
        Some(Operand::Reference(reference)) => match reference.as_single_area() {
            Some(area) => Operand::number(f64::from(area.range.start.col) + 1.0),
            None => Operand::error(CalcError::Ref),
        },
        Some(_) => Operand::error(CalcError::Value),
    }
}

/// `ROWS(range)`: how tall it is.
pub(super) fn rows(ctx: &Ctx, args: &[Operand]) -> Operand {
    match shape_of(ctx, &args[0]) {
        Ok((height, _)) => Operand::number(f64::from(height)),
        Err(e) => Operand::error(e),
    }
}

/// `COLUMNS(range)`: how wide it is.
pub(super) fn columns(ctx: &Ctx, args: &[Operand]) -> Operand {
    match shape_of(ctx, &args[0]) {
        Ok((_, width)) => Operand::number(f64::from(width)),
        Err(e) => Operand::error(e),
    }
}

fn shape_of(_ctx: &Ctx, operand: &Operand) -> Result<(u32, u32), CalcError> {
    match operand {
        Operand::Reference(reference) => {
            let area = reference.as_single_area().ok_or(CalcError::Ref)?;
            Ok((area.range.height(), area.range.width()))
        }
        Operand::Array(array) => Ok((array.rows(), array.cols())),
        Operand::Value(Value::Error(e)) => Err(*e),
        Operand::Value(_) => Ok((1, 1)),
    }
}

/// The area a lookup will walk: the stated range, narrowed to the rows and
/// columns the sheet actually holds.
///
/// Returns the clipped area together with the stated range, because offsets
/// are reported against the latter.
fn searchable(ctx: &Ctx, reference: &Reference) -> Option<(RangeAddr, RangeRef)> {
    let stated = reference.as_single_area()?;
    let clipped = ctx.clipped_areas(reference).into_iter().next()?;
    Some((clipped, stated.range))
}

/// `MATCH(lookup, range, [kind])`.
///
/// `kind` is 1 for the largest value at or below the target in an ascending
/// range, 0 for an exact match in any order, and -1 for the smallest value at
/// or above the target in a descending range. It defaults to 1.
pub(super) fn match_(ctx: &Ctx, args: &[Operand]) -> Operand {
    finish((|| {
        let target = ctx.to_scalar(args[0].clone());
        if let Value::Error(e) = target {
            return Err(e);
        }
        let kind = match args.get(2) {
            None => 1,
            Some(arg) => int_arg(ctx, arg)?,
        };

        let candidates = lookup_vector(ctx, &args[1])?;
        find_position(&target, &candidates, kind)
            .map(|position| Value::number(position as f64))
            .ok_or(CalcError::NotAvailable)
    })())
}

/// The values a one-dimensional lookup argument offers, in order, together
/// with their one-based positions in the stated range.
fn lookup_vector(ctx: &Ctx, operand: &Operand) -> Result<Vec<(usize, Value)>, CalcError> {
    match operand {
        Operand::Reference(reference) => {
            let Some((clipped, stated)) = searchable(ctx, reference) else {
                return Ok(Vec::new());
            };
            let vertical = stated.width() == 1;
            let mut out = Vec::new();
            for cell in clipped.range.cells() {
                let position = if vertical {
                    (cell.row - stated.start.row) as usize + 1
                } else {
                    (cell.col - stated.start.col) as usize + 1
                };
                out.push((position, ctx.cell(CellAddr::new(clipped.sheet, cell))));
            }
            Ok(out)
        }
        Operand::Array(array) => Ok(array
            .values()
            .enumerate()
            .map(|(index, value)| (index + 1, value.clone()))
            .collect()),
        Operand::Value(Value::Error(e)) => Err(*e),
        Operand::Value(value) => Ok(vec![(1, value.clone())]),
    }
}

/// Locate the target among the candidates under one of the three match kinds.
fn find_position(target: &Value, candidates: &[(usize, Value)], kind: i64) -> Option<usize> {
    if kind == 0 {
        let pattern = match target {
            Value::Text(t) if t.contains('*') || t.contains('?') => Some(t.to_string()),
            _ => None,
        };
        return candidates
            .iter()
            .find(|(_, value)| match &pattern {
                Some(p) => value.to_text().is_ok_and(|text| wildcard_matches(p, &text)),
                None => value.compare(target) == Ok(Ordering::Equal),
            })
            .map(|(position, _)| *position);
    }

    // Ordered search. The range is assumed sorted the way the kind says, so
    // the answer is the last candidate that has not yet passed the target.
    let wanted = if kind > 0 {
        Ordering::Greater
    } else {
        Ordering::Less
    };

    let mut best = None;
    for (position, value) in candidates {
        match value.compare(target) {
            Err(_) => continue,
            Ok(Ordering::Equal) => return Some(*position),
            Ok(order) if order == wanted => {
                // Passed the target; an ordered range has nothing better later.
                break;
            }
            Ok(_) => best = Some(*position),
        }
    }
    best
}

/// `INDEX(range, row, [column])`, counting from one.
///
/// A zero row or column means the whole column or row, which is what makes
/// `INDEX` usable as the second half of an `INDEX`/`MATCH` pair that returns
/// a range.
pub(super) fn index(ctx: &Ctx, args: &[Operand]) -> Operand {
    let Operand::Reference(reference) = &args[0] else {
        // An array argument is addressed positionally without a sheet.
        return index_into_array(ctx, args);
    };
    let Some(area) = reference.as_single_area() else {
        return Operand::error(CalcError::Ref);
    };

    let (height, width) = (area.range.height(), area.range.width());

    let first = match int_arg(ctx, &args[1]) {
        Ok(n) => n,
        Err(e) => return Operand::error(e),
    };
    let second = match args.get(2) {
        None => None,
        Some(arg) => match int_arg(ctx, arg) {
            Ok(n) => Some(n),
            Err(e) => return Operand::error(e),
        },
    };

    // With one index over a range that is a single row or column, the index
    // addresses along that line rather than down it.
    let (row_index, col_index) = match second {
        Some(col) => (first, col),
        None if height == 1 => (1, first),
        None => (first, if width == 1 { 1 } else { 0 }),
    };

    if row_index < 0 || col_index < 0 {
        return Operand::error(CalcError::Value);
    }
    if row_index as u32 > height || col_index as u32 > width {
        return Operand::error(CalcError::Ref);
    }

    let start = area.range.start;

    // A zero index selects the whole line rather than one cell.
    let selected = match (row_index, col_index) {
        (0, 0) => area.range,
        (0, c) => {
            let col = start.col + c as u32 - 1;
            RangeRef::new(
                CellRef::new(start.row, col),
                CellRef::new(area.range.end.row, col),
            )
        }
        (r, 0) => {
            let row = start.row + r as u32 - 1;
            RangeRef::new(
                CellRef::new(row, start.col),
                CellRef::new(row, area.range.end.col),
            )
        }
        (r, c) => RangeRef::single(CellRef::new(
            start.row + r as u32 - 1,
            start.col + c as u32 - 1,
        )),
    };

    Operand::Reference(Reference::single(area.sheet, selected))
}

fn index_into_array(ctx: &Ctx, args: &[Operand]) -> Operand {
    let array = match ctx.to_array(&args[0]) {
        Ok(a) => a,
        Err(e) => return Operand::error(e),
    };
    let first = match int_arg(ctx, &args[1]) {
        Ok(n) => n,
        Err(e) => return Operand::error(e),
    };
    let second = match args.get(2) {
        None => None,
        Some(arg) => match int_arg(ctx, arg) {
            Ok(n) => Some(n),
            Err(e) => return Operand::error(e),
        },
    };

    let (row_index, col_index) = match second {
        Some(col) => (first, col),
        None if array.rows() == 1 => (1, first),
        None => (first, 1),
    };

    if row_index < 1 || col_index < 1 {
        return Operand::error(CalcError::Value);
    }
    array
        .get(row_index as u32 - 1, col_index as u32 - 1)
        .cloned()
        .map_or_else(|| Operand::error(CalcError::Ref), Operand::Value)
}

/// `VLOOKUP(lookup, table, column, [approximate])`.
pub(super) fn vlookup(ctx: &Ctx, args: &[Operand]) -> Operand {
    table_lookup(ctx, args, true)
}

/// `HLOOKUP(lookup, table, row, [approximate])`.
pub(super) fn hlookup(ctx: &Ctx, args: &[Operand]) -> Operand {
    table_lookup(ctx, args, false)
}

fn table_lookup(ctx: &Ctx, args: &[Operand], vertical: bool) -> Operand {
    finish((|| {
        let target = ctx.to_scalar(args[0].clone());
        if let Value::Error(e) = target {
            return Err(e);
        }

        let Operand::Reference(reference) = &args[1] else {
            return Err(CalcError::Value);
        };
        let Some(stated) = reference.as_single_area() else {
            return Err(CalcError::Ref);
        };

        let offset = int_arg(ctx, &args[2])?;
        let approximate = match args.get(3) {
            None => true,
            Some(arg) => logical_arg(ctx, arg)?,
        };

        let span = if vertical {
            stated.range.width()
        } else {
            stated.range.height()
        };
        if offset < 1 {
            return Err(CalcError::Value);
        }
        if offset as u32 > span {
            return Err(CalcError::Ref);
        }

        // Search the first column, or the first row.
        let key_line = if vertical {
            RangeRef::new(
                stated.range.start,
                CellRef::new(stated.range.end.row, stated.range.start.col),
            )
        } else {
            RangeRef::new(
                stated.range.start,
                CellRef::new(stated.range.start.row, stated.range.end.col),
            )
        };

        let keys = lookup_vector(
            ctx,
            &Operand::Reference(Reference::single(stated.sheet, key_line)),
        )?;

        let kind = if approximate { 1 } else { 0 };
        let position = find_position(&target, &keys, kind).ok_or(CalcError::NotAvailable)?;

        let found = if vertical {
            CellRef::new(
                stated.range.start.row + position as u32 - 1,
                stated.range.start.col + offset as u32 - 1,
            )
        } else {
            CellRef::new(
                stated.range.start.row + offset as u32 - 1,
                stated.range.start.col + position as u32 - 1,
            )
        };

        Ok(ctx.cell(CellAddr::new(stated.sheet, found)))
    })())
}
