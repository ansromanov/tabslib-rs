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

## Phase 0 — format adapters *(done)*

Formats sit behind `ReadFormat` / `WriteFormat`; the core depends on none of
them. See [docs/architecture.md](docs/architecture.md).

- [x] `container` and the XML codec moved to `format/gp/`
- [x] Two traits, so a rendering cannot be mistaken for a container
- [x] Note-value spellings moved out of the model into the adapter
- [x] Adapter failures boxed behind `Error::Format`, so `Error` has no
      format-specific variants
- [x] Feature flags, with `--no-default-features` proven in CI

## Phase 1 — codec parity *(done)*

The `gp` adapter, completely, in both directions.

- [x] Container read and write, deterministic
- [x] XML payload parse: tracks, tunings, colours, master bars, sections, meters, clefs, bars, voices, beats, rhythms, notes
- [x] Techniques: palm mute, dead, tapped, hammer-on/pull-off, let-ring, vibrato, slide flags, bend values, harmonics
- [x] Rhythm as exact fractions — values, dots, tuplets
- [x] Dynamics per beat
- [x] Ties across beats and barlines
- [x] Percussion articulation table, not just the index
- [x] Track mixer: volume, pan, mute, solo
- [x] Repeats, alternate endings, directions
- [x] Carry unmodelled container blocks through a save unchanged
- [x] **Lossless round-trip**: save and reload changes nothing measurable

Phase 1 ends when a file survives a round-trip with no measurable difference
and reopens correctly in the application that wrote it.

## Phase 2 — inspection *(done)*

Read-only questions about a score, the basis for everything after.

- [x] Score summary: tracks, duration, meters, tempo map, key, tunings
- [x] Timing primitives: bar capacity, beat position, step grids, feel detection
- [x] Track classification: percussion, bass, empty, sounding-bar counts
- [x] Note and technique census
- [x] Bar integrity: find over- and under-full bars
- [x] Key and tuning parsing from strings

## Phase 3 — structural edits *(done)*

Changing the shape of a score, with the integrity guards that make it safe.

- [x] Track: create, clone, remove, rename, reorder, unique naming
- [x] Bar and beat and note construction
- [x] Slice a bar range; splice a range from one score into another
- [x] Append bars from another score
- [x] Section markers: set, patch, rename
- [x] Silence a bar range; drop empty tracks
- [x] Clamp to capacity; assert no bar was made over-full by an edit

The guards matter as much as the edits. An operation that can produce an
over-full bar must be paired with a check that says so.

## Phase 4 — pitch and tuning *(done)*

- [x] Transpose a score, a track or a bar range, chromatic and diatonic
- [x] Retune a score to a target tuning
- [x] Retune **preserving fingering** — same frets, different pitches
- [x] Pitch-class mapping between keys
- [x] Capo and per-staff transposition

## Phase 5 — note and articulation edits *(done)*

The "apply this to a selection" family. Deterministic and predictable, which is
what separates them from generation.

- [x] Set or clear any technique over a selection
- [x] Selection by track, bar range, string, pitch class, or predicate
- [x] Accents and dynamics over a selection
- [x] Re-finger a passage onto different strings, preserving pitch
- [x] Split or merge voices

A worked example of the shape: *palm-mute every note on the lowest string
within these bars*. The user names the target and can predict the result.

## Phase 6 — rendering adapters *(ASCII done; HTML/PDF future)*

Write-only, so none of them takes the round-trip invariants.

- [x] `ascii` — tablature for a track and for a score, bar-range windows
- [ ] `html` — a standalone document
- [ ] `pdf` — print

## Phase 7 — interchange and audio *(done)*

Both round-trip, so both implement the pair of traits.

- [x] `midi` — read and write
- [x] `musicxml` — read and write
- [x] PCM render via a soundfont

Sequenced last because none of it is needed to edit a score correctly, and each
piece is independently useful.

## Non-goals

Generation, arrangement, style modelling, quality scoring, and anything that
needs a collection of music to work. Those belong in a separate crate built on
this one.

Older binary container versions are not planned. The originating application
converts them, and four binary decoders is a large surface to own for files that
can be converted once.
