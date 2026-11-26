//! Document-level workflows for deterministic drum and bass generation.

use std::collections::BTreeMap;

use crate::edits;
use crate::error::{Error, Result};
use crate::model::{Bar, Beat, Document, GeneratorMetadata, Note, NoteValue, Rhythm, Track, Voice};

use super::drums::performance::{generate_seeded_hits, DrumProfile};
use super::drums::presets::SectionDrumFeel;
use super::drums::{extract_composite_bar_onsets, DrumHit, DrumPattern};
use super::execution_log::{Decision, ExecutionLog, GeneratedRole};
use super::meter::BeatGrouping;
use super::pattern_catalog::PatternGrid;
use super::rhythm::{
    derive_bass_tuning, optimize_bass_positions, BassGenerationConfig, BassHit, BassTuning,
};

/// Result of adding a generated track to a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedTrack {
    /// Zero-based index of the newly added track.
    pub track_index: usize,
    /// Number of generated sounding notes.
    pub note_count: usize,
    /// What was decided for each generated bar.
    ///
    /// Counts say a track was written; the log says why each bar looks the way
    /// it does, which is what an assessment finding needs to be traced back to
    /// a decision.
    pub log: ExecutionLog,
}

/// Adds a deterministic percussion track derived from pitched source tracks.
///
/// The source tracks are analyzed bar by bar. The generated hits are solved
/// against the supplied tempo before they are written into a new percussion
/// track. A fixed seed produces the same document structure and note data.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `source_tracks` is empty or names a track outside the
/// document, a bar's meter has no sixteenth grid or beat grouping, or the new track's bars
/// cannot be added to the score.
pub fn generate_drums(
    doc: &mut Document,
    source_tracks: &[usize],
    profile: &DrumProfile,
    seed: u64,
    tempo: u32,
) -> Result<GeneratedTrack> {
    generate_drums_with_sections(doc, source_tracks, profile, seed, tempo, &[])
}

/// Adds a deterministic percussion track with per-range section feels.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `source_tracks` is empty or names a track outside the
/// document, or a section range is empty or outside the document. Also returns it if a bar's
/// meter has no sixteenth grid or beat grouping, or the new track's bars cannot be added.
pub fn generate_drums_with_sections(
    doc: &mut Document,
    source_tracks: &[usize],
    profile: &DrumProfile,
    seed: u64,
    tempo: u32,
    sections: &[super::accompaniment::DrumSectionRequest],
) -> Result<GeneratedTrack> {
    generate_drums_with_profiles(doc, source_tracks, profile, seed, tempo, &[], sections)
}

/// Adds a deterministic percussion track using caller overrides for meter grouping.
///
/// Bars without an override use [`BeatGrouping::for_meter`]. Group starts
/// determine where backbeat snares may be placed, including in odd meters.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `source_tracks` is empty or names a track outside the
/// document, a bar's meter has no sixteenth grid or beat grouping, or the new track's bars
/// cannot be added to the score.
pub fn generate_drums_with_groupings(
    doc: &mut Document,
    source_tracks: &[usize],
    profile: &DrumProfile,
    seed: u64,
    tempo: u32,
    groupings: &[BeatGrouping],
) -> Result<GeneratedTrack> {
    generate_drums_with_profiles(doc, source_tracks, profile, seed, tempo, groupings, &[])
}

