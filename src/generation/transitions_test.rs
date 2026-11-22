use super::*;

use crate::fixtures;
use crate::model::{Beat, NoteValue, Rhythm, Voice};

fn add_empty_drum_track(doc: &mut Document) -> usize {
    let track_index = crate::edits::create_track(
        doc,
        crate::model::Track {
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
            generator_metadata: None,
        },
    );
    let first_id = doc.bars.iter().map(|bar| bar.id).max().unwrap_or(0) + 1;
    for (bar_index, master) in doc.master_bars.iter_mut().enumerate() {
        let id = first_id + u32::try_from(bar_index).unwrap();
        master.bar_ids[track_index] = i32::try_from(id).unwrap();
        doc.bars.push(crate::model::Bar {
            id,
            clef: Some("Neutral".into()),
            voices: vec![Voice {
                id,
                beats: (0..16)
                    .map(|beat_id| Beat {
                        id: beat_id,
                        rhythm: Rhythm::new(NoteValue::Sixteenth),
                        notes: Vec::new(),
                        dynamic: None,
                    })
                    .collect(),
            }],
        });
    }
    track_index
}

#[test]
fn transition_planner_targets_the_bar_before_each_section_marker() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[1].section = Some("Chorus".into());
    doc.master_bars[2].section = Some("Bridge".into());
    let rules = [TransitionRule::new(TransitionKind::PickSlide, 0)];
    assert_eq!(
        plan_transitions(&doc, &rules),
        [
            SectionTransition {
                from_bar: 0,
                to_bar: 1,
                rule_index: 0
            },
            SectionTransition {
                from_bar: 1,
                to_bar: 2,
                rule_index: 0
            },
        ]
    );
}

#[test]
fn drum_fill_writes_articulated_hits_into_empty_tail_beats() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[1].section = Some("Chorus".into());
    let track = add_empty_drum_track(&mut doc);
    let before = doc.note_count();
    let rules = [TransitionRule::new(TransitionKind::DrumFill, track)];
    let transitions = apply_transitions(&mut doc, &rules).unwrap();
    assert_eq!(transitions.len(), 1);
    let drum_id = u32::try_from(doc.master_bars[0].bar_ids[track]).unwrap();
    let drum_bar = doc.bars.iter().find(|bar| bar.id == drum_id).unwrap();
    let hits = drum_bar.voices[0]
        .beats
        .iter()
        .flat_map(|beat| &beat.notes)
        .collect::<Vec<_>>();
    assert_eq!(
        hits.len(),
        4,
        "the generated fill must survive as four drum notes"
    );
    assert!(hits.iter().all(|note| note.articulation.is_some()));
    assert_eq!(doc.note_count(), before + 4);
}

#[test]
fn slide_and_cutoff_preserve_pitches_and_add_the_requested_articulations() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[1].section = Some("Chorus".into());
    let before = doc.note_count();
    let rules = [
        TransitionRule::new(TransitionKind::PickSlide, 0),
        TransitionRule::new(TransitionKind::Cutoff, 0),
    ];
    apply_transitions(&mut doc, &rules).unwrap();
    assert_eq!(doc.note_count(), before);
    assert!(doc.technique_count(|technique| matches!(technique, Technique::Slide { .. })) > 0);
    assert!(doc.technique_count(|technique| *technique == Technique::Staccato) > 0);
    apply_transitions(&mut doc, &rules).unwrap();
    assert_eq!(
        doc.technique_count(|technique| matches!(technique, Technique::Slide { .. })),
        1,
        "reapplying a slide rule must not duplicate the technique"
    );
}

