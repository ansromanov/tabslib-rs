//! Riff-locked rhythm-section generation and bass voice leading.

use super::drums::RiffOnset;

/// Bassline pattern selected by a document-level bass-generation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BassStrategy {
    /// Follow only source riff roots.
    RiffFollow,
    /// Repeat the active riff root on each eighth-note subdivision.
    PumpingEighths,
    /// Alternate riff roots with their perfect fifths.
    RootFifth,
    /// Add fifths and chromatic approach notes between riff roots.
    Walking,
    /// Sustain the first riff root through the remainder of the bar.
    Sustained,
}

/// Tuning selection for generated bass tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BassTuning<'a> {
    /// Derive a supported bass tuning from the selected source guitar tracks.
    Derived,
    /// Use caller-provided open-string MIDI pitches, ordered low to high.
    Custom(&'a [i32]),
}

/// Configuration shared by the bass workflow and stable facade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassGenerationConfig<'a> {
    /// Register and velocity policy for generated bass notes.
    pub profile: BassProfile,
    /// Rhythm and pitch pattern used to expand source roots.
    pub strategy: BassStrategy,
    /// Explicit tuning, or automatic derivation when absent.
    pub tuning: BassTuning<'a>,
    /// Highest playable fret considered by the position optimizer.
    pub max_fret: u32,
}

impl<'a> BassGenerationConfig<'a> {
    /// Uses riff-follow, source-derived tuning, and a 12-fret hand window.
    pub const fn new(profile: BassProfile) -> Self {
        Self {
            profile,
            strategy: BassStrategy::RiffFollow,
            tuning: BassTuning::Derived,
            max_fret: 12,
        }
    }
}

/// A playable string and fret selected for one bass event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassFretPosition {
    /// Zero-based string index in low-to-high tuning order.
    pub string: u32,
    /// Fret number on the selected string.
    pub fret: u32,
}

/// A source riff root represented by its lowest sounding pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiffRoot {
    pub step: u32,
    pub midi: i32,
    pub pitch_class: u8,
    pub accent: bool,
}

/// Extracts deterministic root candidates from analyzed riff onsets.
pub fn extract_riff_roots(onsets: &[RiffOnset]) -> Vec<RiffRoot> {
    onsets
        .iter()
        .filter_map(|onset| {
            let midi = onset.lowest_pitch?;
            Some(RiffRoot {
                step: onset.step,
                midi,
                pitch_class: midi.rem_euclid(12) as u8,
                accent: onset.accent,
            })
        })
        .collect()
}

/// External register and articulation policy for generated bass material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassProfile {
    pub min_midi: u8,
    pub max_midi: u8,
    pub max_leap: u8,
    pub accent_velocity: u8,
    pub normal_velocity: u8,
}

/// A generated bass event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassHit {
    pub step: u32,
    pub midi: u8,
    pub velocity: u8,
    pub accent: bool,
    pub fill: bool,
    /// Duration in sixteenth-note steps, used to preserve sustained notes.
    pub duration_steps: u32,
}

/// Relative bass event supplied by an external fill profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassFillStep {
    pub step: u32,
    pub midi: u8,
    pub velocity: u8,
}

fn candidates(root: u8, profile: BassProfile) -> impl Iterator<Item = u8> {
    (profile.min_midi..=profile.max_midi).filter(move |midi| midi % 12 == root)
}

fn nearest_candidate(root: u8, previous: Option<u8>, profile: BassProfile) -> Option<u8> {
    let mut all = candidates(root, profile).collect::<Vec<_>>();
    if all.is_empty() {
        return None;
    }
    all.sort_by_key(|midi| {
        previous
            .map(|last| u16::from((*midi).abs_diff(last)))
            .unwrap_or(u16::from((*midi).abs_diff(profile.min_midi)))
    });
    if let Some(last) = previous {
        if let Some(within_leap) = all
            .iter()
            .copied()
            .find(|midi| midi.abs_diff(last) <= u8::max(profile.max_leap, 1))
        {
            return Some(within_leap);
        }
    }
    all.first().copied()
}

