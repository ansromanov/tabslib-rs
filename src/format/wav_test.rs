use super::*;
use crate::model::{Bar, Beat, MasterBar, Note, NoteValue, Rhythm, Track, Voice};
use std::io::Cursor;

/// A backend that records what it was sent and renders a flat tone while a
/// note is held, so the compiler and the stream can be tested without a
/// SoundFont2 file. Shared with the stream tests.
#[derive(Default)]
pub(super) struct MockSynth {
    active: bool,
    pub(super) messages: Vec<PerformanceEvent>,
}

impl SynthBackend for MockSynth {
    fn apply(&mut self, event: PerformanceEvent) {
        self.messages.push(event);
        match event.message {
            PerformanceMessage::NoteOn { .. } => self.active = true,
            PerformanceMessage::NoteOff { .. } => self.active = false,
            PerformanceMessage::ProgramChange { .. } => {}
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let value = if self.active { 0.25 } else { 0.0 };
        left.fill(value);
        right.fill(value);
    }

    fn reset(&mut self) {
        self.active = false;
        self.messages.clear();
    }
}

fn one_bar_document() -> Document {
    let beats = (0..4)
        .map(|index| Beat {
            id: index,
            rhythm: Rhythm::new(NoteValue::Quarter),
            notes: if index == 0 {
                vec![Note {
                    id: 1,
                    midi: Some(60),
                    string: Some(0),
                    fret: Some(20),
                    articulation: None,
                    techniques: Vec::new(),
                }]
            } else {
                Vec::new()
            },
            dynamic: if index == 0 { Some("F".into()) } else { None },
        })
        .collect();
    Document {
        title: String::new(),
        artist: String::new(),
        tracks: vec![Track {
            id: 1,
            name: "Guitar".into(),
            color: None,
            tuning: vec![40, 45, 50, 55, 59, 64],
            midi_program: Some(24),
            midi_channel: Some(0),
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
            generator_metadata: None,
        }],
        master_bars: vec![MasterBar {
            index: 0,
            time: (4, 4),
            section: None,
            double_bar: false,
            bar_ids: vec![1],
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        }],
        bars: vec![Bar {
            id: 1,
            clef: Some("G2".into()),
            voices: vec![Voice { id: 1, beats }],
        }],
        tempo_map: vec![crate::model::TempoChange {
            at_bar: 0,
            bpm: 120.0,
            linear: false,
        }],
        key: None,
        source: None,
    }
}

#[test]
fn compiler_emits_program_note_velocity_and_tempo_based_samples() {
    let performance = compile_performance_events(&one_bar_document()).unwrap();

    assert_eq!(performance.sample_count, 88_200);
    assert_eq!(
        performance.events,
        vec![
            PerformanceEvent {
                sample: 0,
                track_index: 0,
                channel: 0,
                message: PerformanceMessage::ProgramChange { program: 24 },
            },
            PerformanceEvent {
                sample: 0,
                track_index: 0,
                channel: 0,
                message: PerformanceMessage::NoteOn {
                    note: 60,
                    velocity: 96,
                },
            },
            PerformanceEvent {
                sample: 22_050,
                track_index: 0,
                channel: 0,
                message: PerformanceMessage::NoteOff { note: 60 },
            },
        ]
    );
}

#[test]
fn compiler_expands_repeats_and_applies_tempo_changes() {
    let mut document = one_bar_document();
    document.master_bars[0].repeat_start = true;
    document.master_bars[0].repeat_end = Some(2);
    let repeated = compile_performance_events(&document).unwrap();
    assert_eq!(repeated.sample_count, 176_400);
    assert_eq!(
        repeated
            .events
            .iter()
            .filter(|event| matches!(event.message, PerformanceMessage::NoteOn { .. }))
            .map(|event| event.sample)
            .collect::<Vec<_>>(),
        vec![0, 88_200]
    );

    let mut changed_tempo = one_bar_document();
    changed_tempo.master_bars.push(MasterBar {
        index: 1,
        time: (4, 4),
        section: None,
        double_bar: false,
        bar_ids: vec![1],
        repeat_start: false,
        repeat_end: None,
        alternate_ending: 0,
        direction: None,
    });
    changed_tempo.tempo_map.push(crate::model::TempoChange {
        at_bar: 1,
        bpm: 60.0,
        linear: false,
    });
    let changed = compile_performance_events(&changed_tempo).unwrap();
    assert_eq!(changed.sample_count, 264_600);
    assert_eq!(
        changed
            .events
            .iter()
            .filter(|event| matches!(event.message, PerformanceMessage::NoteOn { .. }))
            .count(),
        2
    );
}

#[test]
fn articulation_shortens_palm_muted_notes_and_invalid_data_is_rejected() {
    let mut document = one_bar_document();
    document.bars[0].voices[0].beats[0].notes[0]
        .techniques
        .push(Technique::PalmMute);
    let performance = compile_performance_events(&document).unwrap();
    assert!(performance.events.iter().any(|event| {
        event.sample == 11_025 && event.message == PerformanceMessage::NoteOff { note: 60 }
    }));

    document.tempo_map[0].bpm = f64::NAN;
    assert!(compile_performance_events(&document).is_err());
}

#[test]
fn mock_soundfont_backend_produces_stereo_16_bit_wav_with_exact_frame_count() {
    let performance = compile_performance_events(&one_bar_document()).unwrap();
    let mut synth = MockSynth::default();
    let wav = render_compiled(&performance, &mut synth).unwrap();

    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(wav[4..8].try_into().unwrap()),
        36 + 88_200 * 4
    );
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(wav[24..28].try_into().unwrap()),
        SAMPLE_RATE
    );
    assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
    assert_eq!(
        u32::from_le_bytes(wav[40..44].try_into().unwrap()),
        88_200 * 4
    );
    assert_eq!(wav.len(), 44 + 88_200 * 4);
    assert_eq!(i16::from_le_bytes(wav[44..46].try_into().unwrap()), 8192);
    assert_eq!(i16::from_le_bytes(wav[46..48].try_into().unwrap()), 8192);
    assert!(synth.messages.iter().any(|event| {
        event.message
            == PerformanceMessage::NoteOn {
                note: 60,
                velocity: 96,
            }
    }));
}

#[test]
fn malformed_soundfont_bytes_return_a_format_error() {
    let mut invalid_soundfont = Cursor::new(b"not a soundfont".as_slice());
    assert!(create_backend(&mut invalid_soundfont).is_err());
}

#[test]
fn channel_nine_is_authoritative_for_percussion() {
    let mut tuned_percussion = one_bar_document();
    tuned_percussion.tracks[0].midi_channel = Some(9);
    let channels = track_channels(&tuned_percussion, &[0]).unwrap();
    assert_eq!(
        channels[0], 9,
        "channel 9 must be authoritative despite non-empty tuning"
    );

    let mut untuned_pitched = one_bar_document();
    untuned_pitched.tracks[0].tuning.clear();
    untuned_pitched.tracks[0].midi_channel = Some(0);
    let channels = track_channels(&untuned_pitched, &[0]).unwrap();
    assert_ne!(
        channels[0], 9,
        "channel != 9 must be pitched despite empty tuning"
    );
    assert_eq!(channels[0], 0);

    let mut untuned_fallback = one_bar_document();
    untuned_fallback.tracks[0].tuning.clear();
    untuned_fallback.tracks[0].midi_channel = None;
    let channels = track_channels(&untuned_fallback, &[0]).unwrap();
    assert_eq!(
        channels[0], 9,
        "tuning heuristic applies when midi_channel is None"
    );
}
