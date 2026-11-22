use super::*;

use crate::fixtures;
use crate::format::ascii::{layout_track, LayoutOptions};

fn rendered(doc: &crate::model::Document, start: usize, end: usize, width: usize) -> String {
    let layout = layout_track(doc, 0, start, end, &LayoutOptions::default()).expect("layout");
    render_layout(&layout, width)
}

#[test]
fn one_bar_of_palm_muted_eighths_is_golden() {
    let text = rendered(&fixtures::repeated_frets(), 0, 0, 200);
    assert_eq!(
        text,
        "  1 4/4\n\
         PM *-*-*-*-*-*-*-*\n\
         e|----------------|\n\
         B|----------------|\n\
         G|----------------|\n\
         D|----------------|\n\
         A|----------------|\n\
         E|5-5-5-5-5-5-5-5-|"
    );
}

#[test]
fn systems_wrap_to_the_width_and_stay_within_it() {
    let doc = fixtures::repeated_frets();
    let layout = layout_track(&doc, 0, 0, 4, &LayoutOptions::default()).expect("layout");
    let width = layout.system_width(0..2);
    let text = render_layout(&layout, width);
    let systems: Vec<&str> = text.split("\n\n").collect();
    assert!(
        systems.len() >= 3,
        "five bars, at most two per system:\n{text}"
    );
    for line in text.lines() {
        assert!(line.chars().count() <= width, "{line:?} wider than {width}");
    }
    assert!(systems[1].starts_with("  3"), "{}", systems[1]);
}

#[test]
fn connectors_and_headings_are_drawn() {
    let mut doc = fixtures::every_technique();
    doc.master_bars[3].section = Some("Solo".to_string());
    doc.master_bars[3].repeat_start = true;
    let text = rendered(&doc, 3, 3, 200);
    assert!(text.contains("5h6h7h8"), "{text}");
    // The meter matches the previous bar, so it is not repeated.
    assert!(text.starts_with("  4 |: [Solo]\n"), "{text}");
}

#[test]
fn drum_lanes_are_labelled() {
    let text = rendered(&fixtures::drum_patterns(), 0, 0, 200);
    let labels: Vec<&str> = text
        .lines()
        .filter(|line| line.contains('|'))
        .filter_map(|line| line.split('|').next())
        .map(str::trim)
        .collect();
    assert!(
        labels.contains(&"Kick") && labels.contains(&"Hi-Hat"),
        "{text}"
    );
    assert!(text.contains("Kick      |o-------o-------|"), "{text}");
}
