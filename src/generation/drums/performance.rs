//! Generic drum hit generation and performance layers.
//!
//! The profile is deliberately data-only: callers supply probabilities and
//! voices from an external profile, while this module supplies deterministic
//! mechanics shared by all domains.

use std::ops::Range;

use super::{voices, DrumHit, DrumOrnament, DrumPattern};

/// External, style-neutral controls for turning riff onsets into drum hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumProfile {
    /// General MIDI note number used for kick hits.
    pub kick_voice: u8,
    /// General MIDI note number used for snare hits.
    pub snare_voice: u8,
    /// General MIDI note number used for cymbal hits.
    pub cymbal_voice: u8,
    /// Percentage chance (0-100) that an onset also gets a kick; 100 or more always applies.
    pub kick_probability: u8,
    /// Percentage chance (0-100) that an onset gets a snare; accented onsets always get one.
    pub snare_probability: u8,
    /// Percentage chance (0-100) that an onset also gets a cymbal; 100 or more always applies.
    pub cymbal_probability: u8,
}

/// A fill supplied by a profile or caller, relative to its insertion step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillPattern {
    /// Length of the fill in steps; hits at or beyond it are dropped, and zero disables the fill.
    pub length: u32,
    /// Fill hits with steps relative to the insertion step.
    pub hits: Vec<DrumHit>,
}

/// Bounds for deterministic performance variation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanizationConfig {
    /// Maximum random velocity change applied in either direction (MIDI velocity units).
    pub velocity_range: u8,
    /// Maximum random timing change applied in either direction, in `DrumHit::timing_offset` units.
    pub timing_range: i16,
}

/// A caller-selected flam or drag to apply to a hit at an absolute step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArticulationRequest {
    /// General MIDI drum voice to ornament.
    pub midi: u8,
    /// Absolute subdivision step of the principal hit.
    pub step: u32,
    /// Grace-note shape to add.
    pub ornament: DrumOrnament,
}

#[derive(Debug, Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 7;
        value ^= value >> 9;
        value ^= value << 8;
        self.0 = value;
        value
    }

    fn chance(&mut self, percentage: u8) -> bool {
        percentage >= 100 || (percentage > 0 && self.next() % 100 < u64::from(percentage))
    }

    fn signed(&mut self, magnitude: i16) -> i16 {
        if magnitude == 0 {
            return 0;
        }
        let span = u64::from(magnitude.unsigned_abs()) * 2 + 1;
        (self.next() % span) as i16 - magnitude
    }
}

/// Generates a deterministic, profile-driven hit stream from source onsets.
pub fn generate_seeded_hits(
    pattern: &DrumPattern,
    profile: &DrumProfile,
    seed: u64,
) -> Vec<DrumHit> {
    if pattern.steps_per_bar == 0 {
        return Vec::new();
    }
    let mut rng = Rng::new(seed);
    let mut hits = Vec::new();
    for (bar_index, bar) in pattern.bars.iter().enumerate() {
        let bar_offset = u32::try_from(bar_index)
            .unwrap_or(u32::MAX)
            .saturating_mul(pattern.steps_per_bar);
        for (step, source) in bar.iter().enumerate() {
            if !source.onset {
                continue;
            }
            let step = bar_offset.saturating_add(u32::try_from(step).unwrap_or(u32::MAX));
            if rng.chance(profile.kick_probability) {
                hits.push(DrumHit::new(profile.kick_voice, step));
            }
            if source.accent || rng.chance(profile.snare_probability) {
                let mut hit = DrumHit::new(profile.snare_voice, step);
                hit.accent = source.accent;
                hit.ghost = source.dead;
                hits.push(hit);
            }
            if rng.chance(profile.cymbal_probability) {
                hits.push(DrumHit::new(profile.cymbal_voice, step));
            }
        }
    }
    hits
}

/// Adds seeded snare hits at grouped backbeats, accenting an existing snare
/// when one already occupies the step.
pub fn add_seeded_backbeats(
    hits: &mut Vec<DrumHit>,
    backbeat_steps: &[u32],
    profile: &DrumProfile,
    seed: u64,
) {
    let mut rng = Rng::new(seed ^ 0xA076_1D64_78BD_642F);
    for &step in backbeat_steps {
        if let Some(hit) = hits
            .iter_mut()
            .find(|hit| hit.midi == profile.snare_voice && hit.step == step)
        {
            hit.accent = true;
            continue;
        }
        if rng.chance(profile.snare_probability) {
            let mut hit = DrumHit::new(profile.snare_voice, step);
            hit.accent = true;
            hits.push(hit);
        }
    }
}

