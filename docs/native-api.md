# Native application API

Applications can use `tabslib::facade` as the stable high-level path for
score inspection, edits, generation, arrangement, assessment, and
provider-neutral rationale workflows. Every operation works on
`tabslib::Document`; the facade does not require a file path or another score
model.

When an application has GP bytes, `facade::file` provides the corresponding
boundary workflows: `persist`, generation for drums, bass, harmony, solo and
riff, accompaniment, reviewed arrangement assembly, transitions, transpose,
and retuning. It returns bytes rather than opening paths, leaving storage and
atomic replacement to the application. The same module also exposes the
caller-owned known-regression and study summaries through the generation
facade; no reference data or style profile is bundled.

```rust
use tabslib::facade::{self, generation};

fn run() -> tabslib::Result<()> {
    let mut document = tabslib::fixtures::power_chords();
    let stats = facade::inspect::statistics(&document);
    let bass = generation::generate_bass(
        &mut document,
        generation::BassGenerationRequest {
            source_tracks: &[0],
            profile: generation::BassProfile {
                min_midi: 28,
                max_midi: 52,
                max_leap: 7,
                accent_velocity: 110,
                normal_velocity: 90,
            },
        },
    )?;
    let report = generation::assess_document(&document);
    assert!(bass.note_count > 0);
    assert!(stats.notes > 0);
    assert!(report.metrics.contains_key(&generation::Metric::Playability));
    Ok(())
}
```

Key analysis is available through `facade::inspect::determine_key`; its result
distinguishes Major, NaturalMinor, Dorian, Phrygian, Lydian, Mixolydian,
Locrian, and HarmonicMinor, with both overall confidence and a runner-up
margin. `analyze_bar_chords` detects complete triads across sounding tracks
and adds Roman numerals against the estimated key. Use
`analyze_bar_chords_in_key` to supply a tonic and mode explicitly.
`frictions_between(document, first_track, second_track)` reports overlapping
interval pairs, exact overlap durations, and a normalized weighted friction
ratio. Percussion tracks are omitted from these analyses.

The runnable [native workflow example](../examples/native_generation.rs)
combines inspection, edits, seeded drum and bass generation, harmony,
arrangement, assessment, and an optional format round trip using generated
fixtures. Build it with the crate's default features using
`cargo run --example native_generation`.

## Requests, results, and diagnostics

Use the request and result types at `facade::generation` rather than assembling
file-oriented workflows in an application. Drum and bass generation accept
`DrumGenerationRequest` and `BassGenerationRequest` and return
`GeneratedTrack`. Harmony uses `HarmonyConfig` and returns `GeneratedHarmony`,
including candidate-rejection diagnostics. Arrangement uses
`ArrangementRequest` and returns a new `Document`; invalid joins and source
ranges return `ArrangementError`. `assess_document` returns an
`AssessmentReport` with stable ratios and region-specific diagnostics, and
`calibrate_document` compares that report with caller-supplied samples.

The report carries two kinds of metric. Structural ones -- playability,
texture, repetition, silence, density, shape and voice leading -- describe how
the score is written. Quality ones -- palm-mute rate and spread, cymbal
coverage, tom share, unique bar patterns, longest loop, bar count, hypermeter
alignment, the three voicing shares and low-string share -- describe how
idiomatic it is. No target value is compiled into the crate: every metric is a
ratio the caller compares against its own samples, because the numbers that
make a ratio good are measured over a collection of music.

`Metric::Shape` reports the position of the densest bar as a share of the bar
span, where zero is the first bar and one the last. It is not a first-to-last
delta: a document that peaks halfway through and one that climbs steadily to
the end have the same delta and different shapes.

`Metric::LongestLoop` and `Metric::BarCount` are absolutes reported over a
denominator of one. Palm-mute placement is reported separately from the rate,
through `Metric::PalmMuteSpread` and the per-bar `palm_mute_bar` diagnostics,
because a matching aggregate with the wrong placement is the classic false
green.

Findings are emitted per beat and per note, so a document of any length
produces more of them than a reader can walk. `AssessmentReport::summary`
collapses them into one `DiagnosticGroup` per cause, worst severity first, each
naming its `DiagnosticClass`, the number of findings behind it, the codes
present and the master bars it touches. Prefer it to `diagnostics` when
reporting to a person.

