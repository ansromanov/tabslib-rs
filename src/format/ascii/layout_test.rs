use super::*;

use crate::fixtures;
use crate::model::{NoteValue, TempoChange, Voice};

fn layout(doc: &Document, start: usize, end: usize) -> TrackLayout {
    layout_track(doc, 0, start, end, &LayoutOptions::default()).expect("layout")
}

fn frac(numerator: u64, denominator: u64) -> Fraction {
    Fraction::new(numerator, denominator)
}

/// Bar 0 of `repeated_frets` rewritten as quarter, eighth, eighth, half.
fn mixed_rhythm_doc() -> Document {
    let mut doc = fixtures::repeated_frets();
    let beats = &mut doc.bars[0].voices[0].beats;
    beats.truncate(4);
    beats[0].rhythm = Rhythm::new(NoteValue::Quarter);
    beats[3].rhythm = Rhythm::new(NoteValue::Half);
    doc
}

#[test]
fn a_quarter_is_twice_as_wide_as_an_eighth() {
    let bar = &layout(&mixed_rhythm_doc(), 0, 0).bars[0];
    let widths: Vec<usize> = bar.beats.iter().map(|beat| beat.width).collect();
    assert_eq!(widths[0], 2 * widths[1], "quarter vs eighth: {widths:?}");
    assert_eq!(widths[1], widths[2]);
    assert_eq!(widths[3], 4 * widths[1], "half vs eighth: {widths:?}");
    let columns: Vec<usize> = bar.beats.iter().map(|beat| beat.column).collect();
    assert_eq!(
        columns,
        [
            0,
            widths[0],
            widths[0] + widths[1],
            widths[0] + 2 * widths[1]
        ]
    );
    assert_eq!(bar.width, widths.iter().sum::<usize>());
}

#[test]
fn fixed_spacing_gives_every_beat_the_same_width() {
    let options = LayoutOptions {
        proportional: false,
        ..LayoutOptions::default()
    };
    let doc = mixed_rhythm_doc();
    let bar = &layout_track(&doc, 0, 0, 0, &options).expect("layout").bars[0];
    let widths: Vec<usize> = bar.beats.iter().map(|beat| beat.width).collect();
    assert_eq!(widths, vec![widths[0]; 4]);
}

#[test]
fn onsets_and_durations_are_exact_fractions_of_a_whole_note() {
    let bar = &layout(&mixed_rhythm_doc(), 0, 0).bars[0];
    let onsets: Vec<Fraction> = bar.beats.iter().map(|beat| beat.onset).collect();
    assert_eq!(onsets, [frac(0, 1), frac(1, 4), frac(3, 8), frac(1, 2)]);
    let durations: Vec<Fraction> = bar.beats.iter().map(|beat| beat.duration).collect();
    assert_eq!(durations, [frac(1, 4), frac(1, 8), frac(1, 8), frac(1, 2)]);
    assert_eq!(bar.beats[3].rhythm, Some(Rhythm::new(NoteValue::Half)));
}

#[test]
fn triplets_land_on_exact_thirds() {
    let mut doc = fixtures::repeated_frets();
    let beats = &mut doc.bars[0].voices[0].beats;
    beats.truncate(3);
    for beat in beats.iter_mut() {
        beat.rhythm = Rhythm {
            value: NoteValue::Eighth,
            dots: 0,
            tuplet: Some((3, 2)),
        };
    }
    let bar = &layout(&doc, 0, 0).bars[0];
    let onsets: Vec<Fraction> = bar.beats.iter().take(3).map(|beat| beat.onset).collect();
    assert_eq!(onsets, [frac(0, 1), frac(1, 12), frac(1, 6)]);
    // The voice stops after a quarter, so its last beat lasts to the bar end.
    assert_eq!(bar.beats.len(), 3);
    assert_eq!(bar.beats[2].duration, frac(1, 12).plus(frac(3, 4)));
}

