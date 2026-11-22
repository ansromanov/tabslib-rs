use super::*;
use crate::inspect::Fraction;

fn request(document: &Document) -> RiffAdaptationRequest {
    let source = determine_key(document).expect("fixture has a key").key;
    RiffAdaptationRequest {
        track: 0,
        start_bar: 0,
        end_bar: document.master_bars.len() - 1,
        target_key: KeySignature {
            tonic: (source.tonic + 5).rem_euclid(12),
            minor: source.minor,
        },
        target_tempo: None,
        target_meter: None,
        fret_window: (0, 24),
    }
}

fn notes_on_track(document: &Document, track: usize) -> Vec<(i32, u32, i32)> {
    document
        .master_bars
        .iter()
        .filter_map(|master| master.bar_ids.get(track))
        .filter_map(|id| u32::try_from(*id).ok())
        .filter_map(|id| document.bars.iter().find(|bar| bar.id == id))
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| Some((note.midi?, note.string?, note.fret?)))
        .collect()
}

fn onset_positions(document: &Document) -> Vec<Vec<(u64, u64)>> {
    document
        .master_bars
        .iter()
        .map(|master| {
            let mut positions = Vec::new();
            let id = master.bar_ids[0];
            let Some(bar) = u32::try_from(id)
                .ok()
                .and_then(|id| document.bars.iter().find(|bar| bar.id == id))
            else {
                return positions;
            };
            for voice in &bar.voices {
                let mut onset = Fraction::new(0, 1);
                for beat in &voice.beats {
                    if !beat.notes.is_empty() {
                        positions.push((onset.numerator, onset.denominator));
                    }
                    let (numerator, denominator) = beat.rhythm.as_fraction();
                    onset = onset.plus(Fraction::new(numerator, denominator));
                }
            }
            positions
        })
        .collect()
}

#[test]
fn applying_key_adaptation_moves_pitch_and_fingering_together() {
    let mut document = crate::fixtures::power_chords();
    let before = notes_on_track(&document, 0);
    let plan = plan_riff_adaptation(&document, request(&document)).expect("adaptation plan");
    assert_eq!(
        (plan.key.target.tonic - plan.key.source.tonic).rem_euclid(12),
        5,
        "the proposed transposition must land every pitch in the target tonic"
    );
    assert_eq!(
        notes_on_track(&document, 0),
        before,
        "planning never edits the score"
    );

    apply_riff_adaptation(&mut document, &plan).expect("plan applies");
    let after = notes_on_track(&document, 0);
    assert_eq!(after.len(), before.len());
    for ((old_pitch, _, _), (new_pitch, string, fret)) in before.iter().zip(&after) {
        assert_eq!(*new_pitch - *old_pitch, plan.key.semitones);
        assert_eq!(
            crate::model::open_pitch(&document.tracks[0].tuning, *string),
            Some(*new_pitch - *fret),
            "the changed string/fret pair still sounds the changed MIDI pitch"
        );
        assert!((plan.fret_window.0..=plan.fret_window.1).contains(fret));
    }
    assert_eq!(document.key, Some(plan.key.target));
}

#[test]
fn meter_and_tempo_adaptation_preserve_scaled_onsets_and_restore_tempo() {
    let mut document = crate::fixtures::power_chords();
    document.tempo_map = vec![
        TempoChange {
            at_bar: 0,
            bpm: 120.0,
            linear: false,
        },
        TempoChange {
            at_bar: 4,
            bpm: 90.0,
            linear: false,
        },
    ];
    let before_onsets = onset_positions(&document);
    let before_count = document.note_count();
    let mut input = request(&document);
    input.end_bar = 2;
    input.target_meter = Some((7, 8));
    input.target_tempo = Some(150.0);
    let plan = plan_riff_adaptation(&document, input).expect("adaptation plan");
    assert_eq!(
        plan.tempo,
        Some(TempoAdaptation {
            source_bpm: 120.0,
            target_bpm: 150.0
        })
    );
    assert_eq!(
        plan.meter.as_ref().map(|meter| meter.target_meter),
        Some((7, 8))
    );

    apply_riff_adaptation(&mut document, &plan).expect("plan applies");
    assert_eq!(document.note_count(), before_count, "no attack is dropped");
    assert!(document.master_bars[..=2]
        .iter()
        .all(|master| master.time == (7, 8)));
    assert!(
        bar_integrity(&document)
            .iter()
            .all(|entry| !(plan.start_bar..=plan.end_bar).contains(&entry.bar_index)),
        "every adapted voice still fills its new meter"
    );
    for (bar_index, onsets) in before_onsets[..=2].iter().enumerate() {
        let after = &onset_positions(&document)[bar_index];
        assert_eq!(after.len(), onsets.len());
        for ((old_num, old_den), (new_num, new_den)) in onsets.iter().zip(after) {
            assert_eq!(
                new_num * old_den * 8,
                old_num * new_den * 7,
                "each attack moves by the exact 7/8 meter ratio"
            );
        }
    }
    assert_eq!(tempo_at(&document, 2), Some(150.0));
    assert_eq!(tempo_at(&document, 3), Some(120.0));
    assert_eq!(tempo_at(&document, 4), Some(90.0));
}

