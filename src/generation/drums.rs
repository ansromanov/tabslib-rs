//! Drum onset analysis, pattern generation, playability, fills, and humanization.

use std::collections::BTreeMap;

use crate::inspect::{beat_positions, Fraction};
use crate::model::{Bar, Document, Note, Technique};
use crate::percussion::DrumRole;

#[cfg(all(feature = "arrangement", feature = "assessment"))]
use crate::generation::quality_gaps::DrumRoleDecision;

pub mod feel;
pub mod improvement;
#[cfg(feature = "midi")]
pub mod midi_groove;
pub mod performance;
pub mod phrase_clock;
pub mod presets;

/// A riff onset projected onto the score's exact subdivision grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiffOnset {
    /// Zero-based subdivision step within the bar.
    pub step: u32,
    /// Whether the written onset lands exactly on the derived grid.
    pub exact: bool,
    /// Exact written onset within the bar.
    pub onset: Fraction,
    /// Lowest sounding MIDI pitch, when the onset has pitched notes.
    pub lowest_pitch: Option<i32>,
    /// Whether any sounding note is palm-muted.
    pub palm_mute: bool,
    /// Whether any sounding note is accented.
    pub accent: bool,
    /// Whether every sounding note is dead.
    pub dead: bool,
    /// Number of sounding notes at the onset.
    pub polyphony: usize,
    /// Number of source tracks represented by the onset.
    pub track_count: usize,
    /// Names of source tracks represented by the onset.
    pub track_names: Vec<String>,
    /// Deterministic unison strength used by downstream pattern selectors.
    pub unison_score: usize,
}

/// A fixed-size, bar-aware boolean grid for phrase-level analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseGrid {
    /// Number of subdivisions represented by each bar.
    pub steps_per_bar: u32,
    /// One mask per bar, in score order.
    pub bars: Vec<Vec<bool>>,
}

impl PhraseGrid {
    /// Creates an empty grid with `bar_count` bars.
    pub fn new(steps_per_bar: u32, bar_count: usize) -> Self {
        let steps = steps_per_bar as usize;
        Self {
            steps_per_bar,
            bars: vec![vec![false; steps]; bar_count],
        }
    }

    /// Projects onsets onto a fixed-size phrase grid.
    ///
    /// Onsets outside the requested grid are ignored. This makes the method
    /// safe for callers combining material with different subdivision
    /// resolutions while preserving deterministic output.
    pub fn from_onsets(onsets_by_bar: &[Vec<RiffOnset>], steps_per_bar: u32) -> Self {
        let mut grid = Self::new(steps_per_bar, onsets_by_bar.len());
        for (bar_index, onsets) in onsets_by_bar.iter().enumerate() {
            for onset in onsets {
                if let Some(step) = grid.bars[bar_index].get_mut(onset.step as usize) {
                    *step = true;
                }
            }
        }
        grid
    }

    /// Returns all bar masks concatenated in score order.
    pub fn flattened(&self) -> Vec<bool> {
        self.bars.iter().flatten().copied().collect()
    }
}

/// Articulation-preserving step data consumed by future pattern generators.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PatternStep {
    /// Whether the step contains an onset.
    pub onset: bool,
    /// Whether the source material marks the step as accented.
    pub accent: bool,
    /// Whether the source material marks the step as dead.
    pub dead: bool,
    /// Whether the source material contains a palm-muted onset.
    pub palm_mute: bool,
}

/// A deterministic, bar-aware pattern description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumPattern {
    /// Number of subdivisions represented by each bar.
    pub steps_per_bar: u32,
    /// Pattern steps in score order, grouped by bar.
    pub bars: Vec<Vec<PatternStep>>,
}

