//! Incremental synthesis: a compiled performance rendered in pieces.
//!
//! [`render_pcm`](super::render_pcm) synthesizes a whole score before it
//! returns anything, which is the wrong shape for a player: a listener waits
//! for the last bar of a song to be rendered before hearing the first. Owns
//! [`Soundfont`], an instrument bank parsed once and reused, and
//! [`PerformanceStream`], which renders a range of sample frames at a time and
//! can [`seek`](PerformanceStream::seek) to any frame first, so a caller can
//! start at the bar a cursor is on and fill the rest behind the playhead.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::sync::Arc;

use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

use super::{
    float_to_i16, CompiledPerformance, PerformanceEvent, PerformanceMessage, RustySynthBackend,
    SynthBackend, SAMPLE_RATE,
};
use crate::error::{Error, Result};

/// Frames synthesized per call into the SoundFont renderer.
const BLOCK: usize = 1024;

/// A SoundFont2 instrument bank, parsed once and shared by streams.
///
/// Parsing a bank costs the same whether one frame or a whole score follows,
/// so a player that re-renders on every mixer change should hold one of these
/// rather than reopening the file.
#[derive(Clone)]
pub struct Soundfont {
    bank: Arc<SoundFont>,
}

impl std::fmt::Debug for Soundfont {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Soundfont")
            .field("presets", &self.bank.get_presets().len())
            .finish()
    }
}

impl Soundfont {
    /// Parses a SoundFont2 file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        Self::read(&mut reader)
    }

    /// Parses a SoundFont2 bank from an open reader.
    pub fn read(reader: &mut (impl Read + Seek)) -> Result<Self> {
        let bank = SoundFont::new(reader)
            .map_err(|error| Error::Malformed(format!("invalid SoundFont2 data: {error}")))?;
        Ok(Self {
            bank: Arc::new(bank),
        })
    }
}

/// A compiled performance synthesized a range of frames at a time.
///
/// The stream holds one synthesizer and walks the events in order, so
/// successive [`render`](Self::render) calls continue the same performance: a
/// note started in one call is still ringing in the next. [`seek`](Self::seek)
/// restarts the synthesizer at a frame, applying the program changes that
/// precede it so every track keeps its instrument. Notes that were already
/// sounding at that frame are not re-attacked, the way starting playback
/// mid-score never re-strikes a chord struck in the previous bar.
pub struct PerformanceStream {
    core: Stream<RustySynthBackend>,
}

impl std::fmt::Debug for PerformanceStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PerformanceStream")
            .field("events", &self.core.events.len())
            .field("sample_count", &self.core.sample_count)
            .field("frame", &self.core.frame)
            .finish()
    }
}

impl PerformanceStream {
    /// Starts a stream over `performance` at frame zero.
    pub fn new(performance: &CompiledPerformance, soundfont: &Soundfont) -> Result<Self> {
        let settings = SynthesizerSettings::new(
            i32::try_from(SAMPLE_RATE)
                .map_err(|_| Error::Malformed("sample rate is outside MIDI range".into()))?,
        );
        let synthesizer = Synthesizer::new(&soundfont.bank, &settings).map_err(|error| {
            Error::Malformed(format!(
                "could not initialize SoundFont synthesizer: {error}"
            ))
        })?;
        Ok(Self {
            core: Stream::new(RustySynthBackend { synthesizer }, performance),
        })
    }

    /// The frame the next [`render`](Self::render) call starts at.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.core.frame
    }

    /// Frames in the whole performance.
    #[must_use]
    pub fn sample_count(&self) -> u64 {
        self.core.sample_count
    }

    /// Restarts synthesis at `frame`, past the end of the performance included.
    ///
    /// Every program change before `frame` is re-applied, so the instruments
    /// are the ones the score selected; notes already sounding there are not.
    pub fn seek(&mut self, frame: u64) {
        self.core.seek(frame);
    }

    /// Appends up to `frames` stereo frames to `output` as interleaved signed
    /// 16-bit samples, and returns how many were written.
    ///
    /// The stream stops at the end of the performance, so a short or zero
    /// return says the score is finished rather than that anything failed.
    pub fn render(&mut self, frames: usize, output: &mut Vec<i16>) -> Result<usize> {
        self.core.render(frames, output)
    }
}

/// The stream itself, over any synthesis backend so tests can drive it
/// without a SoundFont2 file.
#[derive(Debug)]
struct Stream<B> {
    backend: B,
    events: Vec<PerformanceEvent>,
    sample_count: u64,
    next_event: usize,
    frame: u64,
}

impl<B: SynthBackend> Stream<B> {
    fn new(backend: B, performance: &CompiledPerformance) -> Self {
        Self {
            backend,
            events: performance.events.clone(),
            sample_count: performance.sample_count,
            next_event: 0,
            frame: 0,
        }
    }

    fn seek(&mut self, frame: u64) {
        self.backend.reset();
        self.frame = frame;
        self.next_event = 0;
        while let Some(event) = self.events.get(self.next_event) {
            if event.sample >= frame {
                break;
            }
            if matches!(event.message, PerformanceMessage::ProgramChange { .. }) {
                self.backend.apply(*event);
            }
            self.next_event += 1;
        }
    }

    fn render(&mut self, frames: usize, output: &mut Vec<i16>) -> Result<usize> {
        let remaining = usize::try_from(self.sample_count.saturating_sub(self.frame))
            .unwrap_or(usize::MAX)
            .min(frames);
        let samples = remaining
            .checked_mul(2)
            .ok_or_else(|| Error::Malformed("render length overflows the output".into()))?;
        output
            .try_reserve(samples)
            .map_err(|error| Error::Malformed(format!("could not allocate PCM output: {error}")))?;
        let mut left = [0.0f32; BLOCK];
        let mut right = [0.0f32; BLOCK];
        let mut written = 0usize;
        while written < remaining {
            while let Some(event) = self.events.get(self.next_event) {
                if event.sample > self.frame {
                    break;
                }
                self.backend.apply(*event);
                self.next_event += 1;
            }
            let until_event = self
                .events
                .get(self.next_event)
                .map_or(u64::MAX, |event| event.sample.saturating_sub(self.frame));
            let block = (remaining - written)
                .min(BLOCK)
                .min(usize::try_from(until_event).unwrap_or(usize::MAX))
                .max(1);
            self.backend.render(&mut left[..block], &mut right[..block]);
            for (&left_sample, &right_sample) in left[..block].iter().zip(&right[..block]) {
                output.push(float_to_i16(left_sample));
                output.push(float_to_i16(right_sample));
            }
            written += block;
            self.frame += u64::try_from(block)
                .map_err(|_| Error::Malformed("audio cursor overflow".into()))?;
        }
        Ok(written)
    }
}

#[cfg(test)]
#[path = "stream_test.rs"]
mod tests;