#[test]
fn voices_merge_by_onset() {
    let mut doc = mixed_rhythm_doc();
    // Voice 1 sounds at 0, 1/4, 3/8 and 1/2. Voice 2 plays a dotted half then
    // an eighth on string 3, adding an onset at 3/4.
    let mut second = doc.bars[0].voices[0].clone();
    second.id = 999;
    second.beats.truncate(2);
    second.beats[0].rhythm = Rhythm {
        value: NoteValue::Half,
        dots: 1,
        tuplet: None,
    };
    second.beats[0].notes[0].string = Some(3);
    second.beats[0].notes[0].fret = Some(9);
    second.beats[1].notes[0].string = Some(3);
    second.beats[1].notes[0].fret = Some(10);
    doc.bars[0].voices.push(Voice { ..second });
    let layout = layout(&doc, 0, 0);
    let bar = &layout.bars[0];
    let onsets: Vec<Fraction> = bar.beats.iter().map(|beat| beat.onset).collect();
    assert_eq!(
        onsets,
        [frac(0, 1), frac(1, 4), frac(3, 8), frac(1, 2), frac(3, 4)]
    );
    let line = layout
        .lines
        .iter()
        .position(|line| matches!(line, Line::String { string: 3, .. }))
        .expect("string 3 laid out");
    let first = bar.beats[0].cells[line].as_ref().expect("voice 2 note");
    assert_eq!((first.text.as_str(), first.voice), ("9", 1));
    let last = bar.beats[4].cells[line].as_ref().expect("voice 2 note");
    assert_eq!(last.text, "10");
    assert_eq!(layout.unplaced_notes, 0);
}

#[test]
fn highest_string_is_the_first_line_and_notes_land_on_their_string() {
    let layout = layout(&fixtures::repeated_frets(), 0, 0);
    assert_eq!(layout.labels, ["e", "B", "G", "D", "A", "E"]);
    // Every fixture note is on string 0, the lowest: the last line.
    let bottom = layout.lines.len() - 1;
    for beat in &layout.bars[0].beats {
        assert_eq!(
            beat.cells[bottom].as_ref().map(|cell| cell.text.as_str()),
            Some("5")
        );
        assert!(beat.cells[..bottom].iter().all(Option::is_none));
        assert_eq!(beat.annotations, [Annotation::PalmMute]);
    }
}

#[test]
fn bend_cell_reads_fret_and_target() {
    // every_technique bar 9 bends frets 5..=8 up a whole tone.
    let layout = layout(&fixtures::every_technique(), 9, 9);
    let bottom = layout.lines.len() - 1;
    let texts: Vec<&str> = layout.bars[0]
        .beats
        .iter()
        .filter_map(|beat| beat.cells[bottom].as_ref())
        .map(|cell| cell.text.as_str())
        .collect();
    assert_eq!(texts, ["5b7", "6b8", "7b9", "8b10"]);
}

#[test]
fn hammer_and_slide_connectors_follow_the_next_note() {
    let mut doc = fixtures::every_technique();
    // Bar 3 hammers frets 5, 6, 7, 8; make the last step a pull-off.
    doc.bars[3].voices[0].beats[3].notes[0].fret = Some(2);
    let layout = layout(&doc, 3, 3);
    let bottom = layout.lines.len() - 1;
    let connectors: Vec<Option<Connector>> = layout.bars[0]
        .beats
        .iter()
        .map(|beat| beat.cells[bottom].as_ref().and_then(|cell| cell.connector))
        .collect();
    assert_eq!(
        connectors,
        [
            Some(Connector::HammerOn),
            Some(Connector::HammerOn),
            Some(Connector::PullOff),
            None
        ]
    );
    // Bar 7 is a shift slide up 5, 6, 7, 8.
    let layout = layout_track(&doc, 0, 7, 7, &LayoutOptions::default()).expect("layout");
    let first = layout.bars[0].beats[0].cells[bottom]
        .as_ref()
        .expect("note");
    assert_eq!(first.connector, Some(Connector::SlideUp));
}

#[test]
fn an_empty_bar_is_as_wide_as_its_meter() {
    let mut doc = fixtures::repeated_frets();
    doc.bars[1].voices.clear();
    let layout = layout(&doc, 0, 1);
    let empty = &layout.bars[1];
    assert_eq!(empty.beats.len(), 1);
    assert!(empty.beats[0].rest);
    assert_eq!(empty.beats[0].duration, frac(1, 1));
    // One 4/4 bar = four quarter-note units of min width 2.
    assert_eq!(empty.width, 4 * 2);
}

