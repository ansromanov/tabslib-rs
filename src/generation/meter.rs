//! Meter beat groupings and their exact positions on caller-selected grids.
//!
//! [`BeatGrouping`] describes grouping in denominator units. It owns no
//! style data; callers may use a common default or supply a grouping that
//! sums to the meter numerator.

use thiserror::Error;

#[cfg(feature = "drums")]
use super::pattern_catalog::PatternGrid;

/// Grouped beats for one time signature, expressed in denominator units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatGrouping {
    meter: (u32, u32),
    groups: Vec<u32>,
}

impl BeatGrouping {
    /// Creates a grouping after verifying its positive units sum to the meter.
    pub fn new(meter: (u32, u32), groups: Vec<u32>) -> Result<Self, BeatGroupingError> {
        if meter.0 == 0 || meter.1 == 0 {
            return Err(BeatGroupingError::InvalidMeter { meter });
        }
        let total = groups.iter().try_fold(0u32, |sum, group| {
            if *group == 0 {
                return None;
            }
            sum.checked_add(*group)
        });
        if groups.is_empty() || total != Some(meter.0) {
            return Err(BeatGroupingError::InvalidGroups { meter, groups });
        }
        Ok(Self { meter, groups })
    }

    /// Returns the conventional simple, compound, or additive grouping, or derives
    /// a default unit grouping from the numerator when no specific rule applies.
    pub fn for_meter(meter: (u32, u32)) -> Result<Self, BeatGroupingError> {
        let (numerator, denominator) = meter;
        if numerator == 0 || denominator == 0 {
            return Err(BeatGroupingError::InvalidMeter { meter });
        }
        let groups = match (numerator, denominator) {
            (2..=4, 2 | 4) => {
                vec![1; usize::try_from(numerator).map_err(|_| BeatGroupingError::Overflow)?]
            }
            (5, 8) => vec![2, 3],
            (7, 8) => vec![2, 2, 3],
            (6 | 9 | 12, 8) => {
                vec![3; usize::try_from(numerator / 3).map_err(|_| BeatGroupingError::Overflow)?]
            }
            (5, 4) => vec![3, 2],
            (7, 4) => vec![4, 3],
            _ => vec![1; usize::try_from(numerator).map_err(|_| BeatGroupingError::Overflow)?],
        };
        Self::new(meter, groups)
    }

    /// Meter for this grouping.
    pub const fn meter(&self) -> (u32, u32) {
        self.meter
    }

    /// Denominator-unit group lengths.
    pub fn groups(&self) -> &[u32] {
        &self.groups
    }

    /// Start positions of each group on a grid of `subdivisions_per_quarter`.
    pub fn pulse_steps(
        &self,
        subdivisions_per_quarter: u32,
    ) -> Result<Vec<u32>, BeatGroupingError> {
        let (numerator, denominator) = self.meter;
        if subdivisions_per_quarter == 0 {
            return Err(BeatGroupingError::InvalidSubdivision);
        }
        let quarter_steps = 4u64
            .checked_mul(u64::from(subdivisions_per_quarter))
            .ok_or(BeatGroupingError::Overflow)?;
        let denominator = u64::from(denominator);
        if quarter_steps % denominator != 0 {
            return Err(BeatGroupingError::UnrepresentableUnit {
                meter: self.meter,
                subdivisions_per_quarter,
            });
        }
        let steps_per_unit = quarter_steps / denominator;
        let mut offset = 0u64;
        let mut pulses = Vec::with_capacity(self.groups.len());
        for group in &self.groups {
            pulses.push(u32::try_from(offset).map_err(|_| BeatGroupingError::Overflow)?);
            offset = offset
                .checked_add(
                    u64::from(*group)
                        .checked_mul(steps_per_unit)
                        .ok_or(BeatGroupingError::Overflow)?,
                )
                .ok_or(BeatGroupingError::Overflow)?;
        }
        let expected = u64::from(numerator)
            .checked_mul(steps_per_unit)
            .ok_or(BeatGroupingError::Overflow)?;
        if offset != expected {
            return Err(BeatGroupingError::InvalidGroups {
                meter: self.meter,
                groups: self.groups.clone(),
            });
        }
        Ok(pulses)
    }

    /// Returns group starts in steps of a matching [`PatternGrid`].
    #[cfg(feature = "drums")]
    pub fn pulse_positions(&self, grid: PatternGrid) -> Result<Vec<u32>, BeatGroupingError> {
        self.validate_grid(grid)?;
        self.pulse_steps(grid.subdivisions_per_quarter)
    }

    /// Returns the starts of the second, fourth, and later even-numbered groups.
    pub fn backbeat_steps(
        &self,
        subdivisions_per_quarter: u32,
    ) -> Result<Vec<u32>, BeatGroupingError> {
        Ok(self
            .pulse_steps(subdivisions_per_quarter)?
            .into_iter()
            .enumerate()
            .filter_map(|(index, step)| (index > 0 && index % 2 == 1).then_some(step))
            .collect())
    }

    /// Returns backbeat positions in steps of a matching [`PatternGrid`].
    #[cfg(feature = "drums")]
    pub fn backbeat_positions(&self, grid: PatternGrid) -> Result<Vec<u32>, BeatGroupingError> {
        self.validate_grid(grid)?;
        self.backbeat_steps(grid.subdivisions_per_quarter)
    }

    #[cfg(feature = "drums")]
    fn validate_grid(&self, grid: PatternGrid) -> Result<(), BeatGroupingError> {
        if grid.meter != self.meter {
            return Err(BeatGroupingError::GridMeterMismatch {
                grouping_meter: self.meter,
                grid_meter: grid.meter,
            });
        }
        grid.steps_per_bar()
            .map_err(|error| BeatGroupingError::InvalidGrid(error.to_string()))?;
        Ok(())
    }
}

/// Invalid or unrepresentable meter grouping input.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BeatGroupingError {
    /// Time-signature numerator and denominator must be positive.
    #[error("invalid meter {meter:?}")]
    InvalidMeter { meter: (u32, u32) },
    /// No default grouping is defined for this time signature.
    #[error("no default beat grouping is defined for meter {meter:?}")]
    UnsupportedMeter { meter: (u32, u32) },
    /// Groups must be positive and sum to the time-signature numerator.
    #[error("groups {groups:?} do not form meter {meter:?}")]
    InvalidGroups { meter: (u32, u32), groups: Vec<u32> },
    /// Grid resolution must be positive.
    #[error("subdivisions per quarter must be positive")]
    InvalidSubdivision,
    /// One denominator unit does not land on an integer grid step.
    #[error("meter {meter:?} cannot be grouped on a grid with {subdivisions_per_quarter} subdivisions per quarter")]
    UnrepresentableUnit {
        meter: (u32, u32),
        subdivisions_per_quarter: u32,
    },
    /// A grouping and grid have different time signatures.
    #[cfg(feature = "drums")]
    #[error("beat grouping meter {grouping_meter:?} does not match grid meter {grid_meter:?}")]
    GridMeterMismatch {
        grouping_meter: (u32, u32),
        grid_meter: (u32, u32),
    },
    /// Pattern-grid validation failed.
    #[cfg(feature = "drums")]
    #[error("invalid pattern grid: {0}")]
    InvalidGrid(String),
    /// Position arithmetic or a platform-size conversion overflowed.
    #[error("beat grouping arithmetic overflow")]
    Overflow,
}

#[cfg(test)]
#[path = "meter_test.rs"]
mod tests;
