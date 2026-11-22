//! What a generator decided, bar by bar.
//!
//! The module owns [`ExecutionLog`], [`Decision`] and [`explain`]. A
//! [`GeneratedTrack`](super::workflows::GeneratedTrack) reports counts, so when
//! an assessment flags a bar there is nothing to look at: diagnosis means
//! re-running with different inputs and comparing the output.
//!
//! A log records the choice behind each generated bar and the seed it was
//! derived from, and [`explain`] correlates a flagged region back to the
//! decisions that produced it.
//!
//! # Where provenance is not written
//!
//! A log is not smuggled into the score container. The payload writer emits
//! exactly the shapes the originating application produces, and inventing an
//! element to carry provenance would break that invariant and leave a file
//! whose behaviour in other applications is unknown. A log is serialized
//! beside the score instead, through [`ExecutionLog::to_json`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Schema version written into every serialized log.
pub const LOG_VERSION: u32 = 1;

/// Which generator made a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GeneratedRole {
    /// Percussion.
    Drums,
    /// Bass.
    Bass,
    /// Harmony or second guitar.
    Harmony,
    /// Lead or solo.
    Lead,
    /// Authored riff material.
    Riff,
    /// Section assembly.
    Arrangement,
}

impl GeneratedRole {
    /// Stable machine-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Harmony => "harmony",
            Self::Lead => "lead",
            Self::Riff => "riff",
            Self::Arrangement => "arrangement",
        }
    }
}

/// One recorded choice, tied to the bar it produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// Zero-based master-bar position.
    pub bar: usize,
    /// Zero-based track position the decision wrote into.
    pub track: usize,
    /// Generator that made the choice.
    pub role: GeneratedRole,
    /// What was chosen: a pattern, a strategy, a fill, an ornament.
    pub choice: String,
    /// Position within the phrase the choice was made at, counting from zero.
    pub phrase_position: usize,
    /// Seed the choice was derived from, after the per-bar mix.
    pub seed: u64,
    /// Number of events the choice produced.
    pub events: usize,
}

/// Every decision behind one generation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionLog {
    /// Schema version.
    pub version: u32,
    /// Seed the run started from, before any per-bar mixing.
    pub seed: u64,
    /// Decisions in the order they were made.
    pub decisions: Vec<Decision>,
}

impl ExecutionLog {
    /// An empty log for a run started from `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            version: LOG_VERSION,
            seed,
            decisions: Vec::new(),
        }
    }

    /// Appends one decision.
    pub fn record(&mut self, decision: Decision) {
        self.decisions.push(decision);
    }

    /// Decisions that wrote into one bar of one track.
    pub fn at(&self, bar: usize, track: usize) -> Vec<&Decision> {
        self.decisions
            .iter()
            .filter(|decision| decision.bar == bar && decision.track == track)
            .collect()
    }

    /// How many decisions each role made.
    pub fn by_role(&self) -> BTreeMap<GeneratedRole, usize> {
        let mut counts = BTreeMap::new();
        for decision in &self.decisions {
            *counts.entry(decision.role).or_insert(0) += 1;
        }
        counts
    }

    /// Serializes the log as pretty JSON.
    pub fn to_json(&self) -> Result<String, LogError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| LogError::Serialization(error.to_string()))
    }

    /// Parses a serialized log, rejecting a version this build cannot read.
    pub fn from_json(json: &str) -> Result<Self, LogError> {
        let log: Self = serde_json::from_str(json)
            .map_err(|error| LogError::Serialization(error.to_string()))?;
        if log.version != LOG_VERSION {
            return Err(LogError::UnsupportedVersion(log.version));
        }
        Ok(log)
    }
}

/// Why a log could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LogError {
    /// The log could not be serialized or parsed.
    #[error("execution log serialization failed: {0}")]
    Serialization(String),
    /// The log was written by a different schema version.
    #[error("unsupported execution log version {0}")]
    UnsupportedVersion(u32),
}

/// Decisions that could have produced a finding at one region.
///
/// A diagnostic names a bar and usually a track. This returns what was decided
/// there, which is the step between "bar 12 repeats an earlier pattern" and
/// "bar 12 selected pattern `four_on_the_floor` from seed 0x51f3".
///
/// A diagnostic with no track matches every track's decisions in that bar,
/// because a score-wide finding cannot be attributed to one generator.
pub fn explain(log: &ExecutionLog, bar: usize, track: Option<usize>) -> Vec<&Decision> {
    log.decisions
        .iter()
        .filter(|decision| decision.bar == bar && track.is_none_or(|track| decision.track == track))
        .collect()
}

#[cfg(test)]
#[path = "execution_log_test.rs"]
mod tests;
