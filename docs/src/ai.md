# AI connectors

AI support is optional and off by default. The `ai` feature adds prompt
construction over structured assessment facts, response validation, blind
candidate comparison and the `RationaleProvider` trait; it makes no network
calls. Each connector is a separate feature on top of it:

| feature | service | API key (when not set on the config) |
| --- | --- | --- |
| `ai-anthropic` | Anthropic Messages API, default model `claude-sonnet-5-5` | `ANTHROPIC_API_KEY` |
| `ai-openai-compatible` | any OpenAI-compatible Chat Completions endpoint | `OPENAI_API_KEY`, optional |
| `ai-gemini` | Google Gemini | `GEMINI_API_KEY` |
| `ai-ollama` | an Ollama server | none |

Configure any of them through one type, `AiConfig`:

```rust,ignore
use tabslib::generation::ai::{generate_rationale, AiConfig, AiConnector, PromptInput, ProviderOptions};
use tabslib::generation::assessment::assess;

let provider = AiConfig::new(AiConnector::Anthropic)
    .with_options(ProviderOptions::default().with_max_retries(2))
    .build()?;

let report = assess(&document);
let rationale = generate_rationale(&provider, PromptInput::rationale("draft", &report))?;
```

`with_model`, `with_endpoint`, `with_api_key` and `with_max_tokens` override
the connector's defaults. The connectors are also public types
(`AnthropicProvider`, `OpenAiCompatibleProvider`, `GeminiProvider`,
`OllamaProvider`) for callers who prefer to build them directly, and any type
implementing `RationaleProvider` can stand in for them. Calls are synchronous;
run them on a blocking worker from async code. Retries are off by default.
The crate, connectors included, needs Rust 1.88 or newer.
