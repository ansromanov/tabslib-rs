use super::*;

use crate::format::gp::Gp;
use crate::format::{ReadFormat, WriteFormat};

/// A payload for the five-bar tempo fixture whose tempo automations are
/// replaced by `automations`, spelled exactly as given.
fn payload_with(automations: &str) -> String {
    let document = crate::fixtures::tempo_changes();
    let payload = super::super::write::write(&document);
    let written = automations_xml(&document.tempo_map);
    payload.replace(&written, automations)
}

fn automation(bar: usize, position: &str, value: &str) -> String {
    format!(
        "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>{bar}</Bar>\
         <Position>{position}</Position><Visible>true</Visible><Value>{value}</Value>\
         </Automation>"
    )
}

fn tempo_map(payload: &str) -> Result<Vec<(usize, f64)>> {
    let document = super::super::parse::parse(payload)?;
    Ok(document
        .tempo_map
        .iter()
        .map(|change| (change.at_bar, change.bpm))
        .collect())
}

#[test]
fn reference_unit_converts_to_quarter_note_bpm() {
    // Dotted quarter = 60 is quarter = 90; eighth = 180 is quarter = 90 too.
    // Ignoring the unit would report 60 and 180.
    let payload = payload_with(&format!(
        "{}{}{}",
        automation(0, "0", "60 3"),
        automation(1, "0", "180 1"),
        automation(2, "0", "45 4")
    ));
    assert_eq!(
        tempo_map(&payload).expect("parses"),
        vec![(0, 90.0), (1, 90.0), (2, 90.0)]
    );
}

#[test]
fn quarter_unit_and_bare_value_are_read_unscaled() {
    let payload = payload_with(&format!(
        "{}{}",
        automation(0, "0", "85 2"),
        automation(3, "0", "140")
    ));
    assert_eq!(
        tempo_map(&payload).expect("parses"),
        vec![(0, 85.0), (3, 140.0)]
    );
}

#[test]
fn unknown_reference_unit_is_an_error_not_a_guess() {
    let payload = payload_with(&automation(0, "0", "85 9"));
    assert!(matches!(tempo_map(&payload), Err(Error::Malformed(_))));
}

#[test]
fn mid_bar_changes_snap_to_the_nearest_bar_line() {
    // 1@0.25 governs most of bar 1; 2@0.875 and 3@1 take effect at the next
    // bar line. Reading the written bar would file 105 at bar 2 and 150 at 3.
    let payload = payload_with(&format!(
        "{}{}{}{}",
        automation(0, "0", "120 2"),
        automation(1, "0.25", "110 2"),
        automation(2, "0.875", "105 2"),
        automation(3, "1", "150 2")
    ));
    assert_eq!(
        tempo_map(&payload).expect("parses"),
        vec![(0, 120.0), (1, 110.0), (3, 105.0), (4, 150.0)]
    );
}

#[test]
fn several_changes_on_one_bar_line_keep_the_latest() {
    // A ritardando at the end of bar 1 lands on bar 2's line, where an
    // explicit bar-2 tempo follows it in the score and wins. Keeping both
    // would give bar 2 two tempi; keeping the first would report 100.
    let payload = payload_with(&format!(
        "{}{}{}{}",
        automation(2, "0", "140 2"),
        automation(0, "0", "130 2"),
        automation(1, "0.875", "100 2"),
        automation(0, "0.25", "125 2")
    ));
    assert_eq!(
        tempo_map(&payload).expect("parses"),
        vec![(0, 125.0), (2, 140.0)],
        "one entry per bar, sorted, latest in the score wins"
    );
}

#[test]
fn change_snapped_past_the_last_bar_is_dropped() {
    let payload = payload_with(&format!(
        "{}{}",
        automation(0, "0", "120 2"),
        automation(4, "1", "60 2")
    ));
    assert_eq!(tempo_map(&payload).expect("parses"), vec![(0, 120.0)]);
}

#[test]
fn position_outside_the_bar_is_an_error() {
    let payload = payload_with(&automation(0, "1.5", "120 2"));
    assert!(matches!(tempo_map(&payload), Err(Error::Malformed(_))));
}

#[test]
fn patch_replaces_tempo_and_keeps_other_automations() {
    let other = "<Automation><Type>SyncPoint</Type><Bar>1</Bar></Automation>";
    let payload = payload_with(&format!(
        "{}{other}{}",
        automation(0, "0", "120 2"),
        automation(2, "0.875", "100 2")
    ));
    let map = [TempoChange {
        at_bar: 1,
        bpm: 77.0,
        linear: false,
    }];
    let patched = patch(&payload, &map).expect("payload has a master track");
    assert!(patched.contains(other), "non-tempo automation survives");
    assert_eq!(patched.matches("<Type>Tempo</Type>").count(), 1);
    assert_eq!(tempo_map(&patched).expect("parses"), vec![(1, 77.0)]);
}

#[test]
fn first_change_late_in_bar_zero_still_opens_the_score() {
    // Written at the end of bar 0 with nothing before it: snapping it to bar 1
    // would leave the opening bar with no tempo at all.
    let payload = payload_with(&format!(
        "{}{}",
        automation(0, "1", "155 2"),
        automation(2, "0", "140 2")
    ));
    assert_eq!(
        tempo_map(&payload).expect("parses"),
        vec![(0, 155.0), (2, 140.0)]
    );
}

#[test]
fn patch_without_master_track_defers_to_a_full_write() {
    assert_eq!(patch("<GPIF><Tracks/></GPIF>", &[]), None);
}

fn loaded(payload: &str) -> Document {
    let bytes = super::super::container::write_payload(payload).expect("container");
    Gp::read(&bytes).expect("reads")
}

use crate::model::Document;

#[test]
fn edited_tempo_of_a_loaded_file_is_saved() {
    let mut document = loaded(&payload_with(&automation(0, "0", "85 2")));
    assert_eq!(document.tempo_at(0), Some(85.0));
    document.tempo_map[0].bpm = 100.0;
    document.tempo_map.push(TempoChange {
        at_bar: 3,
        bpm: 50.0,
        linear: true,
    });
    let reloaded = Gp::read(&Gp::write(&document).expect("writes")).expect("reads back");
    assert_eq!(
        reloaded.tempo_map, document.tempo_map,
        "source patching must not resurrect the original tempo"
    );
}

#[test]
fn unedited_tempo_keeps_its_mid_bar_spelling_through_a_note_edit() {
    let original = automation(2, "0.875", "100 2");
    let mut document = loaded(&payload_with(&format!(
        "{}{original}",
        automation(0, "0", "120 2")
    )));
    let fret = document.bars[0].voices[0].beats[0].notes[0]
        .fret
        .expect("fixture note is fretted");
    document.bars[0].voices[0].beats[0].notes[0].fret = Some(fret + 1);
    let bytes = Gp::write(&document).expect("writes");
    let payload = super::super::container::read_payload(&bytes).expect("payload");
    assert!(
        payload.contains(&original),
        "a note edit must not rewrite the source's mid-bar tempo automation"
    );
    let reloaded = Gp::read(&bytes).expect("reads back");
    assert_eq!(
        reloaded.bars[0].voices[0].beats[0].notes[0].fret,
        Some(fret + 1)
    );
}
