use super::*;
use crate::generation::arrangement::{ArrangementPlan, SectionRequest};

fn sample_plan() -> ArrangementPlan {
    ArrangementPlan::new(
        1234,
        vec![SectionRequest {
            label: "verse".to_string(),
            bars: 4,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: Some(50),
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    )
}

#[test]
fn compute_source_hash_deterministic_16_chars() {
    let bytes = b"original source score content";
    let hash = compute_source_hash(bytes);
    assert_eq!(hash.len(), 16);
    assert_eq!(hash, compute_source_hash(bytes));
    assert_ne!(hash, compute_source_hash(b"modified source score"));
}

#[test]
fn plan_roundtrip_with_matching_source() {
    let plan = sample_plan();
    let source = b"test score bytes";
    let json = save_plan(&plan, source, "0.1.0", Vec::new()).expect("save_plan succeeds");
    let (loaded_plan, sidecar) =
        load_plan(&json, Some(source)).expect("load_plan succeeds with matching source");
    assert_eq!(loaded_plan, plan);
    assert_eq!(sidecar.version, PLAN_VERSION);
    assert_eq!(sidecar.source_hash, compute_source_hash(source));
}

#[test]
fn load_plan_rejects_stale_source_modification() {
    let plan = sample_plan();
    let original_source = b"score v1";
    let modified_source = b"score v2";
    let json = save_plan(&plan, original_source, "0.1.0", Vec::new()).expect("save_plan succeeds");

    let err = load_plan(&json, Some(modified_source))
        .expect_err("stale source modification must be rejected");

    match err {
        PlanSidecarError::SourceHashMismatch { expected, actual } => {
            assert_eq!(expected, compute_source_hash(original_source));
            assert_eq!(actual, compute_source_hash(modified_source));
        }
        other => panic!("expected SourceHashMismatch, got {other:?}"),
    }
}

#[test]
fn load_plan_rejects_unsupported_version() {
    let mut sidecar = plan_to_sidecar(&sample_plan(), b"source", "0.1.0", Vec::new());
    sidecar.version = PLAN_VERSION + 1;
    let json = serde_json::to_string(&sidecar).unwrap();

    let err = load_plan(&json, None).expect_err("future version must be rejected");
    match err {
        PlanSidecarError::UnsupportedVersion {
            version,
            max_supported,
        } => {
            assert_eq!(version, PLAN_VERSION + 1);
            assert_eq!(max_supported, PLAN_VERSION);
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
}
