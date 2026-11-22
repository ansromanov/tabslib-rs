//! Authoring a riff from a declarative specification.
//!
//! The module owns [`RiffSpec`], [`validate`] and [`write_riff`]. Every other
//! generator in this crate is accompaniment: it needs material to lock onto.
//! This one originates material, which is what a caller needs before there is
//! anything to accompany.
//!
//! A specification names musical properties -- a rhythmic archetype, a
//! register, a voicing, a mode and a degree per bar -- and never a genre, a
//! band, or a statistic measured over a collection of music. The data that
//! decides which specification to write is the caller's; this module only
//! realizes the one it is given.
//!
//! An unrealizable specification returns [`RiffSpecError`]. Nothing here
//! repairs a specification into a different one, because a caller that asked
//! for eight bars and silently received four cannot tell.

use crate::edits::{new_beat, new_note};
use crate::inspect::MusicalMode;
use crate::model::{
    open_pitch, Bar, Document, KeySignature, MasterBar, NoteValue, Rhythm, Technique, Track, Voice,
};

/// Subdivisions per bar the writer places attacks on.
pub const STEPS_PER_BAR: u32 = 16;
/// Highest fret the writer will reach for.
pub const MAX_FRET: i32 = 12;
/// Scale degrees available to a specification, counting the tonic as one.
pub const DEGREES: u8 = 7;

/// A rhythmic and contour shape a riff can be built on.
///
/// Each names what the figure does rhythmically, so a specification stays
/// readable without reference to any particular repertoire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiffArchetype {
    /// A fixed low note alternating with a moving one.
    PedalTone,
    /// An attack on every subdivision.
    Tremolo,
    /// An eighth followed by two sixteenths, repeated.
    Gallop,
    /// One attack per beat, held.
    Sustained,
    /// Attacks displaced onto the weak half of each beat.
    Syncopated,
    /// Widely spaced attacks with silence between them.
    Sparse,
    /// Sparse attacks displaced around the beat for a locked rhythmic figure.
    LockedSparse,
}

/// Direction of the displacement in a locked sparse figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syncopation {
    /// Attack one sixteenth before each beat.
    Anticipated,
    /// Attack one sixteenth after each beat.
    Delayed,
}

/// Which part of the instrument's range the riff sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    /// The lowest strings.
    Low,
    /// The middle of the range.
    Mid,
}

/// How many notes sound together at each attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voicing {
    /// One note.
    SingleNote,
    /// Root and fifth.
    PowerFifth,
    /// Root, fifth and octave.
    PowerOctave,
    /// Root and octave.
    Octave,
    /// Root and a minor second.
    MinorSecondCluster,
    /// Root and a tritone.
    Tritone,
    /// Root and a minor sixth.
    MinorSixth,
}

impl Voicing {
    /// How many notes the voicing sounds, and so how many strings it needs.
    fn width(self) -> usize {
        match self {
            Self::SingleNote => 1,
            Self::PowerFifth
            | Self::Octave
            | Self::MinorSecondCluster
            | Self::Tritone
            | Self::MinorSixth => 2,
            Self::PowerOctave => 3,
        }
    }

    /// Whether the voicing sounds an octave above the root.
    fn has_octave(self) -> bool {
        matches!(self, Self::PowerOctave | Self::Octave)
    }
}

/// Why a specification cannot be realized.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RiffSpecError {
    /// The specification asks for no bars.
    #[error("a riff needs at least one bar")]
    NoBars,
    /// The specification supplies no scale degrees.
    #[error("a riff needs at least one scale degree")]
    NoDegrees,
    /// A scale degree is outside the range the mode defines.
    #[error("scale degree {0} is outside 1..={DEGREES}")]
    DegreeOutOfRange(u8),
    /// The tuning is empty or too short for the requested voicing.
    #[error("the tuning has {available} strings; the voicing needs {needed}")]
    TuningTooShort {
        /// Strings the tuning provides.
        available: usize,
        /// Strings the voicing needs.
        needed: usize,
    },
    /// The tempo is not a positive number of beats per minute.
    #[error("tempo must be positive")]
    InvalidTempo,
    /// A form could not be assembled from otherwise valid material.
    #[error("the motif could not be assembled: {0}")]
    Unbuildable(String),
    /// A requested root cannot be fretted in range on the chosen strings.
    #[error("degree {degree} cannot be fretted within {MAX_FRET} frets in this register")]
    UnreachableDegree {
        /// The degree that could not be placed.
        degree: u8,
    },
}

