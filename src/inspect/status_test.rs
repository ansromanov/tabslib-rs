use super::*;

#[test]
fn status_reports_score_shape_and_capabilities() {
    let score = crate::fixtures::repeated_frets();
    let status = song_status(&score);
    assert_eq!(status.bars, score.master_bars.len());
    assert!(capability_report(&score).pitched_tracks);
    assert!(rhythm_analysis(&score, 0, 0).is_some());
}

#[test]
fn status_reports_generator_metadata_aligned_with_tracks() {
    let mut score = crate::fixtures::repeated_frets();
    score.tracks[0].generator_metadata = Some(crate::model::GeneratorMetadata {
        engine: "tabslib.bass".into(),
        archetype: Some("Rock".into()),
        settings: std::collections::BTreeMap::from([("seed".into(), "9".into())]),
    });
    let status = song_status(&score);
    assert_eq!(status.generator_metadata.len(), score.tracks.len());
    assert_eq!(
        status.generator_metadata[0]
            .as_ref()
            .map(|metadata| metadata.engine.as_str()),
        Some("tabslib.bass")
    );
}
