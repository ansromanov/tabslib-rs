//! Error type for the crate.
//!
//! The core carries only errors the core can produce. A format adapter's own
//! failures — a malformed archive, unresolvable XML — arrive through
//! [`Error::Format`], boxed, so that enabling or disabling an adapter cannot
//! change the shape of this enum and the core never depends on an adapter's
//! libraries.

use std::io;

/// Anything that can go wrong reading, writing or editing a score.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The file could not be read or written.
    #[error("io: {0}")]
    Io(#[from] io::Error),

    /// The document is structurally unusable.
    #[error("malformed: {0}")]
    Malformed(String),

    /// No compiled-in format recognised the input.
    #[error("no compiled-in format recognises this input")]
    UnknownFormat,

    /// A format adapter failed. The `format` field is that adapter's
    /// [`ReadFormat::NAME`](crate::format::ReadFormat::NAME).
    #[error("{format}: {source}")]
    Format {
        /// Which adapter reported the failure.
        format: &'static str,
        /// The adapter's own error.
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl Error {
    /// Wraps an adapter's error, tagging it with the adapter's name.
    pub fn format<E>(format: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Format {
            format,
            source: Box::new(source),
        }
    }
}

/// [`std::result::Result`] with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
