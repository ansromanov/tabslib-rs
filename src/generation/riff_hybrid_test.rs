use super::*;
use crate::model::Technique;

fn request(mode: HybridMode, seed: u64) -> HybridRequest {
    HybridRequest {
        track_a: 0,
        track_b: 0,
        mode,
        seed,
    }
}

fn shifted_source() -> Document {
    let mut source = crate::fixtures::power_chords();
    for (bar_index, master) in source.master_bars.iter().enumerate() {
        let id = master.bar_ids[0] as u32;
        let bar = source.bars.iter_mut().find(|bar| bar.id == id).unwrap();
        for (beat_index, beat) in bar.voices[0].beats.iter_mut().enumerate() {
            if beat_index % 2 == 1 {
                for note in &mut beat.notes {
                    note.midi = note.midi.map(|pitch| pitch + 2);
                    note.fret = note.fret.map(|fret| fret + 2);
                }
            }
            if bar_index % 2 == 1 {
                for note in &mut beat.notes {
                    note.techniques = vec![Technique::Vibrato];
                }
            }
        }
    }
    source
}

fn pitches(document: &Document, track: usize, bar_index: usize) -> Vec<i32> {
    let id = document.master_bars[bar_index].bar_ids[track] as u32;
    let bar = document.bars.iter().find(|bar| bar.id == id).unwrap();
    bar.voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| note.midi)
        .collect()
}

#[test]
fn contour_hybrid_keeps_a_rhythm_and_is_repeatable_from_seed() {
    let a = crate::fixtures::power_chords();
    let b = shifted_source();
    let request = request(HybridMode::RhythmAContourB, 4);
    let first = hybridize(&a, &b, request).expect("compatible riffs");
    let repeated = hybridize(&a, &b, request).expect("same request repeats");
    assert_eq!(first, repeated);
    assert_eq!(first.note_count(), a.note_count());
    assert_ne!(pitches(&first, 0, 0), pitches(&a, 0, 0));
    let a_id = a.master_bars[0].bar_ids[0] as u32;
    let out_id = first.master_bars[0].bar_ids[0] as u32;
    let a_bar = a.bars.iter().find(|bar| bar.id == a_id).unwrap();
    let out_bar = first.bars.iter().find(|bar| bar.id == out_id).unwrap();
    assert_eq!(
        a_bar.voices[0]
            .beats
            .iter()
            .map(|beat| beat.rhythm)
            .collect::<Vec<_>>(),
        out_bar.voices[0]
            .beats
            .iter()
            .map(|beat| beat.rhythm)
            .collect::<Vec<_>>()
    );
}

#[test]
fn pitches_articulation_and_answers_modes_take_their_named_sources() {
    let a = crate::fixtures::power_chords();
    let b = shifted_source();
    let articulated =
        hybridize(&a, &b, request(HybridMode::PitchesAArticulationB, 9)).expect("compatible riffs");
    assert_eq!(pitches(&articulated, 0, 0), pitches(&a, 0, 0));
    let bar = &articulated
        .bars
        .iter()
        .find(|bar| bar.id == articulated.master_bars[1].bar_ids[0] as u32)
        .unwrap();
    assert!(bar.voices[0].beats[0]
        .notes
        .iter()
        .all(|note| note.techniques == [Technique::Vibrato]));

    let mut framework = a.clone();
    for (bar_index, master) in framework.master_bars.iter().enumerate() {
        let id = master.bar_ids[0] as u32;
        let bar = framework.bars.iter_mut().find(|bar| bar.id == id).unwrap();
        for (beat_index, beat) in bar.voices[0].beats.iter_mut().enumerate() {
            if (beat_index + bar_index) % 2 == 1 {
                beat.notes.clear();
            }
        }
    }
    let answered = hybridize(
        &framework,
        &b,
        request(HybridMode::LowFrameworkAAnswersB, 0),
    )
    .expect("compatible riffs");
    assert!(answered.note_count() > framework.note_count());
}

#[test]
fn interleaving_switches_source_bars_and_meter_mismatches_are_rejected() {
    let a = crate::fixtures::power_chords();
    let b = shifted_source();
    let output =
        hybridize(&a, &b, request(HybridMode::InterleaveBars, 3)).expect("compatible riffs");
    assert_eq!(pitches(&output, 0, 0), pitches(&a, 0, 0));
    assert_eq!(pitches(&output, 0, 1), pitches(&b, 0, 1));

    let mut incompatible = b;
    incompatible.master_bars[0].time = (3, 4);
    assert_eq!(
        hybridize(&a, &incompatible, request(HybridMode::InterleaveBars, 3)),
        Err(HybridError::IncompatibleMeter(0))
    );
}

#[test]
fn candidate_filter_reports_acceptance_reasons_and_metrics_without_ranking() {
    let accepted = crate::fixtures::power_chords();
    let mut rejected = accepted.clone();
    let id = rejected.master_bars[0].bar_ids[0] as u32;
    let bar = rejected.bars.iter_mut().find(|bar| bar.id == id).unwrap();
    bar.voices[0].beats[0].notes[0]
        .techniques
        .push(Technique::LetRing);
    let expected_bars = assessment::assess(&accepted).metrics[&Metric::BarCount];
    let bounds = [MetricThreshold {
        metric: Metric::BarCount,
        minimum: Some(expected_bars),
        maximum: Some(expected_bars),
    }];
    let results = filter_candidates(
        [
            Candidate {
                name: "clean".into(),
                document: accepted,
            },
            Candidate {
                name: "sustain violation".into(),
                document: rejected,
            },
        ],
        &[Requirement::SustainTechnique],
        &bounds,
    );
    assert!(results[0].accepted);
    assert!(results[0].rejections.is_empty());
    assert!(!results[1].accepted);
    assert!(matches!(
        results[1].rejections[0],
        CandidateRejection::Requirement(Violation {
            requirement: Requirement::SustainTechnique,
            ..
        })
    ));
    assert_eq!(
        results[0].assessment.metrics[&Metric::BarCount],
        expected_bars
    );
}
