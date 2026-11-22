//! Compile score documents to timed events and render them through SoundFont2.
//!
//! [`compile_performance_events`] expands score playback order into stable
//! sample-positioned MIDI-like events. [`render_pcm`] synthesizes those events
//! with a caller-supplied SoundFont2 file and writes stereo 16-bit PCM WAV.
//!
//! [`PerformanceStream`] renders the same events a range of frames at a time,
//! from any starting frame, for callers that cannot wait for a whole score.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::inspect;
use crate::model::{Document, Technique};
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

#[path = "wav/stream.rs"]
mod stream;

pub use stream::{PerformanceStream, Soundfont};

/// Output sample rate used by the offline renderer.
pub const SAMPLE_RATE: u32 = 44_100;

/// A MIDI-like message emitted at a sample position in playback order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformanceMessage {
    /// Select a General MIDI program on a channel.
    ProgramChange {
        /// Zero-based General MIDI program number.
        program: u8,
    },
    /// Start a pitched or percussion note.
    NoteOn {
        /// MIDI key or percussion articulation number.
        note: u8,
        /// MIDI velocity from 1 through 127.
        velocity: u8,
    },
    /// Stop a note previously started on this channel.
    NoteOff {
        /// MIDI key or percussion articulation number.
        note: u8,
    },
}

/// One event in the compiled score performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerformanceEvent {
    /// Zero-based PCM sample frame at which the event occurs.
    pub sample: u64,
    /// Zero-based source track index.
    pub track_index: usize,
    /// MIDI channel; channel 9 is reserved for percussion.
    pub channel: u8,
    /// Event to send to a MIDI synthesizer.
    pub message: PerformanceMessage,
}

/// Compiled events and the total score duration in sample frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPerformance {
    /// Events sorted by sample, message priority, track, and note.
    pub events: Vec<PerformanceEvent>,
    /// Duration of the playback, including expanded repeats.
    pub sample_count: u64,
}

