//! Caller-owned, deterministic pattern catalogs with exact grid adaptation.
//!
//! Catalogs contain only the material supplied by the application. They do
//! not assign names to styles or embed curated musical content.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::model::Document;

/// A meter and a rhythmic resolution measured in subdivisions per quarter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PatternGrid {
    /// Meter numerator and denominator.
    pub meter: (u32, u32),
    /// Number of grid steps in one quarter note.
    pub subdivisions_per_quarter: u32,
}

impl PatternGrid {
    /// Creates a grid and verifies that its meter has an integral step count.
    pub fn new(meter: (u32, u32), subdivisions_per_quarter: u32) -> Result<Self, PatternError> {
        let grid = Self {
            meter,
            subdivisions_per_quarter,
        };
        grid.steps_per_bar()?;
        Ok(grid)
    }

    /// Number of steps in one bar.
    pub fn steps_per_bar(self) -> Result<usize, PatternError> {
        let (numerator, denominator) = self.meter;
        if numerator == 0 || denominator == 0 {
            return Err(PatternError::InvalidMeter { meter: self.meter });
        }
        if self.subdivisions_per_quarter == 0 {
            return Err(PatternError::InvalidSubdivision);
        }
        let ticks = u64::from(numerator)
            .checked_mul(4)
            .and_then(|value| value.checked_mul(u64::from(self.subdivisions_per_quarter)))
            .ok_or(PatternError::GridOverflow)?;
        let denominator = u64::from(denominator);
        if ticks % denominator != 0 {
            return Err(PatternError::NonIntegralGrid {
                meter: self.meter,
                subdivisions_per_quarter: self.subdivisions_per_quarter,
            });
        }
        usize::try_from(ticks / denominator).map_err(|_| PatternError::GridOverflow)
    }
}

/// A single reusable pattern on a fixed rhythmic grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPattern<T> {
    /// Caller-assigned stable identifier.
    pub id: String,
    /// Meter and resolution in which this pattern is expressed.
    pub grid: PatternGrid,
    /// Dense step data, grouped by bar. Empty steps use `T::default()`.
    pub bars: Vec<Vec<T>>,
}

impl<T: Clone + Default + PartialEq> CatalogPattern<T> {
    /// Validates a dense pattern. Bars must include every grid step.
    pub fn new(
        id: impl Into<String>,
        grid: PatternGrid,
        bars: Vec<Vec<T>>,
    ) -> Result<Self, PatternError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(PatternError::EmptyPatternId);
        }
        if bars.is_empty() {
            return Err(PatternError::EmptyPattern { id });
        }
        let expected = grid.steps_per_bar()?;
        for (bar, steps) in bars.iter().enumerate() {
            if steps.len() != expected {
                return Err(PatternError::InvalidBarWidth {
                    id,
                    bar,
                    expected,
                    actual: steps.len(),
                });
            }
        }
        Ok(Self { id, grid, bars })
    }

    /// Adapts to a target meter/resolution without dropping active steps.
    /// Events must land exactly on the target grid; empty cells may be padded
    /// or removed. Articulation data travels with each active step unchanged.
    pub fn adapt(&self, target: PatternGrid) -> Result<Self, PatternError> {
        let source_width = self.grid.steps_per_bar()?;
        let target_width = target.steps_per_bar()?;
        let source_sub = u64::from(self.grid.subdivisions_per_quarter);
        let target_sub = u64::from(target.subdivisions_per_quarter);
        let mut bars = Vec::with_capacity(self.bars.len());
        for (bar_index, source_bar) in self.bars.iter().enumerate() {
            if source_bar.len() != source_width {
                return Err(PatternError::InvalidBarWidth {
                    id: self.id.clone(),
                    bar: bar_index,
                    expected: source_width,
                    actual: source_bar.len(),
                });
            }
            let mut target_bar = vec![T::default(); target_width];
            for (source_index, item) in source_bar.iter().enumerate() {
                if item == &T::default() {
                    continue;
                }
                let source_step =
                    u64::try_from(source_index).map_err(|_| PatternError::GridOverflow)?;
                let scaled = source_step
                    .checked_mul(target_sub)
                    .ok_or(PatternError::GridOverflow)?;
                if scaled % source_sub != 0 {
                    return Err(PatternError::UnrepresentableEvent {
                        id: self.id.clone(),
                        bar: bar_index,
                        step: source_index,
                    });
                }
                let target_step =
                    usize::try_from(scaled / source_sub).map_err(|_| PatternError::GridOverflow)?;
                if target_step >= target_width {
                    return Err(PatternError::EventOutsideTarget {
                        id: self.id.clone(),
                        bar: bar_index,
                        step: source_index,
                        target_width,
                    });
                }
                if target_bar[target_step] != T::default() {
                    return Err(PatternError::EventCollision {
                        id: self.id.clone(),
                        bar: bar_index,
                        step: source_index,
                        target_step,
                    });
                }
                target_bar[target_step] = item.clone();
            }
            bars.push(target_bar);
        }
        Self::new(self.id.clone(), target, bars)
    }

    /// Counts steps carrying non-default event data.
    pub fn active_steps(&self) -> usize {
        self.bars
            .iter()
            .flatten()
            .filter(|step| **step != T::default())
            .count()
    }
}

