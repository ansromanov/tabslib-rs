# Introduction

**tabslib** is a Rust library for fretted-instrument notation: an owned,
immutable document model, codecs for the formats that carry it, deterministic
edits over both, and style-neutral generation, arrangement and assessment built
on the same model.

It is written from published format behaviour and depends on no vendor
library. Tests need no score files: every document they use is generated in
code by `tabslib::fixtures`.

## What it does

- **Reads and writes** Guitar Pro 7/8 (`.gp`), MIDI and MusicXML; **renders**
  ASCII tablature, standalone HTML, PDF and WAV.
- **Inspects** a score: summary, timing, key and mode, chords, statistics,
  bar integrity.
- **Edits** deterministically: transpose, retune, slice, splice, set
  techniques over a selection, restructure tracks and bars, with guards that
  refuse to produce an over-full bar.
- **Generates** drums, bass, harmony, solos, riffs and arrangements from a
  seed and a request; the same seed and request always give the same result.
- **Assesses** a document with structural and quality metrics, compared
  against reference values the caller supplies.
- **Explains** assessments through optional AI connectors (Anthropic,
  OpenAI-compatible, Gemini, Ollama), off by default.

## What it does not do

It does not embed a genre, a style identity or measured reference data. Those
decisions belong to the calling application, which passes them in as profiles
and calibration samples. The test for whether something belongs in the
library:

> Can it be specified without naming a genre, a band, or a statistic measured
> over a collection of music?

Transposing a track can. "Add a fill where the section changes" cannot.

## Status

Version 0.1.1. The API is usable and tested but may still change
before 1.0, and the crate is not yet on crates.io. See the
[roadmap](roadmap.md).
