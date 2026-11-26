//! Explicit-role generation of a complementary second guitar.

use crate::edits;
use crate::generation::requirements::{self, Requirement};
use crate::inspect::{self, MusicalMode, TrackKind};
use crate::model::{Bar, Document, KeySignature, Note};

/// Musical role for a generated second guitar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuitarRole {
    /// Thin the source to leave space for the other instruments.
    Support,
    /// Double the source rhythm with octave variation to avoid a verbatim clone.
    Double,
    /// Move pitches to a diatonic third while retaining the source rhythm.
    Harmonise,
    /// Move pitches by an octave while retaining the source rhythm.
    Octave,
    /// Choose support, double, or harmonise from source density per section.
    Contextual,
    /// Double in unison, then harmonise only an eligible phrase tail.
    PhraseTail,
}

/// Diatonic interval used by a phrase-tail second guitar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhraseTailInterval {
    /// The next diatonic third.
    Third,
    /// The next diatonic sixth.
    Sixth,
    /// The octave above the source.
    Octave,
}

/// Settings for generating the second guitar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecondGuitarConfig {
    /// Pitched source track to complement.
    pub source_track: usize,
    /// Explicit role or density-driven contextual mode.
    pub role: GuitarRole,
    /// Optional key used for diatonic harmonising.
    pub key: Option<KeySignature>,
    /// Open-string MIDI pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// Inclusive playable fret range.
    pub fret_window: (i32, i32),
    /// Seed for deterministic octave choices in the double role.
    pub seed: u64,
    /// Name for the generated track.
    pub track_name: String,
    /// Minimum source string number eligible for phrase-tail harmonising.
    pub phrase_tail_min_string: u32,
    /// Minimum source pitch eligible for phrase-tail harmonising.
    pub phrase_tail_min_pitch: i32,
    /// Number of final written beats in each bar treated as the phrase tail.
    pub phrase_tail_beats: usize,
    /// Interval used for eligible phrase-tail notes.
    pub phrase_tail_interval: PhraseTailInterval,
}

impl SecondGuitarConfig {
    /// Creates a contextual second guitar in standard tuning.
    pub fn contextual(source_track: usize, seed: u64) -> Self {
        Self {
            source_track,
            role: GuitarRole::Contextual,
            key: None,
            tuning: vec![40, 45, 50, 55, 59, 64],
            fret_window: (0, 24),
            seed,
            track_name: "Guitar 2".into(),
            phrase_tail_min_string: 1,
            phrase_tail_min_pitch: 60,
            phrase_tail_beats: 2,
            phrase_tail_interval: PhraseTailInterval::Third,
        }
    }
}

/// A contextual role selected for a bar range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionRole {
    /// First bar, zero-based and inclusive.
    pub start_bar: usize,
    /// Last bar, zero-based and inclusive.
    pub end_bar: usize,
    /// Role selected for this section.
    pub role: GuitarRole,
}

/// Details about the appended second-guitar track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedSecondGuitar {
    /// Index of the appended track.
    pub track_index: usize,
    /// Number of generated sounding notes.
    pub note_count: usize,
    /// Role assignment for each source section.
    pub sections: Vec<SectionRole>,
}

/// Why second-guitar generation could not complete.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecondGuitarError {
    /// Source track is missing or is not a sounding fretted instrument.
    #[error("track {0} is not a fretted pitched source")]
    InvalidSource(usize),
    /// The document already reaches the fretted-track limit H6.
    #[error("adding a second guitar would exceed H6: {count} fretted tracks already exist")]
    TrackLimit {
        /// Number of fretted tracks already in the document.
        count: usize,
    },
    /// Tuning, fret range, or selected key is invalid.
    #[error("invalid second-guitar tuning, fret window, or key")]
    InvalidConfig,
    /// The source has no pitched notes to transform.
    #[error("source track {0} contains no pitched notes")]
    EmptySource(usize),
    /// A requested pitch cannot be fingered in the output window.
    #[error("role {role:?} cannot place pitch {pitch} in the configured fret window")]
    Unplayable {
        /// The guitar role being generated.
        role: GuitarRole,
        /// MIDI pitch that cannot be placed.
        pitch: i32,
    },
    /// A source bar reference is invalid.
    #[error("source or generated bar is missing at bar {0}")]
    MissingBar(usize),
    /// Score editing failed.
    #[error("track clone failed: {0}")]
    Edit(String),
    /// Generated output still violates H7.
    #[error("generated second guitar remains a verbatim clone of its source")]
    Verbatim,
}

