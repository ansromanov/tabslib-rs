//! Deterministic orchestration for adding accompaniment layers to a document.
//!
//! [`generate_accompaniment`] combines the native drum, bass, harmony, and
//! solo generators while preserving authored tracks and reporting skipped
//! roles explicitly.

use crate::model::Document;
use crate::Result;

/// Accompaniment roles that can be requested from the native workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccompanimentRole {
    /// Percussion generated from the selected source tracks.
    Drums,
    /// Riff-locked bass generated from the selected source tracks.
    Bass,
    /// Harmony generated from one selected source track.
    Harmony,
    /// A playable lead generated over the document's bars.
    Lead,
    /// An explicitly-role second guitar complementary to one source part.
    SecondGuitar,
}

/// Whether a role is generated or why the request did not add it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccompanimentOutcome {
    /// The generated track's index and sounding note count.
    Generated {
        /// Zero-based index of the new track.
        track_index: usize,
        /// Number of generated sounding notes.
        note_count: usize,
    },
    /// The role was not generated.
    Skipped(AccompanimentSkipReason),
}

/// A reason an accompaniment role was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccompanimentSkipReason {
    /// The caller did not enable this role.
    NotRequested,
    /// A matching authored role already exists and force was not enabled.
    ExistingRole,
    /// This build does not include the feature needed for this role.
    FeatureDisabled,
}

/// Result for one requested accompaniment role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccompanimentRoleResult {
    /// Role described by this result.
    pub role: AccompanimentRole,
    /// Generated track details or the reason it was skipped.
    pub outcome: AccompanimentOutcome,
}

/// Results for all five accompaniment roles, in stable role order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccompanimentResult {
    /// Per-role results in drums, bass, harmony, and lead order.
    pub roles: Vec<AccompanimentRoleResult>,
}

/// Which accompaniment roles the workflow should attempt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccompanimentLayers {
    /// Add a drum track.
    pub drums: bool,
    /// Add a bass track.
    pub bass: bool,
    /// Add a harmony track.
    pub harmony: bool,
    /// Add a lead/solo track.
    pub lead: bool,
}

/// Inputs for deterministic accompaniment generation on an existing document.
#[derive(Debug, Clone)]
pub struct AccompanimentRequest<'a> {
    /// Source tracks used by drum and bass generation, and the default harmony source.
    pub source_tracks: &'a [usize],
    /// Roles to attempt. Unselected roles are returned as `NotRequested`.
    pub layers: AccompanimentLayers,
    /// Shared seed. The workflow derives stable, role-specific seeds from it.
    pub seed: u64,
    /// Add requested roles even when a matching authored role already exists.
    /// Existing tracks are always preserved; this never replaces or removes tracks.
    pub force: bool,
    /// Tempo supplied to the drum playability solver.
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub drum_tempo: u32,
    /// Optional drum profile; the native default is used when absent.
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub drum_profile: Option<super::drums::performance::DrumProfile>,
    /// Optional section-specific drum feels. Uncovered bars use `drum_profile`.
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub drum_sections: Vec<DrumSectionRequest>,
    /// Optional caller-owned MIDI groove index used by drum generation.
    #[cfg(all(feature = "drums", feature = "rhythm", feature = "midi"))]
    pub drum_midi_index: Option<super::drums::midi_groove::MidiGrooveIndex>,
    /// Optional bass profile; the native default is used when absent.
    #[cfg(all(feature = "drums", feature = "rhythm"))]
    pub bass_profile: Option<super::rhythm::BassProfile>,
    /// Optional harmony configuration; defaults to a diatonic third on the first source track.
    #[cfg(feature = "harmony")]
    pub harmony_config: Option<super::harmony::HarmonyConfig>,
    /// Optional lead configuration; defaults to a C-major lead across the document.
    #[cfg(feature = "harmony")]
    pub lead_config: Option<super::solo::SoloConfig>,
    /// Optional explicit second-guitar role.
    #[cfg(feature = "harmony")]
    pub second_guitar: Option<super::second_guitar::SecondGuitarConfig>,
}

