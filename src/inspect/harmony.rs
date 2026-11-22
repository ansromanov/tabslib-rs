//! Read-only key, chord and vertical-interval analysis of sounding score notes.
//!
//! The module owns [`determine_key`], [`analyze_bar_chords`] and
//! [`frictions_between`]. It reads only the document model and makes no
//! assumptions about style or generation.

use super::{bar_for, Fraction};
use crate::model::{Document, KeySignature};

/// A diatonic mode, harmonic minor, or Phrygian dominant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MusicalMode {
    /// Major (Ionian).
    Major,
    /// Natural minor (Aeolian).
    NaturalMinor,
    /// Dorian.
    Dorian,
    /// Phrygian.
    Phrygian,
    /// Lydian.
    Lydian,
    /// Mixolydian.
    Mixolydian,
    /// Locrian.
    Locrian,
    /// Harmonic minor.
    HarmonicMinor,
    /// Phrygian dominant: the fifth mode of harmonic minor, with a flat second
    /// and a major third.
    PhrygianDominant,
}

impl MusicalMode {
    fn intervals(self) -> [i32; 7] {
        match self {
            Self::Major => [0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => [0, 2, 3, 5, 7, 8, 10],
            Self::Dorian => [0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => [0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => [0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => [0, 1, 3, 5, 6, 8, 10],
            Self::HarmonicMinor => [0, 2, 3, 5, 7, 8, 11],
            Self::PhrygianDominant => [0, 1, 4, 5, 7, 8, 10],
        }
    }

    /// Pitch classes of this mode rooted on `tonic`, ascending from the tonic.
    ///
    /// ```
    /// use tabslib::inspect::MusicalMode;
    ///
    /// assert_eq!(MusicalMode::Phrygian.pitch_classes(4), [4, 5, 7, 9, 11, 0, 2]);
    /// ```
    pub fn pitch_classes(self, tonic: i8) -> [i32; 7] {
        let mut classes = self.intervals();
        for class in &mut classes {
            *class = (*class + i32::from(tonic)).rem_euclid(12);
        }
        classes
    }

    /// Whether a key signature for this mode is written as minor.
    ///
    /// Phrygian dominant counts as minor: its sixth and seventh are flat, and
    /// it shares more pitch classes with the natural minor on its tonic than
    /// with the major.
    ///
    /// ```
    /// use tabslib::inspect::MusicalMode;
    ///
    /// assert!(MusicalMode::Dorian.is_minor());
    /// assert!(!MusicalMode::Mixolydian.is_minor());
    /// ```
    pub fn is_minor(self) -> bool {
        matches!(
            self,
            Self::NaturalMinor
                | Self::Dorian
                | Self::Phrygian
                | Self::Locrian
                | Self::HarmonicMinor
                | Self::PhrygianDominant
        )
    }
}

/// A material-based key and mode estimate with supporting evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEstimate {
    /// Tonic pitch class and a major/minor compatibility flag.
    pub key: KeySignature,
    /// Best-fitting scale type.
    pub mode: MusicalMode,
    /// Winning candidate score divided by all candidate scores.
    pub confidence: Fraction,
    /// Separation between the top candidate and runner-up, relative to the top score.
    pub confidence_margin: Fraction,
    /// Number of chord-root observations supporting the tonic.
    pub root_support: usize,
    /// Number of pitched notes used by the estimate.
    pub pitched_notes: usize,
}

/// Quality of a detected triad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordQuality {
    /// Root, major third and perfect fifth.
    Major,
    /// Root, minor third and perfect fifth.
    Minor,
    /// Root, minor third and diminished fifth.
    Diminished,
    /// Root, major third and augmented fifth.
    Augmented,
}

/// A diatonic chord detected from the sounding notes in one master bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarChord {
    /// Zero-based master-bar index.
    pub bar_index: usize,
    /// Chord root as a MIDI pitch class (C is 0).
    pub root: i32,
    /// Detected triad quality.
    pub quality: ChordQuality,
    /// Roman-numeral degree relative to the estimated or supplied key.
    pub numeral: Option<String>,
    /// Number of sounding notes matching the triad tones.
    pub supporting_notes: usize,
}

/// Pitch-pair friction observed while notes on two tracks overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrictionEvent {
    /// Zero-based master-bar index.
    pub bar_index: usize,
    /// Absolute interval in semitones, reduced to one octave.
    pub interval_semitones: i32,
    /// Exact written duration for which this note pair overlaps.
    pub overlap: Fraction,
    /// Dissonance weight: 0 for consonant intervals, up to 4 for a minor second.
    pub weight: u8,
}

