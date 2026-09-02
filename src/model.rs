//! The document model.
//!
//! Two rules carried over from the TypeScript engine's defect history:
//!
//! 1. **Rhythm is never a float.** Collapsing a duration to `f64` is how the
//!    previous writer turned 5072 eighths, 2377 sixteenths and 524 triplets into
//!    9229 quarter notes: anything not in a lookup table silently became a
//!    quarter. `Rhythm` here keeps the written value, the dots and the tuplet
//!    separately, so a value that cannot be represented is a compile-time or
//!    parse-time problem rather than a silent quarter note.
//!
//! 2. **Nodes reference by id, never by pointer.** No `parent`, `next` or
//!    `previous` links. Ownership stays a tree and edits stay value-shaped.

/// Written note value. `Sixteenth` serialises as `16th`, which is what Guitar
/// Pro itself writes -- a writer that emits `Sixteenth` produces a file Guitar
/// Pro will not read back the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "Whole" => Self::Whole,
            "Half" => Self::Half,
            "Quarter" => Self::Quarter,
            "Eighth" => Self::Eighth,
            "Sixteenth" | "16th" => Self::Sixteenth,
            "ThirtySecond" | "32nd" => Self::ThirtySecond,
            "SixtyFourth" | "64th" => Self::SixtyFourth,
            _ => return None,
        })
    }
    /// The spelling Guitar Pro writes.
    pub fn as_gpif(self) -> &'static str {
        match self {
            Self::Whole => "Whole",
            Self::Half => "Half",
            Self::Quarter => "Quarter",
            Self::Eighth => "Eighth",
            Self::Sixteenth => "16th",
            Self::ThirtySecond => "32nd",
            Self::SixtyFourth => "64th",
        }
    }
    /// Denominator of the plain value: Quarter -> 4.
    pub fn denominator(self) -> u32 {
        match self {
            Self::Whole => 1,
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
            Self::Sixteenth => 16,
            Self::ThirtySecond => 32,
            Self::SixtyFourth => 64,
        }
    }
}

/// A written rhythm: value, augmentation dots, and an optional tuplet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rhythm {
    pub value: NoteValue,
    pub dots: u8,
    /// `(num, den)` -- three in the time of two is `(3, 2)`.
    pub tuplet: Option<(u32, u32)>,
}

impl Rhythm {
    pub fn new(value: NoteValue) -> Self {
        Self { value, dots: 0, tuplet: None }
    }
    /// Exact duration in whole notes, as a fraction. No floating point.
    pub fn as_fraction(&self) -> (u64, u64) {
        let mut num: u64 = 1;
        let mut den: u64 = self.value.denominator() as u64;
        // each dot adds half of the running value: 1 dot = 3/2, 2 dots = 7/4
        if self.dots > 0 {
            let d = self.dots as u32;
            num *= (1u64 << d) * 2 - 1;
            den *= 1u64 << d;
        }
        if let Some((tn, td)) = self.tuplet {
            num *= td as u64;
            den *= tn as u64;
        }
        let g = gcd(num, den);
        (num / g, den / g)
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HarmonicKind { Natural, Artificial, Pinch, Tap, Semi, Feedback }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Technique {
    PalmMute,
    Dead,
    Accent,
    Ghost,
    Staccato,
    LetRing,
    Vibrato,
    Tapped,
    HammerOrigin,
    HammerDestination,
    Slide { flags: u32 },
    Bend { origin: i32, middle: i32, dest: i32 },
    Harmonic { kind: HarmonicKind, fret: Option<i32> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: u32,
    pub midi: Option<i32>,
    pub string: Option<u32>,
    pub fret: Option<i32>,
    /// Percussion instrument identity where the staff is a drum kit.
    pub articulation: Option<i32>,
    pub techniques: Vec<Technique>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Beat {
    pub id: u32,
    pub rhythm: Rhythm,
    pub notes: Vec<Note>,
    pub dynamic: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Voice { pub id: u32, pub beats: Vec<Beat> }

#[derive(Debug, Clone, PartialEq)]
pub struct Bar { pub id: u32, pub clef: Option<String>, pub voices: Vec<Voice> }

#[derive(Debug, Clone, PartialEq)]
pub struct MasterBar {
    pub index: usize,
    pub time: (u32, u32),
    pub section: Option<String>,
    pub double_bar: bool,
    pub bar_ids: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: u32,
    pub name: String,
    pub color: Option<(u8, u8, u8)>,
    pub tuning: Vec<i32>,
    pub midi_program: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub title: String,
    pub artist: String,
    pub tracks: Vec<Track>,
    pub master_bars: Vec<MasterBar>,
    /// Indexed by `Bar` id, matching the `<Bars>` table in the file.
    pub bars: Vec<Bar>,
}

impl Document {
    pub fn note_count(&self) -> usize {
        self.bars.iter().flat_map(|b| &b.voices).flat_map(|v| &v.beats).map(|b| b.notes.len()).sum()
    }
    pub fn section_count(&self) -> usize {
        self.master_bars.iter().filter(|m| m.section.is_some()).count()
    }
    pub fn technique_count(&self, want: fn(&Technique) -> bool) -> usize {
        self.bars.iter().flat_map(|b| &b.voices).flat_map(|v| &v.beats)
            .flat_map(|b| &b.notes).flat_map(|n| &n.techniques).filter(|t| want(t)).count()
    }
}