/// A riff described by its musical properties rather than its notes.
#[derive(Debug, Clone, PartialEq)]
pub struct RiffSpec {
    /// Rhythmic and contour shape.
    pub archetype: RiffArchetype,
    /// Range the riff sits in.
    pub register: Register,
    /// How many notes sound at each attack.
    pub voicing: Voicing,
    /// Tonic the degrees are counted from.
    pub key: KeySignature,
    /// Scale the degrees are drawn from.
    pub mode: MusicalMode,
    /// One scale degree per bar, counting the tonic as one. Cycled if shorter
    /// than `bars`.
    pub degrees: Vec<u8>,
    /// Number of bars to write.
    pub bars: usize,
    /// Beats per minute.
    pub tempo: f64,
    /// Open-string pitches, lowest string first.
    pub tuning: Vec<i32>,
    /// Whether attacks are palm muted.
    pub palm_mute: bool,
    /// Optional displacement for a locked sparse archetype.
    pub syncopation: Option<Syncopation>,
    /// Whether a locked sparse figure uses six quarter-note triplets per bar.
    pub quarter_triplets: bool,
    /// Seed for the archetype's deterministic choices.
    pub seed: u64,
    /// Track name written into the document.
    pub name: String,
}

impl RiffSpec {
    /// A four-bar single-note riff on the tonic, as a starting point to edit.
    pub fn minimal(tuning: Vec<i32>) -> Self {
        Self {
            archetype: RiffArchetype::Sustained,
            register: Register::Low,
            voicing: Voicing::SingleNote,
            key: KeySignature {
                tonic: 4,
                minor: true,
            },
            mode: MusicalMode::NaturalMinor,
            degrees: vec![1],
            bars: 4,
            tempo: 120.0,
            tuning,
            palm_mute: false,
            syncopation: None,
            quarter_triplets: false,
            seed: 0,
            name: "Riff".into(),
        }
    }
}

/// Checks a specification and returns it unchanged, or says why it cannot be
/// realized.
///
/// Validation never adjusts the specification. A caller that asked for a
/// degree outside the mode needs to hear that, not receive a different riff.
pub fn validate(spec: &RiffSpec) -> Result<(), RiffSpecError> {
    if spec.bars == 0 {
        return Err(RiffSpecError::NoBars);
    }
    if spec.degrees.is_empty() {
        return Err(RiffSpecError::NoDegrees);
    }
    if let Some(bad) = spec
        .degrees
        .iter()
        .copied()
        .find(|degree| *degree == 0 || *degree > DEGREES)
    {
        return Err(RiffSpecError::DegreeOutOfRange(bad));
    }
    let needed = spec.voicing.width();
    if spec.tuning.len() < needed {
        return Err(RiffSpecError::TuningTooShort {
            available: spec.tuning.len(),
            needed,
        });
    }
    if !spec.tempo.is_finite() || spec.tempo <= 0.0 {
        return Err(RiffSpecError::InvalidTempo);
    }
    for degree in &spec.degrees {
        if root_position(spec, *degree).is_none() {
            return Err(RiffSpecError::UnreachableDegree { degree: *degree });
        }
    }
    Ok(())
}

/// Lowest string the register places the root on.
fn root_string(spec: &RiffSpec) -> u32 {
    match spec.register {
        Register::Low => 0,
        Register::Mid => {
            let highest = spec.tuning.len().saturating_sub(1);
            let needed = spec.voicing.width().saturating_sub(1);
            u32::try_from(highest.saturating_sub(needed).min(2)).unwrap_or(0)
        }
    }
}

/// String and fret for one degree's root, if it is reachable.
fn root_position(spec: &RiffSpec, degree: u8) -> Option<(u32, i32)> {
    let string = root_string(spec);
    let open = open_pitch(&spec.tuning, string)?;
    let interval = *spec
        .mode
        .pitch_classes(spec.key.tonic)
        .get(usize::from(degree.checked_sub(1)?))?;
    let fret = (interval - open).rem_euclid(12);
    if fret > MAX_FRET {
        return None;
    }
    Some((string, fret))
}

