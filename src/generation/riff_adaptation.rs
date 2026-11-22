//! Plans and applies explicit key, tempo and meter changes to a riff slice.
//!
//! The module owns [`RiffAdaptationRequest`], [`RiffAdaptationPlan`],
//! [`plan_riff_adaptation`] and [`apply_riff_adaptation`]. Planning reports
//! the smallest playable octave transposition without changing a score;
//! applying that report is a separate operation.

use thiserror::Error;

use crate::edits;
use crate::inspect::{bar_integrity, determine_key};
use crate::model::{Document, KeySignature, Rhythm, TempoChange};

/// Inputs for adapting an inclusive riff slice on one fretted track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiffAdaptationRequest {
    /// Track containing the riff to adapt.
    pub track: usize,
    /// First master bar, zero-based and inclusive.
    pub start_bar: usize,
    /// Last master bar, zero-based and inclusive.
    pub end_bar: usize,
    /// Desired tonic and major/minor key.
    pub target_key: KeySignature,
    /// Desired tempo at the start of the adapted slice, if it should change.
    pub target_tempo: Option<f64>,
    /// Desired meter for every bar in the slice, if it should change.
    pub target_meter: Option<(u32, u32)>,
    /// Inclusive playable fret range for the adapted fingering.
    pub fret_window: (i32, i32),
}

/// The planned key transposition, in semitones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyAdaptation {
    /// Key estimated from the selected score slice.
    pub source: KeySignature,
    /// Requested destination key.
    pub target: KeySignature,
    /// Chromatic change, including the selected octave displacement.
    pub semitones: i32,
}

/// A caller-requested tempo change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoAdaptation {
    /// Tempo effective at the beginning of the slice before adaptation.
    pub source_bpm: f64,
    /// Requested tempo effective at the beginning of the slice afterwards.
    pub target_bpm: f64,
}

/// A caller-requested meter change across the selected bars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterAdaptation {
    /// Original meter of each selected master bar, in bar order.
    pub source_meters: Vec<(u32, u32)>,
    /// Meter to write on every selected master bar.
    pub target_meter: (u32, u32),
}

/// A read-only adaptation report that can later be applied explicitly.
#[derive(Debug, Clone, PartialEq)]
pub struct RiffAdaptationPlan {
    /// Track containing the riff.
    pub track: usize,
    /// Inclusive first master bar, zero-based.
    pub start_bar: usize,
    /// Inclusive last master bar, zero-based.
    pub end_bar: usize,
    /// Range that must remain playable after transposition.
    pub fret_window: (i32, i32),
    /// Planned chromatic transposition.
    pub key: KeyAdaptation,
    /// Tempo change, absent when the request leaves tempo unchanged.
    pub tempo: Option<TempoAdaptation>,
    /// Meter change, absent when the request leaves meter unchanged.
    pub meter: Option<MeterAdaptation>,
    /// Tempo to restore immediately after the slice, if there is a following
    /// bar and the slice temporarily changes it.
    pub restore_tempo_bpm: Option<f64>,
}

/// Why an adaptation could not be planned or applied without losing score data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RiffAdaptationError {
    /// The selected bar range is empty or outside the document.
    #[error("invalid riff bar range {start_bar}..={end_bar}")]
    InvalidRange { start_bar: usize, end_bar: usize },
    /// The selected track does not exist or has no fretted tuning.
    #[error("track {0} is not a fretted track in this document")]
    InvalidTrack(usize),
    /// The selected material has no measurable key.
    #[error("the selected score slice has no key estimate")]
    NoKey,
    /// The requested tonic is outside the chromatic pitch-class range.
    #[error("target key tonic must be between 0 and 11")]
    InvalidTargetKey,
    /// The fret range is negative or reversed.
    #[error("fret window must be ordered and non-negative")]
    InvalidFretWindow,
    /// No octave of the requested transposition fits every selected note.
    #[error(
        "no transposition into the target key fits track {track} in frets {min_fret}..={max_fret}"
    )]
    NoPlayableTransposition {
        /// Track that could not be refingered.
        track: usize,
        /// Inclusive lower fret bound.
        min_fret: i32,
        /// Inclusive upper fret bound.
        max_fret: i32,
    },
    /// The target tempo is invalid.
    #[error("tempo must be finite and greater than zero")]
    InvalidTempo,
    /// No tempo is written at or before the selected slice.
    #[error("cannot adapt tempo because the source score has no tempo at the selected slice")]
    MissingTempo,
    /// Meter is not a positive numerator over a supported power-of-two denominator.
    #[error("invalid target meter {numerator}/{denominator}")]
    InvalidMeter { numerator: u32, denominator: u32 },
    /// Exact meter scaling would overflow or leave a voice incomplete.
    #[error("meter change cannot preserve the exact onsets and full bars in the selected slice")]
    MeterNotRepresentable,
    /// The document no longer matches the report used to plan the adaptation.
    #[error("the selected riff changed after this adaptation plan was created")]
    StalePlan,
    /// The underlying slice helper rejected its range.
    #[error("score slice failed: {0}")]
    Slice(String),
}

