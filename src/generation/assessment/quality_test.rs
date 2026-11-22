use super::*;

use crate::fixtures;
use crate::generation::assessment::assess;

/// The measured value of one metric, as a pair so an exact ratio can be
/// asserted without floating point.
fn ratio(doc: &Document, metric: Metric) -> (u64, u64) {
    let report = assess(doc);
    let value = report
        .metrics
        .get(&metric)
        .copied()
        .expect("metric measured");
    (value.numerator, value.denominator)
}

#[test]
fn shape_reports_where_the_density_peaks_not_how_far_the_ends_differ() {
    // Five bars whose note counts climb to the middle and fall back. The old
    // computation -- the first-to-last delta over the bar count -- reads zero
    // here, because the ends match. The peak is bar 2 of a four-bar span.
    let counts = [1u64, 3, 9, 3, 1];
    let peak = density_peak(&counts);
    assert_eq!((peak.numerator, peak.denominator), (1, 2), "bar 2 of 4");

    let first_to_last = counts
        .first()
        .zip(counts.last())
        .map(|(first, last)| first.abs_diff(*last))
        .unwrap_or(0);
    assert_eq!(
        first_to_last, 0,
        "the delta the metric used to report cannot see this shape at all"
    );
}

#[test]
fn shape_separates_a_mid_peak_from_a_steady_climb() {
    let mid_peak = density_peak(&[1, 3, 9, 3, 1]);
    let climb = density_peak(&[1, 3, 5, 7, 9]);
    assert_ne!(
        (mid_peak.numerator, mid_peak.denominator),
        (climb.numerator, climb.denominator),
        "two different shapes must not measure the same"
    );
    assert_eq!(
        (climb.numerator, climb.denominator),
        (1, 1),
        "peaks at the end"
    );
}

#[test]
fn shape_resolves_a_tie_to_the_earliest_peak() {
    let peak = density_peak(&[9, 1, 9, 1, 1]);
    assert_eq!((peak.numerator, peak.denominator), (0, 1));
}

#[test]
fn shape_reports_zero_for_a_document_with_no_span() {
    assert_eq!(density_peak(&[]).numerator, 0);
    assert_eq!(density_peak(&[7]).numerator, 0);
}

#[test]
fn palm_mute_rate_counts_muted_notes_against_fretted_notes() {
    // Every note in the fixture is palm muted.
    assert_eq!(
        ratio(&fixtures::repeated_frets(), Metric::PalmMuteRate),
        (1, 1)
    );
    assert_eq!(ratio(&fixtures::scale_run(), Metric::PalmMuteRate), (0, 1));
}

#[test]
fn palm_mute_spread_separates_placement_from_rate() {
    let mut doc = fixtures::repeated_frets();
    // Strip the mute from every bar but the first. The rate falls, and so does
    // the spread; a rate that moved while the spread stood still would mean
    // the two are measuring the same thing.
    let keep = doc.master_bars[0].bar_ids[0];
    for bar in &mut doc.bars {
        if i64::from(bar.id) == i64::from(keep) {
            continue;
        }
        for voice in &mut bar.voices {
            for beat in &mut voice.beats {
                for note in &mut beat.notes {
                    note.techniques.clear();
                }
            }
        }
    }
    assert_eq!(ratio(&doc, Metric::PalmMuteRate), (1, 5));
    assert_eq!(
        ratio(&doc, Metric::PalmMuteSpread),
        (1, 5),
        "one bar of five carries the mute"
    );

    // Now spread the same number of muted notes across every bar. The rate is
    // unchanged; only the spread moves.
    let mut spread = fixtures::repeated_frets();
    for bar in &mut spread.bars {
        for voice in &mut bar.voices {
            for (index, beat) in voice.beats.iter_mut().enumerate() {
                if index == 0 {
                    continue;
                }
                for note in &mut beat.notes {
                    note.techniques.clear();
                }
            }
        }
    }
    assert_eq!(ratio(&spread, Metric::PalmMuteRate), (1, 8));
    assert_eq!(
        ratio(&spread, Metric::PalmMuteSpread),
        (1, 1),
        "every bar carries a mute, which the aggregate cannot show"
    );
}

#[test]
fn cymbal_coverage_and_tom_share_read_the_drum_kit() {
    let doc = fixtures::drum_patterns();
    let (covered, bars) = ratio(&doc, Metric::CymbalCoverage);
    assert!(bars > 0, "the drum fixture has sounding percussion bars");
    assert!(covered > 0, "a rock beat keeps time on a hi-hat or ride");

    let (toms, notes) = ratio(&doc, Metric::TomShare);
    assert!(notes > 0);
    assert!(
        toms < notes,
        "toms are a minority voice, got {toms} of {notes}"
    );
}

#[test]
fn a_fretted_only_document_reports_no_percussion_balance() {
    let doc = fixtures::power_chords();
    assert_eq!(ratio(&doc, Metric::CymbalCoverage), (0, 1));
    assert_eq!(ratio(&doc, Metric::TomShare), (0, 1));
}

