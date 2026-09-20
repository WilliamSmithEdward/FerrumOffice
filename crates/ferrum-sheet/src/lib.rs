//! Workbook model, sparse cell storage and dependency-driven recalculation.
//!
//! The pieces:
//!
//! - [`sheet::Sheet`] stores the cells of one sheet sparsely, and tracks the
//!   bounding box of what is populated so the engine never walks a whole
//!   column to reach four numbers.
//! - [`graph::DependencyGraph`] answers "what reads this cell", including
//!   through ranges, without consulting every formula.
//! - [`workbook::Workbook`] ties them together and recalculates in dependency
//!   order after every edit.

pub mod axis;
pub mod cell;
pub mod graph;
pub mod sheet;
pub mod workbook;

pub use axis::Axis;
pub use cell::{Cell, Input};
pub use graph::{Dependency, DependencyGraph};
pub use sheet::Sheet;
pub use workbook::{RecalcReport, SheetError, Workbook};
