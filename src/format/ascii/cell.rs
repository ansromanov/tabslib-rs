//! What one note looks like on a tablature line.
//!
//! Owns [`Cell`], [`Connector`] and [`Annotation`]. A cell's text is the fret
//! with its decorations; everything a renderer might style differently stays on
//! the cell as the note's own [`Technique`] list, so no styling decision is
//! made here.
//!
//! Text rules, fretted notes:
//!
//! | note | text |
//! | --- | --- |
//! | plain | `12` |
//! | ghost, or tied from the previous note | `(5)` |
//! | dead | `x` |
//! | natural harmonic | `<7>` |
//! | any other harmonic | `[7]` |
//! | bend of whole semitones | `7b9`; with release `7b9r7`; pre-bend `7pb9` |
//! | bend by a fraction of a tone | `7b1/4` (quarter tone), `7b3/4` |
//! | slide in from below / above | `/7`, `\7` |
//!
//! Drum hits read `x` for cymbals, hi-hat and ride and `o` for everything else,
//! upper-cased when accented and parenthesised when ghosted.

use crate::model::{HarmonicKind, Note, Technique};
use crate::percussion::DrumRole;

/// A connecting mark drawn in the gap right after a cell, towards the next note
/// on the same line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connector {
    /// Hammer-on to a higher fret: `h`.
    HammerOn,
    /// Pull-off to a lower fret: `p`.
    PullOff,
    /// Slide up, into the next note or out of this one: `/`.
    SlideUp,
    /// Slide down, into the next note or out of this one: `\`.
    SlideDown,
}

impl Connector {
    /// The ASCII mark for this connector.
    pub fn symbol(self) -> char {
        match self {
            Self::HammerOn => 'h',
            Self::PullOff => 'p',
            Self::SlideUp => '/',
            Self::SlideDown => '\\',
        }
    }
}

/// A technique shown on an annotation row above the staff rather than in the
/// cell text, as printed tablature does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Annotation {
    /// Palm mute: `PM`.
    PalmMute,
    /// Let ring: `LR`.
    LetRing,
    /// Vibrato: `~`.
    Vibrato,
    /// Tapped with the picking hand: `T`.
    Tapped,
    /// Accented: `>`.
    Accent,
    /// Staccato: `.`.
    Staccato,
    /// A non-natural harmonic, named by kind: `AH`, `PH`, `TH`, `SH`, `FH`.
    /// Natural harmonics are marked in the cell as `<7>` instead.
    Harmonic(HarmonicKind),
}

impl Annotation {
    /// Short row label for this annotation.
    pub fn label(self) -> &'static str {
        match self {
            Self::PalmMute => "PM",
            Self::LetRing => "LR",
            Self::Vibrato => "~",
            Self::Tapped => "T",
            Self::Accent => ">",
            Self::Staccato => ".",
            Self::Harmonic(kind) => match kind {
                HarmonicKind::Natural => "NH",
                HarmonicKind::Artificial => "AH",
                HarmonicKind::Pinch => "PH",
                HarmonicKind::Tap => "TH",
                HarmonicKind::Semi => "SH",
                HarmonicKind::Feedback => "FH",
            },
        }
    }
}

/// One note placed on one line of a beat.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Display text, ASCII only; see the module docs for the rules.
    pub text: String,
    /// Fret number, for fretted notes.
    pub fret: Option<i32>,
    /// Sounding MIDI pitch, where the document states one.
    pub pitch: Option<i32>,
    /// Index of the voice the note belongs to.
    pub voice: usize,
    /// The note's techniques, verbatim, for styling.
    pub techniques: Vec<Technique>,
    /// Mark to draw in the gap after the cell, towards the next note on the line.
    pub connector: Option<Connector>,
}

impl Cell {
    /// Width of [`Cell::text`] in columns.
    pub fn width(&self) -> usize {
        self.text.chars().count()
    }

    /// Whether the note is tied from the previous note on its string.
    pub fn is_tie(&self) -> bool {
        self.techniques.contains(&Technique::TieDestination)
    }
}

/// Slide flag bits as the model carries them (see [`Technique::Slide`]).
const SLIDE_SHIFT: u32 = 1;
const SLIDE_LEGATO: u32 = 2;
const SLIDE_OUT_DOWN: u32 = 4;
const SLIDE_OUT_UP: u32 = 8;
const SLIDE_IN_FROM_BELOW: u32 = 16;
const SLIDE_IN_FROM_ABOVE: u32 = 32;

/// Whether a slide connects this note to the next one on its string.
pub(crate) fn slides_to_next(flags: u32) -> bool {
    flags & (SLIDE_SHIFT | SLIDE_LEGATO) != 0
}

/// Connector for a slide that leaves the note without a target note.
fn slide_out(flags: u32) -> Option<Connector> {
    if flags & SLIDE_OUT_DOWN != 0 {
        Some(Connector::SlideDown)
    } else if flags & SLIDE_OUT_UP != 0 {
        Some(Connector::SlideUp)
    } else {
        None
    }
}

