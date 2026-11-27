//! The document-level generation functions and their request types.

use super::*;

use crate::Document;
use crate::Result;

/// Request data for deterministic document-level drum generation.
#[cfg(all(feature = "drums", feature = "rhythm"))]
#[derive(Debug, Clone, Copy)]
pub struct DrumGenerationRequest<'a> {
    /// Source track indexes used to derive the rhythm.
    pub source_tracks: &'a [usize],
    /// Drum profile controlling generated voices and probabilities.
    pub profile: &'a DrumProfile,
    /// Seed that makes generation reproducible.
    pub seed: u64,
    /// Tempo used by the playability solver.
    pub tempo: u32,
}

/// Request data for deterministic document-level bass generation.
#[cfg(all(feature = "drums", feature = "rhythm"))]
#[derive(Debug, Clone, Copy)]
pub struct BassGenerationRequest<'a> {
    /// Source track indexes used to derive the rhythm and pitches.
    pub source_tracks: &'a [usize],
    /// Limits and velocities for generated bass notes.
    pub profile: BassProfile,
    /// Pattern used to expand the source riff roots.
    pub strategy: BassStrategy,
    /// Caller tuning or automatic derivation from a source guitar.
    pub tuning: BassTuning<'a>,
    /// Highest fret considered playable by the position optimizer.
    pub max_fret: u32,
}

/// Request data for deterministic song arrangement.
#[cfg(feature = "arrangement")]
#[derive(Debug, Clone, Copy)]
pub struct ArrangementRequest<'a> {
    /// Source motifs and their inclusive bar ranges.
    pub motifs: &'a [Motif],
    /// Ordered sections to build from those motifs.
    pub sections: &'a [SectionRequest],
    /// Seed used to resolve equal-scoring motif choices.
    pub seed: u64,
}

/// Adds a seeded drum track using the stable document-level request type.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `request.source_tracks` is empty or names a track
/// outside the document, or if a bar's meter cannot be laid on a sixteenth-note grid.
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_drums(
    document: &mut Document,
    request: DrumGenerationRequest<'_>,
) -> Result<GeneratedTrack> {
    workflows::generate_drums(
        document,
        request.source_tracks,
        request.profile,
        request.seed,
        request.tempo,
    )
}

/// Adds a seeded drum track using caller-selected beat groupings by meter.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `request.source_tracks` is empty or names a track
/// outside the document, if a bar's meter cannot be laid on a sixteenth-note grid, or if no
/// beat grouping can be derived for a meter that `groupings` does not override.
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_drums_with_groupings(
    document: &mut Document,
    request: DrumGenerationRequest<'_>,
    groupings: &[BeatGrouping],
) -> Result<GeneratedTrack> {
    workflows::generate_drums_with_groupings(
        document,
        request.source_tracks,
        request.profile,
        request.seed,
        request.tempo,
        groupings,
    )
}

/// Adds a seeded drum track with caller-selected section feels.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `request.source_tracks` is empty or names a track
/// outside the document, if a section range is reversed or ends past the last bar, or if a
/// bar's meter cannot be laid on a sixteenth-note grid.
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_drums_with_sections(
    document: &mut Document,
    request: DrumGenerationRequest<'_>,
    sections: &[crate::generation::accompaniment::DrumSectionRequest],
) -> Result<GeneratedTrack> {
    workflows::generate_drums_with_sections(
        document,
        request.source_tracks,
        request.profile,
        request.seed,
        request.tempo,
        sections,
    )
}

/// Adds a strategy-driven bass track using the stable document-level request type.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `request.source_tracks` is empty or names a track
/// outside the document, if the derived tuning is unsupported or a custom tuning is not 4 or 5
/// ascending MIDI pitches, if a bar's meter cannot be laid on a sixteenth-note grid, or if the
/// bass notes do not fit within `max_fret`.
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_bass(
    document: &mut Document,
    request: BassGenerationRequest<'_>,
) -> Result<GeneratedTrack> {
    workflows::generate_bass(
        document,
        request.source_tracks,
        BassGenerationConfig {
            profile: request.profile,
            strategy: request.strategy,
            tuning: request.tuning,
            max_fret: request.max_fret,
        },
    )
}

