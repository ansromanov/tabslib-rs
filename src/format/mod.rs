//! Format adapters.
//!
//! The core — [`crate::model`], [`crate::inspect`], [`crate::edits`] — knows
//! nothing about files. Everything that reads or writes one implements a trait
//! here, so a new format is an addition rather than an edit.
//!
//! # Two traits, not one
//!
//! Reading and writing are separate capabilities because not every format can
//! do both. A `.gp` file round-trips; a rendering does not, and never will.
//!
//! | adapter | [`ReadFormat`] | [`WriteFormat`] |
//! | --- | :-: | :-: |
//! | [`gp`] | yes | yes |
//! | ascii, html, pdf | no | yes |
//!
//! A single trait would force a rendering to supply a `read` that fails at run
//! time, and would invite a round-trip test that cannot pass. Split, the
//! round-trip invariant has an exact meaning: it applies to every adapter
//! implementing both traits, and to nothing else.

use crate::error::Result;
use crate::model::Document;

#[cfg(feature = "gp")]
pub mod gp;

/// A format that can be parsed into a [`Document`].
pub trait ReadFormat {
    /// Short identifier, used in errors and diagnostics.
    const NAME: &'static str;

    /// Whether these bytes look like this format.
    ///
    /// Cheap and best-effort: a `true` means [`read`](ReadFormat::read) is
    /// worth attempting, not that it will succeed.
    fn detect(bytes: &[u8]) -> bool;

    /// Parses bytes into a document.
    fn read(bytes: &[u8]) -> Result<Document>;
}

/// A format a [`Document`] can be serialised to.
///
/// Implemented alone by renderings, which are deliberately lossy: an ASCII tab
/// or a PDF is a view of a score, not a container for one.
pub trait WriteFormat {
    /// Short identifier, used in errors and diagnostics.
    const NAME: &'static str;

    /// Serialises a document.
    fn write(doc: &Document) -> Result<Vec<u8>>;
}

/// Reads bytes using whichever compiled-in format claims them.
///
/// Returns [`crate::Error::UnknownFormat`] when none does.
pub fn read_any(bytes: &[u8]) -> Result<Document> {
    #[cfg(feature = "gp")]
    if <gp::Gp as ReadFormat>::detect(bytes) {
        return <gp::Gp as ReadFormat>::read(bytes);
    }
    let _ = bytes;
    Err(crate::Error::UnknownFormat)
}