/// Adds a second-guitar track with the requested role.
///
/// H6 is checked before the document is changed. Setting force permits the
/// requested extra track while preserving every authored track.
///
/// # Errors
///
/// Returns [`SecondGuitarError::InvalidSource`] if the source track is not a fretted pitched
/// track and [`SecondGuitarError::InvalidConfig`] if the tuning, fret window or key is invalid.
/// Returns [`SecondGuitarError::TrackLimit`] if two or more fretted tracks exist and `force` is
/// not set, [`SecondGuitarError::EmptySource`] if the source has no notes, and
/// [`SecondGuitarError::MissingBar`] if a bar reference is missing. Returns
/// [`SecondGuitarError::Edit`], [`SecondGuitarError::Unplayable`] or
/// [`SecondGuitarError::Verbatim`] if the track cannot be cloned, a pitch cannot be fingered in
/// the window, or the result still duplicates a track. `document` is unchanged on error.
pub fn generate_second_guitar(
    document: &mut Document,
    config: &SecondGuitarConfig,
    force: bool,
) -> Result<GeneratedSecondGuitar, SecondGuitarError> {
    validate(document, config)?;
    let fretted = fretted_track_count(document);
    if fretted >= 2 && !force {
        return Err(SecondGuitarError::TrackLimit { count: fretted });
    }
    let source_bars = source_bars(document, config.source_track)?;
    if source_bars.iter().all(|bar| note_count(bar) == 0) {
        return Err(SecondGuitarError::EmptySource(config.source_track));
    }
    let roles = if config.role == GuitarRole::Contextual {
        contextual_roles(document, &source_bars)
    } else {
        section_ranges(document)
            .into_iter()
            .map(|(start_bar, end_bar)| SectionRole {
                start_bar,
                end_bar,
                role: config.role,
            })
            .collect()
    };
    let mut effective_config = config.clone();
    effective_config.key = config
        .key
        .or(document.key)
        .or_else(|| inspect::determine_key(document).map(|estimate| estimate.key));
    let mut output = document.clone();
    let track_index = edits::clone_track(&mut output, config.source_track)
        .map_err(|error| SecondGuitarError::Edit(format!("{error:?}")))?;
    output.tracks[track_index].name = unique_name(&output, &config.track_name, track_index);
    output.tracks[track_index].tuning = config.tuning.clone();

    for bar_index in 0..source_bars.len() {
        let role = role_at(&roles, bar_index);
        let target_id = output.master_bars[bar_index].bar_ids[track_index];
        let source_id = output.master_bars[bar_index].bar_ids[config.source_track];
        let Some(target_id) = u32::try_from(target_id).ok() else {
            return Err(SecondGuitarError::MissingBar(bar_index));
        };
        let Some(source_id) = u32::try_from(source_id).ok() else {
            return Err(SecondGuitarError::MissingBar(bar_index));
        };
        let source = output
            .bars
            .iter()
            .find(|bar| bar.id == source_id)
            .cloned()
            .ok_or(SecondGuitarError::MissingBar(bar_index))?;
        let target = output
            .bars
            .iter_mut()
            .find(|bar| bar.id == target_id)
            .ok_or(SecondGuitarError::MissingBar(bar_index))?;
        apply_role(target, &source, role, &effective_config)?;
        if role != GuitarRole::PhraseTail {
            ensure_bar_differs(target, &source, role, &effective_config)?;
        }
    }
    differentiate_from_existing(&mut output, track_index, &effective_config)?;

    let new_violations = requirements::check(&output)
        .into_iter()
        .filter(|violation| {
            violation.requirement == Requirement::TrackDuplication
                && violation.region.track == Some(track_index)
        })
        .collect::<Vec<_>>();
    if !new_violations.is_empty() {
        return Err(SecondGuitarError::Verbatim);
    }
    let generated_note_count = output
        .master_bars
        .iter()
        .filter_map(|bar| bar.bar_ids.get(track_index).copied())
        .filter_map(|id| u32::try_from(id).ok())
        .filter_map(|id| output.bars.iter().find(|bar| bar.id == id))
        .map(note_count)
        .sum();
    *document = output;
    Ok(GeneratedSecondGuitar {
        track_index,
        note_count: generated_note_count,
        sections: roles,
    })
}

