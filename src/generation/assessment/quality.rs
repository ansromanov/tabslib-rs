//! Quality measurements over a document.
//!
//! The module owns [`measure`], which returns the quality half of an
//! [`AssessmentReport`](super::AssessmentReport): ratios describing palm-mute
//! use, percussion balance, repetition, form alignment, voicing mix and
//! register.
//!
//! No target value lives here. Every measurement is a ratio a caller compares
//! against its own samples through
//! [`calibrate`](super::calibrate), because the numbers that make a ratio good
//! are measured over a collection of music and that data does not belong in
//! this crate.

use std::collections::{BTreeMap, BTreeSet};

use super::{Diagnostic, Measurement, Metric, Region};
use crate::inspect::{self, TrackKind};
use crate::model::{Document, Technique};
use crate::percussion::{lookup, DrumRole};

/// Strings counted as the low register, from the lowest string upward.
pub const LOW_STRINGS: u32 = 3;
/// Bar count a section length is expected to be a multiple of.
pub const HYPERMETER_BARS: usize = 4;

/// Measures every quality metric and the diagnostics that support them.
pub(super) fn measure(doc: &Document) -> (BTreeMap<Metric, Measurement>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let fretted = kinds(doc, TrackKind::Other);
    let percussion = kinds(doc, TrackKind::Percussion);

    let mute = palm_mutes(doc, &fretted, &mut diagnostics);
    let cymbals = percussion_balance(doc, &percussion);
    let patterns = pattern_variety(doc);
    let voicing = voicing_mix(doc, &fretted);
    let register = register_share(doc, &fretted);
    let form = hypermeter(doc);
    let bars = u64::try_from(doc.master_bars.len()).unwrap_or(u64::MAX);

    let metrics = BTreeMap::from([
        (
            Metric::PalmMuteRate,
            Measurement::new(mute.muted, mute.notes),
        ),
        (
            Metric::PalmMuteSpread,
            Measurement::new(mute.muted_bars, mute.sounding_bars),
        ),
        (
            Metric::CymbalCoverage,
            Measurement::new(cymbals.timekeeping_bars, cymbals.bars),
        ),
        (
            Metric::TomShare,
            Measurement::new(cymbals.toms, cymbals.notes),
        ),
        (
            Metric::UniqueBarPatterns,
            Measurement::new(patterns.unique, patterns.compared),
        ),
        (
            Metric::LongestLoop,
            Measurement::new(patterns.longest_loop, 1),
        ),
        (Metric::BarCount, Measurement::new(bars, 1)),
        (
            Metric::HypermeterAlignment,
            Measurement::new(form.aligned, form.sections),
        ),
        (
            Metric::VoicingSingleNote,
            Measurement::new(voicing.single, voicing.beats),
        ),
        (
            Metric::VoicingPowerShape,
            Measurement::new(voicing.power, voicing.beats),
        ),
        (
            Metric::VoicingExtendedShape,
            Measurement::new(voicing.extended, voicing.beats),
        ),
        (
            Metric::LowStringShare,
            Measurement::new(register.low, register.positioned),
        ),
    ]);
    (metrics, diagnostics)
}

/// Density peak position, as a share of the document's bar span.
///
/// This is what "shape" means: where the music is at its densest, not how far
/// apart its first and last bars are. A document that peaks halfway through and
/// one that climbs steadily to the end have the same first-to-last delta and
/// very different shapes.
///
/// Ties resolve to the earliest peak. A document of one bar or fewer reports
/// zero, because there is no span to place a peak within.
pub(super) fn density_peak(bar_note_counts: &[u64]) -> Measurement {
    let span = bar_note_counts.len().saturating_sub(1);
    if span == 0 {
        return Measurement::new(0, 1);
    }
    let peak = bar_note_counts
        .iter()
        .enumerate()
        .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
        .map(|(index, _)| index)
        .unwrap_or(0);
    Measurement::new(
        u64::try_from(peak).unwrap_or(u64::MAX),
        u64::try_from(span).unwrap_or(u64::MAX),
    )
}

fn kinds(doc: &Document, wanted: TrackKind) -> Vec<usize> {
    (0..doc.tracks.len())
        .filter(|index| inspect::classify_track(doc, *index).map(|(kind, _)| kind) == Some(wanted))
        .collect()
}

fn bar_of(doc: &Document, bar_index: usize, track_index: usize) -> Option<&crate::model::Bar> {
    let id = *doc.master_bars.get(bar_index)?.bar_ids.get(track_index)?;
    doc.bars.iter().find(|bar| bar.id == id as u32)
}

#[derive(Default)]
struct PalmMutes {
    notes: u64,
    muted: u64,
    sounding_bars: u64,
    muted_bars: u64,
}

/// Palm-mute rate and the bar-level spread behind it.
///
/// The aggregate alone is the documented false green: chugging passages muted
/// and melodic lines open produce the same rate as a mute sprayed evenly over
/// everything. The spread, and the per-bar diagnostics, are what separate them.
fn palm_mutes(doc: &Document, fretted: &[usize], diagnostics: &mut Vec<Diagnostic>) -> PalmMutes {
    let mut out = PalmMutes::default();
    for track_index in fretted {
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, *track_index) else {
                continue;
            };
            let mut bar_notes = 0u64;
            let mut bar_muted = 0u64;
            for voice in &bar.voices {
                for beat in &voice.beats {
                    for note in &beat.notes {
                        bar_notes += 1;
                        if note.techniques.contains(&Technique::PalmMute) {
                            bar_muted += 1;
                        }
                    }
                }
            }
            if bar_notes == 0 {
                continue;
            }
            out.notes += bar_notes;
            out.muted += bar_muted;
            out.sounding_bars += 1;
            if bar_muted > 0 {
                out.muted_bars += 1;
            }
            diagnostics.push(Diagnostic {
                code: "palm_mute_bar",
                region: Region {
                    bar: bar_index,
                    track: Some(*track_index),
                    voice: None,
                    beat: None,
                },
                value: i64::try_from(bar_muted).unwrap_or(i64::MAX),
            });
        }
    }
    out
}