/// Compiles tracks, tempo changes, note values, dynamics, and repeats to events.
///
/// Missing bars and pitchless notes are rests. Unsupported dynamics, invalid
/// pitches, invalid tempo values, and track counts that cannot fit MIDI
/// channels return an error rather than silently changing the score.
///
/// # Errors
///
/// Returns [`crate::Error::Malformed`] if a tempo change names a missing bar or has a
/// non-positive or non-finite value, a track program or note pitch is outside the MIDI range, a
/// bar has a zero meter, a bar reference is missing or invalid, there are more than 15 pitched
/// tracks, a dynamic is unsupported, or a rhythm or the score duration is invalid.
pub fn compile_performance_events(document: &Document) -> Result<CompiledPerformance> {
    validate_tempos(document)?;
    let audible_tracks = audible_tracks(document);
    let channels = track_channels(document, &audible_tracks)?;
    let playback = inspect::playback_order(document);
    let mut events = Vec::new();
    let mut elapsed_seconds = 0.0f64;
    let mut voice_velocities = vec![80u8; document.tracks.len()];

    for &track_index in &audible_tracks {
        let track = document
            .tracks
            .get(track_index)
            .ok_or_else(|| Error::Malformed("track index is outside the document".into()))?;
        let program = track.midi_program.unwrap_or(0);
        let program = u8::try_from(program)
            .ok()
            .filter(|value| *value <= 127)
            .ok_or_else(|| Error::Malformed("track program is outside MIDI range".into()))?;
        if let Some(&channel) = channels.get(track_index) {
            events.push(PerformanceEvent {
                sample: 0,
                track_index,
                channel,
                message: PerformanceMessage::ProgramChange { program },
            });
        }
    }

    for &bar_index in &playback {
        let master = document
            .master_bars
            .get(bar_index)
            .ok_or_else(|| Error::Malformed("playback references a missing master bar".into()))?;
        if master.time.0 == 0 || master.time.1 == 0 {
            return Err(Error::Malformed("master bar has an invalid meter".into()));
        }
        let tempo = tempo_at(document, bar_index);
        let bar_start = elapsed_seconds;

        for &track_index in &audible_tracks {
            let Some(&channel) = channels.get(track_index) else {
                return Err(Error::Malformed("track has no MIDI channel".into()));
            };
            let bar_id = master.bar_ids.get(track_index).copied().unwrap_or(-1);
            if bar_id == -1 {
                continue;
            }
            let bar_id = u32::try_from(bar_id)
                .map_err(|_| Error::Malformed("track references an invalid bar id".into()))?;
            let bar = document
                .bars
                .iter()
                .find(|candidate| candidate.id == bar_id)
                .ok_or_else(|| Error::Malformed("track references a missing bar".into()))?;

            for (voice_index, voice) in bar.voices.iter().enumerate() {
                let velocity_slot = track_index
                    .checked_add(voice_index.saturating_mul(document.tracks.len()))
                    .ok_or_else(|| Error::Malformed("too many score voices".into()))?;
                if voice_velocities.len() <= velocity_slot {
                    voice_velocities.resize(velocity_slot + 1, 80);
                }
                let mut onset_whole_notes = 0.0f64;
                for beat in &voice.beats {
                    if let Some(dynamic) = beat.dynamic.as_deref() {
                        voice_velocities[velocity_slot] = dynamic_velocity(dynamic)?;
                    }
                    let beat_duration = fraction_as_f64(beat.rhythm.as_fraction())?;
                    let onset_seconds =
                        bar_start + whole_notes_to_seconds(onset_whole_notes, tempo);
                    let duration_seconds = whole_notes_to_seconds(beat_duration, tempo);
                    let velocity = voice_velocities[velocity_slot];
                    for note in &beat.notes {
                        let midi = if channel == 9 {
                            note.articulation
                                .and_then(|raw| {
                                    document
                                        .tracks
                                        .get(track_index)?
                                        .percussion_articulations
                                        .iter()
                                        .find(|mapping| mapping.raw_id == raw)
                                        .map(|mapping| i32::from(mapping.midi))
                                })
                                .or(note.midi)
                                .or_else(|| {
                                    note.articulation
                                        .filter(|raw| crate::percussion::midi_note(*raw).is_some())
                                })
                        } else {
                            note.midi
                        };
                        let Some(midi) = midi else {
                            continue;
                        };
                        let midi = u8::try_from(midi)
                            .ok()
                            .filter(|value| *value <= 127)
                            .ok_or_else(|| {
                                Error::Malformed("note pitch is outside MIDI range".into())
                            })?;
                        let mut note_velocity = velocity;
                        for technique in &note.techniques {
                            match technique {
                                Technique::Accent => {
                                    note_velocity = note_velocity.saturating_add(24).min(127)
                                }
                                Technique::Ghost => {
                                    note_velocity = note_velocity.saturating_sub(32).max(1)
                                }
                                Technique::Dead => note_velocity = note_velocity.min(24),
                                Technique::PalmMute | Technique::Staccato => {}
                                Technique::LetRing
                                | Technique::TieOrigin
                                | Technique::TieDestination
                                | Technique::Vibrato
                                | Technique::Tapped
                                | Technique::HammerOrigin
                                | Technique::HammerDestination
                                | Technique::Slide { .. }
                                | Technique::Bend { .. }
                                | Technique::Harmonic { .. } => {}
                            }
                        }
                        if note_velocity == 0 {
                            continue;
                        }
                        let note_duration = if note.techniques.iter().any(|technique| {
                            matches!(technique, Technique::Staccato | Technique::PalmMute)
                        }) {
                            duration_seconds * 0.5
                        } else {
                            duration_seconds
                        };
                        let start_sample = seconds_to_samples(onset_seconds)?;
                        let end_sample = seconds_to_samples(onset_seconds + note_duration)?;
                        events.push(PerformanceEvent {
                            sample: start_sample,
                            track_index,
                            channel,
                            message: PerformanceMessage::NoteOn {
                                note: midi,
                                velocity: note_velocity,
                            },
                        });
                        events.push(PerformanceEvent {
                            sample: end_sample,
                            track_index,
                            channel,
                            message: PerformanceMessage::NoteOff { note: midi },
                        });
                    }
                    onset_whole_notes += beat_duration;
                }
            }
        }
        let bar_duration = fraction_as_f64((u64::from(master.time.0), u64::from(master.time.1)))?;
        elapsed_seconds += whole_notes_to_seconds(bar_duration, tempo);
        if !elapsed_seconds.is_finite() {
            return Err(Error::Malformed("score duration is not finite".into()));
        }
    }

    let sample_count = events
        .iter()
        .map(|event| event.sample)
        .max()
        .unwrap_or(0)
        .max(seconds_to_samples(elapsed_seconds)?);
    events.sort_by_key(|event| {
        (
            event.sample,
            message_priority(event.message),
            event.track_index,
            message_note(event.message),
        )
    });
    Ok(CompiledPerformance {
        events,
        sample_count,
    })
}

