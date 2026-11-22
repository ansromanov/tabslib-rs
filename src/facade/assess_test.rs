use super::*;
use crate::fixtures;

#[test]
fn detect_marked_sections_from_authored_markers() {
    let doc = fixtures::meters_and_sections();
    let sections = detect_marked_sections(&doc, 0);

    assert_eq!(sections.len(), 4);
    assert_eq!(sections[0].name, "Intro");
    assert_eq!(sections[0].start_bar, 0);
    assert_eq!(sections[1].name, "Verse");
    assert_eq!(sections[1].start_bar, 2);
    assert_eq!(sections[2].name, "Chorus");
    assert_eq!(sections[2].start_bar, 4);
    assert_eq!(sections[3].name, "Section 4");
    assert_eq!(sections[3].start_bar, 5);
}

#[test]
fn assess_drums_fast_validation_errors() {
    // Document with guitar only -> NoPercussionTrack
    let guitar_doc = fixtures::scale_run();
    let err = assess_drums_fast(&guitar_doc, AssessDrumsOptions::default())
        .expect_err("should fail without percussion");
    assert_eq!(err, DrumAssessError::NoPercussionTrack);

    // Document with drums only -> NoSourceTrack
    let drum_doc = fixtures::drum_patterns();
    let err = assess_drums_fast(&drum_doc, AssessDrumsOptions::default())
        .expect_err("should fail without pitched source track");
    assert_eq!(err, DrumAssessError::NoSourceTrack);
}

#[test]
fn assess_drums_fast_combined_document() {
    // Create a 2-track document with both guitar and drums
    let mut doc = fixtures::drum_patterns();
    let guitar = fixtures::scale_run();
    // Add guitar track and bars
    let guitar_track_idx = doc.tracks.len();
    doc.tracks.push(guitar.tracks[0].clone());

    for (bar_idx, mb) in doc.master_bars.iter_mut().enumerate() {
        if let Some(guitar_bar_id) = guitar
            .master_bars
            .get(bar_idx)
            .and_then(|gmb| gmb.bar_ids.first())
        {
            mb.bar_ids.push(*guitar_bar_id);
            if let Some(gbar) = guitar.bars.iter().find(|b| b.id == *guitar_bar_id as u32) {
                doc.bars.push(gbar.clone());
            }
        } else {
            mb.bar_ids.push(-1);
        }
    }

    let result = assess_drums_fast(
        &doc,
        AssessDrumsOptions {
            track: Some(0),
            source_track: Some(guitar_track_idx),
            ..AssessDrumsOptions::default()
        },
    )
    .expect("assess_drums_fast succeeds");

    assert_eq!(result.drum_track_name, "drums");
    assert_eq!(result.bar_range, (0, doc.master_bars.len() - 1));
    assert!(!result.sections.is_empty());
}