#[test]
fn riser_and_bass_run_fill_empty_beats_with_different_pitch_steps() {
    let mut riser_doc = fixtures::repeated_frets();
    riser_doc.master_bars[1].section = Some("Chorus".into());
    let riser_track = add_empty_drum_track(&mut riser_doc);
    riser_doc.tracks[riser_track].tuning = crate::fixtures::TUNING_E_STANDARD.to_vec();
    let riser = [TransitionRule::new(TransitionKind::Riser, riser_track)];
    let before_riser = riser_doc.note_count();
    apply_transitions(&mut riser_doc, &riser).unwrap();
    assert_eq!(riser_doc.note_count(), before_riser + 4);
    let riser_id = u32::try_from(riser_doc.master_bars[0].bar_ids[riser_track]).unwrap();
    let riser_pitches = riser_doc
        .bars
        .iter()
        .find(|bar| bar.id == riser_id)
        .unwrap()
        .voices[0]
        .beats
        .iter()
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| note.midi)
        .collect::<Vec<_>>();
    assert_eq!(riser_pitches, [49, 50, 51, 52]);

    let mut bass_doc = fixtures::repeated_frets();
    bass_doc.master_bars[1].section = Some("Chorus".into());
    let bass_track = add_empty_drum_track(&mut bass_doc);
    bass_doc.tracks[bass_track].tuning = crate::fixtures::TUNING_BASS.to_vec();
    let run = [TransitionRule::new(TransitionKind::BassRun, bass_track)];
    let before_run = bass_doc.note_count();
    apply_transitions(&mut bass_doc, &run).unwrap();
    assert_eq!(bass_doc.note_count(), before_run + 4);
    let bass_id = u32::try_from(bass_doc.master_bars[0].bar_ids[bass_track]).unwrap();
    let run_pitches = bass_doc
        .bars
        .iter()
        .find(|bar| bar.id == bass_id)
        .unwrap()
        .voices[0]
        .beats
        .iter()
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| note.midi)
        .collect::<Vec<_>>();
    assert_eq!(run_pitches, [50, 52, 54, 56]);
}

#[test]
fn transition_application_rejects_invalid_rules_before_mutating() {
    let mut doc = fixtures::repeated_frets();
    let before = doc.clone();
    let rule = TransitionRule::new(TransitionKind::DrumFill, usize::MAX);
    assert!(apply_transitions(&mut doc, &[rule]).is_err());
    assert_eq!(doc, before);
}

#[test]
fn a_later_broken_section_reference_rolls_back_earlier_transition_edits() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[1].section = Some("Chorus".into());
    doc.master_bars[2].section = Some("Bridge".into());
    doc.master_bars[1].bar_ids[0] = -1;
    let before = doc.clone();
    let rule = TransitionRule::new(TransitionKind::PickSlide, 0);
    assert!(apply_transitions(&mut doc, &[rule]).is_err());
    assert_eq!(
        doc, before,
        "failed edits must not leave a partial transition"
    );
}

#[test]
fn stop_silences_exactly_the_requested_tail_without_changing_bar_length() {
    let mut doc = fixtures::repeated_frets();
    doc.master_bars[1].section = Some("Drop".into());
    let before = doc.bars[0].voices[0].beats.len();
    let mut rule = TransitionRule::new(TransitionKind::Stop, 0);
    rule.steps = 3;
    apply_transitions(&mut doc, &[rule]).unwrap();
    let beats = &doc.bars[0].voices[0].beats;
    assert_eq!(beats.len(), before);
    assert!(beats[..before - 3]
        .iter()
        .all(|beat| !beat.notes.is_empty()));
    assert!(beats[before - 3..].iter().all(|beat| beat.notes.is_empty()));
}

#[test]
fn entry_silences_other_tracks_for_the_requested_bars_and_sets_pan() {
    let mut doc = fixtures::repeated_frets();
    let second = crate::edits::clone_track(&mut doc, 0).unwrap();
    doc.master_bars[1].section = Some("Entry".into());
    let mut rule = TransitionRule::new(TransitionKind::Entry, second);
    rule.entry_bars = 2;
    rule.entry_pan = Some(35);
    apply_transitions(&mut doc, &[rule]).unwrap();
    assert_eq!(doc.tracks[second].pan, Some(0.35));
    for bar_index in 1..3 {
        let original = u32::try_from(doc.master_bars[bar_index].bar_ids[0]).unwrap();
        let bar = doc.bars.iter().find(|bar| bar.id == original).unwrap();
        assert!(bar
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .all(|beat| beat.notes.is_empty()));
    }
}
