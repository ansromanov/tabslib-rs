use super::*;

use crate::inspect;
use crate::model::sounding_pitch;

const E_STANDARD: [i32; 6] = [40, 45, 50, 55, 59, 64];

fn spec(archetype: RiffArchetype) -> RiffSpec {
    RiffSpec {
        archetype,
        register: Register::Low,
        voicing: Voicing::PowerFifth,
        key: KeySignature {
            tonic: 4,
            minor: true,
        },
        mode: MusicalMode::NaturalMinor,
        degrees: vec![1, 4, 6, 7],
        bars: 4,
        tempo: 140.0,
        tuning: E_STANDARD.to_vec(),
        palm_mute: true,
        syncopation: None,
        quarter_triplets: false,
        seed: 0x51ff,
        name: "Riff".into(),
    }
}

const ARCHETYPES: [RiffArchetype; 7] = [
    RiffArchetype::PedalTone,
    RiffArchetype::Tremolo,
    RiffArchetype::Gallop,
    RiffArchetype::Sustained,
    RiffArchetype::Syncopated,
    RiffArchetype::Sparse,
    RiffArchetype::LockedSparse,
];

/// Every bar's written duration against its meter capacity.
///
/// This is the assertion that moves when the duration speller rounds: a bar
/// keeps its note count while losing an eighth, and nothing else notices.
fn capacity_shortfalls(doc: &Document) -> Vec<(usize, crate::inspect::Fraction)> {
    inspect::bar_integrity(doc)
        .into_iter()
        .map(|entry| (entry.bar_index, entry.duration))
        .collect()
}

#[test]
fn every_archetype_writes_bars_that_are_exactly_full() {
    for archetype in ARCHETYPES {
        let doc = write_riff(&spec(archetype)).expect("a realizable specification");
        assert_eq!(doc.master_bars.len(), 4, "{archetype:?}");
        let shortfalls = capacity_shortfalls(&doc);
        assert!(
            shortfalls.is_empty(),
            "{archetype:?}: bars do not fill their meter: {shortfalls:?}"
        );
    }
}

