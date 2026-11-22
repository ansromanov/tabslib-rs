use tabslib::facade::generation::{
    capability_status, AccompanimentOutcome, AccompanimentRequest, AccompanimentSkipReason,
    Capability, CapabilityStatus, UnsupportedReason,
};

#[test]
fn accompaniment_is_available_from_the_stable_facade_without_generation_features() {
    let source_tracks = [0];
    let request = AccompanimentRequest::new(&source_tracks, 7);
    let mut document = tabslib::fixtures::power_chords();

    let result =
        tabslib::facade::generation::generate_accompaniment(&mut document, &request).unwrap();

    assert_eq!(result.roles.len(), 5);
    assert!(result.roles.iter().all(|role| {
        role.outcome == AccompanimentOutcome::Skipped(AccompanimentSkipReason::NotRequested)
    }));
}

#[test]
fn capability_status_distinguishes_unavailable_work_from_disabled_features() {
    use CapabilityStatus::{Available, Unsupported};
    use UnsupportedReason::FeatureDisabled;

    assert_eq!(capability_status(Capability::ScoreInspection), Available);
    assert_eq!(capability_status(Capability::DocumentEdits), Available);
    assert_eq!(
        capability_status(Capability::AccompanimentWorkflow),
        Available
    );
    let provider_status = if cfg!(any(
        feature = "ai-ollama",
        feature = "ai-openai-compatible",
        feature = "ai-gemini"
    )) {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(
        capability_status(Capability::AiProviderIntegration),
        provider_status
    );

    let expected = if cfg!(feature = "harmony") {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(capability_status(Capability::SoloGeneration), expected);

    let expected = if cfg!(all(feature = "drums", feature = "rhythm")) {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(capability_status(Capability::DrumGeneration), expected);
    assert_eq!(capability_status(Capability::BassGeneration), expected);

    let expected = if cfg!(feature = "harmony") {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(capability_status(Capability::HarmonyGeneration), expected);

    let expected = if cfg!(feature = "arrangement") {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(capability_status(Capability::Arrangement), expected);

    let expected = if cfg!(feature = "assessment") {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(capability_status(Capability::Assessment), expected);
    assert_eq!(capability_status(Capability::Calibration), expected);

    let expected = if cfg!(feature = "ai") {
        Available
    } else {
        Unsupported(FeatureDisabled)
    };
    assert_eq!(capability_status(Capability::AiPrompting), expected);
}

#[cfg(all(feature = "harmony", feature = "assessment", feature = "ai"))]
#[test]
fn stable_facade_exposes_solo_and_provider_neutral_rationale_workflows() {
    use tabslib::facade::generation::{
        self, capture_request, Capability, CapabilityStatus, PromptInput, SoloConfig,
    };

    let mut document = tabslib::fixtures::repeated_frets();
    let solo = generation::generate_solo(&mut document, &SoloConfig::c_major(0, 1, 42)).unwrap();
    assert!(solo.note_count > 0);
    assert_eq!(
        generation::capability_status(Capability::SoloGeneration),
        CapabilityStatus::Available
    );

    let report = generation::assess_document(&document);
    let prompt = PromptInput::rationale("candidate", &report);
    let request = capture_request("offline-test", prompt).unwrap();
    assert!(request.prompt().contains("structured facts"));
    assert_eq!(
        generation::capability_status(Capability::AiPrompting),
        CapabilityStatus::Available
    );
}

#[cfg(all(feature = "ai", feature = "ascii", feature = "assessment"))]
#[test]
fn stable_facade_exposes_candidate_judging_and_tab_critique() {
    use std::collections::BTreeMap;

    use tabslib::facade::generation::{
        self, blend_rankings_with_deterministic, critique_tab, offline_heuristic_judge,
        CandidateRanking, Capability, CapabilityStatus, CapturedRequest, PromptCandidate,
        PromptInput, RationaleProvider,
    };

    struct MockJudge;
    impl RationaleProvider for MockJudge {
        fn name(&self) -> &str {
            "facade-test"
        }

        fn complete(&self, request: &CapturedRequest) -> Result<String, String> {
            assert!(request.prompt().contains("ASCII tablature"));
            Ok(r#"{"schema_version":1,"findings":[]}"#.into())
        }
    }

    let document = tabslib::fixtures::repeated_frets();
    let report = generation::assess_document(&document);
    let candidates = PromptInput::judging(vec![
        PromptCandidate {
            label: "alpha".into(),
            assessment: report.clone(),
        },
        PromptCandidate {
            label: "beta".into(),
            assessment: report,
        },
    ]);
    let offline = offline_heuristic_judge(
        &candidates,
        &BTreeMap::from([(generation::Metric::Density, 1.0)]),
    )
    .unwrap();
    let blended = blend_rankings_with_deterministic(
        &CandidateRanking {
            version: 1,
            ranking: vec!["beta".into(), "alpha".into()],
        },
        &offline.ranking,
        0.5,
    )
    .unwrap();
    assert_eq!(blended.ranking.len(), 2);
    assert_eq!(
        generation::capability_status(Capability::AiJudging),
        CapabilityStatus::Available
    );
    let critique = critique_tab(&MockJudge, "riff", &document).unwrap();
    assert!(critique.findings.is_empty());
    assert_eq!(
        generation::capability_status(Capability::TabCritique),
        CapabilityStatus::Available
    );
}

#[cfg(all(
    feature = "gp",
    feature = "drums",
    feature = "rhythm",
    feature = "harmony",
    feature = "arrangement",
    feature = "assessment"
))]
mod end_to_end {
    use tabslib::facade::generation::{
        self, ArrangementRequest, BassGenerationRequest, DrumGenerationRequest, DrumProfile,
        HarmonyConfig, HarmonyType, Metric, SectionRequest,
    };
    use tabslib::facade::{self, Document};

    fn run_native_workflow() -> (Document, String) {
        let mut document = tabslib::fixtures::power_chords();
        let source_tracks = [0];
        let bass_tuning = [28, 33, 38, 43, 47];
        let drum_profile = DrumProfile {
            kick_voice: 35,
            snare_voice: 38,
            cymbal_voice: 42,
            kick_probability: 100,
            snare_probability: 100,
            cymbal_probability: 100,
        };

        let drums = generation::generate_drums(
            &mut document,
            DrumGenerationRequest {
                source_tracks: &source_tracks,
                profile: &drum_profile,
                seed: 41,
                tempo: 120,
            },
        )
        .unwrap();
        let bass = generation::generate_bass(
            &mut document,
            BassGenerationRequest {
                source_tracks: &source_tracks,
                profile: generation::BassProfile {
                    min_midi: 28,
                    max_midi: 52,
                    max_leap: 7,
                    accent_velocity: 110,
                    normal_velocity: 90,
                },
                strategy: generation::BassStrategy::PumpingEighths,
                tuning: generation::BassTuning::Custom(&bass_tuning),
                max_fret: 12,
            },
        )
        .unwrap();
        let harmony = generation::generate_harmony(
            &mut document,
            HarmonyConfig {
                source_track: 0,
                key: None,
                harmony: HarmonyType::Parallel { semitones: 7 },
                tuning: tabslib::fixtures::TUNING_E_STANDARD.to_vec(),
                max_fret: 24,
                max_leap: 12,
                octave_range: 1,
                seed: 41,
                track_name: "Second Guitar".into(),
            },
        )
        .unwrap();

        assert!(drums.note_count > 0);
        assert!(bass.note_count > 0);
        assert_eq!(document.tracks[bass.track_index].tuning, bass_tuning);
        assert!(harmony.note_count > 0);
        facade::edits::rename_track(&mut document, bass.track_index, "Bass").unwrap();
        assert_eq!(facade::inspect::statistics(&document).tracks, 4);

        let motifs =
            generation::segment_motifs(&document, document.master_bars.len(), 0.0).unwrap();
        let bars = document.master_bars.len();
        let arranged = generation::arrange(
            &document,
            ArrangementRequest {
                motifs: &motifs,
                sections: &[SectionRequest {
                    label: "A".into(),
                    bars,
                    candidates: vec![0],
                    transpose_semitones: 0,
                    energy: None,
                    instrumentation: None,
                    energy_texture: None,
                    climax: false,
                }],
                seed: 41,
            },
        )
        .unwrap();
        let report = generation::assess_document(&arranged);
        assert!(report.metrics.contains_key(&Metric::Playability));
        (arranged, report.to_json())
    }

    #[test]
    fn one_facade_covers_inspection_edits_generation_arrangement_and_assessment() {
        let (first, first_report) = run_native_workflow();
        let (second, second_report) = run_native_workflow();

        assert_eq!(first, second);
        assert_eq!(first_report, second_report);

        let bytes = <facade::format::gp::Gp as facade::format::WriteFormat>::write(&first).unwrap();
        let reloaded =
            <facade::format::gp::Gp as facade::format::ReadFormat>::read(&bytes).unwrap();
        let before = facade::inspect::statistics(&first);
        let after = facade::inspect::statistics(&reloaded);
        assert_eq!(before.notes, after.notes);
        assert_eq!(before.durations, after.durations);
        assert_eq!(
            generation::assess_document(&first).to_json(),
            generation::assess_document(&reloaded).to_json()
        );
    }
}