/// Aggregate interval-friction measurements for a pair of tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrictionAnalysis {
    /// First track index supplied to [`frictions_between`].
    pub first_track: usize,
    /// Second track index supplied to [`frictions_between`].
    pub second_track: usize,
    /// Number of note pairs with non-zero overlap.
    pub compared_pairs: usize,
    /// Number of overlapping pairs assigned a non-zero friction weight.
    pub friction_pairs: usize,
    /// Sum of pairwise overlap durations.
    pub total_overlap: Fraction,
    /// Duration-weighted friction, with interval weight scaled from zero to four.
    pub weighted_friction: Fraction,
    /// Weighted friction normalized to the range zero through one.
    pub friction_ratio: Fraction,
    /// Per-note-pair interval evidence.
    pub events: Vec<FrictionEvent>,
}

/// Returns pitch-class roots whose root and fifth avoid semitone friction with
/// every supplied melody pitch class.
pub fn safe_roots_for_bar(melody_pitch_classes: &[i32]) -> Vec<i32> {
    (0..12)
        .filter(|root| {
            [*root, (*root + 7) % 12].iter().all(|candidate| {
                melody_pitch_classes.iter().all(|pitch| {
                    let interval = (candidate - pitch).rem_euclid(12);
                    interval != 1 && interval != 11
                })
            })
        })
        .collect()
}

/// One contiguous chord run in a section-scoped progression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordRange {
    /// First master-bar index in the run.
    pub start_bar: usize,
    /// Last master-bar index in the run, inclusive.
    pub end_bar: usize,
    /// Chord observed in the run.
    pub chord: BarChord,
}

/// Chord progression limited to one contiguous marked section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionChordProgression {
    /// Section label, or `None` for unmarked material.
    pub label: Option<String>,
    /// First master-bar index in the section.
    pub start_bar: usize,
    /// Last master-bar index in the section, inclusive.
    pub end_bar: usize,
    /// Contiguous detected chord ranges.
    pub ranges: Vec<ChordRange>,
}

/// A caller-supplied inclusive master-bar range for section harmony analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmonySectionRange {
    /// Optional section label for the report.
    pub label: Option<String>,
    /// First master-bar index in the range.
    pub start_bar: usize,
    /// Last master-bar index in the range, inclusive.
    pub end_bar: usize,
}

/// Beat-resolution harmonic rhythm and root-motion measurements for a section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHarmony {
    /// Optional section label.
    pub label: Option<String>,
    /// First master-bar index in the section.
    pub start_bar: usize,
    /// Last master-bar index in the section, inclusive.
    pub end_bar: usize,
    /// Number of bars in the section.
    pub bars: usize,
    /// Number of detected chord changes between successive sounding beats.
    pub chord_changes: usize,
    /// Chord changes per bar, represented exactly.
    pub harmonic_rhythm: Fraction,
    /// Mean shortest root interval between successive detected chords.
    pub mean_root_motion: Fraction,
    /// Largest shortest root interval between successive detected chords.
    pub max_root_motion: u8,
    /// Most frequent chord root as a pitch class, relative to the detected tonic.
    pub most_frequent_root: Option<i32>,
    /// Most frequent root's semitone offset from the detected tonic.
    pub most_frequent_root_relative: Option<i32>,
}

/// Reports harmonic rhythm and root motion for marker-defined sections.
pub fn section_harmony(doc: &Document) -> Vec<SectionHarmony> {
    let ranges = marker_ranges(doc);
    analyze_section_harmony(doc, &ranges)
}