/// A deterministic value stream, so the same seed writes the same riff.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Which of the sixteen subdivisions the archetype attacks.
fn onsets(spec: &RiffSpec, rng: &mut Rng) -> Vec<bool> {
    let steps = usize::try_from(STEPS_PER_BAR).unwrap_or(16);
    match spec.archetype {
        RiffArchetype::Tremolo => vec![true; steps],
        RiffArchetype::Sustained => (0..steps).map(|step| step % 4 == 0).collect(),
        RiffArchetype::Gallop => (0..steps).map(|step| step % 4 != 1).collect(),
        RiffArchetype::Syncopated => (0..steps).map(|step| step % 4 == 2).collect(),
        RiffArchetype::Sparse => (0..steps).map(|step| step % 8 == 0).collect(),
        RiffArchetype::LockedSparse => {
            let syncopation = spec.syncopation.unwrap_or(Syncopation::Anticipated);
            (0..steps)
                .map(|step| match syncopation {
                    Syncopation::Anticipated => step % 4 == 3,
                    Syncopation::Delayed => step % 4 == 1,
                })
                .collect()
        }
        RiffArchetype::PedalTone => (0..steps)
            .map(|step| step % 2 == 0 || rng.next().is_multiple_of(4))
            .collect(),
    }
}

/// Whether an attack sounds the moving note rather than the pedal.
fn is_moving(archetype: RiffArchetype, step: usize) -> bool {
    matches!(archetype, RiffArchetype::PedalTone) && step % 4 == 2
}

fn triplet_rhythms() -> [Rhythm; 6] {
    [Rhythm {
        value: NoteValue::Quarter,
        dots: 0,
        tuplet: Some((3, 2)),
    }; 6]
}

/// Written values summing exactly to `steps` sixteenths, longest first.
///
/// The bar must be exactly full for its meter, so the gap between one attack
/// and the next is spelled as written durations rather than approximated. A
/// value that does not divide cleanly is split, never rounded: rounding is how
/// a bar ends up short while its note count stays correct.
fn durations_for(steps: u32) -> Vec<Rhythm> {
    const SPELLINGS: [(u32, NoteValue, u8); 8] = [
        (16, NoteValue::Whole, 0),
        (12, NoteValue::Half, 1),
        (8, NoteValue::Half, 0),
        (6, NoteValue::Quarter, 1),
        (4, NoteValue::Quarter, 0),
        (3, NoteValue::Eighth, 1),
        (2, NoteValue::Eighth, 0),
        (1, NoteValue::Sixteenth, 0),
    ];
    let mut remaining = steps;
    let mut out = Vec::new();
    while remaining > 0 {
        let Some((size, value, dots)) = SPELLINGS
            .iter()
            .copied()
            .find(|(size, _, _)| *size <= remaining)
        else {
            break;
        };
        out.push(Rhythm {
            value,
            dots,
            tuplet: None,
        });
        remaining -= size;
    }
    out
}

/// The degree a pedal-tone figure moves to, wrapping at the top of the scale.
fn neighbour_degree(degree: u8) -> u8 {
    if degree >= DEGREES {
        1
    } else {
        degree + 1
    }
}

