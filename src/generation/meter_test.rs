use super::*;

#[test]
fn common_meters_have_complete_default_groupings() {
    for (meter, groups) in [
        ((2, 4), vec![1, 1]),
        ((3, 4), vec![1, 1, 1]),
        ((4, 4), vec![1, 1, 1, 1]),
        ((6, 8), vec![3, 3]),
        ((9, 8), vec![3, 3, 3]),
        ((12, 8), vec![3, 3, 3, 3]),
        ((5, 8), vec![2, 3]),
        ((7, 8), vec![2, 2, 3]),
        ((5, 4), vec![3, 2]),
        ((7, 4), vec![4, 3]),
    ] {
        assert_eq!(
            BeatGrouping::for_meter(meter)
                .expect("default grouping")
                .groups(),
            groups,
            "grouping must cover meter {meter:?}"
        );
    }
}

#[test]
fn additive_seven_eight_grouping_changes_backbeat_steps() {
    let first = BeatGrouping::for_meter((7, 8)).expect("default grouping");
    let second = BeatGrouping::new((7, 8), vec![3, 2, 2]).expect("caller grouping");

    assert_eq!(first.pulse_steps(4).expect("sixteenth grid"), vec![0, 4, 8]);
    assert_eq!(
        second.pulse_steps(4).expect("sixteenth grid"),
        vec![0, 6, 10]
    );
    assert_eq!(first.backbeat_steps(4).expect("backbeats"), vec![4]);
    assert_eq!(second.backbeat_steps(4).expect("backbeats"), vec![6]);
}

#[test]
fn group_positions_are_expressed_on_the_matching_pattern_grid() {
    use super::super::pattern_catalog::PatternGrid;

    let grouping = BeatGrouping::for_meter((7, 8)).expect("default grouping");
    let grid = PatternGrid::new((7, 8), 4).expect("sixteenth grid");
    assert_eq!(
        grouping.pulse_positions(grid).expect("grid pulses"),
        vec![0, 4, 8]
    );
    assert_eq!(
        grouping.backbeat_positions(grid).expect("grid backbeats"),
        vec![4]
    );
    assert!(matches!(
        grouping.pulse_positions(PatternGrid::new((4, 4), 4).unwrap()),
        Err(BeatGroupingError::GridMeterMismatch { .. })
    ));
}

#[test]
fn invalid_groupings_and_unrepresentable_units_are_rejected() {
    assert!(matches!(
        BeatGrouping::new((7, 8), vec![2, 2, 2]),
        Err(BeatGroupingError::InvalidGroups { .. })
    ));
    assert!(matches!(
        BeatGrouping::for_meter((0, 8)),
        Err(BeatGroupingError::InvalidMeter { .. })
    ));
    let grouping = BeatGrouping::for_meter((7, 8)).expect("default grouping");
    assert!(matches!(
        grouping.pulse_steps(1),
        Err(BeatGroupingError::UnrepresentableUnit { .. })
    ));
}

#[test]
fn uncommon_meters_derive_unit_pulse_groupings() {
    assert_eq!(
        BeatGrouping::for_meter((1, 4))
            .expect("1/4 grouping")
            .groups(),
        &[1]
    );
    assert_eq!(
        BeatGrouping::for_meter((11, 8))
            .expect("11/8 grouping")
            .groups(),
        &[1; 11]
    );
}
