//! Stable high-level entry points for applications using [`Document`].
//!
//! The facade groups score inspection, edits, generation, arrangement,
//! assessment, and optional format adapters under one path. Its operations
//! work on the native document model; file access stays in [`mod@format`].

pub use crate::edits;
pub use crate::evaluation;
pub use crate::format;
pub use crate::inspect;
pub use crate::load;
pub use crate::model;
pub use crate::pitch;
#[cfg(feature = "gp")]
pub use crate::save;
pub use crate::selection;
pub use crate::{Document, Error, Result};

/// Exports a document as a deterministic MIDI buffer.
///
/// # Errors
///
/// Returns `Ok` for every document: MIDI encoding has no error condition today.
#[cfg(feature = "midi")]
pub fn export_midi(document: &Document) -> Result<Vec<u8>> {
    Ok(crate::format::midi::write(document))
}

/// Renders a document as a stereo 16-bit WAV buffer using a SoundFont.
///
/// # Errors
///
/// Returns any error from `crate::format::wav::render_pcm`: [`crate::Error::Io`] if the SoundFont
/// cannot be opened, and [`crate::Error::Malformed`] if it is not valid SoundFont2 data or the
/// score cannot be rendered (an invalid tempo, more than 15 pitched tracks, or output too long for
/// a WAV file).
#[cfg(feature = "wav")]
pub fn render_wav(document: &Document, soundfont: impl AsRef<std::path::Path>) -> Result<Vec<u8>> {
    crate::format::wav::render_pcm(document, soundfont)
}

/// File-boundary workflows using the compiled-in GP adapter.
#[cfg(feature = "gp")]
pub mod file;

/// Fast drum track assessment and section marker detection.
#[cfg(all(feature = "drums", feature = "arrangement"))]
pub mod assess;
#[cfg(all(feature = "drums", feature = "arrangement"))]
pub use assess::{
    assess_drums_fast, detect_marked_sections, AssessDrumsOptions, DrumAssessError,
    FastAssessmentResult, FastSectionScore, MarkedSection,
};

/// Native generation operations with typed inputs and results.
pub mod generation;
