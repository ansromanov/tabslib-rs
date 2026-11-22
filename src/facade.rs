//! Stable high-level entry points for applications using [`Document`].
//!
//! The facade groups score inspection, edits, generation, arrangement,
//! assessment, and optional format adapters under one path. Its operations
//! work on the native document model; file access stays in [`mod@format`].

pub use crate::edits;
pub use crate::evaluation;
pub use crate::format;
pub use crate::inspect;
pub use crate::load;
pub use crate::model;
pub use crate::pitch;
#[cfg(feature = "gp")]
pub use crate::save;
pub use crate::selection;
pub use crate::{Document, Error, Result};

/// Exports a document as a deterministic MIDI buffer.
///
/// # Errors
///
/// Returns `Ok` for every document: MIDI encoding has no error condition today.
#[cfg(feature = "midi")]
pub fn export_midi(document: &Document) -> Result<Vec<u8>> {
    Ok(crate::format::midi::write(document))
}

/// Renders a document as a stereo 16-bit WAV buffer using a SoundFont.
///
/// # Errors
///
/// Returns any error from `crate::format::wav::render_pcm`: [`crate::Error::Io`] if the SoundFont
/// cannot be opened, and [`crate::Error::Malformed`] if it is not valid SoundFont2 data or the
/// score cannot be rendered (an invalid tempo, more than 15 pitched tracks, or output too long for
/// a WAV file).
#[cfg(feature = "wav")]
pub fn render_wav(document: &Document, soundfont: impl AsRef<std::path::Path>) -> Result<Vec<u8>> {
    crate::format::wav::render_pcm(document, soundfont)
}

/// File-boundary workflows using the compiled-in GP adapter.
#[cfg(feature = "gp")]
pub mod file;

/// Fast drum track assessment and section marker detection.
#[cfg(all(feature = "drums", feature = "arrangement"))]
pub mod assess;
#[cfg(all(feature = "drums", feature = "arrangement"))]
pub use assess::{
    assess_drums_fast, detect_marked_sections, AssessDrumsOptions, DrumAssessError,
    FastAssessmentResult, FastSectionScore, MarkedSection,
};

