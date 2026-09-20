//! Evaluating a parsed formula.
//!
//! The engine never reaches for a workbook directly. It asks a [`Resolver`],
//! which is what keeps this crate free of the document model and lets the
//! whole evaluator be tested against a fixture that fits in a few lines.

use ferrum_core::{
    A1Ref, CalcError, CellAddr, CellRef, MAX_COL, MAX_ROW, RangeAddr, RangeRef, SheetId, Value,
};

use crate::ast::{BinaryOp, Bound, Expr, RefTarget, SheetRef, UnaryOp};
use crate::functions;
use crate::operand::{Array, Operand, Reference};

/// What the evaluator needs from a workbook.
///
/// Every method is a read. Evaluation never changes the document.
pub trait Resolver {
    /// The sheet an unqualified reference belongs to.
    fn current_sheet(&self) -> SheetId;

    /// Look a sheet up by name, case-insensitively.
    fn sheet_by_name(&self, name: &str) -> Option<SheetId>;

    /// Every sheet from `first` to `last` in tab order, for a
    /// three-dimensional reference.
    fn sheets_between(&self, first: &str, last: &str) -> Option<Vec<SheetId>>;

    /// The value in one cell. An empty cell is [`Value::Blank`].
    fn cell(&self, addr: CellAddr) -> Value;

    /// The bounding box of the populated cells on a sheet, or `None` when the
    /// sheet is empty.
    ///
    /// This is what stops `SUM(A:A)` walking 1,048,576 rows to add four
    /// numbers. It may over-estimate; it must never under-estimate, or an
    /// aggregate will silently miss data.
    fn used_bounds(&self, sheet: SheetId) -> Option<RangeRef>;

    /// Resolve a defined name. `sheet` is set when the name was written
    /// sheet-qualified.
    fn defined_name(&self, sheet: Option<SheetId>, name: &str) -> Option<Operand>;
}

/// One evaluation, against one resolver, from one cell.
pub struct Ctx<'a> {
    resolver: &'a dyn Resolver,
    origin: CellAddr,
}

impl<'a> Ctx<'a> {
    pub fn new(resolver: &'a dyn Resolver, origin: CellAddr) -> Self {
        Self { resolver, origin }
    }

    /// The cell the formula being evaluated lives in.
    pub const fn origin(&self) -> CellAddr {
        self.origin
    }

    pub fn resolver(&self) -> &dyn Resolver {
        self.resolver
    }

    pub fn cell(&self, addr: CellAddr) -> Value {
        self.resolver.cell(addr)
    }

    /// Evaluate an expression.
    pub fn eval(&self, expr: &Expr) -> Operand {
        match expr {
            Expr::Literal(value) => Operand::Value(value.clone()),

            Expr::Ref { sheet, target } => self.eval_ref(sheet.as_ref(), target),

            Expr::Name { sheet, name } => {
                let scope = match sheet {
                    None => None,
                    Some(SheetRef::One(s)) => match self.resolver.sheet_by_name(s) {
                        Some(id) => Some(id),
                        None => return Operand::error(CalcError::Ref),
                    },
                    // A name cannot be scoped to a span of sheets.
                    Some(SheetRef::Span { .. }) => return Operand::error(CalcError::Name),
                };
                self.resolver
                    .defined_name(scope, name)
                    .unwrap_or_else(|| Operand::error(CalcError::Name))
            }

            Expr::Unary { op, operand } => {
                let value = self.eval(operand);
                self.map_scalar(value, |v| unary(*op, &v))
            }

            Expr::Percent(inner) => {
                let value = self.eval(inner);
                self.map_scalar(value, |v| match v.to_number() {
                    Ok(n) => Value::number(n / 100.0),
                    Err(e) => Value::Error(e),
                })
            }

            Expr::Binary { op, left, right } => {
                let a = self.eval(left);
                let b = self.eval(right);
                self.binary(*op, a, b)
            }

            Expr::Call { name, args } => functions::call(self, name, args),

            Expr::Array(rows) => self.eval_array_literal(rows),

            Expr::RangeOp { left, right } => self.eval_range_op(left, right),

            Expr::Intersect { left, right } => {
                let (a, b) = (self.eval(left), self.eval(right));
                match (as_reference(a), as_reference(b)) {
                    (Some(x), Some(y)) => match x.intersect(&y) {
                        Some(shared) => Operand::Reference(shared),
                        None => Operand::error(CalcError::Null),
                    },
                    _ => Operand::error(CalcError::Value),
                }
            }

            Expr::Union(parts) => {
                let mut joined: Option<Reference> = None;
                for part in parts {
                    let Some(reference) = as_reference(self.eval(part)) else {
                        return Operand::error(CalcError::Value);
                    };
                    joined = Some(match joined {
                        None => reference,
                        Some(acc) => acc.union(reference),
                    });
                }
                joined.map_or_else(|| Operand::error(CalcError::Null), Operand::Reference)
            }
        }
    }

