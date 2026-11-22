#![cfg(all(feature = "drums", feature = "rhythm"))]

use tabslib::fixtures;
use tabslib::format::{ReadFormat, WriteFormat};
use tabslib::generation::drums::performance::DrumProfile;
use tabslib::generation::drums::voices;
use tabslib::generation::meter::BeatGrouping;
use tabslib::generation::rhythm::{BassGenerationConfig, BassProfile, BassStrategy, BassTuning};
use tabslib::generation::workflows::{
    generate_bass, generate_bass_with_groupings, generate_drums, generate_drums_with_groupings,
};
use tabslib::inspect;

fn drum_profile() -> DrumProfile {
    DrumProfile {
        kick_voice: voices::ACOUSTIC_BASS_DRUM,
        snare_voice: voices::ACOUSTIC_SNARE,
        cymbal_voice: voices::CLOSED_HI_HAT,
        kick_probability: 100,
        snare_probability: 100,
        cymbal_probability: 100,
    }
}

fn bass_profile() -> BassProfile {
    BassProfile {
        min_midi: 28,
        max_midi: 52,
        max_leap: 7,
        accent_velocity: 110,
        normal_velocity: 90,
    }
}

#[test]
fn document_workflows_add_meter_preserving_tracks() {
    let mut doc = fixtures::power_chords();
    let original_bars = doc.master_bars.len();
    let drums = generate_drums(&mut doc, &[0], &drum_profile(), 42, 120).unwrap();
    let bass = generate_bass(&mut doc, &[0], BassGenerationConfig::new(bass_profile())).unwrap();

    assert_eq!(doc.master_bars.len(), original_bars);
    assert_eq!(doc.tracks[drums.track_index].tuning, Vec::<i32>::new());
    assert_eq!(doc.tracks[bass.track_index].tuning, vec![26, 33, 38, 43]);
    assert!(drums.note_count > 0);
    assert!(bass.note_count > 0);
    assert!(inspect::bar_integrity(&doc)
        .iter()
        .all(|item| item.duration <= item.capacity));
    let bass_bar_ids = doc
        .master_bars
        .iter()
        .map(|bar| bar.bar_ids[bass.track_index] as u32)
        .collect::<std::collections::BTreeSet<_>>();
    for master in &doc.master_bars {
        let bar_id = master.bar_ids[bass.track_index] as u32;
        let bar = doc.bars.iter().find(|bar| bar.id == bar_id).unwrap();
        let duration = bar
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .map(|beat| {
                let (numerator, denominator) = beat.rhythm.as_fraction();
                inspect::Fraction::new(numerator, denominator)
            })
            .fold(inspect::Fraction::new(0, 1), inspect::Fraction::plus);
        assert_eq!(duration, inspect::bar_capacity(master.time));
    }
    for bar in doc.bars.iter().filter(|bar| bass_bar_ids.contains(&bar.id)) {
        for note in bar
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
        {
            let string = note.string.unwrap() as usize;
            let fret = note.fret.unwrap();
            assert!((0..=12).contains(&fret));
            assert_eq!(
                doc.tracks[bass.track_index].tuning[string] + fret,
                note.midi.unwrap()
            );
        }
    }
}

#[test]
fn bass_workflow_accepts_derived_and_custom_tuning_with_strategy_selection() {
    let mut derived = fixtures::power_chords();
    derived.tracks[0].tuning = fixtures::TUNING_DROP_D.to_vec();
    let generated = generate_bass(
        &mut derived,
        &[0],
        BassGenerationConfig {
            strategy: BassStrategy::PumpingEighths,
            ..BassGenerationConfig::new(bass_profile())
        },
    )
    .unwrap();
    assert_eq!(
        derived.tracks[generated.track_index].tuning,
        vec![26, 33, 38, 43]
    );

    let custom_tuning = [24, 31, 36, 41, 45];
    let mut custom = fixtures::power_chords();
    let generated = generate_bass(
        &mut custom,
        &[0],
        BassGenerationConfig {
            strategy: BassStrategy::Sustained,
            tuning: BassTuning::Custom(&custom_tuning),
            ..BassGenerationConfig::new(bass_profile())
        },
    )
    .unwrap();
    assert_eq!(custom.tracks[generated.track_index].tuning, custom_tuning);
    assert!(custom.bars.iter().any(|bar| {
        bar.voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .any(|beat| beat.rhythm.as_fraction() == (1, 1) && !beat.notes.is_empty())
    }));
}

