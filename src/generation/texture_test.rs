use super::*;
use crate::model::{Beat, Note, Voice};

use crate::model::{NoteValue, Rhythm};

fn make_bar(notes_per_beat: &[Vec<(u32, i32)>]) -> Bar {
    let beats = notes_per_beat
        .iter()
        .enumerate()
        .map(|(b_idx, notes)| {
            let beat_notes = notes
                .iter()
                .enumerate()
                .map(|(n_idx, &(string, fret))| Note {
                    id: (b_idx * 10 + n_idx) as u32 + 1,
                    midi: None,
                    string: Some(string),
                    fret: Some(fret),
                    articulation: None,
                    techniques: Vec::new(),
                })
                .collect();
            Beat {
                id: b_idx as u32 + 1,
                rhythm: Rhythm::new(NoteValue::Quarter),
                notes: beat_notes,
                dynamic: None,
            }
        })
        .collect();

    Bar {
        id: 1,
        clef: None,
        voices: vec![Voice { id: 1, beats }],
    }
}

#[test]
fn classify_bar_texture_classes() {
    // Empty bar -> Silent
    let silent_bar = make_bar(&[]);
    assert_eq!(classify_bar_texture(&silent_bar), BarTexture::Silent);

    // One attack -> Sustain
    let sustain_bar = make_bar(&[vec![(0, 0)]]);
    assert_eq!(classify_bar_texture(&sustain_bar), BarTexture::Sustain);

    // Repeated identical notes -> Chug
    let chug_bar = make_bar(&[vec![(0, 0)], vec![(0, 0)], vec![(0, 0)]]);
    assert_eq!(classify_bar_texture(&chug_bar), BarTexture::Chug);

    // Changing notes -> Movement
    let movement_bar = make_bar(&[vec![(0, 0)], vec![(0, 2)], vec![(0, 3)]]);
    assert_eq!(classify_bar_texture(&movement_bar), BarTexture::Movement);
}

#[test]
fn assert_texture_class_preserved_success_and_errors() {
    let textures1 = [BarTexture::Sustain, BarTexture::Chug];
    let textures2 = [BarTexture::Sustain, BarTexture::Chug];
    assert!(assert_texture_class_preserved(&textures1, &textures2, "transform").is_ok());

    let count_mismatch = [BarTexture::Sustain];
    let err_count = assert_texture_class_preserved(&textures1, &count_mismatch, "transform")
        .expect_err("mismatched count");
    match err_count {
        TextureError::BarCountMismatch {
            expected, actual, ..
        } => {
            assert_eq!(expected, 2);
            assert_eq!(actual, 1);
        }
        _ => panic!("expected BarCountMismatch"),
    }

    let class_mismatch = [BarTexture::Sustain, BarTexture::Movement];
    let err_class = assert_texture_class_preserved(&textures1, &class_mismatch, "transform")
        .expect_err("mismatched class");
    match err_class {
        TextureError::TextureMismatch {
            bar,
            expected,
            actual,
            ..
        } => {
            assert_eq!(bar, 2);
            assert_eq!(expected, BarTexture::Chug);
            assert_eq!(actual, BarTexture::Movement);
        }
        _ => panic!("expected TextureMismatch"),
    }
}
