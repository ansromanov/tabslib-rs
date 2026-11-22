use super::*;

fn note(fret: i32, techniques: Vec<Technique>) -> Note {
    Note {
        id: 1,
        midi: Some(40 + fret),
        string: Some(0),
        fret: Some(fret),
        articulation: None,
        techniques,
    }
}

fn text(fret: i32, techniques: Vec<Technique>) -> String {
    fretted(&note(fret, techniques), 0).text
}

#[test]
fn plain_fret_is_its_number() {
    assert_eq!(text(12, vec![]), "12");
}

#[test]
fn decorations_follow_the_documented_rules() {
    assert_eq!(text(5, vec![Technique::Ghost]), "(5)");
    assert_eq!(text(5, vec![Technique::TieDestination]), "(5)");
    assert_eq!(text(5, vec![Technique::Dead]), "x");
    let natural = Technique::Harmonic {
        kind: HarmonicKind::Natural,
        fret: Some(7),
    };
    assert_eq!(text(7, vec![natural]), "<7>");
    let pinch = Technique::Harmonic {
        kind: HarmonicKind::Pinch,
        fret: None,
    };
    assert_eq!(text(7, vec![pinch]), "[7]");
}

#[test]
fn bends_read_as_target_frets_or_tone_fractions() {
    let bend = |origin, middle, dest| Technique::Bend {
        origin,
        middle,
        dest,
    };
    // 100 is a whole tone: two frets.
    assert_eq!(text(7, vec![bend(0, 50, 100)]), "7b9");
    assert_eq!(text(7, vec![bend(0, 0, 50)]), "7b8");
    assert_eq!(text(7, vec![bend(0, 100, 0)]), "7b9r7");
    assert_eq!(text(7, vec![bend(100, 100, 0)]), "7pb9r7");
    assert_eq!(text(7, vec![bend(0, 0, 25)]), "7b1/4");
    assert_eq!(text(7, vec![bend(0, 0, 75)]), "7b3/4");
}

#[test]
fn slide_in_prefixes_and_slide_out_connects() {
    assert_eq!(text(7, vec![Technique::Slide { flags: 16 }]), "/7");
    assert_eq!(text(7, vec![Technique::Slide { flags: 32 }]), "\\7");
    let out = fretted(&note(7, vec![Technique::Slide { flags: 4 }]), 0);
    assert_eq!(out.connector, Some(Connector::SlideDown));
    // A shift slide's direction depends on the next note: not decided here.
    let shift = fretted(&note(7, vec![Technique::Slide { flags: 1 }]), 0);
    assert_eq!(shift.connector, None);
}

#[test]
fn cells_keep_the_techniques_for_styling() {
    let cell = fretted(&note(3, vec![Technique::PalmMute, Technique::Accent]), 1);
    assert_eq!(
        cell.techniques,
        vec![Technique::PalmMute, Technique::Accent]
    );
    assert_eq!(cell.voice, 1);
    assert_eq!(cell.fret, Some(3));
}

#[test]
fn annotations_collect_row_techniques_only() {
    let n = note(
        3,
        vec![
            Technique::PalmMute,
            Technique::Vibrato,
            Technique::HammerOrigin,
            Technique::Harmonic {
                kind: HarmonicKind::Natural,
                fret: None,
            },
            Technique::Harmonic {
                kind: HarmonicKind::Artificial,
                fret: None,
            },
        ],
    );
    assert_eq!(
        annotations(&n),
        vec![
            Annotation::PalmMute,
            Annotation::Vibrato,
            Annotation::Harmonic(HarmonicKind::Artificial)
        ]
    );
    assert_eq!(Annotation::PalmMute.label(), "PM");
    assert_eq!(Annotation::LetRing.label(), "LR");
}

#[test]
fn drum_hits_distinguish_metal_and_skins() {
    let hit = Note {
        id: 1,
        midi: Some(42),
        string: None,
        fret: None,
        articulation: Some(42),
        techniques: vec![],
    };
    assert_eq!(drum(&hit, Some(DrumRole::HiHat), 0).text, "x");
    assert_eq!(drum(&hit, Some(DrumRole::Snare), 0).text, "o");
    let mut accented = hit.clone();
    accented.techniques.push(Technique::Accent);
    assert_eq!(drum(&accented, Some(DrumRole::Snare), 0).text, "O");
    let mut ghost = hit;
    ghost.techniques.push(Technique::Ghost);
    assert_eq!(drum(&ghost, Some(DrumRole::Snare), 0).text, "(o)");
}