/// Generates a root-locked bassline with nearest-register voice leading.
pub fn generate_riff_locked_bass(onsets: &[RiffOnset], profile: BassProfile) -> Vec<BassHit> {
    let roots = extract_riff_roots(onsets);
    let mut previous = None;
    roots
        .into_iter()
        .filter_map(|root| {
            let midi = nearest_candidate(root.pitch_class, previous, profile)?;
            previous = Some(midi);
            Some(BassHit {
                step: root.step,
                midi,
                velocity: if root.accent {
                    profile.accent_velocity
                } else {
                    profile.normal_velocity
                },
                accent: root.accent,
                fill: false,
                duration_steps: 1,
            })
        })
        .collect()
}

/// Generates bass events using one of the supported deterministic strategies.
pub fn generate_bass_pattern(
    onsets: &[RiffOnset],
    profile: BassProfile,
    strategy: BassStrategy,
    steps_per_bar: u32,
) -> Vec<BassHit> {
    generate_bass_pattern_with_pulses(onsets, profile, strategy, steps_per_bar, &[])
}

/// Generates bass events and accents events that land on grouped meter pulses.
pub fn generate_bass_pattern_with_pulses(
    onsets: &[RiffOnset],
    profile: BassProfile,
    strategy: BassStrategy,
    steps_per_bar: u32,
    pulse_steps: &[u32],
) -> Vec<BassHit> {
    let roots = extract_riff_roots(onsets);
    if roots.is_empty() {
        return Vec::new();
    }
    if strategy == BassStrategy::RiffFollow {
        let mut hits = generate_riff_locked_bass(onsets, profile);
        accent_pulses(&mut hits, pulse_steps, profile);
        return hits;
    }
    if strategy == BassStrategy::Sustained {
        let root = roots[0];
        let Some(midi) = nearest_candidate(root.pitch_class, None, profile) else {
            return Vec::new();
        };
        let mut hits = vec![BassHit {
            step: root.step,
            midi,
            velocity: if root.accent {
                profile.accent_velocity
            } else {
                profile.normal_velocity
            },
            accent: root.accent,
            fill: false,
            duration_steps: steps_per_bar.saturating_sub(root.step).max(1),
        }];
        accent_pulses(&mut hits, pulse_steps, profile);
        return hits;
    }

    let mut events = Vec::new();
    let mut previous = None;
    match strategy {
        BassStrategy::RiffFollow | BassStrategy::Sustained => {}
        BassStrategy::PumpingEighths => {
            let mut active_root = 0;
            for step in (0..steps_per_bar).step_by(2) {
                while roots
                    .get(active_root + 1)
                    .is_some_and(|next| next.step <= step)
                {
                    active_root += 1;
                }
                let root = roots[active_root];
                if step < root.step {
                    continue;
                }
                push_strategy_hit(
                    &mut events,
                    step,
                    root.pitch_class,
                    step == root.step,
                    profile,
                    &mut previous,
                );
            }
        }
        BassStrategy::RootFifth => {
            for (index, root) in roots.into_iter().enumerate() {
                let pitch_class = if index % 2 == 0 {
                    root.pitch_class
                } else {
                    (root.pitch_class + 7) % 12
                };
                push_strategy_hit(
                    &mut events,
                    root.step,
                    pitch_class,
                    root.accent,
                    profile,
                    &mut previous,
                );
            }
        }
        BassStrategy::Walking => {
            for (index, root) in roots.iter().copied().enumerate() {
                push_strategy_hit(
                    &mut events,
                    root.step,
                    root.pitch_class,
                    root.accent,
                    profile,
                    &mut previous,
                );
                let Some(next) = roots.get(index + 1).copied() else {
                    continue;
                };
                let fifth_step = root.step.saturating_add(2);
                if fifth_step < next.step {
                    push_strategy_hit(
                        &mut events,
                        fifth_step,
                        (root.pitch_class + 7) % 12,
                        false,
                        profile,
                        &mut previous,
                    );
                }
                let approach_step = next.step.saturating_sub(1);
                if approach_step > root.step && approach_step < steps_per_bar {
                    let approach_class = (next.pitch_class + 11) % 12;
                    if events.last().is_none_or(|hit| hit.step < approach_step) {
                        push_strategy_hit(
                            &mut events,
                            approach_step,
                            approach_class,
                            false,
                            profile,
                            &mut previous,
                        );
                    }
                }
            }
            events.sort_by_key(|hit| hit.step);
        }
    }
    accent_pulses(&mut events, pulse_steps, profile);
    events
}