/// Adds a percussion track selected from a caller-owned MIDI groove index.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `source_tracks` is empty or names a track outside the
/// document, a bar's meter has no sixteenth grid, or a groove cannot be adapted to a bar's
/// grid. Also returns it if the new track's bars cannot be added to the score.
#[cfg(feature = "midi")]
pub fn generate_drums_from_midi_index(
    doc: &mut Document,
    source_tracks: &[usize],
    index: &super::drums::midi_groove::MidiGrooveIndex,
    profile: &DrumProfile,
    seed: u64,
    tempo: u32,
) -> Result<GeneratedTrack> {
    validate_sources(doc, source_tracks)?;
    let mut bars = Vec::with_capacity(doc.master_bars.len());
    let mut note_count = 0;
    let mut log = ExecutionLog::new(seed);
    let track_index_hint = doc.tracks.len();
    for bar_index in 0..doc.master_bars.len() {
        let meter = doc.master_bars[bar_index].time;
        let steps = steps_per_bar(meter)?;
        let grid =
            PatternGrid::new(meter, 4).map_err(|error| Error::Malformed(error.to_string()))?;
        let onsets = extract_composite_bar_onsets(doc, source_tracks, bar_index);
        let pattern = DrumPattern::from_onsets(std::slice::from_ref(&onsets), steps);
        let fallback = super::drums::midi_groove::SeededGrooveFallback {
            pattern: &pattern,
            profile,
            seed: seed_for_bar(seed, bar_index),
        };
        let generated = super::drums::midi_groove::generate_from_midi_index(
            index,
            "drums",
            grid,
            tempo,
            std::slice::from_ref(&onsets),
            profile.kick_voice,
            Some(fallback),
        )
        .map_err(|error| Error::Malformed(error.to_string()))?;
        let source = generated.source.clone();
        let hits = generated
            .hits
            .into_iter()
            .filter(|hit| hit.step < steps)
            .collect::<Vec<_>>();
        note_count += hits.len();
        log.record(Decision {
            bar: bar_index,
            track: track_index_hint,
            role: GeneratedRole::Drums,
            choice: format!("midi:{source:?}"),
            phrase_position: bar_index % PHRASE_BARS,
            seed: seed_for_bar(seed, bar_index),
            events: hits.len(),
        });
        bars.push(build_drum_bar(steps, &hits));
    }
    let track = Track {
        id: 0,
        name: "Drums".into(),
        color: None,
        tuning: Vec::new(),
        midi_program: Some(0),
        midi_channel: Some(9),
        pan: None,
        volume: None,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
        generator_metadata: Some(GeneratorMetadata {
            engine: "tabslib.drums".into(),
            archetype: Some("midi-library".into()),
            settings: BTreeMap::from([
                ("seed".into(), seed.to_string()),
                ("tempo_bpm".into(), tempo.to_string()),
            ]),
        }),
    };
    let track_index = append_track(doc, track, bars)?;
    for decision in &mut log.decisions {
        decision.track = track_index;
    }
    Ok(GeneratedTrack {
        track_index,
        note_count,
        log,
    })
}

