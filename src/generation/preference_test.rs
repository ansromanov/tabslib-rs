use super::*;

use crate::fixtures;
use crate::generation::assessment::{assess, CalibrationSample};

fn candidate(label: &str, seed: u64) -> CandidateRef {
    CandidateRef {
        label: label.into(),
        seed,
        context: format!("strategy={label}"),
    }
}

fn sample(label: &str, metric: Metric, numerator: u64, denominator: u64) -> CalibrationSample {
    CalibrationSample {
        label: label.into(),
        metrics: BTreeMap::from([(metric, Measurement::new(numerator, denominator))]),
    }
}

#[test]
fn side_assignment_is_deterministic_and_not_an_alternation() {
    assert_eq!(assign_side(0x51f3), assign_side(0x51f3));

    // A caller stepping the seed by one per comparison must not get a strict
    // alternation: a reader who noticed the pattern would stop being blind.
    let sides: Vec<Side> = (0..16).map(assign_side).collect();
    let alternating: Vec<Side> = (0..16)
        .map(|index| {
            if index % 2 == 0 {
                Side::First
            } else {
                Side::Second
            }
        })
        .collect();
    assert_ne!(sides, alternating, "sides alternate exactly with the seed");
    assert!(
        sides.contains(&Side::First) && sides.contains(&Side::Second),
        "both sides are used"
    );
}

#[test]
fn a_blind_record_puts_the_candidates_where_the_seed_says() {
    let left = candidate("riff-follow", 1);
    let right = candidate("walking", 2);
    let record = blind_record(9, 0x51f3, left.clone(), right.clone(), Choice::First);

    let expected_first = match assign_side(0x51f3) {
        Side::First => &left,
        Side::Second => &right,
    };
    assert_eq!(&record.first, expected_first);
    assert_eq!(record.version, RECORD_VERSION);
    assert_eq!(record.id, format!("{:016x}-{:016x}", 9u64, 0x51f3u64));
}

#[test]
fn the_preferred_candidate_follows_the_side_the_choice_names() {
    // The choice names a side, not a candidate. A record that resolved it to
    // the left-hand argument instead would silently invert every result whose
    // sides were swapped, and the tally would look plausible throughout.
    let left = candidate("a", 1);
    let right = candidate("b", 2);
    for seed in [1u64, 2, 3, 4, 5, 6] {
        let record = blind_record(0, seed, left.clone(), right.clone(), Choice::First);
        assert_eq!(
            record.preferred().map(|candidate| candidate.label.as_str()),
            Some(record.first.label.as_str())
        );
    }
}

#[test]
fn an_undecided_choice_names_no_preferred_candidate() {
    for choice in [Choice::NoPreference, Choice::BothBad] {
        let record = blind_record(0, 1, candidate("a", 1), candidate("b", 2), choice);
        assert!(record.preferred().is_none());
        assert!(!choice.is_decisive());
    }
}

#[test]
fn a_log_round_trips_and_keeps_its_order() {
    let mut log = String::new();
    for (index, choice) in [Choice::First, Choice::Second, Choice::BothBad]
        .into_iter()
        .enumerate()
    {
        let record = blind_record(
            index as u64,
            index as u64 + 1,
            candidate("a", 1),
            candidate("b", 2),
            choice,
        );
        log = append_record(&log, &record).expect("serializable");
    }
    let records = read_records(&log).expect("parseable");
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].choice, Choice::First);
    assert_eq!(records[2].choice, Choice::BothBad);
}

#[test]
fn appending_to_a_log_without_a_trailing_newline_does_not_join_two_records() {
    let record = blind_record(0, 1, candidate("a", 1), candidate("b", 2), Choice::First);
    let first = append_record("", &record).expect("serializable");
    let truncated = first.trim_end().to_string();
    let second = append_record(&truncated, &record).expect("serializable");
    assert_eq!(
        read_records(&second).expect("parseable").len(),
        2,
        "two records must not be written onto one line"
    );
}

#[test]
fn existing_dbt_preference_schema_is_imported_with_provenance() {
    let log = r#"{"schemaVersion":"1.1.0","id":"legacy-1","timestamp":"2026-08-24T06:10:05Z","dimension":"seed","source":{"path":"song.gp","contentHash":"abc"},"sessionDir":"sessions/pair","choice":"b","reason":"B has the stronger phrase.","candidates":{"a":{"side":"a","config":{"seed":41}},"b":{"side":"b","config":{"seed":42}}}}"#;
    let records = read_records(log).expect("legacy record imports");
    assert_eq!(records[0].choice, Choice::Second);
    let provenance = records[0].provenance.as_ref().expect("provenance retained");
    assert_eq!(
        provenance.reason.as_deref(),
        Some("B has the stronger phrase.")
    );
    assert_eq!(provenance.source_content_hash.as_deref(), Some("abc"));
    assert_eq!(records[0].second.seed, 42);
}

#[test]
fn a_record_from_another_schema_version_is_refused() {
    let json = concat!(
        r#"{"version":99,"id":"x","seed":1,"#,
        r#""first":{"label":"a","seed":1,"context":""},"#,
        r#""second":{"label":"b","seed":2,"context":""},"#,
        r#""choice":"First"}"#
    );
    assert_eq!(
        read_records(json),
        Err(PreferenceError::UnsupportedVersion(99))
    );
}

