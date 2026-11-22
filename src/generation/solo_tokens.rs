//! Token-driven solo composition and constrained guitar fingering.

use crate::edits;
use crate::generation::assessment::{self, AssessmentReport};
use crate::generation::harmony::fretboard_positions;
use crate::generation::requirements::{self, Requirement};
use crate::inspect::{self, Fraction};
use crate::model::{Bar, Beat, Document, KeySignature, Note, Rhythm, Technique, Track, Voice};

/// Inputs for composing a token stream over selected score bars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSoloRequest {
    /// Inclusive first bar to fill.
    pub start_bar: usize,
    /// Inclusive last bar to fill.
    pub end_bar: usize,
    /// Tonic used to resolve scale-degree tokens.
    pub key: KeySignature,
    /// Pitch-class intervals from the tonic.
    pub scale: Vec<u8>,
    /// Inclusive MIDI register for scale-degree tokens.
    pub register: (i32, i32),
    /// Open-string MIDI pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// Inclusive playable fret range.
    pub fret_window: (i32, i32),
    /// Number of deterministic fingering candidates to return.
    pub candidate_count: usize,
    /// Seed used only to break equal-cost fingering paths.
    pub seed: u64,
    /// Display name for the appended track.
    pub track_name: String,
}

/// A generated and assessed solo candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct SoloTokenCandidate {
    /// Seed variant used to choose among equal-cost fingerings.
    pub seed: u64,
    /// Index of the appended track.
    pub track_index: usize,
    /// Number of pitched events.
    pub note_count: usize,
    /// Full measurement report for caller-owned ranking.
    pub assessment: AssessmentReport,
    /// Candidate score.
    pub document: Document,
}

