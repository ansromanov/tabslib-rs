use super::*;

use std::collections::BTreeMap;

#[test]
fn generator_metadata_round_trips_through_gp_payload() {
    let mut document = crate::fixtures::repeated_frets();
    document.tracks[0].generator_metadata = Some(crate::model::GeneratorMetadata {
        engine: "test.engine".into(),
        archetype: Some("test-role".into()),
        settings: BTreeMap::from([("seed".into(), "7".into())]),
    });
    let payload = super::super::write::write(&document);
    let restored = parse(&payload).expect("metadata payload parses");
    assert_eq!(
        restored.tracks[0].generator_metadata,
        document.tracks[0].generator_metadata
    );
}

#[test]
fn tempo_automations_round_trip_as_master_track_entries() {
    let document = crate::fixtures::tempo_changes();
    let payload = super::super::write::write(&document);
    let restored = parse(&payload).expect("tempo payload parses");
    assert_eq!(restored.tempo_map, document.tempo_map);
    let reencoded = super::super::write::write(&restored);
    assert_eq!(
        reencoded, payload,
        "re-encoding must be byte-identical, not just tempo-equal"
    );
}

#[test]
fn tempo_is_written_in_the_gp8_automation_spelling() {
    let document = crate::fixtures::tempo_changes();
    let payload = super::super::write::write(&document);
    assert!(
        payload.contains(
            "<MasterTrack><Tracks>0</Tracks><Automations><Automation><Type>Tempo</Type>\
             <Linear>false</Linear><Bar>0</Bar><Position>0</Position>\
             <Visible>true</Visible><Value>105 2</Value></Automation>"
        ),
        "earliest tempo must be a GP8-style element-form automation"
    );
    assert!(
        payload.contains("<Linear>true</Linear>"),
        "the gradual tempo must keep its linear flag"
    );
}

#[test]
fn tempo_is_read_from_older_attribute_spellings_too() {
    let document = crate::fixtures::tempo_changes();
    let payload = super::super::write::write(&document);
    let older = payload
        .replace(
            "<Automation><Type>Tempo</Type>",
            "<Automation type=\"Tempo\" id=\"Tempo-0\">",
        )
        .replace(
            "<Value>72 2</Value>",
            "<Value><TempoValue units=\"BPM\">72</TempoValue></Value>",
        );
    let restored = parse(&older).expect("attribute-form tempo parses");
    assert_eq!(restored.tempo_map, document.tempo_map);
}

/// A drum-kit instrument set whose articulations, in flattened order, sound
/// kick (36), snare (38) and closed hi-hat (42).
const KIT: &str = "<InstrumentSet><Name>Drums</Name><Type>drumKit</Type><Elements>\
    <Element><Name>Kick</Name><Articulations><Articulation><Name>Kick</Name>\
    <OutputMidiNumber>36</OutputMidiNumber></Articulation></Articulations></Element>\
    <Element><Name>Snare</Name><Articulations><Articulation><Name>Snare</Name>\
    <OutputMidiNumber>38</OutputMidiNumber></Articulation></Articulations></Element>\
    <Element><Name>Hat</Name><Articulations><Articulation><Name>Closed</Name>\
    <OutputMidiNumber>42</OutputMidiNumber></Articulation></Articulations></Element>\
    </Elements></InstrumentSet>";

#[test]
fn kit_articulation_indices_resolve_through_the_instrument_set() {
    // Notes name their instrument only by index into the kit, with no pitch:
    // without the kit table, index 1 is not a percussion key and the hit is
    // an unknown instrument.
    let mut document = crate::fixtures::drum_patterns();
    for note in document
        .bars
        .iter_mut()
        .flat_map(|bar| bar.voices.iter_mut())
        .flat_map(|voice| voice.beats.iter_mut())
        .flat_map(|beat| beat.notes.iter_mut())
    {
        note.articulation = match note.midi {
            Some(36) => Some(0),
            Some(38) => Some(1),
            _ => Some(2),
        };
        note.midi = None;
    }
    let payload =
        super::super::write::write(&document).replacen("</Name>", &format!("</Name>{KIT}"), 1);
    assert!(payload.contains(&format!("<Name><![CDATA[drums]]></Name>{KIT}")));
    let restored = parse(&payload).expect("kit payload parses");
    let table: Vec<(i32, u8)> = restored.tracks[0]
        .percussion_articulations
        .iter()
        .map(|mapping| (mapping.raw_id, mapping.midi))
        .collect();
    assert_eq!(table, [(0, 36), (1, 38), (2, 42)]);
}

