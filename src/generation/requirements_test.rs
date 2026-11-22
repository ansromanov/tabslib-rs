use super::*;

use crate::fixtures;
use crate::model::{NoteValue, Rhythm};

/// Violations of one requirement, in document order.
fn failures(doc: &Document, requirement: Requirement) -> Vec<Violation> {
    check(doc)
        .into_iter()
        .filter(|violation| violation.requirement == requirement)
        .collect()
}

/// Mutable access to one beat, so a test can state the shape it needs without
/// rebuilding a document.
fn beat_mut(
    doc: &mut Document,
    bar_index: usize,
    track_index: usize,
    beat_index: usize,
) -> &mut crate::model::Beat {
    let id = doc.master_bars[bar_index].bar_ids[track_index];
    let bar = doc
        .bars
        .iter_mut()
        .find(|bar| bar.id == id as u32)
        .expect("bar exists");
    &mut bar.voices[0].beats[beat_index]
}

#[test]
fn playable_fixtures_violate_nothing() {
    let playable = [
        ("repeated frets", fixtures::repeated_frets()),
        ("power chords", fixtures::power_chords()),
        ("bass line", fixtures::bass_line()),
        ("drum patterns", fixtures::drum_patterns()),
        ("meters and sections", fixtures::meters_and_sections()),
    ];
    for (name, doc) in playable {
        let violations = check(&doc);
        assert!(
            violations.is_empty(),
            "{name}: expected no violations, got {violations:?}"
        );
    }
}

#[test]
fn track_check_excludes_authored_tracks_from_generated_track_diagnostics() {
    let document = crate::fixtures::power_chords();
    assert!(check_track(&document, 0).is_empty());
    assert!(check_track(&document, document.tracks.len()).is_empty());
}

/// The note-value catalogue is a codec fixture, not playable material: it
/// spells every written duration in turn, so most of its bars do not fill
/// their meter and its shortest values sit above the tempo ceiling. Asserting
/// that keeps the checker honest about what it is measuring.
#[test]
fn the_note_value_catalogue_reports_capacity_and_subdivision() {
    let doc = fixtures::all_note_values();
    let reported: std::collections::BTreeSet<Requirement> =
        check(&doc).iter().map(|v| v.requirement).collect();
    assert_eq!(
        reported,
        [Requirement::FastSubdivision, Requirement::BarCapacity]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
    );
}

/// The scale run's last bar carries the five notes left over from its
/// two-octave sequence, so it is three eighths short of its meter. That is a
/// true H10 finding rather than a checker defect, and it is the assertion that
/// would fail if the capacity walk silently rounded.
#[test]
fn the_scale_run_reports_only_its_short_final_bar() {
    let doc = fixtures::scale_run();
    let found = check(&doc);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].requirement, Requirement::BarCapacity);
    assert_eq!(
        found[0].detail, -24,
        "three eighths is twenty-four sixty-fourths"
    );
}

#[test]
fn h1_reports_a_note_outside_the_detected_scale() {
    let mut doc = fixtures::scale_run();
    let clean = failures(&doc, Requirement::ScaleConformance);
    assert!(clean.is_empty(), "scale run starts in scale: {clean:?}");

    let estimate = crate::inspect::determine_key(&doc).expect("a key");
    let allowed = estimate.mode.pitch_classes(estimate.key.tonic);
    let outside = (0..12)
        .find(|class| !allowed.contains(class))
        .expect("a mode leaves five pitch classes out");

    let beat = beat_mut(&mut doc, 0, 0, 0);
    let note = &mut beat.notes[0];
    let string = note.string.expect("a fretted note");
    let open = crate::model::open_pitch(&fixtures::TUNING_E_STANDARD, string).expect("in tuning");
    let fret = (outside - open).rem_euclid(12) + 12;
    note.fret = Some(fret);
    note.midi = Some(open + fret);

    let found = failures(&doc, Requirement::ScaleConformance);
    assert_eq!(found.len(), 1, "expected exactly one out-of-scale note");
    assert_eq!(found[0].detail, i64::from(open + fret));
    assert_eq!(found[0].id(), "H1");
}

#[test]
fn h2_reports_staccato_and_let_ring_separately() {
    let mut doc = fixtures::repeated_frets();
    assert!(failures(&doc, Requirement::SustainTechnique).is_empty());

    beat_mut(&mut doc, 0, 0, 0).notes[0]
        .techniques
        .push(Technique::Staccato);
    beat_mut(&mut doc, 0, 0, 1).notes[0]
        .techniques
        .push(Technique::LetRing);

    let found = failures(&doc, Requirement::SustainTechnique);
    assert_eq!(
        found.iter().map(|v| v.detail).collect::<Vec<_>>(),
        vec![0, 1],
        "staccato reports 0 and let-ring reports 1"
    );
}

