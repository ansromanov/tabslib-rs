//! tabslib -- a score, analysis, and generation engine.
//!
//! Scope for 0.1: read and write the GP7/8 container and its GPIF payload into
//! an owned document model, deterministic analysis and edits, and generation
//! capabilities built on that model.

#![warn(missing_docs)]

pub mod edits;
pub mod error;
pub mod evaluation;
pub mod facade;
pub mod fixtures;
pub mod format;
pub mod generation;
pub mod inspect;
pub mod model;
pub mod percussion;
pub mod pitch;
pub mod selection;
#[cfg(feature = "json")]
pub mod serialization;
pub mod tuning;
pub mod validation;

pub use error::{Error, Result};
pub use format::{ReadFormat, WriteFormat};

/// The schema identifier for the native JSON document envelope.
pub const NATIVE_DOCUMENT_FORMAT: &str = "tabslib-document";

/// The current native JSON document schema version.
pub const NATIVE_DOCUMENT_SCHEMA_VERSION: u32 = 1;

/// A non-fatal diagnostic emitted while loading or validating a document.
pub type Diagnostic = validation::Issue;

/// Reads bytes in any compiled-in format.
///
/// For a known format prefer the adapter directly — `Gp::read(bytes)` — which
/// reports that format's own error rather than "unrecognised".
pub fn load(bytes: &[u8]) -> Result<Document> {
    format::read_any(bytes)
}

/// Loads bytes and returns non-fatal structural diagnostics alongside the document.
pub fn load_with_diagnostics(bytes: &[u8]) -> Result<(Document, Vec<Diagnostic>)> {
    let document = load(bytes)?;
    let diagnostics = validation::validate(&document);
    Ok((document, diagnostics))
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

/// Compiles and runs the Rust examples in `README.md` as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
