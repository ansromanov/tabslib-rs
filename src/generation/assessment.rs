//! Deterministic generation-quality measurements and caller-supplied calibration.
//!
//! Reports describe written score facts. Calibration compares those facts to
//! values supplied by a caller; it does not embed reference data or decide whether a
//! result is stylistically acceptable.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::{sounding_pitch, Bar, Document, Rhythm, Technique};

#[path = "assessment/quality.rs"]
mod quality;

pub use quality::{HYPERMETER_BARS, LOW_STRINGS};

/// A score region, using zero-based indexes into the document model.
///
/// Missing track, voice, or beat indexes identify a score-wide measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Region {
    /// Master-bar position.
    pub bar: usize,
    /// Track position.
    pub track: Option<usize>,
    /// Voice position within the bar.
    pub voice: Option<usize>,
    /// Beat position within the voice.
    pub beat: Option<usize>,
}

/// How much a finding matters.
///
/// Ordered worst first, so sorting a slice of findings by severity puts what
/// blocks a release ahead of what merely describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The score is wrong: the finding describes something no player can read.
    Error,
    /// The score is questionable and worth a look before release.
    Warning,
    /// The finding exists to support a metric, not to be acted on.
    Info,
}

impl Severity {
    /// Stable machine-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// The cause a finding belongs to.
///
/// Findings are emitted per beat and per note, so a single cause produces as
/// many of them as the document is long. The class is what collapses them back
/// into the handful of things actually worth reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticClass {
    /// Per-beat and per-note values that support a metric.
    Measurement,
    /// A written position that does not sound the pitch it states.
    Playability,
    /// A bar whose written pattern occurred earlier.
    Repetition,
    /// Distance between consecutive pitched events.
    VoiceLeading,
}

impl DiagnosticClass {
    /// Stable machine-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Measurement => "measurement",
            Self::Playability => "playability",
            Self::Repetition => "repetition",
            Self::VoiceLeading => "voice_leading",
        }
    }
}

/// One deterministic finding tied to the score region it describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Stable machine-readable finding name.
    pub code: &'static str,
    /// Region responsible for the finding.
    pub region: Region,
    /// Integer measurement associated with the finding.
    pub value: i64,
}

impl Diagnostic {
    /// Severity and class of a finding, derived from its code.
    ///
    /// Both are functions of the code rather than stored fields, so a finding
    /// cannot be constructed carrying a severity that disagrees with what it
    /// reports. An unrecognized code is `Info` and `Measurement`: a new
    /// supporting value is the common case, and guessing a higher severity for
    /// one would be worse than under-reporting it.
    fn classify(code: &str) -> (Severity, DiagnosticClass) {
        match code {
            "unplayable_position" => (Severity::Error, DiagnosticClass::Playability),
            "repeated_bar_pattern" => (Severity::Warning, DiagnosticClass::Repetition),
            "voice_leading_leap" => (Severity::Warning, DiagnosticClass::VoiceLeading),
            _ => (Severity::Info, DiagnosticClass::Measurement),
        }
    }

    /// How much this finding matters.
    pub fn severity(&self) -> Severity {
        Self::classify(self.code).0
    }

    /// The cause this finding belongs to.
    pub fn class(&self) -> DiagnosticClass {
        Self::classify(self.code).1
    }
}

/// Every finding of one class, collapsed into a single readable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticGroup {
    /// Cause shared by every finding in the group.
    pub class: DiagnosticClass,
    /// Worst severity among the findings in the group.
    pub severity: Severity,
    /// Number of findings collapsed into this group.
    pub count: usize,
    /// Codes present in the group, sorted and deduplicated.
    pub codes: Vec<&'static str>,
    /// Master bars the group touches, sorted and deduplicated.
    ///
    /// This is the region summary. Findings within one class differ from each
    /// other by track, voice and beat, and listing every one of those would
    /// reproduce the flat list the group exists to replace.
    pub bars: Vec<usize>,
}