/// Renders a document to stereo 16-bit PCM WAV using the supplied SoundFont2 file.
///
/// # Errors
///
/// Returns any error from [`compile_performance_events`], [`crate::Error::Io`] if the SoundFont
/// file cannot be opened, and [`crate::Error::Malformed`] if it is not valid SoundFont2 data, the
/// synthesizer cannot be initialized, or the output is too long for a WAV file.
pub fn render_pcm(document: &Document, soundfont: impl AsRef<Path>) -> Result<Vec<u8>> {
    let performance = compile_performance_events(document)?;
    validate_wav_length(performance.sample_count)?;
    let file = File::open(soundfont)?;
    let mut reader = BufReader::new(file);
    let mut backend = create_backend(&mut reader)?;
    render_compiled(&performance, &mut backend)
}

fn create_backend(reader: &mut (impl Read + Seek)) -> Result<RustySynthBackend> {
    let soundfont = SoundFont::new(reader)
        .map_err(|error| Error::Malformed(format!("invalid SoundFont2 data: {error}")))?;
    let soundfont = Arc::new(soundfont);
    let settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
    let synthesizer = Synthesizer::new(&soundfont, &settings).map_err(|error| {
        Error::Malformed(format!(
            "could not initialize SoundFont synthesizer: {error}"
        ))
    })?;
    Ok(RustySynthBackend { synthesizer })
}

trait SynthBackend {
    fn apply(&mut self, event: PerformanceEvent);
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);
    /// Silences every voice and restores the default channel state, so a
    /// stream can restart the performance at another frame.
    fn reset(&mut self);
}

struct RustySynthBackend {
    synthesizer: Synthesizer,
}

impl SynthBackend for RustySynthBackend {
    fn apply(&mut self, event: PerformanceEvent) {
        let channel = i32::from(event.channel);
        match event.message {
            PerformanceMessage::ProgramChange { program } => {
                self.synthesizer
                    .process_midi_message(channel, 0xc0, i32::from(program), 0)
            }
            PerformanceMessage::NoteOn { note, velocity } => {
                self.synthesizer
                    .note_on(channel, i32::from(note), i32::from(velocity))
            }
            PerformanceMessage::NoteOff { note } => {
                self.synthesizer.note_off(channel, i32::from(note));
            }
        }
    }

    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.synthesizer.render(left, right);
    }

    fn reset(&mut self) {
        self.synthesizer.reset();
    }
}

