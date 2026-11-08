use super::*;

const FAMILIES: [&str; 4] = [
    "electricGuitar",
    "electricBass",
    "drumKit",
    "padSynthesizer",
];

fn track(id: usize, kind: &str, strip: bool) -> String {
    let strip = if strip {
        "<ChannelStrip version=\"E56\"><Parameters>0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5 0 0.5 0.5 \
         0.8 0.5 0.5 0.5</Parameters><Automations/></ChannelStrip>"
    } else {
        ""
    };
    format!(
        "<Track id=\"{id}\"><InstrumentSet><Type>{kind}</Type></InstrumentSet>\
         <RSE>{strip}</RSE></Track>"
    )
}

fn gpif(strips: bool) -> String {
    let tracks: String = FAMILIES
        .iter()
        .enumerate()
        .map(|(i, kind)| track(i, kind, strips))
        .collect();
    format!("<GPIF><ScoreZoom>1</ScoreZoom><Tracks>{tracks}</Tracks></GPIF>")
}

/// Masks for guitar, bass, drums, pad: notation+tab, notation+tab, notation, notation+tab.
fn part_config() -> Vec<u8> {
    let masks = [3u8, 3, 1, 3];
    let mut out = vec![0, 0, 0, 5, 0, 0, 0, 0, 4];
    out.extend(masks);
    out.extend([0, 0, 0, 0, 1]);
    for mask in &masks[..3] {
        out.extend([*mask, 0, 0, 0, 0, 1]);
    }
    out.extend([masks[3], 0, 0, 0, 1]);
    out
}

fn container(xml: &str) -> Vec<u8> {
    container::write_entries(&[
        ("Content/".to_string(), Vec::new()),
        ("Content/score.gpif".to_string(), xml.as_bytes().to_vec()),
        ("Content/PartConfiguration".to_string(), part_config()),
        ("Content/ScoreViews/1.gpsv".to_string(), vec![9, 8, 7]),
    ])
    .unwrap()
}

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    container::read_entries(bytes)
        .unwrap()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
}

/// The fader value of each track, in track order.
fn faders(bytes: &[u8]) -> Vec<String> {
    let xml = String::from_utf8(entry(bytes, "Content/score.gpif")).unwrap();
    xml.split("<Parameters>")
        .skip(1)
        .map(|p| p.split("</Parameters>").next().unwrap())
        .map(|p| p.split_whitespace().nth(12).unwrap().to_string())
        .collect()
}

fn all() -> Restyle {
    Restyle {
        zoom: Some(1.5),
        tab_only: true,
        volumes: Some(VolumeDb {
            guitar: -2.0,
            bass: -1.0,
            drums: 0.0,
            other: -3.0,
        }),
    }
}

#[test]
fn faders_follow_the_instrument_family() {
    let out = restyle(&container(&gpif(true)), &all()).unwrap();
    assert_eq!(faders(&out), ["0.76", "0.78", "0.8", "0.74"]);
}

#[test]
fn zoom_is_rewritten() {
    let out = restyle(&container(&gpif(true)), &all()).unwrap();
    let xml = String::from_utf8(entry(&out, "Content/score.gpif")).unwrap();
    assert!(xml.contains("<ScoreZoom>1.5</ScoreZoom>"));
}

#[test]
fn tab_only_changes_both_mask_lists_and_spares_notation_only_tracks() {
    let out = restyle(&container(&gpif(true)), &all()).unwrap();
    let part = entry(&out, "Content/PartConfiguration");
    assert_eq!(&part[9..13], [2, 2, 1, 2]);
    let records: Vec<u8> = (0..4).map(|i| part[18 + 6 * i]).collect();
    assert_eq!(records, [2, 2, 1, 2]);
}

#[test]
fn other_entries_and_directories_are_carried_over() {
    let input = container(&gpif(true));
    let out = restyle(&input, &all()).unwrap();
    assert_eq!(entry(&out, "Content/ScoreViews/1.gpsv"), [9, 8, 7]);
    let names = |b: &[u8]| -> Vec<String> {
        container::read_entries(b)
            .unwrap()
            .into_iter()
            .map(|e| e.0)
            .collect()
    };
    assert_eq!(names(&out), names(&input));
}

#[test]
fn default_options_change_no_content_and_output_is_deterministic() {
    let input = container(&gpif(true));
    let same = restyle(&input, &Restyle::default()).unwrap();
    assert_eq!(
        entry(&same, "Content/score.gpif"),
        entry(&input, "Content/score.gpif")
    );
    assert_eq!(entry(&same, "Content/PartConfiguration"), part_config());
    assert_eq!(
        restyle(&input, &all()).unwrap(),
        restyle(&input, &all()).unwrap()
    );
}

#[test]
fn a_track_without_a_channel_strip_is_an_error_not_a_skip() {
    let err = restyle(&container(&gpif(false)), &all()).unwrap_err();
    assert!(err.to_string().contains("channel strip"), "{err}");
}

#[test]
fn a_truncated_part_configuration_is_an_error() {
    let mut entries = container::read_entries(&container(&gpif(true))).unwrap();
    entries[2].1.truncate(12);
    let bytes = container::write_entries(&entries).unwrap();
    let opts = Restyle {
        tab_only: true,
        ..Restyle::default()
    };
    assert!(restyle(&bytes, &opts).is_err());
}
