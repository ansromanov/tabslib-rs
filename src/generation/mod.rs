//! Generative and analytical music tools built into [`crate`].
//!
//! This module contains generation, arrangement, assessment, calibration, and
//! optional AI integrations. Style material is supplied by the caller and is
//! never part of the library source.

pub mod accompaniment;
#[cfg(feature = "ai")]
pub mod ai;
#[cfg(feature = "arrangement")]
pub mod arrangement;
#[cfg(feature = "arrangement")]
pub mod arrangement_dynamics;
#[cfg(all(feature = "arrangement", feature = "assessment"))]
pub mod arrangement_search;
#[cfg(feature = "arrangement")]
mod arrangement_support;
#[cfg(feature = "assessment")]
pub mod assessment;
pub mod candidate_deck;
#[cfg(feature = "drums")]
pub mod drums;
pub mod execution_log;
#[cfg(feature = "arrangement")]
pub mod forms;
#[cfg(feature = "harmony")]
pub mod harmonic;
#[cfg(feature = "harmony")]
pub mod harmony;
#[cfg(feature = "arrangement")]
pub mod intent;
pub mod meter;
#[cfg(feature = "arrangement")]
pub mod motif_roles;
#[cfg(feature = "drums")]
pub mod pattern_catalog;
#[cfg(feature = "arrangement")]
pub mod plan_edit;
#[cfg(feature = "arrangement")]
pub mod plan_sidecar;
#[cfg(feature = "assessment")]
pub mod preference;
#[cfg(feature = "assessment")]
pub mod preference_ranker;
#[cfg(all(feature = "arrangement", feature = "assessment"))]
pub mod quality_gaps;
#[cfg(feature = "assessment")]
pub mod requirements;
#[cfg(feature = "rhythm")]
pub mod rhythm;
#[cfg(feature = "rhythm")]
pub mod rhythm_profile;
#[cfg(feature = "riffs")]
pub mod riff_adaptation;
#[cfg(feature = "arrangement")]
pub mod riff_composition;
#[cfg(all(feature = "riffs", feature = "assessment"))]
pub mod riff_hybrid;
#[cfg(feature = "riffs")]
pub mod riff_spec;
#[cfg(feature = "harmony")]
pub mod second_guitar;
#[cfg(feature = "arrangement")]
pub mod sections;
#[cfg(feature = "harmony")]
pub mod solo;
#[cfg(feature = "harmony")]
pub mod solo_presets;
#[cfg(all(feature = "harmony", feature = "assessment"))]
pub mod solo_tokens;
#[cfg(feature = "assessment")]
pub mod studies;
#[cfg(feature = "riffs")]
pub mod texture;
#[cfg(feature = "harmony")]
pub mod transforms;
#[cfg(feature = "arrangement")]
pub mod transitions;
#[cfg(any(feature = "drums", feature = "rhythm"))]
pub mod workflows;
