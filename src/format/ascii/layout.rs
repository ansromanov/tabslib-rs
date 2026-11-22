//! Structured tablature layout: where every bar, beat and note goes on a
//! character grid, without drawing it.
//!
//! Owns [`LayoutOptions`], [`TrackLayout`], [`BarLayout`], [`BeatLayout`],
//! [`layout_track`], [`beat_at`] and [`wrap`]. A renderer -- the plain-text one
//! in this crate, or a terminal UI that colours notes and highlights the
//! playing beat -- draws from this and makes no layout decisions of its own.
//!
//! # Column model
//!
//! A system of bars `a..b` is drawn as the line label padded to
//! [`TrackLayout::label_width`], one `|`, then for each bar
//! [`BarLayout::width`] columns followed by one `|`.
//! [`TrackLayout::system_width`] is exactly that sum. Inside a bar, a beat
//! occupies [`BeatLayout::width`] columns starting at [`BeatLayout::column`];
//! its cell text starts at the beat's first column.
//!
//! # Timing
//!
//! Onsets and durations are exact [`Fraction`]s of a **whole note**, measured
//! from the start of the bar, the unit [`crate::inspect::rhythm_duration`] and
//! [`crate::inspect::bar_capacity`] use. All voices are merged: a beat is every
//! distinct onset any voice starts a note or rest at, and it lasts until the
//! next onset or the end of the bar.
//!
//! # Spacing
//!
//! Every beat of a bar is at least `unit` columns wide, where `unit` is the
//! widest cell text in the bar plus [`LayoutOptions::gap`], and never less than
//! [`LayoutOptions::min_cell_width`]. With [`LayoutOptions::proportional`] set,
//! a beat's width is `unit` times its duration over the bar's reference
//! duration -- the shorter of its shortest beat and one beat of the meter --
//! rounded up and capped at [`LayoutOptions::max_ratio`] units. A quarter note
//! is therefore twice as wide as an eighth in the same bar, and an empty 4/4
//! bar is four units wide.

use std::ops::Range;

use super::cell::{self, Annotation, Cell, Connector};
use super::lines::{self, Line};
use crate::error::{Error, Result};
use crate::inspect::{bar_capacity, rhythm_duration, Fraction};
use crate::model::{Bar, Beat, Document, Rhythm, Technique, TempoChange, Track};

/// How [`layout_track`] spaces beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutOptions {
    /// Scale beat widths with their duration; `false` gives every beat of a
    /// bar the same width.
    pub proportional: bool,
    /// Fewest columns any beat may take, gap included.
    pub min_cell_width: usize,
    /// Blank columns kept after the widest cell text in a bar. At least one is
    /// always kept, so a [`Connector`] has a column to go in.
    pub gap: usize,
    /// Most units a single beat may take, so one long note beside a run of
    /// very short ones does not stretch the bar without bound.
    pub max_ratio: usize,
}

impl Default for LayoutOptions {
    /// Proportional, two-column minimum, one-column gap, ratio capped at 8.
    fn default() -> Self {
        Self {
            proportional: true,
            min_cell_width: 2,
            gap: 1,
            max_ratio: 8,
        }
    }
}

/// One beat: a distinct onset in the bar, across all voices.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatLayout {
    /// First column of the beat within its bar.
    pub column: usize,
    /// Columns the beat occupies.
    pub width: usize,
    /// Onset from the start of the bar, in whole notes.
    pub onset: Fraction,
    /// Time until the next beat or the end of the bar, in whole notes.
    pub duration: Fraction,
    /// Written rhythm of the lowest-numbered voice that starts a beat here.
    pub rhythm: Option<Rhythm>,
    /// Whether no voice sounds a note here.
    pub rest: bool,
    /// One entry per [`TrackLayout::lines`] entry; `None` where nothing sounds.
    pub cells: Vec<Option<Cell>>,
    /// Technique marks for the annotation rows, each at most once.
    pub annotations: Vec<Annotation>,
    /// Written dynamic of the first voice that states one here, such as `MF`.
    pub dynamic: Option<String>,
}