/// Collapses findings into one row per class, worst severity first.
///
/// A document with one systemic cause produces one group rather than one
/// finding per beat, which is what makes a report readable without walking it.
pub fn group_diagnostics(diagnostics: &[Diagnostic]) -> Vec<DiagnosticGroup> {
    /// One class's findings while they are still being gathered.
    #[derive(Default)]
    struct Gathered {
        severity: Option<Severity>,
        count: usize,
        codes: BTreeSet<&'static str>,
        bars: BTreeSet<usize>,
    }

    let mut by_class: BTreeMap<DiagnosticClass, Gathered> = BTreeMap::new();
    for item in diagnostics {
        let entry = by_class.entry(item.class()).or_default();
        let severity = item.severity();
        entry.severity = Some(entry.severity.map_or(severity, |worst| worst.min(severity)));
        entry.count += 1;
        entry.codes.insert(item.code);
        entry.bars.insert(item.region.bar);
    }
    let mut groups: Vec<DiagnosticGroup> = by_class
        .into_iter()
        .map(|(class, gathered)| DiagnosticGroup {
            class,
            severity: gathered.severity.unwrap_or(Severity::Info),
            count: gathered.count,
            codes: gathered.codes.into_iter().collect(),
            bars: gathered.bars.into_iter().collect(),
        })
        .collect();
    groups.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then(right.count.cmp(&left.count))
            .then(left.class.cmp(&right.class))
    });
    groups
}

/// A non-negative exact measurement. The denominator is always at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    /// Measurement numerator.
    pub numerator: u64,
    /// Measurement denominator.
    pub denominator: u64,
}

impl Measurement {
    /// Creates a measurement and normalizes a zero denominator to one.
    pub fn new(numerator: u64, denominator: u64) -> Self {
        let denominator = denominator.max(1);
        let divisor = gcd(numerator, denominator);
        Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a.max(1)
    } else {
        gcd(b, a % b)
    }
}

/// A measured category in an assessment or calibration input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Notes whose written string and fret agree with the track tuning and MIDI pitch.
    Playability,
    /// Mean simultaneous note count at sounding beat positions.
    Texture,
    /// Share of compared bars whose exact written pattern appeared earlier.
    Repetition,
    /// Share of beats that contain no notes.
    Silence,
    /// Mean note count per beat.
    Density,
    /// Position of the densest bar, as a share of the document's bar span.
    ///
    /// Zero is the first bar and one the last. A single-bar document reports
    /// zero, because there is no span to place a peak within.
    Shape,
    /// Mean absolute semitone distance between consecutive pitched events.
    VoiceLeading,
    /// Palm-muted share of fretted-track notes.
    PalmMuteRate,
    /// Share of sounding fretted bars carrying at least one palm mute.
    PalmMuteSpread,
    /// Share of sounding percussion bars carrying a hi-hat or ride.
    CymbalCoverage,
    /// Tom share of percussion notes.
    TomShare,
    /// Share of compared bars whose written pattern occurs nowhere else.
    UniqueBarPatterns,
    /// Longest run of consecutive bars repeating an earlier run, in bars over one.
    LongestLoop,
    /// Master-bar count, expressed over one.
    BarCount,
    /// Share of sections whose length is a whole number of four-bar phrases.
    HypermeterAlignment,
    /// Share of sounding fretted beats carrying exactly one note.
    VoicingSingleNote,
    /// Share of sounding fretted beats carrying exactly two notes.
    VoicingPowerShape,
    /// Share of sounding fretted beats carrying three or more notes.
    VoicingExtendedShape,
    /// Share of positioned fretted notes played on the low strings.
    LowStringShare,
}

