//! Descriptive profiles read from an authored percussion track.
//!
//! [`profile_existing_drums`] counts mapped kit roles per bar, finds the
//! dominant timekeeping cymbal family, and summarizes written rhythmic feel.
//! It contains no genre or repertoire defaults.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::inspect::{self, BarFeel, TrackKind};
use crate::model::Document;
use crate::percussion::{lookup, DrumRole};

use super::{performance::DrumProfile, voices};

/// Dominant timekeeping cymbal family in an authored percussion track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CymbalPreference {
    /// Closed, pedal, or open hi-hat family.
    Hat,
    /// Ride cymbal or ride bell family.
    Ride,
    /// Crash, splash, or another non-China cymbal family.
    CrashRide,
    /// China cymbal.
    China,
    /// No cymbal family accounts for more than half the observed hits.
    Mixed,
}

/// Rhythmic feel summarized across the sounding bars of a percussion track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrumFeel {
    /// Even written subdivisions.
    Straight,
    /// Explicit triplet subdivisions.
    Triplet,
    /// Unequal paired subdivisions indicating swing.
    Swung,
    /// Sounding bars use more than one supported feel.
    Mixed,
    /// The score uses a meter outside the written feel detector's grid.
    Unsupported,
}

/// Measured role densities and feel for one existing percussion track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumFeelProfile {
    /// Dominant cymbal family, or `None` when the track has no cymbal hits.
    pub cymbal_preference: Option<CymbalPreference>,
    /// Kick note counts, one entry per score bar.
    pub kick_density_per_bar: Vec<usize>,
    /// Snare note counts, one entry per score bar.
    pub snare_density_per_bar: Vec<usize>,
    /// Timekeeping or other cymbal note counts, one entry per score bar.
    pub cymbal_density_per_bar: Vec<usize>,
    /// Summarized feel across sounding bars.
    pub feel: DrumFeel,
    /// Most common mapped kick articulation, if present.
    pub kick_voice: Option<u8>,
    /// Most common mapped snare articulation, if present.
    pub snare_voice: Option<u8>,
    /// Most common articulation in the selected cymbal family, if present.
    pub cymbal_voice: Option<u8>,
}

impl DrumFeelProfile {
    /// Converts measured densities to the existing seeded-generator profile.
    ///
    /// Each hit per bar maps to 25 probability points, capped at 100. With no
    /// cymbal observations, the generated cymbal probability is zero and the
    /// placeholder voice is inert; `cymbal_preference` remains `None`.
    pub fn to_drum_profile(&self) -> DrumProfile {
        DrumProfile {
            kick_voice: self.kick_voice.unwrap_or(voices::ACOUSTIC_BASS_DRUM),
            snare_voice: self.snare_voice.unwrap_or(voices::ACOUSTIC_SNARE),
            cymbal_voice: self.cymbal_voice.unwrap_or(voices::CLOSED_HI_HAT),
            kick_probability: density_probability(&self.kick_density_per_bar),
            snare_probability: density_probability(&self.snare_density_per_bar),
            cymbal_probability: if self.cymbal_preference.is_some() {
                density_probability(&self.cymbal_density_per_bar)
            } else {
                0
            },
        }
    }
}

/// Profiles an existing percussion track without changing the document.
pub fn profile_existing_drums(
    document: &Document,
    track_index: usize,
) -> Result<DrumFeelProfile, DrumFeelError> {
    let Some((TrackKind::Percussion, _)) = inspect::classify_track(document, track_index) else {
        return Err(if track_index >= document.tracks.len() {
            DrumFeelError::TrackOutsideDocument { track_index }
        } else {
            DrumFeelError::NotPercussionTrack { track_index }
        });
    };

    let mut kick_density_per_bar = Vec::with_capacity(document.master_bars.len());
    let mut snare_density_per_bar = Vec::with_capacity(document.master_bars.len());
    let mut cymbal_density_per_bar = Vec::with_capacity(document.master_bars.len());
    let mut kick_voices = BTreeMap::new();
    let mut snare_voices = BTreeMap::new();
    let mut cymbal_voices = BTreeMap::new();
    let mut cymbal_families = BTreeMap::new();
    let mut feels = BTreeSet::new();

    for (bar_index, master_bar) in document.master_bars.iter().enumerate() {
        let bar_id = master_bar
            .bar_ids
            .get(track_index)
            .and_then(|id| u32::try_from(*id).ok())
            .ok_or(DrumFeelError::MissingBar { bar_index })?;
        let bar = document
            .bars
            .iter()
            .find(|bar| bar.id == bar_id)
            .ok_or(DrumFeelError::MissingBar { bar_index })?;
        let mut kicks = 0;
        let mut snares = 0;
        let mut cymbals = 0;
        let sounding = bar
            .voices
            .iter()
            .any(|voice| voice.beats.iter().any(|beat| !beat.notes.is_empty()));
        if sounding {
            feels.insert(drum_feel(inspect::detect_bar_feel(
                document,
                bar_index,
                track_index,
                0,
            )));
        }
        for note in bar
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
        {
            let mapped = note
                .articulation
                .and_then(lookup)
                .or_else(|| note.midi.and_then(lookup));
            let Some((midi, role)) = mapped else {
                continue;
            };
            match role {
                DrumRole::Kick => {
                    kicks += 1;
                    *kick_voices.entry(midi).or_insert(0) += 1;
                }
                DrumRole::Snare => {
                    snares += 1;
                    *snare_voices.entry(midi).or_insert(0) += 1;
                }
                DrumRole::HiHat | DrumRole::Ride | DrumRole::Cymbal => {
                    cymbals += 1;
                    *cymbal_voices.entry(midi).or_insert(0) += 1;
                    *cymbal_families
                        .entry(cymbal_family(role, midi))
                        .or_insert(0) += 1;
                }
                DrumRole::Tom | DrumRole::Other => {}
            }
        }
        kick_density_per_bar.push(kicks);
        snare_density_per_bar.push(snares);
        cymbal_density_per_bar.push(cymbals);
    }

    let cymbal_preference = dominant_cymbal(&cymbal_families);
    Ok(DrumFeelProfile {
        cymbal_preference,
        kick_density_per_bar,
        snare_density_per_bar,
        cymbal_density_per_bar,
        feel: summarize_feel(&feels),
        kick_voice: most_common_voice(&kick_voices),
        snare_voice: most_common_voice(&snare_voices),
        cymbal_voice: selected_cymbal_voice(cymbal_preference, &cymbal_voices, &cymbal_families),
    })
}

