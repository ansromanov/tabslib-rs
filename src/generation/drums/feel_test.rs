use super::*;
use crate::{fixtures, model::NoteValue, percussion};

#[test]
fn fixture_profile_measures_roles_and_dominant_cymbal_family() {
    let document = fixtures::drum_patterns();
    let profile = profile_existing_drums(&document, 0).expect("drum track profiles");

    assert_eq!(profile.cymbal_preference, Some(CymbalPreference::Hat));
    assert_eq!(profile.kick_density_per_bar, [2, 16, 8, 0, 1]);
    assert_eq!(profile.snare_density_per_bar, [2, 1, 8, 8, 0]);
    assert_eq!(profile.cymbal_density_per_bar, [8, 4, 16, 0, 1]);
    assert_eq!(profile.feel, DrumFeel::Straight);

    let generated = profile.to_drum_profile();
    assert_eq!(generated.kick_voice, voices::ACOUSTIC_BASS_DRUM);
    assert_eq!(generated.snare_voice, voices::ACOUSTIC_SNARE);
    assert_eq!(generated.cymbal_voice, voices::CLOSED_HI_HAT);
    assert_eq!(generated.kick_probability, 100);
    assert_eq!(generated.snare_probability, 95);
    assert_eq!(generated.cymbal_probability, 100);
}

#[test]
fn changing_timekeeping_hats_to_rides_changes_the_preference() {
    let mut document = fixtures::drum_patterns();
    for bar in &mut document.bars {
        for beat in bar.voices.iter_mut().flat_map(|voice| &mut voice.beats) {
            for note in &mut beat.notes {
                let midi = note.articulation.or(note.midi).unwrap_or_default();
                if percussion::lookup(midi).is_some_and(|(_, role)| role == DrumRole::HiHat) {
                    note.articulation = Some(51);
                    note.midi = Some(51);
                }
            }
        }
    }

    let profile = profile_existing_drums(&document, 0).expect("mutated drum track profiles");
    assert_eq!(profile.cymbal_preference, Some(CymbalPreference::Ride));
}

#[test]
fn a_track_without_cymbals_keeps_no_preference_and_generates_no_cymbal_hits() {
    let mut document = fixtures::drum_patterns();
    for bar in &mut document.bars {
        for beat in bar.voices.iter_mut().flat_map(|voice| &mut voice.beats) {
            beat.notes.retain(|note| {
                let midi = note.articulation.or(note.midi).unwrap_or_default();
                !percussion::lookup(midi).is_some_and(|(_, role)| {
                    matches!(role, DrumRole::HiHat | DrumRole::Ride | DrumRole::Cymbal)
                })
            });
        }
    }

    let profile = profile_existing_drums(&document, 0).expect("cymbal-free track profiles");
    assert_eq!(profile.cymbal_preference, None);
    assert_eq!(profile.to_drum_profile().cymbal_probability, 0);
}

#[test]
fn profile_summarizes_triplet_and_swung_written_feel() {
    let mut triplets = fixtures::drum_patterns();
    triplets.master_bars.truncate(1);
    triplets.bars.retain(|bar| bar.id == 0);
    for beat in triplets.bars[0].voices[0]
        .beats
        .iter_mut()
        .filter(|beat| !beat.notes.is_empty())
    {
        beat.rhythm.tuplet = Some((3, 2));
    }
    assert_eq!(
        profile_existing_drums(&triplets, 0)
            .expect("triplet profile")
            .feel,
        DrumFeel::Triplet
    );

    let mut swung = fixtures::drum_patterns();
    swung.master_bars.truncate(1);
    swung.bars.retain(|bar| bar.id == 0);
    let beats = &mut swung.bars[0].voices[0].beats;
    for (index, beat) in beats.iter_mut().enumerate() {
        beat.rhythm = if index % 2 == 0 {
            crate::model::Rhythm {
                value: NoteValue::Eighth,
                dots: 1,
                tuplet: None,
            }
        } else {
            crate::model::Rhythm::new(NoteValue::Sixteenth)
        };
    }
    assert_eq!(
        profile_existing_drums(&swung, 0)
            .expect("swung profile")
            .feel,
        DrumFeel::Swung
    );
}

#[test]
fn invalid_or_non_percussion_tracks_return_clear_errors() {
    let document = fixtures::power_chords();
    assert_eq!(
        profile_existing_drums(&document, 1),
        Err(DrumFeelError::TrackOutsideDocument { track_index: 1 })
    );
    assert_eq!(
        profile_existing_drums(&document, 0),
        Err(DrumFeelError::NotPercussionTrack { track_index: 0 })
    );
}
