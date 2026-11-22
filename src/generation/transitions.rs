//! Section-join transition planning and deterministic document edits.
//!
//! The module owns [`TransitionRule`], [`plan_transitions`], and
//! [`apply_transitions`]. Rules place fills into existing empty subdivisions,
//! attach slide articulation to a written note, or shorten the final attack.

use serde::{Deserialize, Serialize};

use crate::model::{Document, Note, Technique};

use super::arrangement::ArrangementError;

/// A transition treatment applied immediately before the next section marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// Add a short tom pattern to a percussion track.
    DrumFill,
    /// Mark the final pitched note with a slide articulation.
    PickSlide,
    /// Add a short ascending pitched run.
    Riser,
    /// Add a short ascending bass run.
    BassRun,
    /// Staccato the final sounding beat.
    Cutoff,
    /// Silence selected tracks immediately before the boundary.
    Stop,
    /// Let one track enter alone after the boundary.
    Entry,
}

/// Editable transition instruction stored in an [`ArrangementPlan`](super::arrangement::ArrangementPlan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRule {
    /// Transition treatment.
    pub kind: TransitionKind,
    /// Track to edit; for cutoffs, the rule is applied to this track only.
    pub track_index: usize,
    /// Number of trailing empty subdivisions to use, capped by the bar length.
    pub steps: usize,
    /// Slide flag bits used by a pick-slide treatment.
    pub slide_flags: u32,
    /// Tracks affected by a stop; `None` means every track.
    #[serde(default)]
    pub stop_tracks: Option<Vec<usize>>,
    /// Number of bars in a single-part entry.
    #[serde(default = "default_entry_bars")]
    pub entry_bars: usize,
    /// Optional normalized pan percentage assigned to the entering track.
    #[serde(default)]
    pub entry_pan: Option<i8>,
}

fn default_entry_bars() -> usize {
    1
}

impl TransitionRule {
    /// Creates a rule with a four-step tail and default slide flags.
    pub fn new(kind: TransitionKind, track_index: usize) -> Self {
        Self {
            kind,
            track_index,
            steps: 4,
            slide_flags: 1,
            stop_tracks: None,
            entry_bars: 1,
            entry_pan: None,
        }
    }
}

/// One resolved transition location in a composed document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionTransition {
    /// Last bar before the section boundary.
    pub from_bar: usize,
    /// Bar that starts the next section.
    pub to_bar: usize,
    /// Rule applied at this boundary.
    pub rule_index: usize,
}

/// Resolves rules to each section boundary after the first section.
pub fn plan_transitions(doc: &Document, rules: &[TransitionRule]) -> Vec<SectionTransition> {
    let boundaries = doc
        .master_bars
        .iter()
        .enumerate()
        .filter_map(|(index, bar)| (index > 0 && bar.section.is_some()).then_some(index))
        .collect::<Vec<_>>();
    boundaries
        .into_iter()
        .flat_map(|to_bar| {
            rules
                .iter()
                .enumerate()
                .map(move |(rule_index, _)| SectionTransition {
                    from_bar: to_bar - 1,
                    to_bar,
                    rule_index,
                })
        })
        .collect()
}

