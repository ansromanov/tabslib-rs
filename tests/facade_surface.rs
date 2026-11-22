//! Every capability the facade names is reachable through the facade.
//!
//! `docs/native-api.md` directs callers to `tabslib::facade` rather than into
//! the crate internals, so a capability whose types are only reachable at
//! `tabslib::generation::...` is not actually exposed. This file imports
//! nothing else: if a re-export is missing, it stops compiling.
//!
//! That is the assertion. A capability listed by `Capability` and reported
//! `Available` while its types cannot be named through `facade` is the drift
//! these tests exist to catch.

#![cfg(all(
    feature = "drums",
    feature = "rhythm",
    feature = "midi",
    feature = "harmony",
    feature = "arrangement",
    feature = "assessment",
    feature = "riffs"
))]

use tabslib::facade::{
    self,
    generation::{
        adapt_midi_groove_to_riff, apply_energy_contour, apply_riff_adaptation, apply_riff_profile,
        apply_section_marker_proposals, apply_tempo_contour, apply_transitions, assess_drums,
        bar_duration, bars_for_minutes, blend_vocabularies, build_candidate_deck,
        build_rhythmic_vocabulary, capability_status, compose_plan_incremental,
        compose_solo_tokens, cosine_similarity, detect_midi_groove_feel, detect_riff_stabs,
        detect_section_boundaries, diff_plan_sections, extract_interval_ngrams,
        extract_rhythm_cells, extract_rhythm_phrases, filter_candidates, form_prior,
        generate_from_midi_index, generate_second_guitar, generate_seeded_hits, humanize_hits,
        hybridize, improve_drums, midi_to_drum_hits, pair_hihat_chokes, plan_form,
        plan_riff_adaptation, plan_transitions, preference_ranker_agreement,
        profile_existing_drums, propose_section_markers, rank_preference_candidates,
        rank_solo_candidates, search_arrangements, segment_motifs, ArrangementPlan,
        ArrangementSearchRequest, ArrangementSearchWeights, BeatGrouping, CallerRiffProfile,
        Candidate, Capability, CapabilityStatus, CymbalPreference, DeckCandidateInput, DeckRequest,
        DrumFeel, DrumGenerationRequest, DrumHit, DrumImprovementConfig, DrumOrnament, DrumPattern,
        DrumProfile, ExactDuration, FillPattern, FormTemplate, GuitarRole, HumanizationConfig,
        HybridMode, HybridRequest, MidiGroove, MidiGrooveError, MidiGrooveFeel,
        MidiGrooveGeneration, MidiGrooveIndex, MidiGrooveSource, PatternConstraints, PatternGrid,
        PatternStep, PhraseClock, PreferenceCandidate, PreferenceRanker, PreferenceTrainingPair,
        RhythmCell, RhythmPhrase, RhythmProfileError, RhythmSectionPatternCatalog,
        RhythmicVocabulary, RiffAdaptationRequest, RiffProfileChange, RiffProfileField,
        RiffProfileValue, RiffStab, SecondGuitarConfig, SectionAnalysisError, SectionBoundary,
        SectionMarkerProposal, SectionMergeProposal, SectionRange, SectionRequest,
        SectionTransition, SeededGrooveFallback, SingleBarFoldProposal, TempoBounds,
        TokenSoloRequest, TransitionKind, TransitionRule,
    },
};

/// Every capability this build reports as available, named through the facade.
const AVAILABLE: [Capability; 22] = [
    Capability::Arrangement,
    Capability::SectionAnalysis,
    Capability::Assessment,
    Capability::FormPlanning,
    Capability::SectionTransitions,
    Capability::PatternCatalogs,
    Capability::MeterGrouping,
    Capability::DrumPerformance,
    Capability::DrumFeelProfiling,
    Capability::MidiGrooveIndex,
    Capability::HarmonyGeneration,
    Capability::AccompanimentWorkflow,
    Capability::RiffAdaptation,
    Capability::RiffHybridization,
    Capability::TokenSoloComposition,
    Capability::SecondGuitarGeneration,
    Capability::CandidateDeck,
    Capability::ArrangementSearch,
    Capability::SongDynamics,
    Capability::PreferenceRanking,
    Capability::RhythmicVocabulary,
    Capability::RiffProfileApplication,
];