    /// Evaluate and collapse to a single value, the way a cell holding one
    /// result needs.
    ///
    /// A multi-cell result is reduced by implicit intersection: a row or
    /// column that lines up with the formula's own position contributes the
    /// aligned cell. Dynamic arrays will replace this with spilling.
    pub fn eval_to_value(&self, expr: &Expr) -> Value {
        self.to_scalar(self.eval(expr))
    }

    /// Reduce an operand to one value.
    pub fn to_scalar(&self, operand: Operand) -> Value {
        match operand {
            Operand::Value(value) => value,
            Operand::Array(array) => array
                .as_scalar()
                .cloned()
                // Legacy collapse. Spilling will change this.
                .unwrap_or_else(|| array.get(0, 0).cloned().unwrap_or(Value::Blank)),
            Operand::Reference(reference) => self.implicit_intersection(&reference),
        }
    }

    /// The classic rule for using a range where one value is wanted.
    fn implicit_intersection(&self, reference: &Reference) -> Value {
        let Some(area) = reference.as_single_area() else {
            return Value::Error(CalcError::Value);
        };
        let range = area.range;

        if range.start == range.end {
            return self.cell(CellAddr::new(area.sheet, range.start));
        }

        let origin = self.origin.cell;

        // One row: take the cell in the formula's own column.
        if range.start.row == range.end.row
            && origin.col >= range.start.col
            && origin.col <= range.end.col
        {
            return self.cell(CellAddr::new(
                area.sheet,
                CellRef::new(range.start.row, origin.col),
            ));
        }

        // One column: take the cell in the formula's own row.
        if range.start.col == range.end.col
            && origin.row >= range.start.row
            && origin.row <= range.end.row
        {
            return self.cell(CellAddr::new(
                area.sheet,
                CellRef::new(origin.row, range.start.col),
            ));
        }

        Value::Error(CalcError::Value)
    }

    /// Every area of a reference, clipped to the cells the sheet actually
    /// holds.
    ///
    /// A whole-column reference addresses a million cells and almost always
    /// touches a handful. Clipping is what makes aggregates over one cost the
    /// data rather than the address space.
    pub fn clipped_areas(&self, reference: &Reference) -> Vec<RangeAddr> {
        reference
            .areas()
            .iter()
            .filter_map(|area| {
                // Only clip when the reference is bigger than the data. A
                // range entirely inside the used box is already tight.
                let used = self.resolver.used_bounds(area.sheet)?;
                area.range
                    .intersection(&used)
                    .map(|clipped| RangeAddr::new(area.sheet, clipped))
            })
            .collect()
    }

    /// Walk the values a reference addresses, skipping the empty expanse
    /// outside the used area.
    pub fn reference_values(&self, reference: &Reference) -> Vec<Value> {
        let mut out = Vec::new();
        for area in self.clipped_areas(reference) {
            for cell in area.range.cells() {
                out.push(self.cell(CellAddr::new(area.sheet, cell)));
            }
        }
        out
    }

    /// Every value an operand contributes to an aggregate.
    ///
    /// A scalar contributes itself, an array its elements, a reference the
    /// cells it addresses.
    pub fn flatten(&self, operand: &Operand) -> Vec<Value> {
        match operand {
            Operand::Value(value) => vec![value.clone()],
            Operand::Array(array) => array.values().cloned().collect(),
            Operand::Reference(reference) => self.reference_values(reference),
        }
    }

