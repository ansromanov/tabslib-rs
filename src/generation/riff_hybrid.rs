//! Deterministic hybrids and caller-configured candidate filtering.

use crate::generation::assessment::{self, AssessmentReport, Measurement, Metric};
use crate::generation::requirements::{self, Requirement, Violation};
use crate::model::Document;

/// A way to take complementary material from two riff documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridMode {
    /// Keep A's attacks and durations; apply B's pitch contour to A's first pitch.
    RhythmAContourB,
    /// Keep A's pitches and attacks; apply B's techniques and dynamics.
    PitchesAArticulationB,
    /// Keep A's framework and place B's attacks into A's rests as answers.
    LowFrameworkAAnswersB,
    /// Alternate bars from A and B, beginning with A.
    InterleaveBars,
}

/// Inputs selecting one riff on each source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridRequest {
    /// Source track from the first document; this is the output track.
    pub track_a: usize,
    /// Source track from the second document.
    pub track_b: usize,
    /// Transformation applied to the selected tracks.
    pub mode: HybridMode,
    /// Seed used to choose a stable alignment offset for source attacks.
    pub seed: u64,
}

/// Why two riffs cannot be combined without changing their meter or topology.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HybridError {
    /// A selected track is missing or has no playable tuning.
    #[error("track {0} is not a fretted track in the source document")]
    InvalidTrack(usize),
    /// The documents have different bar counts.
    #[error("the source riffs have different bar counts")]
    DifferentBarCount,
    /// The source meters differ at one or more bar positions.
    #[error("source meters differ at bar {0}")]
    IncompatibleMeter(usize),
    /// A bar-copying mode requires matching instrument tunings.
    #[error("source tunings differ for a mode that copies fingering")]
    IncompatibleTuning,
    /// The chosen riff has no pitched notes to combine.
    #[error("track {0} has no pitched notes in the selected riff")]
    NoPitchedNotes(usize),
    /// A score reference points to a missing bar.
    #[error("bar data is missing at bar {bar}, track {track}")]
    MissingBar { bar: usize, track: usize },
    /// B's contour cannot be placed in A's instrument range.
    #[error("contour pitch {pitch} cannot be played on track {track}")]
    UnplayablePitch { track: usize, pitch: i32 },
}

/// Combines selected riffs. The inputs remain unchanged, and the same request
/// always produces the same score. Both sources must share bar positions and
/// meters so onsets can be transferred without shifting the song structure.
pub fn hybridize(
    a: &Document,
    b: &Document,
    request: HybridRequest,
) -> Result<Document, HybridError> {
    validate_track(a, request.track_a)?;
    validate_track(b, request.track_b)?;
    if a.master_bars.len() != b.master_bars.len() {
        return Err(HybridError::DifferentBarCount);
    }
    for (index, (left, right)) in a.master_bars.iter().zip(&b.master_bars).enumerate() {
        if left.time != right.time {
            return Err(HybridError::IncompatibleMeter(index));
        }
    }
    if matches!(
        request.mode,
        HybridMode::LowFrameworkAAnswersB | HybridMode::InterleaveBars
    ) && a.tracks[request.track_a].tuning != b.tracks[request.track_b].tuning
    {
        return Err(HybridError::IncompatibleTuning);
    }

    let source_bars = bars_for_track(b, request.track_b)?;
    let mut output = a.clone();
    let target_bars = bars_for_track(a, request.track_a)?;
    if matches!(
        request.mode,
        HybridMode::RhythmAContourB | HybridMode::PitchesAArticulationB
    ) && source_bars.iter().all(|bar| note_count(bar) == 0)
    {
        return Err(HybridError::NoPitchedNotes(request.track_b));
    }
    if matches!(
        request.mode,
        HybridMode::RhythmAContourB | HybridMode::LowFrameworkAAnswersB
    ) && target_bars.iter().all(|bar| note_count(bar) == 0)
    {
        return Err(HybridError::NoPitchedNotes(request.track_a));
    }

    for (bar_index, (target, source)) in target_bars.iter().zip(&source_bars).enumerate() {
        match request.mode {
            HybridMode::RhythmAContourB => apply_contour(
                &mut output,
                request.track_a,
                bar_index,
                target,
                source,
                request.seed,
            )?,
            HybridMode::PitchesAArticulationB => apply_articulation(
                &mut output,
                request.track_a,
                bar_index,
                target,
                source,
                request.seed,
            ),
            HybridMode::LowFrameworkAAnswersB => {
                apply_answers(&mut output, request.track_a, bar_index, source)
            }
            HybridMode::InterleaveBars if bar_index % 2 == 1 => {
                replace_bar_content(&mut output, request.track_a, bar_index, source)?
            }
            HybridMode::InterleaveBars => {}
        }
    }
    assign_fresh_event_ids(&mut output);
    Ok(output)
}

