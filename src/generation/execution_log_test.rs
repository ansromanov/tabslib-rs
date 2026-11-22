use super::*;

fn decision(bar: usize, track: usize, role: GeneratedRole, choice: &str) -> Decision {
    Decision {
        bar,
        track,
        role,
        choice: choice.into(),
        phrase_position: bar % 4,
        seed: 0x51f3 ^ (bar as u64),
        events: 4,
    }
}

fn log() -> ExecutionLog {
    let mut log = ExecutionLog::new(0x51f3);
    log.record(decision(0, 2, GeneratedRole::Drums, "onsets:4/16"));
    log.record(decision(1, 2, GeneratedRole::Drums, "onsets:8/16"));
    log.record(decision(1, 3, GeneratedRole::Bass, "RiffFollow"));
    log
}

#[test]
fn a_log_finds_the_decisions_behind_one_bar_of_one_track() {
    let log = log();
    let found = log.at(1, 2);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].choice, "onsets:8/16");
    assert!(log.at(9, 2).is_empty(), "an unwritten bar has no decisions");
}

#[test]
fn explain_narrows_a_finding_to_the_decision_that_produced_it() {
    let log = log();
    // A diagnostic naming a bar and a track resolves to that generator's
    // choice; this is the step between "bar 1 repeats" and "bar 1 chose
    // onsets:8/16 from seed 0x51f2".
    let drums = explain(&log, 1, Some(2));
    assert_eq!(drums.len(), 1);
    assert_eq!(drums[0].role, GeneratedRole::Drums);
    assert_eq!(drums[0].seed, 0x51f3 ^ 1);
}

#[test]
fn a_score_wide_finding_is_not_attributed_to_one_generator() {
    // A diagnostic with no track cannot be blamed on a single generator, so
    // every decision in the bar comes back.
    let log = log();
    let found = explain(&log, 1, None);
    assert_eq!(found.len(), 2);
    let roles: Vec<GeneratedRole> = found.iter().map(|decision| decision.role).collect();
    assert!(roles.contains(&GeneratedRole::Drums));
    assert!(roles.contains(&GeneratedRole::Bass));
}

#[test]
fn a_log_counts_what_each_role_decided() {
    let counts = log().by_role();
    assert_eq!(counts.get(&GeneratedRole::Drums), Some(&2));
    assert_eq!(counts.get(&GeneratedRole::Bass), Some(&1));
    assert_eq!(counts.get(&GeneratedRole::Lead), None);
}

#[test]
fn a_log_round_trips_through_json_deterministically() {
    let log = log();
    let json = log.to_json().expect("serializable");
    assert_eq!(
        ExecutionLog::from_json(&json).expect("parseable"),
        log,
        "a log that cannot be read back cannot explain a past run"
    );
    assert_eq!(log.to_json().expect("serializable"), json);
}

#[test]
fn a_log_from_another_schema_version_is_refused() {
    let json = r#"{"version": 99, "seed": 1, "decisions": []}"#;
    assert_eq!(
        ExecutionLog::from_json(json),
        Err(LogError::UnsupportedVersion(99))
    );
}

#[test]
fn malformed_json_reports_rather_than_returning_an_empty_log() {
    // An empty log reads as "nothing was decided", which is exactly wrong for
    // a run that did decide something and then failed to serialize.
    assert!(matches!(
        ExecutionLog::from_json("not json"),
        Err(LogError::Serialization(_))
    ));
}

#[test]
fn role_names_are_unique() {
    let roles = [
        GeneratedRole::Drums,
        GeneratedRole::Bass,
        GeneratedRole::Harmony,
        GeneratedRole::Lead,
        GeneratedRole::Riff,
        GeneratedRole::Arrangement,
    ];
    let names: std::collections::BTreeSet<&str> = roles.iter().map(|role| role.name()).collect();
    assert_eq!(names.len(), roles.len());
}
