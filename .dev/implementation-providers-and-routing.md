# TokenScavenger Providers and Routing

## 1. Provider Abstraction

Every upstream provider must implement a common trait that isolates transport, capability discovery, model normalization, and response translation.

Suggested trait shape:

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn supports_endpoint(&self, endpoint: EndpointKind) -> bool;
    fn auth_kind(&self) -> AuthKind;
    fn base_url(&self, config: &ProviderConfig) -> Url;
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap;
    fn capabilities(&self) -> ProviderCapabilities;

    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError>;

    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError>;

    async fn embeddings(
        &self,
        ctx: &ProviderContext,
        request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError>;
}
```

Provider adapters must not know about global routing order or fallback chains. They are purely provider-specific implementations.

## 2. Supported Providers

The original spec requires TokenScavenger to ship with the following baked-in free-provider matrix and treat it as the default catalog basis:

- Google AI Studio / Gemini
- Groq
- OpenRouter free models
- Cloudflare Workers AI
- Cerebras
- NVIDIA NIM
- Cohere
- Mistral AI
- GitHub Models
- Hugging Face Serverless Inference
- Z AI / Zhipu AI
- SiliconFlow

Implementation expectations:

- Each provider gets a dedicated adapter module.
- Each adapter includes discovery logic, request translation, response normalization, auth handling, and rate-limit/quota hint parsing.
- Each adapter must declare whether it is:
  - natively OpenAI-compatible
  - OpenAI-compatible with quirks
  - non-OpenAI-native and requires translation

## 3. Provider Catalog Requirements

Maintain two layers of model/provider metadata:

1. Curated built-in catalog shipped with the binary.
2. Dynamically discovered provider models fetched from upstream endpoints.

Curated catalog fields:

- provider id
- provider display name
- default base URL
- auth scheme
- discovery endpoint kind
- built-in example models
- model family
- modality support
- free/paid classification
- provider region notes
- known quirks
- docs URL
- enabled-by-default flag

Discovered model fields:

- provider id
- upstream model id
- display name if available
- endpoint compatibility
- context window if available
- token/output limits if available
- availability status
- discovered timestamp
- raw upstream metadata blob for debugging

Merge policy:

- Curated metadata is the baseline.
- Discovery augments and updates volatile fields.
- Manual operator overrides always win over discovery.

## 4. Discovery Behavior

On startup:

- Run discovery in parallel for all enabled providers with per-provider timeout.
- Persist both success and failure outcomes.
- If discovery fails for a provider, keep the last successful catalog entry if one exists.

At runtime:

- Scheduled refresh every configurable interval.
- Manual refresh endpoint and UI button.
- Debounce concurrent refreshes for the same provider.

Discovery result states:

- `fresh`
- `stale`
- `error_last_attempt`
- `never_discovered`
- `disabled`

The UI must surface these states clearly.

## 5. Normalized Model Representation

Use an internal canonical model record:

```rust
pub struct CatalogModel {
    pub provider_id: String,
    pub upstream_model_id: String,
    pub public_model_id: String,
    pub endpoint_kinds: Vec<EndpointKind>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_json_mode: bool,
    pub free_tier: bool,
    pub paid_fallback: bool,
    pub priority_hint: i32,
    pub health_score: f32,
    pub operator_enabled: bool,
    pub metadata: serde_json::Value,
}
```

`public_model_id` supports aliases such as `free:llama-3.3-70b`.

## 6. Routing Policy

Routing is a policy engine, not a hard-coded `for` loop. The engine must combine:

- requested endpoint kind
- requested public model or alias
- explicit operator priority
- free-first strategy
- provider health
- breaker state
- recent rate-limit/quota information
- model capability compatibility
- optional tenant/auth-specific policy later

Suggested routing stages:

1. Resolve requested model token to one or more candidate public targets.
2. Expand targets to provider-model pairs.
3. Filter unsupported or disabled candidates.
4. Apply operator ordering.
5. Adjust order by dynamic health and known quota exhaustion.
6. Produce a deterministic attempt plan with audit metadata.

Each request should carry a route plan ID for logging and UI introspection.

## 7. Alias and Fallback Semantics

Alias support requirements:

- Exact provider model IDs remain valid.
- Public aliases map to one or more provider-model pairs.
- Example alias: `free:llama-3.3-70b`.
- Alias resolution must be inspectable and editable in config/UI.

Fallback levels:

1. Same provider, alternate model if explicitly configured.
2. Different free-tier provider for equivalent alias/model family.
3. Lower-priority free-tier providers.
4. Paid fallback group if enabled.

The implementation must never silently route to paid providers unless policy explicitly allows it.

## 8. Retry Policy

Retry classification by provider error:

- Retryable:
  - connect timeout
  - TLS handshake interruption
  - upstream 5xx
  - 429 with retryable semantics
  - transient network reset
- Not retryable on same provider/model:
  - 400 validation errors
  - unsupported parameter
  - invalid local credential
  - explicit quota exhaustion without reset hint

Cross-provider fallback may still proceed after a non-retryable same-provider error if the error implies provider incompatibility rather than caller invalidity.

Configurable retry controls:

- max attempts per provider
- exponential backoff base
- exponential backoff cap
- jitter on/off
- total request budget ceiling

## 9. Circuit Breakers and Health

Provider health data sources:

- passive request outcomes
- active probes
- discovery freshness
- observed rate-limit headers
- last successful request timestamp

Suggested health states:

- `healthy`
- `degraded`
- `rate_limited`
- `quota_exhausted`
- `unhealthy`
- `disabled`

Routing behavior by state:

- `healthy`: normal priority
- `degraded`: lower weighted priority
- `rate_limited`: temporarily deprioritize; retry after header or cooldown
- `quota_exhausted`: remove from free-first routing until reset window
- `unhealthy`: avoid except in explicit override or half-open probe
- `disabled`: never route

Quota reset tracking should use authoritative provider headers when available; otherwise use operator-configured heuristics and label the estimate as inferred.

## 10. Request Translation Rules

The provider boundary must handle:

- auth header differences
- base URL path differences
- chat message schema differences
- embeddings schema differences
- streaming protocol quirks
- usage metadata extraction
- finish reason normalization
- tool call / function call representation differences where supported

If a provider does not support a requested feature:

- either reject the provider candidate and continue fallback
- or transform safely if the semantics are equivalent

Do not silently drop critical features like tool calls or JSON mode if the caller explicitly requested them; fallback or fail with a clear compatibility error.

## 11. Streaming Rules

Chat completion streaming must be normalized to OpenAI-style SSE:

- send `data: {json}\n\n` frames
- terminate with `data: [DONE]\n\n`

Implementation requirements:

- Provider streaming parser per adapter.
- Unified event enum for internal streaming.
- Backpressure-safe bridge from provider stream to client response.
- On provider stream failure mid-flight, log partial usage and failure state.

Mid-stream fallback should not be attempted for chat completions. Once streaming begins from a provider, that attempt is authoritative. Pre-stream fallback is allowed.

## 12. `GET /v1/models` Contract

The models endpoint must return the public merged catalog, not raw provider output.

Requirements:

- include enabled models only by default
- optional query params for admin/debug views to include disabled or stale models
- expose alias entries and underlying provider-backed entries
- include enough metadata for UI consumption without requiring a separate internal API for the same data

## 13. Default Fallback Order

The original spec documents this recommended default starter order:

1. Groq
2. Cerebras
3. Google AI Studio
4. OpenRouter free
5. Cloudflare
6. NVIDIA NIM
7. Mistral
8. GitHub Models
9. Z AI
10. Paid fallback

Implementation requirement:

- Ship this as the initial default policy.
- Allow operator override in config/UI.
- Keep additional baked-in providers available even if not all are in the first-order chain.

## 14. Provider-Specific Validation and Test Fixtures

Each adapter must ship with:

- fixture responses for discovery
- fixture responses for chat completions
- fixture responses for embeddings
- fixture responses for rate-limit/quota conditions
- fixture streaming transcripts where applicable

Each adapter must also define:

- config validation rules
- required secrets/env vars
- supported endpoint matrix
- known incompatibility matrix

## 15. Acceptance Criteria

Provider/routing implementation is complete when:

- all required providers have adapters or a clearly marked phase-based implementation plan consistent with the original roadmap
- discovery works against fixtures and real providers where credentials are available
- alias resolution is deterministic
- fallback logs clearly explain why each provider was skipped or attempted
- quota/rate-limit states influence routing
- circuit breakers change routing behavior and recover correctly
- streaming works on at least the primary free-tier chat providers in end-to-end tests

## 16. Providers And Routing Checklist

Mark each item as the provider/routing work lands and its tests pass.

- [x] Define the provider adapter trait and shared provider context/error types.
- [x] Implement the curated built-in provider/model catalog.
- [x] Implement dynamic discovery, merge policy, persistence, and stale/error states.
- [x] Implement provider registry initialization from config and catalog data.
- [x] Add adapters or explicitly phased placeholders for the required provider set.
- [x] Add fixture coverage for discovery, chat, embeddings, streaming, rate limits, and quota exhaustion for each adapter.
- [x] Implement normalized model records, public aliases, and deterministic alias resolution.
- [x] Implement the route planner with endpoint capability filtering, operator priority, free-first ordering, health, breakers, and quota hints.
- [x] Implement same-provider retries and cross-provider fallback classification.
- [x] Enforce explicit opt-in before paid fallback is used.
- [x] Normalize provider request/response differences, including streaming SSE.
- [x] Ensure skipped/attempted providers are visible in logs, metrics, and UI explanations.
- [x] Cover route planning, fallback, quota state, and breaker recovery in tests.