#[test]
fn plan_rejects_unplayable_windows_invalid_meters_and_missing_keys() {
    let document = crate::fixtures::power_chords();
    let mut input = request(&document);
    input.fret_window = (200, 210);
    assert!(matches!(
        plan_riff_adaptation(&document, input),
        Err(RiffAdaptationError::NoPlayableTransposition { .. })
    ));

    let mut input = request(&document);
    input.target_meter = Some((7, 3));
    assert!(matches!(
        plan_riff_adaptation(&document, input),
        Err(RiffAdaptationError::InvalidMeter { .. })
    ));

    let mut percussion = crate::fixtures::drum_patterns();
    let mut input = request(&document);
    input.target_meter = None;
    input.target_tempo = None;
    assert_eq!(
        plan_riff_adaptation(&percussion, input),
        Err(RiffAdaptationError::InvalidTrack(0))
    );
    percussion.tracks[0].tuning = vec![40, 45, 50, 55, 59, 64];
    for note in percussion
        .bars
        .iter_mut()
        .flat_map(|bar| &mut bar.voices)
        .flat_map(|voice| &mut voice.beats)
        .flat_map(|beat| &mut beat.notes)
    {
        note.midi = None;
    }
    assert_eq!(
        plan_riff_adaptation(&percussion, input),
        Err(RiffAdaptationError::NoKey)
    );
}

#[test]
fn meter_change_rejects_an_incomplete_source_bar_without_dropping_attacks() {
    let mut document = crate::fixtures::power_chords();
    let mut input = request(&document);
    input.target_meter = Some((7, 8));
    let id = document.master_bars[0].bar_ids[0] as u32;
    let bar = document.bars.iter_mut().find(|bar| bar.id == id).unwrap();
    bar.voices[0].beats.pop();
    let before = document.clone();

    assert_eq!(
        plan_riff_adaptation(&document, input),
        Err(RiffAdaptationError::MeterNotRepresentable)
    );
    assert_eq!(
        document, before,
        "planning never edits or truncates the score"
    );
}

#[test]
fn tempo_adaptation_requires_a_written_source_tempo_and_apply_rejects_stale_plans() {
    let mut document = crate::fixtures::power_chords();
    let mut input = request(&document);
    input.target_tempo = Some(150.0);
    assert_eq!(
        plan_riff_adaptation(&document, input),
        Err(RiffAdaptationError::MissingTempo)
    );

    document.tempo_map.push(TempoChange {
        at_bar: 0,
        bpm: 120.0,
        linear: false,
    });
    let plan = plan_riff_adaptation(&document, input).expect("known source tempo");
    document.tempo_map[0].bpm = 130.0;
    let before = document.clone();
    assert_eq!(
        apply_riff_adaptation(&mut document, &plan),
        Err(RiffAdaptationError::StalePlan)
    );
    assert_eq!(
        document, before,
        "a stale report cannot partially edit the score"
    );
}

#[test]
fn rhythm_scaling_refuses_tuplet_overflow_instead_of_losing_an_onset() {
    let rhythm = Rhythm {
        value: crate::model::NoteValue::Quarter,
        dots: 0,
        tuplet: Some((1, u32::MAX)),
    };
    assert!(scale_rhythm(rhythm, (64, 1)).is_none());
}