impl<'a> AccompanimentRequest<'a> {
    /// Creates a request with every role disabled and native role defaults.
    pub fn new(source_tracks: &'a [usize], seed: u64) -> Self {
        Self {
            source_tracks,
            layers: AccompanimentLayers::default(),
            seed,
            force: false,
            #[cfg(all(feature = "drums", feature = "rhythm"))]
            drum_tempo: 120,
            #[cfg(all(feature = "drums", feature = "rhythm"))]
            drum_profile: None,
            #[cfg(all(feature = "drums", feature = "rhythm"))]
            drum_sections: Vec::new(),
            #[cfg(all(feature = "drums", feature = "rhythm", feature = "midi"))]
            drum_midi_index: None,
            #[cfg(all(feature = "drums", feature = "rhythm"))]
            bass_profile: None,
            #[cfg(feature = "harmony")]
            harmony_config: None,
            #[cfg(feature = "harmony")]
            lead_config: None,
            #[cfg(feature = "harmony")]
            second_guitar: None,
        }
    }
}

/// A named drum feel applied to an inclusive master-bar range.
#[cfg(all(feature = "drums", feature = "rhythm"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrumSectionRequest {
    /// First master bar in the range.
    pub start_bar: usize,
    /// Last master bar in the range.
    pub end_bar: usize,
    /// Feel to apply to the range.
    pub feel: super::drums::presets::SectionDrumFeel,
}