#[test]
fn every_archetype_writes_only_notes_in_the_specified_mode() {
    for archetype in ARCHETYPES {
        let spec = spec(archetype);
        let doc = write_riff(&spec).expect("a realizable specification");
        let allowed = spec.mode.pitch_classes(spec.key.tonic);
        for bar in &doc.bars {
            for voice in &bar.voices {
                for beat in &voice.beats {
                    for note in &beat.notes {
                        let midi = note.midi.expect("a sounding pitch");
                        assert!(
                            allowed.contains(&midi.rem_euclid(12)),
                            "{archetype:?}: {midi} is outside {allowed:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn every_written_position_sounds_the_pitch_it_states() {
    for archetype in ARCHETYPES {
        let doc = write_riff(&spec(archetype)).expect("a realizable specification");
        for bar in &doc.bars {
            for voice in &bar.voices {
                for beat in &voice.beats {
                    for note in &beat.notes {
                        let (string, fret, midi) = (
                            note.string.expect("a string"),
                            note.fret.expect("a fret"),
                            note.midi.expect("a pitch"),
                        );
                        assert_eq!(
                            sounding_pitch(&E_STANDARD, string, fret),
                            Some(midi),
                            "{archetype:?}: string {string} fret {fret} does not sound {midi}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn no_written_note_is_staccato_or_let_ring() {
    for archetype in ARCHETYPES {
        let doc = write_riff(&spec(archetype)).expect("a realizable specification");
        for bar in &doc.bars {
            for voice in &bar.voices {
                for beat in &voice.beats {
                    for note in &beat.notes {
                        assert!(
                            !note.techniques.contains(&Technique::Staccato)
                                && !note.techniques.contains(&Technique::LetRing),
                            "{archetype:?} wrote a sustain technique"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_power_voicing_sounds_a_root_and_a_fifth() {
    let mut base = spec(RiffArchetype::Sustained);
    base.voicing = Voicing::PowerFifth;
    let doc = write_riff(&base).expect("a realizable specification");
    let chord = doc.bars[0].voices[0]
        .beats
        .iter()
        .find(|beat| beat.notes.len() > 1)
        .expect("a chorded attack");
    let mut pitches: Vec<i32> = chord.notes.iter().filter_map(|note| note.midi).collect();
    pitches.sort_unstable();
    assert_eq!(
        pitches[1] - pitches[0],
        7,
        "a power fifth is seven semitones, not a fourth"
    );
}

#[test]
fn an_octave_voicing_sounds_twelve_semitones() {
    let mut base = spec(RiffArchetype::Sustained);
    base.voicing = Voicing::Octave;
    let doc = write_riff(&base).expect("a realizable specification");
    let chord = doc.bars[0].voices[0]
        .beats
        .iter()
        .find(|beat| beat.notes.len() > 1)
        .expect("a chorded attack");
    let mut pitches: Vec<i32> = chord.notes.iter().filter_map(|note| note.midi).collect();
    pitches.sort_unstable();
    assert_eq!(pitches[1] - pitches[0], 12);
}

#[test]
fn dissonant_voicings_preserve_their_requested_intervals() {
    for (voicing, interval) in [
        (Voicing::MinorSecondCluster, 1),
        (Voicing::Tritone, 6),
        (Voicing::MinorSixth, 8),
    ] {
        let mut spec = spec(RiffArchetype::Sustained);
        spec.register = Register::Mid;
        spec.voicing = voicing;
        let doc = write_riff(&spec).expect("a dissonant voicing is realizable");
        let notes = doc.bars[0].voices[0]
            .beats
            .iter()
            .find(|beat| beat.notes.len() == 2)
            .expect("a dissonant chord attack")
            .notes
            .iter()
            .filter_map(|note| note.midi)
            .collect::<Vec<_>>();
        assert_eq!(notes[1] - notes[0], interval, "{voicing:?}");
    }
}

#[test]
fn locked_sparse_syncopation_moves_attacks_before_or_after_the_beat() {
    for (syncopation, expected) in [
        (Syncopation::Anticipated, vec![3, 7, 11, 15]),
        (Syncopation::Delayed, vec![1, 5, 9, 13]),
    ] {
        let mut spec = spec(RiffArchetype::LockedSparse);
        spec.syncopation = Some(syncopation);
        let doc = write_riff(&spec).expect("a locked sparse figure");
        let onsets = doc.bars[0].voices[0]
            .beats
            .iter()
            .scan(0u32, |position, beat| {
                let onset = *position;
                let (numerator, denominator) = beat.rhythm.as_fraction();
                *position += u32::try_from(numerator * 16 / denominator).unwrap();
                Some((onset, !beat.notes.is_empty()))
            })
            .filter_map(|(position, sounding)| sounding.then_some(position))
            .collect::<Vec<_>>();
        assert_eq!(onsets, expected, "{syncopation:?}");
    }
}

#[test]
fn locked_sparse_quarter_triplets_fill_the_bar_exactly() {
    let mut spec = spec(RiffArchetype::LockedSparse);
    spec.quarter_triplets = true;
    let doc = write_riff(&spec).expect("quarter triplets are realizable");
    assert!(doc.bars[0].voices[0]
        .beats
        .iter()
        .all(|beat| beat.rhythm.tuplet == Some((3, 2))));
    assert!(capacity_shortfalls(&doc).is_empty());
}

#[test]
fn archetypes_write_different_material() {
    let sparse = write_riff(&spec(RiffArchetype::Sparse)).expect("a riff");
    let tremolo = write_riff(&spec(RiffArchetype::Tremolo)).expect("a riff");
    assert!(
        tremolo.note_count() > sparse.note_count(),
        "a tremolo figure attacks more often than a sparse one: {} against {}",
        tremolo.note_count(),
        sparse.note_count()
    );
}

#[test]
fn the_same_specification_writes_the_same_riff() {
    let first = write_riff(&spec(RiffArchetype::PedalTone)).expect("a riff");
    let second = write_riff(&spec(RiffArchetype::PedalTone)).expect("a riff");
    assert_eq!(first, second, "the writer is deterministic in its seed");
}

#[test]
fn a_different_seed_writes_different_material_where_the_archetype_uses_one() {
    let mut other = spec(RiffArchetype::PedalTone);
    other.seed = 0x1234;
    let first = write_riff(&spec(RiffArchetype::PedalTone)).expect("a riff");
    let second = write_riff(&other).expect("a riff");
    assert_ne!(
        first, second,
        "a pedal figure's fills follow the seed; equal output means the seed is ignored"
    );
}

#[test]
fn palm_mute_is_written_when_asked_and_not_otherwise() {
    let mut muted = spec(RiffArchetype::Sustained);
    muted.palm_mute = true;
    let mut open = muted.clone();
    open.palm_mute = false;

    let census = |doc: &Document| {
        inspect::note_census(doc)
            .techniques
            .get("PalmMute")
            .copied()
            .unwrap_or(0)
    };
    assert!(census(&write_riff(&muted).expect("a riff")) > 0);
    assert_eq!(census(&write_riff(&open).expect("a riff")), 0);
}

#[test]
fn an_unrealizable_specification_is_rejected_rather_than_adjusted() {
    let cases: [(RiffSpec, RiffSpecError); 5] = [
        (
            RiffSpec {
                bars: 0,
                ..spec(RiffArchetype::Sustained)
            },
            RiffSpecError::NoBars,
        ),
        (
            RiffSpec {
                degrees: Vec::new(),
                ..spec(RiffArchetype::Sustained)
            },
            RiffSpecError::NoDegrees,
        ),
        (
            RiffSpec {
                degrees: vec![9],
                ..spec(RiffArchetype::Sustained)
            },
            RiffSpecError::DegreeOutOfRange(9),
        ),
        (
            RiffSpec {
                tuning: vec![40],
                voicing: Voicing::PowerOctave,
                ..spec(RiffArchetype::Sustained)
            },
            RiffSpecError::TuningTooShort {
                available: 1,
                needed: 3,
            },
        ),
        (
            RiffSpec {
                tempo: 0.0,
                ..spec(RiffArchetype::Sustained)
            },
            RiffSpecError::InvalidTempo,
        ),
    ];
    for (spec, expected) in cases {
        assert_eq!(
            write_riff(&spec).map(|_| ()),
            Err(expected.clone()),
            "expected {expected:?}"
        );
        assert_eq!(validate(&spec), Err(expected));
    }
}

#[test]
fn degrees_shorter_than_the_bar_count_cycle() {
    let mut base = spec(RiffArchetype::Sustained);
    base.degrees = vec![1, 4];
    base.bars = 4;
    let doc = write_riff(&base).expect("a riff");
    let first = &doc.bars[0];
    let third = &doc.bars[2];
    assert_eq!(
        first.voices[0].beats[0].notes[0].midi, third.voices[0].beats[0].notes[0].midi,
        "bar three restates the first degree"
    );
}

#[test]
fn a_statement_variation_motif_is_twice_the_material() {
    let motif = MotifSpec {
        riff: spec(RiffArchetype::Sustained),
        form: MotifForm::StatementVariation,
        tail: TailVariation::CadentialRhythm,
    };
    let doc = write_motif(&motif).expect("a motif");
    assert_eq!(doc.master_bars.len(), 8);
    assert!(capacity_shortfalls(&doc).is_empty());
}

#[test]
fn a_sentence_motif_answers_at_twice_the_length() {
    let motif = MotifSpec {
        riff: spec(RiffArchetype::Sustained),
        form: MotifForm::Sentence,
        tail: TailVariation::None,
    };
    let doc = write_motif(&motif).expect("a motif");
    assert_eq!(doc.master_bars.len(), 12, "four stated, eight continuing");
}

#[test]
fn a_cadential_tail_lands_on_the_tonic() {
    let degrees = answer_degrees(&[1, 4, 6, 7], TailVariation::CadentialRhythm);
    assert_eq!(degrees.last(), Some(&1));
}

#[test]
fn an_inverted_tail_runs_the_degrees_backwards() {
    assert_eq!(
        answer_degrees(&[1, 4, 6, 7], TailVariation::ContourInversion),
        vec![7, 6, 4, 1]
    );
}

#[test]
fn durations_always_sum_to_the_gap_they_spell() {
    for steps in 1..=16u32 {
        let total: u64 = durations_for(steps)
            .iter()
            .map(|rhythm| {
                let (num, den) = rhythm.as_fraction();
                num * 16 / den
            })
            .sum();
        assert_eq!(
            total,
            u64::from(steps),
            "{steps} sixteenths were spelled as {total}"
        );
    }
}

#[test]
fn a_phrygian_dominant_riff_sounds_its_flat_second_and_major_third() {
    let mut spec = spec(RiffArchetype::Sustained);
    spec.voicing = Voicing::SingleNote;
    spec.mode = MusicalMode::PhrygianDominant;
    spec.degrees = vec![2, 3];
    let doc = write_riff(&spec).expect("a realizable specification");
    let classes: std::collections::BTreeSet<i32> = doc
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .filter_map(|note| note.midi)
        .map(|midi| midi.rem_euclid(12))
        .collect();
    assert!(
        classes.contains(&5),
        "flat second F missing from {classes:?}"
    );
    assert!(
        classes.contains(&8),
        "major third G# missing from {classes:?}"
    );
    assert!(
        !classes.contains(&7),
        "minor third G present in {classes:?}"
    );
}
