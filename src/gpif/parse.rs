use super::dom::{parse_xml, Element};
use crate::error::{Error, Result};
use crate::model::*;

fn ids(text: &str) -> Vec<i32> {
    text.split_whitespace().filter_map(|s| s.parse().ok()).collect()
}
fn num(el: Option<&Element>) -> Option<i32> {
    el.and_then(|e| e.text.trim().parse().ok())
}

/// `<Rhythms><Rhythm id="N">` -- keyed by id, referenced from beats as
/// `<Rhythm ref="N"/>`. Writing the index as element text instead produces a
/// file neither Guitar Pro nor a correct reader can resolve, and every note
/// silently becomes a quarter.
fn rhythm_table(root: &Element) -> Vec<(u32, Rhythm)> {
    root.child("Rhythms")
        .into_iter()
        .flat_map(|r| r.children_named("Rhythm"))
        .filter_map(|r| {
            let id: u32 = r.attr("id")?.parse().ok()?;
            let value = NoteValue::parse(r.child_text("NoteValue")?)?;
            let dots = r
                .child("AugmentationDot")
                .and_then(|d| d.attr("count").and_then(|c| c.parse().ok()))
                .unwrap_or(if r.child("AugmentationDot").is_some() { 1 } else { 0 });
            let tuplet = r.child("PrimaryTuplet").and_then(|t| {
                Some((t.attr("num")?.parse().ok()?, t.attr("den")?.parse().ok()?))
            });
            Some((id, Rhythm { value, dots, tuplet }))
        })
        .collect()
}

fn techniques_of(note: &Element) -> Vec<Technique> {
    let mut out = Vec::new();
    if note.has_property("PalmMuted") { out.push(Technique::PalmMute); }
    if note.has_property("Muted") { out.push(Technique::Dead); }
    if note.has_property("Tapped") { out.push(Technique::Tapped); }
    if note.has_property("HopoOrigin") { out.push(Technique::HammerOrigin); }
    if note.has_property("HopoDestination") { out.push(Technique::HammerDestination); }
    if note.has_property("LetRing") { out.push(Technique::LetRing); }
    if note.child("Vibrato").is_some() { out.push(Technique::Vibrato); }
    if let Some(p) = note.property("Slide") {
        let flags = p.child_text("Flags").and_then(|f| f.parse().ok()).unwrap_or(0);
        out.push(Technique::Slide { flags });
    }
    if note.has_property("Bended") || note.has_property("Bend") {
        let g = |n: &str| note.property(n).and_then(|p| num(p.children.first())).unwrap_or(0);
        out.push(Technique::Bend {
            origin: g("BendOriginValue"),
            middle: g("BendMiddleValue"),
            dest: g("BendDestinationValue"),
        });
    }
    if let Some(p) = note.property("HarmonicType") {
        let kind = match p.text.trim() {
            "Artificial" => HarmonicKind::Artificial,
            "Pinch" => HarmonicKind::Pinch,
            "Tap" => HarmonicKind::Tap,
            "Semi" => HarmonicKind::Semi,
            "Feedback" => HarmonicKind::Feedback,
            _ => HarmonicKind::Natural,
        };
        let fret = note.property("HarmonicFret").and_then(|f| num(f.children.first()));
        out.push(Technique::Harmonic { kind, fret });
    }
    out
}

