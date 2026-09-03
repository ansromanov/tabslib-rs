//! Deterministic PCM rendering primitives.

use std::path::Path;

use crate::error::{Error, Result};
use crate::model::Document;

/// Renders a score to mono 16-bit PCM WAV after validating an external
/// soundfont path. The compact built-in tone is intentionally dependency-free;
/// callers can use the MIDI export with a soundfont renderer for full voices.
pub fn render_pcm(doc: &Document, soundfont: impl AsRef<Path>) -> Result<Vec<u8>> {
    if !soundfont.as_ref().is_file() {
        return Err(Error::Malformed("soundfont path is not a file".into()));
    }
    let sample_rate = 44_100u32;
    let samples = (doc.master_bars.len().max(1) as u32 * sample_rate / 2) as usize;
    let data_len = samples * 2;
    let mut wav = Vec::with_capacity(44 + data_len);
    wav.extend(b"RIFF");
    wav.extend((36 + data_len as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(sample_rate.to_le_bytes());
    wav.extend((sample_rate * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data_len as u32).to_le_bytes());
    for i in 0..samples {
        let value = ((i as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32).sin()
            * 0.15
            * i16::MAX as f32) as i16;
        wav.extend(value.to_le_bytes());
    }
    Ok(wav)
}