/// A caller-supplied inclusive ratio bound for one assessment metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricThreshold {
    /// Metric measured by [`assessment::assess`].
    pub metric: Metric,
    /// Inclusive lower bound; absent means unbounded below.
    pub minimum: Option<Measurement>,
    /// Inclusive upper bound; absent means unbounded above.
    pub maximum: Option<Measurement>,
}

/// One named candidate supplied to [`filter_candidates`].
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Caller-owned stable candidate name.
    pub name: String,
    /// Candidate score to inspect.
    pub document: Document,
}

/// A candidate rejected by a hard requirement or one metric threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateRejection {
    /// A requested hard requirement failed.
    Requirement(Violation),
    /// The assessment report did not contain the requested metric.
    MissingMetric(Metric),
    /// A measured ratio is below a caller's inclusive minimum.
    BelowMinimum { metric: Metric, actual: Measurement },
    /// A measured ratio is above a caller's inclusive maximum.
    AboveMaximum { metric: Metric, actual: Measurement },
}

/// Assessment and caller-policy results for one input candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateResult {
    /// Caller-owned stable candidate name.
    pub name: String,
    /// Whether every requested requirement and metric bound passed.
    pub accepted: bool,
    /// All rejection reasons in requirement and threshold order.
    pub rejections: Vec<CandidateRejection>,
    /// Reusable assessment metrics for the caller's own ranking policy.
    pub assessment: AssessmentReport,
}

/// Measures candidates and applies only the hard requirements and thresholds
/// supplied by the caller. Passing candidates are not assigned a ranking.
pub fn filter_candidates(
    candidates: impl IntoIterator<Item = Candidate>,
    required: &[Requirement],
    thresholds: &[MetricThreshold],
) -> Vec<CandidateResult> {
    candidates
        .into_iter()
        .map(|candidate| {
            let violations = requirements::check(&candidate.document);
            let report = assessment::assess(&candidate.document);
            let mut rejections = violations
                .into_iter()
                .filter(|violation| required.contains(&violation.requirement))
                .map(CandidateRejection::Requirement)
                .collect::<Vec<_>>();
            for threshold in thresholds {
                let Some(actual) = report.metrics.get(&threshold.metric).copied() else {
                    rejections.push(CandidateRejection::MissingMetric(threshold.metric));
                    continue;
                };
                if threshold
                    .minimum
                    .is_some_and(|minimum| compare(actual, minimum).is_lt())
                {
                    rejections.push(CandidateRejection::BelowMinimum {
                        metric: threshold.metric,
                        actual,
                    });
                }
                if threshold
                    .maximum
                    .is_some_and(|maximum| compare(actual, maximum).is_gt())
                {
                    rejections.push(CandidateRejection::AboveMaximum {
                        metric: threshold.metric,
                        actual,
                    });
                }
            }
            CandidateResult {
                name: candidate.name,
                accepted: rejections.is_empty(),
                rejections,
                assessment: report,
            }
        })
        .collect()
}

fn compare(left: Measurement, right: Measurement) -> std::cmp::Ordering {
    (u128::from(left.numerator) * u128::from(right.denominator))
        .cmp(&(u128::from(right.numerator) * u128::from(left.denominator)))
}

fn validate_track(document: &Document, track: usize) -> Result<(), HybridError> {
    let Some(track_data) = document.tracks.get(track) else {
        return Err(HybridError::InvalidTrack(track));
    };
    if track_data.tuning.is_empty() || track_data.tuning.iter().all(|pitch| *pitch == 0) {
        return Err(HybridError::InvalidTrack(track));
    }
    Ok(())
}