pub fn parse(xml: &str) -> Result<Document> {
    let root = parse_xml(xml)?;
    if root.name != "GPIF" {
        return Err(Error::Malformed("root element is not <GPIF>".into()));
    }
    let score = root.child("Score");
    let mut doc = Document {
        title: score.and_then(|s| s.child_text("Title")).unwrap_or_default().to_string(),
        artist: score.and_then(|s| s.child_text("Artist")).unwrap_or_default().to_string(),
        ..Default::default()
    };

    for (i, t) in root.child("Tracks").into_iter().flat_map(|t| t.children_named("Track")).enumerate() {
        let color = t.child_text("Color").and_then(|c| {
            let v = ids(c);
            (v.len() >= 3).then(|| (v[0] as u8, v[1] as u8, v[2] as u8))
        });
        let tuning = t
            .children_named("Staves")
            .flat_map(|s| s.children_named("Staff"))
            .find_map(|s| s.property("Tuning").and_then(|p| p.child_text("Pitches").map(ids)))
            .unwrap_or_default();
        doc.tracks.push(Track {
            id: t.attr("id").and_then(|a| a.parse().ok()).unwrap_or(i as u32),
            name: t.child_text("Name").unwrap_or_default().to_string(),
            color,
            tuning,
            midi_program: t.child("MidiConnection").and_then(|m| num(m.child("Program"))),
        });
    }

    for (i, m) in root.child("MasterBars").into_iter().flat_map(|m| m.children_named("MasterBar")).enumerate() {
        let time = m
            .child_text("Time")
            .and_then(|t| {
                let (a, b) = t.split_once('/')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            })
            .unwrap_or((4, 4));
        doc.master_bars.push(MasterBar {
            index: i,
            time,
            section: m.child("Section").and_then(|s| s.child_text("Text")).map(str::to_string),
            double_bar: m.child("DoubleBar").is_some(),
            bar_ids: m.child_text("Bars").map(ids).unwrap_or_default(),
        });
    }

    let rhythms = rhythm_table(&root);

    let beat_els: Vec<&Element> =
        root.child("Beats").into_iter().flat_map(|b| b.children_named("Beat")).collect();
    let note_els: Vec<&Element> =
        root.child("Notes").into_iter().flat_map(|n| n.children_named("Note")).collect();
    let voice_els: Vec<&Element> =
        root.child("Voices").into_iter().flat_map(|v| v.children_named("Voice")).collect();

    // Index by id once. A linear scan per reference is O(n^2) and on an
    // 8000-note score that is the whole parse time.
    fn index(els: &[&Element]) -> std::collections::HashMap<i32, usize> {
        els.iter().enumerate()
            .filter_map(|(i, e)| Some((e.attr("id")?.parse::<i32>().ok()?, i)))
            .collect()
    }
    let beat_ix = index(&beat_els);
    let note_ix = index(&note_els);
    let voice_ix = index(&voice_els);
    let rhythm_ix: std::collections::HashMap<u32, Rhythm> = rhythms.iter().copied().collect();

    let build_note = |id: i32| -> Option<Note> {
        let n = note_els[*note_ix.get(&id)?];
        Some(Note {
            id: id as u32,
            midi: n.property("Midi").and_then(|p| num(p.child("Number"))),
            string: n.property("String").and_then(|p| num(p.child("String"))).map(|v| v as u32),
            fret: n.property("Fret").and_then(|p| num(p.child("Fret"))),
            articulation: num(n.child("InstrumentArticulation")),
            techniques: techniques_of(n),
        })
    };
    let build_beat = |id: i32| -> Option<Beat> {
        let b = beat_els[*beat_ix.get(&id)?];
        let r = b.child("Rhythm")?.attr("ref")?.parse::<u32>().ok()?;
        Some(Beat {
            id: id as u32,
            rhythm: rhythm_ix.get(&r).copied().unwrap_or(Rhythm::new(NoteValue::Quarter)),
            notes: b.child_text("Notes").map(ids).unwrap_or_default().into_iter().filter_map(build_note).collect(),
            dynamic: b.child_text("Dynamic").map(str::to_string),
        })
    };
    let build_voice = |id: i32| -> Option<Voice> {
        let v = voice_els[*voice_ix.get(&id)?];
        Some(Voice {
            id: id as u32,
            beats: v.child_text("Beats").map(ids).unwrap_or_default().into_iter().filter_map(build_beat).collect(),
        })
    };

    for b in root.child("Bars").into_iter().flat_map(|b| b.children_named("Bar")) {
        doc.bars.push(Bar {
            id: b.attr("id").and_then(|a| a.parse().ok()).unwrap_or(0),
            clef: b.child_text("Clef").map(str::to_string),
            voices: b.child_text("Voices").map(ids).unwrap_or_default()
                .into_iter().filter(|v| *v >= 0).filter_map(build_voice).collect(),
        });
    }
    Ok(doc)
}
