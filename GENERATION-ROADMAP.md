# Roadmap

Generation is part of the `tabslib` package. The package owns notation,
deterministic edits, codecs, inspection primitives, generation, arrangement,
calibration against caller-supplied references, and AI-facing workflows.

No score files, style-specific code, or artifacts derived from third-party
music belong in this repository. Style profiles and reference data are external
inputs.

## Phase 0 — crate boundary

- [x] Build generation directly on the unified tabslib model and APIs.
- [x] Add feature gates for each domain area.
- [x] Add CI for formatting, linting, tests, and documentation.

## Phase 1 — drum synthesis

- [x] Generic riff onset detection and deterministic onset masks.
- [x] Generic phrase grids and pattern-generation interfaces.
- [x] Document-level seeded drum-track generation with meter-preserving bars.
- [x] Caller-owned profile/archetype pattern catalogs with deterministic selection and meter adaptation.
- [x] Limb-aware playability solver.
- [x] Fills, transitions, and humanization.
- [x] Deterministic seeded generation tests.

## Phase 2 — rhythm section

- [x] Root extraction and riff-locked basslines.
- [x] Bass fills and voice-leading constraints.
- [x] Configurable riff-follow, pumping-eighths, root-fifth, walking, and sustained bass
  strategies, with guitar-derived/custom tunings and cross-bar playable-position optimization.
- [x] Rhythm-section compatibility scoring.

## Phase 3 — harmony and guitar two

- [x] Diatonic and parallel harmony generation.
- [x] Voice-leading and playability constraints.
- [x] Solo generation and phrase-tail variation.

## Phase 4 — riff and song composition

- [x] Motif segmentation and fingerprints.
- [x] Motif compatibility scoring.
- [x] Section transitions and joins.
- [x] Song assembly and form.
- [x] Addressed plan editing with applied and rejected reporting.
- [x] Deterministic seeded composition and octave variation.
- [x] Riff and motif authoring from a declarative specification.

## Phase 5 — assessment and calibration

- [x] Voice-leading and playability metrics.
- [x] Texture, repetition, silence, and shape reports.
- [x] External calibration inputs supplied by callers.
- [x] Blind preference records and a calibration gate verdict.
- [x] Golden reports over generated test documents.
- [x] Hard-requirement checking that rejects rather than repairs.
- [x] Quality ratios, with targets left to the caller.

## Phase 6 — AI and rationale

- [x] Prompt scaffolding over structured assessment facts.
- [x] Rationale generation with deterministic context capture.
- [x] Blind candidate pairing and structured comparison.
- [x] Provider trait and response validation behind the optional `ai` feature.
- [x] Optional Ollama, OpenAI-compatible, Gemini, and Anthropic connectors with configurable retries.
- [x] `AiConfig`: one configuration type that builds any enabled connector.

## Phase 7 — facade and release

- [x] Stable high-level APIs for scripts and applications.
- [ ] Release documentation and examples.