/// Inserts an explicit fill, optionally replacing the target step range.
pub fn apply_fill(
    hits: &mut Vec<DrumHit>,
    fill: &FillPattern,
    start_step: u32,
    replace_steps: Option<Range<u32>>,
) -> usize {
    if fill.length == 0 {
        return 0;
    }
    if let Some(range) = replace_steps {
        hits.retain(|hit| !range.contains(&hit.step));
    }
    let before = hits.len();
    hits.extend(fill.hits.iter().filter_map(|hit| {
        if hit.step >= fill.length {
            return None;
        }
        let mut inserted = hit.clone();
        inserted.step = start_step.saturating_add(hit.step);
        Some(inserted)
    }));
    hits.len() - before
}

/// Adds a transition marker once at a boundary step.
pub fn add_transition_marker(hits: &mut Vec<DrumHit>, step: u32, voice: u8) -> bool {
    if hits.iter().any(|hit| hit.step == step && hit.midi == voice) {
        return false;
    }
    let mut marker = DrumHit::new(voice, step);
    marker.accent = true;
    hits.push(marker);
    true
}

/// Applies seeded velocity and timing variation without changing hit identity.
pub fn humanize_hits(hits: &mut [DrumHit], seed: u64, config: HumanizationConfig) {
    let mut rng = Rng::new(seed);
    for hit in hits {
        let variation = rng.signed(i16::from(config.velocity_range));
        hit.velocity = i16::from(hit.velocity)
            .saturating_add(variation)
            .clamp(1, 127) as u8;
        hit.timing_offset = hit
            .timing_offset
            .saturating_add(rng.signed(config.timing_range));
    }
}

/// Pairs each open hi-hat with its earliest later closed or pedal hi-hat hit.
///
/// The open hit records the absolute step where its sound is cut off. Hits at
/// the same step do not count as later events. Returns the number of pairings
/// that were added or moved earlier by this call.
pub fn pair_hihat_chokes(hits: &mut [DrumHit]) -> usize {
    let closers = hits
        .iter()
        .filter(|hit| matches!(hit.midi, voices::CLOSED_HI_HAT | voices::PEDAL_HI_HAT))
        .map(|hit| hit.step)
        .collect::<Vec<_>>();
    let mut paired = 0;
    for open_hat in hits
        .iter_mut()
        .filter(|hit| hit.midi == voices::OPEN_HI_HAT)
    {
        if let Some(step) = closers
            .iter()
            .copied()
            .filter(|step| *step > open_hat.step)
            .min()
        {
            if open_hat.choked_at_step != Some(step) {
                paired += 1;
            }
            open_hat.choked_at_step = Some(step);
        }
    }
    paired
}

/// Adds deterministic grace hits for caller-selected flam and drag requests.
///
/// Each request matches the first undecorated, non-ghost hit with the supplied
/// voice and step. Flams add one quiet grace hit; drags add two. Grace hits
/// share the principal step and use negative timing offsets so the event order
/// remains representable on the existing subdivision grid.
pub fn apply_articulations(
    hits: &mut Vec<DrumHit>,
    requests: &[ArticulationRequest],
    grace_spacing: i16,
) -> usize {
    let spacing = grace_spacing.max(1);
    let mut applied = 0;
    for request in requests {
        let Some(index) = hits.iter().position(|hit| {
            hit.midi == request.midi
                && hit.step == request.step
                && !hit.ghost
                && hit.ornament.is_none()
        }) else {
            continue;
        };
        let principal = hits[index].clone();
        hits[index].ornament = Some(request.ornament);
        let grace_count: u8 = match request.ornament {
            DrumOrnament::Flam => 1,
            DrumOrnament::Drag => 2,
        };
        for grace_index in (1..=grace_count).rev() {
            let mut grace = principal.clone();
            grace.ghost = true;
            grace.accent = false;
            grace.ornament = None;
            grace.choked_at_step = None;
            let distance = spacing.saturating_mul(i16::from(grace_index));
            grace.timing_offset = grace.timing_offset.saturating_sub(distance);
            grace.velocity = grace.velocity.saturating_sub(20 * grace_index).max(1);
            hits.push(grace);
        }
        applied += 1;
    }
    hits.sort_by_key(|hit| (hit.step, hit.timing_offset));
    applied
}

