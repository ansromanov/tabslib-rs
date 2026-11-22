//! Stable JSON observations for comparing score engines.

use super::{statistics, Fraction};
use crate::model::Document;
use serde::Serialize;
use std::collections::BTreeMap;

/// A deterministic, exported view of the written score.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreObservation {
    /// Counts of the document's structural nodes.
    pub structure: StructureObservation,
    /// Score-wide metadata and navigation markers.
    pub metadata: MetadataObservation,
    /// Track properties and per-track counts.
    pub tracks: Vec<TrackObservation>,
    /// Exact written beat durations keyed as `numerator/denominator`.
    pub duration_histogram: BTreeMap<String, usize>,
    /// Technique counts keyed by their stable names.
    pub technique_counts: BTreeMap<String, usize>,
    /// Percussion articulation counts keyed by their numeric identifiers.
    pub articulation_counts: BTreeMap<String, usize>,
    /// Note counts in master-bar order.
    pub notes_per_bar: Vec<usize>,
}

/// Counts of structural score nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureObservation {
    /// Number of tracks.
    pub tracks: usize,
    /// Number of master bars.
    pub bars: usize,
    /// Number of voices.
    pub voices: usize,
    /// Number of beats, including rests.
    pub beats: usize,
    /// Number of notes.
    pub notes: usize,
}

/// Score-wide metadata relevant to conformance checks.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataObservation {
    /// Score title.
    pub title: String,
    /// Section markers in score order.
    pub sections: Vec<SectionObservation>,
    /// Repeat and navigation properties in bar order.
    pub navigation: Vec<NavigationObservation>,
    /// Tempo changes in score order.
    pub tempo_map: Vec<TempoObservation>,
    /// Initial key signature, when present.
    pub key: Option<KeyObservation>,
}

/// A section marker attached to a master bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionObservation {
    /// Zero-based master-bar index.
    pub bar: usize,
    /// Section name.
    pub name: String,
}

/// Repeat, alternate-ending, and direction data attached to a master bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationObservation {
    /// Zero-based master-bar index.
    pub bar: usize,
    /// Whether the bar starts a repeat.
    pub repeat_start: bool,
    /// Repeat playback count, when the bar ends a repeat.
    pub repeat_end: Option<u32>,
    /// Alternate-ending bit mask.
    pub alternate_ending: u32,
    /// Direction marker, when present.
    pub direction: Option<String>,
}

/// A tempo change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempoObservation {
    /// Zero-based master-bar index.
    pub at_bar: usize,
    /// Beats per minute.
    pub bpm: f64,
}

/// A key signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyObservation {
    /// Tonic pitch class, where C is zero.
    pub tonic: i8,
    /// Whether the key is minor.
    pub minor: bool,
}

/// Track properties and counts used by the differential harness.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackObservation {
    /// Track name.
    pub name: String,
    /// Open-string pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// General MIDI program number.
    pub midi_program: Option<i32>,
    /// Mixer pan.
    pub pan: Option<f64>,
    /// Mixer volume.
    pub volume: Option<f64>,
    /// Mute state.
    pub mute: bool,
    /// Solo state.
    pub solo: bool,
    /// Number of voices in this track.
    pub voices: usize,
    /// Number of beats in this track.
    pub beats: usize,
    /// Number of notes in this track.
    pub notes: usize,
}

/// Builds the stable observation exported by a native engine.
pub fn observe(doc: &Document) -> ScoreObservation {
    let stats = statistics(doc);
    let tracks = doc
        .tracks
        .iter()
        .enumerate()
        .map(|(track_index, track)| {
            let mut voices = 0;
            let mut beats = 0;
            let mut notes = 0;
            for bar_index in 0..doc.master_bars.len() {
                let Some(bar) = super::bar_for(doc, bar_index, track_index) else {
                    continue;
                };
                voices += bar.voices.len();
                for voice in &bar.voices {
                    beats += voice.beats.len();
                    notes += voice
                        .beats
                        .iter()
                        .map(|beat| beat.notes.len())
                        .sum::<usize>();
                }
            }
            TrackObservation {
                name: track.name.clone(),
                tuning: track.tuning.clone(),
                midi_program: track.midi_program,
                pan: track.pan,
                volume: track.volume,
                mute: track.mute,
                solo: track.solo,
                voices,
                beats,
                notes,
            }
        })
        .collect();

    let duration_histogram = stats
        .durations
        .into_iter()
        .map(|(fraction, count)| (fraction_key(fraction), count))
        .collect();
    let articulation_counts = stats
        .articulations
        .into_iter()
        .map(|(articulation, count)| (articulation.to_string(), count))
        .collect();
    let sections = doc
        .master_bars
        .iter()
        .filter_map(|bar| {
            bar.section.clone().map(|name| SectionObservation {
                bar: bar.index,
                name,
            })
        })
        .collect();
    let navigation = doc
        .master_bars
        .iter()
        .map(|bar| NavigationObservation {
            bar: bar.index,
            repeat_start: bar.repeat_start,
            repeat_end: bar.repeat_end,
            alternate_ending: bar.alternate_ending,
            direction: bar.direction.clone(),
        })
        .collect();
    let metadata = MetadataObservation {
        title: doc.title.clone(),
        sections,
        navigation,
        tempo_map: doc
            .tempo_map
            .iter()
            .map(|tempo| TempoObservation {
                at_bar: tempo.at_bar,
                bpm: tempo.bpm,
            })
            .collect(),
        key: doc.key.map(|key| KeyObservation {
            tonic: key.tonic,
            minor: key.minor,
        }),
    };

    ScoreObservation {
        structure: StructureObservation {
            tracks: stats.tracks,
            bars: stats.bars,
            voices: stats.voices,
            beats: stats.beats,
            notes: stats.notes,
        },
        metadata,
        tracks,
        duration_histogram,
        technique_counts: stats.techniques,
        articulation_counts,
        notes_per_bar: stats.notes_per_bar,
    }
}

/// Serializes a score observation as deterministic JSON.
pub fn to_json(doc: &Document) -> Result<String, serde_json::Error> {
    serde_json::to_string(&observe(doc))
}

fn fraction_key(fraction: Fraction) -> String {
    format!("{}/{}", fraction.numerator, fraction.denominator)
}

#[cfg(test)]
#[path = "observation_test.rs"]
mod tests;
