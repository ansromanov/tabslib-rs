//! Motif segmentation, compatibility scoring, transitions, and song assembly.
//!
//! The operations in this module work on written document bars. They preserve
//! exact rhythms and report invalid timing instead of changing it.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::inspect::{bar_capacity, bar_integrity, rhythm_duration, Fraction};
use crate::model::Document;

use super::arrangement_support::{
    fit_bar_to_capacity, range_can_transpose, reassign_ids, seeded_index, splitmix64,
    transpose_bar, transpose_fingerprint, transpose_selected_bars, u32_to_bar_ref, IdAllocator,
};

mod sections;
mod types;
pub use sections::*;
pub use types::*;

/// Fingerprints the content of one master bar across its tracks.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if `bar_index` is outside the score, the bar
/// does not have one bar reference per track, or a reference names a missing bar.
pub fn fingerprint_bar(
    doc: &Document,
    bar_index: usize,
) -> Result<MotifFingerprint, ArrangementError> {
    let master = doc
        .master_bars
        .get(bar_index)
        .ok_or_else(|| ArrangementError::InvalidInput(format!("bar index {bar_index}")))?;
    if master.bar_ids.len() != doc.tracks.len() {
        return Err(ArrangementError::InvalidInput(format!(
            "bar {bar_index} has {} track references for {} tracks",
            master.bar_ids.len(),
            doc.tracks.len()
        )));
    }

    let mut rhythms = Vec::with_capacity(doc.tracks.len());
    let mut pitch_classes = Vec::with_capacity(doc.tracks.len());
    let mut intervals = Vec::with_capacity(doc.tracks.len());
    let mut note_counts = Vec::with_capacity(doc.tracks.len());
    let mut empty_tracks = Vec::new();

    for (track_index, bar_id) in master.bar_ids.iter().enumerate() {
        let Some(id) = u32::try_from(*bar_id).ok() else {
            if *bar_id == -1 {
                rhythms.push(Vec::new());
                pitch_classes.push(Vec::new());
                intervals.push(Vec::new());
                note_counts.push(0);
                empty_tracks.push(track_index);
                continue;
            }
            return Err(ArrangementError::InvalidInput(format!(
                "bar {bar_index} refers to missing bar id {bar_id}"
            )));
        };
        let Some(bar) = doc.bars.iter().find(|bar| bar.id == id) else {
            return Err(ArrangementError::InvalidInput(format!(
                "bar {bar_index} refers to missing bar id {bar_id}"
            )));
        };

        let mut track_rhythms = Vec::new();
        let mut track_pitches = Vec::new();
        let mut prior_pitch = None;
        let mut track_intervals = Vec::new();
        let mut count = 0;
        for voice in &bar.voices {
            for beat in &voice.beats {
                track_rhythms.push(beat.rhythm.as_fraction());
                for note in &beat.notes {
                    if let Some(midi) = note.midi {
                        let pitch_class = u8::try_from(midi.rem_euclid(12)).unwrap_or_default();
                        track_pitches.push(pitch_class);
                        if let Some(previous) = prior_pitch {
                            track_intervals.push(midi - previous);
                        }
                        prior_pitch = Some(midi);
                        count += 1;
                    }
                }
            }
        }
        if count == 0 {
            empty_tracks.push(track_index);
        }
        rhythms.push(track_rhythms);
        pitch_classes.push(track_pitches);
        intervals.push(track_intervals);
        note_counts.push(count);
    }

    Ok(MotifFingerprint {
        rhythms,
        pitch_classes,
        intervals,
        note_counts,
        empty_tracks,
    })
}

