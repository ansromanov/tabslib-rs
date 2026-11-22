use super::*;

fn config(role: GuitarRole, tuning: Vec<i32>) -> SecondGuitarConfig {
    SecondGuitarConfig {
        role,
        tuning,
        ..SecondGuitarConfig::contextual(0, 17)
    }
}

fn bars(document: &Document, track: usize) -> Vec<Bar> {
    document
        .master_bars
        .iter()
        .filter_map(|master| {
            let id = u32::try_from(*master.bar_ids.get(track)?).ok()?;
            document.bars.iter().find(|bar| bar.id == id).cloned()
        })
        .collect()
}

#[test]
fn double_and_octave_keep_every_source_attack_while_support_thins_them() {
    let source = crate::fixtures::power_chords();
    for role in [GuitarRole::Double, GuitarRole::Octave] {
        let mut document = source.clone();
        let result = generate_second_guitar(
            &mut document,
            &config(role, source.tracks[0].tuning.clone()),
            false,
        )
        .expect("role can be played");
        let output = bars(&document, result.track_index);
        let original = bars(&source, 0);
        assert_eq!(output.len(), original.len());
        for (before, after) in original.iter().zip(&output) {
            assert_eq!(
                bar_signature(before)
                    .iter()
                    .map(|item| &item.0)
                    .collect::<Vec<_>>(),
                bar_signature(after)
                    .iter()
                    .map(|item| &item.0)
                    .collect::<Vec<_>>(),
                "double and octave retain every source rhythm"
            );
            assert_ne!(
                bar_signature(before),
                bar_signature(after),
                "no bar is copied verbatim"
            );
        }
        assert!(requirements::check(&document).iter().all(|violation| {
            violation.requirement != Requirement::TrackDuplication
                || violation.region.track != Some(result.track_index)
        }));
    }

    let mut support = source.clone();
    let generated = generate_second_guitar(
        &mut support,
        &config(GuitarRole::Support, source.tracks[0].tuning.clone()),
        false,
    )
    .expect("support role can be played");
    assert!(generated.note_count < source.note_count());
    for (before, after) in bars(&source, 0)
        .iter()
        .zip(bars(&support, generated.track_index))
    {
        assert_eq!(
            before.voices[0]
                .beats
                .iter()
                .map(|beat| beat.rhythm)
                .collect::<Vec<_>>(),
            after.voices[0]
                .beats
                .iter()
                .map(|beat| beat.rhythm)
                .collect::<Vec<_>>(),
            "support removes attacks but keeps the timing grid"
        );
    }
}

#[test]
fn contextual_role_responds_to_density_by_marked_section() {
    let mut source = crate::fixtures::power_chords();
    source.master_bars[2].section = Some("Sparse section".into());
    for bar_index in 2..source.master_bars.len() {
        let id = source.master_bars[bar_index].bar_ids[0] as u32;
        let bar = source.bars.iter_mut().find(|bar| bar.id == id).unwrap();
        for (beat_index, beat) in bar.voices[0].beats.iter_mut().enumerate() {
            if beat_index != 0 {
                beat.notes.clear();
            } else {
                beat.notes.truncate(1);
            }
        }
    }
    let mut document = source;
    let mut contextual = SecondGuitarConfig::contextual(0, 12);
    contextual.tuning = document.tracks[0].tuning.clone();
    let result = generate_second_guitar(&mut document, &contextual, false)
        .expect("contextual roles can be played");
    assert_eq!(result.sections.len(), 2);
    assert_eq!(result.sections[0].role, GuitarRole::Support);
    assert_eq!(result.sections[1].role, GuitarRole::Harmonise);
    assert_eq!(result.sections[0].start_bar, 0);
    assert_eq!(result.sections[0].end_bar, 1);
    assert_eq!(result.sections[1].start_bar, 2);
}

#[test]
fn h6_refuses_a_third_fretted_track_unless_force_is_explicit() {
    let mut source = crate::fixtures::power_chords();
    edits::clone_track(&mut source, 0).unwrap();
    assert_eq!(fretted_track_count(&source), 2);
    let before = source.clone();
    let double_config = config(GuitarRole::Double, source.tracks[0].tuning.clone());
    assert_eq!(
        generate_second_guitar(&mut source, &double_config, false),
        Err(SecondGuitarError::TrackLimit { count: 2 })
    );
    assert_eq!(
        source, before,
        "a rejected operation leaves the score unchanged"
    );

    let result = generate_second_guitar(&mut source, &double_config, true)
        .expect("force explicitly bypasses H6");
    assert_eq!(fretted_track_count(&source), 3);
    assert!(result.track_index >= 2);
}

#[cfg(all(feature = "drums", feature = "rhythm"))]
#[test]
fn accompaniment_workflow_can_request_an_explicit_second_guitar_role() {
    let mut document = crate::fixtures::power_chords();
    let before_tracks = document.tracks.len();
    let source_tracks = [0];
    let mut request = super::super::accompaniment::AccompanimentRequest::new(&source_tracks, 5);
    request.second_guitar = Some(config(
        GuitarRole::Octave,
        document.tracks[0].tuning.clone(),
    ));
    let result = super::super::accompaniment::generate_accompaniment(&mut document, &request)
        .expect("second-guitar request runs in the accompaniment workflow");
    let role = result
        .roles
        .iter()
        .find(|entry| entry.role == super::super::accompaniment::AccompanimentRole::SecondGuitar)
        .unwrap();
    assert!(matches!(
        role.outcome,
        super::super::accompaniment::AccompanimentOutcome::Generated { .. }
    ));
    assert_eq!(document.tracks.len(), before_tracks + 1);
}

#[test]
fn phrase_tail_role_keeps_low_part_in_unison_and_harmonises_eligible_tail() {
    let mut source = crate::fixtures::repeated_frets();
    for note in source.bars[0].voices[0].beats[6..]
        .iter_mut()
        .flat_map(|beat| &mut beat.notes)
    {
        note.string = Some(1);
        note.fret = Some(19);
        note.midi = Some(64);
        note.techniques
            .retain(|technique| *technique != crate::model::Technique::PalmMute);
    }
    let mut config = config(GuitarRole::PhraseTail, source.tracks[0].tuning.clone());
    config.key = Some(crate::model::KeySignature {
        tonic: 4,
        minor: true,
    });
    config.phrase_tail_min_string = 1;
    config.phrase_tail_min_pitch = 60;
    config.phrase_tail_beats = 2;
    let result = generate_second_guitar(&mut source, &config, false).unwrap();
    let generated_id = u32::try_from(source.master_bars[0].bar_ids[result.track_index]).unwrap();
    let generated = source
        .bars
        .iter()
        .find(|bar| bar.id == generated_id)
        .unwrap();
    let original = &source.bars[0];
    for (index, (source_beat, generated_beat)) in original.voices[0]
        .beats
        .iter()
        .zip(&generated.voices[0].beats)
        .enumerate()
    {
        let source_pitch = source_beat.notes[0].midi.unwrap();
        let generated_pitch = generated_beat.notes[0].midi.unwrap();
        if index < 6 {
            assert_eq!(generated_pitch, source_pitch, "low figure is unison");
        } else {
            assert_eq!(
                generated_pitch - source_pitch,
                3,
                "tail is a diatonic third"
            );
            assert!(!generated_beat.notes[0]
                .techniques
                .contains(&crate::model::Technique::PalmMute));
        }
    }
}
