//! In-place presentation edits for `.gp` containers.
//!
//! Owns [`Restyle`], [`VolumeDb`] and [`restyle`]. These settings live outside
//! the score model -- the zoom and mixer fader in the GPIF payload, the
//! notation shown per track in `Content/PartConfiguration` -- so they are
//! patched at the container level and every other entry is carried over
//! unchanged.

use super::container;
use crate::error::{Error, Result};

/// Mixer gain per instrument family, in decibels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeDb {
    /// Guitar tracks.
    pub guitar: f32,
    /// Bass tracks.
    pub bass: f32,
    /// Drum kits.
    pub drums: f32,
    /// Every other instrument.
    pub other: f32,
}

/// Presentation changes to apply; `None` and `false` leave a setting alone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Restyle {
    /// Page zoom as a factor, `1.5` for 150%.
    pub zoom: Option<f32>,
    /// Show tablature only on tracks that show both notation and tablature.
    pub tab_only: bool,
    /// Track fader levels.
    pub volumes: Option<VolumeDb>,
}

const PART_CONFIG: &str = "Content/PartConfiguration";
const BOTH_STAVES: u8 = 3;
const TAB_ONLY: u8 = 2;
/// Fader position of 0 dB, and the position change per decibel. Measured on
/// files saved by the originating application: 0 dB -> 0.80, -2 dB -> 0.76,
/// -3 dB -> 0.74, with positions stored to two decimals.
const FADER_ZERO: f64 = 0.80;
const FADER_PER_DB: f64 = 0.02;
/// Index of the volume fader in a channel strip's parameter list.
const FADER_INDEX: usize = 12;

#[derive(Clone, Copy)]
enum Family {
    Guitar,
    Bass,
    Drums,
    Other,
}

/// Applies `opts` to container bytes and returns the new container bytes.
///
/// Errors if a requested setting has nowhere to be written (no zoom element,
/// a track without a channel strip, an unreadable `PartConfiguration`) rather
/// than skipping it, so a batch never reports success for an untouched file.
pub fn restyle(bytes: &[u8], opts: &Restyle) -> Result<Vec<u8>> {
    let mut entries = container::read_entries(bytes)?;
    let mut touched_gpif = false;
    for (name, data) in entries.iter_mut() {
        if name.ends_with("score.gpif") && (opts.zoom.is_some() || opts.volumes.is_some()) {
            let mut xml =
                String::from_utf8(std::mem::take(data)).map_err(|e| Error::format("gp", e))?;
            if let Some(zoom) = opts.zoom {
                xml = set_zoom(&xml, zoom)?;
            }
            if let Some(volumes) = opts.volumes {
                xml = set_volumes(&xml, volumes)?;
            }
            *data = xml.into_bytes();
            touched_gpif = true;
        } else if name == PART_CONFIG && opts.tab_only {
            set_tab_only(data)?;
        }
    }
    if (opts.zoom.is_some() || opts.volumes.is_some()) && !touched_gpif {
        return Err(Error::Malformed("no score payload in the archive".into()));
    }
    container::write_entries(&entries)
}

fn set_zoom(xml: &str, zoom: f32) -> Result<String> {
    replace_between(xml, "<ScoreZoom>", "</ScoreZoom>", &zoom.to_string())
        .ok_or_else(|| Error::Malformed("score has no ScoreZoom element".into()))
}

/// Replaces the text between the first `open` and the `close` after it.
fn replace_between(xml: &str, open: &str, close: &str, with: &str) -> Option<String> {
    let start = xml.find(open)? + open.len();
    let end = start + xml.get(start..)?.find(close)?;
    Some(format!("{}{}{}", &xml[..start], with, &xml[end..]))
}

fn family(track: &str) -> Family {
    let kind = track
        .find("<InstrumentSet>")
        .and_then(|i| track.get(i..))
        .and_then(|s| s.split("<Type>").nth(1))
        .and_then(|s| s.split("</Type>").next())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if kind.contains("drum") {
        Family::Drums
    } else if kind.contains("bass") {
        Family::Bass
    } else if kind.contains("guitar") {
        Family::Guitar
    } else {
        Family::Other
    }
}

fn set_volumes(xml: &str, volumes: VolumeDb) -> Result<String> {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<Track id=") {
        let Some(len) = rest[start..].find("</Track>") else {
            break;
        };
        let end = start + len + "</Track>".len();
        out.push_str(&rest[..start]);
        let track = &rest[start..end];
        let db = match family(track) {
            Family::Guitar => volumes.guitar,
            Family::Bass => volumes.bass,
            Family::Drums => volumes.drums,
            Family::Other => volumes.other,
        };
        out.push_str(&set_fader(track, db)?);
        rest = &rest[end..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Rewrites the fader value in the first channel strip of one `<Track>`.
fn set_fader(track: &str, db: f32) -> Result<String> {
    let strip = track
        .find("<ChannelStrip")
        .ok_or_else(|| Error::Malformed("track has no channel strip".into()))?;
    let open = "<Parameters>";
    let from = strip
        + track[strip..]
            .find(open)
            .ok_or_else(|| Error::Malformed("channel strip has no parameters".into()))?
        + open.len();
    let to = from
        + track[from..]
            .find("</Parameters>")
            .ok_or_else(|| Error::Malformed("unterminated channel strip parameters".into()))?;
    let mut params: Vec<String> = track[from..to]
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let slot = params
        .get_mut(FADER_INDEX)
        .ok_or_else(|| Error::Malformed("channel strip has no volume fader".into()))?;
    let position = (FADER_ZERO + FADER_PER_DB * f64::from(db)).clamp(0.0, 1.0);
    *slot = format!("{}", (position * 100.0).round() / 100.0);
    Ok(format!(
        "{}{}{}",
        &track[..from],
        params.join(" "),
        &track[to..]
    ))
}

/// Part configuration: a 4-byte count, 4 reserved bytes, a one-byte track
/// count `n`, then `n` display masks; after a 5-byte separator follow `n`
/// records whose first byte is the same mask, six bytes each except the last
/// (five). Bit 0 is notation, bit 1 is tablature.
fn set_tab_only(data: &mut [u8]) -> Result<()> {
    let n = usize::from(*data.get(8).ok_or_else(unreadable)?);
    let records = 9 + n + 5;
    if n == 0 || data.len() <= records + 6 * (n - 1) {
        return Err(unreadable());
    }
    for i in 0..n {
        for at in [9 + i, records + 6 * i] {
            if data[at] == BOTH_STAVES {
                data[at] = TAB_ONLY;
            }
        }
    }
    Ok(())
}

fn unreadable() -> Error {
    Error::Malformed("unreadable PartConfiguration".into())
}

#[cfg(test)]
#[path = "restyle_test.rs"]
mod tests;