/// Segments the score into contiguous regions of similar bars.
///
/// Regions split when adjacent-bar compatibility drops below `threshold` or
/// `max_bars` is reached. Thresholds are clamped to `0..=1`.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if `max_bars` is zero, and any error from
/// [`fingerprint_bar`] if a bar cannot be fingerprinted.
pub fn segment_motifs(
    doc: &Document,
    max_bars: usize,
    threshold: f64,
) -> Result<Vec<Motif>, ArrangementError> {
    if max_bars == 0 {
        return Err(ArrangementError::InvalidInput(
            "max_bars must be greater than zero".into(),
        ));
    }
    let mut fingerprints = Vec::with_capacity(doc.master_bars.len());
    for index in 0..doc.master_bars.len() {
        fingerprints.push(fingerprint_bar(doc, index)?);
    }
    if fingerprints.is_empty() {
        return Ok(Vec::new());
    }

    let threshold = threshold.clamp(0.0, 1.0);
    let mut motifs = Vec::new();
    let mut start = 0;
    for index in 1..=fingerprints.len() {
        let section_boundary = index < fingerprints.len()
            && doc
                .master_bars
                .get(index)
                .and_then(|bar| bar.section.as_ref())
                .is_some();
        let split = index == fingerprints.len()
            || index - start >= max_bars
            || section_boundary
            || score_compatibility(&fingerprints[index - 1], &fingerprints[index]).score
                < threshold;
        if split {
            let end = index - 1;
            motifs.push(Motif {
                start_bar: start,
                end_bar: end,
                fingerprint: combine_fingerprints(&fingerprints[start..=end]),
            });
            start = index;
        }
    }
    Ok(motifs)
}

/// Scores how well two fingerprints fit together.
pub fn score_compatibility(left: &MotifFingerprint, right: &MotifFingerprint) -> Compatibility {
    let rhythm = nested_similarity(&left.rhythms, &right.rhythms);
    let pitch = nested_similarity(&left.pitch_classes, &right.pitch_classes);
    let contour = nested_similarity(&left.intervals, &right.intervals);
    let density = count_similarity(&left.note_counts, &right.note_counts);
    Compatibility {
        rhythm,
        pitch,
        contour,
        density,
        score: rhythm * 0.35 + pitch * 0.25 + contour * 0.25 + density * 0.15,
    }
}

/// Checks whether adjacent bars can be joined without repairing timing.
///
/// # Errors
///
/// Returns any error from [`fingerprint_bar`] if `left_bar` or `right_bar` is outside the
/// score or refers to malformed bar data.
pub fn check_join(
    doc: &Document,
    left_bar: usize,
    right_bar: usize,
) -> Result<JoinReport, ArrangementError> {
    let left = fingerprint_bar(doc, left_bar)?;
    let right = fingerprint_bar(doc, right_bar)?;
    let mut problems = Vec::new();
    for item in bar_integrity(doc) {
        if (item.bar_index == left_bar || item.bar_index == right_bar)
            && item.duration > item.capacity
        {
            problems.push(format!(
                "track {} bar {} voice {} is overfull",
                item.track_index, item.bar_index, item.voice_index
            ));
        }
    }
    Ok(JoinReport {
        valid: problems.is_empty(),
        compatibility: score_compatibility(&left, &right),
        problems,
    })
}