/// Caller constraints for selecting a pattern for a bar or phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternConstraints {
    /// Required target grid.
    pub grid: PatternGrid,
    /// Number of bars requested.
    pub bars: usize,
    /// Inclusive lower bound on active steps across the requested phrase.
    pub min_active_steps: usize,
    /// Inclusive upper bound on active steps across the requested phrase.
    pub max_active_steps: usize,
}

impl PatternConstraints {
    /// Creates constraints for a positive bar count, initially allowing any density.
    pub fn new(grid: PatternGrid, bars: usize) -> Result<Self, PatternError> {
        if bars == 0 {
            return Err(PatternError::EmptyRequest);
        }
        grid.steps_per_bar()?;
        Ok(Self {
            grid,
            bars,
            min_active_steps: 0,
            max_active_steps: usize::MAX,
        })
    }

    /// Builds a one-bar request from a document position.
    pub fn for_bar(
        document: &Document,
        bar_index: usize,
        subdivisions_per_quarter: u32,
    ) -> Result<Self, PatternError> {
        let master_bar = document
            .master_bars
            .get(bar_index)
            .ok_or(PatternError::DocumentRangeOutsideScore)?;
        Self::new(
            PatternGrid::new(master_bar.time, subdivisions_per_quarter)?,
            1,
        )
    }

    /// Builds a phrase request after verifying the range exists and has one meter.
    pub fn for_phrase(
        document: &Document,
        start_bar: usize,
        bars: usize,
        subdivisions_per_quarter: u32,
    ) -> Result<Self, PatternError> {
        if bars == 0 {
            return Err(PatternError::EmptyRequest);
        }
        let end = start_bar
            .checked_add(bars)
            .ok_or(PatternError::DocumentRangeOutsideScore)?;
        let phrase = document
            .master_bars
            .get(start_bar..end)
            .ok_or(PatternError::DocumentRangeOutsideScore)?;
        let meter = phrase
            .first()
            .ok_or(PatternError::DocumentRangeOutsideScore)?
            .time;
        if phrase.iter().any(|bar| bar.time != meter) {
            return Err(PatternError::MixedMeterPhrase);
        }
        Self::new(PatternGrid::new(meter, subdivisions_per_quarter)?, bars)
    }

    /// Restricts compatible patterns by active event count.
    pub fn with_active_steps(mut self, min: usize, max: usize) -> Result<Self, PatternError> {
        if min > max {
            return Err(PatternError::InvalidActiveStepRange { min, max });
        }
        self.min_active_steps = min;
        self.max_active_steps = max;
        Ok(self)
    }
}

