# oneclaw-providers

LLM provider implementations for OneClaw.

Implements the `LlmProvider` trait from `oneclaw-core` for various backends.

## Providers

- **OllamaProvider** — Local inference via Ollama HTTP API. Default model: `llama3.2:1b`. Ideal for edge deployment with no cloud dependency.
- **OpenAICompatProvider** — Any OpenAI-compatible API (OpenAI, Azure, local servers). Supports API key from config or environment variables (`OPENAI_API_KEY`, `ONECLAW_OPENAI_KEY`).

## Usage

```rust
use oneclaw_providers::OllamaProvider;

let provider = OllamaProvider::from_config(&config.providers.ollama);
runtime.provider_mgr.register(Box::new(provider));
```