/// Assembles requested sections from existing motif bars.
///
/// Candidate ties are resolved with `seed`; identical inputs and seed always
/// produce an identical document. Motifs repeat from their first bar to fill
/// the requested section length. Every copied rhythmic value is preserved.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if there are no sections, more than one is
/// marked as the climax, a section requests zero bars or has no candidates, a candidate
/// motif index or range is outside `motifs` or the score, no candidate can take the
/// section's transposition, or a source bar reference is missing. Returns
/// [`ArrangementError::OverfullBar`] if a copied voice still exceeds its destination bar,
/// and [`ArrangementError::InvalidTransposition`] if a transposition cannot be represented.
pub fn compose_song(
    source: &Document,
    motifs: &[Motif],
    sections: &[SectionRequest],
    seed: u64,
) -> Result<Document, ArrangementError> {
    if sections.is_empty() {
        return Err(ArrangementError::InvalidInput(
            "composition needs at least one section".into(),
        ));
    }
    if sections.iter().filter(|section| section.climax).count() > 1 {
        return Err(ArrangementError::InvalidInput(
            "an arrangement can have only one sectional climax".into(),
        ));
    }

    let mut chosen = Vec::with_capacity(sections.len());
    let mut state = seed;
    let mut prior_fingerprint: Option<MotifFingerprint> = None;
    for section in sections {
        if section.bars == 0 || section.candidates.is_empty() {
            return Err(ArrangementError::InvalidInput(format!(
                "section {:?} must request bars and candidates",
                section.label
            )));
        }
        let mut ranked = Vec::new();
        for index in &section.candidates {
            let motif = motifs.get(*index).ok_or_else(|| {
                ArrangementError::InvalidInput(format!(
                    "motif index {index} is unavailable (available motifs: {})",
                    motifs.len()
                ))
            })?;
            validate_motif(source, motif)?;
            if !range_can_transpose(
                source,
                motif.start_bar,
                motif.end_bar,
                section.transpose_semitones,
            ) {
                continue;
            }
            let candidate_start = transpose_fingerprint(
                fingerprint_bar(source, motif.start_bar)?,
                section.transpose_semitones,
            );
            let score = prior_fingerprint.as_ref().map_or(1.0, |prior| {
                score_compatibility(prior, &candidate_start).score
            });
            ranked.push((*index, score));
        }
        if ranked.is_empty() {
            return Err(ArrangementError::InvalidInput(format!(
                "section {:?} has no candidate valid for its transposition",
                section.label
            )));
        }
        let best = ranked
            .iter()
            .map(|(_, score)| *score)
            .fold(f64::NEG_INFINITY, f64::max);
        let tied = ranked
            .iter()
            .filter(|(_, score)| (*score - best).abs() < f64::EPSILON)
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        state = splitmix64(state);
        let choice = tied[seeded_index(state, tied.len())];
        let motif = &motifs[choice];
        let last_offset = (section.bars - 1) % (motif.end_bar - motif.start_bar + 1);
        let last_bar = motif.start_bar + last_offset;
        prior_fingerprint = Some(transpose_fingerprint(
            fingerprint_bar(source, last_bar)?,
            section.transpose_semitones,
        ));
        chosen.push(choice);
    }

    let mut output = source.clone();
    output.master_bars.clear();
    output.bars.clear();
    output.tempo_map.clear();
    output.source = None;
    output.title = if source.title.is_empty() {
        "Arrangement".into()
    } else {
        format!("{} (arrangement)", source.title)
    };

    let mut next_ids = IdAllocator::from_document(source);
    for (section, motif_index) in sections.iter().zip(chosen) {
        let motif = &motifs[motif_index];
        let motif_len = motif.end_bar - motif.start_bar + 1;
        for offset in 0..section.bars {
            let source_bar_index = motif.start_bar + offset % motif_len;
            let source_master = source.master_bars.get(source_bar_index).ok_or_else(|| {
                ArrangementError::InvalidInput(format!("source bar {source_bar_index}"))
            })?;

            let mut master = source_master.clone();
            master.index = output.master_bars.len();
            master.section = if offset == 0 {
                Some(section.label.clone())
            } else {
                None
            };
            master.repeat_start = false;
            master.repeat_end = None;
            master.alternate_ending = 0;
            master.direction = None;
            master.bar_ids.clear();
            let destination_capacity = bar_capacity(master.time);
            for (track_index, source_id) in source_master.bar_ids.iter().enumerate() {
                if *source_id < 0 {
                    master.bar_ids.push(-1);
                    continue;
                }
                let source_id = u32::try_from(*source_id).map_err(|_| {
                    ArrangementError::InvalidInput(format!("invalid bar reference {source_id}"))
                })?;
                let bar = source
                    .bars
                    .iter()
                    .find(|bar| bar.id == source_id)
                    .ok_or_else(|| {
                        ArrangementError::InvalidInput(format!("missing source bar id {source_id}"))
                    })?;
                let mut copied = bar.clone();
                fit_bar_to_capacity(&mut copied, destination_capacity);
                for (voice_index, voice) in copied.voices.iter().enumerate() {
                    let duration = voice.beats.iter().fold(Fraction::new(0, 1), |sum, beat| {
                        sum.plus(rhythm_duration(beat.rhythm))
                    });
                    if duration > destination_capacity {
                        return Err(ArrangementError::OverfullBar {
                            track: track_index,
                            bar: master.index + 1,
                            voice: voice_index,
                            source_bar: source_bar_index + 1,
                        });
                    }
                }
                reassign_ids(&mut copied, &mut next_ids)?;
                transpose_bar(&mut copied, section.transpose_semitones)?;
                master.bar_ids.push(u32_to_bar_ref(copied.id)?);
                output.bars.push(copied);
            }
            let output_bar = output.master_bars.len();
            for tempo in source
                .tempo_map
                .iter()
                .filter(|tempo| tempo.at_bar == source_bar_index)
            {
                let mut mapped = *tempo;
                mapped.at_bar = output_bar;
                output.tempo_map.push(mapped);
            }
            output.master_bars.push(master);
        }
    }
    Ok(output)
}

