//! tabslib -- an independent Guitar Pro score engine.
//!
//! Scope for 0.1: read and write the GP7/8 container and its GPIF payload into
//! an owned document model, with no dependency on any existing Guitar Pro
//! library. The model and codec are designed from the file format, not
//! translated from another implementation.

pub mod error;
pub mod model;
pub mod container;
pub mod gpif;

pub use error::{Error, Result};
pub use model::Document;

/// Reads a `.gp` file's bytes into a [`Document`].
pub fn load(bytes: &[u8]) -> Result<Document> {
    gpif::parse(&container::read_gpif(bytes)?)
}

/// Serialises a [`Document`] back to `.gp` container bytes.
pub fn save(doc: &Document) -> Result<Vec<u8>> {
    container::write_gpif(&gpif::write(doc))
}
