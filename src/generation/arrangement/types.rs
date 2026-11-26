//! The data types of arrangement: fingerprints, motifs, section requests, plans and errors.

use super::*;

/// A stable, style-neutral summary of one master bar across all tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotifFingerprint {
    /// Exact beat lengths, in whole-note fractions, in track and voice order.
    pub rhythms: Vec<Vec<(u64, u64)>>,
    /// Pitch classes in track, beat, and note order.
    pub pitch_classes: Vec<Vec<u8>>,
    /// Consecutive melodic intervals, in semitones, per track.
    pub intervals: Vec<Vec<i32>>,
    /// Number of sounding notes per track.
    pub note_counts: Vec<usize>,
    /// Track indexes that contain no sounding notes in this bar.
    pub empty_tracks: Vec<usize>,
}

/// A contiguous region with similar per-bar fingerprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Motif {
    /// Inclusive starting master-bar index.
    pub start_bar: usize,
    /// Inclusive ending master-bar index.
    pub end_bar: usize,
    /// Combined fingerprint for the region.
    pub fingerprint: MotifFingerprint,
}

/// Component scores for two motifs; each component is in the range `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compatibility {
    /// Similarity of exact written beat durations.
    pub rhythm: f64,
    /// Similarity of pitch-class contours.
    pub pitch: f64,
    /// Similarity of melodic interval sequences.
    pub contour: f64,
    /// Similarity of per-track note density.
    pub density: f64,
    /// Weighted aggregate score.
    pub score: f64,
}

/// Result of checking a structural join between two master bars.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinReport {
    /// Whether the join is structurally valid without changing either bar.
    pub valid: bool,
    /// Compatibility score between the bars, even when the join is valid.
    pub compatibility: Compatibility,
    /// Timing or reference problems that prevent assembly.
    pub problems: Vec<String>,
}

/// One requested section in a deterministic composition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionRequest {
    /// Label written to the first bar of the section.
    pub label: String,
    /// Number of output bars. The selected motif repeats or truncates to fit.
    pub bars: usize,
    /// Candidate indexes into the motif list supplied to [`compose_song`].
    pub candidates: Vec<usize>,
    /// Semitone transposition applied to every selected bar in this section.
    #[serde(default)]
    pub transpose_semitones: i32,
    /// Written intensity from zero to one hundred, or `None` to leave the
    /// assembled dynamics as the source wrote them.
    #[serde(default)]
    pub energy: Option<u8>,
    /// Track indexes that sound in this section, or `None` for every track.
    ///
    /// Tracks outside the list are silenced across the section's bars rather
    /// than removed, so the assembled document keeps its track layout.
    #[serde(default)]
    pub instrumentation: Option<Vec<usize>>,
    /// Optional mapping from section energy to additional texture changes.
    #[serde(default)]
    pub energy_texture: Option<EnergyTexture>,
    /// Whether this is the one marked sectional climax in the plan.
    #[serde(default)]
    pub climax: bool,
}

/// Caller-supplied texture mapping driven by a section's energy value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyTexture {
    /// Tracks that are silent below `unmute_at` and sound at or above it.
    pub optional_tracks: Vec<usize>,
    /// Energy threshold at which optional tracks are restored.
    pub unmute_at: u8,
    /// Track to double one octave above `octave_at`, when configured.
    pub octave_track: Option<usize>,
    /// Energy threshold at which octave doubling begins.
    pub octave_at: u8,
}

impl EnergyTexture {
    /// Creates a caller-defined energy-to-texture mapping.
    pub const fn new(
        optional_tracks: Vec<usize>,
        unmute_at: u8,
        octave_track: Option<usize>,
        octave_at: u8,
    ) -> Self {
        Self {
            optional_tracks,
            unmute_at,
            octave_track,
            octave_at,
        }
    }
}

/// Dynamic written for an energy value, coarse enough to survive a round trip.
pub(super) fn dynamic_for(energy: u8) -> &'static str {
    match energy {
        0..=16 => "PPP",
        17..=33 => "PP",
        34..=49 => "P",
        50..=66 => "MF",
        67..=83 => "F",
        84..=91 => "FF",
        _ => "FFF",
    }
}

