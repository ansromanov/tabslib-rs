use super::super::preference::CandidateRef;
use super::*;

fn report(density: u64) -> AssessmentReport {
    AssessmentReport {
        metrics: BTreeMap::from([
            (Metric::Density, Measurement::new(density, 1)),
            (Metric::Playability, Measurement::new(1, 1)),
        ]),
        diagnostics: Vec::new(),
    }
}

fn pair(id: &str, first: u64, second: u64, choice: Choice) -> PreferenceTrainingPair {
    PreferenceTrainingPair {
        record: PreferenceRecord {
            version: RECORD_VERSION,
            id: id.into(),
            seed: 7,
            first: CandidateRef {
                label: format!("{id}-first"),
                seed: 11,
                context: String::new(),
            },
            second: CandidateRef {
                label: format!("{id}-second"),
                seed: 12,
                context: String::new(),
            },
            choice,
            third: None,
            provenance: None,
        },
        first: report(first),
        second: report(second),
    }
}

#[test]
fn ranker_trains_skips_indecisive_records_and_measures_held_out_agreement() {
    let training = vec![
        pair("one", 9, 2, Choice::First),
        pair("two", 1, 8, Choice::Second),
        pair("three", 7, 3, Choice::First),
        pair("skip", 100, 0, Choice::NoPreference),
    ];
    let model = train_preference_ranker(&training, 3).expect("three decisive records");
    assert_eq!(
        model.training_records, 3,
        "only decisive choices train the model"
    );

    let held_out = pair("held-out", 10, 2, Choice::First);
    let agreement = preference_ranker_agreement(&model, &[held_out]).expect("decisive holdout");
    assert_eq!(
        agreement.correct, 1,
        "the preferred higher-density candidate is predicted"
    );
    assert_eq!(agreement.decisive, 1);
    assert_eq!(agreement.rate(), 1.0);

    let candidates = vec![
        PreferenceCandidate::from_report("sparse", &report(2)),
        PreferenceCandidate::from_report("dense", &report(10)),
    ];
    let ranked = rank_preference_candidates(&model, &candidates);
    assert_eq!(ranked[0].candidate.label, "dense");
    assert!(ranked[0].score > ranked[1].score);

    let json = model.to_json().expect("model serializes");
    assert_eq!(
        PreferenceRanker::from_json(&json).expect("model parses"),
        model
    );
}

#[test]
fn ranker_requires_the_callers_minimum_number_of_decisive_records() {
    let examples = vec![
        pair("one", 9, 2, Choice::First),
        pair("skip", 100, 0, Choice::BothBad),
    ];
    assert_eq!(
        train_preference_ranker(&examples, 2).expect_err("too little evidence"),
        PreferenceRankerError::TooFewDecisive {
            minimum: 2,
            actual: 1,
        },
        "a non-decisive record must not satisfy the evidence minimum"
    );
}

#[test]
fn ranker_rejects_unknown_versions_and_empty_held_out_sets() {
    let mut model = train_preference_ranker(&[pair("one", 9, 2, Choice::First)], 1)
        .expect("one decisive record");
    model.version = PREFERENCE_RANKER_VERSION + 1;
    let json = model.to_json().expect("model serializes");
    assert_eq!(
        PreferenceRanker::from_json(&json).expect_err("unknown schema"),
        PreferenceRankerError::UnsupportedVersion(PREFERENCE_RANKER_VERSION + 1)
    );

    let current = train_preference_ranker(&[pair("one", 9, 2, Choice::First)], 1)
        .expect("one decisive record");
    assert_eq!(
        preference_ranker_agreement(&current, &[pair("tie", 4, 4, Choice::NoPreference)])
            .expect_err("no decisive holdout"),
        PreferenceRankerError::EmptyEvaluation
    );
}
