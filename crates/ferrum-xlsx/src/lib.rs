//! Reading and writing spreadsheet files.
//!
//! A spreadsheet file is a ZIP archive of XML documents. Both halves are
//! written here rather than taken from the registry, which is the policy in
//! `docs/adr/0002-no-runtime-dependencies.md`.
//!
//! The pieces, from the bottom up:
//!
//! - [`crc32`] because every ZIP entry carries one.
//! - [`zip`] for the container.
//! - [`xml`] for the documents inside it.
//!
//! Writing comes first and is done with stored entries, which every reader
//! accepts. Reading a file written elsewhere additionally needs DEFLATE.

pub mod crc32;
pub mod write;
pub mod xml;
pub mod zip;

pub use crc32::crc32;
pub use write::write_workbook;
pub use xml::XmlWriter;
pub use zip::{DosTime, ZipError, ZipWriter};
