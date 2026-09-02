//! Error type for the crate.
//!
//! Every fallible operation returns [`Error`]. Parsing is deliberately strict
//! about structure it must understand and lenient about spellings it can:
//! a rhythm value it cannot represent is [`Error::Malformed`], never a
//! substituted default.

use std::io;

#[derive(Debug, thiserror::Error)]
/// Anything that can go wrong reading or writing a score.
#[non_exhaustive]
pub enum Error {
    /// The file could not be read or written.
    #[error("io: {0}")]
    Io(#[from] io::Error),
    /// The container is not a readable zip archive.
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// The GPIF payload is not well-formed XML.
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    /// The archive contains no `score.gpif`, so it is not a GP7/8 file.
    #[error("not a Guitar Pro 7/8 container: no Content/score.gpif")]
    NotGp7,
    /// The XML parsed but the document it describes is not usable.
    #[error("malformed gpif: {0}")]
    Malformed(String),
}

/// [`std::result::Result`] with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
