//! What FerrumGrid is showing, and what a gesture does to it.
//!
//! [`App`] holds the workbook and everything about the window: where it is
//! scrolled, what is selected, what is being typed. [`View`] is the picture
//! that comes out of it, as plain data. The application draws that picture
//! and sends gestures back, deciding nothing on its own.
//!
//! Nothing here knows about the interface toolkit, which is the point. The
//! whole application can be driven from a test through [`Harness`], with no
//! window, no event loop and no screen, and what that test exercises is the
//! same code the window runs. See
//! `docs/adr/0003-drive-the-application-through-a-harness.md`.

pub mod app;
pub mod harness;
pub mod view;

pub use app::{App, Key, Span, key_from};
pub use harness::Harness;
pub use view::{Cell, Geometry, Header, Rect, Tab, Thumb, View};

// Re-exported so that everything in this crate's own interface can be named
// without reaching past it.
pub use ferrum_core::{CellRef, RangeRef};
pub use ferrum_theme::Theme;
