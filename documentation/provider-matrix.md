# Provider Support Matrix

TokenScavenger ships with 14 built-in provider adapters. This document details each provider's API format, capabilities, free-tier limits, paid fallback behavior, and known quirks.

## Legend

| Icon | Meaning |
|------|---------|
| ✅ | Fully supported and tested |
| ⚠️ | Has quirks, works with caveats |
| 🚧 | Implementation in progress |
| ❌ | Not supported |

## Provider Comparison

| Provider | API Format | Chat | Streaming | Tools | Embeddings | Vision | Free Tier |
|----------|-----------|------|-----------|-------|------------|--------|-----------|
| Groq | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |
| Google Gemini | Native | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| OpenRouter | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ (:free suffix) |
| Cerebras | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ |
| Mistral AI | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| NVIDIA NIM | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| Cloudflare | OpenAI-compat | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ (10k neurons/day) |
| GitHub Models | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ (15 req/min) |
| HuggingFace | OpenAI-compat | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ (1k req/day) |
| SiliconFlow | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ (1k RPM free) |
| ZAI / Zhipu | Semi-OpenAI | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ (flash models) |
| Cohere | Native v2/chat | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ (1k calls/month) |
| DeepSeek | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ❌ | Paid fallback |
| xAI (Grok) | OpenAI-compat | ✅ | ✅ | ✅ | ❌ | ✅ | Paid fallback |

## Provider Details

### Groq

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.groq.com/openai/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Models endpoint** | `GET /models` |
| **Format** | Fully OpenAI-compatible |
| **Free models** | `llama3-70b-8192`, `llama3-8b-8192`, `mixtral-8x7b-32768` |
| **Quirks** | None — fully OpenAI compatible |
| **Rate limits** | Per-model rate limits, visible in Groq console |
| **Docs** | https://console.groq.com/docs |

### Google Gemini

| Property | Value |
|----------|-------|
| **Base URL** | `https://generativelanguage.googleapis.com/v1beta` |
| **Auth** | `x-goog-api-key` header (NOT Bearer) |
| **Chat endpoint** | `POST /models/{model}:generateContent` |
| **Stream endpoint** | `POST /models/{model}:streamGenerateContent` |
| **Models endpoint** | `GET /models` |
| **Format** | Native — uses `contents[{role, parts[{text}]}]` format |
| **Free models** | `gemini-2.0-flash`, `gemini-1.5-flash` |
| **Quirks** | ⚠️ Completely different format from OpenAI. Model is in URL path, not request body. Messages use `parts` array instead of simple `content` string. System instructions via separate `systemInstruction` field. |
| **Rate limits** | 60 requests/minute (free tier) |
| **Docs** | https://ai.google.dev/gemini-api/docs |

### OpenRouter

| Property | Value |
|----------|-------|
| **Base URL** | `https://openrouter.ai/api/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Models endpoint** | `GET /models` |
| **Format** | Fully OpenAI-compatible |
| **Free models** | Any model with `:free` suffix: `meta-llama/llama-3.3-70b-instruct:free` |
| **Quirks** | ⚠️ Extra headers: `HTTP-Referer`, `X-Title` (recommended for rankings). Model format is `provider/model`. Can pass a `models` array for automatic fallback. |
| **Rate limits** | Separate limits for free vs paid users. Check via `GET /v1/key`. |
| **Docs** | https://openrouter.ai/docs |

### Cerebras

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.cerebras.ai/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Format** | Fully OpenAI-compatible |
| **Free models** | `llama3.1-8b`, `gpt-oss-120b` |
| **Quirks** | ⚠️ Extra `time_info` field in every response. Custom rate limit headers (`x-ratelimit-remaining-requests-day`, etc.). |
| **Rate limits** | 30 RPM, 64K TPM per model (free tier) |
| **Docs** | https://inference-docs.cerebras.ai |

### Mistral AI

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.mistral.ai/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Models endpoint** | `GET /models` |
| **Format** | Fully OpenAI-compatible |
| **Free models** | `open-mistral-nemo`, `mistral-small-latest`, `ministral-8b-latest` |
| **Quirks** | None — clean OpenAI compatibility |
| **Rate limits** | ~1-5 req/s on Experiment plan, varies by model |
| **Docs** | https://docs.mistral.ai |

### NVIDIA NIM

| Property | Value |
|----------|-------|
| **Base URL** | `https://integrate.api.nvidia.com/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Format** | OpenAI-compatible |
| **Free models** | 60+ models from `meta/llama-3.1-8b-instruct` to `deepseek-ai/deepseek-v4-pro` |
| **Quirks** | ⚠️ Model format is `author/model-name`. Extra `extra_body` parameter for model-specific settings. |
| **Rate limits** | Rate-limited free tier (NVIDIA API Trial terms) |
| **Docs** | https://build.nvidia.com/docs |

