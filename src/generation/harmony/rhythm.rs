//! Fitting source rhythms to a tempo and decomposing durations into written rhythms.

use super::*;

pub(super) const DEFAULT_BPM: f64 = 120.0;
pub(super) const FAST_SUBDIVISION_BPM: f64 = 80.0;

pub(super) fn tempo_at(doc: &Document, bar_index: usize) -> f64 {
    doc.tempo_map
        .iter()
        .filter(|change| change.at_bar <= bar_index)
        .max_by_key(|change| change.at_bar)
        .map(|change| change.bpm)
        .unwrap_or(DEFAULT_BPM)
}

pub(super) struct AccumBeat {
    pub(super) notes: Vec<Note>,
    pub(super) dynamic: Option<String>,
    pub(super) dur: Fraction,
    pub(super) rhythm: Option<Rhythm>,
}

pub(super) fn clamp_source_beats(beats: &[Beat], bpm: f64) -> Vec<Beat> {
    let sixteenth = Fraction::new(1, 16);
    if bpm <= FAST_SUBDIVISION_BPM
        || !beats
            .iter()
            .any(|b| inspect::rhythm_duration(b.rhythm) < sixteenth)
    {
        return beats.to_vec();
    }

    let mut accumulated: Vec<AccumBeat> = Vec::new();
    let mut i = 0;
    while i < beats.len() {
        let b = &beats[i];
        let dur = inspect::rhythm_duration(b.rhythm);
        let has_notes = b.notes.iter().any(|n| n.midi.is_some());

        if dur < sixteenth && !has_notes {
            if let Some(prev) = accumulated.last_mut() {
                prev.dur = prev.dur.plus(dur);
                prev.rhythm = None;
                i += 1;
                continue;
            }
        }

        let mut curr_dur = dur;
        let mut curr_notes = b.notes.clone();
        let mut curr_dynamic = b.dynamic.clone();
        let mut curr_has_notes = has_notes;
        let mut curr_rhythm = if dur >= sixteenth {
            Some(b.rhythm)
        } else {
            None
        };

        while curr_dur < sixteenth && i + 1 < beats.len() {
            i += 1;
            let next_b = &beats[i];
            let next_dur = inspect::rhythm_duration(next_b.rhythm);
            let next_has_notes = next_b.notes.iter().any(|n| n.midi.is_some());

            curr_dur = curr_dur.plus(next_dur);
            curr_rhythm = None;
            if !curr_has_notes && next_has_notes {
                curr_notes = next_b.notes.clone();
                curr_dynamic = next_b.dynamic.clone();
                curr_has_notes = true;
            }
        }

        if curr_dur < sixteenth {
            if let Some(prev) = accumulated.last_mut() {
                prev.dur = prev.dur.plus(curr_dur);
                prev.rhythm = None;
                if !prev.notes.iter().any(|n| n.midi.is_some()) && curr_has_notes {
                    prev.notes = curr_notes;
                    prev.dynamic = curr_dynamic;
                }
            } else {
                accumulated.push(AccumBeat {
                    notes: curr_notes,
                    dynamic: curr_dynamic,
                    dur: curr_dur,
                    rhythm: curr_rhythm,
                });
            }
        } else {
            accumulated.push(AccumBeat {
                notes: curr_notes,
                dynamic: curr_dynamic,
                dur: curr_dur,
                rhythm: curr_rhythm,
            });
        }
        i += 1;
    }

    let mut out_beats = Vec::new();
    for accum in accumulated {
        if let Some(rhythm) = accum.rhythm {
            out_beats.push(Beat {
                id: 0,
                rhythm,
                notes: accum.notes,
                dynamic: accum.dynamic,
            });
        } else {
            let rhythms = decompose_fraction_to_rhythms(accum.dur);
            for (idx, rhythm) in rhythms.into_iter().enumerate() {
                let (notes, dynamic) = if idx == 0 {
                    (accum.notes.clone(), accum.dynamic.clone())
                } else {
                    (Vec::new(), None)
                };
                out_beats.push(Beat {
                    id: 0,
                    rhythm,
                    notes,
                    dynamic,
                });
            }
        }
    }
    out_beats
}

pub(super) fn decompose_fraction_to_rhythms(frac: Fraction) -> Vec<Rhythm> {
    const SPELLINGS: [(Fraction, NoteValue, u8); 12] = [
        (
            Fraction {
                numerator: 1,
                denominator: 1,
            },
            NoteValue::Whole,
            0,
        ),
        (
            Fraction {
                numerator: 7,
                denominator: 8,
            },
            NoteValue::Half,
            2,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 4,
            },
            NoteValue::Half,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 2,
            },
            NoteValue::Half,
            0,
        ),
        (
            Fraction {
                numerator: 7,
                denominator: 16,
            },
            NoteValue::Quarter,
            2,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 8,
            },
            NoteValue::Quarter,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 4,
            },
            NoteValue::Quarter,
            0,
        ),
        (
            Fraction {
                numerator: 7,
                denominator: 32,
            },
            NoteValue::Eighth,
            2,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 16,
            },
            NoteValue::Eighth,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 8,
            },
            NoteValue::Eighth,
            0,
        ),
        (
            Fraction {
                numerator: 3,
                denominator: 32,
            },
            NoteValue::Sixteenth,
            1,
        ),
        (
            Fraction {
                numerator: 1,
                denominator: 16,
            },
            NoteValue::Sixteenth,
            0,
        ),
    ];
    for (dur, value, dots) in SPELLINGS {
        if dur == frac {
            return vec![Rhythm {
                value,
                dots,
                tuplet: None,
            }];
        }
    }
    if frac.denominator != 0 && 64 % frac.denominator == 0 {
        let mult = 64 / frac.denominator;
        let mut steps = frac.numerator * mult;
        let mut out = Vec::new();
        const STEP_SPELLINGS: [(u64, NoteValue, u8); 12] = [
            (64, NoteValue::Whole, 0),
            (56, NoteValue::Half, 2),
            (48, NoteValue::Half, 1),
            (32, NoteValue::Half, 0),
            (28, NoteValue::Quarter, 2),
            (24, NoteValue::Quarter, 1),
            (16, NoteValue::Quarter, 0),
            (14, NoteValue::Eighth, 2),
            (12, NoteValue::Eighth, 1),
            (8, NoteValue::Eighth, 0),
            (6, NoteValue::Sixteenth, 1),
            (4, NoteValue::Sixteenth, 0),
        ];
        while steps > 0 {
            let choice = STEP_SPELLINGS
                .iter()
                .copied()
                .find(|(s, _, _)| *s <= steps && (steps - *s >= 4 || steps == *s));
            let (s, value, dots) = match choice {
                Some(c) => c,
                None => (4.min(steps), NoteValue::Sixteenth, 0),
            };
            out.push(Rhythm {
                value,
                dots,
                tuplet: None,
            });
            steps = steps.saturating_sub(s);
        }
        return out;
    }
    vec![Rhythm::new(NoteValue::Sixteenth)]
}