#[derive(Default)]
struct PercussionBalance {
    notes: u64,
    toms: u64,
    bars: u64,
    timekeeping_bars: u64,
}

fn percussion_balance(doc: &Document, percussion: &[usize]) -> PercussionBalance {
    let mut out = PercussionBalance::default();
    for track_index in percussion {
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, *track_index) else {
                continue;
            };
            let mut bar_notes = 0u64;
            let mut timekeeping = false;
            for voice in &bar.voices {
                for beat in &voice.beats {
                    for note in &beat.notes {
                        let Some(articulation) = note.articulation else {
                            continue;
                        };
                        let Some((_, role)) = lookup(articulation) else {
                            continue;
                        };
                        bar_notes += 1;
                        match role {
                            DrumRole::Tom => out.toms += 1,
                            DrumRole::HiHat | DrumRole::Ride => timekeeping = true,
                            DrumRole::Kick
                            | DrumRole::Snare
                            | DrumRole::Cymbal
                            | DrumRole::Other => {}
                        }
                    }
                }
            }
            if bar_notes == 0 {
                continue;
            }
            out.notes += bar_notes;
            out.bars += 1;
            if timekeeping {
                out.timekeeping_bars += 1;
            }
        }
    }
    out
}

#[derive(Default)]
struct PatternVariety {
    unique: u64,
    compared: u64,
    longest_loop: u64,
}

fn pattern_variety(doc: &Document) -> PatternVariety {
    let mut out = PatternVariety::default();
    let mut seen: Vec<super::BarPattern> = Vec::new();
    for track_index in 0..doc.tracks.len() {
        let mut track_patterns = Vec::new();
        for (bar_index, master) in doc.master_bars.iter().enumerate() {
            let Some(bar) = bar_of(doc, bar_index, track_index) else {
                continue;
            };
            let pattern = super::bar_pattern(bar, master.time);
            out.compared += 1;
            if !seen.contains(&pattern) {
                out.unique += 1;
                seen.push(pattern.clone());
            }
            track_patterns.push(pattern);
        }
        out.longest_loop = out.longest_loop.max(longest_repeat(&track_patterns));
    }
    out
}

/// Longest run of consecutive bars that repeats an earlier run verbatim.
///
/// This is the loop length a listener hears come back around, which is not the
/// same as the share of bars that repeat something: one twelve-bar loop and
/// twelve scattered repeats give the same repetition ratio.
fn longest_repeat(patterns: &[super::BarPattern]) -> u64 {
    let mut longest = 0usize;
    for start in 0..patterns.len() {
        for later in start + 1..patterns.len() {
            let mut run = 0usize;
            while later + run < patterns.len() && patterns[start + run] == patterns[later + run] {
                run += 1;
                if start + run == later {
                    break;
                }
            }
            longest = longest.max(run);
        }
    }
    u64::try_from(longest).unwrap_or(u64::MAX)
}

#[derive(Default)]
struct VoicingMix {
    beats: u64,
    single: u64,
    power: u64,
    extended: u64,
}

fn voicing_mix(doc: &Document, fretted: &[usize]) -> VoicingMix {
    let mut out = VoicingMix::default();
    for track_index in fretted {
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, *track_index) else {
                continue;
            };
            for voice in &bar.voices {
                for beat in &voice.beats {
                    match beat.notes.len() {
                        0 => continue,
                        1 => out.single += 1,
                        2 => out.power += 1,
                        _ => out.extended += 1,
                    }
                    out.beats += 1;
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct RegisterShare {
    positioned: u64,
    low: u64,
}

fn register_share(doc: &Document, fretted: &[usize]) -> RegisterShare {
    let mut out = RegisterShare::default();
    for track_index in fretted {
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_of(doc, bar_index, *track_index) else {
                continue;
            };
            for voice in &bar.voices {
                for beat in &voice.beats {
                    for note in &beat.notes {
                        let Some(string) = note.string else { continue };
                        out.positioned += 1;
                        if string < LOW_STRINGS {
                            out.low += 1;
                        }
                    }
                }
            }
        }
    }
    out
}

#[derive(Default)]
struct Hypermeter {
    sections: u64,
    aligned: u64,
}

fn hypermeter(doc: &Document) -> Hypermeter {
    let mut out = Hypermeter::default();
    let mut starts: Vec<usize> = doc
        .master_bars
        .iter()
        .enumerate()
        .filter(|(_, master)| master.section.is_some())
        .map(|(index, _)| index)
        .collect();
    if starts.is_empty() {
        return out;
    }
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    let end = doc.master_bars.len();
    let boundaries: BTreeSet<usize> = starts.iter().copied().collect();
    let ordered: Vec<usize> = boundaries.into_iter().collect();
    for (position, start) in ordered.iter().enumerate() {
        let stop = ordered.get(position + 1).copied().unwrap_or(end);
        let length = stop.saturating_sub(*start);
        if length == 0 {
            continue;
        }
        out.sections += 1;
        if length % HYPERMETER_BARS == 0 {
            out.aligned += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "quality_test.rs"]
mod tests;