/// General MIDI drum voices used by the performance solver.
pub mod voices {
    /// General MIDI percussion number: acoustic bass drum.
    pub const ACOUSTIC_BASS_DRUM: u8 = 36;
    /// General MIDI percussion number: acoustic snare.
    pub const ACOUSTIC_SNARE: u8 = 38;
    /// General MIDI percussion number: side stick.
    pub const SIDE_STICK: u8 = 37;
    /// General MIDI percussion number: pedal hi-hat.
    pub const PEDAL_HI_HAT: u8 = 44;
    /// General MIDI percussion number: closed hi-hat.
    pub const CLOSED_HI_HAT: u8 = 42;
    /// General MIDI percussion number: open hi-hat.
    pub const OPEN_HI_HAT: u8 = 46;
    /// General MIDI percussion number: low floor tom.
    pub const LOW_FLOOR_TOM: u8 = 41;
    /// General MIDI percussion number: low tom.
    pub const LOW_TOM: u8 = 45;
    /// General MIDI percussion number: low-mid tom.
    pub const LOW_MID_TOM: u8 = 47;
    /// General MIDI percussion number: high-mid tom.
    pub const HI_MID_TOM: u8 = 48;
    /// General MIDI percussion number: high tom.
    pub const HIGH_TOM: u8 = 50;
    /// General MIDI percussion number: high floor tom.
    pub const HIGH_FLOOR_TOM: u8 = 43;
    /// General MIDI percussion number: crash cymbal 1.
    pub const CRASH_CYMBAL_1: u8 = 49;
    /// General MIDI percussion number: crash cymbal 2.
    pub const CRASH_CYMBAL_2: u8 = 57;
    /// General MIDI percussion number: chinese cymbal.
    pub const CHINESE_CYMBAL: u8 = 52;
    /// General MIDI percussion number: splash cymbal.
    pub const SPLASH_CYMBAL: u8 = 55;
    /// General MIDI percussion number: ride cymbal 1.
    pub const RIDE_CYMBAL_1: u8 = 51;
    /// General MIDI percussion number: ride bell.
    pub const RIDE_BELL: u8 = 53;
}

/// A generated drum hit before physical-limb constraints are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumHit {
    /// General MIDI drum voice.
    pub midi: u8,
    /// Zero-based subdivision step across the whole pattern.
    pub step: u32,
    /// Whether the hit is an accented event.
    pub accent: bool,
    /// Whether the hit is a ghost event.
    pub ghost: bool,
    /// Base velocity before humanization.
    pub velocity: u8,
    /// Small timing displacement applied by humanization.
    pub timing_offset: i16,
    /// Performance ornament attached to this principal hit.
    pub ornament: Option<DrumOrnament>,
    /// Absolute step where an open hi-hat is choked, if paired with a later hat hit.
    pub choked_at_step: Option<u32>,
}

/// A grace-note ornament played immediately before a drum hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumOrnament {
    /// One grace hit before the principal hit.
    Flam,
    /// Two grace hits before the principal hit.
    Drag,
}

impl DrumHit {
    /// Creates a neutral hit with a stable default performance value.
    pub fn new(midi: u8, step: u32) -> Self {
        Self {
            midi,
            step,
            accent: false,
            ghost: false,
            velocity: 100,
            timing_offset: 0,
            ornament: None,
            choked_at_step: None,
        }
    }
}

/// Adds deterministic kick-and-cymbal catches at planned riff stabs.
///
/// Existing hits at the same step are preserved and only receive an accent;
/// missing voices are inserted once, so applying the overlay twice is safe.
#[cfg(all(feature = "arrangement", feature = "assessment"))]
pub fn apply_stab_catches(hits: &mut Vec<DrumHit>, decisions: &[DrumRoleDecision]) {
    for decision in decisions {
        let cymbal = match decision.role {
            DrumRole::Kick => voices::CHINESE_CYMBAL,
            DrumRole::Snare
            | DrumRole::HiHat
            | DrumRole::Tom
            | DrumRole::Ride
            | DrumRole::Cymbal
            | DrumRole::Other => voices::CRASH_CYMBAL_1,
        };
        accent_or_insert(hits, voices::ACOUSTIC_BASS_DRUM, decision.step);
        accent_or_insert(hits, cymbal, decision.step);
    }
    hits.sort_by_key(|hit| (hit.step, hit.midi));
}

fn accent_or_insert(hits: &mut Vec<DrumHit>, midi: u8, step: u32) {
    if let Some(hit) = hits
        .iter_mut()
        .find(|hit| hit.midi == midi && hit.step == step)
    {
        hit.accent = true;
    } else {
        let mut hit = DrumHit::new(midi, step);
        hit.accent = true;
        hits.push(hit);
    }
}

const SINGLE_PEDAL_THRESHOLD_MS: f64 = 111.0;