fn generate_drums_with_profiles(
    doc: &mut Document,
    source_tracks: &[usize],
    profile: &DrumProfile,
    seed: u64,
    tempo: u32,
    groupings: &[BeatGrouping],
    sections: &[super::accompaniment::DrumSectionRequest],
) -> Result<GeneratedTrack> {
    validate_sources(doc, source_tracks)?;
    for section in sections {
        if section.start_bar > section.end_bar || section.end_bar >= doc.master_bars.len() {
            return Err(Error::Malformed("invalid drum section range".into()));
        }
    }
    let mut bars = Vec::with_capacity(doc.master_bars.len());
    let mut note_count = 0;
    let mut log = ExecutionLog::new(seed);
    let track_index_hint = doc.tracks.len();
    for bar_index in 0..doc.master_bars.len() {
        let meter = doc.master_bars[bar_index].time;
        let steps = steps_per_bar(meter)?;
        let grouping = grouping_for_meter(meter, groupings)?;
        let grid =
            PatternGrid::new(meter, 4).map_err(|error| Error::Malformed(error.to_string()))?;
        let backbeats = grouping
            .backbeat_positions(grid)
            .map_err(|error| Error::Malformed(error.to_string()))?;
        let section_feel = sections
            .iter()
            .find(|section| section.start_bar <= bar_index && bar_index <= section.end_bar)
            .map(|section| section.feel);
        let active_profile = section_feel.map(SectionDrumFeel::profile);
        let profile = active_profile.as_ref().unwrap_or(profile);
        let onsets = extract_composite_bar_onsets(doc, source_tracks, bar_index);
        let pattern = DrumPattern::from_onsets(&[onsets], steps);
        let bar_seed = seed_for_bar(seed, bar_index);
        let mut hits = generate_seeded_hits(&pattern, profile, bar_seed);
        if let Some(feel) = section_feel {
            apply_section_feel(&mut hits, &pattern, &backbeats, profile, feel);
        } else {
            super::drums::performance::add_seeded_backbeats(
                &mut hits, &backbeats, profile, bar_seed,
            );
        }
        hits = super::drums::solve_limbs(&hits, tempo, false);
        note_count += hits.len();
        log.record(Decision {
            bar: bar_index,
            track: track_index_hint,
            role: GeneratedRole::Drums,
            choice: format!(
                "profile:{};onsets:{}/{steps};grouping:{:?};backbeats:{backbeats:?}",
                section_feel.map_or("document", SectionDrumFeel::name),
                onset_count(&pattern),
                grouping.groups()
            ),
            phrase_position: bar_index % PHRASE_BARS,
            seed: bar_seed,
            events: hits.len(),
        });
        bars.push(build_drum_bar(steps, &hits));
    }
    let track = Track {
        id: 0,
        name: "Drums".into(),
        color: None,
        tuning: Vec::new(),
        midi_program: Some(0),
        midi_channel: Some(9),
        pan: None,
        volume: None,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
        generator_metadata: Some(GeneratorMetadata {
            engine: "tabslib.drums".into(),
            archetype: Some("seeded".into()),
            settings: BTreeMap::from([
                ("seed".into(), seed.to_string()),
                ("tempo_bpm".into(), tempo.to_string()),
            ]),
        }),
    };
    let track_index = append_track(doc, track, bars)?;
    for decision in &mut log.decisions {
        decision.track = track_index;
    }
    Ok(GeneratedTrack {
        track_index,
        note_count,
        log,
    })
}

fn apply_section_feel(
    hits: &mut Vec<DrumHit>,
    pattern: &DrumPattern,
    backbeats: &[u32],
    profile: &DrumProfile,
    feel: SectionDrumFeel,
) {
    let snares = match feel {
        SectionDrumFeel::FullBackbeat => backbeats.to_vec(),
        SectionDrumFeel::HalfBackbeat => backbeats.iter().copied().step_by(2).collect(),
        SectionDrumFeel::QuarterBackbeat => backbeats.iter().copied().step_by(4).collect(),
        SectionDrumFeel::AlternatingKickSnare => {
            for (index, step) in backbeats.iter().copied().enumerate() {
                let midi = if index % 2 == 0 {
                    profile.kick_voice
                } else {
                    profile.snare_voice
                };
                hits.push(DrumHit::new(midi, step));
            }
            return;
        }
        SectionDrumFeel::LockedKickSteadyCymbal => {
            for (step, source) in pattern.bars[0].iter().enumerate() {
                if source.onset {
                    hits.push(DrumHit::new(
                        profile.kick_voice,
                        u32::try_from(step).unwrap_or(u32::MAX),
                    ));
                }
                hits.push(DrumHit::new(
                    profile.cymbal_voice,
                    u32::try_from(step).unwrap_or(u32::MAX),
                ));
            }
            return;
        }
    };
    for step in snares {
        let mut hit = DrumHit::new(profile.snare_voice, step);
        hit.accent = true;
        hits.push(hit);
    }
}

/// Bars in the phrase a decision's position is reported against.
const PHRASE_BARS: usize = 4;

/// Attacks the source material offered the generator in one bar.
fn onset_count(pattern: &DrumPattern) -> usize {
    pattern
        .bars
        .iter()
        .flatten()
        .filter(|step| step.onset)
        .count()
}