#[test]
fn a_tally_counts_only_decisive_comparisons() {
    let records = vec![
        blind_record(0, 1, candidate("a", 1), candidate("b", 2), Choice::First),
        blind_record(1, 2, candidate("a", 1), candidate("b", 2), Choice::Second),
        blind_record(2, 3, candidate("a", 1), candidate("b", 2), Choice::BothBad),
        blind_record(
            3,
            4,
            candidate("a", 1),
            candidate("b", 2),
            Choice::NoPreference,
        ),
    ];
    let tally = tally(&records);
    let total: usize = tally.values().map(|(won, lost)| won + lost).sum();
    assert_eq!(
        total, 4,
        "two decisive comparisons produce two wins and two losses"
    );
}

#[test]
fn a_gate_fails_a_metric_outside_tolerance_and_passes_one_within() {
    let document = fixtures::repeated_frets();
    let report = assess(&document);
    let observed = report.metrics[&Metric::Silence];

    let policy = GatePolicy {
        tolerance: BTreeMap::from([(Metric::Silence, Measurement::new(1, 10))]),
        quarantined: BTreeSet::new(),
    };

    let matching = gate(
        &report,
        &[sample(
            "reference",
            Metric::Silence,
            observed.numerator,
            observed.denominator.max(1),
        )],
        &policy,
    );
    let silence = matching
        .verdicts
        .iter()
        .find(|entry| entry.metric == Metric::Silence)
        .expect("the metric is gated");
    assert_eq!(silence.verdict, Verdict::Pass);

    let distant = gate(
        &report,
        &[sample("reference", Metric::Silence, 9, 10)],
        &policy,
    );
    let silence = distant
        .verdicts
        .iter()
        .find(|entry| entry.metric == Metric::Silence)
        .expect("the metric is gated");
    assert_eq!(silence.verdict, Verdict::Fail);
    assert!(!distant.passed(), "a failure fails the gate");
    assert_eq!(distant.failures(), vec![Metric::Silence]);
}

#[test]
fn a_quarantined_metric_reports_its_failure_without_failing_the_gate() {
    let report = assess(&fixtures::repeated_frets());
    let policy = GatePolicy {
        tolerance: BTreeMap::from([(Metric::Silence, Measurement::new(1, 100))]),
        quarantined: BTreeSet::from([Metric::Silence]),
    };
    let result = gate(
        &report,
        &[sample("reference", Metric::Silence, 9, 10)],
        &policy,
    );
    let silence = result
        .verdicts
        .iter()
        .find(|entry| entry.metric == Metric::Silence)
        .expect("the metric is gated");
    assert_eq!(silence.verdict, Verdict::Quarantined);
    assert!(
        result.passed(),
        "a quarantined failure is reported, not fatal"
    );
    assert!(result.failures().is_empty());
}

#[test]
fn a_metric_with_no_reference_is_unmeasured_rather_than_passing() {
    // A gate that counts a missing comparison as a success reports green for a
    // run that compared nothing at all. This is the assertion that catches it.
    let report = assess(&fixtures::repeated_frets());
    let result = gate(&report, &[], &GatePolicy::default());
    assert!(
        result
            .verdicts
            .iter()
            .all(|entry| entry.verdict == Verdict::Unmeasured),
        "every metric is unmeasured when no sample is supplied"
    );
    assert!(
        result.passed(),
        "nothing failed, because nothing was tested"
    );
    assert!(
        !result.verdicts.is_empty(),
        "the report still names every metric it could not test"
    );
}

#[test]
fn a_metric_with_a_reference_but_no_tolerance_is_unmeasured() {
    let report = assess(&fixtures::repeated_frets());
    let result = gate(
        &report,
        &[sample("reference", Metric::Silence, 9, 10)],
        &GatePolicy::default(),
    );
    let silence = result
        .verdicts
        .iter()
        .find(|entry| entry.metric == Metric::Silence)
        .expect("the metric is listed");
    assert_eq!(
        silence.verdict,
        Verdict::Unmeasured,
        "no tolerance means no threshold to judge against"
    );
}

#[test]
fn discrimination_reports_a_metric_that_cannot_tell_samples_apart() {
    let same = [
        sample("a", Metric::Silence, 1, 2),
        sample("b", Metric::Silence, 1, 2),
        sample("c", Metric::Silence, 2, 4),
    ];
    assert_eq!(
        discrimination(&same, Metric::Silence),
        1,
        "an equal-reading metric is uninformative however many samples it has"
    );

    let varied = [
        sample("a", Metric::Silence, 1, 2),
        sample("b", Metric::Silence, 1, 4),
    ];
    assert_eq!(discrimination(&varied, Metric::Silence), 2);
    assert_eq!(discrimination(&varied, Metric::Density), 0);
}

#[test]
fn a_rendered_report_names_every_metric_and_marks_the_failures() {
    let report = assess(&fixtures::repeated_frets());
    let result = gate(
        &report,
        &[sample("reference", Metric::Silence, 9, 10)],
        &GatePolicy {
            tolerance: BTreeMap::from([(Metric::Silence, Measurement::new(1, 100))]),
            quarantined: BTreeSet::new(),
        },
    );
    let rendered = result.render();
    assert!(rendered.contains("FAIL"));
    assert!(rendered.contains("silence"));
    assert_eq!(
        rendered.lines().count(),
        result.verdicts.len(),
        "one line per metric"
    );
}

#[test]
fn no_threshold_or_reference_value_is_compiled_into_the_crate() {
    // An empty policy is the only default. A tolerance that makes a metric
    // meaningful is measured over a collection of music, and a guessed one
    // would turn the gate into a coin flip that looks authoritative.
    let policy = GatePolicy::default();
    assert!(policy.tolerance.is_empty());
    assert!(policy.quarantined.is_empty());
}
