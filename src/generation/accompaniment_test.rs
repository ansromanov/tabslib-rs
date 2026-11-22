use super::*;

#[cfg(all(
    feature = "arrangement",
    feature = "drums",
    feature = "rhythm",
    feature = "harmony"
))]
#[test]
fn full_song_layers_every_role_over_a_plan_span() {
    let mut document = crate::fixtures::power_chords();
    let plan = super::super::arrangement::ArrangementPlan::new(
        9,
        vec![super::super::arrangement::SectionRequest {
            label: "Verse".into(),
            bars: document.master_bars.len(),
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let result = generate_full_song(&mut document, &plan, &[0], 9).unwrap();
    assert_eq!(result.roles.len(), 5);
    assert!(result.roles.iter().all(|role| matches!(
        role.outcome,
        AccompanimentOutcome::Generated { note_count, .. } if note_count > 0
    )));
}

#[cfg(all(
    feature = "arrangement",
    feature = "drums",
    feature = "rhythm",
    feature = "harmony"
))]
#[test]
fn full_song_succeeds_with_csharp_standard_and_one_four_bar() {
    let mut document = crate::fixtures::power_chords();
    document.tracks[0].tuning = vec![37, 42, 47, 52, 56, 61];
    document.master_bars[1].time = (1, 4);
    let bar_id = document.master_bars[1].bar_ids[0] as u32;
    let bar = document.bars.iter_mut().find(|b| b.id == bar_id).unwrap();
    for voice in &mut bar.voices {
        voice.beats.truncate(1);
    }
    let plan = super::super::arrangement::ArrangementPlan::new(
        9,
        vec![super::super::arrangement::SectionRequest {
            label: "Verse".into(),
            bars: document.master_bars.len(),
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let result = generate_full_song(&mut document, &plan, &[0], 9).unwrap();
    assert_eq!(result.roles.len(), 5);
    let bass_track = document
        .tracks
        .iter()
        .find(|t| t.name.to_ascii_lowercase().contains("bass"))
        .expect("generated bass track");
    assert_eq!(bass_track.tuning, vec![25, 30, 35, 40]);
}

#[cfg(all(feature = "drums", feature = "rhythm", feature = "harmony"))]
#[test]
fn generation_is_repeatable_and_preserves_authored_tracks() {
    let source_tracks = [0];
    let mut request = AccompanimentRequest::new(&source_tracks, 73);
    request.layers = AccompanimentLayers {
        drums: true,
        bass: true,
        harmony: true,
        lead: true,
    };
    let mut second_guitar = super::super::second_guitar::SecondGuitarConfig::contextual(0, 73);
    second_guitar.role = super::super::second_guitar::GuitarRole::Octave;
    second_guitar.tuning = crate::fixtures::TUNING_DROP_D.to_vec();
    request.second_guitar = Some(second_guitar);
    request.force = true;
    request.drum_profile = Some(super::super::drums::performance::DrumProfile {
        kick_voice: 35,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 100,
        snare_probability: 100,
        cymbal_probability: 100,
    });
    let mut first = crate::fixtures::power_chords();
    let original = first.clone();
    let mut second = original.clone();

    let first_result = generate_accompaniment(&mut first, &request).unwrap();
    let second_result = generate_accompaniment(&mut second, &request).unwrap();

    assert_eq!(
        first, second,
        "shared seed must reproduce all generated note data"
    );
    assert_eq!(first_result, second_result);
    assert_eq!(first.tracks[0], original.tracks[0]);
    assert_eq!(first.bars[..original.bars.len()], original.bars);
    assert_eq!(first.tracks.len(), original.tracks.len() + 5);
    for role in &first_result.roles {
        let AccompanimentOutcome::Generated {
            track_index,
            note_count,
        } = role.outcome
        else {
            panic!("requested role {:?} was skipped", role.role);
        };
        assert!(
            note_count > 0,
            "generated {:?} notes were silently dropped",
            role.role
        );
        assert!(track_index >= original.tracks.len());
    }
}

#[cfg(all(feature = "drums", feature = "rhythm"))]
#[test]
fn section_drum_feels_change_snare_rate_and_name_the_choice() {
    let mut document = crate::fixtures::power_chords();
    let sections = [
        super::DrumSectionRequest {
            start_bar: 0,
            end_bar: 0,
            feel: super::super::drums::presets::SectionDrumFeel::FullBackbeat,
        },
        super::DrumSectionRequest {
            start_bar: 1,
            end_bar: 1,
            feel: super::super::drums::presets::SectionDrumFeel::QuarterBackbeat,
        },
    ];
    let profile = super::super::drums::presets::rock_straight_preset();
    let generated = super::super::workflows::generate_drums_with_sections(
        &mut document,
        &[0],
        &profile,
        41,
        120,
        &sections,
    )
    .unwrap();

    let snares_in_bar = |bar_index: usize| {
        let bar_id =
            u32::try_from(document.master_bars[bar_index].bar_ids[generated.track_index]).unwrap();
        document
            .bars
            .iter()
            .find(|bar| bar.id == bar_id)
            .unwrap()
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
            .filter(|note| note.midi == Some(38))
            .count()
    };

    assert!(
        snares_in_bar(0) > snares_in_bar(1),
        "full and quarter backbeats must produce different snare rates"
    );
    assert!(generated
        .log
        .decisions
        .iter()
        .any(|decision| decision.choice.contains("profile:full_backbeat")));
    assert!(generated
        .log
        .decisions
        .iter()
        .any(|decision| decision.choice.contains("profile:quarter_backbeat")));
}

#[cfg(all(
    feature = "drums",
    feature = "rhythm",
    feature = "harmony",
    feature = "midi"
))]
#[test]
fn midi_library_drums_report_the_selected_groove() {
    let bytes = crate::format::midi::write(&crate::fixtures::drum_patterns());
    let grid = super::super::pattern_catalog::PatternGrid::new((4, 4), 4).unwrap();
    let mut index = super::super::drums::midi_groove::MidiGrooveIndex::new();
    index
        .add_midi("library/straight.mid", "drums", 120, grid, &bytes)
        .unwrap();
    let mut request = AccompanimentRequest::new(&[0], 41);
    request.layers.drums = true;
    request.force = true;
    request.drum_midi_index = Some(index);
    let mut document = crate::fixtures::power_chords();
    let result = generate_accompaniment(&mut document, &request).unwrap();
    let drum_result = result
        .roles
        .iter()
        .find(|role| role.role == AccompanimentRole::Drums)
        .unwrap();
    let track_index = match drum_result.outcome {
        AccompanimentOutcome::Generated { track_index, .. } => track_index,
        AccompanimentOutcome::Skipped(_) => panic!("MIDI drum generation was skipped"),
    };
    assert!(document.tracks[track_index]
        .generator_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.archetype.as_deref() == Some("midi-library")));
    assert!(result
        .roles
        .iter()
        .any(|role| matches!(role.outcome, AccompanimentOutcome::Generated { .. })));
}

