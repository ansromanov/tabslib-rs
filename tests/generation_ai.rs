#![cfg(feature = "ai")]

use std::collections::BTreeMap;

use serde_json::json;
use tabslib::fixtures;
use tabslib::generation::ai::{
    blend_rankings_with_deterministic, blind_pair, capture_request, classify_provider_failure,
    compare_candidates, format_rationale, generate_rationale, generate_section_boundaries,
    generate_section_labels, judge_candidates, judge_preference_agreement, offline_heuristic_judge,
    parse_candidate_ranking, parse_response, parse_section_boundary_response,
    parse_section_labels_response, AiError, AiFailureReason, CandidateRanking, CapturedRequest,
    MetricDirection, PromptCandidate, PromptInput, Rationale, RationaleProvider,
    SectionFingerprint,
};
#[cfg(feature = "ascii")]
use tabslib::generation::ai::{
    capture_tab_critique_request, critique_tab, parse_tab_critique_response, TabDefectClass,
};
use tabslib::generation::assessment::{assess, AssessmentReport, Measurement, Metric};
use tabslib::generation::preference::{CandidateRef, Choice, PreferenceRecord, RECORD_VERSION};

struct FakeProvider {
    response: Result<String, String>,
}

impl RationaleProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake-provider"
    }

    fn complete(&self, _request: &CapturedRequest) -> Result<String, String> {
        self.response.clone()
    }
}

fn response_for(
    label: &str,
    code: &str,
    region: tabslib::generation::assessment::Region,
) -> String {
    json!({
        "rationale": "The supplied report shows the cited score fact.",
        "findings": [{
            "candidate": label,
            "code": code,
            "bar": region.bar,
            "track": region.track,
            "voice": region.voice,
            "beat": region.beat,
            "detail": "This finding matches an assessment diagnostic."
        }]
    })
    .to_string()
}

#[test]
fn prompt_capture_is_stable_and_contains_only_assessment_facts() {
    let score = fixtures::repeated_frets();
    let report = assess(&score);
    let input = PromptInput::rationale("candidate", &report);
    let first = capture_request("fake-provider", input.clone()).unwrap();
    let second = capture_request("fake-provider", input).unwrap();
    assert_eq!(first, second);
    assert!(first.context_json().contains("\"metrics\""));
    assert!(!first.context_json().contains("5555"));
    assert_eq!(first.context_fingerprint().len(), 16);
}

#[test]
fn fake_provider_rationale_is_validated_captured_and_formatted_deterministically() {
    let score = fixtures::repeated_frets();
    let report = assess(&score);
    let diagnostic = &report.diagnostics[0];
    let response = response_for("candidate", diagnostic.code, diagnostic.region);
    let provider = FakeProvider {
        response: Ok(response),
    };
    let rationale =
        generate_rationale(&provider, PromptInput::rationale("candidate", &report)).unwrap();
    assert_eq!(rationale.request.provider(), "fake-provider");
    assert_eq!(rationale.findings.len(), 1);
    assert_eq!(rationale.findings[0].code, diagnostic.code);
    assert!(format_rationale(&rationale).contains(rationale.request.context_fingerprint()));
    assert_eq!(
        format_rationale(&rationale),
        format_rationale(&rationale.clone())
    );
}

#[test]
fn malformed_unsupported_and_provider_failures_are_explicit() {
    let report = assess(&fixtures::repeated_frets());
    let request = capture_request(
        "fake-provider",
        PromptInput::rationale("candidate", &report),
    )
    .unwrap();
    assert!(matches!(
        parse_response(&request, "not json"),
        Err(AiError::MalformedResponse(_))
    ));
    let unsupported = response_for(
        "candidate",
        "unsupported_code",
        report.diagnostics[0].region,
    );
    assert!(matches!(
        parse_response(&request, &unsupported),
        Err(AiError::UnsupportedFinding(_))
    ));
    let failing_provider = FakeProvider {
        response: Err("offline".into()),
    };
    assert_eq!(
        generate_rationale(
            &failing_provider,
            PromptInput::rationale("candidate", &report)
        ),
        Err(AiError::ProviderFailure {
            reason: AiFailureReason::Unknown,
            message: "offline".into(),
        })
    );
}

fn section_fingerprints() -> Vec<SectionFingerprint> {
    ["a", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(index, id)| SectionFingerprint {
            id: id.into(),
            features: [("duration_beats".into(), (index + 4) as f64)].into(),
        })
        .collect()
}

#[test]
fn section_label_prompt_captures_version_and_validates_every_index() {
    let provider = FakeProvider {
        response: Ok(json!({
            "labels": [
                {"section_index": 2, "label": "ending"},
                {"section_index": 0, "label": "opening"},
                {"section_index": 1, "label": "middle"}
            ]
        })
        .to_string()),
    };
    let result = generate_section_labels(&provider, &section_fingerprints()).unwrap();
    assert!(result
        .request
        .prompt()
        .contains("tabslib.section-labels.v1"));
    assert!(result
        .request
        .context_json()
        .contains("tabslib.section-labels.v1"));
    assert_eq!(
        result
            .labels
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>(),
        ["opening", "middle", "ending"]
    );

    let invalid = json!({"labels": [
        {"section_index": 0, "label": "one"},
        {"section_index": 0, "label": "duplicate"},
        {"section_index": 2, "label": "three"}
    ]})
    .to_string();
    let request = capture_request(
        "fake-provider",
        PromptInput::section_labels(&section_fingerprints()),
    )
    .unwrap();
    assert!(matches!(
        parse_section_labels_response(&request, &invalid),
        Err(AiError::MalformedResponse(_))
    ));
}