`Severity` is `Error`, `Warning` or `Info`, and `DiagnosticClass` is the cause
a finding belongs to. Both are derived from the finding's stable `code` rather
than stored beside it, so a finding cannot carry a severity that disagrees with
what it reports. An unrecognized code reads as `Info` and `Measurement`.

`check_requirements` is a separate question from assessment. Assessment
measures ratios a caller compares against its own samples; a requirement is a
boolean with a location. `check_requirements` returns a `Vec<Violation>`, each
naming the `Requirement` that failed, the `Region` responsible, and a
requirement-specific `detail` measurement. An empty vector means nothing the
crate can check rejects the document. Violations are reports: nothing here
edits a document, because repairing a violated span in place is how output
that reads green on every metric still fails by ear.

Section-marker ordering has no document-level expression -- the model stores a
marker on a master bar, and the payload writer is what sequences it before the
bar list -- so it is covered by the adapter's own tests rather than by
`check_requirements`.

Song forms, section transitions, pattern catalogs and drum performance are on
the facade too. `generation::plan_form` turns a `FormTemplate` into
`SectionRequest`s for `arrange`. `generation::plan_transitions` reports the
section boundaries a `TransitionRule` set would treat, and
`generation::apply_transitions` applies them, filling only empty beats so no
written duration changes. `generation::detect_section_boundaries` compares
adjacent `fingerprint_bar` results and reports each boundary with confidence;
`sections_from_boundaries` turns them into bar ranges. Merge and single-bar
fold helpers return proposals without editing the score. To write accepted
boundaries, callers create labels with `propose_section_markers` and explicitly
call `apply_section_marker_proposals`, which fills empty markers and preserves
authored ones. `detect_riff_stabs` reports isolated accented bar/step positions
from caller-selected source tracks. `PatternGrid`, `PatternConstraints` and
`PatternCatalog` -- with the `DrumPatternCatalog` and
`RhythmSectionPatternCatalog` aliases -- select a grid-aligned pattern under a
seed, returning `PatternRejection` diagnostics rather than a fallback. Drum
performance covers `generate_seeded_hits`, `apply_fill`, `humanize_hits`,
`pair_hihat_chokes`, `apply_articulations`, `add_transition_marker`, the
`PhraseClock`, and the `assess_drums` / `improve_drums` pair.

`BeatGrouping::for_meter` supplies simple, compound, and common additive
groupings; `BeatGrouping::new` lets a caller specify another positive grouping
whose units sum to the meter numerator. `pulse_positions` and
`backbeat_positions` convert those groups to steps on a matching `PatternGrid`.
Pass caller overrides to `generate_drums_with_groupings` or
`generate_bass_with_groupings`; the original generation functions use the
default grouping for each bar's meter.

`generation::profile_existing_drums` reads a percussion track through the
General MIDI role lookup and returns its dominant cymbal family, kick/snare/
cymbal hit counts per bar, and written straight, triplet, or swung feel.
`DrumFeelProfile::to_drum_profile` converts those measured densities to the
seeded generator's probabilities. A track with no cymbal observations keeps
`cymbal_preference` as `None` and receives zero generated cymbal probability;
the placeholder voice is inert.

With the `midi` feature enabled, `generation::midi_to_drum_hits` reads MIDI
bytes through the existing adapter and projects recognized General MIDI drum
events onto a caller's `PatternGrid`. `MidiGrooveIndex::add_midi` stores
caller-supplied grooves in memory under an identifier, role, meter and tempo;
`select` picks an exact role and meter with the nearest tempo. The library does
not scan directories or provide a bundled groove collection.
`generate_from_midi_index` moves only kick events to the nearest riff attack,
leaving cymbal timekeeping events at their original steps. Seeded generation
is used only when the caller explicitly supplies `SeededGrooveFallback`.

An `ArrangementPlan` is editable rather than one-shot.
`generation::edit_plan` applies one `PlanEdit` -- repeat, drop, insert, move,
rename, transpose, variation, energy or instrumentation -- and returns the new
plan plus what it touched; `edit_plan_all` applies a list and reports both what
applied and what was refused, so a caller acting on a person's stated intent
keeps the parts that were possible.

A `SlotAddress` names a slot by index, by label, or by the nth occurrence of a
label. Positions shift under insertion and moves, so an address that has to
survive a sequence of edits should name a label. An address matching several
slots is refused rather than resolved to one of them.