fn is_tom(midi: u8) -> bool {
    matches!(
        midi,
        voices::HIGH_TOM
            | voices::HI_MID_TOM
            | voices::LOW_MID_TOM
            | voices::LOW_TOM
            | voices::HIGH_FLOOR_TOM
            | voices::LOW_FLOOR_TOM
    )
}

fn is_timekeeping_cymbal(midi: u8) -> bool {
    matches!(
        midi,
        voices::CLOSED_HI_HAT | voices::OPEN_HI_HAT | voices::RIDE_CYMBAL_1 | voices::RIDE_BELL
    )
}

fn is_kick(midi: u8) -> bool {
    midi == voices::ACOUSTIC_BASS_DRUM
}

fn is_foot(midi: u8) -> bool {
    is_kick(midi) || midi == voices::PEDAL_HI_HAT
}

fn hand_priority(hit: &DrumHit) -> u8 {
    match hit.midi {
        voices::ACOUSTIC_SNARE | voices::SIDE_STICK => {
            if hit.accent {
                100
            } else if hit.ghost {
                75
            } else {
                90
            }
        }
        voices::CRASH_CYMBAL_1 | voices::CRASH_CYMBAL_2 | voices::CHINESE_CYMBAL => {
            if hit.accent {
                85
            } else {
                80
            }
        }
        midi if is_timekeeping_cymbal(midi) => {
            if hit.accent {
                75
            } else {
                70
            }
        }
        voices::SPLASH_CYMBAL => 60,
        voices::HIGH_TOM => 50,
        voices::HI_MID_TOM | voices::LOW_MID_TOM => 45,
        voices::LOW_TOM => 40,
        voices::LOW_FLOOR_TOM | voices::HIGH_FLOOR_TOM => 35,
        _ => 30,
    }
}

fn left_kick_steps(
    hits_by_step: &BTreeMap<u32, Vec<&DrumHit>>,
    tempo: u32,
) -> std::collections::BTreeSet<u32> {
    let ms_per_step = 60_000.0 / f64::from(tempo.max(1)) / 4.0;
    let mut result = std::collections::BTreeSet::new();
    let mut last_step = None;
    let mut right_foot = true;
    for (&step, hits) in hits_by_step {
        if !hits.iter().any(|hit| is_kick(hit.midi)) {
            continue;
        }
        let is_close = last_step
            .map(|last| {
                let gap_ms = f64::from(step.saturating_sub(last)) * ms_per_step;
                gap_ms > 0.0 && gap_ms <= SINGLE_PEDAL_THRESHOLD_MS
            })
            .unwrap_or(false);
        right_foot = if is_close { !right_foot } else { true };
        if !right_foot {
            result.insert(step);
        }
        last_step = Some(step);
    }
    result
}

/// Removes hits that cannot be played by two hands and two feet.
///
/// Applies the physical constraints of a drum kit player while leaving style selection and pattern generation to callers.
pub fn solve_limbs(hits: &[DrumHit], tempo: u32, fill_applied: bool) -> Vec<DrumHit> {
    if hits.is_empty() {
        return Vec::new();
    }
    let mut by_step: BTreeMap<u32, Vec<&DrumHit>> = BTreeMap::new();
    for hit in hits {
        by_step.entry(hit.step).or_default().push(hit);
    }
    let left_kicks = left_kick_steps(&by_step, tempo);
    let mut result = Vec::new();

    for (&step, step_hits) in &by_step {
        let has_toms = step_hits.iter().any(|hit| is_tom(hit.midi));
        let fill_step = has_toms
            || (fill_applied
                && step_hits
                    .iter()
                    .any(|hit| hit.midi == voices::ACOUSTIC_SNARE && !hit.ghost));
        let mut hands = Vec::new();
        let mut feet = Vec::new();
        for hit in step_hits {
            if is_foot(hit.midi) {
                feet.push((*hit).clone());
            } else if !(fill_step && has_toms && is_timekeeping_cymbal(hit.midi)) {
                hands.push((*hit).clone());
            }
        }
        hands.sort_by_key(|hit| std::cmp::Reverse(hand_priority(hit)));
        hands.truncate(2);
        let has_kick = feet.iter().any(|hit| is_kick(hit.midi));
        let double_kick = feet.iter().filter(|hit| is_kick(hit.midi)).count() >= 2;
        let left_kicking = left_kicks.contains(&step);
        let mut resolved_feet = Vec::new();
        for hit in feet {
            if hit.midi == voices::PEDAL_HI_HAT && (left_kicking || (has_kick && double_kick)) {
                continue;
            }
            resolved_feet.push(hit);
            if resolved_feet.len() == 2 {
                break;
            }
        }
        result.extend(hands);
        result.extend(resolved_feet);
    }
    result
}