impl Metric {
    /// Stable machine-readable name, as it appears in serialized reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::Playability => "playability",
            Self::Texture => "texture",
            Self::Repetition => "repetition",
            Self::Silence => "silence",
            Self::Density => "density",
            Self::Shape => "shape",
            Self::VoiceLeading => "voice_leading",
            Self::PalmMuteRate => "palm_mute_rate",
            Self::PalmMuteSpread => "palm_mute_spread",
            Self::CymbalCoverage => "cymbal_coverage",
            Self::TomShare => "tom_share",
            Self::UniqueBarPatterns => "unique_bar_patterns",
            Self::LongestLoop => "longest_loop",
            Self::BarCount => "bar_count",
            Self::HypermeterAlignment => "hypermeter_alignment",
            Self::VoicingSingleNote => "voicing_single_note",
            Self::VoicingPowerShape => "voicing_power_shape",
            Self::VoicingExtendedShape => "voicing_extended_shape",
            Self::LowStringShare => "low_string_share",
        }
    }
}

/// A deterministic summary of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessmentReport {
    /// Exact values for every metric, in stable metric order when serialized.
    pub metrics: BTreeMap<Metric, Measurement>,
    /// Findings sorted by score region and stable code.
    pub diagnostics: Vec<Diagnostic>,
}

impl AssessmentReport {
    /// Collapses this report's findings into one row per cause, worst first.
    ///
    /// Use this rather than walking [`diagnostics`](Self::diagnostics): a
    /// document of any length emits a finding per beat and per note, and the
    /// grouped view is the one a reader can act on.
    pub fn summary(&self) -> Vec<DiagnosticGroup> {
        group_diagnostics(&self.diagnostics)
    }

    /// Returns a stable JSON representation suitable for snapshots and storage.
    pub fn to_json(&self) -> String {
        let mut output = String::from("{\"metrics\":[");
        for (index, (metric, value)) in self.metrics.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            output.push_str(&format!(
                "{{\"name\":\"{}\",\"numerator\":{},\"denominator\":{}}}",
                metric.name(),
                value.numerator,
                value.denominator
            ));
        }
        output.push_str("],\"diagnostics\":[");
        for (index, item) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            output.push_str(&format!(
                "{{\"code\":\"{}\",\"severity\":\"{}\",\"class\":\"{}\",\"bar\":{},\"track\":{},\"voice\":{},\"beat\":{},\"value\":{}}}",
                item.code,
                item.severity().name(),
                item.class().name(),
                item.region.bar,
                optional_index(item.region.track),
                optional_index(item.region.voice),
                optional_index(item.region.beat),
                item.value
            ));
        }
        output.push_str("]}");
        output
    }
}