`generation::apply_energy_contour` assigns caller-provided 0..=100 values to
plan sections. `apply_tempo_contour` writes caller-provided BPM values at
section boundaries after checking caller-supplied `TempoBounds`. Use
`diff_plan_sections` to find changed positions and
`compose_plan_incremental` to rebuild an edited plan. The latter reports which
sections changed and how many identical bars it could reuse from the previous
document; its output is checked against the deterministic full composition.

`SectionRequest` gained `energy` and `instrumentation`, both optional and both
applied by `compose_plan` after assembly: energy writes a dynamic across the
section's sounding beats, and instrumentation silences the tracks it does not
name without removing them. Older plan files parse unchanged.

Comparisons cannot fail a check, so `gate_document` adds the verdict:
`Pass`, `Fail`, `Quarantined`, or `Unmeasured` per metric, against a
`GatePolicy` the caller supplies. A metric with no reference or no tolerance is
`Unmeasured` rather than `Pass` -- a gate that counts a missing comparison as a
success reports green for a run that compared nothing. `GateReport::passed`
answers the question a check asks, `failures` names what to look at, and
`render` prints it. `discrimination` reports whether a metric can tell the
supplied samples apart at all; a metric reading the same for every sample
always agrees with itself and never catches anything.

`blind_record` logs a paired comparison with the sides assigned from a seed, so
the person logging it does not choose which candidate they hear first.
`Choice::BothBad` and `Choice::NoPreference` are real answers about a pair and
useless for ranking, which `Choice::is_decisive` and `tally` both respect.
Records serialize one per line through `append_record` and `read_records`;
nothing reads a clock or a file, because a library that stamps its own time
cannot be replayed and where a log lives is not this crate's decision.
Pair each record with the assessment report for both sides and train a
`PreferenceRanker` with `train_preference_ranker`. The minimum decisive sample
count is caller policy. Models round-trip through versioned JSON; use
`rank_preference_candidates` for deterministic ordering and
`preference_ranker_agreement` on held-out records to measure exact choice
agreement. The crate contains no pre-trained model.

Solo generation is also available through `generation::generate_solo` with a
`SoloConfig`; its result includes phrase boundaries and voice-leading rejection
diagnostics. With the `ai` feature, the facade exposes structured rationale
prompts, request capture, response validation, and candidate comparison. These
workflows depend on the caller to provide a `RationaleProvider`; the crate does
not include a network provider implementation. `PromptInput::judging` creates
a versioned assessment-ranking prompt; `judge_candidates` validates that the
provider returned every candidate exactly once. `blend_rankings_with_deterministic`
combines that result with a deterministic order using a caller-selected weight,
while `offline_heuristic_judge` uses only caller-supplied metric weights.
`judge_preference_agreement` compares the result with decisive recorded choices.
With `ascii`, `critique_tab` sends the actual ASCII rendering and validates
findings by defect class, severity, and zero-based bar. Prose-only praise is not
a valid critique response.

The same provider trait accepts versioned section-label and boundary prompts
built from caller-supplied `SectionFingerprint` values. `generate_section_labels`
requires one validated label per section; `generate_section_boundaries` checks
merge adjacency and split indices against the captured section count. Provider
failures carry an `AiFailureReason`; the HTTP adapters retry connection,
timeout, rate-limit, and server failures while returning authentication and
not-found responses immediately.

Token-driven solo composition accepts explicit MIDI pitches or scale degrees,
durations, rests, bends, slides, vibrato, and hammer-ons. The fingering solver
minimizes fret shifts and string crossings while enforcing the requested fret
window and the H3, H4, and H14 playability rules. It returns the requested
number of seeded candidates with assessment reports. Candidates stay in seed
order until the caller supplies a ranking function; no preferred score is built
into the library.

`write_riff` is the one generator that needs no source material. Every other
operation here accompanies material a caller already has; this one originates
it from a `RiffSpec`, which names a rhythmic archetype, a register, a voicing,
a mode, and one scale degree per bar. `validate_riff` checks a specification
without writing it, and an unrealizable one returns `RiffSpecError` rather than
a quietly different riff. `write_motif` arranges the same material into a
`MotifForm` -- statement and variation, call and response, or a sentence --
with a `TailVariation` controlling what changes in the answer.

The writer satisfies the structural requirements by construction: bars are
exactly full, positions sound the pitches they state, notes stay in the
specified mode, and no note is staccato or let-ring. The fifth in a power
voicing is the mode's own fifth above the degree, not a fixed seven semitones,
because a parallel fifth is not diatonic on every degree.

