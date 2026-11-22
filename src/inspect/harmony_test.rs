use super::*;

fn emphasize_scale(doc: &mut Document, tonic: i32, mode: MusicalMode) {
    let intervals = mode.intervals();
    let mut pitches = vec![tonic; 14];
    pitches.extend(intervals[1..].iter().map(|interval| tonic + interval));
    let mut pitches = pitches.into_iter().cycle();
    for note in doc
        .bars
        .iter_mut()
        .flat_map(|bar| &mut bar.voices)
        .flat_map(|voice| &mut voice.beats)
        .flat_map(|beat| &mut beat.notes)
    {
        note.midi = pitches.next();
    }
}

#[test]
fn key_detection_distinguishes_all_supported_modes() {
    for mode in MODES {
        let mut score = crate::fixtures::repeated_frets();
        emphasize_scale(&mut score, 60, mode);
        let estimate = determine_key(&score).expect("pitched score has a key estimate");
        assert_eq!(estimate.key.tonic, 0, "mode {mode:?}");
        assert_eq!(estimate.mode, mode, "mode {mode:?}");
        assert_eq!(estimate.key.minor, mode.is_minor(), "mode {mode:?}");
        assert!(estimate.confidence > Fraction::new(0, 1));
        assert!(estimate.confidence_margin > Fraction::new(0, 1));
    }
}

#[test]
fn bar_chords_report_quality_and_diatonic_degree() {
    let mut score = crate::fixtures::repeated_frets();
    for bar in &mut score.bars {
        for beat in &mut bar.voices[0].beats {
            beat.notes.clear();
        }
        score_bass_triad(bar, [60, 64, 67]);
    }
    let chords = analyze_bar_chords(&score);
    let chord = chords[0].as_ref().expect("complete triad is detected");
    assert_eq!(chord.root, 0);
    assert_eq!(chord.quality, ChordQuality::Major);
    assert_eq!(chord.numeral.as_deref(), Some("I"));
    assert_eq!(chord.supporting_notes, 3);

    for bar in &mut score.bars {
        score_bass_triad(bar, [57, 60, 64]);
    }
    let chord = analyze_bar_chords_in_key(&score, 0, MusicalMode::Major)[0]
        .clone()
        .expect("minor triad is detected");
    assert_eq!(chord.root, 9);
    assert_eq!(chord.quality, ChordQuality::Minor);
    assert_eq!(chord.numeral.as_deref(), Some("vi"));

    for (pitches, quality, numeral) in [
        ([71, 74, 77], ChordQuality::Diminished, "vii°"),
        ([60, 64, 68], ChordQuality::Augmented, "I+"),
    ] {
        for bar in &mut score.bars {
            score_bass_triad(bar, pitches);
        }
        let chord = analyze_bar_chords_in_key(&score, 0, MusicalMode::Major)[0]
            .clone()
            .expect("triad quality is detected");
        assert_eq!(chord.quality, quality);
        assert_eq!(chord.numeral.as_deref(), Some(numeral));
    }
}

#[test]
fn section_harmony_counts_beat_changes_and_root_motion() {
    let mut score = crate::fixtures::repeated_frets();
    score.master_bars[0].section = Some("Verse".into());
    score.master_bars[2].section = Some("Chorus".into());
    for bar in &mut score.bars {
        for beat in &mut bar.voices[0].beats {
            beat.notes.clear();
        }
    }
    let verse_chords = [[60, 64, 67], [62, 65, 69]];
    for (bar, pitches) in score.bars.iter_mut().take(2).zip(verse_chords) {
        for beat in &mut bar.voices[0].beats {
            score_beat_triad(beat, pitches);
        }
    }
    let chorus_chords = [[60, 64, 67], [67, 71, 74]];
    for bar in score.bars.iter_mut().skip(2) {
        for (index, beat) in bar.voices[0].beats.iter_mut().enumerate() {
            score_beat_triad(beat, chorus_chords[index % chorus_chords.len()]);
        }
    }

    let sections = section_harmony(&score);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].harmonic_rhythm, Fraction::new(1, 2));
    assert!(sections[1].harmonic_rhythm > sections[0].harmonic_rhythm);
    assert!(sections[1].mean_root_motion > sections[0].mean_root_motion);
    assert_eq!(sections[1].max_root_motion, 5);
    assert_eq!(sections[1].most_frequent_root_relative, Some(0));
}