fn accent_pulses(hits: &mut [BassHit], pulse_steps: &[u32], profile: BassProfile) {
    for hit in hits {
        if pulse_steps.contains(&hit.step) {
            hit.accent = true;
            hit.velocity = profile.accent_velocity;
        }
    }
}

fn push_strategy_hit(
    events: &mut Vec<BassHit>,
    step: u32,
    pitch_class: u8,
    accent: bool,
    profile: BassProfile,
    previous: &mut Option<u8>,
) {
    if events.last().is_some_and(|hit| hit.step == step) {
        return;
    }
    let Some(midi) = nearest_candidate(pitch_class, *previous, profile) else {
        return;
    };
    *previous = Some(midi);
    events.push(BassHit {
        step,
        midi,
        velocity: if accent {
            profile.accent_velocity
        } else {
            profile.normal_velocity
        },
        accent,
        fill: false,
        duration_steps: 1,
    });
}

fn preset_intervals(preset_name: &str) -> Option<Vec<i32>> {
    crate::pitch::TUNING_PRESETS
        .iter()
        .find(|preset| preset.name.eq_ignore_ascii_case(preset_name))
        .and_then(|preset| {
            let lowest = *preset.pitches.first()?;
            Some(preset.pitches.iter().map(|pitch| pitch - lowest).collect())
        })
}

/// Derives common bass tunings from recognized low-to-high guitar tunings.
pub fn derive_bass_tuning(guitar_tuning: &[i32]) -> Option<Vec<i32>> {
    if guitar_tuning.is_empty() || guitar_tuning.iter().any(|pitch| !(0..=127).contains(pitch)) {
        return None;
    }
    let lowest = *guitar_tuning.first()?;
    let bass_lowest = lowest.checked_sub(12)?;
    if bass_lowest < 0 {
        return None;
    }
    let guitar_intervals: Vec<i32> = guitar_tuning.iter().map(|pitch| pitch - lowest).collect();

    let standard_6 = preset_intervals("E Standard")?;
    let drop_6 = preset_intervals("Drop D")?;
    let standard_7 = preset_intervals("7-String Standard")?;
    let drop_7 = preset_intervals("7-String Drop A")?;

    let bass_intervals = if guitar_tuning.len() == 6 && guitar_intervals == standard_6 {
        preset_intervals("4-String Bass Standard")?
    } else if guitar_tuning.len() == 7 && guitar_intervals == standard_7 {
        preset_intervals("5-String Bass Standard")?
    } else if guitar_tuning.len() == 6 && guitar_intervals == drop_6 {
        if lowest <= 33 {
            preset_intervals("5-String Bass Drop A")?
        } else {
            preset_intervals("4-String Bass Drop D")?
        }
    } else if guitar_tuning.len() == 7
        && (guitar_intervals == drop_7 || guitar_intervals.starts_with(&drop_6))
    {
        preset_intervals("5-String Bass Drop A")?
    } else {
        return None;
    };

    let bass_tuning: Vec<i32> = bass_intervals
        .into_iter()
        .map(|interval| bass_lowest + interval)
        .collect();
    if bass_tuning.iter().all(|pitch| (0..=127).contains(pitch)) {
        Some(bass_tuning)
    } else {
        None
    }
}

