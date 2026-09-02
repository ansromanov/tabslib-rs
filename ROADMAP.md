# Roadmap

Target: a complete, dependency-free, general-purpose tablature library —
reading, inspecting, editing and writing fretted-instrument notation across the
formats that carry it.

Scope is deliberately bounded. This crate does **format, model and
deterministic edits**. It does not decide what to play — no generation, no
style modelling, no scoring against a reference collection. The test for
whether something belongs here:

> Can it be specified without naming a genre, a band, or a statistic measured
> over a collection of music?

Transposing a track can. "Add a fill where the section changes" cannot.

---

## Phase 1 — codec parity *(in progress)*

The file format, completely, in both directions.

- [x] Container read and write, deterministic
- [x] XML payload parse: tracks, tunings, colours, master bars, sections, meters, clefs, bars, voices, beats, rhythms, notes
- [x] Techniques: palm mute, dead, tapped, hammer-on/pull-off, let-ring, vibrato, slide flags, bend values, harmonics
- [x] Rhythm as exact fractions — values, dots, tuplets
- [ ] Dynamics per beat
- [ ] Ties across beats and barlines
- [ ] Percussion articulation table, not just the index
- [ ] Track mixer: volume, pan, mute, solo
- [ ] Repeats, alternate endings, directions
- [ ] Carry unmodelled container blocks through a save unchanged
- [ ] **Lossless round-trip**: save and reload changes nothing measurable

Phase 1 ends when a file survives a round-trip with no measurable difference
and reopens correctly in the application that wrote it.

## Phase 2 — inspection

Read-only questions about a score, the basis for everything after.

- [ ] Score summary: tracks, duration, meters, tempo map, key, tunings
- [ ] Timing primitives: bar capacity, beat position, step grids, feel detection
- [ ] Track classification: percussion, bass, empty, sounding-bar counts
- [ ] Note and technique census
- [ ] Bar integrity: find over- and under-full bars
- [ ] Key and tuning parsing from strings

## Phase 3 — structural edits

Changing the shape of a score, with the integrity guards that make it safe.

- [ ] Track: create, clone, remove, rename, reorder, unique naming
- [ ] Bar and beat and note construction
- [ ] Slice a bar range; splice a range from one score into another
- [ ] Append bars from another score
- [ ] Section markers: set, patch, rename
- [ ] Silence a bar range; drop empty tracks
- [ ] Clamp to capacity; assert no bar was made over-full by an edit

The guards matter as much as the edits. An operation that can produce an
over-full bar must be paired with a check that says so.

## Phase 4 — pitch and tuning

- [ ] Transpose a score, a track or a bar range, chromatic and diatonic
- [ ] Retune a score to a target tuning
- [ ] Retune **preserving fingering** — same frets, different pitches
- [ ] Pitch-class mapping between keys
- [ ] Capo and per-staff transposition

## Phase 5 — note and articulation edits

The "apply this to a selection" family. Deterministic and predictable, which is
what separates them from generation.

- [ ] Set or clear any technique over a selection
- [ ] Selection by track, bar range, string, pitch class, or predicate
- [ ] Accents and dynamics over a selection
- [ ] Re-finger a passage onto different strings, preserving pitch
- [ ] Split or merge voices

A worked example of the shape: *palm-mute every note on the lowest string
within these bars*. The user names the target and can predict the result.

## Phase 6 — rendering

- [ ] ASCII tablature for a track and for a score
- [ ] Bar-range windows, for comparing two passages side by side

## Phase 7 — audio and interchange

- [ ] MIDI export
- [ ] Standard MIDI file import
- [ ] MusicXML export
- [ ] PCM render via a soundfont

Sequenced last because none of it is needed to edit a score correctly, and each
piece is independently useful.

## Non-goals

Generation, arrangement, style modelling, quality scoring, and anything that
needs a collection of music to work. Those belong in a separate crate built on
this one.

Older binary container versions are not planned. The originating application
converts them, and four binary decoders is a large surface to own for files that
can be converted once.