/// Token parsing, duration, or fingering failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenSoloError {
    /// A token was malformed or outside the grammar.
    #[error("invalid solo token {token:?} at position {index}")]
    InvalidToken {
        /// Zero-based position of the token in the input.
        index: usize,
        /// The offending token text.
        token: String,
    },
    /// A technique token appeared before any pitch event.
    #[error("technique token {token:?} has no preceding pitch")]
    TechniqueWithoutPitch {
        /// The technique token text.
        token: String,
    },
    /// Hammer-on was not followed by a pitch event.
    #[error("hammer-on must be followed by another pitched event")]
    IncompleteHammerOn,
    /// Request fields or selected score bars are invalid.
    #[error("invalid token-solo request")]
    InvalidRequest,
    /// Tokens do not fill every selected bar exactly.
    #[error("token durations do not fill selected bar {bar} exactly")]
    BarDurationMismatch {
        /// Zero-based index of the bar that is not filled.
        bar: usize,
    },
    /// Exact duration accumulation exceeded the supported rational range.
    #[error("token duration exceeds the exact timing range")]
    DurationOverflow,
    /// No fingering path satisfies the window and movement constraints.
    #[error("no fingering solution for event {event} at pitch {pitch}")]
    NoFingeringSolution {
        /// Zero-based index of the pitch event.
        event: usize,
        /// MIDI pitch of the event.
        pitch: i32,
    },
    /// Solver output failed a guaranteed hard requirement.
    #[error("solver output violates {requirement}")]
    SolverInvariant {
        /// Name of the violated requirement.
        requirement: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PitchToken {
    Midi(i32),
    Degree(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenEvent {
    pitch: Option<PitchToken>,
    rhythm: Rhythm,
    techniques: Vec<Technique>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position {
    midi: i32,
    string: u32,
    fret: i32,
}

/// Composes and fingers seeded variants of the token stream.
///
/// Tokens are whitespace- or comma-separated: pitch=60, degree=3,
/// duration=eighth, rest, bend=50, slide, vibrato, and hammer-on. A duration
/// applies to the next pitch or rest; otherwise events use a quarter note.
/// Every selected bar must be filled exactly. Results stay in seed order and
/// are never ranked internally.
pub fn compose_solo_tokens(
    document: &Document,
    request: &TokenSoloRequest,
    tokens: &str,
) -> Result<Vec<SoloTokenCandidate>, TokenSoloError> {
    validate_request(document, request)?;
    let events = parse_tokens(tokens)?;
    for (index, event) in events.iter().enumerate() {
        if event.pitch.is_some() && resolve_pitch(event, request).is_none() {
            return Err(invalid(index, "degree outside scale or register"));
        }
    }
    let bars = place_events(document, request, events)?;
    let mut candidates = Vec::with_capacity(request.candidate_count);
    for index in 0..request.candidate_count {
        let seed = request
            .seed
            .wrapping_add(u64::try_from(index).unwrap_or(u64::MAX));
        let mut output = document.clone();
        let (generated, note_count) = finger_bars(&output, request, &bars, seed)?;
        let track_index = append_track(&mut output, request, generated)?;
        for requirement in [
            Requirement::AdjacentStrings,
            Requirement::HandPositionLeap,
            Requirement::StringMotionRate,
        ] {
            if requirements::check(&output).iter().any(|violation| {
                violation.requirement == requirement && violation.region.track == Some(track_index)
            }) {
                return Err(TokenSoloError::SolverInvariant {
                    requirement: requirement.id(),
                });
            }
        }
        candidates.push(SoloTokenCandidate {
            seed,
            track_index,
            note_count,
            assessment: assessment::assess(&output),
            document: output,
        });
    }
    Ok(candidates)
}

/// Ranks candidates through a caller-owned score. Equal scores preserve seed
/// order; this crate does not define a default musical preference.
pub fn rank_solo_candidates(
    mut candidates: Vec<SoloTokenCandidate>,
    score: impl Fn(&AssessmentReport) -> i64,
) -> Vec<SoloTokenCandidate> {
    candidates.sort_by_key(|candidate| std::cmp::Reverse(score(&candidate.assessment)));
    candidates
}

fn validate_request(doc: &Document, req: &TokenSoloRequest) -> Result<(), TokenSoloError> {
    if doc.master_bars.is_empty()
        || req.start_bar > req.end_bar
        || req.end_bar >= doc.master_bars.len()
        || !(0..12).contains(&req.key.tonic)
        || req.scale.is_empty()
        || req.scale.iter().any(|step| *step > 11)
        || req
            .scale
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != req.scale.len()
        || req.register.0 < 0
        || req.register.0 > req.register.1
        || req.register.1 > 127
        || req.tuning.is_empty()
        || req.tuning.iter().any(|pitch| !(0..=127).contains(pitch))
        || req.fret_window.0 < 0
        || req.fret_window.0 > req.fret_window.1
        || req.fret_window.1 > 36
        || !(1..=64).contains(&req.candidate_count)
    {
        return Err(TokenSoloError::InvalidRequest);
    }
    Ok(())
}

fn parse_tokens(source: &str) -> Result<Vec<TokenEvent>, TokenSoloError> {
    let tokens = source
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut events = Vec::<TokenEvent>::new();
    let mut next_rhythm = Rhythm::new(crate::model::NoteValue::Quarter);
    let mut hammer_destination = false;
    for (index, token) in tokens.iter().enumerate() {
        if let Some(value) = token.strip_prefix("duration=") {
            next_rhythm = parse_duration(value).ok_or_else(|| invalid(index, token))?;
            continue;
        }
        if *token == "rest" {
            events.push(TokenEvent {
                pitch: None,
                rhythm: next_rhythm,
                techniques: Vec::new(),
            });
            next_rhythm = Rhythm::new(crate::model::NoteValue::Quarter);
            continue;
        }
        let pitch = if let Some(value) = token.strip_prefix("pitch=") {
            let midi = value
                .parse::<i32>()
                .ok()
                .filter(|midi| (0..=127).contains(midi));
            Some(PitchToken::Midi(midi.ok_or_else(|| invalid(index, token))?))
        } else if let Some(value) = token.strip_prefix("degree=") {
            let degree = value.parse::<u8>().ok().filter(|degree| *degree > 0);
            Some(PitchToken::Degree(
                degree.ok_or_else(|| invalid(index, token))?,
            ))
        } else {
            None
        };
        if let Some(pitch) = pitch {
            let mut techniques = Vec::new();
            if hammer_destination {
                techniques.push(Technique::HammerDestination);
                hammer_destination = false;
            }
            events.push(TokenEvent {
                pitch: Some(pitch),
                rhythm: next_rhythm,
                techniques,
            });
            next_rhythm = Rhythm::new(crate::model::NoteValue::Quarter);
            continue;
        }
        if !matches!(*token, "vibrato" | "slide" | "hammer-on") && !token.starts_with("bend=") {
            return Err(invalid(index, token));
        }
        let Some(last) = events.last_mut().filter(|event| event.pitch.is_some()) else {
            return Err(TokenSoloError::TechniqueWithoutPitch {
                token: (*token).into(),
            });
        };
        match *token {
            "vibrato" => last.techniques.push(Technique::Vibrato),
            "slide" => last.techniques.push(Technique::Slide { flags: 1 }),
            "hammer-on" => {
                if hammer_destination {
                    return Err(TokenSoloError::IncompleteHammerOn);
                }
                last.techniques.push(Technique::HammerOrigin);
                hammer_destination = true;
            }
            value if value.starts_with("bend=") => {
                let amount = value[5..].parse::<i32>().ok();
                let Some(amount) = amount.filter(|amount| matches!(amount, 50 | 100)) else {
                    return Err(invalid(index, value));
                };
                last.techniques.push(Technique::Bend {
                    origin: 0,
                    middle: amount,
                    dest: amount,
                });
            }
            _ => return Err(invalid(index, token)),
        }
    }
    if hammer_destination {
        return Err(TokenSoloError::IncompleteHammerOn);
    }
    if events.is_empty() {
        return Err(TokenSoloError::InvalidRequest);
    }
    Ok(events)
}

fn invalid(index: usize, token: &str) -> TokenSoloError {
    TokenSoloError::InvalidToken {
        index,
        token: token.into(),
    }
}

fn parse_duration(value: &str) -> Option<Rhythm> {
    use crate::model::NoteValue as N;
    let (name, dots) = if let Some(name) = value.strip_prefix("double-dotted-") {
        (name, 2)
    } else if let Some(name) = value.strip_prefix("dotted-") {
        (name, 1)
    } else {
        (value, 0)
    };
    let value = match name {
        "whole" => N::Whole,
        "half" => N::Half,
        "quarter" => N::Quarter,
        "eighth" => N::Eighth,
        "sixteenth" => N::Sixteenth,
        "thirty-second" => N::ThirtySecond,
        _ => return None,
    };
    Some(Rhythm {
        value,
        dots,
        tuplet: None,
    })
}

fn place_events(
    doc: &Document,
    req: &TokenSoloRequest,
    events: Vec<TokenEvent>,
) -> Result<Vec<Vec<TokenEvent>>, TokenSoloError> {
    let mut bars = vec![Vec::new(); req.end_bar - req.start_bar + 1];
    let mut bar = 0;
    let mut elapsed = Fraction::new(0, 1);
    for event in events {
        if bar >= bars.len() {
            return Err(TokenSoloError::BarDurationMismatch { bar: req.end_bar });
        }
        let capacity = inspect::bar_capacity(doc.master_bars[req.start_bar + bar].time);
        let next = add_fraction(elapsed, inspect::rhythm_duration(event.rhythm))
            .ok_or(TokenSoloError::DurationOverflow)?;
        if next > capacity {
            return Err(TokenSoloError::BarDurationMismatch {
                bar: req.start_bar + bar,
            });
        }
        bars[bar].push(event);
        elapsed = next;
        if elapsed == capacity {
            bar += 1;
            elapsed = Fraction::new(0, 1);
        }
    }
    if bar != bars.len() || bars.iter().flatten().all(|event| event.pitch.is_none()) {
        return Err(TokenSoloError::BarDurationMismatch {
            bar: req.start_bar + bar.min(bars.len().saturating_sub(1)),
        });
    }
    Ok(bars)
}

fn add_fraction(a: Fraction, b: Fraction) -> Option<Fraction> {
    let numerator = u128::from(a.numerator)
        .checked_mul(u128::from(b.denominator))?
        .checked_add(u128::from(b.numerator).checked_mul(u128::from(a.denominator))?)?;
    let denominator = u128::from(a.denominator).checked_mul(u128::from(b.denominator))?;
    let divisor = gcd(numerator, denominator);
    Some(Fraction::new(
        u64::try_from(numerator / divisor).ok()?,
        u64::try_from(denominator / divisor).ok()?,
    ))
}

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

#[derive(Debug, Clone)]
struct Placed {
    bar: usize,
    event: TokenEvent,
    onset: f64,
}

fn finger_bars(
    doc: &Document,
    req: &TokenSoloRequest,
    bars: &[Vec<TokenEvent>],
    seed: u64,
) -> Result<(Vec<(usize, Bar)>, usize), TokenSoloError> {
    let mut placed = Vec::new();
    let mut total_quarters = 0.0;
    for (offset, events) in bars.iter().enumerate() {
        let bar = req.start_bar + offset;
        let mut elapsed = Fraction::new(0, 1);
        for event in events {
            placed.push(Placed {
                bar,
                event: event.clone(),
                onset: total_quarters + quarters(elapsed),
            });
            elapsed = add_fraction(elapsed, inspect::rhythm_duration(event.rhythm))
                .ok_or(TokenSoloError::DurationOverflow)?;
        }
        total_quarters += quarters(inspect::bar_capacity(doc.master_bars[bar].time));
    }
    let pitched = placed
        .iter()
        .filter_map(|event| resolve_pitch(&event.event, req).map(|pitch| (pitch, event)))
        .collect::<Vec<_>>();
    let path = solve(doc, req, &pitched, seed)?;
    let mut positions = path.into_iter();
    let mut generated = Vec::new();
    let mut notes_written = 0;
    for (offset, events) in bars.iter().enumerate() {
        let mut beats = Vec::with_capacity(events.len());
        for event in events {
            let notes = if let Some(pitch) = resolve_pitch(event, req) {
                let pos = positions
                    .next()
                    .ok_or(TokenSoloError::NoFingeringSolution {
                        event: notes_written,
                        pitch,
                    })?;
                notes_written += 1;
                vec![Note {
                    id: 0,
                    midi: Some(pitch),
                    string: Some(pos.string),
                    fret: Some(pos.fret),
                    articulation: None,
                    techniques: event.techniques.clone(),
                }]
            } else {
                Vec::new()
            };
            beats.push(Beat {
                id: 0,
                rhythm: event.rhythm,
                notes,
                dynamic: None,
            });
        }
        generated.push((
            req.start_bar + offset,
            Bar {
                id: 0,
                clef: Some("G2".into()),
                voices: vec![Voice { id: 0, beats }],
            },
        ));
    }
    Ok((generated, notes_written))
}

fn resolve_pitch(event: &TokenEvent, req: &TokenSoloRequest) -> Option<i32> {
    match event.pitch? {
        PitchToken::Midi(pitch) => Some(pitch),
        PitchToken::Degree(degree) => {
            let interval = *req.scale.get(usize::from(degree.checked_sub(1)?))?;
            let pc = (i32::from(req.key.tonic) + i32::from(interval)).rem_euclid(12);
            (req.register.0..=req.register.1)
                .filter(|pitch| pitch.rem_euclid(12) == pc)
                .min_by_key(|pitch| (*pitch - (req.register.0 + req.register.1) / 2).abs())
        }
    }
}

fn solve(
    doc: &Document,
    req: &TokenSoloRequest,
    events: &[(i32, &Placed)],
    seed: u64,
) -> Result<Vec<Position>, TokenSoloError> {
    let options = events
        .iter()
        .map(|(pitch, event)| {
            fretboard_positions(*pitch, &req.tuning, req.fret_window.1)
                .into_iter()
                .filter(|pos| (req.fret_window.0..=req.fret_window.1).contains(&pos.fret))
                .map(|pos| {
                    (
                        Position {
                            midi: pos.midi,
                            string: pos.string,
                            fret: pos.fret,
                        },
                        event.onset,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for (index, ((pitch, _), choices)) in events.iter().zip(&options).enumerate() {
        if choices.is_empty() {
            return Err(TokenSoloError::NoFingeringSolution {
                event: index,
                pitch: *pitch,
            });
        }
    }
    let Some(first) = options.first() else {
        return Err(TokenSoloError::InvalidRequest);
    };
    let mut costs = first
        .iter()
        .map(|(pos, _)| i64::from(pos.fret.abs()) + tie_cost(seed, 0, *pos))
        .collect::<Vec<_>>();
    let mut parents = vec![vec![None; first.len()]];
    for layer in 1..options.len() {
        let mut next_costs = vec![i64::MAX; options[layer].len()];
        let mut prev = vec![None; options[layer].len()];
        let tempo = tempo_at(doc, events[layer].1.bar);
        let elapsed = (options[layer][0].1 - options[layer - 1][0].1).max(0.0001);
        let seconds = elapsed * 60.0 / tempo;
        for (to_index, (to, _)) in options[layer].iter().enumerate() {
            for (from_index, (from, _)) in options[layer - 1].iter().enumerate() {
                let fret_shift = (to.fret - from.fret).abs();
                let string_shift = from.string.abs_diff(to.string);
                if fret_shift >= 7 || f64::from(string_shift) / seconds > 12.0 {
                    continue;
                }
                let cost = costs[from_index]
                    .saturating_add(i64::from(fret_shift) * 3)
                    .saturating_add(i64::from(string_shift) * 40)
                    .saturating_add(tie_cost(seed, layer, *to));
                if cost < next_costs[to_index] {
                    next_costs[to_index] = cost;
                    prev[to_index] = Some(from_index);
                }
            }
        }
        if next_costs.iter().all(|cost| *cost == i64::MAX) {
            return Err(TokenSoloError::NoFingeringSolution {
                event: layer,
                pitch: events[layer].0,
            });
        }
        costs = next_costs;
        parents.push(prev);
    }
    let mut index = costs
        .iter()
        .enumerate()
        .min_by_key(|(_, cost)| *cost)
        .map(|(index, _)| index)
        .ok_or(TokenSoloError::InvalidRequest)?;
    let mut path = Vec::with_capacity(options.len());
    for layer in (0..options.len()).rev() {
        path.push(options[layer][index].0);
        if layer > 0 {
            index = parents[layer][index].ok_or(TokenSoloError::InvalidRequest)?;
        }
    }
    path.reverse();
    Ok(path)
}

fn tie_cost(seed: u64, event: usize, pos: Position) -> i64 {
    let value = seed
        ^ u64::try_from(event)
            .unwrap_or(u64::MAX)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(pos.string).rotate_left(17)
        ^ u64::from(pos.midi.unsigned_abs());
    i64::try_from(value % 7).unwrap_or(0)
}

fn tempo_at(doc: &Document, bar: usize) -> f64 {
    doc.tempo_map
        .iter()
        .filter(|change| change.at_bar <= bar)
        .max_by_key(|change| change.at_bar)
        .map(|change| change.bpm)
        .filter(|bpm| bpm.is_finite() && *bpm > 0.0)
        .unwrap_or(120.0)
}

fn quarters(fraction: Fraction) -> f64 {
    fraction.numerator as f64 * 4.0 / fraction.denominator as f64
}

fn append_track(
    doc: &mut Document,
    req: &TokenSoloRequest,
    mut bars: Vec<(usize, Bar)>,
) -> Result<usize, TokenSoloError> {
    let track = edits::create_track(
        doc,
        Track {
            id: 0,
            name: req.track_name.clone(),
            color: None,
            tuning: req.tuning.clone(),
            midi_program: Some(30),
            midi_channel: Some(0),
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
            generator_metadata: None,
        },
    );
    let mut bar_id = next_id(doc.bars.iter().map(|item| item.id))?;
    let mut voice_id = next_id(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .map(|item| item.id),
    )?;
    let mut beat_id = next_id(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|v| &v.beats)
            .map(|item| item.id),
    )?;
    let mut note_id = next_id(
        doc.bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|v| &v.beats)
            .flat_map(|b| &b.notes)
            .map(|item| item.id),
    )?;
    for (index, mut bar) in bars.drain(..) {
        bar.id = bar_id;
        bar_id = bar_id
            .checked_add(1)
            .ok_or(TokenSoloError::InvalidRequest)?;
        for voice in &mut bar.voices {
            voice.id = voice_id;
            voice_id = voice_id
                .checked_add(1)
                .ok_or(TokenSoloError::InvalidRequest)?;
            for beat in &mut voice.beats {
                beat.id = beat_id;
                beat_id = beat_id
                    .checked_add(1)
                    .ok_or(TokenSoloError::InvalidRequest)?;
                for note in &mut beat.notes {
                    note.id = note_id;
                    note_id = note_id
                        .checked_add(1)
                        .ok_or(TokenSoloError::InvalidRequest)?;
                }
            }
        }
        doc.master_bars[index].bar_ids[track] =
            i32::try_from(bar.id).map_err(|_| TokenSoloError::InvalidRequest)?;
        doc.bars.push(bar);
    }
    Ok(track)
}

fn next_id(values: impl Iterator<Item = u32>) -> Result<u32, TokenSoloError> {
    values
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(TokenSoloError::InvalidRequest)
}

#[cfg(test)]
#[path = "solo_tokens_test.rs"]
mod tests;