    /// Materialise an operand as a rectangle, for element-wise work.
    pub fn to_array(&self, operand: &Operand) -> Result<Array, CalcError> {
        match operand {
            Operand::Value(value) => Ok(Array::scalar(value.clone())),
            Operand::Array(array) => Ok(array.clone()),
            Operand::Reference(reference) => {
                // Arithmetic over a union has no defined shape.
                let area = reference.as_single_area().ok_or(CalcError::Value)?;
                let (rows, cols) = (area.range.height(), area.range.width());
                // A whole-column operand in arithmetic would materialise a
                // million blanks; refuse rather than allocate it.
                if u64::from(rows) * u64::from(cols) > MAX_MATERIALISED_CELLS {
                    return Err(CalcError::Value);
                }
                let cells = area
                    .range
                    .cells()
                    .map(|cell| self.cell(CellAddr::new(area.sheet, cell)))
                    .collect();
                Array::new(rows, cols, cells).ok_or(CalcError::Value)
            }
        }
    }

    fn eval_array_literal(&self, rows: &[Vec<Expr>]) -> Operand {
        let mut built = Vec::with_capacity(rows.len());
        for row in rows {
            let mut line = Vec::with_capacity(row.len());
            for expr in row {
                line.push(self.eval_to_value(expr));
            }
            built.push(line);
        }
        Array::from_rows(built).map_or_else(|| Operand::error(CalcError::Value), Operand::Array)
    }

    fn eval_range_op(&self, left: &Expr, right: &Expr) -> Operand {
        let (a, b) = (self.eval(left), self.eval(right));
        let (Some(x), Some(y)) = (as_reference(a), as_reference(b)) else {
            return Operand::error(CalcError::Value);
        };
        let (Some(first), Some(last)) = (x.as_single_area(), y.as_single_area()) else {
            return Operand::error(CalcError::Value);
        };
        if first.sheet != last.sheet {
            return Operand::error(CalcError::Ref);
        }
        Operand::Reference(Reference::single(
            first.sheet,
            first.range.union_bounds(&last.range),
        ))
    }

    fn eval_ref(&self, sheet: Option<&SheetRef>, target: &RefTarget) -> Operand {
        if matches!(target, RefTarget::Invalid) {
            return Operand::error(CalcError::Ref);
        }

        let sheets: Vec<SheetId> = match sheet {
            None => vec![self.resolver.current_sheet()],
            Some(SheetRef::One(name)) => match self.resolver.sheet_by_name(name) {
                Some(id) => vec![id],
                None => return Operand::error(CalcError::Ref),
            },
            Some(SheetRef::Span { first, last }) => {
                match self.resolver.sheets_between(first, last) {
                    Some(ids) if !ids.is_empty() => ids,
                    _ => return Operand::error(CalcError::Ref),
                }
            }
        };

        let range = match target {
            RefTarget::Cell(a1) => RangeRef::single(a1.cell),
            RefTarget::Range { start, end } => RangeRef::new(start.cell, end.cell),
            RefTarget::Columns { first, last } => RangeRef::whole_columns(first.index, last.index),
            RefTarget::Rows { first, last } => RangeRef::whole_rows(first.index, last.index),
            RefTarget::Invalid => unreachable!("handled above"),
        };

        let areas = sheets
            .into_iter()
            .map(|sheet| RangeAddr::new(sheet, range))
            .collect();
        Reference::new(areas).map_or_else(|| Operand::error(CalcError::Ref), Operand::Reference)
    }

    /// Apply a scalar operation, spreading it over an array or a range.
    fn map_scalar(&self, operand: Operand, f: impl Fn(Value) -> Value) -> Operand {
        match operand {
            Operand::Value(value) => Operand::Value(f(value)),
            other => match self.to_array(&other) {
                Err(e) => Operand::error(e),
                Ok(array) => {
                    let (rows, cols) = (array.rows(), array.cols());
                    let cells = array.values().cloned().map(f).collect();
                    Array::new(rows, cols, cells)
                        .map_or_else(|| Operand::error(CalcError::Value), Operand::Array)
                }
            },
        }
    }

    fn binary(&self, op: BinaryOp, left: Operand, right: Operand) -> Operand {
        // The common case by a wide margin: two plain scalars.
        if let (Operand::Value(a), Operand::Value(b)) = (&left, &right) {
            return Operand::Value(apply_binary(op, a, b));
        }

        // A single-cell reference behaves as its value, which keeps `A1+1`
        // on the scalar path rather than building a one-element array.
        let left = self.demote_single_cell(left);
        let right = self.demote_single_cell(right);
        if let (Operand::Value(a), Operand::Value(b)) = (&left, &right) {
            return Operand::Value(apply_binary(op, a, b));
        }

        let (a, b) = match (self.to_array(&left), self.to_array(&right)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => return Operand::error(e),
        };

        let (rows, cols) = Array::broadcast_shape(&a, &b);
        let mut cells = Vec::with_capacity((rows as usize) * (cols as usize));
        for row in 0..rows {
            for col in 0..cols {
                let x = a.get_broadcast(row, col);
                let y = b.get_broadcast(row, col);
                cells.push(apply_binary(op, &x, &y));
            }
        }
        Array::new(rows, cols, cells)
            .map_or_else(|| Operand::error(CalcError::Value), Operand::Array)
    }