/// Adds a strategy-driven bass track using caller-selected beat groupings.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `request.source_tracks` is empty or names a track
/// outside the document, if the derived tuning is unsupported or a custom tuning is not 4 or 5
/// ascending MIDI pitches, if a bar's meter cannot be laid on a sixteenth-note grid, if no beat
/// grouping can be derived for a meter that `groupings` does not override, or if the bass
/// notes do not fit within `max_fret`.
#[cfg(all(feature = "drums", feature = "rhythm"))]
pub fn generate_bass_with_groupings(
    document: &mut Document,
    request: BassGenerationRequest<'_>,
    groupings: &[BeatGrouping],
) -> Result<GeneratedTrack> {
    workflows::generate_bass_with_groupings(
        document,
        request.source_tracks,
        BassGenerationConfig {
            profile: request.profile,
            strategy: request.strategy,
            tuning: request.tuning,
            max_fret: request.max_fret,
        },
        groupings,
    )
}

/// Adds a seeded harmony track derived from one pitched source track.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `request.source_track` is outside the document, the
/// score is empty, the tuning or playability limits are invalid, the interval is beyond two
/// octaves, a diatonic harmony has no key to use, or the source track has no pitched notes.
#[cfg(feature = "harmony")]
pub fn generate_harmony(
    document: &mut Document,
    request: HarmonyConfig,
) -> Result<GeneratedHarmony> {
    harmony::generate_harmony(document, &request)
}

/// Builds an arrangement from motifs and sections without repairing timing.
///
/// # Errors
///
/// Returns the same errors as [`arrangement::compose_song`], an
/// [`ArrangementError`] for an invalid motif, section or join.
#[cfg(feature = "arrangement")]
pub fn arrange(
    source: &Document,
    request: ArrangementRequest<'_>,
) -> std::result::Result<Document, ArrangementError> {
    arrangement::compose_song(source, request.motifs, request.sections, request.seed)
}

/// Measures the written score and returns stable diagnostics and ratios.
#[cfg(feature = "assessment")]
pub fn assess_document(document: &Document) -> AssessmentReport {
    assessment::assess(document)
}

/// Compares an assessment to caller-supplied measurements.
#[cfg(feature = "assessment")]
pub fn calibrate_document(
    report: &AssessmentReport,
    samples: &[CalibrationSample],
) -> CalibrationReport {
    assessment::calibrate(report, samples)
}

/// A native operation or capability that a caller may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Capability {
    /// Descriptive score summary, statistics, key, and integrity reports.
    ScoreInspection,
    /// Deterministic edits over the document model.
    DocumentEdits,
    /// Document-level drum track generation.
    DrumGeneration,
    /// Document-level riff-locked bass generation.
    BassGeneration,
    /// Harmony and playable second-guitar generation.
    HarmonyGeneration,
    /// Motif analysis and deterministic song arrangement.
    Arrangement,
    /// Content-based section boundaries, marker proposals, and riff stabs.
    SectionAnalysis,
    /// Addressed editing of an arrangement plan.
    PlanEditing,
    /// Caller-shaped energy and tempo contours with incremental rebuilds.
    SongDynamics,
    /// Deterministic score assessment and diagnostics.
    Assessment,
    /// Comparison against caller-supplied calibration samples.
    Calibration,
    /// Hard-requirement checking of a document before release.
    RequirementChecking,
    /// Blind preference records and a calibration gate that can fail.
    PreferenceGating,
    /// Training, persistence, scoring, and evaluation of preference rankers.
    PreferenceRanking,
    /// Native lead or solo generation.
    SoloGeneration,
    /// Token-based solo composition with constrained guitar fingering.
    TokenSoloComposition,
    /// Explicit-role second-guitar generation.
    SecondGuitarGeneration,
    /// Orchestration of selected accompaniment layers on a document.
    AccompanimentWorkflow,
    /// Song-form planning over arranged sections.
    FormPlanning,
    /// Seeded arrangement search ranked against caller policy and time budgets.
    ArrangementSearch,
    /// Section transition planning and application.
    SectionTransitions,
    /// Feature-vector candidate selection and deck paging.
    CandidateDeck,
    /// Seeded selection from a grid-aligned pattern catalog.
    PatternCatalogs,
    /// Meter-derived beat groups, pulses, and backbeat positions.
    MeterGrouping,
    /// Drum phrase scheduling, humanization, and iterative improvement.
    DrumPerformance,
    /// Descriptive feel and per-bar density profiles for authored drums.
    DrumFeelProfiling,
    /// In-memory MIDI groove conversion, indexing, and riff adaptation.
    MidiGrooveIndex,
    /// Authoring a riff from a declarative specification.
    RiffAuthoring,
    /// Explicit key, tempo, and meter adaptation of an existing riff.
    RiffAdaptation,
    /// Combining riff material and filtering candidates by caller policy.
    RiffHybridization,
    /// Caller-data rhythm cells, interval n-grams, and similarity.
    RhythmicVocabulary,
    /// Application of caller profiles to riff specifications.
    RiffProfileApplication,
    /// Provider-neutral prompt scaffolding and rationale workflows.
    AiPrompting,
    /// Provider-backed ranking of assessed candidates.
    AiJudging,
    /// Structured critique of rendered ASCII tablature.
    TabCritique,
    /// AI model/provider integrations.
    AiProviderIntegration,
    /// Versioned section-label and boundary prompts.
    AiSectionPrompts,
    /// Named guitar and bass tuning presets with deterministic resolution.
    NamedTuningPresets,
}

