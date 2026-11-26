//! Chord detection from beat pitch-class counts, and Roman-numeral naming.

use super::*;

pub(super) fn detect_chord(
    counts: [usize; 12],
    bar_index: usize,
    key: Option<(i8, MusicalMode)>,
) -> Option<BarChord> {
    let distinct = counts.iter().filter(|count| **count > 0).count();
    if distinct < 3 {
        return None;
    }

    let templates = [
        (ChordQuality::Major, [0, 4, 7]),
        (ChordQuality::Minor, [0, 3, 7]),
        (ChordQuality::Diminished, [0, 3, 6]),
        (ChordQuality::Augmented, [0, 4, 8]),
    ];
    let mut best: Option<(usize, usize, ChordQuality, usize)> = None;
    for root in 0..12 {
        for (quality, intervals) in templates {
            let support = intervals
                .iter()
                .map(|interval| counts[(root + interval) % 12])
                .sum::<usize>();
            if intervals
                .iter()
                .any(|interval| counts[(root + interval) % 12] == 0)
            {
                continue;
            }
            let outside = counts
                .iter()
                .enumerate()
                .filter(|(pitch, count)| {
                    **count > 0
                        && !intervals
                            .iter()
                            .any(|interval| (root + interval) % 12 == *pitch)
                })
                .map(|(_, count)| count)
                .sum::<usize>();
            let score = support.saturating_sub(outside * 2);
            let key_preference = key.map_or(0, |(tonic, mode)| {
                usize::from(is_scale_root(root as i32, tonic, mode))
            });
            let candidate = (score, key_preference, quality, root);
            if best.as_ref().is_none_or(|current| {
                (candidate.0, candidate.1, usize::MAX - candidate.3)
                    > (current.0, current.1, usize::MAX - current.3)
            }) {
                best = Some(candidate);
            }
        }
    }
    let (score, _, quality, root) = best?;
    if score == 0 {
        return None;
    }
    Some(BarChord {
        bar_index,
        root: root as i32,
        quality,
        numeral: key.map(|(tonic, mode)| roman_numeral(root as i32, quality, tonic, mode)),
        supporting_notes: intervals_for(quality)
            .iter()
            .map(|interval| counts[(root + interval) % 12])
            .sum(),
    })
}

pub(super) fn intervals_for(quality: ChordQuality) -> [usize; 3] {
    match quality {
        ChordQuality::Major => [0, 4, 7],
        ChordQuality::Minor => [0, 3, 7],
        ChordQuality::Diminished => [0, 3, 6],
        ChordQuality::Augmented => [0, 4, 8],
    }
}

pub(super) fn is_scale_root(root: i32, tonic: i8, mode: MusicalMode) -> bool {
    mode.intervals()
        .contains(&((root - i32::from(tonic)).rem_euclid(12)))
}

pub(super) fn roman_numeral(
    root: i32,
    quality: ChordQuality,
    tonic: i8,
    mode: MusicalMode,
) -> String {
    let degree = mode
        .intervals()
        .iter()
        .position(|interval| (i32::from(tonic) + interval).rem_euclid(12) == root)
        .map(|degree| (degree, 0))
        .or_else(|| {
            mode.intervals()
                .iter()
                .enumerate()
                .find_map(|(degree, interval)| {
                    let difference = (root - (i32::from(tonic) + interval)).rem_euclid(12);
                    match difference {
                        1 => Some((degree, 1)),
                        11 => Some((degree, -1)),
                        _ => None,
                    }
                })
        });
    let Some((degree, alteration)) = degree else {
        return "?".into();
    };
    let numeral = ["I", "II", "III", "IV", "V", "VI", "VII"][degree];
    let accidental = match alteration {
        -1 => "b",
        1 => "#",
        _ => "",
    };
    let body = match quality {
        ChordQuality::Major => numeral.to_string(),
        ChordQuality::Minor => numeral.to_ascii_lowercase(),
        ChordQuality::Diminished => format!("{}°", numeral.to_ascii_lowercase()),
        ChordQuality::Augmented => format!("{numeral}+"),
    };
    format!("{accidental}{body}")
}
