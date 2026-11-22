use super::*;
use crate::format::midi;
use crate::model::{NoteValue, Rhythm};

fn drum_midi() -> Vec<u8> {
    midi::write(&crate::fixtures::drum_patterns())
}

fn onset(step: u32) -> RiffOnset {
    RiffOnset {
        step,
        exact: true,
        onset: Fraction::new(u64::from(step), 16),
        lowest_pitch: Some(40),
        palm_mute: false,
        accent: false,
        dead: false,
        polyphony: 1,
        track_count: 1,
        track_names: vec!["riff".into()],
        unison_score: 1,
    }
}

#[test]
fn midi_reader_events_land_on_the_requested_pattern_grid() {
    let bars = midi_to_drum_hits(
        &drum_midi(),
        PatternGrid::new((4, 4), 4).expect("sixteenth grid"),
    )
    .expect("MIDI drum groove converts");

    assert_eq!(bars.len(), 5);
    assert_eq!(bars[0].iter().filter(|hit| hit.midi == 42).count(), 8);
    assert!(bars[0].iter().any(|hit| hit.midi == 36 && hit.step == 0));
    assert!(bars[0].iter().any(|hit| hit.midi == 36 && hit.step == 8));
    assert!(bars[0].iter().all(|hit| hit.step < 16));
}

#[test]
fn supplied_odd_meter_regroups_the_absolute_midi_timeline() {
    let bars = midi_to_drum_hits(
        &drum_midi(),
        PatternGrid::new((7, 8), 4).expect("fourteen-step odd-meter bar"),
    )
    .expect("MIDI groove maps to supplied meter");

    assert!(bars.iter().flatten().all(|hit| hit.step < 14));
    assert!(bars[4].iter().any(|hit| hit.midi == 49 && hit.step == 8));
}

#[test]
fn midi_timing_distinguishes_triplets_and_swing() {
    let mut triplets = crate::fixtures::drum_patterns();
    triplets.master_bars.truncate(1);
    triplets.bars.retain(|bar| bar.id == 0);
    for beat in &mut triplets.bars[0].voices[0].beats {
        beat.rhythm = Rhythm {
            value: NoteValue::Eighth,
            dots: 0,
            tuplet: Some((3, 2)),
        };
    }
    assert_eq!(
        detect_midi_groove_feel(&midi::write(&triplets)).expect("triplet MIDI parses"),
        MidiGrooveFeel::Triplet
    );

    let mut swung = crate::fixtures::drum_patterns();
    swung.master_bars.truncate(1);
    swung.bars.retain(|bar| bar.id == 0);
    for (index, beat) in swung.bars[0].voices[0].beats.iter_mut().enumerate() {
        beat.rhythm = if index % 2 == 0 {
            Rhythm {
                value: NoteValue::Eighth,
                dots: 1,
                tuplet: None,
            }
        } else {
            Rhythm::new(NoteValue::Sixteenth)
        };
    }
    assert_eq!(
        detect_midi_groove_feel(&midi::write(&swung)).expect("swung MIDI parses"),
        MidiGrooveFeel::Swung
    );
}

#[test]
fn riff_adaptation_moves_only_kicks_and_preserves_the_timekeeping_voice() {
    let bytes = drum_midi();
    let grid = PatternGrid::new((4, 4), 4).expect("sixteenth grid");
    let mut index = MidiGrooveIndex::new();
    index
        .add_midi("straight", "verse", 120, grid, &bytes)
        .expect("valid groove");
    let groove = index.select("verse", (4, 4), 120).expect("matching groove");
    assert_eq!(groove.timekeeping_voice(), Some(42));
    let hats_before = groove.bars()[0]
        .iter()
        .filter(|hit| hit.midi == 42)
        .map(|hit| hit.step)
        .collect::<Vec<_>>();
    let kicks_before = groove.bars()[0]
        .iter()
        .filter(|hit| hit.midi == 36)
        .map(|hit| hit.step)
        .collect::<Vec<_>>();

    let onsets = vec![vec![onset(4)]];
    let adapted = adapt_midi_groove_to_riff(groove, &onsets, 36).expect("riff adaptation");
    let hats_after = adapted
        .iter()
        .filter(|hit| hit.midi == 42 && hit.step < 16)
        .map(|hit| hit.step)
        .collect::<Vec<_>>();
    let kicks_after = adapted
        .iter()
        .filter(|hit| hit.midi == 36 && hit.step < 16)
        .map(|hit| hit.step)
        .collect::<Vec<_>>();

    assert_eq!(hats_after, hats_before);
    assert_ne!(kicks_after, kicks_before);
    assert!(kicks_after.iter().all(|step| *step == 4));
}

#[test]
fn index_selects_nearest_tempo_and_reports_duplicate_ids() {
    let bytes = drum_midi();
    let grid = PatternGrid::new((4, 4), 4).expect("sixteenth grid");
    let mut index = MidiGrooveIndex::new();
    index
        .add_midi("slow", "verse", 100, grid, &bytes)
        .expect("slow groove");
    index
        .add_midi("fast", "verse", 140, grid, &bytes)
        .expect("fast groove");

    assert_eq!(
        index.select("verse", (4, 4), 135).map(MidiGroove::id),
        Some("fast")
    );
    assert_eq!(
        index.add_midi("fast", "verse", 140, grid, &bytes),
        Err(MidiGrooveError::DuplicateId { id: "fast".into() })
    );
    assert!(index.select("verse", (3, 4), 120).is_none());
}

#[test]
fn query_ranking_prefers_feel_and_excludes_blast_candidates() {
    let bytes = drum_midi();
    let grid = PatternGrid::new((4, 4), 4).expect("sixteenth grid");
    let mut index = MidiGrooveIndex::new();
    index
        .add_midi("rock-verse", "verse", 120, grid, &bytes)
        .expect("verse groove");
    index
        .add_midi("rock-blast", "blast", 120, grid, &bytes)
        .expect("blast groove");
    let candidates = find_best_midi_grooves(
        &index,
        &MidiGrooveQuery {
            role: "verse".into(),
            meter: (4, 4),
            tempo_bpm: 120,
            feel: Some(MidiGrooveFeel::Straight),
            genre: Some("rock".into()),
            ..MidiGrooveQuery::default()
        },
    );
    assert_eq!(
        candidates
            .iter()
            .map(MidiGrooveCandidate::id)
            .collect::<Vec<_>>(),
        ["rock-verse"]
    );
}