fn bars_for_track(
    document: &Document,
    track: usize,
) -> Result<Vec<crate::model::Bar>, HybridError> {
    document
        .master_bars
        .iter()
        .enumerate()
        .map(|(bar, master)| {
            let id = master
                .bar_ids
                .get(track)
                .copied()
                .and_then(|id| u32::try_from(id).ok())
                .ok_or(HybridError::MissingBar { bar, track })?;
            document
                .bars
                .iter()
                .find(|source| source.id == id)
                .cloned()
                .ok_or(HybridError::MissingBar { bar, track })
        })
        .collect()
}

fn note_count(bar: &crate::model::Bar) -> usize {
    bar.voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .map(|beat| beat.notes.len())
        .sum()
}

fn notes(bar: &crate::model::Bar) -> Vec<(usize, usize, usize)> {
    bar.voices
        .iter()
        .enumerate()
        .flat_map(|(voice, data)| {
            data.beats.iter().enumerate().flat_map(move |(beat, data)| {
                (0..data.notes.len()).map(move |note| (voice, beat, note))
            })
        })
        .collect()
}

fn aligned_index(index: usize, target_len: usize, source_len: usize, seed: u64) -> usize {
    if source_len <= 1 || target_len <= 1 {
        return 0;
    }
    let shift = usize::try_from(seed % u64::try_from(source_len).unwrap_or(1)).unwrap_or(0);
    (index * source_len / target_len + shift) % source_len
}

fn apply_contour(
    output: &mut Document,
    track: usize,
    bar_index: usize,
    target: &crate::model::Bar,
    source: &crate::model::Bar,
    seed: u64,
) -> Result<(), HybridError> {
    let target_notes = notes(target);
    let source_notes = notes(source);
    let Some(&(first_voice, first_beat, first_note)) = source_notes.first() else {
        return Ok(());
    };
    let Some(source_root) = source.voices[first_voice].beats[first_beat].notes[first_note].midi
    else {
        return Ok(());
    };
    let target_root = target_notes
        .first()
        .and_then(|&(voice, beat, note)| target.voices[voice].beats[beat].notes[note].midi);
    let Some(target_root) = target_root else {
        return Ok(());
    };
    let source_len = source_notes.len();
    let target_len = target_notes.len();
    let tuning = output.tracks[track].tuning.clone();
    let Some(bar_id) = output.master_bars[bar_index].bar_ids.get(track).copied() else {
        return Err(HybridError::MissingBar {
            bar: bar_index,
            track,
        });
    };
    let bar_id = u32::try_from(bar_id).map_err(|_| HybridError::MissingBar {
        bar: bar_index,
        track,
    })?;
    let Some(bar) = output.bars.iter_mut().find(|bar| bar.id == bar_id) else {
        return Err(HybridError::MissingBar {
            bar: bar_index,
            track,
        });
    };
    for (index, (voice, beat, note)) in target_notes.into_iter().enumerate() {
        let (source_voice, source_beat, source_note) =
            source_notes[aligned_index(index, target_len, source_len, seed)];
        let Some(source_pitch) =
            source.voices[source_voice].beats[source_beat].notes[source_note].midi
        else {
            continue;
        };
        let pitch = target_root + source_pitch - source_root;
        let Some((string, fret)) = position(&tuning, pitch) else {
            return Err(HybridError::UnplayablePitch { track, pitch });
        };
        let target = &mut bar.voices[voice].beats[beat].notes[note];
        target.midi = Some(pitch);
        target.string = Some(string);
        target.fret = Some(fret);
    }
    Ok(())
}

fn apply_articulation(
    output: &mut Document,
    track: usize,
    bar_index: usize,
    target: &crate::model::Bar,
    source: &crate::model::Bar,
    seed: u64,
) {
    let target_notes = notes(target);
    let source_notes = notes(source);
    if source_notes.is_empty() || target_notes.is_empty() {
        return;
    }
    let target_len = target_notes.len();
    let source_len = source_notes.len();
    let Some(bar_id) = output.master_bars[bar_index].bar_ids.get(track).copied() else {
        return;
    };
    let Some(bar_id) = u32::try_from(bar_id).ok() else {
        return;
    };
    let Some(bar) = output.bars.iter_mut().find(|bar| bar.id == bar_id) else {
        return;
    };
    for (index, (voice, beat, note)) in target_notes.into_iter().enumerate() {
        let (source_voice, source_beat, source_note) =
            source_notes[aligned_index(index, target_len, source_len, seed)];
        bar.voices[voice].beats[beat].notes[note].techniques =
            source.voices[source_voice].beats[source_beat].notes[source_note]
                .techniques
                .clone();
        bar.voices[voice].beats[beat].dynamic = source.voices[source_voice].beats[source_beat]
            .dynamic
            .clone();
    }
}