#[test]
fn document_workflows_are_reproducible_and_round_trip() {
    let mut first = fixtures::power_chords();
    let mut second = fixtures::power_chords();
    generate_drums(&mut first, &[0], &drum_profile(), 7, 140).unwrap();
    generate_drums(&mut second, &[0], &drum_profile(), 7, 140).unwrap();
    assert_eq!(first, second);

    let bytes = <tabslib::format::gp::Gp as WriteFormat>::write(&first).unwrap();
    let reloaded = <tabslib::format::gp::Gp as ReadFormat>::read(&bytes).unwrap();
    assert_eq!(reloaded.note_count(), first.note_count());
    assert_eq!(
        reloaded.technique_count(|_| true),
        first.technique_count(|_| true)
    );
    assert!(reloaded.tracks.iter().any(|track| track.tuning.is_empty()));
}

#[test]
fn workflows_reject_missing_sources() {
    let mut doc = fixtures::power_chords();
    assert!(generate_drums(&mut doc, &[], &drum_profile(), 1, 120).is_err());
    assert!(generate_bass(&mut doc, &[99], BassGenerationConfig::new(bass_profile())).is_err());
}

/// A generated track's log must name a decision for every bar it wrote, and
/// the track index must be the one the document ended up with -- not the hint
/// taken before the track was appended.
#[test]
fn a_generated_track_logs_a_decision_for_every_bar() {
    use tabslib::generation::execution_log::{explain, GeneratedRole};

    let mut doc = fixtures::power_chords();
    let bars = doc.master_bars.len();
    let generated = generate_drums(
        &mut doc,
        &[0],
        &DrumProfile {
            kick_voice: 36,
            snare_voice: 38,
            cymbal_voice: 42,
            kick_probability: 100,
            snare_probability: 100,
            cymbal_probability: 100,
        },
        0x51f3,
        140,
    )
    .unwrap();

    assert_eq!(
        generated.log.decisions.len(),
        bars,
        "one decision per generated bar"
    );
    assert!(
        generated
            .log
            .decisions
            .iter()
            .all(|decision| decision.track == generated.track_index),
        "the log names the track the document ended up with"
    );
    assert_eq!(
        doc.tracks[generated.track_index].midi_channel,
        Some(9),
        "generated drum tracks use the General MIDI percussion channel"
    );
    assert_eq!(
        generated.log.by_role().get(&GeneratedRole::Drums),
        Some(&bars)
    );

    let explained = explain(&generated.log, 0, Some(generated.track_index));
    assert_eq!(explained.len(), 1);
    assert_eq!(
        explained[0].seed, generated.log.seed,
        "the first bar mixes nothing into the run seed"
    );
    // Every later bar must mix, or the recorded seed cannot reproduce the bar.
    let later = explain(&generated.log, 1, Some(generated.track_index));
    assert_eq!(later.len(), 1);
    assert_ne!(
        later[0].seed, generated.log.seed,
        "a per-bar seed is mixed from the run seed, not reused verbatim"
    );
}

#[test]
fn odd_meter_grouping_changes_drum_backbeat_and_bass_accent_steps() {
    let default_grouping = BeatGrouping::for_meter((7, 8)).unwrap();
    let override_grouping = BeatGrouping::new((7, 8), vec![3, 2, 2]).unwrap();
    let mut default = fixtures::meters_and_sections();
    clear_7_8_after_downbeat(&mut default);
    let mut overridden = default.clone();

    let drum_profile = DrumProfile {
        kick_voice: voices::ACOUSTIC_BASS_DRUM,
        snare_voice: voices::ACOUSTIC_SNARE,
        cymbal_voice: voices::CLOSED_HI_HAT,
        kick_probability: 0,
        snare_probability: 100,
        cymbal_probability: 0,
    };
    let default_drums = generate_drums_with_groupings(
        &mut default,
        &[0],
        &drum_profile,
        19,
        120,
        &[default_grouping],
    )
    .unwrap();
    let override_drums = generate_drums_with_groupings(
        &mut overridden,
        &[0],
        &drum_profile,
        19,
        120,
        std::slice::from_ref(&override_grouping),
    )
    .unwrap();
    assert_eq!(
        midi_steps(
            &default,
            default_drums.track_index,
            3,
            voices::ACOUSTIC_SNARE
        ),
        vec![0, 4],
        "the default 2+2+3 grouping places its snare at group two"
    );
    assert_eq!(
        midi_steps(
            &overridden,
            override_drums.track_index,
            3,
            voices::ACOUSTIC_SNARE
        ),
        vec![0, 6],
        "the caller's 3+2+2 grouping moves the snare to its second group"
    );
    assert!(inspect::bar_integrity(&default).is_empty());
    assert!(inspect::bar_integrity(&overridden).is_empty());

    let bass = bass_profile();
    let config = BassGenerationConfig {
        strategy: BassStrategy::PumpingEighths,
        ..BassGenerationConfig::new(bass)
    };
    let mut default_bass_doc = fixtures::meters_and_sections();
    clear_7_8_after_downbeat(&mut default_bass_doc);
    let mut override_bass_doc = default_bass_doc.clone();
    let default_bass = generate_bass(&mut default_bass_doc, &[0], config).unwrap();
    let override_bass =
        generate_bass_with_groupings(&mut override_bass_doc, &[0], config, &[override_grouping])
            .unwrap();
    assert_eq!(
        dynamic_steps(&default_bass_doc, default_bass.track_index, 3, "F"),
        vec![0, 4, 8],
        "the default groups make their pulse bass notes loud"
    );
    assert_eq!(
        dynamic_steps(&override_bass_doc, override_bass.track_index, 3, "F"),
        vec![0, 6, 10],
        "the caller grouping moves the written bass accents"
    );
}

