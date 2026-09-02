use crate::model::*;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
/// Guitar Pro reads section names only from CDATA; given a bare text node it
/// keeps the marker and discards the name. `]]>` has to be split across two
/// blocks, as any XML writer must.
fn cdata(s: &str) -> String {
    format!("<![CDATA[{}]]>", s.replace("]]>", "]]]]><![CDATA[>"))
}

/// Serialises a [`Document`] to a GPIF payload.
pub fn write(doc: &Document) -> String {
    // one entry per distinct rhythm, addressed by id
    let mut table: Vec<Rhythm> = Vec::new();
    let mut index_of = |r: Rhythm| -> usize {
        if let Some(i) = table.iter().position(|x| *x == r) {
            i
        } else {
            table.push(r);
            table.len() - 1
        }
    };

    let mut beats = String::new();
    let mut notes = String::new();
    let mut voices = String::new();
    let mut bars = String::new();
    let (mut bid, mut nid, mut vid) = (0u32, 0u32, 0u32);

    for bar in &doc.bars {
        let mut voice_ids = Vec::new();
        for voice in &bar.voices {
            let mut beat_ids = Vec::new();
            for beat in &voice.beats {
                let mut note_ids = Vec::new();
                for n in &beat.notes {
                    let mut props = String::new();
                    if let Some(f) = n.fret {
                        props.push_str(&format!(
                            "<Property name=\"Fret\"><Fret>{f}</Fret></Property>"
                        ));
                    }
                    if let Some(s) = n.string {
                        props.push_str(&format!(
                            "<Property name=\"String\"><String>{s}</String></Property>"
                        ));
                    }
                    if let Some(m) = n.midi {
                        props.push_str(&format!(
                            "<Property name=\"Midi\"><Number>{m}</Number></Property>"
                        ));
                    }
                    for t in &n.techniques {
                        props.push_str(&match t {
                            Technique::PalmMute => "<Property name=\"PalmMuted\"><Enable/></Property>".to_string(),
                            Technique::Dead => "<Property name=\"Muted\"><Enable/></Property>".to_string(),
                            Technique::Tapped => "<Property name=\"Tapped\"><Enable/></Property>".to_string(),
                            Technique::HammerOrigin => "<Property name=\"HopoOrigin\"><Enable/></Property>".to_string(),
                            Technique::HammerDestination => "<Property name=\"HopoDestination\"><Enable/></Property>".to_string(),
                            Technique::LetRing => "<Property name=\"LetRing\"><Enable/></Property>".to_string(),
                            Technique::Slide { flags } => format!("<Property name=\"Slide\"><Flags>{flags}</Flags></Property>"),
                            Technique::Bend { origin, middle, dest } => format!(
                                "<Property name=\"Bended\"><Enable/></Property>\
                                 <Property name=\"BendOriginValue\"><Float>{origin}</Float></Property>\
                                 <Property name=\"BendMiddleValue\"><Float>{middle}</Float></Property>\
                                 <Property name=\"BendDestinationValue\"><Float>{dest}</Float></Property>"),
                            Technique::Harmonic { kind, fret } => {
                                let k = match kind {
                                    HarmonicKind::Natural => "Natural", HarmonicKind::Artificial => "Artificial",
                                    HarmonicKind::Pinch => "Pinch", HarmonicKind::Tap => "Tap",
                                    HarmonicKind::Semi => "Semi", HarmonicKind::Feedback => "Feedback",
                                };
                                let f = fret.map(|v| format!("<Property name=\"HarmonicFret\"><HFret>{v}</HFret></Property>")).unwrap_or_default();
                                format!("<Property name=\"HarmonicType\">{k}</Property>{f}")
                            }
                            // notation-only marks carry no GPIF note property
                            Technique::Accent | Technique::Ghost | Technique::Staccato | Technique::Vibrato => String::new(),
                        });
                    }
                    let vib = if n.techniques.contains(&Technique::Vibrato) {
                        "<Vibrato>Slight</Vibrato>"
                    } else {
                        ""
                    };
                    let art = n
                        .articulation
                        .map(|a| format!("<InstrumentArticulation>{a}</InstrumentArticulation>"))
                        .unwrap_or_default();
                    notes.push_str(&format!(
                        "<Note id=\"{nid}\">{art}{vib}<Properties>{props}</Properties></Note>"
                    ));
                    note_ids.push(nid.to_string());
                    nid += 1;
                }
                let r = index_of(beat.rhythm);
                let dyn_ = beat
                    .dynamic
                    .as_deref()
                    .map(|d| format!("<Dynamic>{}</Dynamic>", esc(d)))
                    .unwrap_or_default();
                let nrefs = if note_ids.is_empty() {
                    String::new()
                } else {
                    format!("<Notes>{}</Notes>", note_ids.join(" "))
                };
                beats.push_str(&format!(
                    "<Beat id=\"{bid}\">{dyn_}<Rhythm ref=\"{r}\"/>{nrefs}</Beat>"
                ));
                beat_ids.push(bid.to_string());
                bid += 1;
            }
            voices.push_str(&format!(
                "<Voice id=\"{vid}\"><Beats>{}</Beats></Voice>",
                beat_ids.join(" ")
            ));
            voice_ids.push(vid.to_string());
            vid += 1;
        }
        while voice_ids.len() < 4 {
            voice_ids.push("-1".into());
        }
        let clef = bar
            .clef
            .as_deref()
            .map(|c| format!("<Clef>{}</Clef>", esc(c)))
            .unwrap_or_default();
        bars.push_str(&format!(
            "<Bar id=\"{}\">{clef}<Voices>{}</Voices></Bar>",
            bar.id,
            voice_ids.join(" ")
        ));
    }

    let rhythms: String = table
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let dot = if r.dots > 0 {
                format!("<AugmentationDot count=\"{}\"/>", r.dots)
            } else {
                String::new()
            };
            let tup = r
                .tuplet
                .map(|(n, d)| format!("<PrimaryTuplet num=\"{n}\" den=\"{d}\"/>"))
                .unwrap_or_default();
            format!(
                "<Rhythm id=\"{i}\"><NoteValue>{}</NoteValue>{dot}{tup}</Rhythm>",
                r.value.as_gpif()
            )
        })
        .collect();

    let master: String = doc
        .master_bars
        .iter()
        .map(|m| {
            // <Section> must precede <Bars>: the element is sequenced, and Guitar
            // Pro drops a section that arrives after the bar list.
            let sec = m
                .section
                .as_deref()
                .map(|s| {
                    format!(
                        "<Section><Letter>{}</Letter><Text>{}</Text></Section>",
                        cdata(""),
                        cdata(s)
                    )
                })
                .unwrap_or_default();
            let db = if m.double_bar { "<DoubleBar/>" } else { "" };
            let refs: Vec<String> = m.bar_ids.iter().map(|i| i.to_string()).collect();
            format!(
                "<MasterBar><Time>{}/{}</Time>{sec}{db}<Bars>{}</Bars></MasterBar>",
                m.time.0,
                m.time.1,
                refs.join(" ")
            )
        })
        .collect();

    let tracks: String = doc.tracks.iter().map(|t| {
        let color = t.color.map(|(r, g, b)| format!("<Color>{r} {g} {b}</Color>")).unwrap_or_default();
        let tuning = if t.tuning.is_empty() { String::new() } else {
            format!("<Properties><Property name=\"Tuning\"><Pitches>{}</Pitches></Property></Properties>",
                t.tuning.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(" "))
        };
        let prog = t.midi_program.map(|p| format!("<MidiConnection><Program>{p}</Program></MidiConnection>")).unwrap_or_default();
        format!("<Track id=\"{}\"><Name>{}</Name>{color}<Staves><Staff>{tuning}</Staff></Staves>{prog}</Track>",
            t.id, cdata(&t.name))
    }).collect();

    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<GPIF><GPVersion>8.1.4</GPVersion>\
<Score><Title>{}</Title><Artist>{}</Artist></Score>\
<Tracks>{tracks}</Tracks><MasterBars>{master}</MasterBars>\
<Bars>{bars}</Bars><Voices>{voices}</Voices><Beats>{beats}</Beats>\
<Notes>{notes}</Notes><Rhythms>{rhythms}</Rhythms></GPIF>",
        cdata(&doc.title),
        cdata(&doc.artist)
    )
}
