//! Synthetic documents for testing.
//!
//! Every fixture is generated from scratch and contains only generic
//! instrumental material -- scale runs, repeated frets, power chords, note
//! values, standard drum patterns. Nothing here is transcribed from a
//! recording, so the crate is fully testable with no corpus of any kind.
//!
//! These are also the crate's worked examples of building a [`Document`] by
//! hand.

use crate::model::*;

/// Standard 6-string guitar in E, low string first (E2 A2 D3 G3 B3 E4).
pub const TUNING_E_STANDARD: [i32; 6] = [40, 45, 50, 55, 59, 64];
/// Drop D.
pub const TUNING_DROP_D: [i32; 6] = [38, 45, 50, 55, 59, 64];
/// 4-string bass (E1 A1 D2 G2).
pub const TUNING_BASS: [i32; 4] = [28, 33, 38, 43];

/// General MIDI percussion numbers used by the drum fixtures.
pub mod drum {
    pub const KICK: i32 = 36;
    pub const SNARE: i32 = 38;
    pub const HAT_CLOSED: i32 = 42;
    pub const HAT_OPEN: i32 = 46;
    pub const RIDE: i32 = 51;
    pub const CRASH: i32 = 49;
    pub const TOM_HIGH: i32 = 48;
    pub const TOM_FLOOR: i32 = 41;
}

/// Incremental id source, so every fixture has unique note and beat ids.
#[derive(Default)]
struct Ids {
    note: u32,
    beat: u32,
    voice: u32,
    bar: u32,
}

impl Ids {
    fn note(&mut self) -> u32 {
        self.note += 1;
        self.note - 1
    }
    fn beat(&mut self) -> u32 {
        self.beat += 1;
        self.beat - 1
    }
    fn voice(&mut self) -> u32 {
        self.voice += 1;
        self.voice - 1
    }
    fn bar(&mut self) -> u32 {
        self.bar += 1;
        self.bar - 1
    }
}

fn fretted(
    ids: &mut Ids,
    string: u32,
    fret: i32,
    tuning: &[i32],
    techniques: Vec<Technique>,
) -> Note {
    let open = crate::model::open_pitch(tuning, string).expect("string within the tuning");
    Note {
        id: ids.note(),
        midi: Some(open + fret),
        string: Some(string),
        fret: Some(fret),
        articulation: None,
        techniques,
    }
}

fn hit(ids: &mut Ids, midi: i32) -> Note {
    Note {
        id: ids.note(),
        midi: Some(midi),
        string: None,
        fret: None,
        articulation: Some(midi),
        techniques: Vec::new(),
    }
}

fn beat(ids: &mut Ids, rhythm: Rhythm, notes: Vec<Note>) -> Beat {
    Beat {
        id: ids.beat(),
        rhythm,
        notes,
        dynamic: None,
    }
}

fn bar(ids: &mut Ids, clef: &str, beats: Vec<Beat>) -> Bar {
    Bar {
        id: ids.bar(),
        clef: Some(clef.to_string()),
        voices: vec![Voice {
            id: ids.voice(),
            beats,
        }],
    }
}

fn assemble(
    title: &str,
    tracks: Vec<Track>,
    bars: Vec<Bar>,
    meters: Vec<(u32, u32)>,
    sections: Vec<(usize, &str)>,
) -> Document {
    let per = if tracks.is_empty() {
        0
    } else {
        bars.len() / tracks.len().max(1)
    };
    let master_bars = (0..per.max(bars.len().min(meters.len().max(1))))
        .map(|i| MasterBar {
            index: i,
            time: *meters.get(i).unwrap_or(meters.first().unwrap_or(&(4, 4))),
            section: sections
                .iter()
                .find(|(at, _)| *at == i)
                .map(|(_, n)| n.to_string()),
            double_bar: false,
            bar_ids: tracks
                .iter()
                .enumerate()
                .map(|(t, _)| (t * per + i) as i32)
                .collect(),
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        })
        .collect();
    Document {
        title: title.to_string(),
        artist: "test".into(),
        tracks,
        master_bars,
        bars,
        tempo_map: vec![],
        key: None,
        source: None,
    }
}

fn guitar(name: &str, tuning: &[i32]) -> Track {
    Track {
        id: 0,
        name: name.to_string(),
        color: Some((255, 0, 0)),
        tuning: tuning.to_vec(),
        midi_program: Some(30),
        pan: None,
        volume: None,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
    }
}

/// Every note value, plain, dotted and as a triplet.
///
/// This is the fixture that catches a rhythm table written or resolved wrongly:
/// if durations are collapsed, the histogram changes and nothing else does.
pub fn all_note_values() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_E_STANDARD;
    let values = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
        NoteValue::SixtyFourth,
    ];
    let mut bars = Vec::new();
    for v in values {
        for (dots, tuplet) in [
            (0u8, None),
            (1, None),
            (2, None),
            (0, Some((3u32, 2u32))),
            (0, Some((5, 4))),
        ] {
            let r = Rhythm {
                value: v,
                dots,
                tuplet,
            };
            let n = fretted(&mut ids, 0, 5, &tun, vec![]);
            let b = beat(&mut ids, r, vec![n]);
            bars.push(bar(&mut ids, "G2", vec![b]));
        }
    }
    let n = bars.len();
    assemble(
        "note values",
        vec![guitar("gtr", &tun)],
        bars,
        vec![(4, 4); n],
        vec![],
    )
}