fn optional_index(value: Option<usize>) -> String {
    value.map_or_else(|| "null".into(), |index| index.to_string())
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", u32::from(character)))
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

/// Measures every metric in [`Metric`] over one document.
///
/// Structural metrics -- playability, texture, repetition, silence, density,
/// shape and voice leading -- describe how the score is written. The
/// quality ratios -- palm-mute use and spread, percussion
/// balance, pattern variety, loop length, form alignment, voicing mix and
/// register -- describe how idiomatic it is.
///
/// No target value is compiled in. Every measurement is a ratio a caller
/// compares against its own samples through [`calibrate`], because the numbers
/// that make a ratio good are measured over a collection of music.
///
/// All values use exact integer ratios and all diagnostics point to zero-based
/// document regions. No style policy or reference data is applied.
pub fn assess(doc: &Document) -> AssessmentReport {
    let mut diagnostics = Vec::new();
    let mut positioned_total = 0u64;
    let mut positioned_valid = 0u64;
    let mut sounding_beats = 0u64;
    let mut beat_count = 0u64;
    let mut rests = 0u64;
    let mut repeated_bars = 0u64;
    let mut compared_bars = 0u64;
    let mut total_notes = 0u64;
    let mut bar_note_counts = Vec::new();
    let mut previous_patterns = Vec::new();
    let mut previous_pitches = BTreeMap::<(usize, usize), i32>::new();
    let mut leap_sum = 0u64;
    let mut leap_count = 0u64;

    for (bar_index, master) in doc.master_bars.iter().enumerate() {
        let mut master_notes = 0u64;
        for track_index in 0..doc.tracks.len() {
            let Some(bar_id) = master.bar_ids.get(track_index) else {
                continue;
            };
            let Some(bar) = doc
                .bars
                .iter()
                .find(|bar| i64::from(bar.id) == i64::from(*bar_id))
            else {
                continue;
            };
            compared_bars += 1;
            let pattern = bar_pattern(bar, master.time);
            if previous_patterns.contains(&(track_index, pattern.clone())) {
                repeated_bars += 1;
                diagnostics.push(Diagnostic {
                    code: "repeated_bar_pattern",
                    region: Region {
                        bar: bar_index,
                        track: Some(track_index),
                        voice: Some(0),
                        beat: Some(0),
                    },
                    value: 1,
                });
            } else {
                previous_patterns.push((track_index, pattern));
            }
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                for (beat_index, beat) in voice.beats.iter().enumerate() {
                    beat_count += 1;
                    let count = u64::try_from(beat.notes.len()).unwrap_or(u64::MAX);
                    master_notes += count;
                    total_notes += count;
                    let is_silent = beat.notes.is_empty();
                    if is_silent {
                        rests += 1;
                    } else {
                        sounding_beats += 1;
                    }
                    let region = Region {
                        bar: bar_index,
                        track: Some(track_index),
                        voice: Some(voice_index),
                        beat: Some(beat_index),
                    };
                    diagnostics.push(Diagnostic {
                        code: "silence_beat",
                        region,
                        value: i64::from(is_silent),
                    });
                    diagnostics.push(Diagnostic {
                        code: "density_beat",
                        region,
                        value: i64::try_from(count).unwrap_or(i64::MAX),
                    });
                    if !is_silent {
                        diagnostics.push(Diagnostic {
                            code: "texture_onset",
                            region,
                            value: i64::try_from(count).unwrap_or(i64::MAX),
                        });
                    }
                    for note in &beat.notes {
                        if !doc.tracks[track_index].tuning.is_empty() {
                            positioned_total += 1;
                            let valid = match (note.string, note.fret, note.midi) {
                                (Some(string), Some(fret), Some(midi)) if fret >= 0 => {
                                    sounding_pitch(&doc.tracks[track_index].tuning, string, fret)
                                        == Some(midi)
                                }
                                _ => false,
                            };
                            if valid {
                                positioned_valid += 1;
                            } else {
                                diagnostics.push(Diagnostic {
                                    code: "unplayable_position",
                                    region,
                                    value: i64::from(note.midi.unwrap_or(0)),
                                });
                            }
                            diagnostics.push(Diagnostic {
                                code: "playability_position",
                                region,
                                value: i64::from(valid),
                            });
                        }
                    }
                    if beat.notes.len() > 1 {
                        diagnostics.push(Diagnostic {
                            code: "simultaneous_notes",
                            region,
                            value: i64::try_from(beat.notes.len()).unwrap_or(i64::MAX),
                        });
                    }
                    let pitch = beat.notes.iter().find_map(|note| note.midi);
                    let previous = previous_pitches.get(&(track_index, voice_index)).copied();
                    if let (Some(previous), Some(current)) = (previous, pitch) {
                        let leap = previous.abs_diff(current);
                        leap_sum += u64::from(leap);
                        leap_count += 1;
                        if leap > 0 {
                            diagnostics.push(Diagnostic {
                                code: "voice_leading_leap",
                                region,
                                value: i64::from(leap),
                            });
                        }
                    }
                    if let Some(pitch) = pitch {
                        previous_pitches.insert((track_index, voice_index), pitch);
                    }
                }
            }
        }
        bar_note_counts.push(master_notes);
        diagnostics.push(Diagnostic {
            code: "shape_bar_density",
            region: Region {
                bar: bar_index,
                track: None,
                voice: None,
                beat: None,
            },
            value: i64::try_from(master_notes).unwrap_or(i64::MAX),
        });
    }
    let (quality_metrics, quality_diagnostics) = quality::measure(doc);
    diagnostics.extend(quality_diagnostics);
    diagnostics.sort_by_key(|item| (item.region, item.code));
    let mut metrics = BTreeMap::from([
        (
            Metric::Playability,
            Measurement::new(positioned_valid, positioned_total),
        ),
        (
            Metric::Texture,
            Measurement::new(total_notes, sounding_beats),
        ),
        (
            Metric::Repetition,
            Measurement::new(repeated_bars, compared_bars),
        ),
        (Metric::Silence, Measurement::new(rests, beat_count)),
        (Metric::Density, Measurement::new(total_notes, beat_count)),
        (Metric::Shape, quality::density_peak(&bar_note_counts)),
        (Metric::VoiceLeading, Measurement::new(leap_sum, leap_count)),
    ]);
    metrics.extend(quality_metrics);
    AssessmentReport {
        metrics,
        diagnostics,
    }
}

