//! tabslib -- an independent Guitar Pro score engine.
//!
//! Scope for 0.1: read and write the GP7/8 container and its GPIF payload into
//! an owned document model, with no dependency on any existing Guitar Pro
//! library. The model and codec are designed from the file format, not
//! translated from another implementation.

pub mod edits;
pub mod error;
pub mod fixtures;
pub mod format;
pub mod inspect;
pub mod model;
pub mod percussion;
pub mod pitch;
pub mod selection;

pub use error::{Error, Result};
pub use format::{ReadFormat, WriteFormat};

/// Reads bytes in any compiled-in format.
///
/// For a known format prefer the adapter directly — `Gp::read(bytes)` — which
/// reports that format's own error rather than "unrecognised".
pub fn load(bytes: &[u8]) -> Result<Document> {
    format::read_any(bytes)
}

/// Writes a document as `.gp`.
///
/// A convenience for the one format that round-trips. Other formats are
/// renderings and are reached through their adapter:
/// `<Ascii as WriteFormat>::write(&doc)`.
#[cfg(feature = "gp")]
pub fn save(doc: &Document) -> Result<Vec<u8>> {
    <format::gp::Gp as WriteFormat>::write(doc)
}
pub use model::Document;
