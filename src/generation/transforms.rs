//! Deterministic archetype transforms over selected score sections.

use crate::generation::solo::{generate_solo, SoloConfig};
use crate::inspect::Fraction;
use crate::model::{Beat, Document, NoteValue, Rhythm, Technique};
use crate::{Error, Result};

/// Named, style-neutral transform families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archetype {
    /// Guitar transforms.
    Guitar,
    /// Bass transforms.
    Bass,
    /// Drum transforms.
    Drums,
    /// Solo transforms.
    Solo,
}

/// Stable descriptions for articulation names exposed by the facade.
pub const ARTICULATION_DESCRIPTIONS: &[(&str, &str)] = &[
    ("accent", "accented attack"),
    ("ghost", "quiet attack"),
    ("palm_mute", "palm-muted attack"),
    ("staccato", "shortened detached attack"),
    ("octaves", "double the pitch at the octave"),
    ("gallop", "long-short-short rhythmic figure"),
    ("tremolo", "repeated rapid attack"),
    ("breakdown", "sparse accented low-register figure"),
    ("halftime", "slower repeated attack grid"),
    ("doubletime", "faster repeated attack grid"),
    ("ringing", "sustained ringing attack"),
    ("stabs", "short detached chord stabs"),
    ("arpeggiate", "one note of a chord at a time"),
    ("lowStringOnly", "retain only the lowest string"),
    ("pedal", "repeat the lowest pedal pitch"),
    ("sequence", "repeat the first pitch sequence"),
    ("fragmentation", "remove alternating attacks"),
    ("inversion", "invert chord note order"),
    ("retrograde", "reverse the written attack order"),
    ("interpolation", "add passing pitches between attacks"),
    ("shiftedAccents", "accent the offbeat attacks"),
    ("straightOpen", "move fretted notes to open strings"),
];

/// Applies one of the generic articulation transformations to a bar range.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if the range is empty or outside the document or `articulation`
/// is not one of the names in [`ARTICULATION_DESCRIPTIONS`]. Also returns [`Error::Malformed`] if
/// a rhythm articulation cannot fill a voice with plain note values; the document is then
/// unchanged.
pub fn apply_articulation(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    articulation: &str,
) -> Result<()> {
    validate_range(doc, start_bar, end_bar)?;
    if articulation == "octaves" {
        add_octaves(doc, start_bar, end_bar);
        return Ok(());
    }
    if matches!(articulation, "gallop" | "tremolo" | "breakdown") {
        apply_rhythm_articulation(doc, start_bar, end_bar, articulation)?;
    }
    if matches!(articulation, "halftime" | "doubletime") {
        apply_rhythm_articulation(doc, start_bar, end_bar, articulation)?;
    }
    if !matches!(
        articulation,
        "accent"
            | "ghost"
            | "palm_mute"
            | "staccato"
            | "gallop"
            | "tremolo"
            | "breakdown"
            | "halftime"
            | "doubletime"
    ) {
        apply_extended_articulation(doc, start_bar, end_bar, articulation)?;
        return Ok(());
    }
    let technique = match articulation {
        "staccato" | "gallop" | "tremolo" | "breakdown" | "halftime" | "doubletime" => {
            Technique::Staccato
        }
        "palm_mute" => Technique::PalmMute,
        "accent" => Technique::Accent,
        "ghost" => Technique::Ghost,
        other => return Err(Error::Malformed(format!("unknown articulation {other}"))),
    };
    for_each_note_in_range(doc, start_bar, end_bar, |note| {
        if !note.techniques.contains(&technique) {
            note.techniques.push(technique);
        }
    });
    Ok(())
}

