//! Deterministic structural edits over [`Document`].
//!
//! Mutating operations validate that they did not introduce an over-full
//! voice before returning. Existing under-full voices are reported by the
//! inspection API and are not silently repaired.

use std::collections::BTreeMap;

use crate::inspect::{bar_capacity, bar_integrity, rhythm_duration, Fraction};
use crate::model::{Bar, Beat, Document, Note, Rhythm, Track, Voice};

/// Error returned when an edit would leave an over-full voice or has invalid indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// A requested index or range is invalid.
    InvalidRange(String),
    /// An edit introduced one or more over-full voices.
    Overfull(Vec<crate::inspect::BarIntegrity>),
}

type IntegrityKey = (usize, usize, usize);
type IntegrityState = BTreeMap<IntegrityKey, Fraction>;

fn overfull_state(doc: &Document) -> IntegrityState {
    bar_integrity(doc)
        .into_iter()
        .filter(|item| item.duration > item.capacity)
        .map(|item| {
            (
                (item.track_index, item.bar_index, item.voice_index),
                item.duration,
            )
        })
        .collect()
}

fn check(doc: &Document, before: &IntegrityState) -> Result<(), EditError> {
    let overfull = bar_integrity(doc)
        .into_iter()
        .filter(|item| item.duration > item.capacity)
        .filter(|item| {
            let key = (item.track_index, item.bar_index, item.voice_index);
            before
                .get(&key)
                .is_none_or(|duration| item.duration > *duration)
        })
        .collect::<Vec<_>>();
    if overfull.is_empty() {
        Ok(())
    } else {
        Err(EditError::Overfull(overfull))
    }
}
fn range(
    start: usize,
    end: usize,
    len: usize,
) -> Result<std::ops::RangeInclusive<usize>, EditError> {
    if start > end || end >= len {
        Err(EditError::InvalidRange(format!(
            "bar range {start}..={end} outside 0..{len}"
        )))
    } else {
        Ok(start..=end)
    }
}
fn next_id(doc: &Document) -> u32 {
    doc.bars
        .iter()
        .map(|bar| bar.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}
fn unique_name(doc: &Document, requested: &str, ignored: Option<usize>) -> String {
    let names = doc
        .tracks
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != ignored)
        .map(|(_, t)| t.name.as_str())
        .collect::<Vec<_>>();
    if !names.contains(&requested) {
        return requested.to_string();
    }
    (2..)
        .map(|n| format!("{requested} {n}"))
        .find(|candidate| !names.iter().any(|name| *name == candidate))
        .unwrap_or_else(|| format!("{requested} copy"))
}

