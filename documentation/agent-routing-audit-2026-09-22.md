# Agent routing implementation audit: 22 September 2026

Scope: commits `7c0f54e` and `005c5fa` on `agent-subtask-routing`, compared with `.dev/implementation-agent-routing.md`, AGENTS.md, the existing routing and provider contracts, and the public HTTP tests. This audit applies the repository's lazarus-mode correctness, isolation, performance, failure, and operational review order. All fixes below are in the current working tree; this report does not claim they have been committed or released.

## Findings fixed

| Severity | Failure mode | Repair and proof |
| --- | --- | --- |
| High | A project allowed to request an adaptive alias could reach every tier group, because membership in the operator's profile was treated as project authorisation. | Require both alias and selected group in a nonempty project allowlist. HTTP project-key test allows `advanced`, then rejects the same task after policy permits only `economy`. |
| High | Classifier dispatch preceded alias authorisation and ignored project provider allow/deny lists. Rejected requests could still send task material to a disallowed provider. | Check alias and project enabled state before classifier work; gate the classifier on project provider restrictions. HTTP tests verify zero classifier usage when the provider or alias is denied. |
| High | `X-Request-Id` was client-controlled in the request-project map and SQLite primary key, despite a separate server ID existing. Reuse could replace another request row or active project context. | Atomically claim an active ID and check persisted IDs for client-supplied values; issue a new ID on collision and return the effective ID in the response header. Preserve project context until persistence completes and clean it up on success/error/stream drop. Sequential cross-project and concurrent duplicate-ID HTTP tests verify distinct rows and attribution. |
| High | Phase detection treated earlier valid tool results as orphaned when an agent reached a second tool round; a call with no tool result could be treated as an initial task. | Validate all tool rounds in order, including duplicate/orphan/partial IDs. Unit and public HTTP tests cover two completed rounds and missing results. |
| High | Streaming affinity could be committed after `[DONE]` entered an internal channel, before the caller consumed it. | Require both upstream completion and consumer delivery of `[DONE]` before committing the pin. A public HTTP test drops an unread completed stream and confirms the next required continuation fails safely; existing completed/partial stream tests pass. |
| Medium | A busy affinity scope could expire and advance generation before checking its active lease; the old lease then could not release `in_flight`. | Reject concurrent admission before expiry reset. Fake-clock test proves the active lease can release after its absolute deadline. Sweep and commit now share admission locking. |
| Medium | On restart, any usage row with the same request and purpose could clear all uncertain reservations, even when several provider attempts existed. | Keep crash-uncertain reservations as conservative charges until exact attempt-level reconciliation exists. Restart test covers a same-purpose usage row from another attempt. |
| Medium | Tool preference did one SQL lookup per candidate, creating a linear latency cliff despite other planner reads being batched. | Batch capability lookup in chunks of 80 candidate pairs, preserving ordering. Existing routing tests and full suite pass. |
| Low | Classifier per-project admission counters retained zero-count keys across project churn. | Remove zero-count entries atomically; 1,000-project churn test passes. |
| Medium | A profile with required affinity accepted a request without a session ID, leaving no possible pin. An initial tool request could select an adapter whose continuation cannot be replayed. | Reject missing session with `400 session_required`; exclude unsupported continuation adapters when tools and affinity are present. Public HTTP test covers required session enforcement. |

## Validation

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --all-features`: passed after fixes; one pre-existing 30-minute affinity soak remains ignored by default.
- Targeted public HTTP tests cover project policy and classifier isolation, reused IDs under sequential and concurrent callers, multiple tool rounds, completed and dropped streams, and unread queued `[DONE]`.
- `cargo bench --bench benchmarks -- --sample-size 10`: passed. On this Mac, `route_plan_build_empty` was 1.1047–1.1339 µs, `route_plan_large_catalog_1000` was 248.99–251.28 µs, and `agent_phase_and_rule` was 148.77–150.56 ns. Criterion reported large changes against its saved baseline in several unrelated paths, including configuration parsing and SQLite writes. These results are absolute measurements, not a controlled before/after attribution for this patch.
- `git diff --check`: passed.

## Remaining limits and follow-up gates

The automatic classifier has no real-model quality or net-cost study; its routing quality remains experimental. Strict budget guarantees also depend on provider usage and pricing being known: reservations are conservative estimates, and uncertain attempts retain charges rather than being silently cleared. Exact recovery of retained charges needs a shared attempt ID between each reservation and usage event. The current conservative approach can temporarily deny legitimate paid work after a crash; it avoids reopening a budget through an unrelated usage row.

Affinity remains process-local and needs session stickiness across replicas. Provider-specific opaque continuation metadata is still unsupported. The current compatibility matrix assumes the non-Google OpenAI-shaped adapters replay tool history correctly; adapter-by-adapter fixture coverage should be completed before promising strict affinity for every model family. The classifier cache capacity is built from startup configuration, so changing that capacity through hot reload requires a restart; per-entry age is checked against the live TTL, but the underlying cache's startup TTL remains in force. These limits are documented rather than treated as passing end-to-end proof.

No production credentials or external providers were required for ordinary tests. The initial Grok implementation's optional real-model study and a controlled disabled-path throughput comparison remain unverified release gates.
