//! Run a deterministic native workflow entirely over a [`Document`].

use tabslib::facade;
use tabslib::facade::generation::{
    self, ArrangementRequest, BassGenerationRequest, DrumGenerationRequest, DrumProfile,
    HarmonyConfig, HarmonyType, SectionRequest,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = tabslib::fixtures::power_chords();
    let source_tracks = [0];
    let drum_profile = DrumProfile {
        kick_voice: 35,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 80,
        snare_probability: 70,
        cymbal_probability: 75,
    };

    generation::generate_drums(
        &mut document,
        DrumGenerationRequest {
            source_tracks: &source_tracks,
            profile: &drum_profile,
            seed: 17,
            tempo: 120,
        },
    )?;
    generation::generate_bass(
        &mut document,
        BassGenerationRequest {
            source_tracks: &source_tracks,
            profile: generation::BassProfile {
                min_midi: 28,
                max_midi: 52,
                max_leap: 7,
                accent_velocity: 110,
                normal_velocity: 90,
            },
            strategy: generation::BassStrategy::PumpingEighths,
            tuning: generation::BassTuning::Custom(&[38, 45, 50, 55]),
            max_fret: 12,
        },
    )?;
    generation::generate_harmony(
        &mut document,
        HarmonyConfig {
            source_track: 0,
            key: None,
            harmony: HarmonyType::Parallel { semitones: 7 },
            tuning: tabslib::fixtures::TUNING_E_STANDARD.to_vec(),
            max_fret: 24,
            max_leap: 12,
            octave_range: 1,
            seed: 17,
            track_name: "Second Guitar".into(),
        },
    )?;

    let motifs = generation::segment_motifs(&document, document.master_bars.len(), 0.0)?;
    let bars = document.master_bars.len();
    let arranged = generation::arrange(
        &document,
        ArrangementRequest {
            motifs: &motifs,
            sections: &[SectionRequest {
                label: "A".into(),
                bars,
                candidates: vec![0],
                transpose_semitones: 0,
                energy: None,
                instrumentation: None,
                energy_texture: None,
                climax: false,
            }],
            seed: 17,
        },
    )?;

    let summary = facade::inspect::summary(&arranged);
    let report = generation::assess_document(&arranged);
    let reference = generation::CalibrationSample {
        label: "same generated score".into(),
        metrics: report.metrics.clone(),
    };
    let policy = generation::GatePolicy {
        tolerance: report
            .metrics
            .keys()
            .map(|metric| (*metric, generation::Measurement::new(0, 1)))
            .collect(),
        quarantined: std::collections::BTreeSet::new(),
    };
    let gate = generation::gate_document(&report, &[reference], &policy);
    println!("{} bars, {} tracks", summary.bars, summary.tracks.len());
    println!("assessment: {} metrics", report.metrics.len());
    println!("quality gate: {}", gate.passed());
    for group in report.summary() {
        println!(
            "  {:>7} {:<13} {:>4} finding(s) over {} bar(s): {}",
            group.severity.name(),
            group.class.name(),
            group.count,
            group.bars.len(),
            group.codes.join(", ")
        );
    }

    // File adapters are optional; the workflow above never depends on bytes.
    let _bytes = <facade::format::gp::Gp as facade::format::WriteFormat>::write(&arranged)?;
    Ok(())
}
