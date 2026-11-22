#![cfg(feature = "gp")]

use tabslib::facade::file;
use tabslib::facade::generation::{self, KnownRegressionCase};

#[test]
fn persist_is_a_deterministic_document_round_trip() {
    let source = tabslib::save(&tabslib::fixtures::power_chords()).expect("fixture saves");
    let first = file::persist(&source).expect("file persists");
    let second = file::persist(&source).expect("file persists twice");
    assert_eq!(first, second);
    assert_eq!(file::read(&first).expect("round trip").tracks.len(), 1);
}

#[test]
fn file_transpose_changes_written_pitches() {
    let source_document = tabslib::fixtures::repeated_frets();
    let source = tabslib::save(&source_document).expect("fixture saves");
    let output = file::transpose(&source, 2).expect("transpose file");
    let transposed = file::read(&output).expect("read transposed file");
    let first_pitch = |document: &tabslib::Document| {
        document
            .bars
            .iter()
            .flat_map(|bar| &bar.voices)
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
            .find_map(|note| note.midi)
    };
    assert_eq!(
        first_pitch(&source_document).map(|pitch| pitch + 2),
        first_pitch(&transposed)
    );
}

#[test]
fn file_generation_returns_the_generated_track_report() {
    let source = tabslib::save(&tabslib::fixtures::power_chords()).expect("fixture saves");
    let profile = generation::DrumProfile {
        kick_voice: 35,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 80,
        snare_probability: 70,
        cymbal_probability: 75,
    };
    let (output, generated) = file::generate_drums(
        &source,
        generation::DrumGenerationRequest {
            source_tracks: &[0],
            profile: &profile,
            seed: 7,
            tempo: 120,
        },
    )
    .expect("generate drums in file");
    assert!(generated.note_count > 0);
    assert_eq!(
        file::read(&output)
            .expect("read generated file")
            .tracks
            .len(),
        2
    );
}

#[test]
fn known_regression_study_reports_each_case_and_overall_verdict() {
    let case = KnownRegressionCase {
        label: "fixture".into(),
        document: tabslib::fixtures::power_chords(),
        references: Vec::new(),
        policy: generation::GatePolicy::default(),
    };
    let results = generation::evaluate_known_regression_cases(&[case]);
    let summary = generation::summarize_model_judge_study(&results);
    assert_eq!((summary.cases, summary.passed, summary.failed), (1, 1, 0));
    assert_eq!(results[0].label, "fixture");
}