/// Reports harmonic rhythm and root motion for caller-supplied sections.
pub fn analyze_section_harmony(
    doc: &Document,
    ranges: &[HarmonySectionRange],
) -> Vec<SectionHarmony> {
    let key = determine_key(doc);
    let context = key.map(|estimate| (estimate.key.tonic, estimate.mode));
    ranges
        .iter()
        .map(|range| analyze_harmony_range(doc, range, context))
        .collect()
}

fn marker_ranges(doc: &Document) -> Vec<HarmonySectionRange> {
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < doc.master_bars.len() {
        let label = doc.master_bars[start].section.clone();
        let end = (start + 1..doc.master_bars.len())
            .find(|index| doc.master_bars[*index].section.is_some())
            .unwrap_or(doc.master_bars.len())
            .saturating_sub(1);
        ranges.push(HarmonySectionRange {
            label,
            start_bar: start,
            end_bar: end,
        });
        start = end.saturating_add(1);
    }
    ranges
}

fn analyze_harmony_range(
    doc: &Document,
    range: &HarmonySectionRange,
    key: Option<(i8, MusicalMode)>,
) -> SectionHarmony {
    let start = range.start_bar.min(doc.master_bars.len());
    let end = range.end_bar.min(doc.master_bars.len().saturating_sub(1));
    let bars = if start <= end { end - start + 1 } else { 0 };
    let context = key;
    let mut previous: Option<BarChord> = None;
    let mut chord_changes = 0;
    let mut root_motion_sum = 0u64;
    let mut max_root_motion = 0u8;
    let mut root_counts = [0usize; 12];
    if bars > 0 {
        for bar_index in start..=end {
            let beat_count = beat_count(doc, bar_index);
            for beat_index in 0..beat_count {
                let Some(chord) = analyze_one_beat(doc, bar_index, beat_index, context) else {
                    continue;
                };
                root_counts[chord.root as usize] += 1;
                if let Some(ref previous_chord) = previous {
                    if previous_chord.root != chord.root || previous_chord.quality != chord.quality
                    {
                        let motion = root_interval(previous_chord.root, chord.root);
                        chord_changes += 1;
                        root_motion_sum += u64::from(motion);
                        max_root_motion = max_root_motion.max(motion);
                        previous = Some(chord);
                    }
                } else {
                    previous = Some(chord);
                }
            }
        }
    }
    let most_frequent_root = root_counts
        .iter()
        .enumerate()
        .max_by_key(|(root, count)| (**count, usize::MAX - *root))
        .and_then(|(root, count)| (*count > 0).then_some(root as i32));
    let most_frequent_root_relative = most_frequent_root
        .zip(key)
        .map(|(root, (tonic, _))| (root - i32::from(tonic)).rem_euclid(12));
    SectionHarmony {
        label: range.label.clone(),
        start_bar: range.start_bar,
        end_bar: range.end_bar,
        bars,
        chord_changes,
        harmonic_rhythm: Fraction::new(chord_changes as u64, bars.max(1) as u64),
        mean_root_motion: Fraction::new(root_motion_sum, chord_changes.max(1) as u64),
        max_root_motion,
        most_frequent_root,
        most_frequent_root_relative,
    }
}

fn beat_count(doc: &Document, bar_index: usize) -> usize {
    (0..doc.tracks.len())
        .filter_map(|track_index| bar_for(doc, bar_index, track_index))
        .flat_map(|bar| bar.voices.first())
        .map(|voice| voice.beats.len())
        .max()
        .unwrap_or(0)
}

fn root_interval(previous: i32, current: i32) -> u8 {
    let distance = (current - previous).rem_euclid(12);
    distance.min(12 - distance) as u8
}

