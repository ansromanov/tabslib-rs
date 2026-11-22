use super::*;
use crate::fixtures;

#[test]
fn degradation_changes_density_silence_and_playability_at_its_region() {
    let mut original = fixtures::repeated_frets();
    let mut degraded = original.clone();
    let bar_id = degraded.master_bars[0].bar_ids[0] as u32;
    let bar = degraded
        .bars
        .iter_mut()
        .find(|bar| bar.id == bar_id)
        .unwrap();
    let note = &mut bar.voices[0].beats[0].notes[0];
    note.midi = Some(note.midi.unwrap() + 1);
    bar.voices[0].beats[1].notes.clear();

    let before = assess(&original);
    let after = assess(&degraded);
    assert_eq!(before.metrics[&Metric::Density], Measurement::new(1, 1));
    assert_eq!(after.metrics[&Metric::Density], Measurement::new(39, 40));
    assert_eq!(after.metrics[&Metric::Silence], Measurement::new(1, 40));
    assert_eq!(before.metrics[&Metric::Playability], Measurement::new(1, 1));
    assert_eq!(
        after.metrics[&Metric::Playability],
        Measurement::new(38, 39)
    );
    assert!(after.diagnostics.iter().any(|item| {
        item.code == "unplayable_position"
            && item.region
                == Region {
                    bar: 0,
                    track: Some(0),
                    voice: Some(0),
                    beat: Some(0),
                }
    }));
    assert!(after.diagnostics.iter().any(|item| {
        item.code == "silence_beat" && item.region.beat == Some(1) && item.region.bar == 0
    }));
    original.master_bars[0].double_bar = true;
    assert_eq!(assess(&original), before);
}

#[test]
fn exact_patterns_and_voice_leaps_are_reported_with_locations() {
    let mut doc = fixtures::repeated_frets();
    let first_pattern = doc.bars[0].voices[0].beats[0].notes[0].midi;
    for beat in &mut doc.bars[1].voices[0].beats {
        beat.notes[0].midi = first_pattern;
        beat.notes[0].fret = Some(5);
    }
    let report = assess(&doc);
    assert!(report.metrics[&Metric::Repetition].numerator > 0);
    assert!(report.metrics[&Metric::VoiceLeading].numerator > 0);
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.code == "repeated_bar_pattern"));
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.code == "voice_leading_leap"));
}

#[test]
fn json_is_golden_stable_and_calibration_uses_only_external_inputs() {
    let mut fixture = fixtures::repeated_frets();
    fixture.master_bars.truncate(1);
    fixture.bars.truncate(1);
    fixture.bars[0].voices[0].beats.truncate(1);
    let report = assess(&fixture);
    assert_eq!(
        report.to_json(),
        "{\"metrics\":[{\"name\":\"playability\",\"numerator\":1,\"denominator\":1},{\"name\":\"texture\",\"numerator\":1,\"denominator\":1},{\"name\":\"repetition\",\"numerator\":0,\"denominator\":1},{\"name\":\"silence\",\"numerator\":0,\"denominator\":1},{\"name\":\"density\",\"numerator\":1,\"denominator\":1},{\"name\":\"shape\",\"numerator\":0,\"denominator\":1},{\"name\":\"voice_leading\",\"numerator\":0,\"denominator\":1},{\"name\":\"palm_mute_rate\",\"numerator\":1,\"denominator\":1},{\"name\":\"palm_mute_spread\",\"numerator\":1,\"denominator\":1},{\"name\":\"cymbal_coverage\",\"numerator\":0,\"denominator\":1},{\"name\":\"tom_share\",\"numerator\":0,\"denominator\":1},{\"name\":\"unique_bar_patterns\",\"numerator\":1,\"denominator\":1},{\"name\":\"longest_loop\",\"numerator\":0,\"denominator\":1},{\"name\":\"bar_count\",\"numerator\":1,\"denominator\":1},{\"name\":\"hypermeter_alignment\",\"numerator\":0,\"denominator\":1},{\"name\":\"voicing_single_note\",\"numerator\":1,\"denominator\":1},{\"name\":\"voicing_power_shape\",\"numerator\":0,\"denominator\":1},{\"name\":\"voicing_extended_shape\",\"numerator\":0,\"denominator\":1},{\"name\":\"low_string_share\",\"numerator\":1,\"denominator\":1}],\"diagnostics\":[{\"code\":\"shape_bar_density\",\"severity\":\"info\",\"class\":\"measurement\",\"bar\":0,\"track\":null,\"voice\":null,\"beat\":null,\"value\":1},{\"code\":\"palm_mute_bar\",\"severity\":\"info\",\"class\":\"measurement\",\"bar\":0,\"track\":0,\"voice\":null,\"beat\":null,\"value\":1},{\"code\":\"density_beat\",\"severity\":\"info\",\"class\":\"measurement\",\"bar\":0,\"track\":0,\"voice\":0,\"beat\":0,\"value\":1},{\"code\":\"playability_position\",\"severity\":\"info\",\"class\":\"measurement\",\"bar\":0,\"track\":0,\"voice\":0,\"beat\":0,\"value\":1},{\"code\":\"silence_beat\",\"severity\":\"info\",\"class\":\"measurement\",\"bar\":0,\"track\":0,\"voice\":0,\"beat\":0,\"value\":0},{\"code\":\"texture_onset\",\"severity\":\"info\",\"class\":\"measurement\",\"bar\":0,\"track\":0,\"voice\":0,\"beat\":0,\"value\":1}]}"
    );
    assert_eq!(report.to_json(), assess(&fixture).to_json());

    let external = CalibrationSample {
        label: "caller \"reference\"".into(),
        metrics: BTreeMap::from([(Metric::Density, Measurement::new(3, 2))]),
    };
    let calibrated = calibrate(&report, &[external]);
    let density = calibrated
        .values
        .iter()
        .find(|item| item.metric == Metric::Density)
        .unwrap();
    assert_eq!(
        density.references,
        vec![("caller \"reference\"".into(), Measurement::new(3, 2))]
    );
    assert!(calibrated.to_json().contains("caller \\\"reference\\\""));
    assert!(calibrated
        .to_json()
        .contains("\"numerator\":3,\"denominator\":2"));
}

