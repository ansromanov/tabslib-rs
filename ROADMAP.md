# Roadmap

**Status: 0.1.0.** Everything listed under *Done* is implemented and
tested. The public API may still change before 1.0, and the crate is not yet on
crates.io.

Scope: notation, codecs, deterministic edits, and style-neutral generation and
assessment. A feature belongs here when it can be specified without naming a
genre, a band, or a statistic measured over a collection of music. Style
profiles and reference values are inputs from the caller, never compiled in.

## Next

- [ ] Publish to crates.io.
- [ ] `# Errors` sections on every fallible public function.
- [ ] Split the five modules above the 700-line limit (`generation/arrangement`,
      `inspect/harmony`, `generation/harmony`, `facade`, `generation/requirements`).
- [ ] GP coverage: stylesheets, extended properties, lyrics, chord diagrams,
      automations.

## Done

### 0.1.0 release

- [x] Every public item documented; `missing_docs` enforced.
- [x] `std::error::Error` for every public error type.
- [x] This book and the rustdoc API reference on GitHub Pages.
- [x] `v0.1.0` tagged with a generated changelog.

### Notation and codecs

- [x] Format adapters behind `ReadFormat` / `WriteFormat`; the core depends on
      none of them, and `--no-default-features` builds with `thiserror` alone.
- [x] GP7/8: container and XML payload in both directions, deterministic
      output, lossless round trip, unmodelled blocks carried through unchanged.
- [x] Rhythm as exact fractions; techniques, dynamics, ties, percussion
      articulations, mixer, repeats, alternate endings and directions.
- [x] MIDI and MusicXML read and write; ASCII, HTML, PDF and WAV rendering.

### Inspection

- [x] Score summary, timing primitives, track classification, note and
      technique census, bar integrity.
- [x] Key and mode with confidence, per-bar chords with Roman numerals,
      interval friction between tracks, descriptive statistics.

### Edits

- [x] Tracks, bars, beats and notes; slice, splice, append; section markers.
- [x] Transpose (chromatic and diatonic), retune preserving pitch or fingering,
      capo, re-fingering.
- [x] Techniques, accents and dynamics over a selection; voice split and merge.
- [x] Integrity guards: no edit may leave an over-full bar or unplayable note.

### Generation and assessment

- [x] Seeded drum synthesis with limb-aware playability, fills and humanization.
- [x] Riff-locked bass in several strategies; diatonic and parallel harmony;
      second guitar; solos with phrase-tail variation.
- [x] Riff and motif authoring from a declarative specification; motif
      compatibility, transitions, song form and arrangement search.
- [x] Structural and quality metrics, hard requirements that reject rather than
      repair, calibration against caller-supplied samples, blind preference
      records.

### AI (optional)

- [x] Prompt construction over structured assessment facts, response
      validation, blind candidate comparison.
- [x] Connectors for Anthropic, OpenAI-compatible services, Gemini and Ollama,
      configured through `AiConfig`; off by default.

## Non-goals

- Style identity, genre priors or measured reference data inside the crate.
- Older binary GP container versions. The originating application converts
  them, and a binary decoder per version is a large surface for files that can
  be converted once.