#[test]
fn h3_allows_power_shapes_and_rejects_a_skipped_string() {
    let doc = fixtures::power_chords();
    assert!(
        failures(&doc, Requirement::AdjacentStrings).is_empty(),
        "a power chord spans strings 0 and 2 and is idiomatic"
    );

    let mut doc = fixtures::power_chords();
    let beat = beat_mut(&mut doc, 0, 0, 0);
    let tuning = fixtures::TUNING_DROP_D;
    let note = &mut beat.notes[1];
    note.string = Some(3);
    note.fret = Some(1);
    note.midi = crate::model::sounding_pitch(&tuning, 3, 1);

    let found = failures(&doc, Requirement::AdjacentStrings);
    assert_eq!(found.len(), 1, "expected the altered beat to be reported");
    assert_eq!(found[0].detail, 3, "strings 0 and 3 leave a gap of three");
}

#[test]
fn h4_reports_a_fast_leap_and_ignores_a_preserved_shape() {
    let mut doc = fixtures::repeated_frets();
    // Only the first transition leaps; every later beat lands on the same
    // fret, so a second report would mean the walk is comparing the wrong
    // pair of beats.
    for index in 0..8 {
        let beat = beat_mut(&mut doc, 0, 0, index);
        if index == 0 {
            beat.rhythm = Rhythm::new(NoteValue::Sixteenth);
        }
        let fret = if index == 0 { 2 } else { 12 };
        let string = beat.notes[0].string.expect("a fretted note");
        beat.notes[0].fret = Some(fret);
        beat.notes[0].midi =
            crate::model::sounding_pitch(&fixtures::TUNING_E_STANDARD, string, fret);
    }

    let found = failures(&doc, Requirement::HandPositionLeap);
    assert_eq!(found.len(), 1, "one leap of ten frets at a sixteenth");
    assert_eq!(found[0].detail, 10);

    // The same leap at an eighth is a hand move, not a defect.
    beat_mut(&mut doc, 0, 0, 0).rhythm = Rhythm::new(NoteValue::Eighth);
    assert!(failures(&doc, Requirement::HandPositionLeap).is_empty());
}

#[test]
fn h5_reports_a_silent_bar_only_inside_a_sounding_section() {
    let mut doc = fixtures::meters_and_sections();
    assert!(failures(&doc, Requirement::SilentBar).is_empty());

    let id = doc.master_bars[1].bar_ids[0];
    let bar = doc
        .bars
        .iter_mut()
        .find(|bar| bar.id == id as u32)
        .expect("bar exists");
    for voice in &mut bar.voices {
        for beat in &mut voice.beats {
            beat.notes.clear();
        }
    }

    let found = failures(&doc, Requirement::SilentBar);
    assert_eq!(found.len(), 1, "the emptied bar is reported");
    assert_eq!(found[0].region.bar, 1);
}

#[test]
fn h6_counts_fretted_tracks_and_ignores_bass_and_percussion() {
    let mut doc = fixtures::power_chords();
    assert!(failures(&doc, Requirement::FrettedTrackCount).is_empty());

    for extra in 1..3 {
        let mut track = doc.tracks[0].clone();
        track.id = extra;
        track.name = format!("gtr {extra}");
        crate::edits::create_track(&mut doc, track);
    }
    // Copy the source bars so the new tracks sound and classify as fretted.
    let source: Vec<u32> = doc
        .master_bars
        .iter()
        .map(|m| m.bar_ids[0] as u32)
        .collect();
    for (index, master) in doc.master_bars.iter_mut().enumerate() {
        for slot in &mut master.bar_ids[1..3] {
            *slot = source[index] as i32;
        }
    }

    let found = failures(&doc, Requirement::FrettedTrackCount);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].detail, 3, "three fretted tracks is one too many");
}

#[test]
fn h7_reports_a_cloned_track() {
    let mut doc = fixtures::power_chords();
    let mut clone = doc.tracks[0].clone();
    clone.name = "gtr copy".into();
    crate::edits::create_track(&mut doc, clone);
    let source: Vec<i32> = doc.master_bars.iter().map(|m| m.bar_ids[0]).collect();
    for (index, master) in doc.master_bars.iter_mut().enumerate() {
        master.bar_ids[1] = source[index];
    }

    let found = failures(&doc, Requirement::TrackDuplication);
    assert_eq!(found.len(), 1, "the second track repeats the first exactly");
    assert_eq!(found[0].detail, 100);
}

