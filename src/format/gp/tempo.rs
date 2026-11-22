//! Tempo automations on the `<MasterTrack>`: reading, writing and patching.
//!
//! The payload stores a tempo as `<Value>bpm unit</Value>`, where `unit` names
//! the beat the number counts: 1 eighth, 2 quarter, 3 dotted quarter, 4 half,
//! 5 dotted half. The model's [`TempoChange::bpm`] is always quarter notes per
//! minute, so the reader scales by the unit's length in quarters and the writer
//! emits the quarter-note unit `2`, which is what the originating application
//! writes for every tempo it saves.
//!
//! A change may sit anywhere inside a bar: `<Position>` is the fraction of the
//! bar already elapsed, from 0 to 1 inclusive. The model keeps one tempo per
//! bar line, so the reader snaps each change to the nearest bar line (a change
//! at or past the bar's midpoint takes effect at the next bar) and, when two
//! land on the same bar line, keeps the one later in the score. The earliest
//! change in bar 0 always governs bar 0, so a score opens with the first tempo
//! it marks even when that is written past the midpoint. A document
//! that is saved without its tempo map being edited keeps its original
//! automations byte-for-byte through source patching; an edited map replaces
//! them with bar-line changes.

use super::dom::Element;
use crate::error::{Error, Result};
use crate::model::TempoChange;

/// Length of a tempo reference unit in quarter notes.
fn unit_in_quarters(unit: &str) -> Result<f64> {
    match unit {
        "1" => Ok(0.5),
        "2" => Ok(1.0),
        "3" => Ok(1.5),
        "4" => Ok(2.0),
        "5" => Ok(3.0),
        other => Err(Error::Malformed(format!(
            "tempo reference unit {other:?} is not one of 1..=5"
        ))),
    }
}

/// Quarter-note BPM of one automation, tolerating both spellings: GP8's
/// `<Value>105 2</Value>` and the older `<Value><TempoValue>105</TempoValue></Value>`.
///
/// A value with no unit counts quarter notes, as the older spelling always does.
fn quarter_bpm(automation: &Element) -> Result<Option<f64>> {
    let Some(value) = automation.child("Value") else {
        return Ok(None);
    };
    let text = value.child_text("TempoValue").unwrap_or(value.text.trim());
    let mut tokens = text.split_whitespace();
    let Some(bpm) = tokens.next() else {
        return Ok(None);
    };
    let bpm: f64 = bpm
        .parse()
        .map_err(|_| Error::Malformed(format!("tempo value {bpm:?} is not a number")))?;
    let factor = match tokens.next() {
        Some(unit) => unit_in_quarters(unit)?,
        None => 1.0,
    };
    Ok(Some(bpm * factor))
}

/// Fraction of the bar elapsed before the change; absent means the bar line.
fn position(automation: &Element) -> Result<f64> {
    let Some(text) = automation.child_text("Position") else {
        return Ok(0.0);
    };
    match text.parse::<f64>() {
        Ok(value) if (0.0..=1.0).contains(&value) => Ok(value),
        _ => Err(Error::Malformed(format!(
            "tempo position {text:?} is not a fraction of the bar"
        ))),
    }
}

/// Tempo changes from `<MasterTrack><Automations>`: one per bar, sorted by bar.
///
/// `bar_count` is the number of master bars; a change snapped past the last
/// bar has no bar to govern and is dropped. A file with no `<MasterTrack>`
/// contributes nothing, so generated documents with an empty `tempo_map` stay
/// byte-identical to their previous writes.
pub(super) fn read(root: &Element, bar_count: usize) -> Result<Vec<TempoChange>> {
    let Some(automations) = root
        .child("MasterTrack")
        .and_then(|master| master.child("Automations"))
    else {
        return Ok(Vec::new());
    };
    // (written bar, position within it, change at its snapped bar)
    let mut found: Vec<(usize, f64, TempoChange)> = Vec::new();
    for automation in automations.children_named("Automation") {
        // GP8 spells the kind as a child element, GP7 as an attribute.
        let is_tempo = automation.child_text("Type").is_some_and(|t| t == "Tempo")
            || automation.attr("type") == Some("Tempo");
        if !is_tempo {
            continue;
        }
        let Some(bar) = automation.child_text("Bar") else {
            continue;
        };
        let bar: usize = bar
            .parse()
            .map_err(|_| Error::Malformed(format!("tempo bar {bar:?} is not an index")))?;
        let Some(bpm) = quarter_bpm(automation)? else {
            continue;
        };
        let position = position(automation)?;
        let snapped = bar.saturating_add(usize::from(position >= 0.5));
        let linear = automation.child_text("Linear").is_some_and(|v| v == "true");
        found.push((
            bar,
            position,
            TempoChange {
                at_bar: snapped,
                bpm,
                linear,
            },
        ));
    }
    // The score opens with the first tempo it marks: a change written late in
    // bar 0 with nothing before it governs bar 0 rather than leaving the
    // opening bars without a tempo.
    if let Some(first) = found
        .iter_mut()
        .filter(|entry| entry.0 == 0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        first.2.at_bar = 0;
    }
    found.sort_by(|a, b| {
        a.2.at_bar
            .cmp(&b.2.at_bar)
            .then(a.0.cmp(&b.0))
            .then(a.1.total_cmp(&b.1))
    });
    let mut out: Vec<TempoChange> = Vec::new();
    for (_, _, change) in found {
        let snapped = change.at_bar;
        if bar_count > 0 && snapped >= bar_count {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.at_bar == snapped => *last = change,
            _ => out.push(change),
        }
    }
    Ok(out)
}