#[test]
fn rhythmic_vocabulary_and_profile_application_are_reachable_through_the_facade() {
    let source = tabslib::fixtures::repeated_frets();
    let cells = extract_rhythm_cells(&source, &[0], 4).expect("facade cell extraction");
    assert!(!cells.is_empty());
    assert!(!extract_rhythm_phrases(&cells, 2)
        .expect("phrase extraction")
        .is_empty());
    assert!(!extract_interval_ngrams(&source, &[0], 4, 1)
        .expect("interval extraction")
        .is_empty());
    let vocabulary = build_rhythmic_vocabulary(std::slice::from_ref(&source), &[0], 4, 1, &[])
        .expect("caller vocabulary");
    assert_eq!(cosine_similarity(&vocabulary.cells, &vocabulary.cells), 1.0);
    assert_eq!(
        blend_vocabularies(&vocabulary, &vocabulary, 0.5).expect("blend"),
        vocabulary
    );
    assert_eq!(
        form_prior(&[vec!["A".into(), "B".into()]]).unwrap()["A>B"],
        1.0
    );
    let mut spec = tabslib::facade::generation::RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    let changes = apply_riff_profile(
        &mut spec,
        &CallerRiffProfile {
            archetype: Some(tabslib::facade::generation::RiffArchetype::Gallop),
            ..CallerRiffProfile::default()
        },
    )
    .expect("apply caller riff profile");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, RiffProfileField::Archetype);
    let _ = std::mem::size_of::<RhythmCell>();
    let _ = std::mem::size_of::<RhythmPhrase>();
    let _ = std::mem::size_of::<RhythmicVocabulary>();
    let _ = std::mem::size_of::<RhythmProfileError>();
    let _ = std::mem::size_of::<RiffProfileChange>();
    let _ = std::mem::size_of::<RiffProfileValue>();
}

#[test]
fn every_reported_capability_is_available_in_a_full_build() {
    for capability in AVAILABLE {
        assert_eq!(
            capability_status(capability),
            CapabilityStatus::Available,
            "{capability:?} is named but not available"
        );
    }
}