#[test]
fn bend_values_written_as_decimals_are_read() {
    // Guitar Pro spells `<Float>100.000000</Float>`; reading only integers
    // turned every such bend into a flat one.
    let document = crate::fixtures::every_technique();
    let payload = super::super::write::write(&document)
        .replace("<Float>50</Float>", "<Float>50.000000</Float>")
        .replace("<Float>100</Float>", "<Float>100.000000</Float>");
    assert!(payload.contains("<Float>100.000000</Float>"));
    let restored = parse(&payload).expect("decimal bend payload parses");
    let bends: Vec<Technique> = restored
        .bars
        .iter()
        .flat_map(|bar| bar.voices.iter())
        .flat_map(|voice| voice.beats.iter())
        .flat_map(|beat| beat.notes.iter())
        .flat_map(|note| note.techniques.iter().copied())
        .filter(|technique| matches!(technique, Technique::Bend { .. }))
        .collect();
    let expected = Technique::Bend {
        origin: 0,
        middle: 50,
        dest: 100,
    };
    assert_eq!(bends, vec![expected; 4]);
}

#[test]
fn initial_sound_selected_by_bar_zero_sound_automation() {
    let document = crate::fixtures::repeated_frets();
    let payload = super::super::write::write(&document);
    let sounds_xml = r#"<Sounds>
        <Sound>
            <Name><![CDATA[Distortion Guitar]]></Name>
            <Label><![CDATA[Distortion Guitar]]></Label>
            <Path>Stringed/Electric Guitars/Distortion Guitar</Path>
            <Role>Factory</Role>
            <MIDI><Program>30</Program></MIDI>
        </Sound>
        <Sound>
            <Name><![CDATA[Overdriven Guitar]]></Name>
            <Label><![CDATA[Overdriven Guitar]]></Label>
            <Path>Stringed/Electric Guitars/Overdrive Guitar</Path>
            <Role>Factory</Role>
            <MIDI><Program>29</Program></MIDI>
        </Sound>
    </Sounds>
    <Automations>
        <Automation>
            <Type>Sound</Type>
            <Bar>0</Bar>
            <Position>0</Position>
            <Value><![CDATA[Stringed/Electric Guitars/Overdrive Guitar;Overdriven Guitar;Factory]]></Value>
        </Automation>
    </Automations>"#;
    let modified = payload.replace(
        "<Sounds><Sound><MIDI><Program>30</Program></MIDI></Sound></Sounds>",
        sounds_xml,
    );
    let restored = parse(&modified).expect("multi-sound payload parses");
    assert_eq!(
        restored.tracks[0].midi_program,
        Some(29),
        "bar-0 automation must select Overdriven Guitar (29) over first sound (30)"
    );
}

#[test]
fn initial_sound_selected_by_forced_sound_when_bar_zero_automation_absent() {
    let document = crate::fixtures::repeated_frets();
    let payload = super::super::write::write(&document);
    let sounds_xml = r#"<ForcedSound>1</ForcedSound>
    <Sounds>
        <Sound>
            <Name><![CDATA[Distortion Guitar]]></Name>
            <MIDI><Program>30</Program></MIDI>
        </Sound>
        <Sound>
            <Name><![CDATA[Clean Guitar]]></Name>
            <MIDI><Program>27</Program></MIDI>
        </Sound>
    </Sounds>"#;
    let modified = payload.replace(
        "<Sounds><Sound><MIDI><Program>30</Program></MIDI></Sound></Sounds>",
        sounds_xml,
    );
    let restored = parse(&modified).expect("forced sound payload parses");
    assert_eq!(
        restored.tracks[0].midi_program,
        Some(27),
        "ForcedSound must select index 1 (Clean Guitar: 27)"
    );
}

#[test]
fn initial_sound_falls_back_to_first_sound() {
    let document = crate::fixtures::repeated_frets();
    let payload = super::super::write::write(&document);
    let sounds_xml = r#"<Sounds>
        <Sound>
            <Name><![CDATA[Distortion Guitar]]></Name>
            <MIDI><Program>30</Program></MIDI>
        </Sound>
        <Sound>
            <Name><![CDATA[Clean Guitar]]></Name>
            <MIDI><Program>27</Program></MIDI>
        </Sound>
    </Sounds>"#;
    let modified = payload.replace(
        "<Sounds><Sound><MIDI><Program>30</Program></MIDI></Sound></Sounds>",
        sounds_xml,
    );
    let restored = parse(&modified).expect("fallback first sound payload parses");
    assert_eq!(
        restored.tracks[0].midi_program,
        Some(30),
        "when neither automation nor ForcedSound is present, first sound is selected"
    );
}

#[test]
fn program_and_channel_fall_back_to_older_producer_spellings() {
    let document = crate::fixtures::repeated_frets();
    let payload = super::super::write::write(&document);
    let gm_xml =
        r#"<GeneralMidi><Program>25</Program><PrimaryChannel>3</PrimaryChannel></GeneralMidi>"#;
    let modified = payload
        .replace(
            "<MidiConnection><PrimaryChannel>0</PrimaryChannel></MidiConnection>",
            gm_xml,
        )
        .replace(
            "<Sounds><Sound><MIDI><Program>30</Program></MIDI></Sound></Sounds>",
            "",
        );
    let restored = parse(&modified).expect("GeneralMidi fallback payload parses");
    assert_eq!(restored.tracks[0].midi_program, Some(25));
    assert_eq!(restored.tracks[0].midi_channel, Some(3));
}