/// Counts physical-limb violations before solving a hit list.
pub fn count_limb_violations(hits: &[DrumHit], tempo: u32) -> usize {
    let mut by_step: BTreeMap<u32, Vec<&DrumHit>> = BTreeMap::new();
    for hit in hits {
        by_step.entry(hit.step).or_default().push(hit);
    }
    let left_kicks = left_kick_steps(&by_step, tempo);
    by_step
        .iter()
        .map(|(&step, step_hits)| {
            let hands = step_hits.iter().filter(|hit| !is_foot(hit.midi)).count();
            let feet = step_hits.iter().filter(|hit| is_foot(hit.midi)).count();
            let pedal_conflict = step_hits.iter().any(|hit| hit.midi == voices::PEDAL_HI_HAT)
                && left_kicks.contains(&step);
            hands.saturating_sub(2) + feet.saturating_sub(2) + usize::from(pedal_conflict)
        })
        .sum()
}

impl DrumPattern {
    /// Projects source onsets into a pattern interface without generating new
    /// musical material.
    pub fn from_onsets(onsets_by_bar: &[Vec<RiffOnset>], steps_per_bar: u32) -> Self {
        let empty = PatternStep {
            onset: false,
            accent: false,
            dead: false,
            palm_mute: false,
        };
        let mut pattern = Self {
            steps_per_bar,
            bars: vec![vec![empty; steps_per_bar as usize]; onsets_by_bar.len()],
        };
        for (bar_index, onsets) in onsets_by_bar.iter().enumerate() {
            for onset in onsets {
                let Some(step) = pattern.bars[bar_index].get_mut(onset.step as usize) else {
                    continue;
                };
                step.onset = true;
                step.accent |= onset.accent;
                step.dead |= onset.dead;
                step.palm_mute |= onset.palm_mute;
            }
        }
        pattern
    }

    /// Converts a validated caller-owned catalog pattern to the drum
    /// generator's bar representation without changing articulation metadata.
    pub fn from_catalog_pattern(
        pattern: &super::pattern_catalog::CatalogPattern<PatternStep>,
    ) -> Result<Self, super::pattern_catalog::PatternError> {
        let steps_per_bar = u32::try_from(pattern.grid.steps_per_bar()?)
            .map_err(|_| super::pattern_catalog::PatternError::GridOverflow)?;
        Ok(Self {
            steps_per_bar,
            bars: pattern.bars.clone(),
        })
    }
}

fn bar_for(doc: &Document, bar_index: usize, track_index: usize) -> Option<&Bar> {
    let id = *doc.master_bars.get(bar_index)?.bar_ids.get(track_index)?;
    doc.bars.iter().find(|bar| bar.id == id as u32)
}

fn is_percussion(doc: &Document, track_index: usize) -> bool {
    doc.tracks[track_index].tuning.is_empty()
        || doc.tracks[track_index]
            .tuning
            .iter()
            .all(|pitch| *pitch == 0)
}

fn has_technique(note: &Note, wanted: fn(&Technique) -> bool) -> bool {
    note.techniques.iter().any(wanted)
}