/// Creates a bar with one empty voice and a fresh identity.
pub fn new_bar(doc: &Document) -> Bar {
    let voice_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .map(|voice| voice.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    Bar {
        id: next_id(doc),
        clef: None,
        voices: vec![Voice {
            id: voice_id,
            beats: Vec::new(),
        }],
    }
}
/// Creates a beat with a note or rest payload.
pub fn new_beat(id: u32, rhythm: Rhythm, notes: Vec<Note>) -> Beat {
    Beat {
        id,
        rhythm,
        notes,
        dynamic: None,
    }
}
/// Creates a note with the supplied pitch and string position.
pub fn new_note(id: u32, midi: Option<i32>, string: Option<u32>, fret: Option<i32>) -> Note {
    Note {
        id,
        midi,
        string,
        fret,
        articulation: None,
        techniques: Vec::new(),
    }
}

/// Adds a track, making its name unique.
pub fn create_track(doc: &mut Document, mut track: Track) -> usize {
    track.name = unique_name(doc, &track.name, None);
    track.id = doc
        .tracks
        .iter()
        .map(|item| item.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let index = doc.tracks.len();
    doc.tracks.push(track);
    for master in &mut doc.master_bars {
        master.bar_ids.push(-1);
    }
    index
}
/// Clones a track and all its referenced bars.
pub fn clone_track(doc: &mut Document, track_index: usize) -> Result<usize, EditError> {
    let before = overfull_state(doc);
    let source = doc
        .tracks
        .get(track_index)
        .cloned()
        .ok_or_else(|| EditError::InvalidRange("track index".into()))?;
    let mut clone = source;
    clone.name = unique_name(doc, &clone.name, None);
    clone.id = doc
        .tracks
        .iter()
        .map(|item| item.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let new_index = doc.tracks.len();
    doc.tracks.push(clone);
    let source_ids = doc
        .master_bars
        .iter()
        .map(|master| *master.bar_ids.get(track_index).unwrap_or(&-1))
        .collect::<Vec<_>>();
    let mut new_ids = Vec::with_capacity(source_ids.len());
    for id in source_ids {
        if id < 0 {
            new_ids.push(-1);
            continue;
        }
        let original = doc
            .bars
            .iter()
            .find(|bar| bar.id == id as u32)
            .cloned()
            .ok_or_else(|| EditError::InvalidRange("track bar reference".into()))?;
        let new_id = next_id(doc);
        let mut copied = original;
        copied.id = new_id;
        doc.bars.push(copied);
        new_ids.push(new_id as i32);
    }
    for (master, new_id) in doc.master_bars.iter_mut().zip(new_ids) {
        master.bar_ids.push(new_id);
    }
    check(doc, &before)?;
    Ok(new_index)
}
/// Removes a track and its bar references.
pub fn remove_track(doc: &mut Document, track_index: usize) -> Result<Track, EditError> {
    let before = overfull_state(doc);
    if track_index >= doc.tracks.len() {
        return Err(EditError::InvalidRange("track index".into()));
    }
    if doc.tracks.len() == 1 {
        return Err(EditError::InvalidRange(
            "cannot remove the last track".into(),
        ));
    }
    let removed_ids = doc
        .master_bars
        .iter()
        .filter_map(|master| master.bar_ids.get(track_index).copied())
        .collect::<std::collections::BTreeSet<_>>();
    let removed = doc.tracks.remove(track_index);
    for master in &mut doc.master_bars {
        if track_index < master.bar_ids.len() {
            master.bar_ids.remove(track_index);
        }
    }
    let remaining = doc
        .master_bars
        .iter()
        .flat_map(|master| master.bar_ids.iter())
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    doc.bars.retain(|bar| {
        !removed_ids.contains(&(bar.id as i32)) || remaining.contains(&(bar.id as i32))
    });
    check(doc, &before)?;
    Ok(removed)
}
/// Renames a track, adding a numeric suffix if needed to preserve uniqueness.
pub fn rename_track(doc: &mut Document, track_index: usize, name: &str) -> Result<(), EditError> {
    let unique = unique_name(doc, name, Some(track_index));
    let track = doc
        .tracks
        .get_mut(track_index)
        .ok_or_else(|| EditError::InvalidRange("track index".into()))?;
    track.name = unique;
    Ok(())
}
/// Reorders tracks and their per-master-bar references.
pub fn reorder_track(doc: &mut Document, from: usize, to: usize) -> Result<(), EditError> {
    let before = overfull_state(doc);
    if from >= doc.tracks.len() || to >= doc.tracks.len() {
        return Err(EditError::InvalidRange("track index".into()));
    }
    let track = doc.tracks.remove(from);
    doc.tracks.insert(to, track);
    for master in &mut doc.master_bars {
        let id = master.bar_ids.remove(from);
        master.bar_ids.insert(to, id);
    }
    check(doc, &before)
}

/// Returns an independent document containing inclusive master bars.
pub fn slice(doc: &Document, start: usize, end: usize) -> Result<Document, EditError> {
    let before = overfull_state(doc);
    let selected = range(start, end, doc.master_bars.len())?;
    let mut out = doc.clone();
    out.master_bars = selected
        .clone()
        .map(|i| {
            let mut master = doc.master_bars[i].clone();
            master.index = i - start;
            master
        })
        .collect();
    let ids = out
        .master_bars
        .iter()
        .flat_map(|m| m.bar_ids.iter())
        .copied()
        .filter(|id| *id >= 0)
        .map(|id| id as u32)
        .collect::<std::collections::BTreeSet<_>>();
    out.bars.retain(|bar| ids.contains(&bar.id));
    check(&out, &before)?;
    Ok(out)
}
/// Replaces a destination bar range with a source range of equal length.
pub fn splice(
    destination: &mut Document,
    dest_start: usize,
    dest_end: usize,
    source: &Document,
    source_start: usize,
) -> Result<(), EditError> {
    let before = overfull_state(destination);
    let count = dest_end
        .checked_sub(dest_start)
        .and_then(|x| x.checked_add(1))
        .ok_or_else(|| EditError::InvalidRange("destination range".into()))?;
    range(dest_start, dest_end, destination.master_bars.len())?;
    range(
        source_start,
        source_start + count - 1,
        source.master_bars.len(),
    )?;
    if destination.tracks.len() != source.tracks.len() {
        return Err(EditError::InvalidRange(
            "documents have different track counts".into(),
        ));
    }
    let source_slice = slice(source, source_start, source_start + count - 1)?;
    let mut next = next_id(destination);
    for offset in 0..count {
        destination.master_bars[dest_start + offset] = source_slice.master_bars[offset].clone();
        destination.master_bars[dest_start + offset].index = dest_start + offset;
        for track in 0..destination.tracks.len() {
            let source_id = source_slice.master_bars[offset].bar_ids[track];
            let copied = source_slice
                .bars
                .iter()
                .find(|bar| bar.id == source_id as u32)
                .cloned()
                .ok_or_else(|| EditError::InvalidRange("source bar reference".into()))?;
            let mut copied = copied;
            copied.id = next;
            next += 1;
            destination.bars.push(copied);
            destination.master_bars[dest_start + offset].bar_ids[track] = (next - 1) as i32;
        }
    }
    check(destination, &before)
}
/// Appends all bars from another document with matching track count.
pub fn append(destination: &mut Document, source: &Document) -> Result<(), EditError> {
    let before = overfull_state(destination);
    if destination.tracks.len() != source.tracks.len() {
        return Err(EditError::InvalidRange(
            "documents have different track counts".into(),
        ));
    }
    let source_part = slice(
        source,
        0,
        source
            .master_bars
            .len()
            .checked_sub(1)
            .ok_or_else(|| EditError::InvalidRange("source is empty".into()))?,
    )?;
    let mut next = next_id(destination);
    for mut master in source_part.master_bars {
        for id in &mut master.bar_ids {
            let copied = source_part
                .bars
                .iter()
                .find(|bar| bar.id == *id as u32)
                .cloned()
                .ok_or_else(|| EditError::InvalidRange("source bar reference".into()))?;
            let mut copied = copied;
            copied.id = next;
            *id = next as i32;
            next += 1;
            destination.bars.push(copied);
        }
        master.index = destination.master_bars.len();
        destination.master_bars.push(master);
    }
    check(destination, &before)
}

/// Sets or clears a section marker at a bar.
pub fn set_section(
    doc: &mut Document,
    bar: usize,
    section: Option<String>,
) -> Result<(), EditError> {
    let master = doc
        .master_bars
        .get_mut(bar)
        .ok_or_else(|| EditError::InvalidRange("bar index".into()))?;
    master.section = section;
    Ok(())
}
/// Patches a section marker's text without changing other master-bar data.
pub fn patch_section(
    doc: &mut Document,
    bar: usize,
    section: impl FnOnce(Option<&str>) -> Option<String>,
) -> Result<(), EditError> {
    let master = doc
        .master_bars
        .get_mut(bar)
        .ok_or_else(|| EditError::InvalidRange("bar index".into()))?;
    master.section = section(master.section.as_deref());
    Ok(())
}
/// Renames all section markers with one label.
pub fn rename_section(doc: &mut Document, old: &str, new: &str) {
    for master in &mut doc.master_bars {
        if master.section.as_deref() == Some(old) {
            master.section = Some(new.to_string());
        }
    }
}
/// Silences notes in an inclusive bar range while preserving rhythms and bars.
pub fn silence(doc: &mut Document, start: usize, end: usize) -> Result<(), EditError> {
    let before = overfull_state(doc);
    let selected = range(start, end, doc.master_bars.len())?;
    for index in selected {
        for id in doc.master_bars[index].bar_ids.clone() {
            if let Some(bar) = doc.bars.iter_mut().find(|bar| bar.id == id as u32) {
                for voice in &mut bar.voices {
                    for beat in &mut voice.beats {
                        beat.notes.clear();
                    }
                }
            }
        }
    }
    check(doc, &before)
}
/// Removes tracks with no sounding bars, preserving at least one track.
pub fn drop_empty_tracks(doc: &mut Document) -> Result<(), EditError> {
    let before = overfull_state(doc);
    let mut keep = Vec::new();
    for (i, track) in doc.tracks.iter().enumerate() {
        let sounding = doc.master_bars.iter().any(|master| {
            master.bar_ids.get(i).is_some_and(|id| {
                doc.bars
                    .iter()
                    .find(|bar| bar.id == *id as u32)
                    .is_some_and(|bar| {
                        bar.voices
                            .iter()
                            .any(|voice| voice.beats.iter().any(|beat| !beat.notes.is_empty()))
                    })
            })
        });
        if sounding {
            keep.push((i, track.clone()));
        }
    }
    if keep.is_empty() {
        let track = doc
            .tracks
            .first()
            .cloned()
            .ok_or_else(|| EditError::InvalidRange("document has no tracks".into()))?;
        keep.push((0, track));
    }
    let old = std::mem::take(&mut doc.tracks);
    let mut mapping = Vec::new();
    for (new, (old_index, mut track)) in keep.into_iter().enumerate() {
        track.id = new as u32;
        mapping.push((old_index, new));
        doc.tracks.push(track);
    }
    for master in &mut doc.master_bars {
        master.bar_ids = mapping
            .iter()
            .map(|(old_index, _)| master.bar_ids.get(*old_index).copied().unwrap_or(-1))
            .collect();
    }
    let used = doc
        .master_bars
        .iter()
        .flat_map(|m| m.bar_ids.iter())
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    doc.bars.retain(|bar| used.contains(&(bar.id as i32)));
    let _ = old;
    check(doc, &before)
}

/// Clamps one voice by dropping its final beats until it fits its capacity.
pub fn clamp_voice(
    doc: &mut Document,
    track: usize,
    bar: usize,
    voice: usize,
) -> Result<(), EditError> {
    let before = overfull_state(doc);
    let id = *doc
        .master_bars
        .get(bar)
        .and_then(|m| m.bar_ids.get(track))
        .ok_or_else(|| EditError::InvalidRange("bar or track index".into()))?;
    let capacity = bar_capacity(doc.master_bars[bar].time);
    let target = doc
        .bars
        .iter_mut()
        .find(|item| item.id == id as u32)
        .and_then(|item| item.voices.get_mut(voice))
        .ok_or_else(|| EditError::InvalidRange("voice index".into()))?;
    let mut total = Fraction::new(0, 1);
    for beat in &target.beats {
        total = total.plus(rhythm_duration(beat.rhythm));
    }
    while total > capacity {
        let beat = target
            .beats
            .pop()
            .ok_or_else(|| EditError::InvalidRange("empty voice".into()))?;
        total = Fraction::new(
            total.numerator * beat.rhythm.as_fraction().1
                - beat.rhythm.as_fraction().0 * total.denominator,
            total.denominator * beat.rhythm.as_fraction().1,
        );
    }
    check(doc, &before)
}
/// Asserts that the document has no over-full voice.
pub fn assert_no_overfull(doc: &Document) -> Result<(), EditError> {
    let empty = IntegrityState::new();
    check(doc, &empty)
}