/// Adds a configurable bass track derived from pitched source tracks.
///
/// The selected strategy expands riff roots into bass events. The tuning may
/// be derived from a supported guitar tuning or supplied by the caller. A
/// document-wide optimizer keeps each event inside `max_fret` and minimizes
/// string skips and hand-position shifts across bar boundaries.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `source_tracks` is empty or names a track outside the
/// document, a custom tuning is not 4 or 5 ascending MIDI pitches, or no derived tuning is
/// supported. Also returns it if a bar's meter has no sixteenth grid or beat grouping, the bass
/// notes do not fit `max_fret`, or the new track's bars cannot be added.
pub fn generate_bass(
    doc: &mut Document,
    source_tracks: &[usize],
    config: BassGenerationConfig<'_>,
) -> Result<GeneratedTrack> {
    generate_bass_with_groupings(doc, source_tracks, config, &[])
}

/// Adds a configurable bass track using caller overrides for meter grouping.
///
/// Group starts align bass accents in addition to the strategy's normal
/// source-riff placement.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `source_tracks` is empty or names a track outside the
/// document, a custom tuning is not 4 or 5 ascending MIDI pitches, or no derived tuning is
/// supported. Also returns it if a bar's meter has no sixteenth grid or beat grouping, the bass
/// notes do not fit `max_fret`, or the new track's bars cannot be added.
pub fn generate_bass_with_groupings(
    doc: &mut Document,
    source_tracks: &[usize],
    config: BassGenerationConfig<'_>,
    groupings: &[BeatGrouping],
) -> Result<GeneratedTrack> {
    validate_sources(doc, source_tracks)?;
    let tuning = match config.tuning {
        BassTuning::Derived => source_tracks
            .iter()
            .filter_map(|index| derive_bass_tuning(&doc.tracks[*index].tuning))
            .min_by_key(|candidate| candidate[0])
            .ok_or_else(|| {
                Error::Malformed("source tracks do not use a supported guitar tuning".into())
            })?,
        BassTuning::Custom(tuning) => validate_bass_tuning(tuning)?,
    };
    let mut hits_by_bar = Vec::with_capacity(doc.master_bars.len());
    let mut steps_by_bar = Vec::with_capacity(doc.master_bars.len());
    let mut log = ExecutionLog::new(0);
    let track_index_hint = doc.tracks.len();
    for bar_index in 0..doc.master_bars.len() {
        let meter = doc.master_bars[bar_index].time;
        let steps = steps_per_bar(meter)?;
        let grouping = grouping_for_meter(meter, groupings)?;
        let grid =
            PatternGrid::new(meter, 4).map_err(|error| Error::Malformed(error.to_string()))?;
        let pulses = grouping
            .pulse_positions(grid)
            .map_err(|error| Error::Malformed(error.to_string()))?;
        let onsets = extract_composite_bar_onsets(doc, source_tracks, bar_index);
        let hits = super::rhythm::generate_bass_pattern_with_pulses(
            &onsets,
            config.profile,
            config.strategy,
            steps,
            &pulses,
        );
        log.record(Decision {
            bar: bar_index,
            track: track_index_hint,
            role: GeneratedRole::Bass,
            choice: format!("{:?}", config.strategy),
            phrase_position: bar_index % PHRASE_BARS,
            seed: 0,
            events: hits.len(),
        });
        hits_by_bar.push(hits);
        steps_by_bar.push(steps);
    }
    let all_hits = hits_by_bar.iter().flatten().copied().collect::<Vec<_>>();
    let positions = optimize_bass_positions(&all_hits, &tuning, config.max_fret)
        .ok_or_else(|| Error::Malformed("bass notes exceed the configured fret window".into()))?;
    let note_count = all_hits.len();
    let mut position_cursor = 0;
    let mut bars = Vec::with_capacity(doc.master_bars.len());
    for (steps, hits) in steps_by_bar.into_iter().zip(hits_by_bar.iter()) {
        let end = position_cursor + hits.len();
        bars.push(build_bass_bar(
            steps,
            hits,
            &positions[position_cursor..end],
        )?);
        position_cursor = end;
    }
    let track = Track {
        id: 0,
        name: "Bass".into(),
        color: None,
        tuning,
        midi_program: Some(33),
        midi_channel: Some(0),
        pan: None,
        volume: None,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
        generator_metadata: Some(GeneratorMetadata {
            engine: "tabslib.bass".into(),
            archetype: Some(format!("{:?}", config.strategy)),
            settings: BTreeMap::from([("max_fret".into(), config.max_fret.to_string())]),
        }),
    };
    let track_index = append_track(doc, track, bars)?;
    for decision in &mut log.decisions {
        decision.track = track_index;
    }
    Ok(GeneratedTrack {
        track_index,
        note_count,
        log,
    })
}