Style vocabularies, genre priors and measured statistics stay outside the crate.
A specification names musical properties; the data that decides which
specification to write belongs to the caller.
`generation::extract_rhythm_cells` records attacks, rests, and palm mutes on a
caller-selected step grid; `extract_rhythm_phrases` groups those cells, and
`extract_interval_ngrams` measures pitched-onset intervals. Build normalized
caller-data distributions with `build_rhythmic_vocabulary`, compare sparse
distributions with `cosine_similarity`, blend them with a caller weight, and
derive `form_prior` from caller-supplied role sequences. With `riffs`,
`apply_riff_profile` applies optional caller values and returns typed before/
after entries for every changed `RiffSpec` field. No profile table ships with
the crate.

An existing riff can be adapted explicitly with `plan_riff_adaptation` and
`apply_riff_adaptation`. The request selects one track and an inclusive bar
range, a destination key, an optional tempo and meter, and a playable fret
window. Planning reports the chromatic transposition, including the nearest
octave that fits every note in that window; it does not edit the document.
Applying the plan updates pitches and string/fret positions together. Tempo
changes are scoped to the selected bars and restore the following bar's tempo
when one exists. Meter changes scale each rhythmic value exactly, preserve all
attacks, and fail when the source bars are incomplete or the exact durations
cannot be represented. A plan that no longer matches the score is rejected
without partial edits.

Two existing riffs can be combined through `generation::hybridize`. The caller
chooses whether to keep A's rhythm and use B's contour, keep A's pitches and
use B's articulation, place B's attacks into rests in A's low framework, or
alternate bars from both sources. The documents must have matching bar counts
and meters; impossible contour fingerings return an error. The seed controls a
repeatable attack alignment. `filter_candidates` then reports all requested
hard-requirement failures and caller-supplied metric bounds, alongside the full
assessment report. It accepts candidates without ranking them, leaving the
ranking policy with the caller.

For an existing authored score, `generate_accompaniment` requests drums, bass,
harmony, and lead in one call. Set `AccompanimentLayers` to choose roles and
provide optional role-specific profiles or configurations on
`AccompanimentRequest`; its shared seed is mixed with a stable role tag.

An explicit second-guitar configuration can add support, double, harmonise,
octave, or contextual roles through `generate_second_guitar`, or as part of
`generate_accompaniment`. Support thins attacks; double and octave keep the
source rhythm. Contextual mode chooses a role for each marked section from its
source note density. The generated part is refingered in its requested tuning
and checked against H7. H6 refuses a third fretted non-bass track unless the
caller enables force; force preserves all existing tracks and adds the request.
`AccompanimentResult` reports each role as generated, not requested, already
present, or unavailable because a feature is disabled. Existing percussion and
bass are recognized from score classification; harmony and lead are recognized
by track name. Set `force` to append a new layer despite a matching authored
role. Existing tracks are retained, and a generation error leaves the document
unchanged.

`GeneratedTrack` carries an `ExecutionLog` alongside its counts. Counts say a
track was written; the log says why each bar looks the way it does -- the
choice made, the phrase position it was made at, the per-bar seed it was
derived from, and how many events it produced. `generation::explain` takes a
diagnostic's bar and track and returns the decisions behind it, which is the
step between "bar 12 repeats an earlier pattern" and the decision that repeated
it. A diagnostic with no track returns every generator's decisions in that bar,
because a score-wide finding cannot be attributed to one of them.

A log is serialized beside the score through `ExecutionLog::to_json`, never
written into the score container. The payload writer emits exactly the shapes
the originating application produces, and inventing an element to carry
provenance would break that invariant and leave a file whose behaviour in other
applications is unknown.

The facade keeps result types specific to each operation so errors and
diagnostics remain meaningful. It does not turn failures into fallback output.

## Capability status

Optional build features are visible to callers through
`facade::generation::capability_status`:

```rust
use tabslib::facade::generation::{
    capability_status, Capability, CapabilityStatus, UnsupportedReason,
};

match capability_status(Capability::SoloGeneration) {
    CapabilityStatus::Available => { /* use the native operation */ }
    CapabilityStatus::Unsupported(UnsupportedReason::FeatureDisabled) => {
        /* rebuild with the required feature */
    }
    CapabilityStatus::Unsupported(UnsupportedReason::NotImplemented) => {
        /* select another workflow or report the unsupported operation */
    }
    _ => unreachable!("non-exhaustive status variants may be added later"),
}
```