/// Applies each transition rule at all section boundaries in the document.
///
/// Fills and runs only occupy empty beats, preserving every written duration.
/// Invalid track or bar references return an error without changing the input.
pub fn apply_transitions(
    doc: &mut Document,
    rules: &[TransitionRule],
) -> Result<Vec<SectionTransition>, ArrangementError> {
    for rule in rules {
        if rule.track_index >= doc.tracks.len() {
            return Err(ArrangementError::InvalidInput(format!(
                "transition track index {}",
                rule.track_index
            )));
        }
        if let Some(tracks) = &rule.stop_tracks {
            if let Some(track) = tracks.iter().find(|track| **track >= doc.tracks.len()) {
                return Err(ArrangementError::InvalidInput(format!(
                    "stop transition track index {track}"
                )));
            }
        }
        if rule.steps == 0 {
            return Err(ArrangementError::InvalidInput(
                "transition step count must be positive".into(),
            ));
        }
        if rule.kind == TransitionKind::Entry && rule.entry_bars == 0 {
            return Err(ArrangementError::InvalidInput(
                "entry transition bar count must be positive".into(),
            ));
        }
        if rule.kind == TransitionKind::DrumFill && !doc.tracks[rule.track_index].tuning.is_empty()
        {
            return Err(ArrangementError::InvalidInput(
                "drum-fill rules require a percussion track".into(),
            ));
        }
    }
    let transitions = plan_transitions(doc, rules);
    let mut edited = doc.clone();
    let mut next_note_id = edited
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| note.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for transition in &transitions {
        let Some(rule) = rules.get(transition.rule_index) else {
            continue;
        };
        match rule.kind {
            TransitionKind::Stop => {
                apply_stop(&mut edited, transition.from_bar, rule)?;
                continue;
            }
            TransitionKind::Entry => {
                apply_entry(&mut edited, transition.to_bar, rule)?;
                continue;
            }
            _ => {}
        }
        let Some(master) = edited.master_bars.get(transition.from_bar) else {
            return Err(ArrangementError::InvalidInput(format!(
                "transition source bar {}",
                transition.from_bar
            )));
        };
        let bar_ref = master
            .bar_ids
            .get(rule.track_index)
            .copied()
            .ok_or_else(|| {
                ArrangementError::InvalidInput("transition track has no bar reference".into())
            })?;
        let bar_id = u32::try_from(bar_ref).map_err(|_| {
            ArrangementError::InvalidInput("transition track is missing its source bar".into())
        })?;
        let bar = edited
            .bars
            .iter_mut()
            .find(|bar| bar.id == bar_id)
            .ok_or_else(|| ArrangementError::InvalidInput(format!("transition bar id {bar_id}")))?;
        let voice = bar
            .voices
            .first_mut()
            .ok_or_else(|| ArrangementError::InvalidInput("transition bar has no voice".into()))?;
        match rule.kind {
            TransitionKind::DrumFill => {
                let toms = [48, 47, 45, 41];
                let tail = voice.beats.len().saturating_sub(rule.steps);
                for (offset, beat) in voice.beats.iter_mut().enumerate().skip(tail) {
                    if beat.notes.is_empty() {
                        let midi = toms[(offset - tail) % toms.len()];
                        beat.notes.push(Note {
                            id: next_note_id,
                            midi: Some(midi),
                            string: None,
                            fret: None,
                            articulation: Some(midi),
                            techniques: Vec::new(),
                        });
                        next_note_id = next_note_id.saturating_add(1);
                    }
                }
            }
            TransitionKind::PickSlide => {
                if let Some(note) = voice
                    .beats
                    .iter_mut()
                    .rev()
                    .flat_map(|beat| &mut beat.notes)
                    .next()
                {
                    let slide = Technique::Slide {
                        flags: rule.slide_flags,
                    };
                    if !note.techniques.contains(&slide) {
                        note.techniques.push(slide);
                    }
                }
            }
            TransitionKind::Riser | TransitionKind::BassRun => {
                let tail = voice.beats.len().saturating_sub(rule.steps);
                let base = voice
                    .beats
                    .iter()
                    .rev()
                    .flat_map(|beat| &beat.notes)
                    .find_map(|note| note.midi)
                    .unwrap_or(48);
                let tuning = &edited.tracks[rule.track_index].tuning;
                for (offset, beat) in voice.beats.iter_mut().enumerate().skip(tail) {
                    if !beat.notes.is_empty() {
                        continue;
                    }
                    let interval = match rule.kind {
                        TransitionKind::Riser => i32::try_from(offset - tail + 1).unwrap_or(1),
                        TransitionKind::BassRun => {
                            i32::try_from((offset - tail + 1) * 2).unwrap_or(2)
                        }
                        _ => 1,
                    };
                    let midi = base.saturating_add(interval);
                    let position = tuning
                        .iter()
                        .enumerate()
                        .filter_map(|(string, open)| {
                            let fret = midi - *open;
                            (0..=24).contains(&fret).then_some((string, fret))
                        })
                        .min_by_key(|(_, fret)| *fret);
                    let (string, fret) = match position {
                        Some((string, fret)) => (u32::try_from(string).unwrap_or(0), fret),
                        None if tuning.is_empty() => (0, midi),
                        None => continue,
                    };
                    beat.notes.push(Note {
                        id: next_note_id,
                        midi: Some(midi),
                        string: (!tuning.is_empty()).then_some(string),
                        fret: (!tuning.is_empty()).then_some(fret),
                        articulation: None,
                        techniques: Vec::new(),
                    });
                    next_note_id = next_note_id.saturating_add(1);
                }
            }
            TransitionKind::Cutoff => {
                if let Some(beat) = voice.beats.last_mut() {
                    for note in &mut beat.notes {
                        if !note.techniques.contains(&Technique::Staccato) {
                            note.techniques.push(Technique::Staccato);
                        }
                    }
                }
            }
            TransitionKind::Stop | TransitionKind::Entry => {
                unreachable!("stop and entry transitions are applied before track-local edits")
            }
        }
    }
    *doc = edited;
    Ok(transitions)
}