/// Groups detected bar chords by section and collapses adjacent equal chords.
pub fn progression_for_sections(doc: &Document) -> Vec<SectionChordProgression> {
    let chords = analyze_bar_chords(doc);
    let mut sections = Vec::new();
    let mut start = 0;
    while start < doc.master_bars.len() {
        let label = doc.master_bars[start].section.clone();
        let end = (start + 1..doc.master_bars.len())
            .find(|index| doc.master_bars[*index].section.is_some())
            .unwrap_or(doc.master_bars.len())
            .saturating_sub(1);
        let mut ranges = Vec::new();
        for index in start..=end {
            let Some(chord) = chords.get(index).and_then(Option::as_ref) else {
                continue;
            };
            let same = ranges.last().is_some_and(|previous: &ChordRange| {
                previous.chord.root == chord.root && previous.chord.quality == chord.quality
            });
            if same {
                if let Some(previous) = ranges.last_mut() {
                    previous.end_bar = index;
                }
            } else {
                ranges.push(ChordRange {
                    start_bar: index,
                    end_bar: index,
                    chord: chord.clone(),
                });
            }
        }
        sections.push(SectionChordProgression {
            label,
            start_bar: start,
            end_bar: end,
            ranges,
        });
        start = end.saturating_add(1);
    }
    sections
}

const MODES: [MusicalMode; 9] = [
    MusicalMode::Major,
    MusicalMode::NaturalMinor,
    MusicalMode::Dorian,
    MusicalMode::Phrygian,
    MusicalMode::Lydian,
    MusicalMode::Mixolydian,
    MusicalMode::Locrian,
    MusicalMode::HarmonicMinor,
    MusicalMode::PhrygianDominant,
];

/// Estimates tonic and mode from pitch-class distribution and chord-root evidence.
///
/// The result distinguishes all seven church modes, harmonic minor, and
/// Phrygian dominant. Its
/// confidence is the winning scale score relative to all candidate scores;
/// `confidence_margin` reports separation from the runner-up. Percussion is
/// excluded. If no perfect-fifth root evidence exists, pitch counts provide
/// the root fallback.
pub fn determine_key(doc: &Document) -> Option<KeyEstimate> {
    let mut pitch_classes = [0usize; 12];
    let mut root_support = [0usize; 12];
    let mut pitched_notes = 0;
    for track_index in 0..doc.tracks.len() {
        if is_percussion(doc, track_index) {
            continue;
        }
        for bar_index in 0..doc.master_bars.len() {
            let Some(bar) = bar_for(doc, bar_index, track_index) else {
                continue;
            };
            for voice in &bar.voices {
                for beat in &voice.beats {
                    let mut present = [false; 12];
                    for note in &beat.notes {
                        let Some(midi) = note.midi else { continue };
                        let pitch_class = midi.rem_euclid(12) as usize;
                        present[pitch_class] = true;
                        pitch_classes[pitch_class] += 1;
                        pitched_notes += 1;
                    }
                    for root in 0..12 {
                        if present[root] && present[(root + 7) % 12] {
                            root_support[root] += 1;
                        }
                    }
                }
            }
        }
    }
    if pitched_notes == 0 {
        return None;
    }
    if root_support.iter().all(|support| *support == 0) {
        root_support = pitch_classes;
    }

    let mut candidates = Vec::with_capacity(12 * MODES.len());
    for (tonic, tonic_root_support) in root_support.iter().copied().enumerate() {
        for mode in MODES {
            let scale = mode.intervals();
            let mut score = 0usize;
            for (pitch_class, count) in pitch_classes.iter().copied().enumerate() {
                let relative = (pitch_class + 12 - tonic) % 12;
                if scale.contains(&(relative as i32)) {
                    score += count * 2;
                }
                if pitch_class == tonic {
                    score += count * 2;
                }
            }
            score += tonic_root_support * 4;
            candidates.push((score, tonic, mode));
        }
    }
    candidates.sort_by_key(|candidate| {
        (
            std::cmp::Reverse(candidate.0),
            std::cmp::Reverse(root_support[candidate.1]),
            mode_preference(candidate.2),
            candidate.1,
        )
    });
    let &(score, tonic, mode) = candidates.first()?;
    let runner_up = candidates.get(1).map_or(0, |candidate| candidate.0);
    let total_score = candidates
        .iter()
        .map(|candidate| candidate.0)
        .sum::<usize>();
    Some(KeyEstimate {
        key: KeySignature {
            tonic: tonic as i8,
            minor: mode.is_minor(),
        },
        mode,
        confidence: Fraction::new(score as u64, total_score.max(1) as u64),
        confidence_margin: Fraction::new(
            score.saturating_sub(runner_up) as u64,
            score.max(1) as u64,
        ),
        root_support: root_support[tonic],
        pitched_notes,
    })
}