fn validate_bass_tuning(tuning: &[i32]) -> Result<Vec<i32>> {
    if !(4..=5).contains(&tuning.len())
        || tuning.iter().any(|pitch| !(0..=127).contains(pitch))
        || tuning.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(Error::Malformed(
            "custom bass tuning must contain 4 or 5 ascending MIDI pitches".into(),
        ));
    }
    Ok(tuning.to_vec())
}

fn validate_sources(doc: &Document, source_tracks: &[usize]) -> Result<()> {
    if source_tracks.is_empty() {
        return Err(Error::Malformed(
            "at least one source track is required".into(),
        ));
    }
    if source_tracks.iter().any(|track| *track >= doc.tracks.len()) {
        return Err(Error::Malformed(
            "source track index is outside the document".into(),
        ));
    }
    Ok(())
}

fn grouping_for_meter(meter: (u32, u32), overrides: &[BeatGrouping]) -> Result<BeatGrouping> {
    if let Some(grouping) = overrides.iter().find(|grouping| grouping.meter() == meter) {
        return Ok(grouping.clone());
    }
    BeatGrouping::for_meter(meter).map_err(|error| Error::Malformed(error.to_string()))
}

fn steps_per_bar(time: (u32, u32)) -> Result<u32> {
    let (numerator, denominator) = time;
    if numerator == 0 || denominator == 0 || numerator.checked_mul(16).is_none() {
        return Err(Error::Malformed("invalid meter for generated track".into()));
    }
    let steps = numerator * 16;
    if steps % denominator != 0 {
        return Err(Error::Malformed(
            "meter cannot be represented on a sixteenth grid".into(),
        ));
    }
    Ok(steps / denominator)
}