#[test]
fn bar_marks_come_from_the_master_bar_and_tempo_map() {
    let mut doc = fixtures::meters_and_sections();
    doc.master_bars[1].repeat_start = true;
    doc.master_bars[2].repeat_end = Some(3);
    doc.master_bars[2].alternate_ending = 0b101;
    doc.tempo_map = vec![TempoChange {
        at_bar: 1,
        bpm: 96.0,
        linear: false,
    }];
    let layout = layout(&doc, 0, 3);
    let bars = &layout.bars;
    assert_eq!(
        bars.iter().map(|bar| bar.index).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(bars[0].section.as_deref(), Some("Intro"));
    assert!(bars[0].time_changed && bars[1].time_changed);
    assert_eq!(bars[1].time, (3, 4));
    assert_eq!(bars[1].tempo_change.map(|tempo| tempo.bpm), Some(96.0));
    assert!(bars[0].tempo_change.is_none());
    assert!(bars[1].repeat_start);
    assert_eq!(bars[2].repeat_end, Some(3));
    assert_eq!(bars[2].alternate_endings, [1, 3]);
    assert_eq!(bars[2].capacity, frac(3, 4));
}

#[test]
fn unchanged_meter_is_not_flagged() {
    let layout = layout(&fixtures::repeated_frets(), 0, 2);
    let changed: Vec<bool> = layout.bars.iter().map(|bar| bar.time_changed).collect();
    assert_eq!(changed, [true, false, false]);
    // A range starting mid-score compares with the bar before it.
    let later = layout_track(
        &fixtures::repeated_frets(),
        0,
        1,
        1,
        &LayoutOptions::default(),
    )
    .expect("layout");
    assert!(!later.bars[0].time_changed);
}

#[test]
fn drum_hits_land_in_their_lanes() {
    let doc = fixtures::drum_patterns();
    let layout = layout(&doc, 0, 0);
    assert!(layout.percussion);
    let lane = |label: &str| {
        layout
            .labels
            .iter()
            .position(|candidate| candidate == label)
            .expect("lane present")
    };
    let hits = |line: usize| -> Vec<usize> {
        layout.bars[0]
            .beats
            .iter()
            .enumerate()
            .filter(|(_, beat)| beat.cells[line].is_some())
            .map(|(index, _)| index)
            .collect()
    };
    assert_eq!(hits(lane("Kick")), [0, 4]);
    assert_eq!(hits(lane("Snare")), [2, 6]);
    assert_eq!(hits(lane("Hi-Hat")).len(), 8);
    assert_eq!(layout.unplaced_notes, 0);
}

#[test]
fn beat_at_maps_a_position_to_the_sounding_beat() {
    let bar = &layout(&mixed_rhythm_doc(), 0, 0).bars[0];
    assert_eq!(beat_at(bar, frac(0, 1)), Some(0));
    assert_eq!(beat_at(bar, frac(1, 8)), Some(0));
    assert_eq!(beat_at(bar, frac(1, 4)), Some(1));
    assert_eq!(beat_at(bar, frac(7, 16)), Some(2));
    assert_eq!(beat_at(bar, frac(15, 16)), Some(3));
    assert_eq!(beat_at(bar, frac(1, 1)), None);
}

#[test]
fn wrap_never_exceeds_the_width_and_keeps_every_bar() {
    // Bar 3 holds two-digit frets, so it is wider than the others.
    let layout = layout(&fixtures::repeated_frets(), 0, 4);
    let narrow = layout.system_width(0..1);
    assert!(layout.system_width(3..4) > narrow);
    for width in [narrow, narrow * 2, narrow * 3 - 1, 1000] {
        let systems = wrap(&layout, width);
        let covered: Vec<usize> = systems.iter().flat_map(|range| range.clone()).collect();
        assert_eq!(covered, (0..5).collect::<Vec<_>>());
        for range in &systems {
            assert!(
                range.len() == 1 || layout.system_width(range.clone()) <= width,
                "{width}: {range:?}"
            );
        }
    }
    assert_eq!(wrap(&layout, 1000), vec![0..5]);
    assert_eq!(wrap(&layout, narrow * 2), vec![0..2, 2..3, 3..4, 4..5]);
    // Too narrow for any bar: each bar still gets a system of its own.
    assert_eq!(wrap(&layout, 1).len(), 5);
}

#[test]
fn invalid_selection_is_an_error() {
    let doc = fixtures::repeated_frets();
    let options = LayoutOptions::default();
    assert!(layout_track(&doc, 1, 0, 0, &options).is_err());
    assert!(layout_track(&doc, 0, 2, 1, &options).is_err());
    assert!(layout_track(&doc, 0, 0, 5, &options).is_err());
}