fn density_probability(density_per_bar: &[usize]) -> u8 {
    if density_per_bar.is_empty() {
        return 0;
    }
    let total = density_per_bar
        .iter()
        .map(|count| u128::try_from(*count).unwrap_or(u128::MAX))
        .sum::<u128>();
    let bars = u128::try_from(density_per_bar.len()).unwrap_or(u128::MAX);
    let probability = total.saturating_mul(25).saturating_add(bars / 2) / bars;
    u8::try_from(probability.min(100)).unwrap_or(100)
}

fn drum_feel(feel: BarFeel) -> DrumFeel {
    match feel {
        BarFeel::Straight => DrumFeel::Straight,
        BarFeel::Triplet => DrumFeel::Triplet,
        BarFeel::Swung => DrumFeel::Swung,
        BarFeel::Unsupported => DrumFeel::Unsupported,
    }
}

fn summarize_feel(feels: &BTreeSet<DrumFeel>) -> DrumFeel {
    match (feels.len(), feels.first()) {
        (0, _) => DrumFeel::Straight,
        (1, Some(feel)) => *feel,
        _ => DrumFeel::Mixed,
    }
}

fn cymbal_family(role: DrumRole, midi: u8) -> CymbalPreference {
    match role {
        DrumRole::HiHat => CymbalPreference::Hat,
        DrumRole::Ride => CymbalPreference::Ride,
        DrumRole::Cymbal if midi == 52 => CymbalPreference::China,
        DrumRole::Cymbal => CymbalPreference::CrashRide,
        DrumRole::Kick | DrumRole::Snare | DrumRole::Tom | DrumRole::Other => {
            CymbalPreference::Mixed
        }
    }
}

fn dominant_cymbal(counts: &BTreeMap<CymbalPreference, usize>) -> Option<CymbalPreference> {
    let total = counts.values().sum::<usize>();
    let (family, count) = counts.iter().max_by_key(|(_, count)| *count)?;
    Some(if count.saturating_mul(2) > total {
        *family
    } else {
        CymbalPreference::Mixed
    })
}

fn most_common_voice(counts: &BTreeMap<u8, usize>) -> Option<u8> {
    counts
        .iter()
        .max_by_key(|(voice, count)| (**count, std::cmp::Reverse(**voice)))
        .map(|(voice, _)| *voice)
}

fn selected_cymbal_voice(
    preference: Option<CymbalPreference>,
    voices: &BTreeMap<u8, usize>,
    families: &BTreeMap<CymbalPreference, usize>,
) -> Option<u8> {
    let family = preference.filter(|preference| *preference != CymbalPreference::Mixed);
    if let Some(family) = family {
        voices
            .iter()
            .filter(|(voice, _)| {
                lookup(i32::from(**voice))
                    .is_some_and(|(midi, role)| cymbal_family(role, midi) == family)
            })
            .max_by_key(|(voice, count)| (**count, std::cmp::Reverse(**voice)))
            .map(|(voice, _)| *voice)
    } else if families.is_empty() {
        None
    } else {
        most_common_voice(voices)
    }
}

/// Why an authored track could not be profiled.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DrumFeelError {
    /// Track index is outside the document.
    #[error("track {track_index} is outside the document")]
    TrackOutsideDocument {
        /// Zero-based index of the offending track.
        track_index: usize,
    },
    /// The selected track is not classified as percussion.
    #[error("track {track_index} is not a percussion track")]
    NotPercussionTrack {
        /// Zero-based index of the offending track.
        track_index: usize,
    },
    /// The selected track has no bar referenced at the given score position.
    #[error("percussion track is missing the bar at score position {bar_index}")]
    MissingBar {
        /// Zero-based score position of the missing bar.
        bar_index: usize,
    },
}

#[cfg(test)]
#[path = "feel_test.rs"]
mod tests;