    /// Turn a one-cell reference into the value it holds.
    fn demote_single_cell(&self, operand: Operand) -> Operand {
        match &operand {
            Operand::Reference(reference) => match reference.as_single_cell() {
                Some(addr) => Operand::Value(self.cell(addr)),
                None => operand,
            },
            _ => operand,
        }
    }
}

/// Ceiling on the cells an operand may materialise for element-wise work.
///
/// Well above any real array formula and far below a whole column, so a
/// mistake reports `#VALUE!` instead of exhausting memory.
const MAX_MATERIALISED_CELLS: u64 = 4_194_304;

/// Read an operand as a reference, if it is one.
fn as_reference(operand: Operand) -> Option<Reference> {
    match operand {
        Operand::Reference(reference) => Some(reference),
        _ => None,
    }
}

fn unary(op: UnaryOp, value: &Value) -> Value {
    match value.to_number() {
        Err(e) => Value::Error(e),
        Ok(n) => Value::number(match op {
            UnaryOp::Negate => -n,
            UnaryOp::Plus => n,
        }),
    }
}

/// One binary operator on two scalars.
pub fn apply_binary(op: BinaryOp, a: &Value, b: &Value) -> Value {
    if op == BinaryOp::Concat {
        return match (a.to_text(), b.to_text()) {
            (Ok(x), Ok(y)) => Value::text(format!("{x}{y}")),
            (Err(e), _) | (_, Err(e)) => Value::Error(e),
        };
    }

    if op.is_comparison() {
        return match a.compare(b) {
            Err(e) => Value::Error(e),
            Ok(order) => Value::Logical(match op {
                BinaryOp::Equal => order.is_eq(),
                BinaryOp::NotEqual => order.is_ne(),
                BinaryOp::Less => order.is_lt(),
                BinaryOp::LessOrEqual => order.is_le(),
                BinaryOp::Greater => order.is_gt(),
                BinaryOp::GreaterOrEqual => order.is_ge(),
                _ => unreachable!("is_comparison covers exactly these"),
            }),
        };
    }

    let (x, y) = match (a.to_number(), b.to_number()) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return Value::Error(e),
    };

    match op {
        BinaryOp::Add => Value::number(x + y),
        BinaryOp::Subtract => Value::number(x - y),
        BinaryOp::Multiply => Value::number(x * y),
        BinaryOp::Divide => {
            if y == 0.0 {
                Value::Error(CalcError::Div0)
            } else {
                Value::number(x / y)
            }
        }
        BinaryOp::Power => power(x, y),
        // Concat and the comparisons returned above.
        _ => unreachable!("handled above"),
    }
}

/// Exponentiation, with the edge cases a spreadsheet defines differently from
/// IEEE arithmetic.
///
/// `0^0` and a negative base under a fractional exponent are both `#NUM!`
/// rather than 1 and NaN, and `0` to a negative power is a division by zero.
/// These three are recall rather than measurement; see
/// `docs/open-questions.md`.
fn power(base: f64, exponent: f64) -> Value {
    if base == 0.0 {
        if exponent == 0.0 {
            return Value::Error(CalcError::Num);
        }
        if exponent < 0.0 {
            return Value::Error(CalcError::Div0);
        }
    }
    Value::number(base.powf(exponent))
}

/// Build a reference from an A1 pair, for callers outside the parser.
pub fn range_of(sheet: SheetId, start: A1Ref, end: A1Ref) -> Reference {
    Reference::single(sheet, RangeRef::new(start.cell, end.cell))
}

/// Clamp a bound to the grid, for callers constructing whole-row or
/// whole-column references by hand.
pub const fn clamp_bound(bound: Bound, is_row: bool) -> u32 {
    let limit = if is_row { MAX_ROW } else { MAX_COL };
    if bound.index > limit {
        limit
    } else {
        bound.index
    }
}