#[test]
fn h9_reports_short_subdivisions_only_above_the_tempo_ceiling() {
    let mut doc = fixtures::repeated_frets();
    beat_mut(&mut doc, 0, 0, 0).rhythm = Rhythm::new(NoteValue::ThirtySecond);

    doc.tempo_map = vec![crate::model::TempoChange {
        at_bar: 0,
        bpm: 60.0,
        linear: false,
    }];
    assert!(
        failures(&doc, Requirement::FastSubdivision).is_empty(),
        "a thirty-second at 60 BPM is playable"
    );

    doc.tempo_map = vec![crate::model::TempoChange {
        at_bar: 0,
        bpm: 180.0,
        linear: false,
    }];
    let found = failures(&doc, Requirement::FastSubdivision);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].detail, 180);
}

#[test]
fn h10_reports_a_voice_that_underfills_its_bar() {
    let mut doc = fixtures::repeated_frets();
    assert!(failures(&doc, Requirement::BarCapacity).is_empty());

    beat_mut(&mut doc, 0, 0, 0).rhythm = Rhythm::new(NoteValue::Sixteenth);

    let found = failures(&doc, Requirement::BarCapacity);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].detail, -4,
        "a sixteenth where an eighth belonged is four sixty-fourths short"
    );
}

#[test]
fn h11_accepts_a_whole_tone_bend_and_rejects_a_three_quarter_tone() {
    // The scale run opens on the tonic, so a whole-tone bend lands on the
    // second degree and stays in scale by construction.
    let mut doc = fixtures::scale_run();
    beat_mut(&mut doc, 0, 0, 0).notes[0]
        .techniques
        .push(Technique::Bend {
            origin: 0,
            middle: 50,
            dest: 100,
        });
    assert!(
        failures(&doc, Requirement::BendInterval).is_empty(),
        "a semitone through to a whole tone is playable and diatonic"
    );

    for technique in &mut beat_mut(&mut doc, 0, 0, 0).notes[0].techniques {
        if let Technique::Bend { dest, .. } = technique {
            *dest = 75;
        }
    }

    let found = failures(&doc, Requirement::BendInterval);
    assert_eq!(found.len(), 1, "three quarter tones is not playable");
    assert_eq!(found[0].detail, 75);
}

#[test]
fn h13_reports_every_track_after_the_first_sharing_a_name() {
    let mut doc = fixtures::power_chords();
    let clone = doc.tracks[0].clone();
    crate::edits::create_track(&mut doc, clone);
    doc.tracks[1].name = doc.tracks[0].name.clone();

    let found = failures(&doc, Requirement::TrackNameCollision);
    assert_eq!(
        found.len(),
        1,
        "the first occurrence is the binding one; later ones are the defect"
    );
    assert_eq!(found[0].region.track, Some(1));
    assert_eq!(found[0].detail, 2);
}

#[test]
fn h14_reports_string_motion_faster_than_a_player_sustains() {
    let mut doc = fixtures::repeated_frets();
    doc.tempo_map = vec![crate::model::TempoChange {
        at_bar: 0,
        bpm: 200.0,
        linear: false,
    }];
    // Only the first transition crosses strings; the rest stay put, so a
    // second report would mean the walk is counting something else.
    for index in 0..8 {
        let beat = beat_mut(&mut doc, 0, 0, index);
        if index == 0 {
            beat.rhythm = Rhythm::new(NoteValue::Sixteenth);
        }
        let string = if index == 0 { 0 } else { 5 };
        let fret = beat.notes[0].fret.expect("a fretted note");
        beat.notes[0].string = Some(string);
        beat.notes[0].midi =
            crate::model::sounding_pitch(&fixtures::TUNING_E_STANDARD, string, fret);
    }

    let found = failures(&doc, Requirement::StringMotionRate);
    assert_eq!(found.len(), 1, "five strings crossed in a sixteenth at 200");
    assert!(
        found[0].detail > 12,
        "expected a rate above the ceiling, got {}",
        found[0].detail
    );
}

#[test]
fn requirement_identifiers_are_unique_and_stable() {
    let all = [
        Requirement::ScaleConformance,
        Requirement::SustainTechnique,
        Requirement::AdjacentStrings,
        Requirement::HandPositionLeap,
        Requirement::SilentBar,
        Requirement::FrettedTrackCount,
        Requirement::TrackDuplication,
        Requirement::FastSubdivision,
        Requirement::BarCapacity,
        Requirement::BendInterval,
        Requirement::TrackNameCollision,
        Requirement::StringMotionRate,
    ];
    let ids: std::collections::BTreeSet<&str> = all.iter().map(|r| r.id()).collect();
    let codes: std::collections::BTreeSet<&str> = all.iter().map(|r| r.code()).collect();
    assert_eq!(ids.len(), all.len(), "identifiers collide");
    assert_eq!(codes.len(), all.len(), "codes collide");
}
