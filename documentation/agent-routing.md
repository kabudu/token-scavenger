# Subtask routing

Subtask routing is off unless `[routing.agent] enabled = true`. A request whose `model` is not one of the configured profile names keeps today's route: the same filters, the same order, and no affinity state.

The feature chooses a capability tier for one labelled subtask, then optionally keeps later turns of that subtask on the provider and model that already served it. It does not split one model call into hidden tasks, run tools, or pin a whole session.

## Caller contract

Send one chat request per task, with these headers. They are reserved even when the feature is disabled: malformed values return `400`, and they are never forwarded upstream.

| Header | Meaning |
| --- | --- |
| `x-ts-session` | Opaque run id, 1–128 characters from `[A-Za-z0-9._:-]` |
| `x-ts-subtask` | Opaque subtask id. Requires `x-ts-session`. Sibling subtasks stay independent. |
| `x-ts-task-type` | Operator label such as `plan` or `extract`, 1–64 characters. |
| `x-ts-tier` | `auto`, `economy`, `standard`, or `advanced`. |
| `x-ts-phase` | `auto`, `planner`, `tool_result`, or `finalize`. Structure wins over this hint. |
| `x-ts-affinity` | `off`, `prefer`, or `required`, and it cannot weaken an operator-required pin. |

The total size of these headers is limited to 1 KiB. Duplicate headers are rejected. A session without a subtask is one default branch; parallel work needs distinct subtask ids.

Unauthenticated mode is one shared trust domain (`principal_id = master`). Do not treat that as per-client isolation.

```python
# Example only. TokenScavenger does not require this SDK at runtime.
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:8000/v1", api_key="YOUR_PROXY_KEY")

def chat(session, subtask, task_type, messages, tools=None):
    return client.chat.completions.create(
        model="agent-auto",
        messages=messages,
        tools=tools,
        extra_headers={
            "x-ts-session": session,
            "x-ts-subtask": subtask,
            "x-ts-task-type": task_type,
        },
    )
```

Create the `agent-auto` profile and its model groups yourself. The example does not create them.

## Tiers and rules

A profile maps `economy`, `standard`, and `advanced` onto existing model groups. Ordered rules match task label, phase, tool/json/vision requirements, and input-size bands. The first match wins. An unknown label uses the profile default. An explicit tier hint is allowed only when that profile defines the tier. A continuation of a matched tool call reuses the pinned tier.

Project policy must allow the public profile name. The profile's three groups are the operator's delegation for that alias. The alias does not allow any other group, and provider, privacy, and paid policies still apply to the chosen target.

`max_candidates` rejects a profile that expands past the configured cap (hard maximum 256) instead of truncating it.

## Affinity

Affinity state is process-local. A restart drops every pin. More than one replica needs session stickiness; SQLite on a network filesystem is not a shared session store.

| Mode | Behaviour |
| --- | --- |
| `off` | No pin. |
| `prefer` | Reuse the pin when it is still eligible inside the same free/paid partition. A paid pin does not jump ahead of an eligible free candidate. |
| `required` | Do not switch. A missing or incomplete tool continuation returns `409 session_state_unavailable`. A temporarily unhealthy target returns `503 affinity_target_unavailable`. |

Idle expiry defaults to 600 seconds and is refreshed only after a successful turn. Absolute lifetime defaults to 3600 seconds. One request may be in flight for an exact session and subtask (`409 subtask_busy`). Distinct subtasks run concurrently. At the configured cap, new affinity scopes return `429 affinity_capacity_exceeded` with `Retry-After`; requests without a session still route.

Pinning does not preserve Gemini thought signatures and does not guarantee a prompt-cache hit.

## Continuation portability

| Providers | Tool continuation |
| --- | --- |
| OpenAI-shaped adapters (Groq, OpenRouter, Cloudflare, Cerebras, NVIDIA, Cohere, Mistral, GitHub Models, Hugging Face, Z.ai, SiliconFlow, DeepSeek, xAI, and the local OpenAI-compatible adapters) | Replayable through `messages`, `tool_calls`, and `tool_call_id`. |
| Google Gemini | Not replayable. Required tool continuation returns `400 unsupported_continuation`. Soft affinity will not keep a tool loop on Gemini. |

Ambiguous tool history (partial, duplicate, or orphan ids) is not repaired. A required affinity request with that history returns `409 ambiguous_continuation`.

## Classification

`routing.agent.mode` is `rules`, `shadow`, or `adaptive`. The classifier is disabled by default and is experimental: this repository ships a rules-versus-default fixture, not a measured quality or cost study. Do not expect a fixed saving multiple.

When enabled, the classifier is one non-streaming call with no tools, no retries, and no re-entry into adaptive routing. It may return only `{"tier":"economy|standard|advanced","confidence":0.0}`. Low confidence, timeout, saturation, or malformed output uses the profile default and records a distinct status. `allowed_project_ids` is the allowlist; empty means no project may classify, including the default project. Local-only and free-only project policy still apply to the classifier target.

Shadow mode may sample a classification but still executes the rules route. Preview never calls the classifier. If execution would need one, preview sets `classification_required` and shows the configured fallback. `simulated_tier` is preview-only and is labelled `simulated`.

Paid adaptive calls and paid classification take a single-process reservation before dispatch when a hard budget ceiling applies. The hold covers the request, project, key, organization, environment, and global/provider/group windows that are configured. A failed, timed-out, or cancelled attempt keeps that reservation as a retained charge, and a retry reserves again. Historical spend plus in-flight reservations are what the ceiling sees. Unresolved reservations survive restart. That ledger is not a provider invoice. If reservation state cannot be loaded, paid adaptive routing and paid classification stay blocked; free rules routing still works. A paid call with no configured ceiling is not held. An unknown price is rejected while a ceiling applies.

## Operator surfaces

`GET /admin/route-plan` stays the existing explanation. `POST /admin/route-plan/preview` accepts representative messages in the body, takes no lease, and writes no request row. The Routing page can explain a plan and preview a subtask. Decision fields are phase, tier, source, pin outcome, and classifier status. Task labels are escaped.

Traces record an `agent_decision` event with digests, not raw session ids. Metrics use bounded labels only.

## Rollback

Set `enabled = false` or `mode = "rules"` and reload. Accounting columns are additive and stay readable. Disabling the feature does not delete usage rows.