/// Repeated single frets -- 5555, 7777, 8888 -- one bar each, straight eighths.
pub fn repeated_frets() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_E_STANDARD;
    let mut bars = Vec::new();
    for fret in [5, 7, 8, 12, 0] {
        let beats = (0..8)
            .map(|_| {
                let n = fretted(&mut ids, 0, fret, &tun, vec![Technique::PalmMute]);
                beat(&mut ids, Rhythm::new(NoteValue::Eighth), vec![n])
            })
            .collect();
        bars.push(bar(&mut ids, "G2", beats));
    }
    let n = bars.len();
    assemble(
        "repeated frets",
        vec![guitar("gtr", &tun)],
        bars,
        vec![(4, 4); n],
        vec![],
    )
}

/// A two-octave major scale up and down, one note per eighth.
pub fn scale_run() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_E_STANDARD;
    // major scale steps, ascending then descending
    let steps = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23, 24];
    let mut seq: Vec<i32> = steps.to_vec();
    seq.extend(steps.iter().rev().skip(1).copied());
    let mut bars = Vec::new();
    for chunk in seq.chunks(8) {
        let beats = chunk
            .iter()
            .map(|semi| {
                // keep it on one string so the fixture is unambiguous
                let n = fretted(&mut ids, 0, *semi, &tun, vec![]);
                beat(&mut ids, Rhythm::new(NoteValue::Eighth), vec![n])
            })
            .collect();
        bars.push(bar(&mut ids, "G2", beats));
    }
    let n = bars.len();
    assemble(
        "scale run",
        vec![guitar("gtr", &tun)],
        bars,
        vec![(4, 4); n],
        vec![],
    )
}

/// Power chords: root+fifth and root+fifth+octave, moved up the neck.
pub fn power_chords() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_DROP_D;
    let mut bars = Vec::new();
    for root in [0, 3, 5, 7, 10] {
        let mut beats = Vec::new();
        for two_note in [true, false] {
            for _ in 0..2 {
                let mut notes = vec![
                    fretted(&mut ids, 0, root, &tun, vec![Technique::PalmMute]),
                    fretted(&mut ids, 1, root, &tun, vec![Technique::PalmMute]),
                ];
                if !two_note {
                    notes.push(fretted(&mut ids, 2, root, &tun, vec![Technique::PalmMute]));
                }
                beats.push(beat(&mut ids, Rhythm::new(NoteValue::Quarter), notes));
            }
        }
        bars.push(bar(&mut ids, "G2", beats));
    }
    let n = bars.len();
    assemble(
        "power chords",
        vec![guitar("gtr", &tun)],
        bars,
        vec![(4, 4); n],
        vec![],
    )
}

/// One bar per technique, so a codec that drops one shows a census delta.
pub fn every_technique() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_E_STANDARD;
    let all = vec![
        vec![Technique::PalmMute],
        vec![Technique::Dead],
        vec![Technique::Tapped],
        vec![Technique::HammerOrigin],
        vec![Technique::HammerDestination],
        vec![Technique::LetRing],
        vec![Technique::Vibrato],
        vec![Technique::Slide { flags: 1 }],
        vec![Technique::Slide { flags: 2 }],
        vec![Technique::Bend {
            origin: 0,
            middle: 50,
            dest: 100,
        }],
        vec![Technique::Harmonic {
            kind: HarmonicKind::Natural,
            fret: Some(12),
        }],
        vec![Technique::Harmonic {
            kind: HarmonicKind::Pinch,
            fret: None,
        }],
        vec![Technique::PalmMute, Technique::HammerOrigin],
    ];
    let mut bars = Vec::new();
    for t in all {
        let beats = (0..4)
            .map(|i| {
                let n = fretted(&mut ids, 0, 5 + i, &tun, t.clone());
                beat(&mut ids, Rhythm::new(NoteValue::Quarter), vec![n])
            })
            .collect();
        bars.push(bar(&mut ids, "G2", beats));
    }
    let n = bars.len();
    assemble(
        "techniques",
        vec![guitar("gtr", &tun)],
        bars,
        vec![(4, 4); n],
        vec![],
    )
}