Score inspection and document edits are always available. Requirement checking
follows `assessment`. Drum and bass
generation require both `drums` and `rhythm`; harmony and solo generation use
`harmony`; arrangement and assessment are controlled by their matching Cargo
features. Token-based solo composition requires both `harmony` and
`assessment`. Form planning, section transitions, and section analysis follow
`arrangement`; pattern
catalogs and drum performance follow `drums`. Riff and motif authoring uses `riffs`.
Riff key, tempo, and meter adaptation also uses `riffs`.
Riff hybridization and candidate filtering require `riffs` and `assessment`.
Rhythmic vocabulary extraction uses `rhythm`; applying profiles to `RiffSpec`
also requires `riffs`.
Second-guitar generation follows `harmony`.
Arrangement search and hard-requirement ranking require both `arrangement` and
`assessment`.
Provider-neutral prompt and rationale workflows use `ai`. AI provider
integrations report `Unsupported(FeatureDisabled)` unless at least one of
`ai-anthropic`, `ai-ollama`, `ai-openai-compatible`, or `ai-gemini` is
enabled. A caller can
inspect this status before doing work; no legacy model or provider is selected
implicitly.
Candidate judging uses `ai`; structured tab critique needs both `ai` and
`ascii`.
Accompaniment orchestration is always available, and its result reports
feature-disabled roles individually.
Named tuning presets are also always available through `generation` and
`pitch`; their open-string lists follow the document model's lowest-string-first
convention. Callers can resolve names case-insensitively, reverse-resolve an
exact pitch list, or pass a preset name directly to either retune operation.

`generation::search_arrangements` samples distinct plans from caller-selected
motifs with a repeatable seed, composes each plan, and ranks the results using
caller-supplied assessment weights, hard-requirement penalties, and optional
time-budget penalties. Every result carries its editable plan, composed score,
assessment report, hard-requirement violations, exact playback duration, and
weighted score. `bar_duration` represents meter and finite tempo values as an
exact rational number of seconds. `bars_for_minutes` counts whole bars in a
repeating playback pattern without exceeding the requested duration, while
`resolve_repeats` reports the closest whole-playback repeat count and its
achieved duration when an exact target is impossible.

`FeatureVector::from_document` measures density, register, voicing mix,
palm-mute rate, and explicit time feel from a score. `build_candidate_deck`
selects caller-provided documents using a seeded max-distance pass and the
caller-supplied archetype labels. Every selected pair meets the requested
minimum feature distance or the builder reports how many candidates it could
select. `CandidateDeck` pages forward and backward without reordering the
selected entries.

## Migration map

The native facade keeps the useful workflow while replacing file-shaped inputs
with a `Document`. Read or write bytes only at the application boundary using
`facade::load`, `facade::save`, or the optional `facade::format` adapters.

