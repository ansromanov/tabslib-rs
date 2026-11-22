//! Deterministic phrase-boundary fill scheduling for drum patterns.

/// A drum fill location scheduled at a phrase or section boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledFill {
    /// Number of score bars before the boundary (the boundary after bar N).
    pub bar_boundary: usize,
    /// Phrase lengths that end at this boundary, in bars.
    pub phrase_lengths: Vec<usize>,
    /// Whether this is also the start of a caller-supplied section.
    pub section_boundary: bool,
    /// Absolute subdivision step where the fill begins.
    pub start_step: u32,
    /// Number of subdivision steps reserved for the fill.
    pub length: u32,
}

/// A stateless clock that places fills at 4-, 8-, and 16-bar phrase endings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhraseClock;

impl PhraseClock {
    /// Schedules fills at phrase ends and supplied section starts.
    ///
    /// Section starts are zero-based bar indexes. A fill ends exactly at the
    /// boundary and is clamped to at most one bar. The initial section start
    /// (bar zero) has no preceding material and therefore creates no fill.
    pub fn schedule_fills(
        bar_count: usize,
        steps_per_bar: u32,
        fill_steps: u32,
        section_starts: &[usize],
    ) -> Vec<ScheduledFill> {
        if bar_count == 0 || steps_per_bar == 0 || fill_steps == 0 {
            return Vec::new();
        }
        let length = fill_steps.min(steps_per_bar);
        let mut scheduled = Vec::new();
        for bar_boundary in 1..=bar_count {
            let phrase_lengths = [4, 8, 16]
                .into_iter()
                .filter(|phrase_length| bar_boundary % phrase_length == 0)
                .collect::<Vec<_>>();
            let section_boundary = section_starts.contains(&bar_boundary);
            if phrase_lengths.is_empty() && !section_boundary {
                continue;
            }
            let boundary_step = u32::try_from(bar_boundary)
                .unwrap_or(u32::MAX)
                .saturating_mul(steps_per_bar);
            scheduled.push(ScheduledFill {
                bar_boundary,
                phrase_lengths,
                section_boundary,
                start_step: boundary_step.saturating_sub(length),
                length,
            });
        }
        scheduled
    }
}
