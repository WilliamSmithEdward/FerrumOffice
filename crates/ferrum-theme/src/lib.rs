//! Colour tokens and grid metrics for every FerrumOffice surface.
//!
//! Two rules hold this crate together.
//!
//! **Geometry is theme-independent.** Sizing, pixel alignment and zoom are
//! identical in light and dark; only colours change. Everything dimensional
//! lives in [`metrics`] and is not part of a [`Palette`].
//!
//! **Every colour clears a contrast floor.** The floors are checked by a test
//! in this crate rather than asserted in a document, so a token that fails
//! cannot be committed. See [`contrast`].

pub mod contrast;
pub mod metrics;
pub mod palette;

pub use palette::{Palette, Rgb, Theme};
