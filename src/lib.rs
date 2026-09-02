//! tabslib -- a general-purpose tablature library.
//!
//! An owned, immutable document model for fretted-instrument notation, codecs
//! for the formats that carry it, and deterministic edits over both. Scope for
//! 0.1 is reading and writing the version 7 and 8 `.gp` container and its XML
//! payload; ASCII tablature, MusicXML and MIDI are planned.
//!
//! The library does not decide what to play. Generation, arrangement, style
//! modelling and quality scoring are deliberately out of scope: they need a
//! model of a musical style, and belong in a crate built on top of this one.
//!
//! Written from published format behaviour, with no dependency on any vendor
//! library and nothing translated from another implementation.
//!
//! Guitar Pro is a trademark of Arobas Music. This library is not affiliated
//! with or endorsed by Arobas Music; format names describe compatibility only.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod container;
pub mod error;
pub mod fixtures;
pub mod gpif;
pub mod model;

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