fn apply_extended_articulation(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    articulation: &str,
) -> Result<()> {
    let mut beat_number = 0usize;
    for bar_index in start_bar..=end_bar {
        let mut reverse = Vec::new();
        for_each_beat_in_bar(doc, bar_index, |beat| {
            match articulation {
                "ringing" => {
                    beat.notes.iter_mut().for_each(|note| {
                        note.techniques.retain(|technique| {
                            !matches!(technique, Technique::Staccato | Technique::PalmMute)
                        });
                        if !note.techniques.contains(&Technique::LetRing) {
                            note.techniques.push(Technique::LetRing);
                        }
                    });
                }
                "stabs" => add_technique(&mut beat.notes, Technique::Staccato),
                "lowStringOnly" => {
                    let lowest = beat.notes.iter().filter_map(|note| note.string).min();
                    beat.notes.retain(|note| note.string == lowest);
                }
                "pedal" => {
                    if let Some(note) = beat.notes.iter().min_by_key(|note| note.midi).cloned() {
                        beat.notes = vec![note];
                    }
                }
                "fragmentation" if beat_number % 2 == 1 => beat.notes.clear(),
                "shiftedAccents" if beat_number % 2 == 1 => {
                    add_technique(&mut beat.notes, Technique::Accent)
                }
                "straightOpen" => {
                    beat.notes.iter_mut().for_each(|note| {
                        if note.string.is_some() {
                            note.fret = Some(0);
                        }
                    });
                }
                "inversion" => beat.notes.sort_by_key(|note| note.midi),
                "retrograde" => reverse.push(beat.notes.clone()),
                "sequence" => {
                    if beat_number % 2 == 1 {
                        let shift = 1.min(beat.notes.len());
                        beat.notes.rotate_left(shift);
                    }
                }
                "interpolation" => {
                    if let (Some(first), Some(last)) = (beat.notes.first(), beat.notes.last()) {
                        if first.id != last.id {
                            beat.notes[0].midi = first.midi.map(|pitch| pitch + 1);
                        }
                    }
                }
                "arpeggiate" => {
                    if beat.notes.len() > 1 {
                        beat.notes.truncate(1);
                    }
                }
                _ => return,
            }
            beat_number = beat_number.saturating_add(1);
        });
        if articulation == "retrograde" {
            let mut index = 0;
            for_each_beat_in_bar(doc, bar_index, |beat| {
                if let Some(notes) = reverse.iter().rev().nth(index) {
                    beat.notes = notes.clone();
                }
                index += 1;
            });
        }
    }
    if matches!(articulation, "sequence" | "interpolation" | "arpeggiate") {
        return Ok(());
    }
    if matches!(
        articulation,
        "ringing"
            | "stabs"
            | "lowStringOnly"
            | "pedal"
            | "fragmentation"
            | "inversion"
            | "retrograde"
            | "shiftedAccents"
            | "straightOpen"
    ) {
        Ok(())
    } else {
        Err(Error::Malformed(format!(
            "unknown articulation {articulation}"
        )))
    }
}

fn add_technique(notes: &mut [crate::model::Note], technique: Technique) {
    for note in notes {
        if !note.techniques.contains(&technique) {
            note.techniques.push(technique);
        }
    }
}

/// Rewrites each voice in range as a repeated rhythm cell that fills exactly
/// the voice's written length, re-attacking whatever pitch was sounding at
/// each new onset. Every voice is planned before any is replaced, so an
/// error leaves the document untouched.
fn apply_rhythm_articulation(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    articulation: &str,
) -> Result<()> {
    let cell: &[NoteValue] = match articulation {
        "gallop" => &[
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::Sixteenth,
        ],
        "tremolo" => &[NoteValue::ThirtySecond],
        "halftime" => &[NoteValue::Half],
        "doubletime" => &[NoteValue::Eighth],
        _ => &[NoteValue::Quarter],
    };
    let mut ids = next_ids(doc);
    let mut planned = Vec::new();
    for bar_index in start_bar..=end_bar {
        let Some(master) = doc.master_bars.get(bar_index) else {
            continue;
        };
        for bar_id in &master.bar_ids {
            let Ok(bar_id) = u32::try_from(*bar_id) else {
                continue;
            };
            let Some(bar) = doc.bars.iter().find(|bar| bar.id == bar_id) else {
                continue;
            };
            for (voice_index, voice) in bar.voices.iter().enumerate() {
                let beats = refill_voice(&voice.beats, cell, articulation == "breakdown", &mut ids)
                    .map_err(|message| {
                        Error::Malformed(format!("bar {}: {message}", bar_index + 1))
                    })?;
                planned.push((bar_id, voice_index, beats));
            }
        }
    }
    for (bar_id, voice_index, beats) in planned {
        if let Some(voice) = doc
            .bars
            .iter_mut()
            .find(|bar| bar.id == bar_id)
            .and_then(|bar| bar.voices.get_mut(voice_index))
        {
            voice.beats = beats;
        }
    }
    Ok(())
}