#[test]
fn unique_bar_patterns_falls_when_bars_repeat() {
    // Every bar of the repeated-fret fixture states a different fret, so no
    // two bars share a pattern.
    assert_eq!(
        ratio(&fixtures::repeated_frets(), Metric::UniqueBarPatterns),
        (1, 1)
    );

    let mut doc = fixtures::repeated_frets();
    let first = doc.master_bars[0].bar_ids[0];
    for master in &mut doc.master_bars {
        master.bar_ids[0] = first;
    }
    assert_eq!(
        ratio(&doc, Metric::UniqueBarPatterns),
        (1, 5),
        "one distinct pattern across five bars"
    );
}

#[test]
fn longest_loop_measures_the_run_not_the_count_of_repeats() {
    // Four bars repeated once: the repetition ratio and the loop length are
    // different facts, and this is the case that separates them.
    let mut looped = fixtures::repeated_frets();
    let ids: Vec<i32> = looped.master_bars.iter().map(|m| m.bar_ids[0]).collect();
    for (index, master) in looped.master_bars.iter_mut().enumerate() {
        master.bar_ids[0] = ids[index % 2];
    }
    let (run, over) = ratio(&looped, Metric::LongestLoop);
    assert_eq!(over, 1, "an absolute is reported over one");
    assert!(run >= 2, "a two-bar figure comes back around, got {run}");
}

#[test]
fn bar_count_reports_the_document_length_over_one() {
    let doc = fixtures::repeated_frets();
    assert_eq!(
        ratio(&doc, Metric::BarCount),
        (doc.master_bars.len() as u64, 1)
    );
}

#[test]
fn hypermeter_alignment_counts_sections_of_whole_four_bar_phrases() {
    let doc = fixtures::meters_and_sections();
    let (aligned, sections) = ratio(&doc, Metric::HypermeterAlignment);
    assert!(sections > 0, "the fixture carries section markers");
    assert!(aligned <= sections);
}

#[test]
fn a_document_without_sections_reports_no_hypermeter() {
    assert_eq!(
        ratio(&fixtures::repeated_frets(), Metric::HypermeterAlignment),
        (0, 1)
    );
}

#[test]
fn the_voicing_mix_partitions_every_sounding_fretted_beat() {
    let doc = fixtures::power_chords();
    let (single, _) = ratio(&doc, Metric::VoicingSingleNote);
    assert_eq!(single, 0, "power chords carry no single-note beats");

    // Measurements are reduced, so the numerators are not raw counts and
    // cannot be added directly. Summing them as rationals is what checks the
    // partition.
    let shares = [
        Metric::VoicingSingleNote,
        Metric::VoicingPowerShape,
        Metric::VoicingExtendedShape,
    ]
    .map(|metric| ratio(&doc, metric));
    let denominator: u128 = shares.iter().map(|(_, den)| u128::from(*den)).product();
    let numerator: u128 = shares
        .iter()
        .map(|(num, den)| u128::from(*num) * denominator / u128::from(*den))
        .sum();
    assert_eq!(
        numerator, denominator,
        "the three shares must account for every beat exactly once"
    );

    let melodic = fixtures::scale_run();
    assert_eq!(ratio(&melodic, Metric::VoicingSingleNote), (1, 1));
}

#[test]
fn low_string_share_reads_the_register() {
    // The fixtures sit on string 0, the lowest.
    assert_eq!(
        ratio(&fixtures::repeated_frets(), Metric::LowStringShare),
        (1, 1)
    );

    let mut high = fixtures::repeated_frets();
    for bar in &mut high.bars {
        for voice in &mut bar.voices {
            for beat in &mut voice.beats {
                for note in &mut beat.notes {
                    note.string = Some(5);
                    note.midi = crate::model::sounding_pitch(
                        &fixtures::TUNING_E_STANDARD,
                        5,
                        note.fret.expect("a fretted note"),
                    );
                }
            }
        }
    }
    assert_eq!(ratio(&high, Metric::LowStringShare), (0, 1));
}

#[test]
fn every_metric_is_measured_for_every_document() {
    let all = [
        Metric::PalmMuteRate,
        Metric::PalmMuteSpread,
        Metric::CymbalCoverage,
        Metric::TomShare,
        Metric::UniqueBarPatterns,
        Metric::LongestLoop,
        Metric::BarCount,
        Metric::HypermeterAlignment,
        Metric::VoicingSingleNote,
        Metric::VoicingPowerShape,
        Metric::VoicingExtendedShape,
        Metric::LowStringShare,
    ];
    for (name, doc) in fixtures::all() {
        let report = assess(&doc);
        for metric in all {
            assert!(
                report.metrics.contains_key(&metric),
                "{name}: {metric:?} is missing"
            );
        }
    }
}

#[test]
fn no_target_value_is_compiled_into_the_crate() {
    // Quality thresholds are chosen by the caller.
    // Only structural constants live here.
    assert_eq!(LOW_STRINGS, 3);
    assert_eq!(HYPERMETER_BARS, 4);
}