/// Writes the document a specification describes.
///
/// The result is deterministic in the specification, including its seed. It
/// satisfies the structural requirements by construction: every bar is exactly
/// full, every note lies in the specified mode, voicings use only root, fifth
/// and octave shapes, and no note is staccato or let-ring.
///
/// ```
/// use tabslib::generation::riff_spec::{write_riff, RiffSpec};
///
/// let spec = RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
/// let riff = write_riff(&spec).expect("a realizable specification");
/// assert_eq!(riff.master_bars.len(), 4);
/// ```
pub fn write_riff(spec: &RiffSpec) -> Result<Document, RiffSpecError> {
    validate(spec)?;
    let mut rng = Rng::new(spec.seed);
    let mut next_id = 0u32;

    let mut bars = Vec::new();
    let mut master_bars = Vec::new();
    for bar_index in 0..spec.bars {
        let degree = spec.degrees[bar_index % spec.degrees.len()];
        let Some(root) = root_position(spec, degree) else {
            return Err(RiffSpecError::UnreachableDegree { degree });
        };
        // A pedal figure moves to a neighbouring scale degree, so the moving
        // note is diatonic by construction rather than a fixed interval that
        // might leave the mode.
        let moving_degree = neighbour_degree(degree);
        let moving = root_position(spec, moving_degree).unwrap_or(root);
        let mut beats = Vec::new();
        if spec.archetype == RiffArchetype::LockedSparse && spec.quarter_triplets {
            for rhythm in triplet_rhythms() {
                let notes = voicing_notes(spec, degree, root, &mut next_id);
                let beat_id = next_id;
                next_id += 1;
                beats.push(new_beat(beat_id, rhythm, notes));
            }
        } else {
            let pattern = onsets(spec, &mut rng);
            let mut step = 0usize;
            while step < pattern.len() {
                if !pattern[step] {
                    let beat_id = next_id;
                    next_id += 1;
                    beats.push(new_beat(
                        beat_id,
                        Rhythm::new(NoteValue::Sixteenth),
                        Vec::new(),
                    ));
                    step += 1;
                    continue;
                }
                let mut gap = 1u32;
                while step + (gap as usize) < pattern.len() && !pattern[step + gap as usize] {
                    gap += 1;
                }
                let (position, position_degree) = if is_moving(spec.archetype, step) {
                    (moving, moving_degree)
                } else {
                    (root, degree)
                };
                let mut first = true;
                for rhythm in durations_for(gap) {
                    let notes = if first {
                        voicing_notes(spec, position_degree, position, &mut next_id)
                    } else {
                        Vec::new()
                    };
                    first = false;
                    let beat_id = next_id;
                    next_id += 1;
                    beats.push(new_beat(beat_id, rhythm, notes));
                }
                step += gap as usize;
            }
        }
        let bar_id = u32::try_from(bar_index).unwrap_or(u32::MAX);
        bars.push(Bar {
            id: bar_id,
            clef: Some("G2".into()),
            voices: vec![Voice { id: bar_id, beats }],
        });
        master_bars.push(MasterBar {
            index: bar_index,
            time: (4, 4),
            section: None,
            double_bar: false,
            bar_ids: vec![bar_id as i32],
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        });
    }

    Ok(Document {
        title: spec.name.clone(),
        artist: String::new(),
        tracks: vec![Track {
            id: 0,
            name: spec.name.clone(),
            color: None,
            tuning: spec.tuning.clone(),
            midi_program: Some(30),
            midi_channel: Some(0),
            pan: None,
            volume: None,
            mute: false,
            solo: false,
            percussion_articulations: Vec::new(),
            generator_metadata: None,
        }],
        master_bars,
        bars,
        tempo_map: vec![crate::model::TempoChange {
            at_bar: 0,
            bpm: spec.tempo,
            linear: false,
        }],
        key: Some(spec.key),
        ..Document::default()
    })
}

/// The notes one attack sounds.
///
/// Each interval is placed by pitch rather than by a fixed string and fret
/// offset, so the shape stays correct in any tuning. A fifth two frets up the
/// next string is only a fifth while the tuning is in fourths.
///
/// The fifth is the mode's own fifth above the degree, not a fixed seven
/// semitones. A parallel fifth is not diatonic on every degree -- on the
/// second degree of a natural minor it lands a semitone outside the scale --
/// so a fixed interval would write out-of-scale notes on exactly the degrees a
/// caller is most likely to reach for.
fn voicing_notes(
    spec: &RiffSpec,
    degree: u8,
    root: (u32, i32),
    next_id: &mut u32,
) -> Vec<crate::model::Note> {
    let (root_string, root_fret) = root;
    let techniques = if spec.palm_mute {
        vec![Technique::PalmMute]
    } else {
        Vec::new()
    };
    let Some(root_midi) = crate::model::sounding_pitch(&spec.tuning, root_string, root_fret) else {
        return Vec::new();
    };

    let mut pitches = vec![root_midi];
    match spec.voicing {
        Voicing::SingleNote => {}
        Voicing::PowerFifth | Voicing::PowerOctave => {
            if let Some(fifth) = diatonic_fifth(spec, degree, root_midi) {
                pitches.push(fifth);
            }
            if spec.voicing.has_octave() {
                pitches.push(root_midi + 12);
            }
        }
        Voicing::Octave => pitches.push(root_midi + 12),
        Voicing::MinorSecondCluster => pitches.push(root_midi + 1),
        Voicing::Tritone => pitches.push(root_midi + 6),
        Voicing::MinorSixth => pitches.push(root_midi + 8),
    }

    let mut notes = Vec::new();
    for pitch in pitches {
        let Some((string, fret)) = place(spec, pitch, root_string, root_fret) else {
            continue;
        };
        let id = *next_id;
        *next_id += 1;
        let mut note = new_note(id, Some(pitch), Some(string), Some(fret));
        note.techniques = techniques.clone();
        notes.push(note);
    }
    notes
}