/// Why a catalog entry could not satisfy a selection request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternRejection {
    /// Rejected pattern identifier.
    pub pattern_id: String,
    /// Human-readable, deterministic reason.
    pub reason: String,
}

/// A collection of caller-supplied patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternCatalog<T> {
    patterns: Vec<CatalogPattern<T>>,
}

impl<T: Clone + Default + PartialEq> PatternCatalog<T> {
    /// Creates a catalog and rejects duplicate identifiers. Empty catalogs
    /// are valid values so selection can report `EmptyCatalog` explicitly.
    pub fn new(patterns: Vec<CatalogPattern<T>>) -> Result<Self, PatternError> {
        let mut ids = BTreeSet::new();
        for pattern in &patterns {
            if !ids.insert(pattern.id.clone()) {
                return Err(PatternError::DuplicatePatternId {
                    id: pattern.id.clone(),
                });
            }
        }
        Ok(Self { patterns })
    }

    /// Patterns in caller-provided order.
    pub fn patterns(&self) -> &[CatalogPattern<T>] {
        &self.patterns
    }

    /// Filters, adapts, then selects with a stable seeded index.
    pub fn select_seeded(
        &self,
        constraints: &PatternConstraints,
        seed: u64,
    ) -> Result<SelectedPattern<T>, PatternError> {
        if self.patterns.is_empty() {
            return Err(PatternError::EmptyCatalog);
        }
        let mut compatible = Vec::new();
        let mut rejections = Vec::new();
        for (catalog_index, pattern) in self.patterns.iter().enumerate() {
            if pattern.bars.len() != constraints.bars {
                rejections.push(PatternRejection {
                    pattern_id: pattern.id.clone(),
                    reason: format!(
                        "has {} bars; {} requested",
                        pattern.bars.len(),
                        constraints.bars
                    ),
                });
                continue;
            }
            match pattern.adapt(constraints.grid) {
                Ok(adapted) => {
                    let density = adapted.active_steps();
                    if density < constraints.min_active_steps
                        || density > constraints.max_active_steps
                    {
                        rejections.push(PatternRejection {
                            pattern_id: pattern.id.clone(),
                            reason: format!(
                                "has {density} active steps; allowed {}..={}",
                                constraints.min_active_steps, constraints.max_active_steps
                            ),
                        });
                    } else {
                        compatible.push((catalog_index, adapted));
                    }
                }
                Err(error) => rejections.push(PatternRejection {
                    pattern_id: pattern.id.clone(),
                    reason: error.to_string(),
                }),
            }
        }
        if compatible.is_empty() {
            return Err(PatternError::NoCompatiblePattern { rejections });
        }
        let index = seeded_index(seed, compatible.len())?;
        let (catalog_index, pattern) = compatible.swap_remove(index);
        Ok(SelectedPattern {
            catalog_index,
            pattern,
        })
    }
}

/// Selected and already-adapted pattern plus its original catalog position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedPattern<T> {
    /// Position in the original catalog.
    pub catalog_index: usize,
    /// Adapted pattern ready for the requested document span.
    pub pattern: CatalogPattern<T>,
}

/// A neutral rhythm-section event cell for coordinating bass and harmony.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RhythmSectionStep {
    /// Bass onset at this step.
    pub bass_onset: bool,
    /// Harmony onset at this step.
    pub harmony_onset: bool,
    /// Shared accent metadata.
    pub accent: bool,
    /// Marks a phrase-tail/fill event.
    pub fill: bool,
}

/// Catalog type for articulation-preserving drum steps.
#[cfg(feature = "drums")]
pub type DrumPatternCatalog = PatternCatalog<super::drums::PatternStep>;

/// Catalog type for neutral rhythm-section steps.
pub type RhythmSectionPatternCatalog = PatternCatalog<RhythmSectionStep>;

fn seeded_index(seed: u64, len: usize) -> Result<usize, PatternError> {
    let len = u64::try_from(len).map_err(|_| PatternError::GridOverflow)?;
    let mut value = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    usize::try_from(value % len).map_err(|_| PatternError::GridOverflow)
}