/// Inspects a score slice and plans a playable octave-equivalent transposition.
///
/// The function uses [`determine_key`] on the selected bars. It never changes
/// `document`; use [`apply_riff_adaptation`] after reviewing the returned plan.
pub fn plan_riff_adaptation(
    document: &Document,
    request: RiffAdaptationRequest,
) -> Result<RiffAdaptationPlan, RiffAdaptationError> {
    validate_selection(document, request.track, request.start_bar, request.end_bar)?;
    if request.fret_window.0 < 0 || request.fret_window.0 > request.fret_window.1 {
        return Err(RiffAdaptationError::InvalidFretWindow);
    }
    if !(0..=11).contains(&request.target_key.tonic) {
        return Err(RiffAdaptationError::InvalidTargetKey);
    }
    if request
        .target_tempo
        .is_some_and(|tempo| !tempo.is_finite() || tempo <= 0.0)
    {
        return Err(RiffAdaptationError::InvalidTempo);
    }
    if let Some((numerator, denominator)) = request.target_meter {
        if !valid_meter((numerator, denominator)) {
            return Err(RiffAdaptationError::InvalidMeter {
                numerator,
                denominator,
            });
        }
    }

    let slice = edits::slice(document, request.start_bar, request.end_bar)
        .map_err(|error| RiffAdaptationError::Slice(format!("{error:?}")))?;
    let source_key = determine_key(&slice).ok_or(RiffAdaptationError::NoKey)?.key;
    let semitones = choose_transposition(
        document,
        request.track,
        request.start_bar,
        request.end_bar,
        source_key,
        request.target_key,
        request.fret_window,
    )?;

    let tempo = request
        .target_tempo
        .map(|target_bpm| {
            let source_bpm =
                tempo_at(document, request.start_bar).ok_or(RiffAdaptationError::MissingTempo)?;
            Ok(TempoAdaptation {
                source_bpm,
                target_bpm,
            })
        })
        .transpose()?;
    let restore_tempo_bpm = if tempo.is_some() && request.end_bar + 1 < document.master_bars.len() {
        tempo_at(document, request.end_bar + 1)
    } else {
        None
    };
    let meter = request.target_meter.map(|target_meter| MeterAdaptation {
        source_meters: document.master_bars[request.start_bar..=request.end_bar]
            .iter()
            .map(|master| master.time)
            .collect(),
        target_meter,
    });
    if meter.is_some()
        && bar_integrity(document)
            .iter()
            .any(|entry| (request.start_bar..=request.end_bar).contains(&entry.bar_index))
    {
        return Err(RiffAdaptationError::MeterNotRepresentable);
    }

    Ok(RiffAdaptationPlan {
        track: request.track,
        start_bar: request.start_bar,
        end_bar: request.end_bar,
        fret_window: request.fret_window,
        key: KeyAdaptation {
            source: source_key,
            target: request.target_key,
            semitones,
        },
        tempo,
        meter,
        restore_tempo_bpm,
    })
}

