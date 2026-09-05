//! Standard General MIDI percussion articulation mapping.

/// Coarse role of a percussion instrument in a standard kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumRole {
    /// Bass drum or kick.
    Kick,
    /// Snare drum.
    Snare,
    /// Closed, pedal, or open hi-hat.
    HiHat,
    /// Toms.
    Tom,
    /// Ride cymbal.
    Ride,
    /// Crash, china, splash, or other cymbal.
    Cymbal,
    /// Any mapped auxiliary percussion sound.
    Other,
}

/// A track-local articulation identifier resolved to a standard MIDI sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PercussionArticulation {
    /// Identifier stored on the score note.
    pub raw_id: i32,
    /// Standard General MIDI percussion pitch.
    pub midi: u8,
    /// Broad kit role for inspection and export decisions.
    pub role: DrumRole,
}

/// Resolves one track-local articulation using its observed MIDI pitch.
pub const fn articulation(raw_id: i32, midi: i32) -> Option<PercussionArticulation> {
    match midi_note(midi) {
        Some(midi) => match role(midi) {
            Some(role) => Some(PercussionArticulation { raw_id, midi, role }),
            None => None,
        },
        None => None,
    }
}

/// Returns the standard MIDI pitch for a raw articulation identifier.
pub const fn midi_note(articulation: i32) -> Option<u8> {
    if articulation >= 35 && articulation <= 81 {
        Some(articulation as u8)
    } else {
        None
    }
}

/// Maps a standard MIDI percussion pitch to its broad kit role.
pub const fn role(midi: u8) -> Option<DrumRole> {
    match midi {
        35 | 36 => Some(DrumRole::Kick),
        37..=40 => Some(DrumRole::Snare),
        42 | 44 | 46 => Some(DrumRole::HiHat),
        41 | 43 | 45 | 47 | 48 | 50 => Some(DrumRole::Tom),
        51 | 53 | 59 => Some(DrumRole::Ride),
        49 | 52 | 55 | 57 | 58 => Some(DrumRole::Cymbal),
        35..=81 => Some(DrumRole::Other),
        _ => None,
    }
}

/// Looks up both the normalized MIDI note and broad instrument role.
pub const fn lookup(articulation: i32) -> Option<(u8, DrumRole)> {
    match midi_note(articulation) {
        Some(midi) => match role(midi) {
            Some(role) => Some((midi, role)),
            None => None,
        },
        None => None,
    }
}