/// Adds selected accompaniment layers to a native document deterministically.
///
/// Existing percussion and bass tracks are detected from score classification;
/// harmony and lead tracks are detected from their authored role names. With
/// `force` enabled matching tracks remain untouched and new generated tracks
/// are appended alongside them. If generation fails, the input document is
/// left unchanged.
///
/// # Errors
///
/// Returns the first error from any selected role's generator, for example
/// [`crate::Error::Malformed`] for an invalid source track, bar range or configuration. On error
/// `document` is left unchanged.
pub fn generate_accompaniment(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentResult> {
    let mut generated = document.clone();
    let mut roles = Vec::with_capacity(5);
    for role in [
        AccompanimentRole::Drums,
        AccompanimentRole::Bass,
        AccompanimentRole::Harmony,
        AccompanimentRole::Lead,
        AccompanimentRole::SecondGuitar,
    ] {
        let selected = match role {
            AccompanimentRole::Drums => request.layers.drums,
            AccompanimentRole::Bass => request.layers.bass,
            AccompanimentRole::Harmony => request.layers.harmony,
            AccompanimentRole::Lead => request.layers.lead,
            AccompanimentRole::SecondGuitar => second_guitar_requested(request),
        };
        if !selected {
            roles.push(skipped(role, AccompanimentSkipReason::NotRequested));
            continue;
        }
        if !request.force && has_authored_role(&generated, role) {
            roles.push(skipped(role, AccompanimentSkipReason::ExistingRole));
            continue;
        }
        let result = match role {
            AccompanimentRole::Drums => generate_drums(&mut generated, request)?,
            AccompanimentRole::Bass => generate_bass(&mut generated, request)?,
            AccompanimentRole::Harmony => generate_harmony(&mut generated, request)?,
            AccompanimentRole::Lead => generate_lead(&mut generated, request)?,
            AccompanimentRole::SecondGuitar => generate_second_guitar(&mut generated, request)?,
        };
        roles.push(AccompanimentRoleResult {
            role,
            outcome: result,
        });
    }
    *document = generated;
    Ok(AccompanimentResult { roles })
}

/// Layers all available accompaniment roles over a document assembled by a plan.
///
/// The plan is checked against the document's bar span before generation. The
/// shared seed and role-specific defaults make the result reproducible, while
/// authored tracks remain in place alongside generated roles.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if the plan's section bars do not add up to the document's
/// bar count. Otherwise returns any error from [`generate_accompaniment`].
#[cfg(feature = "arrangement")]
pub fn generate_full_song(
    document: &mut Document,
    plan: &super::arrangement::ArrangementPlan,
    source_tracks: &[usize],
    seed: u64,
) -> Result<AccompanimentResult> {
    let planned_bars = plan
        .sections
        .iter()
        .map(|section| section.bars)
        .sum::<usize>();
    if planned_bars != document.master_bars.len() {
        return Err(crate::Error::Malformed(format!(
            "arrangement plan covers {planned_bars} bars but document contains {}",
            document.master_bars.len()
        )));
    }
    let mut request = AccompanimentRequest::new(source_tracks, seed);
    request.layers = AccompanimentLayers {
        drums: true,
        bass: true,
        harmony: true,
        lead: true,
    };
    request.force = true;
    #[cfg(feature = "harmony")]
    {
        if let Some(source) = source_tracks.first().copied() {
            let mut second_guitar =
                super::second_guitar::SecondGuitarConfig::contextual(source, seed);
            if let Some(track) = document.tracks.get(source) {
                if !track.tuning.is_empty() {
                    second_guitar.tuning = track.tuning.clone();
                }
            }
            request.second_guitar = Some(second_guitar);
        }
    }
    generate_accompaniment(document, &request)
}

fn skipped(role: AccompanimentRole, reason: AccompanimentSkipReason) -> AccompanimentRoleResult {
    AccompanimentRoleResult {
        role,
        outcome: AccompanimentOutcome::Skipped(reason),
    }
}

fn has_authored_role(document: &Document, role: AccompanimentRole) -> bool {
    document.tracks.iter().enumerate().any(|(index, track)| {
        let name = track.name.to_ascii_lowercase();
        match role {
            AccompanimentRole::Drums => crate::inspect::classify_track(document, index)
                .is_some_and(|(kind, sounding_bars)| {
                    kind == crate::inspect::TrackKind::Percussion && sounding_bars > 0
                }),
            AccompanimentRole::Bass => crate::inspect::classify_track(document, index).is_some_and(
                |(kind, sounding_bars)| {
                    kind == crate::inspect::TrackKind::Bass && sounding_bars > 0
                },
            ),
            AccompanimentRole::Harmony => {
                name.contains("harmony")
                    || name.contains("guitar 2")
                    || name.contains("second guitar")
            }
            AccompanimentRole::Lead => name.contains("lead") || name.contains("solo"),
            AccompanimentRole::SecondGuitar => {
                name.contains("guitar 2") || name.contains("second guitar")
            }
        }
    })
}

#[cfg(feature = "harmony")]
fn second_guitar_requested(request: &AccompanimentRequest<'_>) -> bool {
    request.second_guitar.is_some()
}

#[cfg(not(feature = "harmony"))]
fn second_guitar_requested(_request: &AccompanimentRequest<'_>) -> bool {
    false
}

#[cfg(feature = "harmony")]
fn generate_second_guitar(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    let Some(config) = &request.second_guitar else {
        return Ok(AccompanimentOutcome::Skipped(
            AccompanimentSkipReason::NotRequested,
        ));
    };
    let result = super::second_guitar::generate_second_guitar(document, config, request.force)
        .map_err(|error| crate::Error::Malformed(error.to_string()))?;
    Ok(AccompanimentOutcome::Generated {
        track_index: result.track_index,
        note_count: result.note_count,
    })
}

#[cfg(not(feature = "harmony"))]
fn generate_second_guitar(
    _document: &mut Document,
    _request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    Ok(AccompanimentOutcome::Skipped(
        AccompanimentSkipReason::FeatureDisabled,
    ))
}

#[cfg(all(feature = "drums", feature = "rhythm"))]
fn generate_drums(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    let default_profile = super::drums::performance::DrumProfile {
        kick_voice: 35,
        snare_voice: 38,
        cymbal_voice: 42,
        kick_probability: 80,
        snare_probability: 70,
        cymbal_probability: 75,
    };
    let profile = request.drum_profile.as_ref().unwrap_or(&default_profile);
    #[cfg(feature = "midi")]
    let result = if let Some(index) = request.drum_midi_index.as_ref() {
        super::workflows::generate_drums_from_midi_index(
            document,
            request.source_tracks,
            index,
            profile,
            role_seed(request.seed, AccompanimentRole::Drums),
            request.drum_tempo,
        )?
    } else {
        super::workflows::generate_drums_with_sections(
            document,
            request.source_tracks,
            profile,
            role_seed(request.seed, AccompanimentRole::Drums),
            request.drum_tempo,
            &request.drum_sections,
        )?
    };
    #[cfg(not(feature = "midi"))]
    let result = super::workflows::generate_drums_with_sections(
        document,
        request.source_tracks,
        profile,
        role_seed(request.seed, AccompanimentRole::Drums),
        request.drum_tempo,
        &request.drum_sections,
    )?;
    Ok(AccompanimentOutcome::Generated {
        track_index: result.track_index,
        note_count: result.note_count,
    })
}

#[cfg(not(all(feature = "drums", feature = "rhythm")))]
fn generate_drums(
    _document: &mut Document,
    _request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    Ok(AccompanimentOutcome::Skipped(
        AccompanimentSkipReason::FeatureDisabled,
    ))
}

#[cfg(all(feature = "drums", feature = "rhythm"))]
fn generate_bass(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    let profile = request.bass_profile.unwrap_or(super::rhythm::BassProfile {
        min_midi: 28,
        max_midi: 52,
        max_leap: 7,
        accent_velocity: 110,
        normal_velocity: 90,
    });
    let result = super::workflows::generate_bass(
        document,
        request.source_tracks,
        super::rhythm::BassGenerationConfig::new(profile),
    )?;
    Ok(AccompanimentOutcome::Generated {
        track_index: result.track_index,
        note_count: result.note_count,
    })
}

#[cfg(not(all(feature = "drums", feature = "rhythm")))]
fn generate_bass(
    _document: &mut Document,
    _request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    Ok(AccompanimentOutcome::Skipped(
        AccompanimentSkipReason::FeatureDisabled,
    ))
}

#[cfg(feature = "harmony")]
fn generate_harmony(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    let mut config = request.harmony_config.clone().unwrap_or_else(|| {
        super::harmony::HarmonyConfig::diatonic_third(
            request.source_tracks.first().copied().unwrap_or(0),
            request.seed,
        )
    });
    config.seed = role_seed(request.seed, AccompanimentRole::Harmony);
    let result = super::harmony::generate_harmony(document, &config)?;
    Ok(AccompanimentOutcome::Generated {
        track_index: result.track_index,
        note_count: result.note_count,
    })
}

#[cfg(not(feature = "harmony"))]
fn generate_harmony(
    _document: &mut Document,
    _request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    Ok(AccompanimentOutcome::Skipped(
        AccompanimentSkipReason::FeatureDisabled,
    ))
}

#[cfg(feature = "harmony")]
fn generate_lead(
    document: &mut Document,
    request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    let mut config = request.lead_config.clone().unwrap_or_else(|| {
        let end_bar = document.master_bars.len().saturating_sub(1);
        let mut config = super::solo::SoloConfig::c_major(0, end_bar, request.seed);
        if let Some(key) = document.key {
            config.key = key;
        }
        config
    });
    config.seed = role_seed(request.seed, AccompanimentRole::Lead);
    let result = super::solo::generate_solo(document, &config)?;
    Ok(AccompanimentOutcome::Generated {
        track_index: result.track_index,
        note_count: result.note_count,
    })
}

#[cfg(not(feature = "harmony"))]
fn generate_lead(
    _document: &mut Document,
    _request: &AccompanimentRequest<'_>,
) -> Result<AccompanimentOutcome> {
    Ok(AccompanimentOutcome::Skipped(
        AccompanimentSkipReason::FeatureDisabled,
    ))
}

#[cfg(any(all(feature = "drums", feature = "rhythm"), feature = "harmony"))]
fn role_seed(seed: u64, role: AccompanimentRole) -> u64 {
    let tag = match role {
        AccompanimentRole::Drums => 0x243f_6a88_85a3_08d3,
        AccompanimentRole::Bass => 0x1319_8a2e_0370_7344,
        AccompanimentRole::Harmony => 0xa409_3822_299f_31d0,
        AccompanimentRole::Lead => 0x082e_fa98_ec4e_6c89,
        AccompanimentRole::SecondGuitar => 0x4528_21e6_38d0_1377,
    };
    seed ^ tag
}

#[cfg(test)]
#[path = "accompaniment_test.rs"]
mod tests;