#[test]
fn section_harmony_accepts_caller_ranges() {
    let mut score = crate::fixtures::repeated_frets();
    for bar in &mut score.bars {
        for beat in &mut bar.voices[0].beats {
            score_beat_triad(beat, [60, 64, 67]);
        }
    }
    let sections = analyze_section_harmony(
        &score,
        &[HarmonySectionRange {
            label: Some("Caller".into()),
            start_bar: 1,
            end_bar: 3,
        }],
    );
    assert_eq!(sections[0].label.as_deref(), Some("Caller"));
    assert_eq!(sections[0].bars, 3);
    assert_eq!(sections[0].chord_changes, 0);
}

fn score_bass_triad(bar: &mut crate::model::Bar, pitches: [i32; 3]) {
    score_beat_triad(&mut bar.voices[0].beats[0], pitches);
}

fn score_beat_triad(beat: &mut crate::model::Beat, pitches: [i32; 3]) {
    let template = beat
        .notes
        .first()
        .cloned()
        .unwrap_or_else(|| crate::model::Note {
            id: 999,
            midi: Some(60),
            string: None,
            fret: None,
            articulation: None,
            techniques: Vec::new(),
        });
    beat.notes.clear();
    for (index, midi) in pitches.into_iter().enumerate() {
        let mut note = template.clone();
        note.id += index as u32;
        note.midi = Some(midi);
        beat.notes.push(note);
    }
}

fn paired_tracks(interval: i32) -> Document {
    let mut score = crate::fixtures::repeated_frets();
    let original_bars = score.bars.clone();
    score.tracks.push(score.tracks[0].clone());
    for (bar_index, bar) in score.master_bars.iter_mut().enumerate() {
        let copy = &original_bars[bar_index];
        let mut second = copy.clone();
        second.id += original_bars.len() as u32;
        for note in second
            .voices
            .iter_mut()
            .flat_map(|voice| &mut voice.beats)
            .flat_map(|beat| &mut beat.notes)
            .filter_map(|note| note.midi.as_mut())
        {
            *note = 60 + interval;
        }
        score.bars.push(second);
        bar.bar_ids.push((original_bars.len() + bar_index) as i32);
    }
    for bar in score.bars.iter_mut().take(original_bars.len()) {
        for note in bar
            .voices
            .iter_mut()
            .flat_map(|voice| &mut voice.beats)
            .flat_map(|beat| &mut beat.notes)
        {
            note.midi = Some(60);
        }
    }
    score
}

#[test]
fn track_friction_uses_overlapping_duration_and_interval_quality() {
    let dissonant = frictions_between(&paired_tracks(1), 0, 1)
        .expect("two distinct pitched tracks can be compared");
    assert!(dissonant.compared_pairs > 0);
    assert_eq!(dissonant.friction_pairs, dissonant.compared_pairs);
    assert_eq!(dissonant.friction_ratio, Fraction::new(1, 1));
    assert_eq!(dissonant.total_overlap, Fraction::new(5, 1));
    assert!(dissonant
        .events
        .iter()
        .all(|event| event.interval_semitones == 1));

    let consonant = frictions_between(&paired_tracks(7), 0, 1).expect("tracks are valid");
    assert_eq!(consonant.friction_pairs, 0);
    assert_eq!(consonant.friction_ratio, Fraction::new(0, 1));
    assert!(frictions_between(&paired_tracks(1), 0, 0).is_none());
}

#[test]
fn safe_roots_reject_semitone_friction() {
    let roots = safe_roots_for_bar(&[1]);
    assert!(!roots.contains(&0));
    assert!(roots.iter().all(|root| (0..12).contains(root)));
}

#[test]
fn progression_is_grouped_inside_section_boundaries() {
    let document = crate::fixtures::repeated_frets();
    let progressions = progression_for_sections(&document);
    assert!(!progressions.is_empty());
    assert!(progressions
        .iter()
        .all(|section| section.start_bar <= section.end_bar));
}