/// Native generation operations with typed inputs and results.
pub mod generation {
    use crate::generation::accompaniment;
    #[cfg(feature = "arrangement")]
    pub use crate::generation::accompaniment::generate_full_song;
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub use crate::generation::accompaniment::DrumSectionRequest;
    pub use crate::generation::accompaniment::{
        AccompanimentLayers, AccompanimentOutcome, AccompanimentRequest, AccompanimentResult,
        AccompanimentRole, AccompanimentRoleResult, AccompanimentSkipReason,
    };
    #[cfg(feature = "ai")]
    pub use crate::generation::ai::{
        assess_sections_with_ai, blend_rankings_with_deterministic, blind_pair,
        boundary_decisions_action_space, build_section_assessment_payload,
        capture_drum_critique_request, capture_request, classify_provider_failure,
        compare_candidates, critique_drums, format_rationale, generate_rationale,
        generate_section_boundaries, generate_section_labels, judge_candidates,
        judge_preference_agreement, offline_heuristic_judge, parse_candidate_ranking,
        parse_drum_critique_response, parse_response, parse_section_boundary_response,
        parse_section_labels_response, rationale_prose_action_space, resolve_boundary_decisions,
        resolve_rationale_prose, resolve_section_decisions, section_decisions_action_space,
        verify_bar_claims_in_tab, AiError, AiFailureReason, BarClaim, BlindPair,
        CandidateEvaluation, CandidateRanking, CapturedRequest, DiagnosticComparison, DrumCritique,
        DrumCritiqueFinding, InteractiveDecisionProvider, JudgeAgreement, MetricComparison,
        MetricDirection, PromptCandidate, PromptInput, PromptTask, Rationale, RationaleFinding,
        RationaleProvider, SectionAssessment, SectionAssessmentResponse, SectionBoundaryResponse,
        SectionFingerprint, SectionLabel, SectionLabelsResponse, SectionMergeSuggestion,
        VerifiedBarClaim,
    };
    #[cfg(all(feature = "ai", feature = "ascii"))]
    pub use crate::generation::ai::{
        capture_tab_critique_request, critique_tab, parse_tab_critique_response, verify_bar_claims,
        TabCritique, TabCritiqueFinding, TabDefectClass,
    };
    #[cfg(feature = "arrangement")]
    use crate::generation::arrangement;
    #[cfg(feature = "arrangement")]
    pub use crate::generation::arrangement::{
        apply_seeded_variation, check_join, compose_plan, compose_song, fingerprint_bar,
        score_compatibility, segment_motifs, ArrangementError, ArrangementPlan, Compatibility,
        EnergyTexture, JoinReport, Motif, MotifFingerprint, SectionRequest,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::arrangement_dynamics::{
        apply_energy_contour, apply_tempo_contour, compose_plan_incremental, diff_plan_sections,
        ContourError, IncrementalBuild, TempoBounds,
    };
    #[cfg(all(feature = "arrangement", feature = "assessment"))]
    pub use crate::generation::arrangement_search::{
        bar_duration, bars_for_minutes, document_bar_duration, resolve_repeats,
        search_arrangements, ArrangementCandidate, ArrangementSearchError,
        ArrangementSearchRequest, ArrangementSearchWeights, BarsForMinutes, ExactDuration,
        RepeatResolution,
    };
    #[cfg(feature = "assessment")]
    use crate::generation::assessment;
    #[cfg(feature = "assessment")]
    pub use crate::generation::assessment::{
        assess, calibrate, group_diagnostics, AssessmentReport, CalibrationReport,
        CalibrationSample, CalibrationValue, Diagnostic, DiagnosticClass, DiagnosticGroup,
        Measurement, Metric, Region, Severity,
    };
    pub use crate::generation::candidate_deck::{
        build_candidate_deck, CandidateDeck, DeckCandidateInput, DeckEntry, DeckError, DeckRequest,
        FeatureVector,
    };
    #[cfg(all(feature = "drums", feature = "arrangement", feature = "assessment"))]
    pub use crate::generation::drums::apply_stab_catches;
    #[cfg(feature = "drums")]
    pub use crate::generation::drums::feel::{
        profile_existing_drums, CymbalPreference, DrumFeel, DrumFeelError, DrumFeelProfile,
    };
    #[cfg(feature = "drums")]
    pub use crate::generation::drums::improvement::{
        assess as assess_drums, improve as improve_drums, DrumAssessment, DrumImprovementConfig,
        DrumImprovementReport, DrumRatio,
    };
    #[cfg(all(feature = "drums", feature = "midi"))]
    pub use crate::generation::drums::midi_groove::{
        adapt_midi_groove_to_riff, detect_midi_groove_feel, find_best_midi_grooves,
        generate_from_midi_index, index_midi_directory, midi_to_drum_hits, MidiGroove,
        MidiGrooveCandidate, MidiGrooveError, MidiGrooveFeel, MidiGrooveFile, MidiGrooveGeneration,
        MidiGrooveIndex, MidiGrooveQuery, MidiGrooveSource, SeededGrooveFallback,
    };
    #[cfg(feature = "drums")]
    pub use crate::generation::drums::performance::{
        add_transition_marker, apply_articulations, apply_fill, apply_performance_layer,
        generate_seeded_hits, humanize_hits, pair_hihat_chokes, ArticulationRequest, DrumProfile,
        FillPattern, HumanizationConfig, PerformanceLayerConfig,
    };
    #[cfg(feature = "drums")]
    pub use crate::generation::drums::phrase_clock::{PhraseClock, ScheduledFill};
    #[cfg(feature = "drums")]
    pub use crate::generation::drums::presets::{
        apply_explicit_velocity_tier_shift, apply_variations, backbeat_snares, cymbal_ostinato,
        half_time_groove_preset, intensity_velocity_factor, resolve_auto_preset, riff_kicks,
        rock_straight_preset, shape_velocities, unghost_backbeats, SectionDrumFeel,
    };
    #[cfg(feature = "drums")]
    pub use crate::generation::drums::{
        DrumHit, DrumOrnament, DrumPattern, PatternStep, PhraseGrid, RiffOnset,
    };
    pub use crate::generation::execution_log::{
        explain, Decision, ExecutionLog, GeneratedRole, LogError,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::forms::{plan_form, FormTemplate};
    #[cfg(feature = "harmony")]
    pub use crate::generation::harmonic::{
        build_chord_tones, build_chord_tones_in_key, build_seventh_chord, find_voicings,
        find_voicings_with_options, generate_harmonic_riffs, is_chord_tone, key_from_chord_label,
        nearest_chord_tone, parse_chord_label, pick_best_voicing, pick_best_voicing_with_options,
        sample_progression, voicing_to_midi_pitches, write_harmonic_riff, ChordLabel, ChordQuality,
        ChordVoicing, HarmonicError, HarmonicRiffConfig, VoicingOptions,
    };
    #[cfg(feature = "harmony")]
    use crate::generation::harmony;
    #[cfg(feature = "harmony")]
    pub use crate::generation::harmony::{
        GeneratedHarmony, HarmonyConfig, HarmonyDiagnostics, HarmonyType,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::intent::{
        apply_intent, AppliedIntentEdit, ArrangementIntent, IntentApplication, IntentError,
        RejectedIntentEdit,
    };
    pub use crate::generation::meter::{BeatGrouping, BeatGroupingError};
    #[cfg(feature = "arrangement")]
    pub use crate::generation::motif_roles::{
        assign_roles, candidate_motifs_for_role, MotifEnergy, MotifRole, RoleAssignment,
        RoleCandidates,
    };
    #[cfg(feature = "drums")]
    pub use crate::generation::pattern_catalog::{
        CatalogPattern, DrumPatternCatalog, PatternCatalog, PatternConstraints, PatternError,
        PatternGrid, PatternRejection, RhythmSectionPatternCatalog, RhythmSectionStep,
        SelectedPattern,
    };
    #[cfg(feature = "arrangement")]
    use crate::generation::plan_edit;
    #[cfg(feature = "arrangement")]
    pub use crate::generation::plan_edit::{
        planned_bars, resolve, EditOutcome, EditReport, Placement, PlanEdit, PlanEditError,
        RejectedEdit, SlotAddress,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::plan_sidecar::{
        compute_source_hash, load_plan, plan_from_sidecar, plan_to_sidecar, save_plan,
        PlanEditRecord, PlanSidecar, PlanSidecarError, PLAN_VERSION,
    };
    #[cfg(feature = "assessment")]
    use crate::generation::preference;
    #[cfg(feature = "assessment")]
    pub use crate::generation::preference::{
        append_record, assign_side, blind_record, discrimination, read_records, tally,
        CandidateRef, Choice, GatePolicy, GateReport, MetricVerdict, PreferenceError,
        PreferenceRecord, Side, Verdict,
    };
    #[cfg(feature = "assessment")]
    pub use crate::generation::preference_ranker::{
        preference_ranker_agreement, rank_preference_candidates, score_preference_candidate,
        train_preference_ranker, PreferenceAgreement, PreferenceCandidate, PreferenceRanker,
        PreferenceRankerError, PreferenceTrainingPair, PreferenceWeight, ScoredPreferenceCandidate,
        PREFERENCE_RANKER_VERSION,
    };
    #[cfg(all(feature = "arrangement", feature = "assessment"))]
    pub use crate::generation::quality_gaps::{
        arrangement_hard_constraint_violations, evaluate_arrangement,
        find_horizontal_motion_violations, plan_drum_roles, CompositionQuality, DrumRoleDecision,
        HorizontalMotionViolation,
    };
    #[cfg(feature = "assessment")]
    use crate::generation::requirements;
    #[cfg(feature = "assessment")]
    pub use crate::generation::requirements::check_track;
    #[cfg(feature = "assessment")]
    pub use crate::generation::requirements::{Requirement, Violation};
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub use crate::generation::rhythm::{
        BassFretPosition, BassGenerationConfig, BassProfile, BassStrategy, BassTuning,
    };
    #[cfg(all(feature = "rhythm", feature = "riffs"))]
    pub use crate::generation::rhythm_profile::{
        apply_riff_profile, CallerRiffProfile, RiffProfileChange, RiffProfileField,
        RiffProfileValue,
    };
    #[cfg(feature = "rhythm")]
    pub use crate::generation::rhythm_profile::{
        blend_vocabularies, build_rhythmic_vocabulary, cosine_similarity, extract_interval_ngrams,
        extract_rhythm_cells, extract_rhythm_phrases, form_prior, RhythmCell, RhythmPhrase,
        RhythmProfileError, RhythmicVocabulary,
    };
    #[cfg(feature = "rhythm")]
    pub use crate::generation::rhythm_profile::{
        extract_rhythm_cells_tolerant, recommended_steps_per_quarter, RhythmExtraction,
        SkippedRhythm,
    };
    #[cfg(feature = "riffs")]
    pub use crate::generation::riff_adaptation::{
        apply_riff_adaptation, plan_riff_adaptation, KeyAdaptation, MeterAdaptation,
        RiffAdaptationError, RiffAdaptationPlan, RiffAdaptationRequest, TempoAdaptation,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::riff_composition::{
        arrange_from_riffs, build_track_mapping, central_key, compatibility_matrix,
        compose_from_riffs, compose_from_slots, order_sources, recommend_slot_order,
        RiffCompositionConfig, RiffCompositionError, RiffSlot, RiffSource, SlotAdaptation,
        SlotCompositionResult, SlotPlan, SlotRange,
    };
    #[cfg(all(feature = "riffs", feature = "assessment"))]
    pub use crate::generation::riff_hybrid::{
        filter_candidates, hybridize, Candidate, CandidateRejection, CandidateResult, HybridError,
        HybridMode, HybridRequest, MetricThreshold,
    };
    #[cfg(feature = "riffs")]
    use crate::generation::riff_spec;
    #[cfg(feature = "riffs")]
    pub use crate::generation::riff_spec::{
        write_motif, MotifForm, MotifSpec, Register, RiffArchetype, RiffSpec, RiffSpecError,
        Syncopation, TailVariation, Voicing,
    };
    #[cfg(feature = "harmony")]
    pub use crate::generation::second_guitar::{
        generate_second_guitar, GeneratedSecondGuitar, GuitarRole, PhraseTailInterval,
        SecondGuitarConfig, SecondGuitarError, SectionRole,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::sections::{
        apply_section_marker_proposals, detect_riff_stabs, detect_section_boundaries,
        propose_section_markers, propose_section_merges, propose_single_bar_folds,
        sections_from_boundaries, RiffStab, SectionAnalysisError, SectionBoundary,
        SectionMarkerProposal, SectionMergeProposal, SectionRange, SingleBarFoldProposal,
    };
    #[cfg(feature = "harmony")]
    pub use crate::generation::solo::{generate_solo, GeneratedSolo, SoloConfig, SoloDiagnostics};
    #[cfg(feature = "harmony")]
    pub use crate::generation::solo_presets::{
        arpeggio_scale_steps, blues_scale_steps, calculate_lead_beat_dynamics,
        diatonic_scale_steps, generate_solo_population, get_solo_preset, list_solo_presets,
        pentatonic_scale_steps, plan_lead_spans_from_plan, plan_solo_spans, LeadSpan,
        SoloPopulationCandidate, SoloPreset,
    };
    #[cfg(all(feature = "harmony", feature = "assessment"))]
    pub use crate::generation::solo_tokens::{
        compose_solo_tokens, rank_solo_candidates, SoloTokenCandidate, TokenSoloError,
        TokenSoloRequest,
    };
    #[cfg(feature = "assessment")]
    pub use crate::generation::studies::{
        evaluate_known_regression_cases, summarize_model_judge_study, KnownRegressionCase,
        KnownRegressionResult, ModelJudgeStudySummary,
    };
    #[cfg(all(feature = "assessment", feature = "ai", feature = "ascii"))]
    pub use crate::generation::studies::{summarize_tab_critique_study, TabCritiqueStudySummary};
    #[cfg(feature = "riffs")]
    pub use crate::generation::texture::{
        assert_texture_class_preserved, classify_bar_texture, BarTexture, TextureError,
    };
    #[cfg(feature = "harmony")]
    pub use crate::generation::transforms::{
        apply_articulation, apply_solo_archetype, patch_score_section, transform_score, Archetype,
        ARTICULATION_DESCRIPTIONS,
    };
    #[cfg(feature = "arrangement")]
    pub use crate::generation::transitions::{
        apply_transitions, plan_transitions, SectionTransition, TransitionKind, TransitionRule,
    };
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    use crate::generation::workflows;
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub use crate::generation::workflows::GeneratedTrack;
    pub use crate::pitch::{
        preset_name_for_tuning, resolve_tuning_preset, retune_preserve_fingering,
        retune_preserve_pitch, PitchError, PitchTarget, TuningPreset, TuningPresetError,
        TUNING_PRESETS,
    };
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
            C::AccompanimentWorkflow
            | C::MeterGrouping
            | C::CandidateDeck
            | C::NamedTuningPresets => Available,
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
}