fn render_compiled(
    performance: &CompiledPerformance,
    backend: &mut impl SynthBackend,
) -> Result<Vec<u8>> {
    let data_len = validate_wav_length(performance.sample_count)?;
    let capacity = 44usize
        .checked_add(data_len)
        .ok_or_else(|| Error::Malformed("WAV output length overflow".into()))?;
    let mut wav = Vec::new();
    wav.try_reserve_exact(capacity)
        .map_err(|error| Error::Malformed(format!("could not allocate WAV output: {error}")))?;
    wav.extend(b"RIFF");
    let data_len_u32: u32 = data_len
        .try_into()
        .map_err(|_| Error::Malformed("WAV output exceeds the RIFF size limit".into()))?;
    let riff_len = data_len_u32
        .checked_add(36)
        .ok_or_else(|| Error::Malformed("WAV output exceeds the RIFF size limit".into()))?;
    wav.extend(riff_len.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(SAMPLE_RATE.to_le_bytes());
    wav.extend((SAMPLE_RATE * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data_len_u32.to_le_bytes());
    let mut cursor = 0u64;
    for &event in &performance.events {
        if event.sample > performance.sample_count {
            return Err(Error::Malformed(
                "event occurs beyond the compiled duration".into(),
            ));
        }
        render_until(event.sample, &mut cursor, backend, &mut wav)?;
        backend.apply(event);
    }
    render_until(performance.sample_count, &mut cursor, backend, &mut wav)?;
    Ok(wav)
}

fn render_until(
    target: u64,
    cursor: &mut u64,
    backend: &mut impl SynthBackend,
    output: &mut Vec<u8>,
) -> Result<()> {
    const BLOCK: usize = 1024;
    let mut left = [0.0f32; BLOCK];
    let mut right = [0.0f32; BLOCK];
    while *cursor < target {
        let remaining: usize = (target - *cursor)
            .min(BLOCK as u64)
            .try_into()
            .map_err(|_| Error::Malformed("audio block exceeds addressable memory".into()))?;
        backend.render(&mut left[..remaining], &mut right[..remaining]);
        for (&left_sample, &right_sample) in left[..remaining].iter().zip(&right[..remaining]) {
            output.extend(float_to_i16(left_sample).to_le_bytes());
            output.extend(float_to_i16(right_sample).to_le_bytes());
        }
        *cursor += u64::try_from(remaining)
            .map_err(|_| Error::Malformed("audio cursor overflow".into()))?;
    }
    Ok(())
}

fn audible_tracks(document: &Document) -> Vec<usize> {
    let has_solo = document.tracks.iter().any(|track| track.solo);
    document
        .tracks
        .iter()
        .enumerate()
        .filter_map(|(index, track)| (!track.mute && (!has_solo || track.solo)).then_some(index))
        .collect()
}

fn track_channels(document: &Document, tracks: &[usize]) -> Result<Vec<u8>> {
    let mut channels = vec![None; document.tracks.len()];
    let mut available = (0u8..16).filter(|channel| *channel != 9);
    for &track_index in tracks {
        let track = document
            .tracks
            .get(track_index)
            .ok_or_else(|| Error::Malformed("track index is outside the document".into()))?;
        let percussion = match track.midi_channel {
            Some(channel) => channel == 9,
            None => track.tuning.is_empty() || track.tuning.iter().all(|pitch| *pitch == 0),
        };
        channels[track_index] = Some(if percussion {
            9
        } else {
            available.next().ok_or_else(|| {
                Error::Malformed("more than 15 pitched tracks cannot be rendered".into())
            })?
        });
    }
    Ok(channels
        .into_iter()
        .map(|channel| channel.unwrap_or(0))
        .collect())
}

fn validate_tempos(document: &Document) -> Result<()> {
    for tempo in &document.tempo_map {
        if tempo.at_bar >= document.master_bars.len() {
            return Err(Error::Malformed(
                "tempo change references a missing bar".into(),
            ));
        }
        if !tempo.bpm.is_finite() || tempo.bpm <= 0.0 {
            return Err(Error::Malformed(
                "tempo must be a positive finite value".into(),
            ));
        }
    }
    Ok(())
}

fn tempo_at(document: &Document, bar_index: usize) -> f64 {
    document
        .tempo_map
        .iter()
        .filter(|change| change.at_bar <= bar_index)
        .max_by_key(|change| change.at_bar)
        .map_or(120.0, |change| change.bpm)
}

fn dynamic_velocity(dynamic: &str) -> Result<u8> {
    match dynamic.trim().to_ascii_uppercase().as_str() {
        "PPP" => Ok(16),
        "PP" => Ok(32),
        "P" => Ok(48),
        "MP" => Ok(64),
        "MF" => Ok(80),
        "F" => Ok(96),
        "FF" => Ok(112),
        "FFF" => Ok(120),
        "SF" | "SFZ" | "SFF" => Ok(127),
        other => Err(Error::Malformed(format!(
            "unsupported dynamic marking: {other}"
        ))),
    }
}

fn fraction_as_f64((numerator, denominator): (u64, u64)) -> Result<f64> {
    if denominator == 0 {
        return Err(Error::Malformed("rhythm has a zero denominator".into()));
    }
    let value = numerator as f64 / denominator as f64;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Malformed("rhythm duration is not finite".into()))
    }
}

fn whole_notes_to_seconds(whole_notes: f64, bpm: f64) -> f64 {
    whole_notes * 4.0 * 60.0 / bpm
}

fn seconds_to_samples(seconds: f64) -> Result<u64> {
    let sample = seconds * f64::from(SAMPLE_RATE);
    if !sample.is_finite() || sample < 0.0 || sample > u64::MAX as f64 {
        return Err(Error::Malformed(
            "performance time exceeds sample range".into(),
        ));
    }
    Ok(sample.round() as u64)
}

fn message_priority(message: PerformanceMessage) -> u8 {
    match message {
        PerformanceMessage::ProgramChange { .. } => 0,
        PerformanceMessage::NoteOff { .. } => 1,
        PerformanceMessage::NoteOn { .. } => 2,
    }
}

fn message_note(message: PerformanceMessage) -> u8 {
    match message {
        PerformanceMessage::ProgramChange { program } => program,
        PerformanceMessage::NoteOn { note, .. } | PerformanceMessage::NoteOff { note } => note,
    }
}

fn float_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

fn validate_wav_length(sample_count: u64) -> Result<usize> {
    let data_len = sample_count
        .checked_mul(4)
        .ok_or_else(|| Error::Malformed("WAV data length overflow".into()))?;
    let data_len_u32: u32 = data_len
        .try_into()
        .map_err(|_| Error::Malformed("WAV output exceeds the RIFF size limit".into()))?;
    data_len_u32
        .checked_add(36)
        .ok_or_else(|| Error::Malformed("WAV output exceeds the RIFF size limit".into()))?;
    usize::try_from(data_len)
        .map_err(|_| Error::Malformed("WAV output exceeds addressable memory".into()))
}

#[cfg(test)]
#[path = "wav_test.rs"]
mod tests;