fn single_track_bar_onsets(doc: &Document, track_index: usize, bar_index: usize) -> Vec<RiffOnset> {
    let Some(bar) = bar_for(doc, bar_index, track_index) else {
        return Vec::new();
    };
    let mut onsets = Vec::new();
    for (voice_index, voice) in bar.voices.iter().enumerate() {
        let positions = beat_positions(doc, bar_index, track_index, voice_index);
        for position in positions {
            let Some(beat) = voice.beats.get(position.beat_index) else {
                continue;
            };
            let sounding = beat
                .notes
                .iter()
                .filter(|note| {
                    !has_technique(note, |technique| {
                        matches!(technique, Technique::TieDestination)
                    })
                })
                .collect::<Vec<_>>();
            if sounding.is_empty() {
                continue;
            }
            let lowest_pitch = sounding.iter().filter_map(|note| note.midi).min();
            onsets.push(RiffOnset {
                step: position.step,
                exact: position.exact,
                onset: position.onset,
                lowest_pitch,
                palm_mute: sounding.iter().any(|note| {
                    has_technique(note, |technique| matches!(technique, Technique::PalmMute))
                }),
                accent: sounding.iter().any(|note| {
                    has_technique(note, |technique| matches!(technique, Technique::Accent))
                }),
                dead: sounding.iter().all(|note| {
                    has_technique(note, |technique| matches!(technique, Technique::Dead))
                }),
                polyphony: sounding.len(),
                track_count: 1,
                track_names: vec![doc.tracks[track_index].name.clone()],
                unison_score: 1,
            });
        }
    }
    onsets.sort_by_key(|onset| onset.step);
    onsets
}

/// Extracts onsets from one pitched source track for one master bar.
pub fn extract_bar_onsets(doc: &Document, track: usize, bar: usize) -> Vec<RiffOnset> {
    if track >= doc.tracks.len() || bar >= doc.master_bars.len() || is_percussion(doc, track) {
        return Vec::new();
    }
    single_track_bar_onsets(doc, track, bar)
}

/// Merges simultaneous onsets from multiple source tracks.
pub fn combine_track_onsets(onsets_by_track: &[Vec<RiffOnset>]) -> Vec<RiffOnset> {
    let mut by_step: BTreeMap<u32, Vec<&RiffOnset>> = BTreeMap::new();
    for onsets in onsets_by_track {
        for onset in onsets {
            by_step.entry(onset.step).or_default().push(onset);
        }
    }
    let total_tracks = onsets_by_track
        .iter()
        .filter(|onsets| !onsets.is_empty())
        .count();
    by_step
        .into_iter()
        .map(|(step, grouped)| {
            let effective = grouped
                .iter()
                .copied()
                .filter(|onset| !onset.dead)
                .collect::<Vec<_>>();
            let pitches = effective
                .iter()
                .filter_map(|onset| onset.lowest_pitch)
                .collect::<Vec<_>>();
            let mut track_names = grouped
                .iter()
                .flat_map(|onset| onset.track_names.iter().cloned())
                .collect::<Vec<_>>();
            track_names.sort();
            track_names.dedup();
            let track_count = if track_names.is_empty() {
                grouped.len()
            } else {
                track_names.len()
            };
            let accent = grouped.iter().any(|onset| onset.accent)
                || (total_tracks > 1 && track_count >= total_tracks.min(3));
            let mut unison_score = track_count;
            if pitches.iter().any(|pitch| *pitch < 45)
                && (pitches.iter().any(|pitch| *pitch >= 45)
                    || grouped.iter().any(|onset| onset.palm_mute))
                && track_count >= 2
            {
                unison_score += 2;
            }
            if grouped
                .iter()
                .filter(|onset| onset.palm_mute || onset.accent)
                .count()
                >= 2
            {
                unison_score += 1;
            }
            RiffOnset {
                step,
                exact: grouped.iter().any(|onset| onset.exact),
                onset: grouped[0].onset,
                lowest_pitch: pitches.into_iter().min(),
                palm_mute: grouped.iter().any(|onset| onset.palm_mute),
                accent,
                dead: grouped.iter().all(|onset| onset.dead),
                polyphony: grouped.iter().map(|onset| onset.polyphony).sum(),
                track_count,
                track_names,
                unison_score,
            }
        })
        .collect()
}

/// Extracts and combines onsets from several pitched source tracks.
pub fn extract_composite_bar_onsets(
    doc: &Document,
    tracks: &[usize],
    bar: usize,
) -> Vec<RiffOnset> {
    let per_track = tracks
        .iter()
        .map(|track| extract_bar_onsets(doc, *track, bar))
        .collect::<Vec<_>>();
    combine_track_onsets(&per_track)
}

/// Returns a boolean grid marking every onset step.
pub fn onset_step_mask(onsets: &[RiffOnset], steps: u32) -> Vec<bool> {
    let mut mask = vec![false; steps as usize];
    for onset in onsets {
        if let Some(slot) = mask.get_mut(onset.step as usize) {
            *slot = true;
        }
    }
    mask
}