/// Selects playable positions while minimizing string skips and hand shifts.
pub fn optimize_bass_positions(
    hits: &[BassHit],
    tuning: &[i32],
    max_fret: u32,
) -> Option<Vec<BassFretPosition>> {
    if hits.is_empty() {
        return Some(Vec::new());
    }
    let choices = hits
        .iter()
        .map(|hit| {
            tuning
                .iter()
                .enumerate()
                .filter_map(|(string, open)| {
                    let fret = i32::from(hit.midi) - *open;
                    let fret = u32::try_from(fret).ok()?;
                    if fret > max_fret {
                        return None;
                    }
                    Some(BassFretPosition {
                        string: u32::try_from(string).ok()?,
                        fret,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if choices.iter().any(Vec::is_empty) {
        return None;
    }

    let mut costs = choices[0]
        .iter()
        .map(|position| u64::from(position.fret) * 2 + u64::from(position.string))
        .collect::<Vec<_>>();
    let mut parents = Vec::with_capacity(choices.len());
    parents.push(vec![None; choices[0].len()]);
    for current in choices.iter().skip(1) {
        let mut next_costs = Vec::with_capacity(current.len());
        let mut next_parents = Vec::with_capacity(current.len());
        for position in current {
            let (parent, cost) = choices[parents.len() - 1]
                .iter()
                .enumerate()
                .map(|(index, prior)| {
                    let shift = prior.fret.abs_diff(position.fret);
                    let string_skip = prior.string.abs_diff(position.string);
                    (
                        index,
                        costs[index] + u64::from(shift) * 2 + u64::from(string_skip) * 3,
                    )
                })
                .min_by_key(|(_, cost)| *cost)?;
            next_costs.push(cost);
            next_parents.push(Some(parent));
        }
        costs = next_costs;
        parents.push(next_parents);
    }
    let mut selected = vec![0; choices.len()];
    selected[choices.len() - 1] = costs
        .iter()
        .enumerate()
        .min_by_key(|(_, cost)| *cost)
        .map(|(index, _)| index)?;
    for event in (1..choices.len()).rev() {
        selected[event - 1] = parents[event][selected[event]]?;
    }
    Some(
        choices
            .iter()
            .zip(selected)
            .map(|(options, index)| options[index])
            .collect(),
    )
}

/// Inserts explicit bass-fill steps and marks them as fills.
pub fn apply_bass_fill(hits: &mut Vec<BassHit>, fill: &[BassFillStep], start_step: u32) -> usize {
    let before = hits.len();
    hits.extend(fill.iter().map(|step| BassHit {
        step: start_step.saturating_add(step.step),
        midi: step.midi,
        velocity: step.velocity,
        accent: false,
        fill: true,
        duration_steps: 1,
    }));
    hits.len() - before
}

/// Measures the distance between adjacent bass events in semitones.
pub fn voice_leading_leaps(hits: &[BassHit]) -> Vec<u8> {
    hits.windows(2)
        .map(|pair| pair[0].midi.abs_diff(pair[1].midi))
        .collect()
}

/// Riff-lock and density metrics for a generated rhythm section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RhythmCompatibility {
    pub riff_onsets: usize,
    pub bass_onsets: usize,
    pub locked_onsets: usize,
    pub riff_coverage: f64,
    pub bass_lock_ratio: f64,
    pub score: f64,
}

/// Scores whether bass events remain aligned with the source riff.
pub fn score_rhythm_compatibility(
    riff_onsets: &[RiffOnset],
    bass_hits: &[BassHit],
) -> RhythmCompatibility {
    let riff_steps = riff_onsets
        .iter()
        .map(|onset| onset.step)
        .collect::<std::collections::BTreeSet<_>>();
    let bass_steps = bass_hits
        .iter()
        .filter(|hit| !hit.fill)
        .map(|hit| hit.step)
        .collect::<std::collections::BTreeSet<_>>();
    let locked = bass_steps.intersection(&riff_steps).count();
    let riff_coverage = if riff_steps.is_empty() {
        1.0
    } else {
        locked as f64 / riff_steps.len() as f64
    };
    let bass_lock_ratio = if bass_steps.is_empty() {
        1.0
    } else {
        locked as f64 / bass_steps.len() as f64
    };
    RhythmCompatibility {
        riff_onsets: riff_steps.len(),
        bass_onsets: bass_steps.len(),
        locked_onsets: locked,
        riff_coverage,
        bass_lock_ratio,
        score: (riff_coverage + bass_lock_ratio) / 2.0,
    }
}
