//! Types shared by every FerrumOffice application.
//!
//! This crate holds the vocabulary that the calculation engine, the document
//! model and the user interface all have to agree on: what a cell can contain,
//! how the grid is addressed, and how values convert into one another.
//!
//! It has no dependencies and performs no I/O.

pub mod coord;
pub mod defaults;
pub mod value;

pub use coord::{
    A1Ref, CellAddr, CellRef, MAX_COL, MAX_ROW, ParseRefError, RangeAddr, RangeRef, SheetId,
    column_label, parse_column_label,
};
pub use value::{CalcError, Value, compare_text, format_general, parse_number};
