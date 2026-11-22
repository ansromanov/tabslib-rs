use super::*;
use crate::model::Technique;

fn uniform_power_chords() -> Document {
    let mut document = crate::fixtures::power_chords();
    let source = document.bars[0].voices[0]
        .beats
        .iter()
        .map(|beat| beat.notes.iter().map(|note| note.midi).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for bar in &mut document.bars {
        for (beat, pitches) in bar.voices[0].beats.iter_mut().zip(&source) {
            for (note, pitch) in beat.notes.iter_mut().zip(pitches) {
                note.midi = *pitch;
            }
        }
    }
    document
}

fn boundary(bar_index: usize) -> SectionBoundary {
    SectionBoundary {
        bar_index,
        confidence: 1.0,
        similarity: None,
        already_marked: false,
    }
}

#[test]
fn texture_changes_detect_a_confident_boundary_but_repeated_bars_do_not() {
    let mut changed = uniform_power_chords();
    for bar in &mut changed.bars[2..] {
        for note in bar
            .voices
            .iter_mut()
            .flat_map(|voice| &mut voice.beats)
            .flat_map(|beat| &mut beat.notes)
        {
            note.midi = note.midi.map(|midi| midi + 3);
        }
    }
    let before = changed.clone();
    let boundaries = detect_section_boundaries(&changed, 0.8).expect("valid score");

    assert_eq!(
        boundaries
            .iter()
            .map(|item| item.bar_index)
            .collect::<Vec<_>>(),
        [2]
    );
    assert!((boundaries[0].confidence - 0.25).abs() < f64::EPSILON);
    assert_eq!(changed, before, "detection must leave the score untouched");

    let repeated = uniform_power_chords();
    assert!(
        detect_section_boundaries(&repeated, 0.8)
            .expect("repeated score fingerprints")
            .is_empty(),
        "identical adjacent fingerprints do not create boundaries"
    );
}

#[test]
fn merge_and_fold_suggestions_report_edits_without_mutating_the_score() {
    let document = uniform_power_chords();
    let boundaries = [boundary(1), boundary(2)];
    let before = document.clone();

    let merges = propose_section_merges(&document, &boundaries, 0.9).expect("merge suggestions");
    assert_eq!(
        merges
            .iter()
            .map(|item| item.remove_marker_at)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let folds = propose_single_bar_folds(&document, &boundaries).expect("fold suggestions");
    let middle = folds
        .iter()
        .find(|proposal| proposal.section_bar == 1)
        .expect("middle one-bar section has a fold proposal");
    assert_eq!(middle.into_bar, 0);
    assert_eq!(middle.remove_marker_at, 1);
    assert_eq!(document, before, "proposals are read-only");
}

#[test]
fn marker_application_skips_existing_labels_and_never_moves_them() {
    let mut document = uniform_power_chords();
    document.master_bars[2].section = Some("Authored Verse".into());
    let original = document.master_bars[2].section.clone();
    let boundaries = [boundary(1), boundary(2), boundary(3)];
    let labels = ["Intro", "Replace me", "Bridge"].map(str::to_string);
    let proposals = propose_section_markers(&boundaries, &labels).expect("labels match boundaries");

    let applied = apply_section_marker_proposals(&mut document, &proposals).expect("marker edits");
    assert_eq!(applied, [1, 3]);
    assert_eq!(document.master_bars[1].section.as_deref(), Some("Intro"));
    assert_eq!(document.master_bars[2].section, original);
    assert_eq!(document.master_bars[3].section.as_deref(), Some("Bridge"));
}

#[test]
fn isolated_accents_report_their_bar_and_step() {
    let mut document = crate::fixtures::repeated_frets();
    document.bars[0].voices[0].beats[0].notes[0]
        .techniques
        .push(Technique::Accent);
    let stabs = detect_riff_stabs(&document, &[0], 1).expect("riff track");

    assert_eq!(stabs.len(), 1);
    assert_eq!(stabs[0].bar_index, 0);
    assert_eq!(stabs[0].step, 0);
    assert_eq!(stabs[0].track_names, ["gtr"]);
}

#[test]
fn invalid_inputs_are_reported() {
    let document = crate::fixtures::repeated_frets();
    assert_eq!(
        detect_section_boundaries(&document, f64::NAN),
        Err(SectionAnalysisError::InvalidThreshold)
    );
    assert_eq!(
        sections_from_boundaries(2, &[boundary(2)]),
        Err(SectionAnalysisError::InvalidBoundary {
            bar_index: 2,
            bar_count: 2,
        })
    );
    assert_eq!(
        detect_riff_stabs(&document, &[], 1),
        Err(SectionAnalysisError::NoSourceTracks)
    );
}