/// Validation, adaptation, and selection failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatternError {
    /// Meter components must be positive.
    #[error("invalid meter {meter:?}")]
    InvalidMeter {
        /// Time signature as `(numerator, denominator)`.
        meter: (u32, u32),
    },
    /// Grid resolution must be positive.
    #[error("subdivisions per quarter must be positive")]
    InvalidSubdivision,
    /// Meter and resolution do not produce an integral bar width.
    #[error("meter {meter:?} has no integral grid at {subdivisions_per_quarter} subdivisions per quarter")]
    NonIntegralGrid {
        /// Time signature as `(numerator, denominator)`.
        meter: (u32, u32),
        /// Grid subdivisions per quarter note.
        subdivisions_per_quarter: u32,
    },
    /// Arithmetic or platform-size conversion overflowed.
    #[error("pattern grid arithmetic overflow")]
    GridOverflow,
    /// Pattern identifiers must contain a non-whitespace character.
    #[error("pattern identifier cannot be empty")]
    EmptyPatternId,
    /// A pattern must contain at least one bar.
    #[error("pattern {id:?} has no bars")]
    EmptyPattern {
        /// Identifier of the empty pattern.
        id: String,
    },
    /// A bar must be represented by a dense grid of the expected width.
    #[error("pattern {id:?} bar {bar} has {actual} steps; expected {expected}")]
    InvalidBarWidth {
        /// Pattern identifier.
        id: String,
        /// Zero-based bar index within the pattern.
        bar: usize,
        /// Expected number of steps in the bar.
        expected: usize,
        /// Actual number of steps in the bar.
        actual: usize,
    },
    /// Two pattern values would land on the same target step.
    #[error(
        "pattern {id:?} bar {bar} steps collide at target step {target_step} (source step {step})"
    )]
    EventCollision {
        /// Pattern identifier.
        id: String,
        /// Zero-based bar index within the pattern.
        bar: usize,
        /// Source step within the bar.
        step: usize,
        /// Target step within the bar where the values collide.
        target_step: usize,
    },
    /// Active source step falls off the end of the target bar.
    #[error("pattern {id:?} bar {bar} event at source step {step} falls outside target width {target_width}")]
    EventOutsideTarget {
        /// Pattern identifier.
        id: String,
        /// Zero-based bar index within the pattern.
        bar: usize,
        /// Source step within the bar.
        step: usize,
        /// Number of steps in the target bar.
        target_width: usize,
    },
    /// An active source step cannot be represented at the requested resolution.
    #[error("pattern {id:?} bar {bar} event at source step {step} is not representable on the target grid")]
    UnrepresentableEvent {
        /// Pattern identifier.
        id: String,
        /// Zero-based bar index within the pattern.
        bar: usize,
        /// Source step within the bar.
        step: usize,
    },
    /// Requested span is empty.
    #[error("selection must request at least one bar")]
    EmptyRequest,
    /// A requested bar or phrase is outside the document.
    #[error("requested bar range is outside the document")]
    DocumentRangeOutsideScore,
    /// A phrase spans more than one meter.
    #[error("a pattern phrase must have a single meter")]
    MixedMeterPhrase,
    /// Active-step density range is inverted.
    #[error("invalid active-step range {min}..={max}")]
    InvalidActiveStepRange {
        /// Requested minimum number of active steps.
        min: usize,
        /// Requested maximum number of active steps.
        max: usize,
    },
    /// Catalog identifiers must be unique.
    #[error("duplicate pattern identifier {id:?}")]
    DuplicatePatternId {
        /// The repeated identifier.
        id: String,
    },
    /// No patterns were supplied.
    #[error("pattern catalog is empty")]
    EmptyCatalog,
    /// No entry survived filtering/adaptation; includes one reason per rejected entry.
    #[error("no compatible pattern ({rejections:?})")]
    NoCompatiblePattern {
        /// One rejection per candidate entry.
        rejections: Vec<PatternRejection>,
    },
}