fn differentiate_from_existing(
    document: &mut Document,
    generated_track: usize,
    config: &SecondGuitarConfig,
) -> Result<(), SecondGuitarError> {
    for existing_track in 0..generated_track {
        for bar_index in 0..document.master_bars.len() {
            let new_id = document.master_bars[bar_index].bar_ids[generated_track];
            let old_id = document.master_bars[bar_index].bar_ids[existing_track];
            let (Ok(new_id), Ok(old_id)) = (u32::try_from(new_id), u32::try_from(old_id)) else {
                continue;
            };
            let Some(old_bar) = document.bars.iter().find(|bar| bar.id == old_id).cloned() else {
                continue;
            };
            let Some(new_bar) = document.bars.iter_mut().find(|bar| bar.id == new_id) else {
                continue;
            };
            if bar_signature(new_bar) != bar_signature(&old_bar) {
                continue;
            }
            let Some(note) = new_bar
                .voices
                .iter_mut()
                .flat_map(|voice| &mut voice.beats)
                .flat_map(|beat| &mut beat.notes)
                .find(|note| note.midi.is_some())
            else {
                continue;
            };
            let old_pitch = note.midi.unwrap_or(0);
            let changed = [12, -12, 7, -7]
                .into_iter()
                .find_map(|shift| {
                    let pitch = old_pitch + shift;
                    (pitch != old_pitch
                        && position(&config.tuning, pitch, config.fret_window).is_some())
                    .then_some(pitch)
                })
                .ok_or(SecondGuitarError::Unplayable {
                    role: config.role,
                    pitch: old_pitch,
                })?;
            refinger(note, changed, config, config.role)?;
        }
    }
    Ok(())
}

fn validate(doc: &Document, config: &SecondGuitarConfig) -> Result<(), SecondGuitarError> {
    if config.source_track >= doc.tracks.len()
        || !matches!(
            inspect::classify_track(doc, config.source_track).map(|(kind, _)| kind),
            Some(TrackKind::Other)
        )
    {
        return Err(SecondGuitarError::InvalidSource(config.source_track));
    }
    if config.tuning.is_empty()
        || config.tuning.iter().any(|pitch| !(0..=127).contains(pitch))
        || config.fret_window.0 < 0
        || config.fret_window.0 > config.fret_window.1
        || config.fret_window.1 > 36
        || config.key.is_some_and(|key| !(0..=11).contains(&key.tonic))
    {
        return Err(SecondGuitarError::InvalidConfig);
    }
    Ok(())
}

fn fretted_track_count(doc: &Document) -> usize {
    (0..doc.tracks.len())
        .filter(|track| {
            matches!(
                inspect::classify_track(doc, *track).map(|(kind, _)| kind),
                Some(TrackKind::Other)
            )
        })
        .count()
}

fn source_bars(doc: &Document, track: usize) -> Result<Vec<Bar>, SecondGuitarError> {
    doc.master_bars
        .iter()
        .enumerate()
        .map(|(bar_index, master)| {
            let id = master
                .bar_ids
                .get(track)
                .copied()
                .and_then(|id| u32::try_from(id).ok())
                .ok_or(SecondGuitarError::MissingBar(bar_index))?;
            doc.bars
                .iter()
                .find(|bar| bar.id == id)
                .cloned()
                .ok_or(SecondGuitarError::MissingBar(bar_index))
        })
        .collect()
}

fn section_ranges(doc: &Document) -> Vec<(usize, usize)> {
    if doc.master_bars.is_empty() {
        return Vec::new();
    }
    let mut starts = vec![0];
    for (index, master) in doc.master_bars.iter().enumerate().skip(1) {
        if master.section.is_some() {
            starts.push(index);
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            (
                *start,
                starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or(doc.master_bars.len())
                    - 1,
            )
        })
        .collect()
}

fn contextual_roles(doc: &Document, bars: &[Bar]) -> Vec<SectionRole> {
    section_ranges(doc)
        .into_iter()
        .map(|(start, end)| {
            let (notes, beats) = bars[start..=end].iter().fold((0usize, 0usize), |sum, bar| {
                (
                    sum.0 + note_count(bar),
                    sum.1 + bar.voices.iter().flat_map(|voice| &voice.beats).count(),
                )
            });
            let density = notes as f64 / beats.max(1) as f64;
            let role = if density >= 1.5 {
                GuitarRole::Support
            } else if density < 0.75 {
                GuitarRole::Harmonise
            } else {
                GuitarRole::Double
            };
            SectionRole {
                start_bar: start,
                end_bar: end,
                role,
            }
        })
        .collect()
}