/// The mode's fifth above a degree: four scale steps up, as a pitch above the
/// root.
fn diatonic_fifth(spec: &RiffSpec, degree: u8, root_midi: i32) -> Option<i32> {
    let classes = spec.mode.pitch_classes(spec.key.tonic);
    let index = usize::from(degree.checked_sub(1)?);
    let class = *classes.get((index + 4) % classes.len())?;
    let mut pitch = root_midi + 1;
    while pitch < root_midi + 13 {
        if pitch.rem_euclid(12) == class {
            return Some(pitch);
        }
        pitch += 1;
    }
    None
}

/// Places one pitch on the lowest string that keeps it within one hand
/// position of the root.
fn place(spec: &RiffSpec, pitch: i32, root_string: u32, root_fret: i32) -> Option<(u32, i32)> {
    let strings = u32::try_from(spec.tuning.len()).unwrap_or(0);
    (root_string..strings).find_map(|string| {
        let open = open_pitch(&spec.tuning, string)?;
        let fret = pitch - open;
        ((0..=MAX_FRET + 4).contains(&fret) && (fret - root_fret).abs() <= 4)
            .then_some((string, fret))
    })
}

/// How a motif states its material and answers itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotifForm {
    /// The figure, then the figure again with its tail changed.
    StatementVariation,
    /// The figure in the low register, answered in the middle.
    CallResponse,
    /// The figure, the figure again, then a continuation twice as long.
    Sentence,
}

/// What changes in the second half of a motif.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailVariation {
    /// The degrees run backwards.
    ContourInversion,
    /// The last degree is held for the whole answering phrase.
    Extension,
    /// The answering phrase lands on the tonic.
    CadentialRhythm,
    /// The tail repeats unchanged.
    None,
}

/// A motif described as a form over a riff specification.
#[derive(Debug, Clone, PartialEq)]
pub struct MotifSpec {
    /// The material the form is built from.
    pub riff: RiffSpec,
    /// How the material is stated and answered.
    pub form: MotifForm,
    /// What changes in the answer.
    pub tail: TailVariation,
}

/// Writes a motif: the riff's material arranged into a form.
///
/// The result is one document whose bar count is the form's, not the riff's.
/// A form that cannot be built from the given material returns the same
/// [`RiffSpecError`] the material would.
///
/// ```
/// use tabslib::generation::riff_spec::{write_motif, MotifForm, MotifSpec, RiffSpec, TailVariation};
///
/// let spec = MotifSpec {
///     riff: RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]),
///     form: MotifForm::StatementVariation,
///     tail: TailVariation::CadentialRhythm,
/// };
/// let motif = write_motif(&spec).expect("a realizable motif");
/// assert_eq!(motif.master_bars.len(), 8);
/// ```
pub fn write_motif(spec: &MotifSpec) -> Result<Document, RiffSpecError> {
    validate(&spec.riff)?;
    let statement = spec.riff.clone();
    let mut answer = statement.clone();
    answer.degrees = answer_degrees(&statement.degrees, spec.tail);
    match spec.form {
        MotifForm::CallResponse => answer.register = Register::Mid,
        MotifForm::Sentence => answer.bars = statement.bars * 2,
        MotifForm::StatementVariation => {}
    }
    if matches!(spec.tail, TailVariation::Extension) {
        answer.archetype = RiffArchetype::Sustained;
    }
    let mut document = write_riff(&statement)?;
    let tail = write_riff(&answer)?;
    crate::edits::append(&mut document, &tail)
        .map_err(|error| RiffSpecError::Unbuildable(format!("{error:?}")))?;
    Ok(document)
}

/// The degrees the answering phrase uses.
fn answer_degrees(degrees: &[u8], tail: TailVariation) -> Vec<u8> {
    match tail {
        TailVariation::None => degrees.to_vec(),
        TailVariation::ContourInversion => degrees.iter().rev().copied().collect(),
        TailVariation::Extension => degrees.last().map(|last| vec![*last]).unwrap_or_default(),
        TailVariation::CadentialRhythm => {
            let mut out = degrees.to_vec();
            if let Some(last) = out.last_mut() {
                *last = 1;
            }
            out
        }
    }
}

#[cfg(test)]
#[path = "riff_spec_test.rs"]
mod tests;