fn refill_voice(
    beats: &[Beat],
    cell: &[NoteValue],
    accent_downbeat: bool,
    ids: &mut (u32, u32),
) -> std::result::Result<Vec<Beat>, String> {
    let spans = beat_spans(beats);
    let total = spans.last().map_or(Fraction::new(0, 1), |(_, end, _)| *end);
    if total.numerator == 0 {
        return Ok(beats.to_vec());
    }
    if !total.denominator.is_power_of_two() || total.denominator > 64 {
        return Err(format!(
            "voice length {}/{} cannot be filled with plain note values",
            total.numerator, total.denominator
        ));
    }
    let mut out = Vec::new();
    let mut at = Fraction::new(0, 1);
    let mut step = 0;
    while at < total {
        let remaining = minus(total, at);
        let mut value = cell[step % cell.len()];
        while value_length(value) > remaining {
            value = shorter(value).ok_or("remaining length is shorter than a 64th")?;
        }
        let source = spans
            .iter()
            .find(|(start, end, _)| *start <= at && at < *end)
            .map(|(_, _, beat)| *beat)
            .ok_or("no written beat covers an onset")?;
        let notes = source
            .notes
            .iter()
            .map(|note| {
                let mut copy = note.clone();
                copy.id = ids.1;
                ids.1 = ids.1.saturating_add(1);
                copy.techniques.retain(|technique| {
                    !matches!(technique, Technique::TieOrigin | Technique::TieDestination)
                });
                copy
            })
            .collect();
        let dynamic = if accent_downbeat && out.is_empty() {
            Some("FF".to_string())
        } else {
            source.dynamic.clone()
        };
        out.push(Beat {
            id: ids.0,
            rhythm: Rhythm::new(value),
            notes,
            dynamic,
        });
        ids.0 = ids.0.saturating_add(1);
        at = at.plus(value_length(value));
        step += 1;
    }
    Ok(out)
}

fn beat_spans(beats: &[Beat]) -> Vec<(Fraction, Fraction, &Beat)> {
    let mut at = Fraction::new(0, 1);
    beats
        .iter()
        .map(|beat| {
            let (numerator, denominator) = beat.rhythm.as_fraction();
            let end = at.plus(Fraction::new(numerator, denominator));
            let span = (at, end, beat);
            at = end;
            span
        })
        .collect()
}

fn value_length(value: NoteValue) -> Fraction {
    Fraction::new(1, u64::from(value.denominator()))
}

fn shorter(value: NoteValue) -> Option<NoteValue> {
    match value {
        NoteValue::Whole => Some(NoteValue::Half),
        NoteValue::Half => Some(NoteValue::Quarter),
        NoteValue::Quarter => Some(NoteValue::Eighth),
        NoteValue::Eighth => Some(NoteValue::Sixteenth),
        NoteValue::Sixteenth => Some(NoteValue::ThirtySecond),
        NoteValue::ThirtySecond => Some(NoteValue::SixtyFourth),
        NoteValue::SixtyFourth => None,
    }
}

fn minus(left: Fraction, right: Fraction) -> Fraction {
    Fraction::new(
        left.numerator * right.denominator - right.numerator * left.denominator,
        left.denominator * right.denominator,
    )
}

fn next_ids(doc: &Document) -> (u32, u32) {
    let beats = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter());
    let beat = beats.clone().map(|beat| beat.id).max().unwrap_or(0);
    let note = beats
        .flat_map(|beat| beat.notes.iter())
        .map(|note| note.id)
        .max()
        .unwrap_or(0);
    (beat.saturating_add(1), note.saturating_add(1))
}