type NotePattern = (
    Option<i32>,
    Option<u32>,
    Option<i32>,
    Option<i32>,
    Vec<Technique>,
);
type BeatPattern = (Rhythm, Option<String>, Vec<NotePattern>);
type BarPattern = ((u32, u32), Option<String>, Vec<Vec<BeatPattern>>);

fn bar_pattern(bar: &Bar, meter: (u32, u32)) -> BarPattern {
    (
        meter,
        bar.clef.clone(),
        bar.voices
            .iter()
            .map(|voice| {
                voice
                    .beats
                    .iter()
                    .map(|beat| {
                        (
                            beat.rhythm,
                            beat.dynamic.clone(),
                            beat.notes
                                .iter()
                                .map(|note| {
                                    (
                                        note.midi,
                                        note.string,
                                        note.fret,
                                        note.articulation,
                                        note.techniques.clone(),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect()
            })
            .collect(),
    )
}

/// Caller-provided reference measurements for calibration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationSample {
    /// Caller-owned label; no label is inferred from score or reference data.
    pub label: String,
    /// Measurements supplied by the caller.
    pub metrics: BTreeMap<Metric, Measurement>,
}

/// Descriptive comparison between an observed metric and external references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationValue {
    /// Metric being compared.
    pub metric: Metric,
    /// Value measured from the document.
    pub observed: Measurement,
    /// Caller-supplied reference values and labels for this metric.
    pub references: Vec<(String, Measurement)>,
}

/// Calibration output contains comparisons only; it never returns a pass/fail policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationReport {
    /// External sample labels in caller order.
    pub sample_labels: Vec<String>,
    /// Metric comparisons in stable metric order.
    pub values: Vec<CalibrationValue>,
}

impl CalibrationReport {
    /// Returns stable JSON while preserving caller-provided sample order.
    pub fn to_json(&self) -> String {
        let labels = self
            .sample_labels
            .iter()
            .map(|label| json_string(label))
            .collect::<Vec<_>>()
            .join(",");
        let values = self.values.iter().map(|item| {
            let references = item.references.iter().map(|(label, measurement)| format!(
                "{{\"label\":{},\"numerator\":{},\"denominator\":{}}}",
                json_string(label), measurement.numerator, measurement.denominator
            )).collect::<Vec<_>>().join(",");
            format!(
                "{{\"name\":\"{}\",\"observed\":{{\"numerator\":{},\"denominator\":{}}},\"references\":[{}]}}",
                item.metric.name(), item.observed.numerator, item.observed.denominator, references
            )
        }).collect::<Vec<_>>().join(",");
        format!("{{\"samples\":[{}],\"metrics\":[{}]}}", labels, values)
    }
}

/// Compares an assessment with caller-supplied measurements.
pub fn calibrate(report: &AssessmentReport, samples: &[CalibrationSample]) -> CalibrationReport {
    let sample_labels = samples.iter().map(|sample| sample.label.clone()).collect();
    let values = report
        .metrics
        .iter()
        .map(|(metric, observed)| {
            let references = samples
                .iter()
                .filter_map(|sample| {
                    sample
                        .metrics
                        .get(metric)
                        .map(|value| (sample.label.clone(), *value))
                })
                .collect();
            CalibrationValue {
                metric: *metric,
                observed: *observed,
                references,
            }
        })
        .collect();
    CalibrationReport {
        sample_labels,
        values,
    }
}

#[cfg(test)]
#[path = "assessment_test.rs"]
mod tests;
