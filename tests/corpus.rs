//! Round-trip invariants over a corpus of real files.
//!
//! No file is committed here and no test names one. Point `TABSLIB_CORPUS` at a
//! directory of `.gp` files to run these; without it they skip, so the crate
//! stays testable standalone.
//!
//! Every assertion is a quantity that *changes* if the feature under test is
//! silently dropped. Note count, title and track count all stay correct while
//! rhythm and articulation are being destroyed, so none of them is sufficient
//! on its own -- the duration histogram and the per-technique census are the
//! assertions that carry weight here.

use std::collections::BTreeMap;
use std::path::PathBuf;
use tabslib::model::{NoteValue, Rhythm, Technique};
use tabslib::Document;

fn corpus_files() -> Vec<PathBuf> {
    let Some(dir) = std::env::var("TABSLIB_CORPUS").ok().map(PathBuf::from).filter(|p| p.is_dir())
    else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "gp"))
        .collect();
    v.sort();
    v
}

fn duration_histogram(doc: &Document) -> BTreeMap<(u64, u64), usize> {
    let mut h = BTreeMap::new();
    for bar in &doc.bars {
        for v in &bar.voices {
            for b in &v.beats {
                *h.entry(b.rhythm.as_fraction()).or_insert(0) += 1;
            }
        }
    }
    h
}

fn technique_census(doc: &Document) -> BTreeMap<&'static str, usize> {
    let mut c = BTreeMap::new();
    for bar in &doc.bars {
        for v in &bar.voices {
            for b in &v.beats {
                for n in &b.notes {
                    for t in &n.techniques {
                        let k = match t {
                            Technique::PalmMute => "palm-mute",
                            Technique::Dead => "dead",
                            Technique::Accent => "accent",
                            Technique::Ghost => "ghost",
                            Technique::Staccato => "staccato",
                            Technique::LetRing => "let-ring",
                            Technique::Vibrato => "vibrato",
                            Technique::Tapped => "tapped",
                            Technique::HammerOrigin => "hammer-origin",
                            Technique::HammerDestination => "hammer-destination",
                            Technique::Slide { .. } => "slide",
                            Technique::Bend { .. } => "bend",
                            Technique::Harmonic { .. } => "harmonic",
                        };
                        *c.entry(k).or_insert(0) += 1;
                    }
                }
            }
        }
    }
    c
}

#[test]
fn every_file_loads() {
    for path in corpus_files() {
        let bytes = std::fs::read(&path).expect("read");
        let doc = tabslib::load(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!doc.bars.is_empty(), "{}: no bars", path.display());
        assert!(doc.note_count() > 0, "{}: no notes", path.display());
    }
}

/// The load/save/reload invariant. Anything the codec silently drops shows up
/// here as a changed count, which is the whole point of the test.
#[test]
fn round_trip_changes_nothing_measurable() {
    for path in corpus_files() {
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).expect("read");
        let before = tabslib::load(&bytes).expect("load");
        let saved = tabslib::save(&before).expect("save");
        let after = tabslib::load(&saved).expect("reload");

        assert_eq!(after.note_count(), before.note_count(), "{name}: notes");
        assert_eq!(after.section_count(), before.section_count(), "{name}: sections");
        assert_eq!(after.tracks.len(), before.tracks.len(), "{name}: tracks");
        assert_eq!(
            technique_census(&after),
            technique_census(&before),
            "{name}: techniques"
        );
        assert_eq!(
            duration_histogram(&after),
            duration_histogram(&before),
            "{name}: duration histogram"
        );
    }
}

/// Saving twice must produce identical bytes -- the zip writer stamps
/// wall-clock time unless told not to, which silently breaks reproducibility.
#[test]
fn save_is_deterministic() {
    for path in corpus_files().into_iter().take(3) {
        let bytes = std::fs::read(&path).expect("read");
        let doc = tabslib::load(&bytes).expect("load");
        assert_eq!(
            tabslib::save(&doc).expect("a"),
            tabslib::save(&doc).expect("b"),
            "{}: save is not byte-deterministic",
            path.display()
        );
    }
}

#[test]
fn rhythm_fractions_are_exact() {
    assert_eq!(Rhythm::new(NoteValue::Quarter).as_fraction(), (1, 4));
    assert_eq!(Rhythm { value: NoteValue::Half, dots: 1, tuplet: None }.as_fraction(), (3, 4));
    assert_eq!(Rhythm { value: NoteValue::Eighth, dots: 0, tuplet: Some((3, 2)) }.as_fraction(), (1, 12));
    assert_eq!(Rhythm { value: NoteValue::Quarter, dots: 2, tuplet: None }.as_fraction(), (7, 16));
}

/// `16th` is what Guitar Pro writes; `Sixteenth` is what a plausible
/// implementation writes, and it does not read back the same way.
#[test]
fn note_values_use_the_spelling_guitar_pro_writes() {
    assert_eq!(NoteValue::Sixteenth.as_gpif(), "16th");
    assert_eq!(NoteValue::ThirtySecond.as_gpif(), "32nd");
    assert_eq!(NoteValue::Quarter.as_gpif(), "Quarter");
    assert_eq!(NoteValue::parse("Sixteenth"), Some(NoteValue::Sixteenth));
    assert_eq!(NoteValue::parse("16th"), Some(NoteValue::Sixteenth));
}