#[test]
fn one_systemic_cause_collapses_to_one_group() {
    // Every position in this document disagrees with its tuning, so the flat
    // list carries one finding per note and the grouped view carries one row.
    let mut doc = fixtures::repeated_frets();
    for bar in &mut doc.bars {
        for voice in &mut bar.voices {
            for beat in &mut voice.beats {
                for note in &mut beat.notes {
                    note.midi = Some(0);
                }
            }
        }
    }
    let report = assess(&doc);
    let flat = report
        .diagnostics
        .iter()
        .filter(|item| item.code == "unplayable_position")
        .count();
    assert!(flat > 1, "expected the cause to repeat, got {flat}");

    let summary = report.summary();
    let playability: Vec<&DiagnosticGroup> = summary
        .iter()
        .filter(|group| group.class == DiagnosticClass::Playability)
        .collect();
    assert_eq!(playability.len(), 1, "one cause is one row");
    assert_eq!(playability[0].count, flat);
    assert_eq!(playability[0].severity, Severity::Error);
    assert_eq!(playability[0].codes, vec!["unplayable_position"]);
    assert_eq!(
        playability[0].bars.len(),
        doc.master_bars.len(),
        "the group names the bars it touches"
    );
}

#[test]
fn groups_are_ordered_worst_first() {
    let mut doc = fixtures::repeated_frets();
    doc.bars[0].voices[0].beats[0].notes[0].midi = Some(0);
    let summary = assess(&doc).summary();
    let severities: Vec<Severity> = summary.iter().map(|group| group.severity).collect();
    let mut sorted = severities.clone();
    sorted.sort();
    assert_eq!(severities, sorted, "got {summary:?}");
    assert_eq!(
        summary.first().map(|group| group.severity),
        Some(Severity::Error)
    );
}

#[test]
fn severity_and_class_follow_the_code() {
    let cases = [
        (
            "unplayable_position",
            Severity::Error,
            DiagnosticClass::Playability,
        ),
        (
            "repeated_bar_pattern",
            Severity::Warning,
            DiagnosticClass::Repetition,
        ),
        (
            "voice_leading_leap",
            Severity::Warning,
            DiagnosticClass::VoiceLeading,
        ),
        ("density_beat", Severity::Info, DiagnosticClass::Measurement),
        ("silence_beat", Severity::Info, DiagnosticClass::Measurement),
    ];
    for (code, severity, class) in cases {
        let diagnostic = Diagnostic {
            code,
            region: Region {
                bar: 0,
                track: None,
                voice: None,
                beat: None,
            },
            value: 0,
        };
        assert_eq!(diagnostic.severity(), severity, "{code}");
        assert_eq!(diagnostic.class(), class, "{code}");
    }
}

#[test]
fn an_empty_report_groups_to_nothing() {
    assert!(group_diagnostics(&[]).is_empty());
}