/// One bar of one track, with the score-wide marks of its master bar.
#[derive(Debug, Clone, PartialEq)]
pub struct BarLayout {
    /// Master-bar index in the document.
    pub index: usize,
    /// Columns of content, excluding the closing bar line.
    pub width: usize,
    /// Time signature as `(numerator, denominator)`.
    pub time: (u32, u32),
    /// Whether the time signature differs from the previous master bar's; always
    /// true for the first bar of the score.
    pub time_changed: bool,
    /// Bar length the meter allows, in whole notes.
    pub capacity: Fraction,
    /// Section marker starting here.
    pub section: Option<String>,
    /// Starts a repeated passage.
    pub repeat_start: bool,
    /// Ends a repeated passage; the value is the play count.
    pub repeat_end: Option<u32>,
    /// Alternate-ending numbers this bar belongs to, from 1.
    pub alternate_endings: Vec<u32>,
    /// Whether a double bar line is written here.
    pub double_bar: bool,
    /// Navigation direction, such as `Coda` or `Fine`.
    pub direction: Option<String>,
    /// Tempo change taking effect at this bar.
    pub tempo_change: Option<TempoChange>,
    /// Beats in time order.
    pub beats: Vec<BeatLayout>,
}

/// A laid-out track over a range of master bars.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackLayout {
    /// Track index in the document.
    pub track: usize,
    /// Track name.
    pub name: String,
    /// Whether the lines are drum lanes rather than strings.
    pub percussion: bool,
    /// Staff lines, top line first: strings highest-pitched first, or drum lanes.
    pub lines: Vec<Line>,
    /// One label per line: tuning note names, or drum instrument names.
    pub labels: Vec<String>,
    /// Laid-out bars, in score order.
    pub bars: Vec<BarLayout>,
    /// Notes that could not be placed: a string outside the tuning, a fretted
    /// note with no string, or a second note on a line and onset already taken
    /// by a lower-numbered voice.
    pub unplaced_notes: usize,
}

impl TrackLayout {
    /// Width of the widest line label.
    pub fn label_width(&self) -> usize {
        self.labels
            .iter()
            .map(|label| label.chars().count())
            .max()
            .unwrap_or(0)
    }

    /// Columns needed to draw `bars` (indices into [`TrackLayout::bars`]) as
    /// one system, labels and bar lines included.
    pub fn system_width(&self, bars: Range<usize>) -> usize {
        let content: usize = self
            .bars
            .get(bars)
            .unwrap_or(&[])
            .iter()
            .map(|bar| bar.width + 1)
            .sum();
        self.label_width() + 1 + content
    }

    /// Position in [`TrackLayout::bars`] of a master bar, if laid out.
    pub fn position_of(&self, master_bar: usize) -> Option<usize> {
        self.bars.iter().position(|bar| bar.index == master_bar)
    }
}

pub(crate) fn bar_for(doc: &Document, bar: usize, track: usize) -> Option<&Bar> {
    let id = *doc.master_bars.get(bar)?.bar_ids.get(track)?;
    let id = u32::try_from(id).ok()?;
    doc.bars.iter().find(|candidate| candidate.id == id)
}

fn minus(a: Fraction, b: Fraction) -> Fraction {
    Fraction::new(
        (a.numerator * b.denominator).saturating_sub(b.numerator * a.denominator),
        a.denominator * b.denominator,
    )
}

/// `ceil(unit * duration / reference)`, clamped to `unit..=unit * max_ratio`.
fn scaled_width(unit: usize, duration: Fraction, reference: Fraction, max_ratio: usize) -> usize {
    let cap = unit.saturating_mul(max_ratio.max(1));
    let (Ok(unit_wide), true) = (u128::try_from(unit), reference.numerator > 0) else {
        return unit;
    };
    let numerator = unit_wide * u128::from(duration.numerator) * u128::from(reference.denominator);
    let denominator = u128::from(duration.denominator) * u128::from(reference.numerator);
    let width = numerator.div_ceil(denominator.max(1));
    usize::try_from(width)
        .unwrap_or(cap)
        .clamp(unit, cap.max(unit))
}

/// Line a note belongs on, if it can be placed.
fn line_of(lines: &[Line], track: &Track, note: &crate::model::Note) -> Option<usize> {
    if lines::is_percussion(track) {
        let key = lines::drum_key(track, note);
        return lines
            .iter()
            .position(|line| matches!(line, Line::Drum { midi, .. } if *midi == key));
    }
    let string = note.string?;
    lines
        .iter()
        .position(|line| matches!(line, Line::String { string: s, .. } if *s == string))
}