fn apply_stop(
    doc: &mut Document,
    bar_index: usize,
    rule: &TransitionRule,
) -> Result<(), ArrangementError> {
    let all_tracks = (0..doc.tracks.len()).collect::<Vec<_>>();
    let tracks = rule.stop_tracks.as_deref().unwrap_or(&all_tracks);
    for track_index in tracks {
        let master = doc
            .master_bars
            .get(bar_index)
            .ok_or_else(|| ArrangementError::InvalidInput("stop source bar".into()))?;
        let bar_id = master
            .bar_ids
            .get(*track_index)
            .and_then(|id| u32::try_from(*id).ok())
            .ok_or_else(|| ArrangementError::InvalidInput("stop track has no bar".into()))?;
        let bar = doc
            .bars
            .iter_mut()
            .find(|bar| bar.id == bar_id)
            .ok_or_else(|| ArrangementError::InvalidInput("stop bar is missing".into()))?;
        for voice in &mut bar.voices {
            let start = voice.beats.len().saturating_sub(rule.steps);
            for beat in voice.beats.iter_mut().skip(start) {
                beat.notes.clear();
            }
        }
    }
    Ok(())
}

fn apply_entry(
    doc: &mut Document,
    start_bar: usize,
    rule: &TransitionRule,
) -> Result<(), ArrangementError> {
    if let Some(pan) = rule.entry_pan {
        doc.tracks[rule.track_index].pan = Some(f64::from(pan) / 100.0);
    }
    let end_bar = start_bar
        .saturating_add(rule.entry_bars)
        .min(doc.master_bars.len());
    for bar_index in start_bar..end_bar {
        for track_index in 0..doc.tracks.len() {
            if track_index == rule.track_index {
                continue;
            }
            let bar_id = doc
                .master_bars
                .get(bar_index)
                .and_then(|master| master.bar_ids.get(track_index))
                .and_then(|id| u32::try_from(*id).ok())
                .ok_or_else(|| ArrangementError::InvalidInput("entry track has no bar".into()))?;
            let bar = doc
                .bars
                .iter_mut()
                .find(|bar| bar.id == bar_id)
                .ok_or_else(|| ArrangementError::InvalidInput("entry bar is missing".into()))?;
            for voice in &mut bar.voices {
                for beat in &mut voice.beats {
                    beat.notes.clear();
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "transitions_test.rs"]
mod tests;
