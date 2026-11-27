//! Applying a plan's section settings to the assembled bars: energy, texture and seeded variation.

use super::*;

/// Writes each section's energy and instrumentation onto the assembled bars.
///
/// Both are plan-level decisions with no place in the motif material, so they
/// are applied after assembly rather than folded into the selection. A section
/// that sets neither is left exactly as `compose_song` wrote it.
pub(super) fn apply_section_settings(
    doc: &mut Document,
    sections: &[SectionRequest],
) -> Result<(), ArrangementError> {
    let mut start = 0usize;
    for section in sections {
        let end = start + section.bars;
        if let Some(energy) = section.energy.or(section.climax.then_some(100)) {
            if energy > 100 {
                return Err(ArrangementError::InvalidInput(format!(
                    "section energy {energy} is outside 0..=100"
                )));
            }
            let dynamic = dynamic_for(energy);
            for bar_index in start..end.min(doc.master_bars.len()) {
                for track_index in 0..doc.tracks.len() {
                    let Some(id) = doc
                        .master_bars
                        .get(bar_index)
                        .and_then(|master| master.bar_ids.get(track_index))
                        .copied()
                    else {
                        continue;
                    };
                    let Some(bar) = doc
                        .bars
                        .iter_mut()
                        .find(|bar| i64::from(bar.id) == i64::from(id))
                    else {
                        continue;
                    };
                    for voice in &mut bar.voices {
                        for beat in &mut voice.beats {
                            if !beat.notes.is_empty() {
                                beat.dynamic = Some(dynamic.to_string());
                            }
                        }
                    }
                }
            }
        }
        if let Some(tracks) = &section.instrumentation {
            if let Some(bad) = tracks.iter().find(|index| **index >= doc.tracks.len()) {
                return Err(ArrangementError::InvalidInput(format!(
                    "section instrumentation names track {bad}, which does not exist"
                )));
            }
        }
        if let Some(texture) = &section.energy_texture {
            validate_energy_texture(doc, texture)?;
        }
        if section.instrumentation.is_some() || section.energy.is_some() {
            for bar_index in start..end.min(doc.master_bars.len()) {
                for track_index in 0..doc.tracks.len() {
                    let instrumentation_mutes = !section.climax
                        && section
                            .instrumentation
                            .as_ref()
                            .is_some_and(|tracks| !tracks.contains(&track_index));
                    let texture_mutes = section
                        .energy_texture
                        .as_ref()
                        .zip(section.energy)
                        .is_some_and(|(texture, energy)| {
                            energy < texture.unmute_at
                                && texture.optional_tracks.contains(&track_index)
                        });
                    if !instrumentation_mutes && !texture_mutes {
                        continue;
                    }
                    let Some(id) = doc
                        .master_bars
                        .get(bar_index)
                        .and_then(|master| master.bar_ids.get(track_index))
                        .copied()
                    else {
                        continue;
                    };
                    let Some(bar) = doc
                        .bars
                        .iter_mut()
                        .find(|bar| i64::from(bar.id) == i64::from(id))
                    else {
                        continue;
                    };
                    for voice in &mut bar.voices {
                        for beat in &mut voice.beats {
                            beat.notes.clear();
                        }
                    }
                }
            }
        }
        if let Some(texture) = &section.energy_texture {
            if let Some(energy) = section.energy {
                if energy >= texture.octave_at {
                    if let Some(track_index) = texture.octave_track {
                        add_octave_doubling(doc, start, end, track_index)?;
                    }
                }
            }
        }
        if section.climax {
            apply_climax_texture(doc, start, end);
        }
        start = end;
    }
    Ok(())
}

