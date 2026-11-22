//! Integration tests for the arrangement-plan surface reachable through the facade.

use tabslib::facade::{
    self,
    generation::{
        apply_intent, apply_performance_layer, assert_texture_class_preserved, assign_roles,
        candidate_motifs_for_role, classify_bar_texture, compute_source_hash, load_plan, save_plan,
        verify_bar_claims, verify_bar_claims_in_tab, ArrangementIntent, ArrangementPlan, BarClaim,
        BarTexture, MotifEnergy, MotifRole, PerformanceLayerConfig, PlanEdit, PlanSidecarError,
        RoleAssignment, SectionRequest, SlotAddress, TextureError, PLAN_VERSION,
    },
};
use tabslib::fixtures;

#[test]
fn item1_motif_roles_and_candidates() {
    let motifs = [
        MotifEnergy::new(1, 0.2),
        MotifEnergy::new(2, 0.8),
        MotifEnergy::new(3, 0.5),
    ];

    let roles = assign_roles(&motifs);
    assert_eq!(
        roles,
        RoleAssignment {
            primary: 1,
            secondary: 2,
            tertiary: 3,
        }
    );

    let candidates = candidate_motifs_for_role(&motifs, MotifRole::Primary, 1);
    assert_eq!(candidates.ids, vec![1, 3]);
    assert_eq!(candidates.weights, vec![3, 1]);
}