fn add_octaves(doc: &mut Document, start_bar: usize, end_bar: usize) {
    let mut next_id = doc
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .map(|note| note.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    for bar_index in start_bar..=end_bar {
        for_each_beat_in_bar(doc, bar_index, |beat| {
            let added = beat
                .notes
                .iter()
                .filter_map(|note| {
                    let midi = note.midi?.checked_add(12)?;
                    let mut octave = note.clone();
                    octave.id = next_id;
                    next_id = next_id.saturating_add(1);
                    octave.midi = Some(midi);
                    octave.fret = octave.fret.and_then(|fret| fret.checked_add(12));
                    Some(octave)
                })
                .collect::<Vec<_>>();
            beat.notes.extend(added);
        });
    }
}

/// Applies a linear dynamic change to an inclusive bar range.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if the range is empty or outside the document.
pub fn patch_score_section(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    intensity_offset: i8,
    louder_end: bool,
) -> Result<()> {
    validate_range(doc, start_bar, end_bar)?;
    let length = end_bar - start_bar;
    for bar_index in start_bar..=end_bar {
        let progress = if length == 0 {
            0
        } else {
            match ((bar_index - start_bar) * 100).checked_div(length) {
                Some(value) => u8::try_from(value).unwrap_or(100),
                None => 100,
            }
        };
        let signed_progress = if louder_end {
            i16::from(progress)
        } else {
            100 - i16::from(progress)
        };
        let shift = i16::from(intensity_offset) + signed_progress / 10;
        for_each_beat_in_bar(doc, bar_index, |beat| {
            let current = beat.dynamic.as_deref().and_then(dynamic_level).unwrap_or(3);
            beat.dynamic = Some(dynamic_name((current + shift).clamp(0, 6)).to_string());
        });
    }
    Ok(())
}

/// Applies a solo archetype to an inclusive bar range in place.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if the range is empty or outside the document. Also returns
/// any error from [`generate_solo`] for the resulting configuration.
pub fn apply_solo_archetype(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    mut config: SoloConfig,
) -> Result<()> {
    if start_bar > end_bar || end_bar >= doc.master_bars.len() {
        return Err(Error::Malformed("invalid transform range".into()));
    }
    config.start_bar = start_bar;
    config.end_bar = end_bar;
    generate_solo(doc, &config).map(|_| ())
}

/// Applies a deterministic, style-neutral archetype transform.
///
/// # Errors
///
/// Returns [`Error::Malformed`] if `archetype` is [`Archetype::Solo`] and `solo` is `None`.
/// Otherwise returns any error from [`apply_solo_archetype`] or [`apply_articulation`].
pub fn transform_score(
    doc: &mut Document,
    archetype: Archetype,
    start_bar: usize,
    end_bar: usize,
    solo: Option<SoloConfig>,
) -> Result<()> {
    match archetype {
        Archetype::Solo => apply_solo_archetype(
            doc,
            start_bar,
            end_bar,
            solo.ok_or_else(|| Error::Malformed("solo transform needs a configuration".into()))?,
        ),
        Archetype::Guitar => apply_articulation(doc, start_bar, end_bar, "octaves"),
        Archetype::Bass => {
            apply_articulation(doc, start_bar, end_bar, "staccato")?;
            apply_articulation(doc, start_bar, end_bar, "palm_mute")
        }
        Archetype::Drums => apply_articulation(doc, start_bar, end_bar, "breakdown"),
    }
}

fn validate_range(doc: &Document, start_bar: usize, end_bar: usize) -> Result<()> {
    if start_bar > end_bar || end_bar >= doc.master_bars.len() {
        return Err(Error::Malformed("invalid transform range".into()));
    }
    Ok(())
}

fn for_each_note_in_range(
    doc: &mut Document,
    start_bar: usize,
    end_bar: usize,
    mut apply: impl FnMut(&mut crate::model::Note),
) {
    for bar_index in start_bar..=end_bar {
        for_each_note_in_bar(doc, bar_index, &mut apply);
    }
}

fn for_each_note_in_bar(
    doc: &mut Document,
    bar_index: usize,
    mut apply: impl FnMut(&mut crate::model::Note),
) {
    for_each_beat_in_bar(doc, bar_index, |beat| {
        for note in &mut beat.notes {
            apply(note);
        }
    });
}

fn for_each_beat_in_bar(
    doc: &mut Document,
    bar_index: usize,
    mut apply: impl FnMut(&mut crate::model::Beat),
) {
    let Some(master) = doc.master_bars.get(bar_index).cloned() else {
        return;
    };
    for id in master.bar_ids {
        let Ok(id) = u32::try_from(id) else { continue };
        let Some(bar) = doc.bars.iter_mut().find(|bar| bar.id == id) else {
            continue;
        };
        for voice in &mut bar.voices {
            for beat in &mut voice.beats {
                apply(beat);
            }
        }
    }
}

fn dynamic_level(dynamic: &str) -> Option<i16> {
    Some(match dynamic {
        "PPP" => 0,
        "PP" => 1,
        "P" => 2,
        "MF" => 3,
        "F" => 4,
        "FF" => 5,
        "FFF" => 6,
        _ => return None,
    })
}

fn dynamic_name(level: i16) -> &'static str {
    match level {
        0 => "PPP",
        1 => "PP",
        2 => "P",
        3 => "MF",
        4 => "F",
        5 => "FF",
        _ => "FFF",
    }
}

#[cfg(test)]
#[path = "transforms_test.rs"]
mod tests;
