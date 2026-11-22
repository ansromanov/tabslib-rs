use super::*;
use crate::generation::riff_spec::{write_riff, RiffSpec};

#[test]
fn composes_sources_in_energy_order() {
    let mut high = RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    high.name = "high".into();
    high.key.tonic = 7;
    let mut low = high.clone();
    low.name = "low".into();
    low.key.tonic = 0;
    let sources = vec![
        RiffSource {
            label: "high".into(),
            document: write_riff(&high).unwrap(),
            energy: 80,
        },
        RiffSource {
            label: "low".into(),
            document: write_riff(&low).unwrap(),
            energy: 10,
        },
    ];
    let output = compose_from_riffs(
        &sources,
        RiffCompositionConfig {
            order_by_energy: true,
            ..RiffCompositionConfig::default()
        },
    )
    .unwrap();
    assert_eq!(output.master_bars.len(), 8);
    assert_eq!(output.key.map(|key| key.tonic), Some(7));
}

#[test]
fn central_key_preserves_mode_when_relative_keys_tie() {
    let mut major = RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    major.key = KeySignature {
        tonic: 7,
        minor: false,
    };
    let mut minor = major.clone();
    minor.key = KeySignature {
        tonic: 11,
        minor: true,
    };
    let sources = vec![
        RiffSource {
            label: "major".into(),
            document: write_riff(&major).unwrap(),
            energy: 0,
        },
        RiffSource {
            label: "minor".into(),
            document: write_riff(&minor).unwrap(),
            energy: 0,
        },
    ];

    assert_eq!(central_key(&sources), minor.key);
}

#[test]
fn central_key_uses_circular_pitch_class_distance() {
    let mut a = RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    a.key = KeySignature {
        tonic: 9,
        minor: false,
    };
    let mut b = a.clone();
    b.key = KeySignature {
        tonic: 11,
        minor: false,
    };
    let sources = vec![
        RiffSource {
            label: "a".into(),
            document: write_riff(&a).unwrap(),
            energy: 0,
        },
        RiffSource {
            label: "b".into(),
            document: write_riff(&b).unwrap(),
            energy: 0,
        },
    ];

    let target = central_key(&sources);
    assert!(matches!(target.tonic, 9 | 11));
    for source in [a.key, b.key] {
        assert!(key_distance(target, source) <= 5);
    }
}

fn key_distance(left: KeySignature, right: KeySignature) -> u8 {
    let difference = left.tonic.abs_diff(right.tonic);
    difference.min(12 - difference)
}

#[test]
fn slot_composition_labels_slots_and_silences_unmatched_tracks() {
    let mut first = RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    first.name = "guitar".into();
    let mut second = RiffSpec::minimal(vec![28, 33, 38, 43]);
    second.name = "bass".into();
    let sources = vec![
        RiffSource {
            label: "guitar".into(),
            document: write_riff(&first).unwrap(),
            energy: 20,
        },
        RiffSource {
            label: "bass".into(),
            document: write_riff(&second).unwrap(),
            energy: 80,
        },
    ];
    let plan = SlotPlan {
        slots: vec![
            RiffSlot {
                source: 0,
                label: "Verse".into(),
                bars: 2,
            },
            RiffSlot {
                source: 1,
                label: "Chorus".into(),
                bars: 2,
            },
        ],
        max_bars: Some(3),
    };
    let result = compose_from_slots(&sources, &plan, RiffCompositionConfig::default()).unwrap();
    assert_eq!(result.document.master_bars.len(), 2);
    assert_eq!(
        result.document.master_bars[0].section.as_deref(),
        Some("Verse")
    );
    assert_eq!(result.dropped_slots, vec!["Chorus"]);
    assert!(result.document.master_bars[0]
        .bar_ids
        .iter()
        .any(|id| *id >= 0));
}

#[test]
fn central_key_uses_circular_pitch_distance_and_mode() {
    let mut g_major = RiffSpec::minimal(vec![40, 45, 50, 55, 59, 64]);
    g_major.key = KeySignature {
        tonic: 7,
        minor: false,
    };
    let mut b_minor = g_major.clone();
    b_minor.key = KeySignature {
        tonic: 11,
        minor: true,
    };
    let sources = vec![
        RiffSource {
            label: "g".into(),
            document: write_riff(&g_major).unwrap(),
            energy: 0,
        },
        RiffSource {
            label: "b".into(),
            document: write_riff(&b_minor).unwrap(),
            energy: 0,
        },
    ];
    assert_eq!(central_key(&sources), b_minor.key);
}
