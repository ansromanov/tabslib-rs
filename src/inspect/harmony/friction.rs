//! Dissonance between two tracks: pairs of overlapping pitched notes weighted by overlap.

use super::*;

/// Measures dissonant pitch pairs whose written durations overlap across two tracks.
///
/// Returns `None` for invalid or identical track indices. The analysis compares
/// every pitched note pair across both tracks in each bar and weights each
/// interval by its exact overlap duration.
pub fn frictions_between(
    doc: &Document,
    first_track: usize,
    second_track: usize,
) -> Option<FrictionAnalysis> {
    if first_track == second_track
        || first_track >= doc.tracks.len()
        || second_track >= doc.tracks.len()
        || is_percussion(doc, first_track)
        || is_percussion(doc, second_track)
    {
        return None;
    }
    let mut events = Vec::new();
    let mut total_overlap = Fraction::new(0, 1);
    let mut weighted_friction = Fraction::new(0, 1);
    let mut friction_pairs = 0;
    for bar_index in 0..doc.master_bars.len() {
        let first = track_events(doc, bar_index, first_track);
        let second = track_events(doc, bar_index, second_track);
        for left in &first {
            for right in &second {
                let Some(overlap) = overlap(left.onset, left.duration, right.onset, right.duration)
                else {
                    continue;
                };
                let interval = (left.midi - right.midi).abs().rem_euclid(12);
                let weight = interval_weight(interval);
                total_overlap = total_overlap.plus(overlap);
                if weight > 0 {
                    friction_pairs += 1;
                    weighted_friction =
                        weighted_friction.plus(scale_fraction(overlap, weight as u64));
                }
                events.push(FrictionEvent {
                    bar_index,
                    interval_semitones: interval,
                    overlap,
                    weight,
                });
            }
        }
    }
    let friction_ratio = if total_overlap == Fraction::new(0, 1) {
        Fraction::new(0, 1)
    } else {
        let scaled_total = scale_fraction(total_overlap, 4);
        fraction_ratio(weighted_friction, scaled_total)
    };
    Some(FrictionAnalysis {
        first_track,
        second_track,
        compared_pairs: events.len(),
        friction_pairs,
        total_overlap,
        weighted_friction,
        friction_ratio,
        events,
    })
}

#[derive(Clone, Copy)]
pub(super) struct TimedNote {
    pub(super) midi: i32,
    pub(super) onset: Fraction,
    pub(super) duration: Fraction,
}

pub(super) fn track_events(doc: &Document, bar_index: usize, track_index: usize) -> Vec<TimedNote> {
    let Some(bar) = bar_for(doc, bar_index, track_index) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for voice in &bar.voices {
        let mut onset = Fraction::new(0, 1);
        for beat in &voice.beats {
            let duration = crate::inspect::rhythm_duration(beat.rhythm);
            events.extend(beat.notes.iter().filter_map(|note| {
                note.midi.map(|midi| TimedNote {
                    midi,
                    onset,
                    duration,
                })
            }));
            onset = onset.plus(duration);
        }
    }
    events
}

pub(super) fn overlap(
    a_onset: Fraction,
    a_duration: Fraction,
    b_onset: Fraction,
    b_duration: Fraction,
) -> Option<Fraction> {
    let start = a_onset.max(b_onset);
    let end = a_onset.plus(a_duration).min(b_onset.plus(b_duration));
    if end <= start {
        return None;
    }
    let numerator = end.numerator * start.denominator - start.numerator * end.denominator;
    let denominator = end.denominator * start.denominator;
    Some(Fraction::new(numerator, denominator))
}

pub(super) fn scale_fraction(value: Fraction, factor: u64) -> Fraction {
    Fraction::new(value.numerator * factor, value.denominator)
}

pub(super) fn fraction_ratio(numerator: Fraction, denominator: Fraction) -> Fraction {
    Fraction::new(
        numerator.numerator * denominator.denominator,
        numerator.denominator * denominator.numerator,
    )
}

pub(super) fn interval_weight(interval: i32) -> u8 {
    match interval {
        1 | 11 => 4,
        6 => 3,
        2 | 10 => 2,
        5 | 7 => 0,
        0 | 3 | 4 | 8 | 9 => 0,
        _ => 1,
    }
}

pub(super) fn is_percussion(doc: &Document, track_index: usize) -> bool {
    doc.tracks.get(track_index).is_some_and(|track| {
        track.tuning.is_empty() || track.tuning.iter().all(|pitch| *pitch == 0)
    })
}
