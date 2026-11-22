# Architecture

```text
                 tabslib
     document model, codecs, inspection,
       statistics, and deterministic edits
                       |
                       v
               tabslib::generation
    generation, arrangement, calibration,
       rationale, and optional AI services
```

The generation modules consume documents and general analysis results from
the rest of `tabslib`. They may create a changed document through the edit APIs,
but the decision about what to generate belongs here.

## Modules

| Module | Responsibility |
|---|---|
| `drums` | Generic onsets, phrase-clock fills, profile-driven generation, performance articulations, limb playability, and caller-governed improvement |
| `rhythm` | Root extraction, riff-locked bass, fills, voice leading, and compatibility scoring |
| `harmony` | Diatonic and parallel harmony, guitar-two voicing, voice leading, and fretboard playability |
| `solo` | Seeded scale-constrained lead generation, rhythmic tiling, and phrase-tail variation |
| `arrangement` | Motifs, compatibility, transitions, and song form |
| `assessment` | Generation-oriented metrics and external calibration |
| `ai` | Prompt, rationale, and blind-evaluation integrations |

General key determination, score statistics, integrity, and basic operations
remain in the general `tabslib` modules. This package must not turn those into genre-specific
rules or hide reference data inside an algorithm.

The optional `ai` feature, off by default, depends on assessment reports and
`serde_json` for canonical prompt and response data. It supplies a provider trait and validates
that returned findings match a candidate diagnostic and score region. The
`ai-anthropic`, `ai-ollama`, `ai-openai-compatible`, and `ai-gemini` features
add synchronous HTTP connectors for the Anthropic Messages API, Ollama,
OpenAI-compatible Chat Completions, and Gemini. `AiConfig` builds any enabled
connector from one set of settings. A connector reads no environment variable
itself; `AiConfig` falls back to the service's conventional key variable only
when the caller sets no key. Timeout, retry count,
and retry delay are configurable, with retries disabled by default. Because the provider trait is synchronous,
async applications should run provider calls on a blocking worker. Candidate
comparison remains descriptive and never ranks or changes documents implicitly.
Builds without `ai` have no AI serialization dependency; builds without a
provider feature have no HTTP client dependency.
