//! File-boundary workflows over the native document facade.
//!
//! The functions in this module deliberately accept bytes rather than paths:
//! storage, permissions, and atomic replacement belong to the application.
//! The GP adapter is used only at this boundary; all generation remains a
//! document operation.

use crate::format::gp::Gp;
use crate::format::{ReadFormat, WriteFormat};
use crate::generation::accompaniment::{AccompanimentRequest, AccompanimentResult};
#[cfg(feature = "arrangement")]
use crate::generation::arrangement::{ArrangementError, ArrangementPlan, Motif};
#[cfg(feature = "drums")]
use crate::generation::drums::performance::DrumProfile;
#[cfg(all(feature = "drums", feature = "rhythm", feature = "harmony"))]
use crate::generation::rhythm::BassGenerationConfig;
#[cfg(feature = "arrangement")]
use crate::generation::riff_composition::{
    RiffCompositionConfig, RiffCompositionError, RiffSource,
};
#[cfg(all(feature = "drums", feature = "rhythm", feature = "harmony"))]
use crate::generation::solo::SoloConfig;
#[cfg(feature = "arrangement")]
use crate::generation::transitions::{SectionTransition, TransitionRule};
#[cfg(all(feature = "drums", feature = "rhythm"))]
use crate::generation::workflows::GeneratedTrack;
use crate::model::Document;
use crate::{Error, Result};

/// Reads a GP document from bytes.
///
/// # Errors
///
/// Returns any error from [`Gp::read`] if `bytes` are not a valid GP container or score.
pub fn read(bytes: &[u8]) -> Result<Document> {
    Gp::read(bytes)
}

/// Writes a native document as deterministic GP bytes.
///
/// # Errors
///
/// Returns any error from [`Gp::write`], such as [`Error::Format`] or [`Error::Io`] if the GP
/// archive cannot be written.
pub fn write(document: &Document) -> Result<Vec<u8>> {
    Gp::write(document)
}

/// Removes one track from GP bytes and returns the updated file bytes.
///
/// # Errors
///
/// Returns any error from [`read`], or [`Error::Malformed`] if `track_index` is outside the score,
/// is the only track, or the removal would leave an over-full voice. Also returns any error from
/// [`write()`].
pub fn remove_track(bytes: &[u8], track_index: usize) -> Result<Vec<u8>> {
    let mut document = read(bytes)?;
    crate::edits::remove_track(&mut document, track_index)
        .map_err(|error| Error::Malformed(format!("cannot remove track: {error:?}")))?;
    write(&document)
}

/// Renders GP bytes to a deterministic MIDI buffer.
///
/// # Errors
///
/// Returns any error from [`read`] if `bytes` are not a valid GP file.
#[cfg(feature = "midi")]
pub fn export_midi(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(crate::format::midi::write(&read(bytes)?))
}

/// Renders GP bytes to a stereo 16-bit WAV buffer using a SoundFont.
///
/// # Errors
///
/// Returns any error from [`read`], or any error from `crate::format::wav::render_pcm`:
/// [`Error::Io`] if the SoundFont cannot be opened, and [`Error::Malformed`] if it is invalid or
/// the score cannot be rendered (an invalid tempo, more than 15 pitched tracks, or output too
/// long).
#[cfg(feature = "wav")]
pub fn render_wav(bytes: &[u8], soundfont: impl AsRef<std::path::Path>) -> Result<Vec<u8>> {
    crate::format::wav::render_pcm(&read(bytes)?, soundfont)
}

fn transform<F>(bytes: &[u8], operation: F) -> Result<Vec<u8>>
where
    F: FnOnce(&mut Document) -> Result<()>,
{
    let mut document = read(bytes)?;
    operation(&mut document)?;
    write(&document)
}

/// Reads and writes a file without changing its document model.
///
/// # Errors
///
/// Returns any error from [`read`], or any error from [`write()`] on the unchanged document.
pub fn persist(bytes: &[u8]) -> Result<Vec<u8>> {
    transform(bytes, |_| Ok(()))
}

/// Alias for [`persist`], named for callers exposing a file workflow menu.
///
/// # Errors
///
/// Returns the same errors as [`persist`].
pub fn persist_file(bytes: &[u8]) -> Result<Vec<u8>> {
    persist(bytes)
}

/// Generates drums in a file and returns the written result and track report.
///
/// # Errors
///
/// Returns any error from [`read`], any error from [`crate::facade::generation::generate_drums`]
/// (such as [`Error::Malformed`] for invalid source tracks or an unsupported meter), or any error
/// from [`write()`].
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_drums(
    bytes: &[u8],
    request: crate::facade::generation::DrumGenerationRequest<'_>,
) -> Result<(Vec<u8>, GeneratedTrack)> {
    let mut document = read(bytes)?;
    let generated = crate::facade::generation::generate_drums(&mut document, request)?;
    Ok((write(&document)?, generated))
}

