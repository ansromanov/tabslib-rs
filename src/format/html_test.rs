use super::*;

#[test]
fn renders_a_standalone_document_with_score_material() {
    let mut document = crate::fixtures::repeated_frets();
    document.title = "A <score> & tab".to_string();
    let html = render_score(&document).expect("render");

    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("A &lt;score&gt; &amp; tab"));
    assert!(html.contains("Tuning: E4 B3 G3 D3 A2 E2"));
    assert!(html.contains("Bar 1 4/4"));
    assert!(html.contains("class=\"rhythm\""));
    assert!(html.contains(">5</td>"));
}

#[test]
fn renders_only_the_requested_track_and_bar_range() {
    let mut document = crate::fixtures::repeated_frets();
    let mut second_track = document.tracks[0].clone();
    second_track.name = "gtr 2".to_string();
    document.tracks.push(second_track);
    for master_bar in &mut document.master_bars {
        master_bar.bar_ids.push(master_bar.bar_ids[0]);
    }
    let html = render_track_range(&document, 1, 1, 1).expect("render");

    assert!(html.contains("gtr 2"));
    assert!(html.contains("Bar 2 4/4"));
    assert!(!html.contains("Bar 1 4/4"));
    assert!(!html.contains("Bar 3 4/4"));
}

#[test]
fn write_format_returns_utf8_html() {
    let document = crate::fixtures::repeated_frets();
    let bytes = <Html as WriteFormat>::write(&document).expect("write");
    assert_eq!(
        String::from_utf8(bytes).expect("utf8"),
        render_score(&document).expect("render")
    );
}