/// Bend amount in the model's units (100 is a whole tone) as display text:
/// a fret when it is whole semitones, otherwise a fraction of a tone.
fn bend_target(fret: i32, amount: i32) -> String {
    if amount % 50 == 0 {
        return (fret + amount / 50).to_string();
    }
    let whole = amount / 100;
    let quarters = (amount % 100) / 25;
    let fraction = match quarters {
        1 => "1/4",
        2 => "1/2",
        3 => "3/4",
        _ => return format!("{}.{:02}", whole, amount % 100),
    };
    if whole == 0 {
        fraction.to_string()
    } else {
        format!("{whole} {fraction}")
    }
}

fn bend_text(fret: i32, origin: i32, middle: i32, dest: i32) -> String {
    if origin > 0 {
        let mut text = format!("{fret}pb{}", bend_target(fret, origin));
        if dest < origin {
            text.push_str(&format!("r{}", bend_target(fret, dest)));
        } else if dest > origin {
            text.push_str(&format!("b{}", bend_target(fret, dest)));
        }
        return text;
    }
    if middle > dest {
        return format!(
            "{fret}b{}r{}",
            bend_target(fret, middle),
            bend_target(fret, dest)
        );
    }
    format!("{fret}b{}", bend_target(fret, dest.max(middle)))
}

/// A cell for a fretted note. The connector for hammer-ons and shift or legato
/// slides depends on the next note and is filled in by the layout pass.
pub(crate) fn fretted(note: &Note, voice: usize) -> Cell {
    let fret = note.fret;
    let mut text = fret.map_or_else(|| "?".to_string(), |fret| fret.to_string());
    let mut prefix = "";
    let mut connector = None;
    let mut wrapped = false;
    for technique in &note.techniques {
        match *technique {
            Technique::Ghost | Technique::TieDestination => wrapped = true,
            Technique::Harmonic { kind, .. } => {
                text = match kind {
                    HarmonicKind::Natural => format!("<{text}>"),
                    HarmonicKind::Artificial
                    | HarmonicKind::Pinch
                    | HarmonicKind::Tap
                    | HarmonicKind::Semi
                    | HarmonicKind::Feedback => format!("[{text}]"),
                }
            }
            Technique::Bend {
                origin,
                middle,
                dest,
            } => {
                if let Some(fret) = fret {
                    text = bend_text(fret, origin, middle, dest);
                }
            }
            Technique::Slide { flags } => {
                if flags & SLIDE_IN_FROM_BELOW != 0 {
                    prefix = "/";
                } else if flags & SLIDE_IN_FROM_ABOVE != 0 {
                    prefix = "\\";
                }
                if !slides_to_next(flags) {
                    connector = slide_out(flags);
                }
            }
            Technique::PalmMute
            | Technique::Dead
            | Technique::Accent
            | Technique::Staccato
            | Technique::LetRing
            | Technique::TieOrigin
            | Technique::Vibrato
            | Technique::Tapped
            | Technique::HammerOrigin
            | Technique::HammerDestination => {}
        }
    }
    if note.techniques.contains(&Technique::Dead) {
        text = "x".to_string();
    } else if wrapped {
        text = format!("({text})");
    }
    Cell {
        text: format!("{prefix}{text}"),
        fret,
        pitch: note.midi,
        voice,
        techniques: note.techniques.clone(),
        connector,
    }
}

/// A cell for a drum hit.
pub(crate) fn drum(note: &Note, role: Option<DrumRole>, voice: usize) -> Cell {
    let metal = matches!(
        role,
        Some(DrumRole::Cymbal | DrumRole::HiHat | DrumRole::Ride)
    );
    let mut text = if metal { "x" } else { "o" }.to_string();
    if note.techniques.contains(&Technique::Accent) {
        text = text.to_ascii_uppercase();
    }
    if note.techniques.contains(&Technique::Ghost) {
        text = format!("({text})");
    }
    Cell {
        text,
        fret: None,
        pitch: note.midi,
        voice,
        techniques: note.techniques.clone(),
        connector: None,
    }
}

/// Annotation-row marks a note carries.
pub(crate) fn annotations(note: &Note) -> Vec<Annotation> {
    let mut out = Vec::new();
    for technique in &note.techniques {
        let annotation = match *technique {
            Technique::PalmMute => Some(Annotation::PalmMute),
            Technique::LetRing => Some(Annotation::LetRing),
            Technique::Vibrato => Some(Annotation::Vibrato),
            Technique::Tapped => Some(Annotation::Tapped),
            Technique::Accent => Some(Annotation::Accent),
            Technique::Staccato => Some(Annotation::Staccato),
            Technique::Harmonic { kind, .. } => match kind {
                HarmonicKind::Natural => None,
                HarmonicKind::Artificial
                | HarmonicKind::Pinch
                | HarmonicKind::Tap
                | HarmonicKind::Semi
                | HarmonicKind::Feedback => Some(Annotation::Harmonic(kind)),
            },
            Technique::Dead
            | Technique::Ghost
            | Technique::TieOrigin
            | Technique::TieDestination
            | Technique::HammerOrigin
            | Technique::HammerDestination
            | Technique::Slide { .. }
            | Technique::Bend { .. } => None,
        };
        if let Some(annotation) = annotation {
            out.push(annotation);
        }
    }
    out
}

#[cfg(test)]
#[path = "cell_test.rs"]
mod tests;