fn apply_answers(
    output: &mut Document,
    track: usize,
    bar_index: usize,
    source: &crate::model::Bar,
) {
    let Some(bar_id) = output.master_bars[bar_index].bar_ids.get(track).copied() else {
        return;
    };
    let Some(bar_id) = u32::try_from(bar_id).ok() else {
        return;
    };
    let tuning = output.tracks[track].tuning.clone();
    let Some(bar) = output.bars.iter_mut().find(|bar| bar.id == bar_id) else {
        return;
    };
    for (voice_index, voice) in bar.voices.iter_mut().enumerate() {
        let Some(source_voice) = source.voices.get(voice_index) else {
            continue;
        };
        let target_beats = voice.beats.len();
        let source_beats = source_voice.beats.len();
        if source_beats == 0 {
            continue;
        }
        for (beat_index, beat) in voice.beats.iter_mut().enumerate() {
            if !beat.notes.is_empty() {
                continue;
            }
            let source_index = beat_index * source_beats / target_beats.max(1);
            let source_beat = &source_voice.beats[source_index.min(source_beats - 1)];
            let answers = source_beat
                .notes
                .iter()
                .filter_map(|note| {
                    let pitch = note.midi?;
                    let (string, fret) = position(&tuning, pitch)?;
                    let mut answer = note.clone();
                    answer.string = Some(string);
                    answer.fret = Some(fret);
                    Some(answer)
                })
                .collect::<Vec<_>>();
            if !answers.is_empty() {
                beat.notes = answers;
                beat.dynamic = source_beat.dynamic.clone();
            }
        }
    }
}

fn replace_bar_content(
    output: &mut Document,
    track: usize,
    bar_index: usize,
    source: &crate::model::Bar,
) -> Result<(), HybridError> {
    let Some(id) = output.master_bars[bar_index].bar_ids.get(track).copied() else {
        return Err(HybridError::MissingBar {
            bar: bar_index,
            track,
        });
    };
    let id = u32::try_from(id).map_err(|_| HybridError::MissingBar {
        bar: bar_index,
        track,
    })?;
    let Some(target) = output.bars.iter_mut().find(|bar| bar.id == id) else {
        return Err(HybridError::MissingBar {
            bar: bar_index,
            track,
        });
    };
    target.clef = source.clef.clone();
    target.voices = source.voices.clone();
    Ok(())
}

fn position(tuning: &[i32], pitch: i32) -> Option<(u32, i32)> {
    tuning
        .iter()
        .enumerate()
        .filter_map(|(index, open)| Some((u32::try_from(index).ok()?, pitch - open)))
        .filter(|(_, fret)| (0..=24).contains(fret))
        .min_by_key(|(string, fret)| ((fret - 5).abs(), *string))
}

fn assign_fresh_event_ids(document: &mut Document) {
    let mut next_id = document
        .bars
        .iter()
        .flat_map(|bar| &bar.voices)
        .flat_map(|voice| &voice.beats)
        .flat_map(|beat| &beat.notes)
        .map(|note| note.id)
        .chain(
            document
                .bars
                .iter()
                .flat_map(|bar| &bar.voices)
                .flat_map(|voice| &voice.beats)
                .map(|beat| beat.id),
        )
        .chain(
            document
                .bars
                .iter()
                .flat_map(|bar| &bar.voices)
                .map(|voice| voice.id),
        )
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for bar in &mut document.bars {
        for voice in &mut bar.voices {
            voice.id = next_id;
            next_id = next_id.saturating_add(1);
            for beat in &mut voice.beats {
                beat.id = next_id;
                next_id = next_id.saturating_add(1);
                for note in &mut beat.notes {
                    note.id = next_id;
                    next_id = next_id.saturating_add(1);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "riff_hybrid_test.rs"]
mod tests;