/// The tempo map as GP8 element-form `<Automation>` entries, in map order.
pub(super) fn automations_xml(tempo_map: &[TempoChange]) -> String {
    tempo_map
        .iter()
        .map(|tempo| {
            format!(
                "<Automation><Type>Tempo</Type><Linear>{}</Linear><Bar>{}</Bar>\
                 <Position>0</Position><Visible>true</Visible><Value>{} 2</Value></Automation>",
                if tempo.linear { "true" } else { "false" },
                tempo.at_bar,
                tempo.bpm
            )
        })
        .collect()
}

/// Byte range of the `<MasterTrack>...</MasterTrack>` element, if present.
fn master_track_range(payload: &str) -> Option<std::ops::Range<usize>> {
    let start = payload.find("<MasterTrack>")?;
    let end = payload[start..].find("</MasterTrack>")? + start + "</MasterTrack>".len();
    Some(start..end)
}

/// The `<MasterTrack>...</MasterTrack>` slice of a payload, if present.
pub(super) fn master_track(payload: &str) -> Option<&str> {
    payload.get(master_track_range(payload)?)
}

/// Whether an `<Automation ...>...</Automation>` block is a tempo automation.
fn is_tempo_block(block: &str) -> bool {
    block.contains("<Type>Tempo</Type>") || block.starts_with("<Automation type=\"Tempo\"")
}

/// Replaces every tempo automation inside a source payload's `<MasterTrack>`
/// with `tempo_map`, leaving every other automation and byte untouched.
///
/// Returns `None` when the payload has no `<MasterTrack>` to patch, so the
/// caller can fall back to a full write.
pub(super) fn patch(payload: &str, tempo_map: &[TempoChange]) -> Option<String> {
    let range = master_track_range(payload)?;
    let master = payload.get(range.clone())?;
    let mut patched_master = String::with_capacity(master.len());
    let mut rest = master;
    loop {
        let open = [rest.find("<Automation>"), rest.find("<Automation ")]
            .into_iter()
            .flatten()
            .min();
        let Some(open) = open else {
            patched_master.push_str(rest);
            break;
        };
        let close = rest[open..].find("</Automation>")? + open + "</Automation>".len();
        patched_master.push_str(&rest[..open]);
        let block = &rest[open..close];
        if !is_tempo_block(block) {
            patched_master.push_str(block);
        }
        rest = &rest[close..];
    }
    let tempo = automations_xml(tempo_map);
    let patched_master = if let Some(at) = patched_master.find("<Automations>") {
        let at = at + "<Automations>".len();
        format!("{}{tempo}{}", &patched_master[..at], &patched_master[at..])
    } else if let Some(at) = patched_master.find("<Automations/>") {
        let end = at + "<Automations/>".len();
        format!(
            "{}<Automations>{tempo}</Automations>{}",
            &patched_master[..at],
            &patched_master[end..]
        )
    } else {
        let at = patched_master.rfind("</MasterTrack>")?;
        format!(
            "{}<Automations>{tempo}</Automations>{}",
            &patched_master[..at],
            &patched_master[at..]
        )
    };
    let mut output = String::with_capacity(payload.len() + tempo.len());
    output.push_str(payload.get(..range.start)?);
    output.push_str(&patched_master);
    output.push_str(payload.get(range.end..)?);
    Some(output)
}

#[cfg(test)]
#[path = "tempo_test.rs"]
mod tests;