fn seed_for_bar(seed: u64, bar: usize) -> u64 {
    seed.wrapping_add((bar as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

fn build_drum_bar(steps: u32, hits: &[DrumHit]) -> Bar {
    let mut notes = vec![Vec::new(); steps as usize];
    for hit in hits {
        if let Some(slot) = notes.get_mut(hit.step as usize) {
            slot.push(Note {
                id: 0,
                midi: Some(i32::from(hit.midi)),
                string: None,
                fret: None,
                articulation: Some(i32::from(hit.midi)),
                techniques: Vec::new(),
            });
        }
    }
    build_bar(notes)
}

fn build_bass_bar(
    steps: u32,
    hits: &[BassHit],
    positions: &[super::rhythm::BassFretPosition],
) -> Result<Bar> {
    if hits.len() != positions.len() {
        return Err(Error::Malformed(
            "bass position count does not match generated events".into(),
        ));
    }
    let mut beats = Vec::new();
    let mut cursor = 0;
    for (index, (hit, position)) in hits.iter().zip(positions).enumerate() {
        if hit.step < cursor || hit.step >= steps {
            return Err(Error::Malformed("bass event steps are out of order".into()));
        }
        append_rest_beats(&mut beats, hit.step - cursor);
        let next_step = hits.get(index + 1).map(|next| next.step).unwrap_or(steps);
        let duration = hit
            .duration_steps
            .max(1)
            .min(next_step.saturating_sub(hit.step).max(1))
            .min(steps - hit.step);
        let chunks = rhythm_chunks(duration);
        for (chunk_index, rhythm) in chunks.iter().copied().enumerate() {
            let mut techniques = Vec::new();
            if chunk_index > 0 {
                techniques.push(crate::model::Technique::TieDestination);
            }
            if chunk_index + 1 < chunks.len() {
                techniques.push(crate::model::Technique::TieOrigin);
            }
            beats.push(Beat {
                id: 0,
                rhythm,
                notes: vec![Note {
                    id: 0,
                    midi: Some(i32::from(hit.midi)),
                    string: Some(position.string),
                    fret: Some(i32::try_from(position.fret).map_err(|_| {
                        Error::Malformed("optimized bass fret exceeds model range".into())
                    })?),
                    articulation: None,
                    techniques,
                }],
                dynamic: (chunk_index == 0 && hit.accent).then(|| "F".to_string()),
            });
        }
        cursor = hit.step.saturating_add(duration);
    }
    if cursor < steps {
        append_rest_beats(&mut beats, steps - cursor);
    }
    Ok(Bar {
        id: 0,
        clef: Some("Neutral".into()),
        voices: vec![Voice { id: 0, beats }],
    })
}

fn append_rest_beats(beats: &mut Vec<Beat>, steps: u32) {
    beats.extend(rhythm_chunks(steps).into_iter().map(|rhythm| Beat {
        id: 0,
        rhythm,
        notes: Vec::new(),
        dynamic: None,
    }));
}

fn rhythm_chunks(mut steps: u32) -> Vec<Rhythm> {
    let choices = [
        (28, NoteValue::Whole, 2),
        (24, NoteValue::Whole, 1),
        (16, NoteValue::Whole, 0),
        (14, NoteValue::Half, 2),
        (12, NoteValue::Half, 1),
        (8, NoteValue::Half, 0),
        (7, NoteValue::Quarter, 2),
        (6, NoteValue::Quarter, 1),
        (4, NoteValue::Quarter, 0),
        (3, NoteValue::Eighth, 1),
        (2, NoteValue::Eighth, 0),
        (1, NoteValue::Sixteenth, 0),
    ];
    let mut result = Vec::new();
    while steps > 0 {
        let mut selected = (1, NoteValue::Sixteenth, 0);
        for choice in choices {
            if choice.0 <= steps {
                selected = choice;
                break;
            }
        }
        let (duration, value, dots) = selected;
        result.push(Rhythm {
            value,
            dots,
            tuplet: None,
        });
        steps -= duration;
    }
    result
}

fn build_bar(notes: Vec<Vec<Note>>) -> Bar {
    Bar {
        id: 0,
        clef: Some("Neutral".into()),
        voices: vec![Voice {
            id: 0,
            beats: notes
                .into_iter()
                .map(|notes| Beat {
                    id: 0,
                    rhythm: Rhythm::new(NoteValue::Sixteenth),
                    notes,
                    dynamic: None,
                })
                .collect(),
        }],
    }
}

fn append_track(doc: &mut Document, track: Track, mut bars: Vec<Bar>) -> Result<usize> {
    let track_index = edits::create_track(doc, track);
    let mut next_bar_id = doc
        .bars
        .iter()
        .map(|bar| bar.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut next_note_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .map(|note| note.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut next_voice_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .map(|voice| voice.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mut next_beat_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .map(|beat| beat.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for (bar_index, bar) in bars.iter_mut().enumerate() {
        bar.id = next_bar_id;
        next_bar_id = next_bar_id.saturating_add(1);
        for voice in &mut bar.voices {
            voice.id = next_voice_id;
            next_voice_id = next_voice_id.saturating_add(1);
            for beat in &mut voice.beats {
                beat.id = next_beat_id;
                next_beat_id = next_beat_id.saturating_add(1);
                for note in &mut beat.notes {
                    note.id = next_note_id;
                    next_note_id = next_note_id.saturating_add(1);
                }
            }
        }
        let Some(master) = doc.master_bars.get_mut(bar_index) else {
            return Err(Error::Malformed(
                "generated bar count does not match score".into(),
            ));
        };
        let Some(reference) = master.bar_ids.get_mut(track_index) else {
            return Err(Error::Malformed(
                "generated track bar reference is missing".into(),
            ));
        };
        *reference = i32::try_from(bar.id)
            .map_err(|_| Error::Malformed("generated bar id exceeds format range".into()))?;
    }
    doc.bars.extend(bars);
    Ok(track_index)
}