#[test]
fn workflow_succeeds_with_one_four_bar_and_csharp_standard_source() {
    let mut doc = fixtures::power_chords();
    doc.tracks[0].tuning = vec![37, 42, 47, 52, 56, 61];

    doc.master_bars[1].time = (1, 4);
    let bar_id = doc.master_bars[1].bar_ids[0] as u32;
    let bar = doc.bars.iter_mut().find(|b| b.id == bar_id).unwrap();
    for voice in &mut bar.voices {
        voice.beats.truncate(1);
    }

    let drums = generate_drums(&mut doc, &[0], &drum_profile(), 42, 120).unwrap();
    let bass = generate_bass(&mut doc, &[0], BassGenerationConfig::new(bass_profile())).unwrap();

    assert_eq!(doc.tracks[bass.track_index].tuning, vec![25, 30, 35, 40]);
    assert!(drums.note_count > 0);
    assert!(bass.note_count > 0);

    let integrity_issues = inspect::bar_integrity(&doc);
    assert!(
        integrity_issues.is_empty(),
        "integrity issues found: {integrity_issues:?}"
    );
}

fn clear_7_8_after_downbeat(document: &mut tabslib::Document) {
    let master_index = document
        .master_bars
        .iter()
        .position(|bar| bar.time == (7, 8))
        .unwrap();
    let bar_id = u32::try_from(document.master_bars[master_index].bar_ids[0]).unwrap();
    let bar = document
        .bars
        .iter_mut()
        .find(|bar| bar.id == bar_id)
        .unwrap();
    for voice in &mut bar.voices {
        for beat in voice.beats.iter_mut().skip(1) {
            beat.notes.clear();
        }
    }
}

fn midi_steps(document: &tabslib::Document, track: usize, master_bar: usize, midi: u8) -> Vec<u32> {
    let bar_id = u32::try_from(document.master_bars[master_bar].bar_ids[track]).unwrap();
    let bar = document.bars.iter().find(|bar| bar.id == bar_id).unwrap();
    bar.voices[0]
        .beats
        .iter()
        .enumerate()
        .filter(|(_, beat)| {
            beat.notes
                .iter()
                .any(|note| note.midi == Some(i32::from(midi)))
        })
        .map(|(step, _)| u32::try_from(step).unwrap())
        .collect()
}

fn dynamic_steps(
    document: &tabslib::Document,
    track: usize,
    master_bar: usize,
    dynamic: &str,
) -> Vec<u32> {
    let bar_id = u32::try_from(document.master_bars[master_bar].bar_ids[track]).unwrap();
    let bar = document.bars.iter().find(|bar| bar.id == bar_id).unwrap();
    let mut step = 0u32;
    let mut positions = Vec::new();
    for beat in &bar.voices[0].beats {
        if beat.dynamic.as_deref() == Some(dynamic) {
            positions.push(step);
        }
        let (numerator, denominator) = beat.rhythm.as_fraction();
        let steps = numerator * 16 / denominator;
        step += u32::try_from(steps).unwrap();
    }
    positions
}

#[test]
fn arrange_short_meter_overfull_fixture_fits_destination_meter() {
    let source = fixtures::short_meter_overfull();
    let motifs = tabslib::generation::arrangement::segment_motifs(
        &source,
        source.master_bars.len().max(1),
        0.0,
    )
    .unwrap();
    assert_eq!(motifs.len(), 1);
    let plan = tabslib::generation::arrangement::ArrangementPlan::new(
        0,
        vec![tabslib::generation::arrangement::SectionRequest {
            label: "Motif 1".into(),
            bars: 3,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let output = tabslib::generation::arrangement::compose_plan(&source, &motifs, &plan).unwrap();
    assert_eq!(output.master_bars.len(), 3);
    for master in &output.master_bars {
        assert_eq!(master.time, (1, 4));
    }
    for item in inspect::bar_integrity(&output) {
        assert!(
            item.duration <= item.capacity,
            "bar {} voice {} exceeds capacity",
            item.bar_index + 1,
            item.voice_index
        );
    }
}