/// Why a requested capability cannot currently run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedReason {
    /// The crate was compiled without the feature needed by the capability.
    FeatureDisabled,
    /// No native implementation is available yet.
    NotImplemented,
}

/// Availability of a native capability in this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapabilityStatus {
    /// The capability is available in this build.
    Available,
    /// The capability is unavailable, with an explicit reason.
    Unsupported(UnsupportedReason),
}

/// Reports whether a capability is available without falling back to a legacy model.
pub const fn capability_status(capability: Capability) -> CapabilityStatus {
    use Capability as C;
    use CapabilityStatus::{Available, Unsupported};
    use UnsupportedReason::FeatureDisabled;

    match capability {
        C::ScoreInspection | C::DocumentEdits => Available,
        C::DrumGeneration | C::BassGeneration => {
            if cfg!(all(feature = "drums", feature = "rhythm")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::HarmonyGeneration => {
            if cfg!(feature = "harmony") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::Arrangement
        | C::FormPlanning
        | C::SectionTransitions
        | C::PlanEditing
        | C::SongDynamics
        | C::SectionAnalysis => {
            if cfg!(feature = "arrangement") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::ArrangementSearch => {
            if cfg!(all(feature = "arrangement", feature = "assessment")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::Assessment
        | C::Calibration
        | C::RequirementChecking
        | C::PreferenceGating
        | C::PreferenceRanking => {
            if cfg!(feature = "assessment") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::SoloGeneration => {
            if cfg!(feature = "harmony") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::TokenSoloComposition => {
            if cfg!(all(feature = "harmony", feature = "assessment")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::SecondGuitarGeneration => {
            if cfg!(feature = "harmony") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::AccompanimentWorkflow | C::MeterGrouping | C::CandidateDeck | C::NamedTuningPresets => {
            Available
        }
        C::PatternCatalogs | C::DrumPerformance | C::DrumFeelProfiling => {
            if cfg!(feature = "drums") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::MidiGrooveIndex => {
            if cfg!(all(feature = "drums", feature = "midi")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::RiffAuthoring => {
            if cfg!(feature = "riffs") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::RiffAdaptation => {
            if cfg!(feature = "riffs") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::RiffHybridization => {
            if cfg!(all(feature = "riffs", feature = "assessment")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::RhythmicVocabulary => {
            if cfg!(feature = "rhythm") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::RiffProfileApplication => {
            if cfg!(all(feature = "rhythm", feature = "riffs")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::AiPrompting | C::AiJudging => {
            if cfg!(feature = "ai") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::AiSectionPrompts => {
            if cfg!(feature = "ai") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::TabCritique => {
            if cfg!(all(feature = "ai", feature = "ascii")) {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
        C::AiProviderIntegration => {
            if cfg!(feature = "ai-http") {
                Available
            } else {
                Unsupported(FeatureDisabled)
            }
        }
    }
}

/// Reports every hard requirement the document violates, in requirement order.
///
/// An empty result means nothing this crate can check rejects the document.
/// A non-empty one names what to regenerate; the violations are reports,
/// and no operation here repairs a document in place.
#[cfg(feature = "assessment")]
pub fn check_requirements(document: &Document) -> Vec<Violation> {
    requirements::check(document)
}

/// Writes the document a riff specification describes.
///
/// Unlike every other generator here, this one needs no source material:
/// it originates a riff rather than accompanying one. An unrealizable
/// specification returns [`RiffSpecError`] rather than a different riff.
///
/// # Errors
///
/// Returns the [`RiffSpecError`] that [`validate_riff`] reports if `spec` cannot be realized.
#[cfg(feature = "riffs")]
pub fn write_riff(spec: &RiffSpec) -> std::result::Result<Document, RiffSpecError> {
    riff_spec::write_riff(spec)
}

/// Checks a riff specification without writing it.
///
/// # Errors
///
/// Returns [`RiffSpecError::NoBars`] if `spec.bars` is zero, [`RiffSpecError::NoDegrees`] if
/// there are no degrees, and [`RiffSpecError::DegreeOutOfRange`] if a degree is not in 1..=7.
/// Returns [`RiffSpecError::TuningTooShort`] if the tuning has fewer strings than the voicing
/// needs, [`RiffSpecError::InvalidTempo`] if the tempo is not a positive finite number, and
/// [`RiffSpecError::UnreachableDegree`] if a degree cannot be fretted within range.
#[cfg(feature = "riffs")]
pub fn validate_riff(spec: &RiffSpec) -> std::result::Result<(), RiffSpecError> {
    riff_spec::validate(spec)
}

/// Applies one addressed edit to a plan, returning the new plan.
///
/// The plan is returned rather than mutated, so a refused edit cannot
/// leave a half-applied one behind.
///
/// # Errors
///
/// Returns [`PlanEditError::NoSuchSlot`] or [`PlanEditError::AmbiguousSlot`] if the address
/// matches no slot or several. Returns [`PlanEditError::ZeroRepeat`],
/// [`PlanEditError::LastSlot`], [`PlanEditError::InvalidSection`] or
/// [`PlanEditError::EnergyOutOfRange`] if the edit itself is not valid.
#[cfg(feature = "arrangement")]
pub fn edit_plan(
    plan: &crate::generation::arrangement::ArrangementPlan,
    edit: &PlanEdit,
) -> std::result::Result<
    (crate::generation::arrangement::ArrangementPlan, EditOutcome),
    PlanEditError,
> {
    plan_edit::apply(plan, edit)
}

/// Applies a list of edits in order, reporting what applied and what did not.
#[cfg(feature = "arrangement")]
pub fn edit_plan_all(
    plan: &crate::generation::arrangement::ArrangementPlan,
    edits: &[PlanEdit],
) -> EditReport {
    plan_edit::apply_all(plan, edits)
}

/// Turns a calibration comparison into pass, fail or quarantine verdicts.
///
/// The thresholds come from the caller. No tolerance is compiled in,
/// because the value that makes a metric meaningful is measured over a
/// collection of music.
#[cfg(feature = "assessment")]
pub fn gate_document(
    report: &AssessmentReport,
    samples: &[CalibrationSample],
    policy: &GatePolicy,
) -> GateReport {
    preference::gate(report, samples, policy)
}

/// Adds selected accompaniment layers to a document in one deterministic operation.
///
/// # Errors
///
/// Returns any error from generating a requested layer (drums, bass, harmony, lead or second
/// guitar), such as [`crate::Error::Malformed`] for invalid source tracks or settings. On
/// error the document is left unchanged.
pub fn generate_accompaniment(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentResult> {
    accompaniment::generate_accompaniment(document, request)
}