/// Generic drum patterns: straight rock beat, double kick, blast beat, a fill.
pub fn drum_patterns() -> Document {
    use drum::*;
    let mut ids = Ids::default();
    let mut bars = Vec::new();

    // straight beat: hat on eighths, kick 1 and 3, snare 2 and 4
    let mut beats = Vec::new();
    for i in 0..8 {
        let mut notes = vec![hit(&mut ids, HAT_CLOSED)];
        if i % 4 == 0 {
            notes.push(hit(&mut ids, KICK));
        }
        if i % 4 == 2 {
            notes.push(hit(&mut ids, SNARE));
        }
        beats.push(beat(&mut ids, Rhythm::new(NoteValue::Eighth), notes));
    }
    bars.push(bar(&mut ids, "Neutral", beats));

    // double kick: sixteenth kick under a ride
    let mut beats = Vec::new();
    for i in 0..16 {
        let mut notes = vec![hit(&mut ids, KICK)];
        if i % 4 == 0 {
            notes.push(hit(&mut ids, RIDE));
        }
        if i == 8 {
            notes.push(hit(&mut ids, SNARE));
        }
        beats.push(beat(&mut ids, Rhythm::new(NoteValue::Sixteenth), notes));
    }
    bars.push(bar(&mut ids, "Neutral", beats));

    // blast beat: alternating kick and snare on sixteenths
    let mut beats = Vec::new();
    for i in 0..16 {
        let voice = if i % 2 == 0 { KICK } else { SNARE };
        let a = hit(&mut ids, voice);
        let b = hit(&mut ids, HAT_CLOSED);
        let notes = vec![a, b];
        beats.push(beat(&mut ids, Rhythm::new(NoteValue::Sixteenth), notes));
    }
    bars.push(bar(&mut ids, "Neutral", beats));

    // fill: snare into toms, then a crash
    let mut beats = Vec::new();
    for i in 0..16 {
        let v = match i / 4 {
            0 => SNARE,
            1 => TOM_HIGH,
            2 => TOM_FLOOR,
            _ => SNARE,
        };
        let n = hit(&mut ids, v);
        beats.push(beat(&mut ids, Rhythm::new(NoteValue::Sixteenth), vec![n]));
    }
    bars.push(bar(&mut ids, "Neutral", beats));
    let mut beats = Vec::new();
    let c = hit(&mut ids, CRASH);
    let k = hit(&mut ids, KICK);
    beats.push(beat(&mut ids, Rhythm::new(NoteValue::Whole), vec![c, k]));
    bars.push(bar(&mut ids, "Neutral", beats));

    let drums = Track {
        id: 0,
        name: "drums".into(),
        color: Some((0, 0, 255)),
        tuning: vec![],
        midi_program: None,
        pan: None,
        volume: None,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
    };
    let n = bars.len();
    assemble("drum patterns", vec![drums], bars, vec![(4, 4); n], vec![])
}

/// Several time signatures in one document, with section markers.
pub fn meters_and_sections() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_E_STANDARD;
    let meters = [(4u32, 4u32), (3, 4), (6, 8), (7, 8), (5, 4), (12, 8)];
    let mut bars = Vec::new();
    for (num, den) in meters {
        let count = num as usize;
        let value = if den == 8 {
            NoteValue::Eighth
        } else {
            NoteValue::Quarter
        };
        let beats = (0..count)
            .map(|_| {
                let n = fretted(&mut ids, 0, 3, &tun, vec![]);
                beat(&mut ids, Rhythm::new(value), vec![n])
            })
            .collect();
        bars.push(bar(&mut ids, "G2", beats));
    }
    assemble(
        "meters",
        vec![guitar("gtr", &tun)],
        bars,
        meters.to_vec(),
        vec![(0, "Intro"), (2, "Verse"), (4, "Chorus")],
    )
}

/// A bass line, to exercise a 4-string tuning and the bass clef.
pub fn bass_line() -> Document {
    let mut ids = Ids::default();
    let tun = TUNING_BASS;
    let mut bars = Vec::new();
    for root in [0, 5, 7, 3] {
        let beats = (0..8)
            .map(|i| {
                let n = fretted(
                    &mut ids,
                    0,
                    root + if i % 4 == 3 { 2 } else { 0 },
                    &tun,
                    vec![],
                );
                beat(&mut ids, Rhythm::new(NoteValue::Eighth), vec![n])
            })
            .collect();
        bars.push(bar(&mut ids, "F4", beats));
    }
    let track = Track {
        id: 0,
        name: "bass".into(),
        color: Some((0, 255, 0)),
        tuning: tun.to_vec(),
        midi_program: Some(33),
        pan: None,
        volume: None,
        mute: false,
        solo: false,
        percussion_articulations: Vec::new(),
    };
    let n = bars.len();
    assemble("bass", vec![track], bars, vec![(4, 4); n], vec![])
}

/// Every fixture, for tests that want to sweep all of them.
pub fn all() -> Vec<(&'static str, Document)> {
    vec![
        ("all_note_values", all_note_values()),
        ("repeated_frets", repeated_frets()),
        ("scale_run", scale_run()),
        ("power_chords", power_chords()),
        ("every_technique", every_technique()),
        ("drum_patterns", drum_patterns()),
        ("meters_and_sections", meters_and_sections()),
        ("bass_line", bass_line()),
    ]
}