/// Detects the best-supported complete triad in each master bar.
///
/// Bars with fewer than three distinct chord tones produce `None`. Key context
/// defaults to [`determine_key`], while `numeral` stays absent when no key can
/// be estimated.
pub fn analyze_bar_chords(doc: &Document) -> Vec<Option<BarChord>> {
    let key = determine_key(doc).map(|estimate| (estimate.key.tonic, estimate.mode));
    analyze_bar_chords_with_context(doc, key)
}

/// Detects per-bar triads and Roman numerals against an explicit key and mode.
pub fn analyze_bar_chords_in_key(
    doc: &Document,
    tonic: i8,
    mode: MusicalMode,
) -> Vec<Option<BarChord>> {
    analyze_bar_chords_with_context(doc, Some((tonic.rem_euclid(12), mode)))
}

fn analyze_bar_chords_with_context(
    doc: &Document,
    key: Option<(i8, MusicalMode)>,
) -> Vec<Option<BarChord>> {
    (0..doc.master_bars.len())
        .map(|bar_index| analyze_one_bar(doc, bar_index, key))
        .collect()
}

fn analyze_one_bar(
    doc: &Document,
    bar_index: usize,
    key: Option<(i8, MusicalMode)>,
) -> Option<BarChord> {
    let mut counts = [0usize; 12];
    for track_index in 0..doc.tracks.len() {
        if is_percussion(doc, track_index) {
            continue;
        }
        let Some(bar) = bar_for(doc, bar_index, track_index) else {
            continue;
        };
        for note in bar
            .voices
            .iter()
            .flat_map(|voice| &voice.beats)
            .flat_map(|beat| &beat.notes)
        {
            if let Some(midi) = note.midi {
                counts[midi.rem_euclid(12) as usize] += 1;
            }
        }
    }
    detect_chord(counts, bar_index, key)
}

fn analyze_one_beat(
    doc: &Document,
    bar_index: usize,
    beat_index: usize,
    key: Option<(i8, MusicalMode)>,
) -> Option<BarChord> {
    let mut counts = [0usize; 12];
    for track_index in 0..doc.tracks.len() {
        if is_percussion(doc, track_index) {
            continue;
        }
        let Some(bar) = bar_for(doc, bar_index, track_index) else {
            continue;
        };
        for voice in &bar.voices {
            let Some(beat) = voice.beats.get(beat_index) else {
                continue;
            };
            for note in &beat.notes {
                if let Some(midi) = note.midi {
                    counts[midi.rem_euclid(12) as usize] += 1;
                }
            }
        }
    }
    detect_chord(counts, bar_index, key)
}