/// A serializable arrangement request that can be reviewed or edited before assembly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrangementPlan {
    /// Schema version for forward-compatible plan files.
    pub version: u32,
    /// Seed used to resolve equally ranked motif candidates.
    pub seed: u64,
    /// Maximum number of source bars grouped into one motif.
    #[serde(default = "default_motif_max_bars")]
    pub motif_max_bars: usize,
    /// Minimum adjacent-bar compatibility required to keep a motif together.
    #[serde(default = "default_motif_threshold")]
    pub motif_threshold: f64,
    /// Ordered sections to assemble.
    pub sections: Vec<SectionRequest>,
    /// Transition rules applied after the sections are assembled.
    #[serde(default)]
    pub transitions: Vec<crate::generation::transitions::TransitionRule>,
}

impl PartialEq for ArrangementPlan {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.seed == other.seed
            && self.motif_max_bars == other.motif_max_bars
            && self.motif_threshold.to_bits() == other.motif_threshold.to_bits()
            && self.sections == other.sections
            && self.transitions == other.transitions
    }
}

impl Eq for ArrangementPlan {}

impl ArrangementPlan {
    /// Creates a plan with the current schema version.
    pub fn new(seed: u64, sections: Vec<SectionRequest>) -> Self {
        Self {
            version: 1,
            seed,
            motif_max_bars: default_motif_max_bars(),
            motif_threshold: default_motif_threshold(),
            sections,
            transitions: Vec::new(),
        }
    }

    /// Serializes this plan as pretty JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArrangementError::Serialization`] if the plan cannot be encoded as JSON.
    pub fn to_json(&self) -> Result<String, ArrangementError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ArrangementError::Serialization(error.to_string()))
    }

    /// Parses and validates a JSON arrangement plan.
    ///
    /// # Errors
    ///
    /// Returns [`ArrangementError::Serialization`] if `json` is not a valid plan, and
    /// [`ArrangementError::InvalidInput`] if the plan version is not 1, it has no sections,
    /// `motif_max_bars` is zero, or `motif_threshold` is not a finite value in `0..=1`.
    pub fn from_json(json: &str) -> Result<Self, ArrangementError> {
        let plan: Self = serde_json::from_str(json)
            .map_err(|error| ArrangementError::Serialization(error.to_string()))?;
        if plan.version != 1 {
            return Err(ArrangementError::InvalidInput(format!(
                "unsupported arrangement plan version {}",
                plan.version
            )));
        }
        if plan.sections.is_empty() {
            return Err(ArrangementError::InvalidInput(
                "arrangement plan needs at least one section".into(),
            ));
        }
        if plan.motif_max_bars == 0 {
            return Err(ArrangementError::InvalidInput(
                "motif_max_bars must be greater than zero".into(),
            ));
        }
        if !plan.motif_threshold.is_finite() || !(0.0..=1.0).contains(&plan.motif_threshold) {
            return Err(ArrangementError::InvalidInput(
                "motif_threshold must be between 0 and 1".into(),
            ));
        }
        Ok(plan)
    }
}

pub(super) fn default_motif_max_bars() -> usize {
    16
}

pub(super) fn default_motif_threshold() -> f64 {
    0.25
}

/// Errors returned by segmentation and composition operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArrangementError {
    /// A document reference, range, or composition request is invalid.
    #[error("invalid arrangement input: {0}")]
    InvalidInput(String),
    /// A composed bar has an overfull voice.
    #[error(
        "bar {bar} on track {track}, voice {voice} exceeds its meter (source bar {source_bar})"
    )]
    OverfullBar {
        /// Track index containing the invalid voice.
        track: usize,
        /// Destination master-bar number (one-based) containing the invalid voice.
        bar: usize,
        /// Voice index containing the invalid timing.
        voice: usize,
        /// Source master-bar number (one-based) that supplied the invalid material.
        source_bar: usize,
    },
    /// A transposition would leave the representable pitch or fret range.
    #[error("transposition {semitones} is invalid for note {note_id}")]
    InvalidTransposition {
        /// Affected note identity.
        note_id: u32,
        /// Requested semitone offset.
        semitones: i32,
    },
    /// A plan could not be encoded or decoded.
    #[error("arrangement plan serialization failed: {0}")]
    Serialization(String),
}