pub(super) fn apply_climax_texture(doc: &mut Document, start: usize, end: usize) {
    for track_index in 0..doc.tracks.len() {
        if !doc.tracks[track_index].tuning.is_empty() {
            continue;
        }
        for bar_index in start..end.min(doc.master_bars.len()) {
            let Some(id) = doc.master_bars[bar_index].bar_ids.get(track_index).copied() else {
                continue;
            };
            let Ok(id) = u32::try_from(id) else { continue };
            if let Some(bar) = doc.bars.iter_mut().find(|bar| bar.id == id) {
                for voice in &mut bar.voices {
                    let mut snare = 0usize;
                    for beat in &mut voice.beats {
                        for note in &mut beat.notes {
                            if note.articulation == Some(38) {
                                snare += 1;
                                if snare.is_multiple_of(2) {
                                    note.articulation = Some(42);
                                    note.midi = Some(42);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn validate_energy_texture(
    doc: &Document,
    texture: &EnergyTexture,
) -> Result<(), ArrangementError> {
    if texture.unmute_at > 100 || texture.octave_at > 100 {
        return Err(ArrangementError::InvalidInput(
            "energy texture thresholds must be within 0..=100".into(),
        ));
    }
    if texture
        .optional_tracks
        .iter()
        .chain(texture.octave_track.iter())
        .any(|track| *track >= doc.tracks.len())
    {
        return Err(ArrangementError::InvalidInput(
            "energy texture names a track that does not exist".into(),
        ));
    }
    Ok(())
}

pub(super) fn add_octave_doubling(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    track_index: usize,
) -> Result<(), ArrangementError> {
    let mut next_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .map(|note| note.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for bar_index in start_bar..end_bar.min(doc.master_bars.len()) {
        let bar_id = doc
            .master_bars
            .get(bar_index)
            .and_then(|master| master.bar_ids.get(track_index))
            .and_then(|id| u32::try_from(*id).ok())
            .ok_or_else(|| {
                ArrangementError::InvalidInput("missing texture bar reference".into())
            })?;
        let bar = doc
            .bars
            .iter_mut()
            .find(|bar| bar.id == bar_id)
            .ok_or_else(|| ArrangementError::InvalidInput("missing texture bar".into()))?;
        for voice in &mut bar.voices {
            let mut added = Vec::new();
            for beat in &voice.beats {
                for note in &beat.notes {
                    let Some(midi) = note.midi.and_then(|midi| midi.checked_add(12)) else {
                        continue;
                    };
                    let mut doubled = note.clone();
                    doubled.id = next_id;
                    next_id = next_id.saturating_add(1);
                    doubled.midi = Some(midi);
                    doubled.fret = doubled.fret.and_then(|fret| fret.checked_add(12));
                    added.push((beat.id, doubled));
                }
            }
            for (beat_id, note) in added {
                if let Some(beat) = voice.beats.iter_mut().find(|beat| beat.id == beat_id) {
                    beat.notes.push(note);
                }
            }
        }
    }
    Ok(())
}

/// Applies a deterministic octave variation to an inclusive master-bar range.
///
/// The seed selects among valid `-12`, `0`, and `+12` semitone transforms.
/// Transposition updates sounding pitches and fretted positions together. The
/// document is unchanged if the requested range is invalid.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if `start_bar > end_bar`, `end_bar` is outside
/// the score, or none of the `-12`, `0` and `+12` transforms can be applied to the range.
/// Returns [`ArrangementError::InvalidTransposition`] if the chosen transposition fails.
pub fn apply_seeded_variation(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    seed: u64,
) -> Result<i32, ArrangementError> {
    if start_bar > end_bar || end_bar >= doc.master_bars.len() {
        return Err(ArrangementError::InvalidInput(format!(
            "variation range {start_bar}..={end_bar} is outside the score"
        )));
    }
    let candidates = [-12, 0, 12]
        .into_iter()
        .filter(|semitones| range_can_transpose(doc, start_bar, end_bar, *semitones))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(ArrangementError::InvalidInput(
            "no valid variation transform".into(),
        ));
    }
    let nonzero = candidates
        .iter()
        .copied()
        .filter(|semitones| *semitones != 0)
        .collect::<Vec<_>>();
    let choices = if nonzero.is_empty() {
        &candidates
    } else {
        &nonzero
    };
    let semitones = choices[seeded_index(seed, choices.len())];

    transpose_selected_bars(doc, start_bar, end_bar, semitones)?;
    doc.source = None;
    Ok(semitones)
}