fn role_at(sections: &[SectionRole], bar: usize) -> GuitarRole {
    sections
        .iter()
        .find(|section| (section.start_bar..=section.end_bar).contains(&bar))
        .map(|section| section.role)
        .unwrap_or(GuitarRole::Support)
}

fn apply_role(
    target: &mut Bar,
    source: &Bar,
    role: GuitarRole,
    config: &SecondGuitarConfig,
) -> Result<(), SecondGuitarError> {
    let mut event = 0usize;
    for (voice_index, voice) in target.voices.iter_mut().enumerate() {
        let Some(source_voice) = source.voices.get(voice_index) else {
            continue;
        };
        let tail_start = voice.beats.len().saturating_sub(config.phrase_tail_beats);
        for (beat_index, beat) in voice.beats.iter_mut().enumerate() {
            if role == GuitarRole::Support && beat_index % 2 == 1 {
                beat.notes.clear();
                continue;
            }
            for note in &mut beat.notes {
                let Some(pitch) = note.midi else { continue };
                let target_pitch = match role {
                    GuitarRole::Support => pitch,
                    GuitarRole::Double => {
                        if event.is_multiple_of(2) {
                            pitch
                        } else {
                            octave_pitch(pitch, config)?
                        }
                    }
                    GuitarRole::Octave => octave_pitch(pitch, config)?,
                    GuitarRole::Harmonise => harmonized_pitch(pitch, config.key, config)?,
                    GuitarRole::Contextual => unreachable!("contextual is resolved per section"),
                    GuitarRole::PhraseTail => {
                        if note
                            .string
                            .is_some_and(|string| string >= config.phrase_tail_min_string)
                            && pitch >= config.phrase_tail_min_pitch
                            && beat_index >= tail_start
                            && !note.techniques.contains(&crate::model::Technique::PalmMute)
                        {
                            let harmonized = phrase_tail_pitch(pitch, config)?;
                            note.techniques.retain(|technique| {
                                *technique != crate::model::Technique::PalmMute
                            });
                            harmonized
                        } else {
                            pitch
                        }
                    }
                };
                refinger(note, target_pitch, config, role)?;
                event += 1;
            }
            if let Some(source_beat) = source_voice.beats.get(beat_index) {
                beat.dynamic = source_beat.dynamic.clone();
            }
        }
    }
    Ok(())
}

fn phrase_tail_pitch(pitch: i32, config: &SecondGuitarConfig) -> Result<i32, SecondGuitarError> {
    match config.phrase_tail_interval {
        PhraseTailInterval::Octave => octave_pitch(pitch, config),
        PhraseTailInterval::Third | PhraseTailInterval::Sixth => {
            let steps = match config.phrase_tail_interval {
                PhraseTailInterval::Third => 2,
                PhraseTailInterval::Sixth => 5,
                PhraseTailInterval::Octave => unreachable!(),
            };
            let Some(key) = config.key else {
                return harmonized_pitch(pitch, None, config);
            };
            let mode = if key.minor {
                MusicalMode::NaturalMinor
            } else {
                MusicalMode::Major
            };
            let pitches = mode.pitch_classes(key.tonic);
            let pitch_class = pitch.rem_euclid(12);
            let Some(degree) = pitches
                .iter()
                .position(|candidate| *candidate == pitch_class)
            else {
                return harmonized_pitch(pitch, Some(key), config);
            };
            let target_pc = pitches[(degree + steps) % pitches.len()];
            let interval = (target_pc - pitch_class).rem_euclid(12);
            let target = pitch + if interval == 0 { 12 } else { interval };
            position(&config.tuning, target, config.fret_window)
                .map(|_| target)
                .ok_or(SecondGuitarError::Unplayable {
                    role: GuitarRole::PhraseTail,
                    pitch: target,
                })
        }
    }
}

fn octave_pitch(pitch: i32, config: &SecondGuitarConfig) -> Result<i32, SecondGuitarError> {
    [12, -12]
        .into_iter()
        .find(|offset| position(&config.tuning, pitch + offset, config.fret_window).is_some())
        .map(|offset| pitch + offset)
        .ok_or(SecondGuitarError::Unplayable {
            role: GuitarRole::Octave,
            pitch,
        })
}

