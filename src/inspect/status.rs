//! High-level song status, capability, and exact per-bar rhythm reports.

use serde::{Deserialize, Serialize};

use crate::model::{Document, GeneratorMetadata};

/// A compact status snapshot of a score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongStatus {
    /// Score title.
    pub title: String,
    /// Number of written bars.
    pub bars: usize,
    /// Number of section markers.
    pub sections: usize,
    /// Track summaries as `(name, kind, sounding_bars)`.
    pub tracks: Vec<(String, String, usize)>,
    /// Initial tempo, when present.
    pub initial_tempo: Option<f64>,
    /// Total duration as an exact whole-note fraction.
    pub duration: (u64, u64),
    /// Index of the first pitched track, when one exists.
    pub primary_pitched_track: Option<usize>,
    /// Generator metadata aligned with the document's track indexes.
    pub generator_metadata: Vec<Option<GeneratorMetadata>>,
}

/// Model-level capabilities available for a score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReport {
    /// The document contains at least one pitched track.
    pub pitched_tracks: bool,
    /// The document contains a percussion track.
    pub percussion_tracks: bool,
    /// Notes have string and fret locations that can be edited.
    pub fretboard_editing: bool,
    /// Exact rhythm information is available.
    pub exact_rhythm: bool,
    /// The document can be deterministically inspected without a provider.
    pub offline_inspection: bool,
}

/// Exact rhythm facts for one track and bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RhythmAnalysis {
    /// Zero-based track index.
    pub track: usize,
    /// Zero-based master-bar index.
    pub bar: usize,
    /// Number of voices.
    pub voices: usize,
    /// Number of beats across all voices.
    pub beats: usize,
    /// Number of sounding beats.
    pub sounding_beats: usize,
    /// Number of rests.
    pub rests: usize,
    /// Exact duration of each voice as `(numerator, denominator)`.
    pub voice_durations: Vec<(u64, u64)>,
    /// Written dynamic labels in score order.
    pub dynamics: Vec<String>,
}

/// Builds a high-level status snapshot.
pub fn song_status(doc: &Document) -> SongStatus {
    let summary = crate::inspect::summary(doc);
    let tracks = doc
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let (kind, sounding) = crate::inspect::classify_track(doc, index)
                .map(|(kind, bars)| (format!("{kind:?}"), bars))
                .unwrap_or_else(|| ("Unknown".to_string(), 0));
            (track.name.clone(), kind, sounding)
        })
        .collect();
    SongStatus {
        title: doc.title.clone(),
        bars: summary.bars,
        sections: doc.section_count(),
        tracks,
        initial_tempo: doc.tempo_map.first().map(|tempo| tempo.bpm),
        duration: (summary.duration.numerator, summary.duration.denominator),
        primary_pitched_track: doc
            .tracks
            .iter()
            .enumerate()
            .find_map(|(index, track)| (!track.tuning.is_empty()).then_some(index)),
        generator_metadata: doc
            .tracks
            .iter()
            .map(|track| track.generator_metadata.clone())
            .collect(),
    }
}

/// Reports capabilities implied by the model contents.
pub fn capability_report(doc: &Document) -> CapabilityReport {
    let pitched_tracks = doc.tracks.iter().any(|track| !track.tuning.is_empty());
    let percussion_tracks = doc.tracks.iter().any(|track| track.tuning.is_empty());
    let fretboard_editing = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .any(|note| note.string.is_some() || note.fret.is_some());
    CapabilityReport {
        pitched_tracks,
        percussion_tracks,
        fretboard_editing,
        exact_rhythm: true,
        offline_inspection: true,
    }
}

/// Analyzes one bar without converting rhythm to floating point.
pub fn rhythm_analysis(doc: &Document, track: usize, bar: usize) -> Option<RhythmAnalysis> {
    let master = doc.master_bars.get(bar)?;
    let id = u32::try_from(*master.bar_ids.get(track)?).ok()?;
    let score_bar = doc.bars.iter().find(|candidate| candidate.id == id)?;
    let mut result = RhythmAnalysis {
        track,
        bar,
        voices: score_bar.voices.len(),
        beats: 0,
        sounding_beats: 0,
        rests: 0,
        voice_durations: Vec::new(),
        dynamics: Vec::new(),
    };
    for voice in &score_bar.voices {
        let mut duration = crate::inspect::Fraction::new(0, 1);
        for beat in &voice.beats {
            result.beats += 1;
            let (num, den) = beat.rhythm.as_fraction();
            duration = duration.plus(crate::inspect::Fraction::new(num, den));
            if beat.notes.is_empty() {
                result.rests += 1;
            } else {
                result.sounding_beats += 1;
            }
            if let Some(dynamic) = &beat.dynamic {
                result.dynamics.push(dynamic.clone());
            }
        }
        result
            .voice_durations
            .push((duration.numerator, duration.denominator));
    }
    Some(result)
}

#[cfg(test)]
#[path = "status_test.rs"]
mod tests;