#[test]
fn boundary_prompt_returns_only_validated_in_range_suggestions() {
    let provider = FakeProvider {
        response: Ok(json!({
            "merges": [{"left": 0, "right": 1, "reason": "matching duration"}],
            "splits_after": [1]
        })
        .to_string()),
    };
    let sections = section_fingerprints();
    let result = generate_section_boundaries(&provider, &sections).unwrap();
    assert!(result
        .request
        .prompt()
        .contains("tabslib.section-boundaries.v1"));
    assert_eq!((result.merges[0].left, result.merges[0].right), (0, 1));
    assert_eq!(result.splits_after, [1]);

    let request =
        capture_request("fake-provider", PromptInput::section_boundaries(&sections)).unwrap();
    let out_of_range = json!({"merges": [], "splits_after": [2]}).to_string();
    assert!(matches!(
        parse_section_boundary_response(&request, &out_of_range),
        Err(AiError::MalformedResponse(_))
    ));
}

#[test]
fn provider_failures_expose_stable_typed_reasons() {
    assert_eq!(
        classify_provider_failure("unreachable: refused"),
        AiFailureReason::Unreachable
    );
    assert_eq!(
        classify_provider_failure("timeout: request took too long"),
        AiFailureReason::Timeout
    );
    assert_eq!(
        classify_provider_failure("auth: HTTP 401"),
        AiFailureReason::Auth
    );
    assert_eq!(
        classify_provider_failure("not_found: HTTP 404"),
        AiFailureReason::NotFound
    );
    assert_eq!(
        classify_provider_failure("server_error: HTTP 503"),
        AiFailureReason::ServerError
    );
    assert_eq!(
        classify_provider_failure("opaque error"),
        AiFailureReason::Unknown
    );
}

#[test]
fn candidate_comparison_reports_metric_and_diagnostic_changes_without_mutation() {
    let left_score = fixtures::repeated_frets();
    let mut right_score = left_score.clone();
    let first_bar_id = right_score.master_bars[0].bar_ids[0] as u32;
    let first_bar = right_score
        .bars
        .iter_mut()
        .find(|bar| bar.id == first_bar_id)
        .unwrap();
    first_bar.voices[0].beats[1].notes.clear();
    let left_before = left_score.clone();
    let right_before = right_score.clone();

    let input =
        PromptInput::comparison("left", &assess(&left_score), "right", &assess(&right_score));
    let evaluation = compare_candidates(&input).unwrap();
    let density = evaluation
        .metrics
        .iter()
        .find(|item| item.metric == Metric::Density)
        .unwrap();
    assert_eq!(density.left, Measurement::new(1, 1));
    assert_eq!(density.right, Measurement::new(39, 40));
    assert_eq!(density.direction, MetricDirection::Lower);
    assert!(evaluation
        .diagnostics
        .iter()
        .any(|item| item.code == "silence_beat" && item.right_values == vec![1]));
    assert_eq!(left_score, left_before);
    assert_eq!(right_score, right_before);
    assert_eq!(
        compare_candidates(&input).unwrap(),
        evaluation,
        "evaluation output is deterministic"
    );
    assert_eq!(
        compare_candidates(&PromptInput::rationale("left", &assess(&left_score))),
        Err(AiError::InvalidContext(
            "candidate comparison requires exactly two comparison candidates".into()
        ))
    );
}

#[test]
fn comparison_inputs_can_be_captured_for_provider_evaluation() {
    let score = fixtures::repeated_frets();
    let report = assess(&score);
    let input = PromptInput::comparison("A", &report, "B", &report);
    let request = capture_request("fake", input).unwrap();
    assert!(request.context_json().contains("candidate_comparison"));
    assert!(request.context_json().contains("\"label\":\"A\""));
    assert!(request.context_json().contains("\"label\":\"B\""));
    let no_findings = Rationale {
        request: request.clone(),
        text: "No supported issue was returned.".into(),
        findings: Vec::new(),
    };
    assert!(format_rationale(&no_findings).contains("No supported issue"));
}

#[test]
fn blinded_pair_order_is_seeded_and_keeps_original_labels_out_of_provider_context() {
    let score = fixtures::repeated_frets();
    let report = assess(&score);
    let first = blind_pair(
        "private left label",
        &report,
        "private right label",
        &report,
        42,
    )
    .unwrap();
    let repeated = blind_pair(
        "private left label",
        &report,
        "private right label",
        &report,
        42,
    )
    .unwrap();
    assert_eq!(first, repeated);
    let context = first.input.to_json().unwrap();
    assert!(context.contains("Candidate A"));
    assert!(context.contains("Candidate B"));
    assert!(!context.contains("private left label"));
    assert!(!context.contains("private right label"));
    assert!(
        (first.candidate_a_identity == "private left label"
            && first.candidate_b_identity == "private right label")
            || (first.candidate_a_identity == "private right label"
                && first.candidate_b_identity == "private left label")
    );
}