fn detect_chord(
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

fn intervals_for(quality: ChordQuality) -> [usize; 3] {
    match quality {
        ChordQuality::Major => [0, 4, 7],
        ChordQuality::Minor => [0, 3, 7],
        ChordQuality::Diminished => [0, 3, 6],
        ChordQuality::Augmented => [0, 4, 8],
    }
}

fn is_scale_root(root: i32, tonic: i8, mode: MusicalMode) -> bool {
    mode.intervals()
        .contains(&((root - i32::from(tonic)).rem_euclid(12)))
}

fn roman_numeral(root: i32, quality: ChordQuality, tonic: i8, mode: MusicalMode) -> String {
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

/// Measures dissonant pitch pairs whose written durations overlap across two tracks.
///
/// Returns `None` for invalid or identical track indices. The analysis compares
/// every pitched note pair across both tracks in each bar and weights each
/// interval by its exact overlap duration.
pub fn frictions_between(
    doc: &Document,
    first_track: usize,
    second_track: usize,
) -> Option<FrictionAnalysis> {
    if first_track == second_track
        || first_track >= doc.tracks.len()
        || second_track >= doc.tracks.len()
        || is_percussion(doc, first_track)
        || is_percussion(doc, second_track)
    {
        return None;
    }
    let mut events = Vec::new();
    let mut total_overlap = Fraction::new(0, 1);
    let mut weighted_friction = Fraction::new(0, 1);
    let mut friction_pairs = 0;
    for bar_index in 0..doc.master_bars.len() {
        let first = track_events(doc, bar_index, first_track);
        let second = track_events(doc, bar_index, second_track);
        for left in &first {
            for right in &second {
                let Some(overlap) = overlap(left.onset, left.duration, right.onset, right.duration)
                else {
                    continue;
                };
                let interval = (left.midi - right.midi).abs().rem_euclid(12);
                let weight = interval_weight(interval);
                total_overlap = total_overlap.plus(overlap);
                if weight > 0 {
                    friction_pairs += 1;
                    weighted_friction =
                        weighted_friction.plus(scale_fraction(overlap, weight as u64));
                }
                events.push(FrictionEvent {
                    bar_index,
                    interval_semitones: interval,
                    overlap,
                    weight,
                });
            }
        }
    }
    let friction_ratio = if total_overlap == Fraction::new(0, 1) {
        Fraction::new(0, 1)
    } else {
        let scaled_total = scale_fraction(total_overlap, 4);
        fraction_ratio(weighted_friction, scaled_total)
    };
    Some(FrictionAnalysis {
        first_track,
        second_track,
        compared_pairs: events.len(),
        friction_pairs,
        total_overlap,
        weighted_friction,
        friction_ratio,
        events,
    })
}

#[derive(Clone, Copy)]
struct TimedNote {
    midi: i32,
    onset: Fraction,
    duration: Fraction,
}

fn track_events(doc: &Document, bar_index: usize, track_index: usize) -> Vec<TimedNote> {
    let Some(bar) = bar_for(doc, bar_index, track_index) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for voice in &bar.voices {
        let mut onset = Fraction::new(0, 1);
        for beat in &voice.beats {
            let duration = super::rhythm_duration(beat.rhythm);
            events.extend(beat.notes.iter().filter_map(|note| {
                note.midi.map(|midi| TimedNote {
                    midi,
                    onset,
                    duration,
                })
            }));
            onset = onset.plus(duration);
        }
    }
    events
}

fn overlap(
    a_onset: Fraction,
    a_duration: Fraction,
    b_onset: Fraction,
    b_duration: Fraction,
) -> Option<Fraction> {
    let start = a_onset.max(b_onset);
    let end = a_onset.plus(a_duration).min(b_onset.plus(b_duration));
    if end <= start {
        return None;
    }
    let numerator = end.numerator * start.denominator - start.numerator * end.denominator;
    let denominator = end.denominator * start.denominator;
    Some(Fraction::new(numerator, denominator))
}

fn scale_fraction(value: Fraction, factor: u64) -> Fraction {
    Fraction::new(value.numerator * factor, value.denominator)
}

fn fraction_ratio(numerator: Fraction, denominator: Fraction) -> Fraction {
    Fraction::new(
        numerator.numerator * denominator.denominator,
        numerator.denominator * denominator.numerator,
    )
}

fn interval_weight(interval: i32) -> u8 {
    match interval {
        1 | 11 => 4,
        6 => 3,
        2 | 10 => 2,
        5 | 7 => 0,
        0 | 3 | 4 | 8 | 9 => 0,
        _ => 1,
    }
}

fn is_percussion(doc: &Document, track_index: usize) -> bool {
    doc.tracks.get(track_index).is_some_and(|track| {
        track.tuning.is_empty() || track.tuning.iter().all(|pitch| *pitch == 0)
    })
}

fn mode_preference(mode: MusicalMode) -> usize {
    MODES
        .iter()
        .position(|candidate| *candidate == mode)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "harmony_test.rs"]
mod tests;