#[test]
fn item2_apply_intent_with_error_isolation() {
    let plan = ArrangementPlan::new(
        100,
        vec![SectionRequest {
            label: "verse".into(),
            bars: 4,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: Some(50),
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );

    let intent = ArrangementIntent {
        intent: "Double verse and drop non-existent chorus".into(),
        edits: vec![
            PlanEdit::Repeat {
                address: SlotAddress::label("verse"),
                times: 2,
            },
            PlanEdit::Drop {
                address: SlotAddress::label("nonexistent"),
            },
        ],
        timestamp: Some("2026-09-19T00:00:00Z".into()),
    };

    let result = apply_intent(&plan, &intent, &[]).expect("apply_intent succeeds");
    assert_eq!(result.applied.len(), 1);
    assert_eq!(result.rejected.len(), 1);
    assert_eq!(result.plan.sections.len(), 2);
}

#[test]
fn item3_plan_sidecars_and_stale_source_rejection() {
    let plan = ArrangementPlan::new(
        42,
        vec![SectionRequest {
            label: "intro".into(),
            bars: 4,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );

    let original_bytes = b"source document version 1";
    let sidecar_json = save_plan(&plan, original_bytes, "0.1.0", Vec::new()).expect("save_plan");

    // Success on same source
    let (loaded, sidecar) =
        load_plan(&sidecar_json, Some(original_bytes)).expect("load_plan matches source");
    assert_eq!(loaded, plan);
    assert_eq!(sidecar.version, PLAN_VERSION);

    // Rejection on modified source
    let modified_bytes = b"source document modified";
    let err = load_plan(&sidecar_json, Some(modified_bytes))
        .expect_err("stale source must be rejected via compute_source_hash");
    match err {
        PlanSidecarError::SourceHashMismatch { expected, actual } => {
            assert_eq!(expected, compute_source_hash(original_bytes));
            assert_eq!(actual, compute_source_hash(modified_bytes));
        }
        other => panic!("expected SourceHashMismatch, got {other:?}"),
    }
}

#[test]
fn item4_drum_performance_layer_pipeline() {
    use tabslib::generation::drums::{voices, DrumHit};

    let mut hat = DrumHit::new(voices::OPEN_HI_HAT, 0);
    hat.accent = true;
    let mut snare = DrumHit::new(voices::ACOUSTIC_SNARE, 4);
    snare.ghost = true;
    let closer_hat = DrumHit::new(voices::CLOSED_HI_HAT, 4);

    let mut ctx = PerformanceLayerConfig::new(140);
    ctx.intensity = 2; // Un-ghost backbeat snare
    ctx.backbeat_steps = vec![4];
    ctx.stab_steps = vec![8]; // Catch stab with kick and china

    let hits = apply_performance_layer(&[hat, snare, closer_hat], &ctx);

    // Snare at step 4 is unghosted and accented
    let s = hits
        .iter()
        .find(|h| h.midi == voices::ACOUSTIC_SNARE && h.step == 4)
        .expect("snare present");
    assert!(!s.ghost);
    assert!(s.accent);

    // Open hat is choked at step 4
    let h = hits
        .iter()
        .find(|h| h.midi == voices::OPEN_HI_HAT)
        .expect("open hat present");
    assert_eq!(h.choked_at_step, Some(4));

    // Stab catch inserted at step 8
    assert!(hits
        .iter()
        .any(|h| h.midi == voices::ACOUSTIC_BASS_DRUM && h.step == 8));
    assert!(hits
        .iter()
        .any(|h| h.midi == voices::CHINESE_CYMBAL && h.step == 8));
}

#[test]
fn item5_verify_bar_claims_doc_and_tab() {
    let doc = fixtures::scale_run();
    let claims = vec![
        BarClaim {
            bar: 0,
            assertion: "exposition".into(),
            value: None,
        },
        BarClaim {
            bar: 999,
            assertion: "out of range".into(),
            value: None,
        },
    ];

    let verified = verify_bar_claims(&doc, &claims);
    assert_eq!(verified.len(), 2);
    assert!(verified[0].verified);
    assert!(!verified[1].verified);
    assert!(verified[1].reason.is_some());

    let tab_text = "Guitar [1..=4]\n| 0 | 1 | 2 | 3 |";
    let tab_verified = verify_bar_claims_in_tab(tab_text, &claims);
    assert!(tab_verified[0].verified);
    assert!(!tab_verified[1].verified);
}

#[test]
fn item6_texture_classification_and_assertion() {
    let doc = fixtures::scale_run();
    let bar0 = &doc.bars[0];
    let texture = classify_bar_texture(bar0);
    assert!(matches!(
        texture,
        BarTexture::Silent | BarTexture::Sustain | BarTexture::Chug | BarTexture::Movement
    ));

    let before = [BarTexture::Chug, BarTexture::Movement];
    let after = [BarTexture::Chug, BarTexture::Movement];
    assert!(assert_texture_class_preserved(&before, &after, "adaptation").is_ok());

    let mismatched = [BarTexture::Chug, BarTexture::Sustain];
    let err = assert_texture_class_preserved(&before, &mismatched, "adaptation")
        .expect_err("texture mismatch");
    assert!(matches!(err, TextureError::TextureMismatch { bar: 2, .. }));
}

#[test]
fn item7_assess_drums_fast_and_detect_marked_sections() {
    let doc = fixtures::meters_and_sections();
    let sections = facade::detect_marked_sections(&doc, 0);
    assert!(!sections.is_empty());
    assert_eq!(sections[0].name, "Intro");

    // Add drums to meters_and_sections doc to assess
    let mut combined = doc.clone();
    let drum_doc = fixtures::drum_patterns();
    let drum_track_idx = combined.tracks.len();
    combined.tracks.push(drum_doc.tracks[0].clone());

    for (bar_idx, mb) in combined.master_bars.iter_mut().enumerate() {
        if let Some(drum_bar_id) = drum_doc
            .master_bars
            .get(bar_idx)
            .and_then(|dmb| dmb.bar_ids.first())
        {
            mb.bar_ids.push(*drum_bar_id);
            if let Some(dbar) = drum_doc.bars.iter().find(|b| b.id == *drum_bar_id as u32) {
                combined.bars.push(dbar.clone());
            }
        } else {
            mb.bar_ids.push(-1);
        }
    }

    let result = facade::assess_drums_fast(
        &combined,
        facade::AssessDrumsOptions {
            track: Some(drum_track_idx),
            source_track: Some(0),
            ..facade::AssessDrumsOptions::default()
        },
    )
    .expect("assess_drums_fast");

    assert_eq!(result.drum_track_name, "drums");
    assert_eq!(result.source_track_name, "gtr");
    assert!(!result.sections.is_empty());
}