/// File-workflow spelling of [`generate_drums`].
///
/// # Errors
///
/// Returns the same errors as [`generate_drums`].
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_drums_for_file(
    bytes: &[u8],
    request: crate::facade::generation::DrumGenerationRequest<'_>,
) -> Result<(Vec<u8>, GeneratedTrack)> {
    generate_drums(bytes, request)
}

/// Generates bass in a file and returns the written result and track report.
///
/// # Errors
///
/// Returns any error from [`read`], any error from [`crate::facade::generation::generate_bass`]
/// (such as [`Error::Malformed`] for invalid source tracks, an unsupported tuning, or notes that
/// exceed `max_fret`), or any error from [`write()`].
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_bass(
    bytes: &[u8],
    request: crate::facade::generation::BassGenerationRequest<'_>,
) -> Result<(Vec<u8>, GeneratedTrack)> {
    let mut document = read(bytes)?;
    let generated = crate::facade::generation::generate_bass(&mut document, request)?;
    Ok((write(&document)?, generated))
}

/// File-workflow spelling of [`generate_bass`].
///
/// # Errors
///
/// Returns the same errors as [`generate_bass`].
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_bass_for_file(
    bytes: &[u8],
    request: crate::facade::generation::BassGenerationRequest<'_>,
) -> Result<(Vec<u8>, GeneratedTrack)> {
    generate_bass(bytes, request)
}

/// Generates harmony in a file.
///
/// # Errors
///
/// Returns any error from [`read`], any error from [`crate::facade::generation::generate_harmony`]
/// (such as [`Error::Malformed`] for an invalid source track, tuning or interval, or a source with
/// no pitched notes), or any error from [`write()`].
#[cfg(feature = "harmony")]
pub fn generate_harmony(
    bytes: &[u8],
    request: crate::facade::generation::HarmonyConfig,
) -> Result<(Vec<u8>, crate::facade::generation::GeneratedHarmony)> {
    let mut document = read(bytes)?;
    let generated = crate::facade::generation::generate_harmony(&mut document, request)?;
    Ok((write(&document)?, generated))
}

/// File-workflow spelling of [`generate_harmony`].
///
/// # Errors
///
/// Returns the same errors as [`generate_harmony`].
#[cfg(feature = "harmony")]
pub fn generate_harmony_for_file(
    bytes: &[u8],
    request: crate::facade::generation::HarmonyConfig,
) -> Result<(Vec<u8>, crate::facade::generation::GeneratedHarmony)> {
    generate_harmony(bytes, request)
}

/// Generates a lead in a file.
///
/// # Errors
///
/// Returns any error from [`read`], any error from [`crate::facade::generation::generate_solo`]
/// (such as [`Error::Malformed`] for an invalid bar range, key, scale, register or tuning), or any
/// error from [`write()`].
#[cfg(feature = "harmony")]
pub fn generate_solo(
    bytes: &[u8],
    request: crate::facade::generation::SoloConfig,
) -> Result<(Vec<u8>, crate::facade::generation::GeneratedSolo)> {
    let mut document = read(bytes)?;
    let generated = crate::facade::generation::generate_solo(&mut document, &request)?;
    Ok((write(&document)?, generated))
}

/// File-workflow spelling of [`generate_solo`].
///
/// # Errors
///
/// Returns the same errors as [`generate_solo`].
#[cfg(feature = "harmony")]
pub fn generate_solo_for_file(
    bytes: &[u8],
    request: crate::facade::generation::SoloConfig,
) -> Result<(Vec<u8>, crate::facade::generation::GeneratedSolo)> {
    generate_solo(bytes, request)
}

/// Layers drums, bass, and a lead over one file in a single workflow.
///
/// # Errors
///
/// Returns any error from [`read`], any error from the drum, bass or solo generators (such as
/// [`Error::Malformed`] for invalid source tracks, tuning or solo settings), or any error from
/// [`write()`].
#[cfg(all(feature = "drums", feature = "rhythm", feature = "harmony"))]
pub fn build_full_song(
    bytes: &[u8],
    source_tracks: &[usize],
    drum_profile: &DrumProfile,
    drum_seed: u64,
    tempo: u32,
    bass: BassGenerationConfig<'_>,
    solo: &SoloConfig,
) -> Result<Vec<u8>> {
    let mut document = read(bytes)?;
    crate::generation::workflows::generate_drums(
        &mut document,
        source_tracks,
        drum_profile,
        drum_seed,
        tempo,
    )?;
    crate::generation::workflows::generate_bass(&mut document, source_tracks, bass)?;
    crate::generation::solo::generate_solo(&mut document, solo)?;
    write(&document)
}