fn harmonized_pitch(
    pitch: i32,
    key: Option<KeySignature>,
    config: &SecondGuitarConfig,
) -> Result<i32, SecondGuitarError> {
    let Some(key) = key else {
        return [3, 4]
            .into_iter()
            .find_map(|shift| {
                let candidate = pitch + shift;
                position(&config.tuning, candidate, config.fret_window).map(|_| candidate)
            })
            .ok_or(SecondGuitarError::Unplayable {
                role: GuitarRole::Harmonise,
                pitch,
            });
    };
    let mode = if key.minor {
        MusicalMode::NaturalMinor
    } else {
        MusicalMode::Major
    };
    let pitches = mode.pitch_classes(key.tonic);
    let pitch_class = pitch.rem_euclid(12);
    let Some(degree) = pitches
        .iter()
        .position(|candidate| *candidate == pitch_class)
    else {
        return [3, 4]
            .into_iter()
            .find_map(|shift| {
                let candidate = pitch + shift;
                position(&config.tuning, candidate, config.fret_window).map(|_| candidate)
            })
            .ok_or(SecondGuitarError::Unplayable {
                role: GuitarRole::Harmonise,
                pitch,
            });
    };
    let target_pc = pitches[(degree + 2) % pitches.len()];
    let interval = (target_pc - pitch_class).rem_euclid(12);
    let target = pitch + if interval == 0 { 12 } else { interval };
    if position(&config.tuning, target, config.fret_window).is_some() {
        Ok(target)
    } else {
        Err(SecondGuitarError::Unplayable {
            role: GuitarRole::Harmonise,
            pitch: target,
        })
    }
}

fn refinger(
    note: &mut Note,
    pitch: i32,
    config: &SecondGuitarConfig,
    role: GuitarRole,
) -> Result<(), SecondGuitarError> {
    let (string, fret) = position(&config.tuning, pitch, config.fret_window)
        .ok_or(SecondGuitarError::Unplayable { role, pitch })?;
    note.midi = Some(pitch);
    note.string = Some(string);
    note.fret = Some(fret);
    Ok(())
}

fn position(tuning: &[i32], pitch: i32, window: (i32, i32)) -> Option<(u32, i32)> {
    tuning
        .iter()
        .enumerate()
        .filter_map(|(index, open)| {
            let string = u32::try_from(index).ok()?;
            let fret = pitch - open;
            (window.0..=window.1)
                .contains(&fret)
                .then_some((string, fret))
        })
        .min_by_key(|(string, fret)| (*fret, *string))
}

fn ensure_bar_differs(
    target: &mut Bar,
    source: &Bar,
    role: GuitarRole,
    config: &SecondGuitarConfig,
) -> Result<(), SecondGuitarError> {
    if bar_signature(target) != bar_signature(source) {
        return Ok(());
    }
    let Some(note) = target
        .voices
        .iter_mut()
        .flat_map(|voice| &mut voice.beats)
        .flat_map(|beat| &mut beat.notes)
        .find(|note| note.midi.is_some())
    else {
        return Ok(());
    };
    let pitch = note.midi.unwrap_or(0);
    let different = [12, -12, 7, -7]
        .into_iter()
        .find_map(|shift| {
            let candidate = pitch + shift;
            position(&config.tuning, candidate, config.fret_window).map(|_| candidate)
        })
        .ok_or(SecondGuitarError::Unplayable { role, pitch })?;
    refinger(note, different, config, role)
}

fn bar_signature(bar: &Bar) -> Vec<(RhythmKey, Vec<i32>)> {
    bar.voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .map(|beat| {
            (
                RhythmKey::from(beat.rhythm),
                beat.notes.iter().filter_map(|note| note.midi).collect(),
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RhythmKey(crate::model::NoteValue, u8, Option<(u32, u32)>);

impl From<crate::model::Rhythm> for RhythmKey {
    fn from(rhythm: crate::model::Rhythm) -> Self {
        Self(rhythm.value, rhythm.dots, rhythm.tuplet)
    }
}

fn note_count(bar: &Bar) -> usize {
    bar.voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .map(|beat| beat.notes.len())
        .sum()
}

fn unique_name(doc: &Document, name: &str, except: usize) -> String {
    let exists = |candidate: &str| {
        doc.tracks
            .iter()
            .enumerate()
            .any(|(index, track)| index != except && track.name == candidate)
    };
    if !exists(name) {
        return name.to_string();
    }
    for suffix in 2.. {
        let candidate = format!("{name} {suffix}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    unreachable!("an unused track name is always available")
}

#[cfg(test)]
#[path = "second_guitar_test.rs"]
mod tests;
