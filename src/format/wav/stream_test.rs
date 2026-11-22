use super::*;

use crate::format::wav::compile_performance_events;
use crate::format::wav::tests::MockSynth;

fn performance() -> CompiledPerformance {
    compile_performance_events(&crate::fixtures::repeated_frets()).expect("compiles")
}

fn stream(performance: &CompiledPerformance) -> Stream<MockSynth> {
    Stream::new(MockSynth::default(), performance)
}

#[test]
fn a_stream_renders_the_whole_performance_and_then_stops() {
    let performance = performance();
    let mut stream = stream(&performance);
    let mut pcm = Vec::new();
    let mut total = 0usize;
    loop {
        let written = stream.render(4_096, &mut pcm).expect("renders");
        if written == 0 {
            break;
        }
        total += written;
    }
    assert_eq!(
        u64::try_from(total).expect("fits"),
        performance.sample_count
    );
    assert_eq!(pcm.len(), total * 2, "output is interleaved stereo");
}

#[test]
fn rendering_in_pieces_matches_rendering_in_one_call() {
    // A piece that ends mid-note must leave the synthesizer ringing rather
    // than restart it: render the same performance in one call and in many
    // small ones, and compare every sample.
    let performance = performance();
    let frames = usize::try_from(performance.sample_count).expect("fits");

    let mut whole = Vec::new();
    stream(&performance)
        .render(frames, &mut whole)
        .expect("renders");

    let mut pieces = Vec::new();
    let mut streamed = stream(&performance);
    while streamed.render(777, &mut pieces).expect("renders") > 0 {}

    assert_eq!(pieces.len(), whole.len());
    assert!(
        pieces == whole,
        "piecewise synthesis diverged from the whole-score render"
    );
}

#[test]
fn a_seek_renders_the_same_samples_as_that_part_of_the_whole_score() {
    let performance = performance();
    let start = performance.sample_count / 2;
    let frames = 2_048usize;

    let mut whole = Vec::new();
    stream(&performance)
        .render(
            usize::try_from(performance.sample_count).expect("fits"),
            &mut whole,
        )
        .expect("renders");

    let mut tail = Vec::new();
    let mut seeked = stream(&performance);
    seeked.seek(start);
    assert_eq!(seeked.frame, start);
    assert_eq!(seeked.render(frames, &mut tail).expect("renders"), frames);

    let offset = usize::try_from(start).expect("fits") * 2;
    assert_eq!(
        tail,
        whole[offset..offset + tail.len()],
        "a seek must land on the audio the whole-score render has there"
    );
}

#[test]
fn a_seek_re_applies_the_program_changes_that_precede_it() {
    // Dropping them would restart every track on program 0: a distorted
    // guitar would play as an acoustic grand piano from the seek onwards.
    let performance = performance();
    let mut seeked = stream(&performance);
    seeked.seek(performance.sample_count / 2);
    let programs: Vec<u8> = seeked
        .backend
        .messages
        .iter()
        .filter_map(|event| match event.message {
            PerformanceMessage::ProgramChange { program } => Some(program),
            PerformanceMessage::NoteOn { .. } | PerformanceMessage::NoteOff { .. } => None,
        })
        .collect();
    let expected: Vec<u8> = performance
        .events
        .iter()
        .filter_map(|event| match event.message {
            PerformanceMessage::ProgramChange { program } => Some(program),
            PerformanceMessage::NoteOn { .. } | PerformanceMessage::NoteOff { .. } => None,
        })
        .collect();
    assert!(!expected.is_empty(), "the fixture selects an instrument");
    assert_eq!(programs, expected);
    assert!(
        seeked
            .backend
            .messages
            .iter()
            .all(|event| matches!(event.message, PerformanceMessage::ProgramChange { .. })),
        "a seek must not re-attack notes that were already sounding"
    );
}

#[test]
fn a_stream_seeked_past_the_end_renders_nothing() {
    let performance = performance();
    let mut streamed = stream(&performance);
    streamed.seek(performance.sample_count + 1);
    let mut pcm = Vec::new();
    assert_eq!(streamed.render(1_024, &mut pcm).expect("renders"), 0);
    assert!(pcm.is_empty());
}

#[test]
fn a_soundfont_rejects_bytes_that_are_not_a_bank() {
    let mut not_a_bank = std::io::Cursor::new(b"not a soundfont".as_slice());
    assert!(Soundfont::read(&mut not_a_bank).is_err());
}