### Cloudflare Workers AI

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1` |
| **Auth** | `Authorization: Bearer <token>` |
| **Chat endpoint** | `POST /chat/completions` (OpenAI-compat) or `POST /run/{model}` (native) |
| **Format** | OpenAI-compatible endpoint available |
| **Free models** | `@cf/meta/llama-3.3-70b-instruct-fp8-fast`, `@cf/meta/llama-3.1-8b-instruct` |
| **Quirks** | ⚠️ Account ID must be in the URL path. 10,000 neurons/day free allocation. Native format has different response wrapping. |
| **Rate limits** | 300 req/min for text generation |
| **Docs** | https://developers.cloudflare.com/workers-ai |

### GitHub Models

| Property | Value |
|----------|-------|
| **Base URL** | `https://models.inference.ai.azure.com` |
| **Auth** | `Authorization: Bearer <github_pat>` (requires `models:read` scope) |
| **Chat endpoint** | `POST /chat/completions` |
| **Format** | Fully OpenAI-compatible |
| **Free models** | `openai/gpt-4o-mini`, `meta-llama/Llama-3.3-70B-Instruct`, `mistralai/Mistral-Large`, `deepseek/DeepSeek-R1` |
| **Quirks** | ⚠️ Uses GitHub Personal Access Token as Bearer token. Rate limits vary by model tier. |
| **Rate limits** | 15 req/min (low tier), 10 req/min (high tier), 150/day and 50/day respectively |
| **Docs** | https://docs.github.com/en/github-models |

### HuggingFace Serverless Inference

| Property | Value |
|----------|-------|
| **Base URL** | `https://api-inference.huggingface.co/v1` |
| **Auth** | `Authorization: Bearer hf_xxx` |
| **Chat endpoint** | `POST /chat/completions` |
| **Format** | OpenAI-compatible (new API) |
| **Free models** | `google/gemma-2-2b-it`, `microsoft/Phi-3-mini-4k-instruct` |
| **Quirks** | ⚠️ Primarily CPU inference for most models. Newer Inference Providers router adds GPU options via partners. |
| **Rate limits** | 1,000 requests/day (free), 20,000/day (PRO) |
| **Docs** | https://huggingface.co/docs/api-inference |

### SiliconFlow

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.siliconflow.cn/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Models endpoint** | `GET /models` |
| **Format** | Fully OpenAI-compatible |
| **Free models** | `Qwen/Qwen2.5-7B-Instruct` and many others |
| **Quirks** | ⚠️ Paid models prefixed with `Pro/` (e.g., `Pro/Qwen/Qwen2.5-7B-Instruct`). Chinese provider — documentation primarily in Chinese. |
| **Rate limits** | 1,000 RPM for free models |
| **Docs** | https://docs.siliconflow.cn |

### ZAI / Zhipu AI

| Property | Value |
|----------|-------|
| **Base URL** | `https://open.bigmodel.cn/api/paas/v4` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` (path is `/api/paas/v4/chat/completions`) |
| **Format** | Mostly OpenAI-compatible, different base path |
| **Free models** | `glm-4.7-flash`, `glm-4-flash-250414` |
| **Quirks** | ⚠️ URL path is `/api/paas/v4/` not `/v1/`. Extra parameters: `thinking`, `tool_stream`, `do_sample`. No `/models` endpoint — uses curated catalog. |
| **Rate limits** | Free tier available for flash models |
| **Docs** | https://open.bigmodel.cn/dev/api |

### Cohere

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.cohere.com` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /v2/chat` |
| **Format** | Native v2 format — NOT OpenAI compatible |
| **Free models** | `command-a-03-2025`, `command-r7b-12-2024` (trial keys) |
| **Quirks** | ⚠️ Uses `/v2/chat` not `/v1/chat/completions`. Content is an array of objects `[{type: "text", text: "..."}]`. SSE uses named event types. Trial: 1,000 calls/month, 20 req/min. |
| **Rate limits** | 20 req/min (trial), 500 req/min (production) |
| **Docs** | https://docs.cohere.com |

### DeepSeek

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.deepseek.com` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Models endpoint** | `GET /models` |
| **Format** | OpenAI-compatible |
| **Paid models** | `deepseek-v4-flash`, `deepseek-v4-pro` |
| **Quirks** | ⚠️ Default OpenAI-compatible base URL omits `/v1`. Legacy compatibility names `deepseek-chat` and `deepseek-reasoner` may still work but are scheduled for deprecation by DeepSeek. |
| **Routing** | Configure with `free_only = false`; TokenScavenger routes to it only when `[routing].allow_paid_fallback = true`. |
| **Docs** | https://api-docs.deepseek.com/ |

### xAI (Grok)

| Property | Value |
|----------|-------|
| **Base URL** | `https://api.x.ai/v1` |
| **Auth** | `Authorization: Bearer <key>` |
| **Chat endpoint** | `POST /chat/completions` |
| **Models endpoint** | `GET /models` |
| **Format** | OpenAI-compatible |
| **Paid models** | `grok-4.20`, `grok-4.20-reasoning` |
| **Quirks** | ⚠️ xAI treats chat completions as a compatibility endpoint. Reasoning models can reject some classic chat parameters such as stop sequences and penalties. |
| **Routing** | Configure with `free_only = false`; TokenScavenger routes to it only when `[routing].allow_paid_fallback = true`. |
| **Docs** | https://docs.x.ai/docs/api-reference |