#[test]
fn candidate_judge_uses_a_versioned_prompt_and_validates_a_complete_ranking() {
    let first = assess(&fixtures::repeated_frets());
    let second = assess(&fixtures::power_chords());
    let input = PromptInput::judging(vec![
        PromptCandidate {
            label: "A".into(),
            assessment: first,
        },
        PromptCandidate {
            label: "B".into(),
            assessment: second,
        },
    ]);
    let provider = FakeProvider {
        response: Ok(json!({"schema_version": 1, "ranking": ["B", "A"]}).to_string()),
    };
    let ranking = judge_candidates(&provider, input.clone()).expect("structured ranking");
    assert_eq!(ranking.ranking, vec!["B", "A"]);
    let request = capture_request("fake-provider", input).expect("versioned judge prompt");
    assert!(request.context_json().contains("candidate_judging"));
    assert!(request.context_json().contains("schema_version"));
    assert!(matches!(
        parse_candidate_ranking(&request, r#"{"schema_version":1,"ranking":["A"]}"#),
        Err(AiError::InvalidRanking(_))
    ));
    assert!(matches!(
        parse_candidate_ranking(&request, r#"{"schema_version":2,"ranking":["A","B"]}"#),
        Err(AiError::MalformedResponse(_))
    ));
}

#[test]
fn ranking_blend_and_offline_judge_follow_caller_weights() {
    let model = CandidateRanking {
        version: 1,
        ranking: vec!["A".into(), "B".into(), "C".into()],
    };
    let deterministic = vec!["C".into(), "B".into(), "A".into()];
    assert_eq!(
        blend_rankings_with_deterministic(&model, &deterministic, 1.0)
            .unwrap()
            .ranking,
        model.ranking,
        "weight one must preserve the provider ranking"
    );
    assert_eq!(
        blend_rankings_with_deterministic(&model, &deterministic, 0.0)
            .unwrap()
            .ranking,
        deterministic,
        "weight zero must preserve the deterministic ranking"
    );

    let candidates = PromptInput::judging(vec![
        PromptCandidate {
            label: "sparse".into(),
            assessment: density_report(1, 2),
        },
        PromptCandidate {
            label: "dense".into(),
            assessment: density_report(3, 2),
        },
    ]);
    let heuristic =
        offline_heuristic_judge(&candidates, &BTreeMap::from([(Metric::Density, 1.0)])).unwrap();
    assert_eq!(heuristic.ranking[0], "dense");
}

#[test]
fn judge_agreement_counts_only_decisive_pairs_present_in_the_ranking() {
    let ranking = CandidateRanking {
        version: 1,
        ranking: vec!["dense".into(), "sparse".into()],
    };
    let record = PreferenceRecord {
        version: RECORD_VERSION,
        id: "held-out".into(),
        seed: 1,
        first: CandidateRef {
            label: "sparse".into(),
            seed: 2,
            context: String::new(),
        },
        second: CandidateRef {
            label: "dense".into(),
            seed: 3,
            context: String::new(),
        },
        choice: Choice::Second,
        third: None,
        provenance: None,
    };
    let agreement = judge_preference_agreement(&ranking, &[record]).unwrap();
    assert_eq!(agreement.correct, 1);
    assert_eq!(agreement.decisive, 1);
    assert_eq!(agreement.rate(), 1.0);
}

fn density_report(numerator: u64, denominator: u64) -> AssessmentReport {
    AssessmentReport {
        metrics: BTreeMap::from([(Metric::Density, Measurement::new(numerator, denominator))]),
        diagnostics: Vec::new(),
    }
}

#[cfg(feature = "ascii")]
#[test]
fn tab_critique_uses_ascii_and_rejects_unstructured_praise() {
    let document = fixtures::repeated_frets();
    let request = capture_tab_critique_request("fake-provider", "riff", &document).unwrap();
    assert!(request.tab_text().is_some_and(|tab| tab.contains("s0|")));
    assert!(request.prompt().contains("zero-based integer"));
    let provider = FakeProvider {
        response: Ok(json!({
            "schema_version": 1,
            "findings": [{
                "class": "loop-without-development",
                "severity": "warning",
                "bar": 0,
                "detail": "The repeated figure occupies this bar without a visible variation."
            }]
        })
        .to_string()),
    };
    let critique = critique_tab(&provider, "riff", &document).unwrap();
    assert_eq!(
        critique.findings[0].class,
        TabDefectClass::LoopWithoutDevelopment
    );
    assert_eq!(critique.findings[0].bar, 0);
    assert!(matches!(
        parse_tab_critique_response(&request, "This tab is excellent!"),
        Err(AiError::MalformedResponse(_))
    ));
}