/// Assembles and applies a reviewed arrangement plan.
///
/// # Errors
///
/// Returns [`ArrangementError::InvalidInput`] if the plan version is not 1 or its
/// section settings are invalid (energy above 100, an instrumentation track that does not
/// exist, or an invalid energy texture). Otherwise returns any error from [`compose_song`]
/// or from applying the plan's transitions.
pub fn compose_plan(
    source: &Document,
    motifs: &[Motif],
    plan: &ArrangementPlan,
) -> Result<Document, ArrangementError> {
    if plan.version != 1 {
        return Err(ArrangementError::InvalidInput(format!(
            "unsupported arrangement plan version {}",
            plan.version
        )));
    }
    let mut output = compose_song(source, motifs, &plan.sections, plan.seed)?;
    apply_section_settings(&mut output, &plan.sections)?;
    super::transitions::apply_transitions(&mut output, &plan.transitions)?;
    Ok(output)
}

#[cfg(test)]
#[path = "arrangement_test.rs"]
mod tests;

fn validate_motif(doc: &Document, motif: &Motif) -> Result<(), ArrangementError> {
    if motif.start_bar > motif.end_bar || motif.end_bar >= doc.master_bars.len() {
        return Err(ArrangementError::InvalidInput(format!(
            "motif range {}..={} is outside the score",
            motif.start_bar, motif.end_bar
        )));
    }
    Ok(())
}

fn combine_fingerprints(parts: &[MotifFingerprint]) -> MotifFingerprint {
    let tracks = parts.first().map(|item| item.rhythms.len()).unwrap_or(0);
    let mut result = MotifFingerprint {
        rhythms: vec![Vec::new(); tracks],
        pitch_classes: vec![Vec::new(); tracks],
        intervals: vec![Vec::new(); tracks],
        note_counts: vec![0; tracks],
        empty_tracks: Vec::new(),
    };
    for part in parts {
        for track in 0..tracks {
            result.rhythms[track].extend(part.rhythms.get(track).into_iter().flatten().copied());
            result.pitch_classes[track]
                .extend(part.pitch_classes.get(track).into_iter().flatten().copied());
            result.intervals[track]
                .extend(part.intervals.get(track).into_iter().flatten().copied());
            result.note_counts[track] += part.note_counts.get(track).copied().unwrap_or(0);
        }
    }
    result.empty_tracks = result
        .note_counts
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count == 0).then_some(index))
        .collect();
    result
}

fn nested_similarity<T: PartialEq>(left: &[Vec<T>], right: &[Vec<T>]) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let tracks = left.len().max(right.len());
    let score = (0..tracks)
        .map(|track| match (left.get(track), right.get(track)) {
            (Some(a), Some(b)) if a.is_empty() && b.is_empty() => 1.0,
            (Some(a), Some(b)) => {
                let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
                f64::from(u32::try_from(matches).unwrap_or(u32::MAX))
                    / f64::from(u32::try_from(a.len().max(b.len())).unwrap_or(u32::MAX))
            }
            _ => 0.0,
        })
        .sum::<f64>();
    score / f64::from(u32::try_from(tracks).unwrap_or(u32::MAX))
}

fn count_similarity(left: &[usize], right: &[usize]) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let tracks = left.len().max(right.len());
    let similarity = (0..tracks)
        .map(|track| match (left.get(track), right.get(track)) {
            (Some(a), Some(b)) => {
                let maximum = (*a).max(*b);
                if maximum == 0 {
                    1.0
                } else {
                    1.0 - f64::from(u32::try_from(a.abs_diff(*b)).unwrap_or(u32::MAX))
                        / f64::from(u32::try_from(maximum).unwrap_or(u32::MAX))
                }
            }
            _ => 0.0,
        })
        .sum::<f64>();
    similarity / f64::from(u32::try_from(tracks).unwrap_or(u32::MAX))
}