#[cfg(all(feature = "drums", feature = "rhythm", feature = "harmony"))]
#[test]
fn authored_roles_are_skipped_until_force_adds_alongside_them() {
    let source_tracks = [0];
    let mut request = AccompanimentRequest::new(&source_tracks, 19);
    request.layers = AccompanimentLayers {
        drums: true,
        bass: true,
        harmony: true,
        lead: true,
    };
    let mut second_guitar = super::super::second_guitar::SecondGuitarConfig::contextual(0, 19);
    second_guitar.role = super::super::second_guitar::GuitarRole::Octave;
    second_guitar.tuning = crate::fixtures::TUNING_DROP_D.to_vec();
    request.second_guitar = Some(second_guitar);
    request.force = true;
    let mut document = crate::fixtures::power_chords();
    generate_accompaniment(&mut document, &request).unwrap();
    let with_authored_roles = document.clone();

    request.force = false;
    let skipped = generate_accompaniment(&mut document, &request).unwrap();
    assert_eq!(document, with_authored_roles);
    assert!(skipped.roles.iter().all(|entry| {
        entry.outcome == AccompanimentOutcome::Skipped(AccompanimentSkipReason::ExistingRole)
    }));

    request.force = true;
    let forced = generate_accompaniment(&mut document, &request).unwrap();
    assert_eq!(document.tracks.len(), with_authored_roles.tracks.len() + 5);
    assert_eq!(
        document.tracks[..with_authored_roles.tracks.len()],
        with_authored_roles.tracks
    );
    assert!(forced
        .roles
        .iter()
        .all(|entry| matches!(entry.outcome, AccompanimentOutcome::Generated { .. })));
}

#[cfg(feature = "harmony")]
#[test]
fn a_later_generation_error_leaves_the_document_unchanged() {
    let source_tracks = [0];
    let mut request = AccompanimentRequest::new(&source_tracks, 31);
    request.layers = AccompanimentLayers {
        harmony: true,
        lead: true,
        ..AccompanimentLayers::default()
    };
    let mut invalid_lead = super::super::solo::SoloConfig::c_major(10, 11, 31);
    invalid_lead.tuning = crate::fixtures::TUNING_E_STANDARD.to_vec();
    request.lead_config = Some(invalid_lead);
    let mut document = crate::fixtures::power_chords();
    let before = document.clone();

    assert!(generate_accompaniment(&mut document, &request).is_err());
    assert_eq!(document, before);
}

#[cfg(not(any(feature = "drums", feature = "rhythm", feature = "harmony")))]
#[test]
fn requested_roles_report_disabled_features() {
    let source_tracks = [];
    let mut request = AccompanimentRequest::new(&source_tracks, 3);
    request.layers = AccompanimentLayers {
        drums: true,
        bass: true,
        harmony: true,
        lead: true,
    };
    let mut document = crate::fixtures::power_chords();
    let before = document.clone();
    let result = generate_accompaniment(&mut document, &request).unwrap();

    for entry in result.roles {
        let has_feature = match entry.role {
            AccompanimentRole::Drums | AccompanimentRole::Bass => {
                cfg!(all(feature = "drums", feature = "rhythm"))
            }
            AccompanimentRole::Harmony | AccompanimentRole::Lead => cfg!(feature = "harmony"),
        };
        if !has_feature {
            assert_eq!(
                entry.outcome,
                AccompanimentOutcome::Skipped(AccompanimentSkipReason::FeatureDisabled)
            );
        }
    }
    assert_eq!(document.tracks, before.tracks);
}
