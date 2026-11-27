//! Native generation operations with typed inputs and results.

mod functions;
pub use functions::*;

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
    capture_drum_critique_request, capture_request, classify_provider_failure, compare_candidates,
    critique_drums, format_rationale, generate_rationale, generate_section_boundaries,
    generate_section_labels, judge_candidates, judge_preference_agreement, offline_heuristic_judge,
    parse_candidate_ranking, parse_drum_critique_response, parse_response,
    parse_section_boundary_response, parse_section_labels_response, rationale_prose_action_space,
    resolve_boundary_decisions, resolve_rationale_prose, resolve_section_decisions,
    section_decisions_action_space, verify_bar_claims_in_tab, AiError, AiFailureReason, BarClaim,
    BlindPair, CandidateEvaluation, CandidateRanking, CapturedRequest, DiagnosticComparison,
    DrumCritique, DrumCritiqueFinding, InteractiveDecisionProvider, JudgeAgreement,
    MetricComparison, MetricDirection, PromptCandidate, PromptInput, PromptTask, Rationale,
    RationaleFinding, RationaleProvider, SectionAssessment, SectionAssessmentResponse,
    SectionBoundaryResponse, SectionFingerprint, SectionLabel, SectionLabelsResponse,
    SectionMergeSuggestion, VerifiedBarClaim,
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
    bar_duration, bars_for_minutes, document_bar_duration, resolve_repeats, search_arrangements,
    ArrangementCandidate, ArrangementSearchError, ArrangementSearchRequest,
    ArrangementSearchWeights, BarsForMinutes, ExactDuration, RepeatResolution,
};
#[cfg(feature = "assessment")]
use crate::generation::assessment;
#[cfg(feature = "assessment")]
pub use crate::generation::assessment::{
    assess, calibrate, group_diagnostics, AssessmentReport, CalibrationReport, CalibrationSample,
    CalibrationValue, Diagnostic, DiagnosticClass, DiagnosticGroup, Measurement, Metric, Region,
    Severity,
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
    assign_roles, candidate_motifs_for_role, MotifEnergy, MotifRole, RoleAssignment, RoleCandidates,
};
#[cfg(feature = "drums")]
pub use crate::generation::pattern_catalog::{
    CatalogPattern, DrumPatternCatalog, PatternCatalog, PatternConstraints, PatternError,
    PatternGrid, PatternRejection, RhythmSectionPatternCatalog, RhythmSectionStep, SelectedPattern,
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
    compute_source_hash, load_plan, plan_from_sidecar, plan_to_sidecar, save_plan, PlanEditRecord,
    PlanSidecar, PlanSidecarError, PLAN_VERSION,
};
#[cfg(feature = "assessment")]
use crate::generation::preference;
#[cfg(feature = "assessment")]
pub use crate::generation::preference::{
    append_record, assign_side, blind_record, discrimination, read_records, tally, CandidateRef,
    Choice, GatePolicy, GateReport, MetricVerdict, PreferenceError, PreferenceRecord, Side,
    Verdict,
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
    apply_riff_profile, CallerRiffProfile, RiffProfileChange, RiffProfileField, RiffProfileValue,
};
#[cfg(feature = "rhythm")]
pub use crate::generation::rhythm_profile::{
    blend_vocabularies, build_rhythmic_vocabulary, cosine_similarity, extract_interval_ngrams,
    extract_rhythm_cells, extract_rhythm_phrases, form_prior, RhythmCell, RhythmPhrase,
    RhythmProfileError, RhythmicVocabulary,
};
#[cfg(feature = "rhythm")]
pub use crate::generation::rhythm_profile::{
    extract_rhythm_cells_tolerant, recommended_steps_per_quarter, RhythmExtraction, SkippedRhythm,
};
#[cfg(feature = "riffs")]
pub use crate::generation::riff_adaptation::{
    apply_riff_adaptation, plan_riff_adaptation, KeyAdaptation, MeterAdaptation,
    RiffAdaptationError, RiffAdaptationPlan, RiffAdaptationRequest, TempoAdaptation,
};
#[cfg(feature = "arrangement")]
pub use crate::generation::riff_composition::{
    arrange_from_riffs, build_track_mapping, central_key, compatibility_matrix, compose_from_riffs,
    compose_from_slots, order_sources, recommend_slot_order, RiffCompositionConfig,
    RiffCompositionError, RiffSlot, RiffSource, SlotAdaptation, SlotCompositionResult, SlotPlan,
    SlotRange,
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
    arpeggio_scale_steps, blues_scale_steps, calculate_lead_beat_dynamics, diatonic_scale_steps,
    generate_solo_population, get_solo_preset, list_solo_presets, pentatonic_scale_steps,
    plan_lead_spans_from_plan, plan_solo_spans, LeadSpan, SoloPopulationCandidate, SoloPreset,
};
#[cfg(all(feature = "harmony", feature = "assessment"))]
pub use crate::generation::solo_tokens::{
    compose_solo_tokens, rank_solo_candidates, SoloTokenCandidate, TokenSoloError, TokenSoloRequest,
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