struct BarContext<'a> {
    doc: &'a Document,
    track_index: usize,
    track: &'a Track,
    lines: &'a [Line],
    options: &'a LayoutOptions,
}

fn layout_bar(ctx: &BarContext<'_>, index: usize, unplaced: &mut usize) -> Option<BarLayout> {
    let master = ctx.doc.master_bars.get(index)?;
    let capacity = bar_capacity(master.time);
    let mut end = capacity;
    let mut events: Vec<(Fraction, usize, &Beat)> = Vec::new();
    if let Some(bar) = bar_for(ctx.doc, index, ctx.track_index) {
        for (voice_index, voice) in bar.voices.iter().enumerate() {
            let mut onset = Fraction::new(0, 1);
            for beat in &voice.beats {
                events.push((onset, voice_index, beat));
                onset = onset.plus(rhythm_duration(beat.rhythm));
            }
            end = end.max(onset);
        }
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut onsets: Vec<Fraction> = events.iter().map(|event| event.0).collect();
    onsets.dedup();
    if onsets.is_empty() {
        onsets.push(Fraction::new(0, 1));
    }

    let mut beats = Vec::with_capacity(onsets.len());
    for (slot, onset) in onsets.iter().enumerate() {
        let next = onsets.get(slot + 1).copied().unwrap_or(end);
        let mut beat = BeatLayout {
            column: 0,
            width: 0,
            onset: *onset,
            duration: minus(next, *onset),
            rhythm: None,
            rest: true,
            cells: vec![None; ctx.lines.len()],
            annotations: Vec::new(),
            dynamic: None,
        };
        for (_, voice, source) in events.iter().filter(|event| event.0 == *onset) {
            beat.rhythm = beat.rhythm.or(Some(source.rhythm));
            if beat.dynamic.is_none() {
                beat.dynamic = source.dynamic.clone();
            }
            for note in &source.notes {
                beat.rest = false;
                let Some(line) = line_of(ctx.lines, ctx.track, note) else {
                    *unplaced += 1;
                    continue;
                };
                let Some(slot) = beat.cells.get_mut(line) else {
                    *unplaced += 1;
                    continue;
                };
                if slot.is_some() {
                    *unplaced += 1;
                    continue;
                }
                *slot = Some(match ctx.lines.get(line) {
                    Some(Line::Drum { role, .. }) => cell::drum(note, *role, *voice),
                    Some(Line::String { .. }) | None => cell::fretted(note, *voice),
                });
                for annotation in cell::annotations(note) {
                    if !beat.annotations.contains(&annotation) {
                        beat.annotations.push(annotation);
                    }
                }
            }
        }
        beats.push(beat);
    }

    let widest = beats
        .iter()
        .flat_map(|beat| beat.cells.iter().flatten())
        .map(Cell::width)
        .max()
        .unwrap_or(1);
    let unit = (widest + ctx.options.gap.max(1)).max(ctx.options.min_cell_width);
    let beat_unit = Fraction::new(1, u64::from(master.time.1));
    let shortest = beats
        .iter()
        .map(|beat| beat.duration)
        .filter(|duration| duration.numerator > 0)
        .min()
        .unwrap_or(beat_unit);
    let reference = shortest.min(beat_unit);
    let mut column = 0;
    for beat in &mut beats {
        beat.width = if ctx.options.proportional {
            scaled_width(unit, beat.duration, reference, ctx.options.max_ratio)
        } else {
            unit
        };
        beat.column = column;
        column += beat.width;
    }

    let previous_time = index
        .checked_sub(1)
        .and_then(|previous| ctx.doc.master_bars.get(previous))
        .map(|previous| previous.time);
    Some(BarLayout {
        index,
        width: column,
        time: master.time,
        time_changed: previous_time != Some(master.time),
        capacity,
        section: master.section.clone(),
        repeat_start: master.repeat_start,
        repeat_end: master.repeat_end,
        alternate_endings: (0..32)
            .filter(|bit| master.alternate_ending & (1 << bit) != 0)
            .map(|bit| bit + 1)
            .collect(),
        double_bar: master.double_bar,
        direction: master.direction.clone(),
        tempo_change: ctx
            .doc
            .tempo_map
            .iter()
            .rfind(|change| change.at_bar == index)
            .copied(),
        beats,
    })
}

/// Fills hammer-on/pull-off and note-to-note slide connectors, which depend on
/// the next note on the same line. A note whose next note lies outside the
/// laid-out range keeps no connector.
fn connect(bars: &mut [BarLayout], line_count: usize) {
    for line in 0..line_count {
        let mut previous: Option<(usize, usize)> = None;
        for bar in 0..bars.len() {
            for beat in 0..bars[bar].beats.len() {
                let Some(fret) = bars[bar].beats[beat]
                    .cells
                    .get(line)
                    .and_then(|cell| cell.as_ref())
                    .map(|cell| cell.fret)
                else {
                    continue;
                };
                if let Some((from_bar, from_beat)) = previous {
                    if let Some(Some(origin)) = bars[from_bar].beats[from_beat].cells.get_mut(line)
                    {
                        let rising = fret.zip(origin.fret).map(|(to, from)| to >= from);
                        let falling = rising == Some(false);
                        let mut connector = origin.connector;
                        for technique in &origin.techniques {
                            match *technique {
                                Technique::HammerOrigin => {
                                    connector = Some(if falling {
                                        Connector::PullOff
                                    } else {
                                        Connector::HammerOn
                                    });
                                }
                                Technique::Slide { flags } => {
                                    if cell::slides_to_next(flags) {
                                        connector = Some(if falling {
                                            Connector::SlideDown
                                        } else {
                                            Connector::SlideUp
                                        });
                                    }
                                }
                                Technique::PalmMute
                                | Technique::Dead
                                | Technique::Accent
                                | Technique::Ghost
                                | Technique::Staccato
                                | Technique::LetRing
                                | Technique::TieOrigin
                                | Technique::TieDestination
                                | Technique::Vibrato
                                | Technique::Tapped
                                | Technique::HammerDestination
                                | Technique::Bend { .. }
                                | Technique::Harmonic { .. } => {}
                            }
                        }
                        origin.connector = connector;
                    }
                }
                previous = Some((bar, beat));
            }
        }
    }
}

/// Lays out one track over the inclusive master-bar range `start..=end`.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if `track` is not a track of `doc`, or if `start > end` or
/// `end` is past the last master bar.
pub fn layout_track(
    doc: &Document,
    track: usize,
    start: usize,
    end: usize,
    options: &LayoutOptions,
) -> Result<TrackLayout> {
    let track_data = doc
        .tracks
        .get(track)
        .ok_or_else(|| Error::Malformed(format!("track index {track} outside document")))?;
    if start > end || end >= doc.master_bars.len() {
        return Err(Error::Malformed(format!(
            "bar range {start}..={end} outside document"
        )));
    }
    let (lines, labels): (Vec<Line>, Vec<String>) = lines::track_lines(doc, track, track_data)
        .into_iter()
        .unzip();
    let ctx = BarContext {
        doc,
        track_index: track,
        track: track_data,
        lines: &lines,
        options,
    };
    let mut unplaced_notes = 0;
    let mut bars: Vec<BarLayout> = (start..=end)
        .filter_map(|index| layout_bar(&ctx, index, &mut unplaced_notes))
        .collect();
    connect(&mut bars, lines.len());
    Ok(TrackLayout {
        track,
        name: track_data.name.clone(),
        percussion: lines::is_percussion(track_data),
        lines,
        labels,
        bars,
        unplaced_notes,
    })
}

/// The beat sounding at `onset` (whole notes from the start of the bar): the
/// last beat starting at or before it. `None` past the end of the bar.
pub fn beat_at(bar: &BarLayout, onset: Fraction) -> Option<usize> {
    let last = bar.beats.last()?;
    if onset >= last.onset.plus(last.duration) {
        return None;
    }
    bar.beats.iter().rposition(|beat| beat.onset <= onset)
}

/// Splits the bars into systems no wider than `width` columns, as ranges into
/// [`TrackLayout::bars`]. A bar too wide for any system gets one to itself.
pub fn wrap(layout: &TrackLayout, width: usize) -> Vec<Range<usize>> {
    let mut systems = Vec::new();
    let mut start = 0;
    let base = layout.label_width() + 1;
    let mut used = base;
    for (index, bar) in layout.bars.iter().enumerate() {
        let needed = bar.width + 1;
        if index > start && used + needed > width {
            systems.push(start..index);
            start = index;
            used = base;
        }
        used += needed;
    }
    if start < layout.bars.len() {
        systems.push(start..layout.bars.len());
    }
    systems
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod tests;
