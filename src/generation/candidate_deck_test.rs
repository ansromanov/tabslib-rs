use super::*;
use crate::model::Technique;

fn variants() -> Vec<DeckCandidateInput> {
    let dense = crate::fixtures::power_chords();
    let mut sparse = dense.clone();
    for (bar_index, master) in sparse.master_bars.iter().enumerate() {
        let id = master.bar_ids[0] as u32;
        let bar = sparse.bars.iter_mut().find(|bar| bar.id == id).unwrap();
        for (beat_index, beat) in bar.voices[0].beats.iter_mut().enumerate() {
            if beat_index % 2 == 1 {
                beat.notes.clear();
            } else if beat.notes.len() > 1 {
                beat.notes.truncate(1);
            }
            if (bar_index + beat_index) % 2 == 0 {
                for note in &mut beat.notes {
                    note.techniques
                        .retain(|technique| *technique != Technique::PalmMute);
                }
            }
        }
    }
    let mut high = dense.clone();
    for bar in &mut high.bars {
        for note in bar
            .voices
            .iter_mut()
            .flat_map(|voice| &mut voice.beats)
            .flat_map(|beat| &mut beat.notes)
        {
            note.midi = note.midi.map(|pitch| pitch + 12);
            note.fret = note.fret.map(|fret| fret + 12);
        }
    }
    vec![
        DeckCandidateInput {
            id: "dense".into(),
            archetype: "caller:heavy".into(),
            document: dense,
        },
        DeckCandidateInput {
            id: "sparse".into(),
            archetype: "caller:open".into(),
            document: sparse,
        },
        DeckCandidateInput {
            id: "high-register".into(),
            archetype: "caller:lifted".into(),
            document: high,
        },
    ]
}

#[test]
fn document_features_cover_density_register_voicing_mutes_and_time_feel() {
    let document = crate::fixtures::power_chords();
    let features = FeatureVector::from_document(&document);
    assert!(features.density > 0.0);
    assert!((0.0..=1.0).contains(&features.register));
    assert!((0.0..=1.0).contains(&features.voicing_mix.iter().sum::<f64>()));
    assert_eq!(features.palm_mute_rate, 1.0);
    assert_eq!(features.time_feel, 0.0);

    let mut swung = document;
    for bar in &mut swung.bars {
        for beat in &mut bar.voices[0].beats {
            beat.rhythm.tuplet = Some((3, 2));
        }
    }
    assert_eq!(FeatureVector::from_document(&swung).time_feel, 1.0);
}

#[test]
fn vector_distance_is_zero_for_equal_features_and_positive_for_distinct_scores() {
    let candidates = variants();
    let first = FeatureVector::from_document(&candidates[0].document);
    assert_eq!(first.distance(first), 0.0);
    let sparse = FeatureVector::from_document(&candidates[1].document);
    assert!(first.distance(sparse) > 0.0);
}

#[test]
fn deck_is_seeded_pairwise_spaced_and_pages_in_both_directions() {
    let candidates = variants();
    let request = DeckRequest {
        candidate_count: 3,
        minimum_distance: 0.01,
        page_size: 2,
        seed: 42,
    };
    let mut deck = build_candidate_deck(&candidates, request).unwrap();
    let repeated = build_candidate_deck(&candidates, request).unwrap();
    assert_eq!(
        deck.entries()
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>(),
        repeated
            .entries()
            .iter()
            .map(|entry| &entry.id)
            .collect::<Vec<_>>()
    );
    for left in 0..deck.entries().len() {
        for right in left + 1..deck.entries().len() {
            assert!(
                deck.entries()[left]
                    .features
                    .distance(deck.entries()[right].features)
                    >= request.minimum_distance
            );
        }
    }
    assert_eq!(deck.current_page().len(), 2);
    assert_eq!(deck.next_page().len(), 1);
    assert_eq!(
        deck.next_page().len(),
        1,
        "forward paging clamps at the end"
    );
    assert_eq!(deck.previous_page().len(), 2);
    assert_eq!(
        deck.previous_page().len(),
        2,
        "back paging clamps at the start"
    );
    assert!(deck
        .entries()
        .iter()
        .any(|entry| entry.archetype == "caller:heavy"));
}

#[test]
fn deck_reports_when_the_callers_distance_cannot_be_met() {
    let one = variants().remove(0);
    assert_eq!(
        build_candidate_deck(
            &[
                one.clone(),
                DeckCandidateInput {
                    id: "copy".into(),
                    ..one
                }
            ],
            DeckRequest {
                candidate_count: 2,
                minimum_distance: 0.1,
                page_size: 1,
                seed: 1,
            }
        ),
        Err(DeckError::InsufficientCandidates {
            requested: 2,
            available: 1
        })
    );
}