/// Contextual configuration driving the drum performance pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceLayerConfig {
    /// Score tempo in BPM.
    pub tempo: u32,
    /// Backbeat subdivision steps where snares are unghosted when intensity >= 2.
    pub backbeat_steps: Vec<u32>,
    /// Section intensity rating from 0 to 3 (values >= 2 unghost backbeats).
    pub intensity: u8,
    /// Whether ghost notes are retained (defaults to true).
    pub ghost_notes: bool,
    /// Whether accents are retained (defaults to true).
    pub accents: bool,
    /// Riff stab steps to catch with kick and cymbal.
    pub stab_steps: Vec<u32>,
    /// Whether a fill was applied in this context (used by limb solving).
    pub fill_applied: bool,
    /// Caller-selected articulation requests (flams, drags).
    pub articulations: Vec<ArticulationRequest>,
    /// Optional humanization config: (seed, config).
    pub humanization: Option<(u64, HumanizationConfig)>,
    /// Base velocity for shaping dynamics (defaults to 100).
    pub base_velocity: u8,
}

impl Default for PerformanceLayerConfig {
    fn default() -> Self {
        Self {
            tempo: 120,
            backbeat_steps: Vec::new(),
            intensity: 1,
            ghost_notes: true,
            accents: true,
            stab_steps: Vec::new(),
            fill_applied: false,
            articulations: Vec::new(),
            humanization: None,
            base_velocity: 100,
        }
    }
}

impl PerformanceLayerConfig {
    /// Creates a configuration with defaults for the given tempo.
    pub fn new(tempo: u32) -> Self {
        Self {
            tempo,
            ..Default::default()
        }
    }
}

fn accent_or_insert_hit(hits: &mut Vec<DrumHit>, midi: u8, step: u32) {
    if let Some(hit) = hits
        .iter_mut()
        .find(|hit| hit.midi == midi && hit.step == step)
    {
        hit.accent = true;
    } else {
        let mut hit = DrumHit::new(midi, step);
        hit.accent = true;
        hits.push(hit);
    }
}

/// Applies a full performance layer to a raw drum hit stream.
///
/// Steps executed in order:
/// 1. Backbeat unghosting when intensity >= 2
/// 2. Ghost-note filtering if disabled
/// 3. Riff-stab kick and cymbal catches
/// 4. Accent gating if disabled
/// 5. Physical limb constraint solving
/// 6. Human articulation ornament application
/// 7. Hi-hat choke pairing
/// 8. Re-solving physical limb constraints
/// 9. Velocity shaping across dynamic tiers
/// 10. Optional deterministic humanization
pub fn apply_performance_layer(raw_hits: &[DrumHit], ctx: &PerformanceLayerConfig) -> Vec<DrumHit> {
    if raw_hits.is_empty() {
        return Vec::new();
    }

    let mut hits = raw_hits.to_vec();

    // 1. Backbeat unghosting at hot section intensity
    if ctx.intensity >= 2 && !ctx.backbeat_steps.is_empty() {
        for hit in &mut hits {
            if matches!(hit.midi, voices::ACOUSTIC_SNARE | voices::SIDE_STICK)
                && ctx.backbeat_steps.contains(&hit.step)
            {
                hit.ghost = false;
                hit.accent = true;
            }
        }
    }

    // 2. Ghost note gating
    if !ctx.ghost_notes {
        hits.retain(|hit| !hit.ghost);
    }

    // 3. Riff-stab catches (kick + china unison accents)
    for &step in &ctx.stab_steps {
        accent_or_insert_hit(&mut hits, voices::ACOUSTIC_BASS_DRUM, step);
        accent_or_insert_hit(&mut hits, voices::CHINESE_CYMBAL, step);
    }

    // 4. Accent gating
    if !ctx.accents {
        for hit in &mut hits {
            hit.accent = false;
        }
    }

    // 5. First limb solve
    hits = super::solve_limbs(&hits, ctx.tempo, ctx.fill_applied);

    // 6. Articulations
    if !ctx.articulations.is_empty() {
        apply_articulations(&mut hits, &ctx.articulations, 10);
    }

    // 7. Hi-hat chokes
    pair_hihat_chokes(&mut hits);

    // 8. Re-solve limbs
    hits = super::solve_limbs(&hits, ctx.tempo, ctx.fill_applied);

    // 9. Velocity shaping
    super::presets::shape_velocities(&mut hits, ctx.base_velocity);

    // 10. Humanization
    if let Some((seed, config)) = ctx.humanization {
        humanize_hits(&mut hits, seed, config);
    }

    hits
}

#[cfg(test)]
#[path = "performance_test.rs"]
mod tests;
