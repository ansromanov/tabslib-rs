use super::*;
use crate::inspect::{self, BarFeel};
use crate::model::{NoteValue, Rhythm};

#[test]
fn reader_preserves_the_onsets_written_in_midi_ticks() {
    let mut source = crate::fixtures::drum_patterns();
    source.master_bars.truncate(1);
    source.bars.retain(|bar| bar.id == 0);
    let beats = &mut source.bars[0].voices[0].beats;
    for (index, beat) in beats.iter_mut().enumerate() {
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
    let bytes = write(&source);

    let imported = read(&bytes).expect("MIDI reads");
    let positions = inspect::beat_positions(&imported, 0, 0, 0);
    let sounding = positions
        .iter()
        .filter(|position| {
            !imported.bars[0].voices[0].beats[position.beat_index]
                .notes
                .is_empty()
        })
        .map(|position| position.onset)
        .collect::<Vec<_>>();
    let expected = [
        (0, 1),
        (3, 16),
        (1, 4),
        (7, 16),
        (1, 2),
        (11, 16),
        (3, 4),
        (15, 16),
    ]
    .map(|(numerator, denominator)| inspect::Fraction::new(numerator, denominator));

    assert_eq!(imported.note_count(), source.note_count());
    assert_eq!(sounding, expected);
}

#[test]
fn reader_preserves_triplet_subdivision_timing_for_feel_detection() {
    let mut source = crate::fixtures::drum_patterns();
    source.master_bars.truncate(1);
    source.bars.retain(|bar| bar.id == 0);
    for beat in &mut source.bars[0].voices[0].beats {
        beat.rhythm = Rhythm {
            value: NoteValue::Eighth,
            dots: 0,
            tuplet: Some((3, 2)),
        };
    }

    let imported = read(&write(&source)).expect("triplet MIDI reads");
    assert_eq!(
        inspect::detect_bar_feel(&imported, 0, 0, 0),
        BarFeel::Triplet
    );
}