/// Applies a previously inspected plan to its score slice without dropping notes.
pub fn apply_riff_adaptation(
    document: &mut Document,
    plan: &RiffAdaptationPlan,
) -> Result<(), RiffAdaptationError> {
    validate_selection(document, plan.track, plan.start_bar, plan.end_bar)?;
    if plan.fret_window.0 < 0 || plan.fret_window.0 > plan.fret_window.1 {
        return Err(RiffAdaptationError::InvalidFretWindow);
    }
    if !(0..=11).contains(&plan.key.target.tonic) {
        return Err(RiffAdaptationError::InvalidTargetKey);
    }
    let slice = edits::slice(document, plan.start_bar, plan.end_bar)
        .map_err(|error| RiffAdaptationError::Slice(format!("{error:?}")))?;
    if determine_key(&slice).map(|estimate| estimate.key) != Some(plan.key.source) {
        return Err(RiffAdaptationError::StalePlan);
    }
    if choose_transposition(
        document,
        plan.track,
        plan.start_bar,
        plan.end_bar,
        plan.key.source,
        plan.key.target,
        plan.fret_window,
    )? != plan.key.semitones
    {
        return Err(RiffAdaptationError::StalePlan);
    }
    if plan
        .tempo
        .is_some_and(|tempo| !tempo.target_bpm.is_finite() || tempo.target_bpm <= 0.0)
    {
        return Err(RiffAdaptationError::InvalidTempo);
    }
    if plan.tempo.is_some_and(|tempo| {
        !tempo.source_bpm.is_finite()
            || tempo.source_bpm <= 0.0
            || tempo_at(document, plan.start_bar) != Some(tempo.source_bpm)
    }) {
        return Err(RiffAdaptationError::StalePlan);
    }
    if let Some(meter) = &plan.meter {
        if !valid_meter(meter.target_meter)
            || meter.source_meters.len() != plan.end_bar - plan.start_bar + 1
            || document.master_bars[plan.start_bar..=plan.end_bar]
                .iter()
                .map(|master| master.time)
                .ne(meter.source_meters.iter().copied())
        {
            return Err(RiffAdaptationError::StalePlan);
        }
        if bar_integrity(document)
            .iter()
            .any(|entry| (plan.start_bar..=plan.end_bar).contains(&entry.bar_index))
        {
            return Err(RiffAdaptationError::MeterNotRepresentable);
        }
    }

    let mut output = document.clone();
    transpose_and_refinger(&mut output, plan)?;
    if let Some(meter) = &plan.meter {
        for bar_index in plan.start_bar..=plan.end_bar {
            let source_meter = output.master_bars[bar_index].time;
            let scale = capacity_ratio(source_meter, meter.target_meter)
                .ok_or(RiffAdaptationError::MeterNotRepresentable)?;
            let ids = output.master_bars[bar_index].bar_ids.clone();
            for id in ids {
                let Some(id) = u32::try_from(id).ok() else {
                    continue;
                };
                let Some(bar) = output.bars.iter_mut().find(|bar| bar.id == id) else {
                    continue;
                };
                for voice in &mut bar.voices {
                    for beat in &mut voice.beats {
                        beat.rhythm = scale_rhythm(beat.rhythm, scale)
                            .ok_or(RiffAdaptationError::MeterNotRepresentable)?;
                    }
                }
            }
            output.master_bars[bar_index].time = meter.target_meter;
        }
        if bar_integrity(&output)
            .iter()
            .any(|entry| (plan.start_bar..=plan.end_bar).contains(&entry.bar_index))
        {
            return Err(RiffAdaptationError::MeterNotRepresentable);
        }
    }
    if let Some(tempo) = plan.tempo {
        apply_tempo(&mut output, plan, tempo.target_bpm);
    }
    output.key = Some(plan.key.target);
    *document = output;
    Ok(())
}

fn validate_selection(
    document: &Document,
    track: usize,
    start_bar: usize,
    end_bar: usize,
) -> Result<(), RiffAdaptationError> {
    if start_bar > end_bar || end_bar >= document.master_bars.len() {
        return Err(RiffAdaptationError::InvalidRange { start_bar, end_bar });
    }
    let Some(source_track) = document.tracks.get(track) else {
        return Err(RiffAdaptationError::InvalidTrack(track));
    };
    if source_track.tuning.is_empty() || source_track.tuning.iter().all(|pitch| *pitch == 0) {
        return Err(RiffAdaptationError::InvalidTrack(track));
    }
    Ok(())
}

fn choose_transposition(
    document: &Document,
    track: usize,
    start_bar: usize,
    end_bar: usize,
    source_key: KeySignature,
    target_key: KeySignature,
    fret_window: (i32, i32),
) -> Result<i32, RiffAdaptationError> {
    let difference = (i32::from(target_key.tonic) - i32::from(source_key.tonic)).rem_euclid(12);
    let mut offsets = (-48i32..=48)
        .filter(|offset| (*offset).rem_euclid(12) == difference)
        .collect::<Vec<_>>();
    offsets.sort_by_key(|offset| ((*offset).abs(), *offset));
    for offset in offsets {
        if notes_fit(document, track, start_bar, end_bar, offset, fret_window) {
            return Ok(offset);
        }
    }
    Err(RiffAdaptationError::NoPlayableTransposition {
        track,
        min_fret: fret_window.0,
        max_fret: fret_window.1,
    })
}

fn notes_fit(
    document: &Document,
    track: usize,
    start_bar: usize,
    end_bar: usize,
    semitones: i32,
    fret_window: (i32, i32),
) -> bool {
    for bar_index in start_bar..=end_bar {
        let Some(id) = document.master_bars[bar_index].bar_ids.get(track) else {
            continue;
        };
        let Some(id) = u32::try_from(*id).ok() else {
            continue;
        };
        let Some(bar) = document.bars.iter().find(|bar| bar.id == id) else {
            continue;
        };
        for note in bar
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
        {
            let Some(pitch) = note.midi else { continue };
            if choose_position(
                &document.tracks[track].tuning,
                pitch + semitones,
                note.string,
                fret_window,
            )
            .is_none()
            {
                return false;
            }
        }
    }
    true
}