| File-oriented workflow | Native API |
| --- | --- |
| Analyze score structure and harmony | `facade::inspect::summary`, `statistics`, `determine_key`, `analyze_bar_chords`, `analyze_bar_chords_in_key`, `frictions_between` |
| Rename, reorder, slice, or transform tracks | `facade::edits` and `facade::pitch` |
| Resolve a standard guitar or bass tuning and retune by name | `generation::resolve_tuning_preset`, `retune_preserve_pitch`, and `retune_preserve_fingering` |
| Generate drums from source tracks | `generation::generate_drums` with `DrumGenerationRequest` |
| Generate a riff-locked bass track | `generation::generate_bass` with `BassGenerationRequest` |
| Add a harmony or second-guitar track | `generation::generate_harmony` with `HarmonyConfig` |
| Generate an explicit-role second guitar | `generation::generate_second_guitar` with `SecondGuitarConfig` |
| Add selected accompaniment roles to an authored document | `generation::generate_accompaniment` with `AccompanimentRequest` |
| Search and rank arrangement plans against a time budget | `generation::search_arrangements` with `ArrangementSearchRequest` and caller-supplied `ArrangementSearchWeights` |
| Generate a lead track | `generation::generate_solo` with `SoloConfig` |
| Compose and finger a token-defined solo | `generation::compose_solo_tokens` and caller-ranked `rank_solo_candidates` |
| Build and page a feature-spaced candidate deck | `generation::FeatureVector`, `build_candidate_deck`, and `CandidateDeck` |
| Write a riff from nothing | `generation::write_riff` with `RiffSpec` |
| Measure and compare rhythmic vocabulary | `generation::extract_rhythm_cells`, `extract_interval_ngrams`, `build_rhythmic_vocabulary`, and `cosine_similarity` |
| Apply caller rhythm profiles | `generation::form_prior`, `blend_vocabularies`, and `apply_riff_profile` |
| Adapt an existing riff across key, tempo, or meter | `generation::plan_riff_adaptation` then `apply_riff_adaptation` with `RiffAdaptationRequest` |
| Combine two riffs or filter candidates | `generation::hybridize` and `filter_candidates` with `HybridRequest` and caller-supplied `MetricThreshold` values |
| State and answer a figure | `generation::write_motif` with `MotifSpec` |
| Segment riffs and assemble sections | `generation::segment_motifs` then `generation::arrange` |
| Plan a song form | `generation::plan_form` with `FormTemplate` |
| Treat section boundaries | `generation::plan_transitions` and `apply_transitions` with `TransitionRule` |
| Detect content sections and isolated riff stabs | `generation::detect_section_boundaries`, `detect_riff_stabs`, and explicit marker proposals |
| Select a pattern under a seed | `generation::PatternCatalog` with `PatternGrid` and `PatternConstraints` |
| Humanize, fill, or improve a drum performance | `generation::generate_seeded_hits`, `humanize_hits`, `PhraseClock`, `improve_drums` |
| Index, select, and adapt a MIDI drum groove | `generation::MidiGrooveIndex`, `midi_to_drum_hits`, `generate_from_midi_index` (enable `midi`) |
| Change one section of a plan | `generation::edit_plan` with `PlanEdit` and `SlotAddress` |
| Apply a list of intended changes | `generation::edit_plan_all`, reading `EditReport::rejected` |
| Shape song energy and tempo, then rebuild an edited arrangement | `generation::apply_energy_contour`, `apply_tempo_contour`, `diff_plan_sections`, and `compose_plan_incremental` |
| Measure or calibrate generated output | `generation::assess_document` and `calibrate_document` |
| Trace a finding back to the decision that caused it | `generation::explain` over `GeneratedTrack::log` |
| Fail a check on a metric | `generation::gate_document` with `GatePolicy` |
| Log what a listener preferred | `generation::blind_record` and `append_record` |
| Train and evaluate a preference ranker | `generation::train_preference_ranker`, `rank_preference_candidates`, and `preference_ranker_agreement` |
| Report findings to a person | `AssessmentReport::summary` or `generation::group_diagnostics` |
| Reject a candidate that violates a hard requirement | `generation::check_requirements` with `Requirement` and `Violation` |
| Prepare and validate provider rationale | `generation::PromptInput`, `capture_request`, and `generate_rationale` |
| Judge and blend candidate rankings | `generation::judge_candidates`, `offline_heuristic_judge`, and `blend_rankings_with_deterministic` |
| Measure agreement with recorded preferences | `generation::judge_preference_agreement` |
| Critique an ASCII tablature rendering | Enable `ai` and `ascii`, then use `generation::critique_tab` |
| Label sections or request merge/split suggestions | `generation::SectionFingerprint`, `generate_section_labels`, and `generate_section_boundaries` |
| Ask Anthropic, Ollama, an OpenAI-compatible service, or Gemini for assessment rationale | Enable the matching `ai-anthropic`, `ai-ollama`, `ai-openai-compatible`, or `ai-gemini` feature and build the connector with `AiConfig` |

The native API leaves orchestration, storage paths, backups, and provider
choices with the calling application. No compatibility adapter accepts a
foreign score object; convert once at the format boundary and keep the full
workflow on `Document`.

## Safety, diagnostics, and JSON

GP loading applies bounded archive, entry, decompression-ratio, and GPIF XML
limits by default. Applications that need a different policy can use
`format::gp::Gp::read_with_limits` or `format::gp::parse_payload_with_limits`.
Malformed or over-limit input returns a typed `Error`; it is never truncated
into a plausible document.

`validation::validate` checks document references, identifiers, pitches,
fretting ranges, and written structure. `load_with_diagnostics` returns a
successfully loaded document together with those non-fatal `Diagnostic` values.
For caching or interchange, the optional `json` feature provides deterministic,
versioned `serialization::encode_document` and `decode_document` functions, as
well as `Document::to_json` and `Document::from_json`.
