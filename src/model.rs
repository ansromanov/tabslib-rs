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
/// A written note value, before dots and tuplets are applied.
pub enum NoteValue {
    /// Whole note.
    Whole,
    /// Half note.
    Half,
    /// Quarter note.
    Quarter,
    /// Eighth note.
    Eighth,
    /// Sixteenth note. Spelled `16th` in the file.
    Sixteenth,
    /// Thirty-second note. Spelled `32nd` in the file.
    ThirtySecond,
    /// Sixty-fourth note. Spelled `64th` in the file.
    SixtyFourth,
}

impl NoteValue {
    /// Parses a GPIF note value, accepting both spellings (`16th`, `Sixteenth`).
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
/// A complete written rhythm: value, augmentation dots and tuplet.
///
/// Kept as three separate fields rather than one duration so that no value is
/// ever approximated. [`Rhythm::as_fraction`] does exact integer arithmetic.
pub struct Rhythm {
    /// The written note value.
    pub value: NoteValue,
    /// Augmentation dots; each adds half of the running value.
    pub dots: u8,
    /// `(num, den)` -- three in the time of two is `(3, 2)`.
    pub tuplet: Option<(u32, u32)>,
}

impl Rhythm {
    /// A plain rhythm: no dots, no tuplet.
    pub fn new(value: NoteValue) -> Self {
        Self {
            value,
            dots: 0,
            tuplet: None,
        }
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
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Which kind of harmonic a note sounds.
pub enum HarmonicKind {
    /// Sounded by touching a node of the open string.
    Natural,
    /// Fretted, with the harmonic node touched by the picking hand.
    Artificial,
    /// Squealed with the edge of the thumb while picking.
    Pinch,
    /// Sounded by tapping the node.
    Tap,
    /// Half-harmonic, partially damped.
    Semi,
    /// Sustained by amplifier feedback.
    Feedback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// A playing technique attached to a note.
///
/// The set is closed and matches are exhaustive on purpose: adding a variant
/// must break the build at every site that writes one, so a new technique
/// cannot be silently dropped on save.
pub enum Technique {
    /// Damped with the picking-hand palm at the bridge.
    PalmMute,
    /// Fretting hand muted; a pitchless click.
    Dead,
    /// Played louder than its neighbours.
    Accent,
    /// Played much quieter than its neighbours.
    Ghost,
    /// Shortened, detached from what follows.
    Staccato,
    /// Allowed to sustain past its written value.
    LetRing,
    /// Pitch oscillated by the fretting hand.
    Vibrato,
    /// Sounded by tapping the fret rather than picking.
    Tapped,
    /// Start of a hammer-on or pull-off; the next note on this string is slurred.
    HammerOrigin,
    /// End of a hammer-on or pull-off.
    HammerDestination,
    /// Slid to or from another fret.
    ///
    /// `flags` is the file's own bitfield: direction and kind are encoded
    /// together, so it is carried verbatim rather than interpreted.
    Slide {
        /// Raw GPIF slide bitfield.
        flags: u32,
    },
    /// Pitch bent by the fretting hand, as a three-point curve.
    Bend {
        /// Bend amount at the start of the note.
        origin: i32,
        /// Bend amount at the midpoint.
        middle: i32,
        /// Bend amount at the end.
        dest: i32,
    },
    /// Sounded as a harmonic.
    Harmonic {
        /// Which kind of harmonic.
        kind: HarmonicKind,
        /// Node fret, for artificial and tapped harmonics.
        fret: Option<i32>,
    },
}

#[derive(Debug, Clone, PartialEq)]
/// One sounding note.
pub struct Note {
    /// Identity within the document.
    pub id: u32,
    /// Sounding MIDI pitch, where the file states one.
    pub midi: Option<i32>,
    /// String number, counted from the highest-pitched string.
    pub string: Option<u32>,
    /// Fret number; 0 is the open string.
    pub fret: Option<i32>,
    /// Percussion instrument identity where the staff is a drum kit.
    pub articulation: Option<i32>,
    /// Playing techniques applied to this note.
    pub techniques: Vec<Technique>,
}

#[derive(Debug, Clone, PartialEq)]
/// One rhythmic position, sounding zero or more notes together.
pub struct Beat {
    /// Identity within the document.
    pub id: u32,
    /// How long this beat lasts.
    pub rhythm: Rhythm,
    /// Notes sounding together here; empty is a rest.
    pub notes: Vec<Note>,
    /// Written dynamic, such as `MF`.
    pub dynamic: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
/// An independent rhythmic line within a bar.
pub struct Voice {
    /// Identity within the document.
    pub id: u32,
    /// Beats in playing order.
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, PartialEq)]
/// One bar of one track.
pub struct Bar {
    /// Identity within the document; referenced from [`MasterBar::bar_ids`].
    pub id: u32,
    /// Clef, as GPIF spells it (`G2`, `F4`, `Neutral`).
    pub clef: Option<String>,
    /// Independent rhythmic lines in this bar.
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, PartialEq)]
/// Score-wide properties of a bar position: meter, section marker, barline.
pub struct MasterBar {
    /// Position in the score, from zero.
    pub index: usize,
    /// Time signature as `(numerator, denominator)`.
    pub time: (u32, u32),
    /// Section marker starting at this bar, if any.
    pub section: Option<String>,
    /// Whether a double barline is drawn here.
    pub double_bar: bool,
    /// One [`Bar`] id per track, in track order.
    pub bar_ids: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
/// One instrument part.
pub struct Track {
    /// Identity within the document.
    pub id: u32,
    /// Track name as shown in the editor.
    pub name: String,
    /// Display colour as RGB.
    pub color: Option<(u8, u8, u8)>,
    /// Open-string MIDI pitches, lowest string first. Empty for percussion.
    pub tuning: Vec<i32>,
    /// General MIDI program number.
    pub midi_program: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
/// A whole score.
///
/// Nodes reference each other by id; there are no parent or sibling pointers,
/// so the document stays a tree and edits stay value-shaped.
pub struct Document {
    /// Score title.
    pub title: String,
    /// Score artist.
    pub artist: String,
    /// Instrument parts.
    pub tracks: Vec<Track>,
    /// Score-wide bar properties, in order.
    pub master_bars: Vec<MasterBar>,
    /// Indexed by `Bar` id, matching the `<Bars>` table in the file.
    pub bars: Vec<Bar>,
}

impl Document {
    /// Total sounding notes across every track.
    pub fn note_count(&self) -> usize {
        self.bars
            .iter()
            .flat_map(|b| &b.voices)
            .flat_map(|v| &v.beats)
            .map(|b| b.notes.len())
            .sum()
    }
    /// Number of bars carrying a section marker.
    pub fn section_count(&self) -> usize {
        self.master_bars
            .iter()
            .filter(|m| m.section.is_some())
            .count()
    }
    /// Counts notes whose techniques match `want`.
    ///
    /// A census like this is the assertion that catches a codec silently
    /// dropping an articulation; a note count would not move.
    pub fn technique_count(&self, want: fn(&Technique) -> bool) -> usize {
        self.bars
            .iter()
            .flat_map(|b| &b.voices)
            .flat_map(|v| &v.beats)
            .flat_map(|b| &b.notes)
            .flat_map(|n| &n.techniques)
            .filter(|t| want(t))
            .count()
    }
}
