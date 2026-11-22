use super::*;

#[test]
fn arrangement_plan_round_trips_sections_seed_and_transition_rules() {
    let mut plan = ArrangementPlan::new(
        42,
        vec![SectionRequest {
            label: "Verse".into(),
            bars: 4,
            candidates: vec![1, 3],
            transpose_semitones: -2,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    plan.transitions
        .push(super::super::transitions::TransitionRule::new(
            super::super::transitions::TransitionKind::PickSlide,
            0,
        ));
    let json = plan.to_json().unwrap();
    let restored = ArrangementPlan::from_json(&json).unwrap();
    assert_eq!(
        restored, plan,
        "serialization must preserve editable plan data"
    );
}

#[test]
fn arrangement_plan_uses_bounded_motif_defaults() {
    let plan = ArrangementPlan::new(
        0,
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
    assert_eq!(plan.motif_max_bars, 16);
    assert_eq!(plan.motif_threshold, 0.25);
}

#[test]
fn arrangement_plan_rejects_unknown_versions_and_empty_sections() {
    let unknown = r#"{"version":2,"seed":0,"sections":[],"transitions":[]}"#;
    assert!(matches!(
        ArrangementPlan::from_json(unknown),
        Err(ArrangementError::InvalidInput(_))
    ));
    let empty = ArrangementPlan::new(0, Vec::new()).to_json().unwrap();
    assert!(ArrangementPlan::from_json(&empty).is_err());
}

#[test]
fn arrangement_plan_accepts_omitted_optional_fields() {
    let json = r#"{"version":1,"seed":4,"sections":[{"label":"A","bars":1,"candidates":[0]}]}"#;
    let plan = ArrangementPlan::from_json(json).unwrap();
    assert_eq!(plan.sections[0].transpose_semitones, 0);
    assert!(plan.transitions.is_empty());
}

#[test]
fn arrangement_plan_accepts_one_climax_and_rejects_two() {
    let source = crate::fixtures::repeated_frets();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: fingerprint_bar(&source, 0).unwrap(),
    };
    let sections = vec![
        SectionRequest {
            label: "Verse".into(),
            bars: 1,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        },
        SectionRequest {
            label: "Climax".into(),
            bars: 1,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: true,
        },
    ];
    let plan = ArrangementPlan::new(0, sections.clone());
    assert!(compose_plan(&source, std::slice::from_ref(&motif), &plan).is_ok());
    let mut invalid = sections;
    invalid[0].climax = true;
    assert!(compose_song(&source, &[motif], &invalid, 0).is_err());
}

#[test]
fn composing_a_plan_applies_transitions_at_its_section_joins() {
    let source = crate::fixtures::repeated_frets();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: fingerprint_bar(&source, 0).unwrap(),
    };
    let mut plan = ArrangementPlan::new(
        7,
        vec![
            SectionRequest {
                label: "Verse".into(),
                bars: 1,
                candidates: vec![0],
                transpose_semitones: 0,
                energy: None,
                instrumentation: None,
                energy_texture: None,
                climax: false,
            },
            SectionRequest {
                label: "Chorus".into(),
                bars: 1,
                candidates: vec![0],
                transpose_semitones: 0,
                energy: None,
                instrumentation: None,
                energy_texture: None,
                climax: false,
            },
        ],
    );
    plan.transitions
        .push(super::super::transitions::TransitionRule::new(
            super::super::transitions::TransitionKind::PickSlide,
            0,
        ));
    let output = compose_plan(&source, &[motif], &plan).unwrap();
    assert_eq!(output.master_bars[1].section.as_deref(), Some("Chorus"));
    assert_eq!(
        output.technique_count(|technique| matches!(
            technique,
            crate::model::Technique::Slide { .. }
        )),
        1,
        "the saved plan must apply its join treatment during composition"
    );
}

#[test]
fn section_energy_controls_optional_texture_and_octave_doubling() {
    let source = crate::fixtures::repeated_frets();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: fingerprint_bar(&source, 0).unwrap(),
    };
    let texture = EnergyTexture::new(vec![0], 50, Some(0), 80);
    let mut low_plan = ArrangementPlan::new(
        3,
        vec![SectionRequest {
            label: "Low".into(),
            bars: 1,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: Some(40),
            instrumentation: None,
            energy_texture: Some(texture.clone()),
            climax: false,
        }],
    );
    let low = compose_plan(&source, std::slice::from_ref(&motif), &low_plan).unwrap();
    low_plan.sections[0].energy = Some(90);
    let high = compose_plan(&source, &[motif], &low_plan).unwrap();

    let low_notes = low.note_count();
    let high_notes = high.note_count();
    assert_eq!(
        low_notes, 0,
        "optional tracks must be silent below their threshold"
    );
    assert_eq!(
        high_notes, 16,
        "high energy must restore and double the source notes"
    );
}

#[test]
fn omitted_energy_texture_leaves_composition_unchanged() {
    let source = crate::fixtures::repeated_frets();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: fingerprint_bar(&source, 0).unwrap(),
    };
    let base = ArrangementPlan::new(
        5,
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
    let with_mapping = ArrangementPlan {
        sections: vec![SectionRequest {
            energy_texture: Some(EnergyTexture::new(vec![0], 50, Some(0), 80)),
            ..base.sections[0].clone()
        }],
        ..base.clone()
    };
    assert_eq!(
        compose_plan(&source, std::slice::from_ref(&motif), &base).unwrap(),
        compose_plan(&source, &[motif], &with_mapping).unwrap(),
        "a texture mapping without energy must preserve the source composition"
    );
}

#[test]
fn composing_short_meter_overfull_fixture_fits_destination_meter() {
    let source = crate::fixtures::short_meter_overfull();
    let motif = Motif {
        start_bar: 0,
        end_bar: 0,
        fingerprint: fingerprint_bar(&source, 0).unwrap(),
    };
    let plan = ArrangementPlan::new(
        1,
        vec![SectionRequest {
            label: "Intro".into(),
            bars: 2,
            candidates: vec![0],
            transpose_semitones: 0,
            energy: None,
            instrumentation: None,
            energy_texture: None,
            climax: false,
        }],
    );
    let output = compose_plan(&source, &[motif], &plan).expect("composition must succeed");
    assert_eq!(output.master_bars.len(), 2);
    for master in &output.master_bars {
        assert_eq!(master.time, (1, 4));
    }
    let bar = &output.bars[0];
    let voice = &bar.voices[0];
    let dur = voice
        .beats
        .iter()
        .fold(crate::inspect::Fraction::new(0, 1), |sum, b| {
            sum.plus(crate::inspect::rhythm_duration(b.rhythm))
        });
    assert_eq!(dur, crate::inspect::Fraction::new(1, 4));
    for item in crate::inspect::bar_integrity(&output) {
        assert!(
            item.duration <= item.capacity,
            "composed bar {} on track {} exceeds meter: {:?} > {:?}",
            item.bar_index + 1,
            item.track_index,
            item.duration,
            item.capacity
        );
    }
}

#[test]
fn overfull_bar_error_uses_one_based_bar_numbers_and_names_source_bar() {
    let err = ArrangementError::OverfullBar {
        track: 8,
        bar: 50,
        voice: 0,
        source_bar: 50,
    };
    let message = err.to_string();
    assert_eq!(
        message,
        "bar 50 on track 8, voice 0 exceeds its meter (source bar 50)"
    );
}