fn choose_position(
    tuning: &[i32],
    pitch: i32,
    preferred_string: Option<u32>,
    fret_window: (i32, i32),
) -> Option<(u32, i32)> {
    let mut positions = tuning
        .iter()
        .enumerate()
        .filter_map(|(index, open)| {
            let fret = pitch - open;
            let string = u32::try_from(index).ok()?;
            (fret_window.0..=fret_window.1)
                .contains(&fret)
                .then_some((string, fret))
        })
        .collect::<Vec<_>>();
    positions.sort_by_key(|(string, fret)| {
        (Some(*string) != preferred_string, (fret - 5).abs(), *string)
    });
    positions.into_iter().next()
}

fn transpose_and_refinger(
    document: &mut Document,
    plan: &RiffAdaptationPlan,
) -> Result<(), RiffAdaptationError> {
    let tuning = document.tracks[plan.track].tuning.clone();
    for bar_index in plan.start_bar..=plan.end_bar {
        let Some(id) = document.master_bars[bar_index]
            .bar_ids
            .get(plan.track)
            .copied()
        else {
            continue;
        };
        let Some(id) = u32::try_from(id).ok() else {
            continue;
        };
        let Some(bar) = document.bars.iter_mut().find(|bar| bar.id == id) else {
            continue;
        };
        for note in bar
            .voices
            .iter_mut()
            .flat_map(|voice| &mut voice.beats)
            .flat_map(|beat| &mut beat.notes)
        {
            let Some(pitch) = note.midi else { continue };
            let new_pitch = pitch + plan.key.semitones;
            let Some((string, fret)) =
                choose_position(&tuning, new_pitch, note.string, plan.fret_window)
            else {
                return Err(RiffAdaptationError::NoPlayableTransposition {
                    track: plan.track,
                    min_fret: plan.fret_window.0,
                    max_fret: plan.fret_window.1,
                });
            };
            note.midi = Some(new_pitch);
            note.string = Some(string);
            note.fret = Some(fret);
        }
    }
    Ok(())
}

fn valid_meter(meter: (u32, u32)) -> bool {
    meter.0 > 0 && (1..=64).contains(&meter.0) && matches!(meter.1, 1 | 2 | 4 | 8 | 16 | 32 | 64)
}

fn capacity_ratio(source: (u32, u32), target: (u32, u32)) -> Option<(u64, u64)> {
    if !valid_meter(source) || !valid_meter(target) {
        return None;
    }
    let numerator = u64::from(target.0).checked_mul(u64::from(source.1))?;
    let denominator = u64::from(target.1).checked_mul(u64::from(source.0))?;
    let divisor = gcd(numerator, denominator);
    Some((numerator / divisor, denominator / divisor))
}

fn scale_rhythm(rhythm: Rhythm, (scale_num, scale_den): (u64, u64)) -> Option<Rhythm> {
    let (old_num, old_den) = rhythm
        .tuplet
        .map_or((1u64, 1u64), |(num, den)| (u64::from(den), u64::from(num)));
    let numerator = old_num.checked_mul(scale_num)?;
    let denominator = old_den.checked_mul(scale_den)?;
    let divisor = gcd(numerator, denominator);
    let numerator = u32::try_from(numerator / divisor).ok()?;
    let denominator = u32::try_from(denominator / divisor).ok()?;
    Some(Rhythm {
        tuplet: if numerator == 1 && denominator == 1 {
            None
        } else {
            Some((denominator, numerator))
        },
        ..rhythm
    })
}

fn tempo_at(document: &Document, bar: usize) -> Option<f64> {
    document
        .tempo_map
        .iter()
        .filter(|change| change.at_bar <= bar)
        .max_by_key(|change| change.at_bar)
        .map(|change| change.bpm)
}

fn apply_tempo(document: &mut Document, plan: &RiffAdaptationPlan, target_bpm: f64) {
    document
        .tempo_map
        .retain(|change| change.at_bar < plan.start_bar || change.at_bar > plan.end_bar);
    document.tempo_map.push(TempoChange {
        at_bar: plan.start_bar,
        bpm: target_bpm,
        linear: false,
    });
    if let Some(restore_bpm) = plan.restore_tempo_bpm {
        let restore_bar = plan.end_bar + 1;
        if !document
            .tempo_map
            .iter()
            .any(|change| change.at_bar == restore_bar)
        {
            document.tempo_map.push(TempoChange {
                at_bar: restore_bar,
                bpm: restore_bpm,
                linear: false,
            });
        }
    }
    document.tempo_map.sort_by_key(|change| change.at_bar);
}

fn gcd(left: u64, right: u64) -> u64 {
    if right == 0 {
        left.max(1)
    } else {
        gcd(right, left % right)
    }
}

#[cfg(test)]
#[path = "riff_adaptation_test.rs"]
mod tests;