/// Derives a sibling output path with a stable suffix.
pub fn derive_output_path(input: &str, suffix: &str) -> String {
    let trimmed = input.trim_end_matches('/');
    match trimmed.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}{suffix}"),
        None => format!("{trimmed}{suffix}"),
    }
}

/// Returns the source bytes as a recoverable backup payload before overwrite.
pub fn create_backup(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

/// Writes an execution log to its sidecar representation.
///
/// # Errors
///
/// Returns [`Error::Malformed`] carrying the serializer's message if the log cannot be encoded as
/// JSON.
pub fn write_generator_log(log: &crate::generation::execution_log::ExecutionLog) -> Result<String> {
    log.to_json()
        .map_err(|error| Error::Malformed(error.to_string()))
}

/// Reads an execution log from its sidecar representation.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `json` is not a valid execution log or was written with a
/// schema version this build cannot read.
pub fn read_generator_log(json: &str) -> Result<crate::generation::execution_log::ExecutionLog> {
    crate::generation::execution_log::ExecutionLog::from_json(json)
        .map_err(|error| Error::Malformed(error.to_string()))
}

/// Writes a new riff document from a specification.
///
/// # Errors
///
/// Returns the [`crate::facade::generation::RiffSpecError`] from validating `spec` (no bars or
/// degrees, a degree outside 1..=7, a tuning that is too short, a non-positive tempo, or a degree
/// that cannot be fretted), or
/// [`RiffSpecError::Unbuildable`](crate::facade::generation::RiffSpecError::Unbuildable) if the
/// document cannot be written as GP.
#[cfg(feature = "riffs")]
pub fn generate_riff(
    spec: &crate::facade::generation::RiffSpec,
) -> std::result::Result<Vec<u8>, crate::facade::generation::RiffSpecError> {
    let document = crate::facade::generation::write_riff(spec)?;
    write(&document)
        .map_err(|error| crate::facade::generation::RiffSpecError::Unbuildable(error.to_string()))
}

/// File-workflow spelling of [`generate_riff`].
///
/// # Errors
///
/// Returns the same errors as [`generate_riff`].
#[cfg(feature = "riffs")]
pub fn generate_riff_for_file(
    spec: &crate::facade::generation::RiffSpec,
) -> std::result::Result<Vec<u8>, crate::facade::generation::RiffSpecError> {
    generate_riff(spec)
}

/// Applies the selected accompaniment layers to a file.
///
/// # Errors
///
/// Returns any error from [`read`], any error from
/// [`crate::facade::generation::generate_accompaniment`] (such as [`Error::Malformed`] for invalid
/// source tracks or settings), or any error from [`write()`].
pub fn generate_accompaniment(
    bytes: &[u8],
    request: &AccompanimentRequest<'_>,
) -> Result<(Vec<u8>, AccompanimentResult)> {
    let mut document = read(bytes)?;
    let result = crate::facade::generation::generate_accompaniment(&mut document, request)?;
    Ok((write(&document)?, result))
}

/// Assembles a reviewed arrangement plan from a file.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if `bytes` cannot be read or the result cannot be
/// written as GP, and otherwise any error from composing `plan` with `motifs`.
#[cfg(feature = "arrangement")]
pub fn assemble(
    bytes: &[u8],
    motifs: &[Motif],
    plan: &ArrangementPlan,
) -> std::result::Result<Vec<u8>, ArrangementError> {
    let source = read(bytes).map_err(|error| ArrangementError::InvalidInput(error.to_string()))?;
    let document = crate::generation::arrangement::compose_plan(&source, motifs, plan)?;
    write(&document).map_err(|error| ArrangementError::InvalidInput(error.to_string()))
}

/// File-workflow spelling of [`assemble`].
///
/// # Errors
///
/// Returns the same errors as [`assemble`].
#[cfg(feature = "arrangement")]
pub fn assemble_native_document_file(
    bytes: &[u8],
    motifs: &[Motif],
    plan: &ArrangementPlan,
) -> std::result::Result<Vec<u8>, ArrangementError> {
    assemble(bytes, motifs, plan)
}

/// Reads and concatenates several riff files into one deterministic file.
///
/// # Errors
///
/// Returns [`RiffCompositionError::InvalidInput`] if a file cannot be read or the result cannot be
/// written as GP. Otherwise returns any error from composing the sources, such as
/// [`RiffCompositionError::InvalidInput`] for no files or mismatched track layouts, or
/// [`RiffCompositionError::Transposition`] if a source cannot be moved to the target key.
#[cfg(feature = "arrangement")]
pub fn compose_riff_files(
    files: &[(&str, &[u8], u8)],
    config: RiffCompositionConfig,
) -> std::result::Result<Vec<u8>, RiffCompositionError> {
    let sources = files
        .iter()
        .map(|(label, bytes, energy)| {
            read(bytes)
                .map(|document| RiffSource {
                    label: (*label).into(),
                    document,
                    energy: *energy,
                })
                .map_err(|error| RiffCompositionError::InvalidInput(error.to_string()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let document = crate::generation::riff_composition::compose_from_riffs(&sources, config)?;
    write(&document).map_err(|error| RiffCompositionError::InvalidInput(error.to_string()))
}

/// Applies transition rules to a file and returns resolved boundary reports.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if `bytes` cannot be read or the result cannot be
/// written as GP, or if a rule names a missing track, has no steps, or is otherwise invalid.
#[cfg(feature = "arrangement")]
pub fn apply_transitions(
    bytes: &[u8],
    rules: &[TransitionRule],
) -> std::result::Result<(Vec<u8>, Vec<SectionTransition>), ArrangementError> {
    let mut document =
        read(bytes).map_err(|error| ArrangementError::InvalidInput(error.to_string()))?;
    let transitions = crate::generation::transitions::apply_transitions(&mut document, rules)?;
    let output =
        write(&document).map_err(|error| ArrangementError::InvalidInput(error.to_string()))?;
    Ok((output, transitions))
}

/// Transposes all pitched material in a file.
///
/// # Errors
///
/// Returns any error from [`read`], [`Error::Malformed`] if the score is empty or a transposed
/// note would have a negative fret or no matching string, or any error from [`write()`].
pub fn transpose(bytes: &[u8], semitones: i32) -> Result<Vec<u8>> {
    transform(bytes, |document| {
        crate::pitch::transpose(document, semitones)
            .map_err(|error| Error::Malformed(format!("{error:?}")))
    })
}

/// File-workflow spelling of [`transpose`].
///
/// # Errors
///
/// Returns the same errors as [`transpose`].
pub fn transpose_native_document_file(bytes: &[u8], semitones: i32) -> Result<Vec<u8>> {
    transpose(bytes, semitones)
}

/// Retunes all fretted tracks while preserving their written pitches.
///
/// # Errors
///
/// Returns any error from [`read`], [`Error::Malformed`] if `tuning` is empty or has a pitch
/// outside 0..=127, if the score is empty, or if a note cannot be placed on its string in the new
/// tuning, or any error from [`write()`].
pub fn retune_preserve_pitch(bytes: &[u8], tuning: &[i32]) -> Result<Vec<u8>> {
    transform(bytes, |document| {
        for track in 0..document.tracks.len() {
            crate::pitch::retune_preserve_pitch(document, track, tuning)
                .map_err(|error| Error::Malformed(format!("{error:?}")))?;
        }
        Ok(())
    })
}

/// File-workflow spelling of [`retune_preserve_pitch`].
///
/// # Errors
///
/// Returns the same errors as [`retune_preserve_pitch`].
pub fn retune_native_document_file(bytes: &[u8], tuning: &[i32]) -> Result<Vec<u8>> {
    retune_preserve_pitch(bytes, tuning)
}

/// Retunes all fretted tracks while preserving their fingering.
///
/// # Errors
///
/// Returns any error from [`read`], [`Error::Malformed`] if `tuning` is empty or has a pitch
/// outside 0..=127, if the score is empty, or if a note cannot be placed on its string in the new
/// tuning, or any error from [`write()`].
pub fn retune_preserve_fingering(bytes: &[u8], tuning: &[i32]) -> Result<Vec<u8>> {
    transform(bytes, |document| {
        for track in 0..document.tracks.len() {
            crate::pitch::retune_preserve_fingering(document, track, tuning)
                .map_err(|error| Error::Malformed(format!("{error:?}")))?;
        }
        Ok(())
    })
}

/// File-workflow spelling of [`retune_preserve_fingering`].
///
/// # Errors
///
/// Returns the same errors as [`retune_preserve_fingering`].
pub fn retune_fingering_native_document_file(bytes: &[u8], tuning: &[i32]) -> Result<Vec<u8>> {
    retune_preserve_fingering(bytes, tuning)
}
