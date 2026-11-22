#![cfg(feature = "drums")]

use tabslib::generation::drums::{DrumPattern, PatternStep};
use tabslib::generation::pattern_catalog::{
    CatalogPattern, PatternCatalog, PatternConstraints, PatternError, PatternGrid,
    RhythmSectionPatternCatalog, RhythmSectionStep,
};
use tabslib::model::{Document, MasterBar};

fn grid(meter: (u32, u32), subdivisions: u32) -> PatternGrid {
    PatternGrid::new(meter, subdivisions).expect("valid grid")
}

fn pattern(
    id: &str,
    meter: (u32, u32),
    subdivisions: u32,
    active: &[usize],
) -> CatalogPattern<PatternStep> {
    let grid = grid(meter, subdivisions);
    let width = grid.steps_per_bar().expect("width");
    let mut bars = vec![vec![PatternStep::default(); width]];
    for &step in active {
        bars[0][step] = PatternStep {
            onset: true,
            accent: true,
            dead: false,
            palm_mute: true,
        };
    }
    CatalogPattern::new(id, grid, bars).expect("valid pattern")
}

#[test]
fn empty_catalog_and_incompatible_meter_have_explicit_diagnostics() {
    let catalog =
        PatternCatalog::<PatternStep>::new(Vec::new()).expect("empty catalog is representable");
    let request = PatternConstraints::new(grid((4, 4), 4), 1).expect("request");
    assert_eq!(
        catalog.select_seeded(&request, 9),
        Err(PatternError::EmptyCatalog)
    );

    let catalog =
        PatternCatalog::new(vec![pattern("long-meter", (4, 4), 4, &[12])]).expect("catalog");
    let request = PatternConstraints::new(grid((3, 4), 4), 1).expect("request");
    let error = catalog
        .select_seeded(&request, 9)
        .expect_err("event would be truncated");
    let PatternError::NoCompatiblePattern { rejections } = error else {
        panic!("unexpected error")
    };
    assert_eq!(rejections.len(), 1);
    assert!(rejections[0].reason.contains("falls outside target width"));
}

#[test]
fn selection_is_seeded_and_preserves_articulation_and_boundary_events() {
    let patterns = vec![
        pattern("first", (4, 4), 4, &[0, 15]),
        pattern("second", (4, 4), 4, &[2]),
        pattern("third", (4, 4), 4, &[3, 7]),
    ];
    let catalog = PatternCatalog::new(patterns).expect("catalog");
    let request = PatternConstraints::new(grid((4, 4), 4), 1)
        .expect("request")
        .with_active_steps(1, 2)
        .expect("density");
    let a = catalog.select_seeded(&request, 42).expect("selection");
    let b = catalog.select_seeded(&request, 42).expect("same selection");
    assert_eq!(a, b);
    assert_eq!(a.pattern.bars[0].len(), 16);
    let edge = pattern("edge", (4, 4), 4, &[15]);
    let selected_edge = PatternCatalog::new(vec![edge])
        .expect("catalog")
        .select_seeded(&request, 42)
        .expect("edge event remains compatible");
    assert_eq!(
        selected_edge.pattern.bars[0][15],
        PatternStep {
            onset: true,
            accent: true,
            dead: false,
            palm_mute: true
        }
    );
    let drum_pattern =
        DrumPattern::from_catalog_pattern(&selected_edge.pattern).expect("drum conversion");
    assert_eq!(drum_pattern.bars[0][15], selected_edge.pattern.bars[0][15]);
}

#[test]
fn adaptation_rejects_unrepresentable_onsets_and_keeps_metadata_when_exact() {
    let unrepresentable = pattern("off-grid", (4, 4), 4, &[1]);
    let error = unrepresentable
        .adapt(grid((4, 4), 2))
        .expect_err("step 1 cannot map");
    assert!(matches!(
        error,
        PatternError::UnrepresentableEvent { step: 1, .. }
    ));

    let exact = pattern("exact", (4, 4), 4, &[2, 14]);
    let adapted = exact
        .adapt(grid((4, 4), 2))
        .expect("both events map exactly");
    assert_eq!(adapted.bars[0][1], exact.bars[0][2]);
    assert_eq!(adapted.bars[0][7], exact.bars[0][14]);
}

#[test]
fn document_bar_and_phrase_constraints_validate_meters_and_ranges() {
    let mut document = Document::default();
    document.master_bars = vec![
        MasterBar {
            index: 0,
            time: (4, 4),
            section: None,
            double_bar: false,
            bar_ids: vec![],
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        },
        MasterBar {
            index: 1,
            time: (4, 4),
            section: None,
            double_bar: false,
            bar_ids: vec![],
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        },
        MasterBar {
            index: 2,
            time: (3, 4),
            section: None,
            double_bar: false,
            bar_ids: vec![],
            repeat_start: false,
            repeat_end: None,
            alternate_ending: 0,
            direction: None,
        },
    ];
    assert_eq!(
        PatternConstraints::for_bar(&document, 0, 4)
            .expect("bar")
            .bars,
        1
    );
    assert_eq!(
        PatternConstraints::for_phrase(&document, 0, 2, 4)
            .expect("phrase")
            .bars,
        2
    );
    assert_eq!(
        PatternConstraints::for_phrase(&document, 1, 2, 4),
        Err(PatternError::MixedMeterPhrase)
    );
    assert_eq!(
        PatternConstraints::for_bar(&document, 9, 4),
        Err(PatternError::DocumentRangeOutsideScore)
    );
}

#[test]
fn generic_rhythm_section_catalog_accepts_only_caller_material() {
    let grid = grid((6, 8), 2);
    let mut steps = vec![RhythmSectionStep::default(); grid.steps_per_bar().expect("width")];
    steps[0] = RhythmSectionStep {
        bass_onset: true,
        harmony_onset: false,
        accent: true,
        fill: false,
    };
    let pattern = CatalogPattern::new("caller-pattern", grid, vec![steps]).expect("pattern");
    let catalog = RhythmSectionPatternCatalog::new(vec![pattern]).expect("catalog");
    let request = PatternConstraints::new(grid, 1).expect("request");
    assert_eq!(
        catalog
            .select_seeded(&request, 1)
            .expect("select")
            .pattern
            .id,
        "caller-pattern"
    );
}
