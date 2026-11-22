use super::*;

#[test]
fn renders_deterministic_vector_pdf_with_score_content() {
    let document = crate::fixtures::repeated_frets();
    let first = render_score(&document).expect("render");
    let second = render_score(&document).expect("render");

    assert_eq!(first, second);
    assert!(first.starts_with(b"%PDF-1.4\n"));
    assert!(first
        .windows(b"/Type /Page".len())
        .any(|window| window == b"/Type /Page"));
    assert!(first
        .windows(b"Bar 1 4/4".len())
        .any(|window| window == b"Bar 1 4/4"));
    assert!(first.ends_with(b"%%EOF\n"));
}

#[test]
fn renders_bar_ranges_and_wraps_long_bars() {
    let mut document = crate::fixtures::repeated_frets();
    let extra_beats = document.bars[0].voices[0]
        .beats
        .iter()
        .cycle()
        .take(13)
        .cloned()
        .collect::<Vec<_>>();
    document.bars[0].voices[0].beats.extend(extra_beats);
    let pdf = render_track_range(&document, 0, 0, 0).expect("render");
    let text = String::from_utf8_lossy(&pdf);

    assert!(text.contains("Bar 1 4/4"));
    assert!(text.contains("\\(continued\\)"));
    assert!(!text.contains("Bar 2 4/4"));
}

#[test]
fn write_format_matches_direct_rendering() {
    let document = crate::fixtures::repeated_frets();
    assert_eq!(
        <Pdf as WriteFormat>::write(&document).expect("write"),
        render_score(&document).expect("render")
    );
}