#[test]
fn song_dynamics_are_reachable_through_the_facade() {
    let plan = ArrangementPlan::new(
        3,
        vec![SectionRequest {
            label: "A".into(),
            bars: 1,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    assert_eq!(
        apply_energy_contour(&plan, &[72])
            .expect("valid facade energy contour")
            .sections[0]
            .energy,
        Some(72)
    );
    let document = tabslib::fixtures::power_chords();
    assert!(apply_tempo_contour(
        &document,
        &[document.master_bars.len()],
        &[120.0],
        TempoBounds {
            minimum_bpm: 60.0,
            maximum_bpm: 180.0,
        }
    )
    .is_ok());
    assert!(diff_plan_sections(&plan, &plan).is_empty());
    let _ = compose_plan_incremental;
}

#[test]
fn preference_ranker_is_reachable_through_the_facade() {
    let _ = preference_ranker_agreement;
    let _ = rank_preference_candidates;
    let _ = std::mem::size_of::<PreferenceRanker>();
    let _ = std::mem::size_of::<PreferenceTrainingPair>();
    let document = tabslib::fixtures::power_chords();
    let report = tabslib::facade::generation::assess_document(&document);
    assert_eq!(
        PreferenceCandidate::from_report("candidate", &report).label,
        "candidate"
    );
}

#[test]
fn arrangement_search_and_exact_time_budget_are_reachable_through_the_facade() {
    let source = tabslib::fixtures::power_chords();
    assert_eq!(
        bar_duration((4, 4), 120.0).unwrap(),
        ExactDuration {
            numerator: 2,
            denominator: 1
        }
    );
    assert_eq!(bars_for_minutes(&source, 1).unwrap().bars, 30);

    let motifs = segment_motifs(&source, 1, 1.0).expect("valid score fingerprints");
    let plan = ArrangementPlan::new(
        7,
        vec![SectionRequest {
            label: "Section".into(),
            bars: 1,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let candidates = search_arrangements(
        &source,
        &motifs,
        &plan,
        &ArrangementSearchRequest {
            candidate_count: 1,
            seed: 4,
            target_minutes: None,
            weights: ArrangementSearchWeights::default(),
        },
    )
    .expect("one candidate can be searched");
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        capability_status(Capability::ArrangementSearch),
        CapabilityStatus::Available
    );
}

#[test]
fn riff_adaptation_is_planned_and_applied_through_the_facade() {
    let mut document = tabslib::fixtures::power_chords();
    let before = document.clone();
    let plan = plan_riff_adaptation(
        &document,
        RiffAdaptationRequest {
            track: 0,
            start_bar: 0,
            end_bar: 0,
            target_key: facade::model::KeySignature {
                tonic: 5,
                minor: false,
            },
            target_tempo: None,
            target_meter: None,
            fret_window: (0, 24),
        },
    )
    .expect("riff is playable in the destination key");
    assert_eq!(document, before, "planning is read-only");
    apply_riff_adaptation(&mut document, &plan).expect("explicit application succeeds");
    assert_eq!(document.key, Some(plan.key.target));
}

#[test]
fn riff_hybrids_and_candidate_filter_are_reachable_through_the_facade() {
    let first = tabslib::fixtures::power_chords();
    let hybrid = hybridize(
        &first,
        &first,
        HybridRequest {
            track_a: 0,
            track_b: 0,
            mode: HybridMode::InterleaveBars,
            seed: 17,
        },
    )
    .expect("compatible riffs");
    let results = filter_candidates(
        [Candidate {
            name: "hybrid".into(),
            document: hybrid,
        }],
        &[],
        &[],
    );
    assert!(results[0].accepted);
    assert!(!results[0].assessment.metrics.is_empty());
}

#[test]
fn token_solo_composition_and_caller_ranking_are_reachable_through_the_facade() {
    let document = tabslib::fixtures::power_chords();
    let candidates = compose_solo_tokens(
        &document,
        &TokenSoloRequest {
            start_bar: 0,
            end_bar: 0,
            key: facade::model::KeySignature {
                tonic: 0,
                minor: false,
            },
            scale: vec![0, 2, 4, 5, 7, 9, 11],
            register: (52, 79),
            tuning: vec![40, 45, 50, 55, 59, 64],
            fret_window: (0, 12),
            candidate_count: 2,
            seed: 4,
            track_name: "Facade Solo".into(),
        },
        "degree=1 degree=3 degree=5 degree=7",
    )
    .expect("tokens fit and can be fingered");
    let ranked = rank_solo_candidates(candidates, |report| {
        i64::try_from(report.metrics.len()).unwrap_or(0)
    });
    assert_eq!(ranked.len(), 2);
}

#[test]
fn explicit_second_guitar_roles_are_available_through_the_facade() {
    let mut document = tabslib::fixtures::power_chords();
    let mut config = SecondGuitarConfig::contextual(0, 3);
    config.role = GuitarRole::Octave;
    config.tuning = document.tracks[0].tuning.clone();
    let result = generate_second_guitar(&mut document, &config, false)
        .expect("source role and fret window are compatible");
    assert_eq!(document.tracks.len(), 2);
    assert_eq!(result.track_index, 1);
}

#[test]
fn candidate_decks_and_feature_vectors_are_reachable_through_the_facade() {
    let deck = build_candidate_deck(
        &[DeckCandidateInput {
            id: "facade-candidate".into(),
            archetype: "caller-defined".into(),
            document: tabslib::fixtures::power_chords(),
        }],
        DeckRequest {
            candidate_count: 1,
            minimum_distance: 0.0,
            page_size: 1,
            seed: 2,
        },
    )
    .expect("one candidate makes a deck");
    assert_eq!(deck.current_page()[0].id, "facade-candidate");
    assert!(deck.current_page()[0].features.density > 0.0);
}

#[test]
fn song_forms_are_planned_through_the_facade() {
    let sections = plan_form(FormTemplate::VerseChorus, 4, &[0], 0).expect("a form");
    assert!(!sections.is_empty(), "a form plans at least one section");
    assert!(sections.iter().all(|section| section.bars == 4));
}

#[test]
fn section_transitions_are_planned_and_applied_through_the_facade() {
    let mut document = tabslib::fixtures::meters_and_sections();
    let rules = [TransitionRule::new(TransitionKind::Cutoff, 0)];
    let planned: Vec<SectionTransition> = plan_transitions(&document, &rules);
    let applied = apply_transitions(&mut document, &rules).expect("a valid track index");
    assert_eq!(
        applied.len(),
        planned.len(),
        "planning and applying must agree on how many transitions there are"
    );
}

#[test]
fn section_analysis_reports_boundaries_and_applies_only_explicit_markers() {
    let mut document = tabslib::fixtures::power_chords();
    let boundaries = detect_section_boundaries(&document, 0.8).expect("valid fingerprints");
    assert!(
        !boundaries.is_empty(),
        "changed bar fingerprints create boundaries"
    );
    let labels = (0..boundaries.len())
        .map(|index| format!("Section {index}"))
        .collect::<Vec<_>>();
    let proposals = propose_section_markers(&boundaries, &labels).expect("one label per boundary");
    let applied = apply_section_marker_proposals(&mut document, &proposals)
        .expect("explicit marker application");
    assert_eq!(applied.len(), boundaries.len());

    let stabs: Vec<RiffStab> = detect_riff_stabs(&document, &[0], 1).expect("pitched source");
    assert!(stabs.is_empty(), "unaccented fixture has no riff stabs");
    let _ = SectionAnalysisError::InvalidThreshold;
    let _ = SectionBoundary {
        bar_index: 1,
        confidence: 0.9,
        similarity: Some(0.1),
        already_marked: false,
    };
    let _ = SectionMarkerProposal {
        bar_index: 1,
        label: "Verse".into(),
    };
    let _ = SectionMergeProposal {
        remove_marker_at: 1,
        left_bar: 0,
        right_bar: 1,
        similarity: 1.0,
    };
    let _ = SectionRange {
        start_bar: 0,
        end_bar: 1,
    };
    let _ = SingleBarFoldProposal {
        section_bar: 1,
        into_bar: 0,
        remove_marker_at: 1,
        similarity: 1.0,
    };
}

#[test]
fn pattern_catalogs_are_built_and_constrained_through_the_facade() {
    let grid = PatternGrid::new((4, 4), 4).expect("a supported grid");
    assert_eq!(grid.steps_per_bar().expect("steps"), 16);
    let constraints = PatternConstraints::new(grid, 1).expect("constraints");
    let catalog = RhythmSectionPatternCatalog::new(Vec::new()).expect("an empty catalog");
    assert!(
        catalog.select_seeded(&constraints, 0).is_err(),
        "an empty catalog reports an error rather than inventing a pattern"
    );
}

#[test]
fn odd_meter_grouping_is_selectable_through_the_facade() {
    let default = BeatGrouping::for_meter((7, 8)).expect("default additive grouping");
    let override_grouping =
        BeatGrouping::new((7, 8), vec![3, 2, 2]).expect("caller-selected additive grouping");
    assert_eq!(
        default.backbeat_steps(4).expect("default backbeats"),
        vec![4]
    );
    assert_eq!(
        override_grouping
            .backbeat_steps(4)
            .expect("override backbeats"),
        vec![6]
    );
    let mut document = tabslib::fixtures::meters_and_sections();
    let profile = DrumProfile {
        kick_voice: 36,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 0,
        snare_probability: 100,
        cymbal_probability: 0,
    };
    let generated = facade::generation::generate_drums_with_groupings(
        &mut document,
        DrumGenerationRequest {
            source_tracks: &[0],
            profile: &profile,
            seed: 7,
            tempo: 120,
        },
        &[override_grouping],
    )
    .expect("facade odd-meter drum generation");
    assert!(
        generated.note_count > 0,
        "grouped generation writes drum notes"
    );
}

#[test]
fn drum_performance_is_driven_through_the_facade() {
    let pattern = DrumPattern {
        steps_per_bar: 16,
        bars: vec![(0..16)
            .map(|step| PatternStep {
                onset: step % 4 == 0,
                accent: false,
                dead: false,
                palm_mute: false,
            })
            .collect()],
    };
    let profile = DrumProfile {
        kick_voice: 36,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 100,
        snare_probability: 100,
        cymbal_probability: 100,
    };
    let mut hits: Vec<DrumHit> = generate_seeded_hits(&pattern, &profile, 7);
    assert!(!hits.is_empty(), "a seeded profile places hits");
    let before = hits.clone();

    humanize_hits(
        &mut hits,
        7,
        HumanizationConfig {
            velocity_range: 8,
            timing_range: 4,
        },
    );
    let _ = pair_hihat_chokes(&mut hits);
    let _ = FillPattern {
        length: 4,
        hits: Vec::new(),
    };
    let _ = DrumOrnament::Flam;
    let _ = PhraseClock;

    let assessment = assess_drums(&before, &pattern, profile.kick_voice);
    let report = improve_drums(
        &mut hits,
        &pattern,
        DrumImprovementConfig {
            kick_voice: profile.kick_voice,
            target_kick_lock_percent: 50,
            max_repeated_bar_percent: 100,
            max_passes: 2,
        },
    );
    assert_eq!(
        report.initial.locked_onsets, assessment.locked_onsets,
        "the improvement run starts from the same assessment the caller can take"
    );
}

#[test]
fn authored_drum_feel_is_profiled_through_the_facade() {
    let document = tabslib::fixtures::drum_patterns();
    let profile = profile_existing_drums(&document, 0).expect("fixture drum track");
    assert_eq!(profile.cymbal_preference, Some(CymbalPreference::Hat));
    assert_eq!(profile.feel, DrumFeel::Straight);
    assert_eq!(profile.kick_density_per_bar, [2, 16, 8, 0, 1]);
}

#[test]
fn midi_grooves_are_imported_indexed_selected_and_adapted_through_the_facade() {
    let bytes = tabslib::format::midi::write(&tabslib::fixtures::drum_patterns());
    let grid = PatternGrid::new((4, 4), 4).expect("sixteenth grid");
    let bars = midi_to_drum_hits(&bytes, grid).expect("MIDI groove converts");
    assert!(!bars.is_empty());
    assert_eq!(
        detect_midi_groove_feel(&bytes),
        Ok(MidiGrooveFeel::Straight)
    );

    let mut index = MidiGrooveIndex::new();
    index
        .add_midi("straight", "verse", 120, grid, &bytes)
        .expect("groove is indexed");
    let selected: &MidiGroove = index.select("verse", (4, 4), 118).expect("nearest tempo");
    assert_eq!(selected.id(), "straight");
    let result = generate_from_midi_index(&index, "verse", grid, 118, &[], 36, None)
        .expect("indexed groove selected");
    assert_eq!(
        result.source,
        MidiGrooveSource::Indexed {
            groove_id: "straight".into()
        }
    );
    let _ = adapt_midi_groove_to_riff;

    let pattern = DrumPattern {
        steps_per_bar: 16,
        bars: vec![vec![PatternStep::default(); 16]],
    };
    let profile = DrumProfile {
        kick_voice: 36,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 0,
        snare_probability: 0,
        cymbal_probability: 0,
    };
    let fallback = SeededGrooveFallback {
        pattern: &pattern,
        profile: &profile,
        seed: 9,
    };
    let generated = generate_from_midi_index(&index, "chorus", grid, 118, &[], 36, Some(fallback))
        .expect("caller explicitly requested fallback");
    assert_eq!(generated.source, MidiGrooveSource::SeededFallback);
    let _ = MidiGrooveError::NoDrumNotes;
    let _ = MidiGrooveGeneration {
        hits: Vec::new(),
        source: MidiGrooveSource::SeededFallback,
    };
}

#[test]
fn assessment_and_inspection_stay_reachable_through_the_facade() {
    let document = tabslib::fixtures::power_chords();
    let statistics = facade::inspect::statistics(&document);
    assert!(statistics.notes > 0);
    let report = facade::generation::assess_document(&document);
    assert!(!report.metrics.is_empty());
}
