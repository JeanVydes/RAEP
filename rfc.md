# RFC: Remote Agent Execution Protocol (RAEP)

**Status:** Draft\
**Document version:** 0.1.0\
**Wire version:** `1.0`\
**Date:** October 2026\
**Schema dialect:** `https://json-schema.org/draft/2020-12/schema`

---

## 1. Abstract and Scope

The **Remote Agent Execution Protocol (RAEP)** is a Layer 7, event-driven protocol. It runs over WebSockets (`wss://`) and, as an alternative binding, over HTTP Server-Sent Events (Section 4.5). It defines the interface between a stateless client (UI or CLI) and a **Remote Agent Execution Service** (the *server*).

The server owns session state, context and history storage, authentication to and consumption of LLM providers, and isolated execution of tools and background jobs.

**In scope:** message formats, ordering and delivery semantics, lifecycles, errors, security requirements.\
**Out of scope:** the provider API, tool implementations, sandbox technology, agent prompt design, UI behaviour.

### 1.1 Vendor Neutrality

RAEP is agnostic to the LLM provider, the model, the tool vendor and the hosting stack. These rules keep it that way:

1. **No vendor names on the wire.** No field, event name, enum value or error code names a vendor or product. Provider and model are opaque strings chosen by the implementer (Section 7).
2. **Provider-specific parameters are quarantined.** They live under `provider.options` or `ext`; receivers ignore what they do not know, and nothing in the core depends on them.
3. **Normalised concepts replace native ones.** `sampling`, `reasoning` (`effort`, `budget_tokens`, `visibility`), `usage` and `stop_reason` are defined by RAEP. The server maps them to and from each provider and drops what a provider cannot express. Provider-specific usage dimensions go in `usage.extra`.
4. **Provider internals stay server-side.** Native payloads, error bodies and credentials are never exposed to clients (Sections 23 and 24). Errors are normalised to `PROVIDER_ERROR` and `PROVIDER_RATE_LIMITED`.
5. **Tools, skills and MCP servers are described by interface** (name, schema, scopes), not by product; any implementation may back them.
6. **Transports are bindings of one envelope.** WebSocket (Section 4) and SSE (Section 4.5) are defined here. Another binding, such as HTTP/2 streams or gRPC, MAY be defined as a profile if it carries the envelope unchanged and keeps `seq`, `ACK` and resume semantics.
7. **Examples use placeholders** such as `example-provider`, and conformance (Section 30) is independent of any provider.
8. **Extensions are namespaced.** Extension event types use `X_<NS>_`, where NS is a namespace the implementer chooses (Section 26.2).

## 2. Terminology

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as in RFC 2119 and RFC 8174.

| Term | Meaning |
| --- | --- |
| Client | Stateless UI/CLI that renders events and sends commands. |
| Server | The Remote Agent Execution Service. |
| Session | Persistent server-side conversation bound to one `agent_id`. |
| Turn | One unit of agent work, from an input (or system trigger) to a terminal state. |
| Tool call | One tool invocation made by the model within a turn. |
| Job | A detached background process started by a tool; it outlives its turn. |
| Delta | An incremental fragment of streamed output. |
| Sandbox | The isolated environment (process, container or microVM) in which tools and jobs run. |
| Agent | The configuration (provider, model, settings, tool set) that drives a session (Section 7). |
| Child session | A session spawned by another session, its delegator (Section 8.2). |
| Delegator | The parent session, and the agent running in it, that spawned a child session. |
| Root session | A session with no parent. |
| Capability | A feature a server advertises in `HELLO` (Section 6.1), or a feature a client declares in `SESSION_ATTACH`. |
| Part | One element of a multimodal input or output (Sections 12.4 and 12.5). |
| Notification | A content-free webhook nudge sent to a registered endpoint (Section 9). |
| Bundle | An archive that carries a session tree between servers (Section 11). |
| Run | One turn started over HTTP without an event channel (Section 20.1). |
| Snapshot | A copy of a session's sandbox workspace taken at the end of a turn (Section 18.4). |
| Fork | A new root session created from a point in another session (Section 18.1). |
| Tenant | The owner of a set of sessions, quotas and credentials; the unit of isolation (Section 25.3). |
| Approver | A user or group allowed to decide a tool approval (Section 13.6). |

## 3. Core Constraints and Guarantees

1. **Stateless client.** The client MUST NOT persist authoritative session state, call LLM providers, or hold provider API keys.
2. **Event-driven.** Every interaction is an asynchronous JSON frame inside the standard envelope (Section 5).
3. **High-resolution streaming.** Model and tool output is streamed as deltas (`TOKEN_DELTA`, `TOOL_OUTPUT_DELTA`, `JOB_PROGRESS_DELTA`).
4. **Asynchronous jobs.** Long-running tools MAY be detached as jobs, freeing the turn. On completion the result is injected into the agent history as a system event.
5. **Control signalling.** Active turns can be cancelled (`CANCEL_TURN`) and jobs terminated (`CANCEL_JOB`).
6. **Ordered, resumable delivery.** Server events carry a per-session sequence number so a client can detect gaps and resume after a disconnect without loss (Section 16).
7. **Human control.** The server MAY require explicit client approval before executing sensitive tools (Section 13.3).

## 4. Transport

### 4.1 Connection

- The WebSocket endpoint is `/raep/v1/ws`. It MUST be served over `wss://` (TLS 1.2 minimum, TLS 1.3 RECOMMENDED); plain `ws://` MUST NOT be offered outside loopback development.
- The client MUST request the WebSocket subprotocol `raep.v1`. The server MUST reject the upgrade if it does not support it.
- **Authentication:** non-browser clients send `Authorization: Bearer <token>`. Browser clients, which cannot set headers on WebSocket upgrades, send a single-use, short-lived (60 s maximum) ticket as a second subprotocol value: `Sec-WebSocket-Protocol: raep.v1, raep.auth.<ticket>`.
- The server MUST validate the `Origin` header against an allow-list for browser clients.

### 4.2 Framing

- One envelope per WebSocket **text** frame, UTF-8 JSON. Binary frames MUST be rejected with close code `4400`.
- Default maximum frame size is 1 MiB; the server advertises the actual value in `HELLO`. Larger frames MUST be rejected with `1009`.
- Servers MUST split large deltas rather than exceed the limit, and MUST NOT split a UTF-8 code point across deltas.

### 4.3 Keepalive

The server SHOULD send a WebSocket ping every 20 s and close the connection if no pong arrives within 60 s. Clients MUST answer pings and MAY send their own.

### 4.4 Close codes

| Code | Meaning | Client action |
| --- | --- | --- |
| 1000 | Normal closure | None |
| 1001 | Server going away | Reconnect with backoff, resume |
| 1008 | Policy violation | Do not retry blindly |
| 1009 | Frame too large | Fix the sender |
| 1013 | Server overloaded or draining | Reconnect with backoff, honouring `retry_after_ms` |
| 4001 | Unauthenticated or token expired | Re-authenticate |
| 4003 | Forbidden | Do not retry |
| 4008 | Slow consumer (replay buffer exceeded) | Reconnect and resume |
| 4009 | Controller superseded by another client | Re-attach as observer or take over explicitly |
| 4400 | Protocol error (malformed frame, binary frame) | Fix the sender |
| 4426 | Unsupported wire version | Upgrade client |

Reconnection SHOULD use exponential backoff with jitter (base 500 ms, cap 30 s).

### 4.5 SSE Transport (Server-Sent Events)

RAEP MAY also run over HTTP Server-Sent Events plus HTTP POST, for networks that block WebSockets. The envelope, event types, `seq` ordering and every lifecycle in this document are identical to the WebSocket binding; only the carrier differs. A server that supports it advertises `transport.sse` in `HELLO.capabilities` (WebSocket: `transport.ws`).

| Purpose | Request | Notes |
| --- | --- | --- |
| Open stream and attach | `GET /raep/v1/sessions/{session_id}/events` with `Accept: text/event-stream` | Query parameters: `agent_id`, `mode`, `create_if_missing`, `takeover`, or `attach`, the base64url encoding of a complete `SESSION_ATTACH` payload, which also carries `agent_override`, `capabilities`, `client_info` and `options` and wins over the individual parameters. Opening the stream acts as `SESSION_ATTACH`; the server then sends `HELLO` and `SESSION_ATTACHED`. |
| Send a command | `POST /raep/v1/sessions/{session_id}/commands` with `Content-Type: application/json` | Body is one envelope. The server replies `202 Accepted`; the result arrives on the stream. |
| Upload media | `POST /raep/v1/sessions/{session_id}/uploads` | See Section 12.4. |

**Framing.** Each server envelope is one SSE event:

```
id: 57
event: JOB_FINISHED
data: {"version":"1.0","id":"01JB0000000000000000000040","seq":57,"type":"JOB_FINISHED",...}
```

- `id` is the envelope `seq`, `event` is the envelope `type`, and `data` is the whole envelope on one line.
- The server sends an initial `retry: 2000` and a comment line `: keepalive` every 15 s.
- The server sets `Cache-Control: no-cache` and `X-Accel-Buffering: no`. HTTP/2 is RECOMMENDED; with HTTP/1.1, browsers allow only 6 connections per origin, so use one stream per session.

**Resume.** On reconnect the client sends `Last-Event-ID: <seq>`, which is equivalent to `resume_from_seq`. Replay and truncation follow Section 16. Browser `EventSource` sends this header automatically.

**Authentication.** The bearer token goes in `Authorization`. `EventSource` cannot set headers, so browser clients SHOULD use a fetch-based SSE reader. Otherwise they MAY pass a single-use ticket (60 s) in the `ticket` query parameter; tickets MUST NOT be reusable and MUST be stripped from logs.

**Commands.** The server processes commands in arrival order, so clients SHOULD await each `202` before sending the next command in the same session. `Idempotency-Key` MAY repeat the envelope `id`; the deduplication rules of Section 16 apply. `ACK` is an ordinary command.

**Errors and close.** A rejected command returns the matching HTTP status with an `ERROR` envelope as the body. A stream-level fault emits a final `ERROR` event with `details.close_code`, then ends the response.

| HTTP | Meaning | WebSocket equivalent |
| --- | --- | --- |
| 400 | Invalid frame | 4400 |
| 401 | Unauthenticated | 4001 |
| 403 | Forbidden | 4003 |
| 409 | Session busy or controller superseded | 4009 |
| 413 | Payload too large | 1009 |
| 426 | Unsupported wire version | 4426 |
| 429 | Rate limited | 1008 |
| 503 | Unavailable | 1001 |

### 4.6 Authentication Refresh

Tokens can expire while a connection stays open. Before expiry the server sends `AUTH_EXPIRING` (`expires_at`, `refresh_before`). The client answers with `AUTH_REFRESH` (`token`), and the server replies `AUTH_REFRESHED` (`expires_at`) or `ERROR UNAUTHENTICATED` followed by close code `4001`.

- Without a refresh, the server closes with `4001` at expiry. The client then reconnects and resumes (Section 16).
- The server recomputes effective scopes from the new token. A scope the new token no longer grants stops applying at the next tool call; a running turn is not aborted for that reason alone.
- Over SSE, the client sends `AUTH_REFRESH` through the commands endpoint, or simply sends a new `Authorization` header on any POST, which the server treats as a refresh.
- Servers MUST redact `AUTH_REFRESH.token` from logs and from replay buffers.

### 4.7 HTTP Resources

Besides the event channel, RAEP uses HTTP resources. All of them live under the wire major `/raep/v1/`, except the admin API (Section 28.3).

| Resource | Method and path | Section |
| --- | --- | --- |
| WebSocket endpoint | `GET /raep/v1/ws` (upgrade) | 4.1 |
| Event stream (SSE) | `GET /raep/v1/sessions/{session_id}/events` | 4.5 |
| Commands (SSE) | `POST /raep/v1/sessions/{session_id}/commands` | 4.5 |
| Sessions | `POST` and `GET /raep/v1/sessions`; `GET`, `PATCH` and `DELETE /raep/v1/sessions/{session_id}` | 19.1 |
| Shares | `GET /raep/v1/sessions/{session_id}/shares`; `PUT` and `DELETE /raep/v1/sessions/{session_id}/shares/{principal}` | 21.1 |
| Agents | `GET /raep/v1/agents`; `GET /raep/v1/agents/{agent_id}`; `GET /raep/v1/agents/{agent_id}/versions` | 19.2 |
| Upload | `POST /raep/v1/sessions/{session_id}/uploads`; `PUT` and `HEAD` on the returned `upload_url` | 12.4 |
| Output | `GET /raep/v1/sessions/{session_id}/outputs/{output_id}` | 12.5 |
| Snapshots | `GET /raep/v1/sessions/{session_id}/snapshots`; `GET /raep/v1/sessions/{session_id}/snapshots/{turn_id}` | 18.4 |
| Export download | `GET /raep/v1/exports/{export_id}` | 11.1 |
| Import | `POST /raep/v1/imports`; `POST /raep/v1/imports/{import_id}/commit`; `DELETE /raep/v1/imports/{import_id}` | 11.2 |
| Runs | `POST /raep/v1/runs`; `GET /raep/v1/runs/{run_id}`; `POST /raep/v1/runs/{run_id}/cancel` | 20.1 |
| Schedules | `POST` and `GET /raep/v1/schedules`; `GET`, `PATCH` and `DELETE /raep/v1/schedules/{schedule_id}` | 20.2 |
| Triggers | `POST` and `GET /raep/v1/triggers`; `DELETE /raep/v1/triggers/{trigger_id}`; `POST /raep/v1/triggers/{trigger_id}/fire` | 20.3 |
| Approvals | `GET /raep/v1/approvals`; `POST /raep/v1/approvals/{approval_id}` | 13.6 |
| Quota | `GET /raep/v1/quota` | 25.4 |
| Discovery and health | `GET /raep/v1/info`; `GET /raep/v1/health/live`; `GET /raep/v1/health/ready` | 28.2 |
| Admin | `/raep/admin/v1/...` | 28.3 |

- Every resource uses the authentication of Section 4.1 and the session authorization of Section 24, except the discovery and health resources, which need none. A resource that belongs to a session is available only to clients authorised on that session.
- Browser requests are checked against the `Origin` allow-list. Servers that allow cross-origin use answer preflight requests and allow the headers `Authorization`, `Content-Type`, `Idempotency-Key`, `Last-Event-ID` and `Content-Range`.
- A failed request returns the HTTP status of Section 4.5 for transport faults, and otherwise `404` (`NOT_FOUND`), `409` (`IDEMPOTENCY_CONFLICT`, `REWIND_BLOCKED`, `SESSION_BUSY`), `422` (`INVALID_FRAME`), `429` (`RATE_LIMITED`, `QUOTA_EXCEEDED`) or `503` (`UNAVAILABLE`), with `Retry-After` where it applies. The body is an `ERROR` envelope as JSON.
- `POST` requests that create something accept an `Idempotency-Key` header (Section 16).

## 5. Protocol Envelope

Every frame uses this envelope.

| Field | Type | Sender | Description |
| --- | --- | --- | --- |
| `version` | string | both | Wire version. Constant `1.0`. |
| `id` | string (1-128) | both | Unique per sender. ULID RECOMMENDED. Used for deduplication. |
| `seq` | integer >= 1 | server only | Monotonic per session across all server events. Absent on client frames and on connection-level frames (Section 6). |
| `ref` | string or null | both | `id` of the frame this one answers (for example an `ERROR`). |
| `session_id` | string | both | Session identifier. MUST be unguessable (at least 128 bits of entropy). |
| `turn_id` | string or null | both | Turn scope, if any. |
| `job_id` | string or null | both | Job scope, if any. |
| `timestamp` | integer | both | Unix epoch milliseconds, sender clock. Informational only; ordering uses `seq`. |
| `type` | string | both | Event type from the catalog. |
| `payload` | object | both | Type-specific body. |
| `ext` | object | both | OPTIONAL extension data (Section 26.2). |

**Conventions.** Field names are `snake_case`. `timestamp` and fields ending in `_at` are Unix epoch milliseconds; fields ending in `_ms` are milliseconds, in `_s` seconds, and `bytes` and fields ending in `_bytes` are bytes. Identifiers are opaque strings. For an optional field, an absent field and `null` mean the same unless a definition says otherwise. Strings are UTF-8, and a length limit stated in characters counts Unicode code points.

Unknown `type` values MUST be answered with `ERROR` code `UNKNOWN_TYPE` (client frames) or ignored (server frames). Unknown payload fields MUST be ignored.

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://raep.vyde.dev/v1/envelope.json","type":"object","properties":{"version":{"const":"1.0"},"id":{"type":"string","minLength":1,"maxLength":128},"seq":{"type":"integer","minimum":1},"ref":{"type":["string","null"]},"session_id":{"type":"string"},"turn_id":{"type":["string","null"]},"job_id":{"type":["string","null"]},"timestamp":{"type":"integer"},"type":{"type":"string"},"payload":{"type":"object"},"ext":{"type":"object"}},"required":["version","id","session_id","timestamp","type","payload"],"additionalProperties":false}
```

Each `type` has a payload schema under `https://raep.vyde.dev/v1/payloads/<TYPE>.json`. The frame schema `https://raep.vyde.dev/v1/frame.json` validates the envelope and selects the payload schema by `type` (Appendix B). Section 27 lists the payload fields of every type.

## 6. Handshake and Capabilities

1. After the transport opens, the server sends **`HELLO`** with its capabilities and limits.
2. The client sends **`SESSION_ATTACH`**.
3. The server replies **`SESSION_ATTACHED`**, replays missed events if the client asked to resume (Section 16), then sends **`SESSION_STATE`**.

**`HELLO`.** Payload: `server_version` (the server implementation's version, opaque to clients), `wire_versions` (supported list), `capabilities` (Section 6.1), `limits` (Section 6.2), `heartbeat_s`, `draining` (true while the server drains, Section 28.4) and `deprecations` (Section 26.3).

**`SESSION_ATTACH`.** Payload: `agent_id` (required), `agent_override` (Section 7), `create_if_missing` (default true), `mode` (`controller` or `observer`, default `controller`), `takeover` (boolean, Section 10), `resume_from_seq` (the last `seq` the client processed, optional), `capabilities` (what the client supports, below), `client_info` (`name`, `version`) and `options` (session options such as `job_turn_policy`, Section 14.4, and `notify`, Section 9).

For an existing session, `agent_id` MUST equal the session's current agent; otherwise the server answers `ERROR INVALID_FRAME` with `details.reason: agent_mismatch`. With `create_if_missing: false` and no such session the server answers `ERROR SESSION_NOT_FOUND`.

**Client capabilities.** `SESSION_ATTACH.capabilities` is an object with optional members: `render`, the output modalities the client can display (`text`, `image`, `document`, `audio`, `video`; default `["text"]`), and `features`, the optional features the client implements (`approvals`, `output.parts`, `presence`, `input.stream`; default none). A third member, `client_tools`, lists the tools the client executes itself (Section 13.5). The server never sends output parts to a client that did not declare `output.parts`, and never asks for approval from a client that did not declare `approvals` (Section 13.3).

**`SESSION_ATTACHED`.** Payload: `created` (boolean), `mode`, `last_seq`, `replay` (`{from_seq, to_seq, truncated, lost}`, Section 17.3), `negotiated_capabilities` (the capabilities of `HELLO` that apply to this client, that is, those that do not depend on a client feature it lacks) and `agent` (the non-secret summary of the effective agent, Section 7).

If a client feature that server policy makes mandatory is missing (for example approvals are mandatory and the client does not declare `approvals`), the server fails the attach with `ERROR CAPABILITY_REQUIRED`.

```json
{"version":"1.0","id":"01JB0000000000000000000001","ref":null,"session_id":"","turn_id":null,"job_id":null,"timestamp":1793404800100,"type":"HELLO","payload":{"server_version":"example-server 1.0.0","wire_versions":["1.0"],"capabilities":["transport.ws","jobs","approvals","resume","history","uploads","delegation","notify"],"limits":{"max_frame_bytes":1048576,"max_prompt_bytes":262144,"max_concurrent_jobs":8,"replay_buffer_events":10000,"job_output_retention_s":86400},"heartbeat_s":20}}
```

**Connection-level frames and session id allocation.** `HELLO` and connection-level `ERROR` frames are sent before a session is bound: they carry an empty `session_id` and no `seq`. The first sequenced event is `SESSION_ATTACHED`. In `SESSION_ATTACH` the client sets `session_id` to an existing session's id, or to an empty string to have the server allocate one; the allocated id comes back in the `session_id` of `SESSION_ATTACHED`. A server MAY accept a client-proposed id for a new session only if it has at least 128 bits of entropy, and otherwise rejects it with `ERROR INVALID_FRAME`. Over SSE the path segment `new` requests allocation. One connection MAY attach to several sessions (Section 8.2.11); every frame is routed by its `session_id`.

### 6.1 Capability Registry

A server advertises exactly the capabilities it implements in `HELLO.capabilities`.

| Capability | Meaning | Section |
| --- | --- | --- |
| `transport.ws` | WebSocket binding | 4.1 |
| `transport.sse` | SSE binding | 4.5 |
| `jobs` | Background jobs | 14 |
| `approvals` | Human approval of tool calls | 13.3 |
| `resume` | Ordered replay and resume | 16 |
| `history` | `HISTORY_GET` | 16 |
| `uploads` | Uploads of media | 12.4 |
| `uploads.resumable` | Resumable uploads | 12.4 |
| `reconfigure` | `SESSION_CONFIGURE` and handoff | 8.1, 8.3 |
| `delegation` | Child sessions and agent tools | 8.2 |
| `delegation.structured_result` | `output_schema` for child results | 8.2.6 |
| `notify` | Out-of-band notifications | 9 |
| `output.parts` | Non-text output parts | 12.5 |
| `export`, `import` | Session export and import | 11 |
| `client_tools` | Tools executed by the client | 13.5 |
| `structured_output` | `response_format` on turns | 12.6 |
| `approvals.policy` | Group, quorum and policy-based approval | 13.6 |
| `compaction` | `SESSION_COMPACT` and `CONTEXT_PIN` | 18.3, 18.5 |
| `fork`, `rewind` | `SESSION_FORK`, `SESSION_REWIND` | 18.1, 18.2 |
| `workspace.snapshots` | Per-turn workspace snapshots | 18.4 |
| `collaboration` | Shared sessions, several controllers, presence | 21 |
| `input.stream` | Streamed audio and video input | 12.7 |
| `manage` | Session and agent management API | 19 |
| `runs` | One-shot runs | 20.1 |
| `schedules` | Schedules and triggers | 20.2, 20.3 |
| `federation` | Remote delegates | 22.2 |
| `quota` | Quota resource | 25.4 |
| `admin` | Admin API | 28.3 |

Extension capabilities use the prefix `X_<NS>_` (Section 26.2).

### 6.2 Limits Registry

`HELLO.limits` advertises the server's limits. A server that omits a limit applies the default below; where there is no default, no limit is assumed.

| Limit | Default | Meaning |
| --- | --- | --- |
| `max_frame_bytes` | 1048576 | Largest frame (Section 4.2). |
| `max_prompt_bytes` | 262144 | Largest `prompt` or total text of `content`. |
| `max_parts` | 16 | Parts in one `content` array. |
| `max_inline_bytes` | 262144 | Raw bytes of one `inline` source. |
| `max_upload_bytes` | 52428800 | Largest upload. |
| `max_concurrent_jobs` | 8 | Running jobs per session. |
| `max_queued_inputs` | 8 | Inputs waiting in the queue (Section 12.1). |
| `replay_buffer_events` | 10000 | Events kept for replay (Section 16). |
| `job_output_retention_s` | 86400 | Retention of job output. |
| `session_retention_s` | none | How long a closed or idle session is kept; server policy. |
| `max_commands_per_s` | 30 | Commands per second per connection (Section 25.5). |
| `max_client_tool_result_bytes` | 1048576 | Largest client tool result (Section 13.5). |
| `max_tool_result_bytes` | 262144 | Largest tool result passed to the model (Section 13.4). |
| `max_input_stream_bytes` | 33554432 | Largest realtime input stream (Section 12.7). |
| `max_workspace_bytes` | 1073741824 | Largest workspace snapshot (Section 18.4). |
| `max_snapshots` | 50 | Snapshots kept per session (Section 18.4). |
| `max_pinned_turns` | 20 | Pinned turns per session (Section 18.5). |
| `max_participants` | 8 | Controllers and viewers attached at once (Section 21). |
| `run_retention_s` | 604800 | Retention of runs (Section 20.1). |
| `max_inspect_bytes` | 65536 | Largest `inspect_agent` response (Section 8.2.7). |
| `max_result_bytes` | 65536 | Largest structured child output (Section 8.2.6). |
| `child_retention_s` | `job_output_retention_s` | Retention of a closed child (Section 8.2.12). |
| `max_notify_targets` | 4 | Notification targets per session (Section 9). |
| `max_output_part_bytes` | 10485760 | Largest output part (Section 12.5). |
| `max_export_bytes` | 536870912 | Largest export bundle (Section 11.1). |
| `export_ttl_s` | 3600 | Lifetime of an export download. |

## 7. Agent Definition

An agent is whatever the server says it is. RAEP identifies it by `agent_id` and treats it as opaque. This section defines an OPTIONAL, RECOMMENDED descriptor, `AgentSpec`, that servers MAY adopt fully, adopt in part, or replace with their own. Every field is optional, unknown fields are ignored, and provider-specific data goes under `provider.options` or `ext`. A minimal valid agent is just an id. Clients MUST NOT assume a field exists unless the server returned it.

The model behind the descriptor: **agent = provider + model + configuration + tool set.**

| Group | Fields | Description |
| --- | --- | --- |
| Identity | `id`, `version`, `name`, `description` | `version` is pinned per session; changing an agent does not alter a running session unless the session follows `latest`. |
| Provider | `provider.id`, `provider.options`, `provider.retry`, `provider.fallbacks[]` | Any LLM provider: hosted, self-hosted or gateway. Credentials and endpoints are referenced server-side (`credentials_ref`) and are NEVER sent to clients. Retries and fallbacks: Section 7.1. |
| Model | `model.id`, `model.revision` | Model name at the provider. |
| Sampling | `sampling.temperature`, `top_p`, `top_k`, `max_output_tokens`, `stop_sequences`, `seed` | Provider-neutral names; the server maps them or drops unsupported ones. |
| Reasoning | `reasoning.mode` (`off`, `auto`, `on`), `effort` (`minimal`, `low`, `medium`, `high`, `max`), `budget_tokens`, `visibility` (`hidden`, `summary`, `full`) | `visibility` decides whether `TOKEN_DELTA` with `channel: reasoning` is emitted. |
| Budgets | `max_turn_tokens`, `max_session_tokens`, `max_turn_cost`, `max_session_cost` (with `currency`), `max_tool_calls_per_turn`, `max_turn_duration_s`, `max_concurrent_jobs`, `max_job_runtime_s`, `on_exceed` (`stop`, `ask`, `degrade`) | Enforced as described below. |
| Instructions | `system_prompt` (inline or `ref`) | Never sent to clients unless server policy allows. |
| Skills | `skills[]`: `id`, `version`, `activation` (`auto`, `pinned`, `on_request`) | Reusable instruction and tool bundles. |
| MCP | `mcp_servers[]`: `name`, `transport` (`streamable_http`, `sse`, `stdio`), `endpoint_ref`, `auth_ref`, `tool_filter` (`allow`, `deny`), `required` | Server-side connections; their tools join the tool set after filtering. |
| Tool set | `tools.builtin[]`, `tools.allow[]`, `tools.deny[]`, `tools.approval` (per tool: `never`, `risk_based` or `always`), `tools.detach_allowed`, `tools.policy`, `tools.client` | Final set = built-in plus MCP tools, minus `deny`, limited to `allow` when present. Reliability policy: Section 13.4. Client tools: Section 13.5. |
| Scopes | `scopes[]` | Permission strings such as `fs:read`, `fs:write`, `net:egress`, `shell:exec`, `jobs:spawn`, `mcp:<name>:<action>`, `delegate:spawn`, `delegate:inspect`, `delegate:message`, `delegate:stop` (or `delegate:*`), `session:export`, `session:import`. |
| Delegation | `delegation.delegates[]`, `max_child_depth`, `max_children`, `max_concurrent_children`, `max_descendants`, `max_spawns_per_turn`, `default_budget_share`, `default_lifetime`, `child_idle_ttl_s`, `inspect`, `child_access`, `approval_routing`, `approval_parent_timeout_s`, `child_turn_policy`, `on_parent_cancel`, `workspace`, `max_wait_s` | Sub-agents as child sessions. Fields, defaults and rules are in Section 8.2.1. |
| Handoff | `handoff_targets[]` | Agents this agent may hand the conversation to (Section 8.3). |
| Approval | `approval.routing`, `approval.policy_ref`, `approval.separation_of_duties` | Who decides tool approvals (Section 13.6). |
| Context | `context.sources[]`, `context.compaction` | Memory, retrieval and compaction (Section 7.2). |
| Workspace | `workspace.snapshots` (`off` or `per_turn`) | Sandbox snapshots (Section 18.4). |
| Capabilities | `capabilities.input[]`, `output[]`, `features[]`, `limits` | Modalities: `text`, `image`, `document`, `video`, `audio`. Features: `streaming`, `tool_use`, `reasoning`, `structured_output`, `prompt_caching`. Limits per modality: `mime_types`, `max_bytes`, `max_items`, `max_duration_ms`. |
| Defaults | `defaults.job_turn_policy`, `defaults.queue_policy` | Used when the client does not set them. |
| Overrides | `overridable[]` | Field paths a session or turn may change, for example `sampling.temperature`, `reasoning.effort`. |

**Effective configuration.** Layers merge in this order, later wins: the agent spec, the session-level `agent_override` (in `SESSION_ATTACH` or `SESSION_CONFIGURE`), and the per-turn `overrides` (in `USER_INPUT`). An override may change only fields listed in `overridable`. Scopes, tool sets and budgets can only be narrowed, never widened. Effective scopes are the intersection of the agent's scopes, the token's scopes and any override. A violation returns `ERROR AGENT_OVERRIDE_DENIED`.

**What the client sees.** `SESSION_ATTACHED.agent` carries a non-secret summary of the effective agent: `id`, `version`, `provider.id`, `model.id`, `sampling`, `reasoning`, `budgets`, tool names, `scopes` and `capabilities`. Credentials, endpoints, `auth_ref` and `system_prompt` are never included.

**Capability negotiation.** The server announces the modalities the agent accepts (`capabilities.input`) and may produce (`capabilities.output`). The client declares in `SESSION_ATTACH.capabilities.render` which output modalities it can display. A server MAY accept a modality by conversion, for example audio to transcript or video to sampled frames, and then lists it as `{modality, via}` in `capabilities.input`.

**Budget enforcement.** The server checks budgets before every model call and tool call. On exceed, `stop` ends the turn as `FAILED` with `stop_reason: budget_exceeded` and `ERROR BUDGET_EXCEEDED`. `ask` sends a `TOOL_APPROVAL_REQUEST` with `tool_name: budget` so the controller can extend the limit. `degrade` lowers reasoning effort or disables optional tools and continues. When a session-level budget is exhausted, new `USER_INPUT` is rejected with `BUDGET_EXCEEDED`. `budget_exceeded` is a `stop_reason` value (Section 12.3).

**Scope enforcement.** A tool call that needs a scope the agent lacks is not executed: `TOOL_CALL_FINISHED` carries `status: DENIED` and the model receives a tool error. MCP tools are governed by scopes of the form `mcp:<name>:<action>`.

```json
{"id":"example-agent","version":"3","name":"Example Agent","provider":{"id":"example-provider"},"model":{"id":"example-model-large"},"sampling":{"temperature":0.2,"max_output_tokens":8192},"reasoning":{"mode":"on","effort":"medium","budget_tokens":8000,"visibility":"summary"},"budgets":{"max_turn_tokens":200000,"max_session_cost":25,"currency":"USD","max_tool_calls_per_turn":40,"on_exceed":"stop"},"skills":[{"id":"repo-review","activation":"auto"}],"mcp_servers":[{"name":"github","transport":"streamable_http","tool_filter":{"allow":["issues.*","pulls.*"]}}],"tools":{"builtin":["shell_exec","file_read","file_write"],"approval":{"shell_exec":"risk_based","file_write":"never"},"detach_allowed":true},"scopes":["fs:read","fs:write","shell:exec","jobs:spawn","mcp:github:read","delegate:*"],"capabilities":{"input":["text","image","document"],"output":["text"],"features":["streaming","tool_use","reasoning"]},"delegation":{"delegates":["example-reviewer"],"max_child_depth":2,"max_concurrent_children":4,"inspect":"events","child_access":"observer","approval_routing":"human"},"overridable":["sampling.temperature","reasoning.effort"]}
```

### 7.1 Provider Routing and Retries

`provider.retry` controls retries of a failed model call: `max_attempts` (default 3, counting the first), `backoff_ms` (default 500), `max_backoff_ms` (default 30000), `jitter` (default true) and `retry_on` (default `PROVIDER_ERROR`, `PROVIDER_RATE_LIMITED`).

`provider.fallbacks[]` is an ordered list of alternative targets, each `{provider, model, when, max_attempts}`. `provider` and `model` have the shape of Section 7; `when` lists the error codes that trigger the fallback (default as `retry_on`). Routing is ordered: the primary target is tried first, then each fallback in turn, each for at most its `max_attempts`.

- **Capabilities.** A fallback is used only if it supports everything the turn needs (the input modalities, tool use, and reasoning when `reasoning.mode` is `on`); otherwise it is skipped.
- **Mapping.** Sampling and reasoning settings are mapped to the fallback as usual. Opaque provider reasoning material from another provider is dropped.
- **Streaming.** When a call fails after output has started, the server emits `PROVIDER_RETRY` before the next attempt. Its `discard_from_index` is the first `TOKEN_DELTA` `index` of the failed attempt; clients MUST drop every delta of the turn with an `index` at or above it, and the next attempt restarts from that index. Reasoning deltas follow the same rule.
- **Accounting.** Every attempt counts against the budgets. `usage.extra.attempts` lists each attempt's provider, model and usage.
- **Visibility.** `PROVIDER_RETRY` carries `attempt`, `reason` (the error code), `provider` (`{id, model}` of the next attempt), `fallback` (true when the target differs from the primary) and `discard_from_index`. The agent summary shows the primary target only.
- **Failure.** When every attempt fails, the turn ends `FAILED` with the last `PROVIDER_ERROR` or `PROVIDER_RATE_LIMITED`.

### 7.2 Context Sources and Compaction

`context.sources[]` declares where the agent gets context beyond the conversation. Each source is `{id, kind, mode, scope, limits}`:

| Field | Values |
| --- | --- |
| `kind` | `memory` (durable notes the agent writes and reads), `retrieval` (a search index), `workspace` (files in the sandbox) |
| `mode` | `auto` (the server injects relevant items every turn), `pinned` (always included), `on_request` (the model asks through tools) |
| `scope` | `user`, `tenant` or `session`: who can see what a `memory` source holds |
| `limits` | `max_items` and `max_tokens` injected per turn |

Sources with `mode: on_request` expose two reserved tools: `context_search` (`source*`, `query*`, `limit`) and `context_write` (`source*`, `content*`, `key`), which needs the scope `context:write` (`context:read` for search). Injected items carry their provenance and are untrusted data (Section 24). The backing stores are server-private, like MCP servers; RAEP defines only how they are declared and exposed. The agent summary lists source ids and kinds, nothing else.

`context.compaction` controls how the server keeps the model context within the model's window: `mode` (`auto`, the default, or `manual`), `threshold` (fraction of the window at which `auto` starts; default 0.8), `strategy` (`summarize`, the default, or `truncate`) and `keep_last_turns` (default 4). Compaction changes only what the model sees. Stored history is unchanged, pinned turns stay verbatim (Section 18.5), and each compaction is reported with `HISTORY_COMPACTED` (Section 8.4). A client can trigger it with `SESSION_COMPACT` (Section 18.3).

## 8. Session Configuration, Delegation and Notices

### 8.1 Runtime reconfiguration

`SESSION_CONFIGURE` (controller only) changes the effective agent configuration of an attached session without re-attaching. Payload: `agent_override` (same shape as in `SESSION_ATTACH`, bounded by `overridable` and the narrowing rules of Section 7), `agent_id` (switch to another agent, allowed only if the token's scopes permit it), `options` (for example `job_turn_policy`, `notify`) and `apply` (`now` or `next_turn`, default `now`).

Changes apply at a turn boundary. With a turn active and `apply: now`, the server answers `ERROR SESSION_BUSY`; with `apply: next_turn` it stores the change and applies it when the turn ends. The server replies `SESSION_CONFIGURED` with the new effective agent summary and `applied` (false when the change was stored for `next_turn`), and emits `AGENT_CHANGED` if the agent or its model changed.

### 8.2 Delegation (child sessions)

A sub-agent is a **child session** of the session that spawned it, its *delegator*. A child is a full session: it has its own `session_id`, `seq`, history, turns, jobs, approvals and sandbox. The delegator keeps three rights over it: to inspect it, to give it feedback and to stop it. Delegation is advertised as the `delegation` capability; `delegation.structured_result` additionally signals support for `output_schema` (Section 8.2.6).

Terms: the **parent** is the session that spawned a child; the **root** is a session with no parent; **depth** is the number of ancestors (root = 0, its children = 1). Everything in this section applies to every level of the tree.

#### 8.2.1 Configuration

Delegation is configured by the `delegation` group of `AgentSpec` (Section 7). Every field is optional and the defaults apply when the group is absent, except `delegates`: without it nothing can be spawned.

| Field | Default | Meaning |
| --- | --- | --- |
| `delegates[]` | none | Allow-list of `agent_id` values this agent may spawn, or remote delegate entries (Section 22.2). |
| `max_child_depth` | 2 | Maximum depth of the tree below this session. `0` disables spawning. |
| `max_children` | 16 | Total children this session may spawn over its life. |
| `max_concurrent_children` | 4 | Children of this session not in a terminal state. |
| `max_descendants` | 32 | Non-terminal descendants in the whole tree below the root. |
| `max_spawns_per_turn` | 8 | Spawns allowed in one turn. |
| `default_budget_share` | 0.25 | Used when `spawn_agent` omits `budget_share`. |
| `default_lifetime` | `turn` | Used when `spawn_agent` omits `lifetime`. |
| `child_idle_ttl_s` | 3600 | How long an `IDLE` child lives without a message. |
| `inspect` | `events` | Cap on what the parent can read: `summary`, `events` or `full` (Section 8.2.7). |
| `child_access` | `observer` | What a client authorised on the parent gets on a child: `observer` or `controller` (Section 8.2.11). |
| `approval_routing` | `human` | `human`, `parent`, `group` or `policy` (Sections 8.2.9 and 13.6). |
| `approval_parent_timeout_s` | 30 | How long a parent agent has to decide before the request escalates to a human. |
| `child_turn_policy` | `always` | Which child events start a parent turn; same values as `job_turn_policy` (Section 14.4). |
| `on_parent_cancel` | `kill` | `kill` or `graceful`: how `lifetime: turn` children stop when the parent turn is cancelled. |
| `workspace` | `isolated` | Default workspace mode (Section 8.2.3). |
| `max_wait_s` | 600 | Upper bound for `wait_agent.timeout_s`. |

A child's effective `delegation` group is its own agent's group narrowed by the parent's: numeric limits take the minimum, `max_child_depth` is the minimum of the child's own value and the parent's value minus one, and `delegates` is the child's own list (it is not constrained by the parent's list, but every spawn is still checked against the child agent's own scopes). Scopes for the agent tools are `delegate:spawn`, `delegate:inspect`, `delegate:message` and `delegate:stop`, or `delegate:*`. A tool call without the matching scope fails with tool error `SCOPE_DENIED`.

#### 8.2.2 Creating a child

A `spawn_agent` tool call creates the child. The server:

1. Checks `agent_id` against `delegates`, the limits of Section 8.2.3 and the scopes. Any failure creates nothing and consumes nothing.
2. Allocates `session_id` (at least 128 bits of entropy, Section 24) and pins the child agent's version.
3. Computes the child's effective configuration: the child agent's spec, narrowed by `scopes`, `inspect` and `budget_share` from the call.
4. Reserves the budget share (Section 8.2.3), copies `context.attachments` into the child's own upload store, and starts the child's first turn from `task`.
5. Writes the child's `SESSION_STATE` (`parent`: `session_id`, `turn_id`, `tool_call_id`, `depth`), adds the child to the parent's `children`, emits `CHILD_SESSION_STARTED` on the parent stream, and returns `child_session_id` to the model.

The server MUST persist the tool-call record before creating the child, so that a retry after a crash finds the same child (Section 8.2.5, idempotency).

#### 8.2.3 Rules

- **Scopes.** A child's scopes are a subset of its parent's. They can only be narrowed. The parent's tokens are not forwarded: a child runs with the scopes the server computes, and a client token that later loses a scope on the parent loses it on every descendant.
- **Budgets.** `budget_share` is either a fraction in (0, 1] of the parent's remaining budget at spawn time, applied per metric, or an object of absolute amounts per metric (`tokens`, `cost`, `tool_calls`) that MUST NOT exceed the parent's remaining amounts. The share is **reserved**: it is deducted from the parent's `budget_remaining` while the child is open, and the unused part is returned when the child reaches a terminal state. A grandchild draws from its own parent's remaining budget, so the total spent by any subtree can never exceed what its root reserved. A metric the parent does not limit is not limited for the child unless the child's own agent limits it. `duration` is not shareable: a child's wall-clock cap is the smaller of its own `max_turn_duration_s` and the parent's remaining turn time. Usage of every descendant counts towards each ancestor's `usage_rollup` (Section 8.2.12). Exhausting a share ends the child according to the child agent's `on_exceed` (normally `stop`: the child turn is `FAILED` with `stop_reason: budget_exceeded`, the child state is `FAILED` and `child_result.status` is `budget_exceeded`); it never ends the parent. An absolute share above the parent's remaining amount, or a parent with nothing remaining, fails the spawn with `BUDGET_EXCEEDED` and `details.metric: budget_share`.
- **Limits.** A spawn that would exceed a limit creates nothing and returns tool error `BUDGET_EXCEEDED` with `details` `{metric, limit, current}`, where `metric` is one of `child_depth`, `children`, `concurrent_children`, `descendants`, `spawns_per_turn` or `budget_share`. Depth is checked against the effective `max_child_depth` of the spawning session, so a grandchild's depth counts against every ancestor's limit. Limits are not wire `ERROR` frames: they surface as `TOOL_CALL_FINISHED` with `status: ERROR` in the spawning session.
- **Lifetime.**
  - `lifetime: turn` (default): when the spawning turn reaches a terminal state, the server stops the child. After `COMPLETED` the stop is `graceful` with a 5 s grace period, then `kill`. After `CANCELLED` or `FAILED` it follows `on_parent_cancel`. The child's result is recorded in the parent history but does not start a new parent turn. A turn that wants a child's result MUST wait for it (`wait_agent`).
  - `lifetime: session`: the child outlives the turn, like a job, until it finishes, is stopped, expires (`ttl_s`) or the parent session closes. `CANCEL_TURN` on the parent does not affect it.
- **Closing.** Closing the parent closes all its descendants first (children before parents), kills their jobs and releases their sandboxes. `SESSION_CLOSE` with `purge: true` purges every descendant. A child whose runtime a server loses MUST be marked `FAILED` with `error.code: INTERNAL`.
- **Idle.** After finishing a task a child stays `IDLE` and accepts further messages until it is closed or `ttl_s` (default `child_idle_ttl_s`) passes without one; it then closes with `CHILD_SESSION_UPDATED` `reason: state_change`.
- **Failure.** A child's failure does not fail the parent; it is reported as state `FAILED`. `FAILED` is terminal: the parent can spawn a new child with a corrected task.
- **Workspace.** `isolated` (default): the child has its own sandbox filesystem containing only its attachments. `shared_ro`: the parent's workspace is mounted read-only. `shared_rw`: the parent's workspace is mounted read-write; it requires `fs:write` on both parent and child, and a server SHOULD refuse two concurrent `shared_rw` children of the same parent unless the agent spec allows it, since concurrent writers are undefined behaviour. Network egress is never wider than the narrowest scope in the chain.
- **Context.** A child sees only `task` by default. `context.mode: summary` adds a server-generated, redacted summary of the parent's conversation, at most 4 KiB, labelled as delegator context. `context.attachments` lists upload ids from the parent session (at most 16). Uploads are session-scoped (Section 12.4), so the server copies each into the child's store under a new id; the copies count against the child's limits.

#### 8.2.4 Child states

| State | Meaning |
| --- | --- |
| `RUNNING` | A turn is starting or active. |
| `AWAITING_APPROVAL` | A tool call of the child waits for a decision (Section 8.2.9). |
| `AWAITING_INPUT` | The child called `ask_delegator` and waits for an answer. |
| `IDLE` | The turn completed; the child accepts messages until its TTL. |
| `FAILED` | Terminal. The turn failed unrecoverably or the budget share ran out. |
| `STOPPED` | Terminal. Stopped by the delegator, a controller client, or the end of the spawning turn. |
| `CLOSED` | Terminal. Resources released; follows `FAILED`, `STOPPED` and TTL expiry. |

Transitions: spawn -> `RUNNING`; `RUNNING` <-> `AWAITING_APPROVAL` / `AWAITING_INPUT`; `RUNNING` -> `IDLE` on `COMPLETED`; `IDLE` -> `RUNNING` on a message; any non-terminal state -> `FAILED` or `STOPPED`; `FAILED`, `STOPPED` and `IDLE` (on expiry) -> `CLOSED`. Each state change is reported once with `CHILD_SESSION_UPDATED`. After a terminal state the child accepts no messages; tools that need a live child return `CHILD_NOT_ACTIVE`.

#### 8.2.5 Agent tools

The parent agent controls its children through eight provider-neutral tools. Servers MAY rename them but MUST keep the semantics when they advertise `delegation`. Arguments and results are specified in Appendix B. Only the session that spawned a child can use these tools on it; any other child id returns `CHILD_NOT_FOUND`, the same as an unknown id, so ids cannot be probed.

| Tool | Arguments | Behaviour |
| --- | --- | --- |
| `spawn_agent` | `agent_id*`, `task*` (text or content parts), `name`, `lifetime`, `budget_share`, `scopes` (narrowing), `inspect`, `output_schema`, `context`, `workspace`, `ttl_s` | Creates the child. Returns `child_session_id`, `state`, `depth`, `budget_reserved`. |
| `inspect_agent` | `child_session_id*`, `view` (`status` default, `transcript`, `events`, `jobs`), `from_seq`, `cursor`, `limit`, `job_id`, `from_index` | Read-only (Section 8.2.7). |
| `message_agent` | `child_session_id*`, `content*`, `delivery` (`steer` default, `queue`, `interrupt`) | Feedback (Section 8.2.8). Returns `accepted`, `delivery`, `child_state`, `held`. |
| `stop_agent` | `child_session_id*`, `mode` (`graceful` default, `kill`), `reason`, `timeout_s` (default 30) | `graceful` lets the current tool call finish, then stops the child; if it has not stopped after `timeout_s` the server escalates to `kill`. `kill` cancels the turn, kills its jobs and closes. Returns when the child is terminal, with `state` and its final `result`. |
| `wait_agent` | `child_session_id*`, `until` (`idle`, `finished`), `timeout_s` | Blocks the tool call (the parent turn stays in `EXECUTING_TOOL`) until the child is `IDLE` (`idle`) or `FAILED`, `STOPPED` or `CLOSED` (`finished`), or until `timeout_s` (capped by `max_wait_s`). Returns `state`, `timed_out` and the `result`. A timeout is a normal return with `timed_out: true`, not an error. If the parent turn is cancelled while waiting, the call ends with status `CANCELLED`. |
| `list_agents` | `state` (list), `cursor`, `limit` | Lists this session's children with a status snapshot. |
| `approve_child_call` | `child_session_id*`, `tool_call_id*`, `decision*` (`approve`, `deny`), `reason` | Answers a routed approval (Section 8.2.9). |
| `ask_delegator` | `question*`, `options`, `timeout_s` | **Called by the child**, not the parent. Moves the child to `AWAITING_INPUT` (Section 8.2.9). |

**Tool errors.** Delegation tools report failures to the model as tool errors with these codes: `AGENT_NOT_FOUND`, `DELEGATE_NOT_ALLOWED` (not in `delegates`), `CHILD_NOT_FOUND` (unknown, not yours, or expired), `CHILD_NOT_ACTIVE` (terminal state), `DELEGATOR_SUSPENDED` (Section 8.2.11), `BUDGET_EXCEEDED` (with `details.metric`), `SCOPE_DENIED`, `OUTPUT_SCHEMA_INVALID`, `APPROVAL_TIMEOUT`, `APPROVAL_NOT_PENDING`, `RISK_TOO_HIGH`, `DELEGATOR_TIMEOUT` and `INVALID_ARGUMENT`. None of them is a wire `ERROR` frame.

**Idempotency.** Model-issued tool calls are not client frames, so the frame-`id` deduplication of Section 16 does not cover them. Delegation tools are deduplicated by the key `(parent session_id, tool_call_id)`, for as long as the tool-call record is kept in the history (not a fixed window):

- A repeated `spawn_agent` returns the original result, with the same `child_session_id`, and creates nothing. Combined with the persist-before-create rule of Section 8.2.2, a crash and retry yields one child.
- A repeated `message_agent` does not deliver the message again; it returns the original outcome.
- `stop_agent` on a child that is already terminal returns its terminal `state` and `result`, like `CANCEL_JOB` on a finished job.
- `approve_child_call` repeated with the same decision returns the original outcome; with a different decision it returns `APPROVAL_NOT_PENDING`.
- `inspect_agent`, `wait_agent` and `list_agents` are reads and are safe to repeat.

Client commands addressed to a child session (`USER_INPUT`, `TOOL_APPROVAL_RESPONSE`, `CANCEL_TURN` and others) are ordinary frames of that session and follow Section 16 there.

#### 8.2.6 Results

Each time a child finishes a turn (it moves to `IDLE`) or reaches a terminal state, the server builds a **`child_result`**:

| Field | Meaning |
| --- | --- |
| `status*` | `completed`, `failed`, `stopped` or `budget_exceeded`. |
| `summary*` | The child's final message, at most 2 KiB; `summary_truncated` is true if it was cut. |
| `output` | Structured output, present only when `spawn_agent` set `output_schema`. |
| `output_schema_valid` | Whether `output` passed validation. |
| `artifacts[]` | Files the child produced: `name`, `mime_type`, `bytes` and `source` (`kind: upload`, `upload_id`). The server copies each into the **parent's** upload store, so the ids are valid in the parent. |
| `usage` | The child's own usage for the whole child session so far. |
| `error` | An `ERROR` payload when `failed` or `budget_exceeded`. |
| `final_seq`, `child_turn_id` | Position in the child stream, for `inspect_agent` and attach. |

**Structured output.** With `output_schema` (a JSON Schema, 2020-12, at most 16 KiB, local `$ref` only) the child's final turn MUST produce a JSON value that satisfies it; how the server obtains it is provider-specific. The server validates it. On failure it runs one automatic correction turn in the child, charged to the child's budget. If it still fails, the child becomes `FAILED`, the result has `status: failed` and `error.code: OUTPUT_SCHEMA_INVALID` with at most 10 validation paths in `details`, and `summary` keeps the text. `output` is at most `limits.max_result_bytes` (default 64 KiB); a larger value is a failure of the same kind. A server that cannot do this does not advertise `delegation.structured_result` and rejects `output_schema` with `INVALID_ARGUMENT`.

**Delivery.** The result returns like a job result: a system event in the parent's history, with provenance (`child_session_id`, `agent_id`, `tool_call_id`, `status`), delivered by the rules of Section 14.4 with `child_turn_policy`. An `AWAITING_INPUT` child counts as needing attention and starts a turn under `always` and `on_failure`. `wait_agent` and `stop_agent` return the same `child_result` directly; the system event is still recorded, marked `delivered_via: tool`, and does not start a turn. `child_result` is untrusted data and MUST be passed to the model as data, never as instructions (Section 14.4).

#### 8.2.7 Inspection

`delegation.inspect` (narrowable in `spawn_agent`) caps what the parent can read:

| Level | Views allowed |
| --- | --- |
| `summary` | `status` |
| `events` | `status`, `events` |
| `full` | `status`, `events`, `transcript`, `jobs` |

A view above the level fails with `SCOPE_DENIED` and `details.inspect_level`.

| View | Returns | Limits |
| --- | --- | --- |
| `status` | One `child_status` object: `state`, `activity`, `last_output_excerpt` (2 KiB), `pending`, `usage`, `usage_rollup`, `budget_remaining`, `jobs[]` (id, command, status), `children[]`, `last_seq`, `delegator_suspended`. | A single object. |
| `transcript` | `items[]` of `history_item` (Appendix B), newest last, and `next_cursor`. | `limit` default 50, maximum 100. Each item's text is cut at 8 KiB and flagged `truncated`; non-text parts appear as placeholders. |
| `events` | Redacted raw events in `seq` order and `next_seq`. Deltas of one stream are coalesced by default. Reasoning deltas are never included. | `from_seq`, `limit` default 100, maximum 200. |
| `jobs` | With `job_id`, the retained output of that job from `from_index`; without it, the child's job summaries. | Subject to retention (Section 8.2.12). |

Every response is capped at `limits.max_inspect_bytes` (default 64 KiB). When a view is cut, the result has `truncated: true` and the cursor to continue. The server MUST NOT fail a request only for being large.

**Reasoning visibility.** The child's own effective `reasoning.visibility` is the only rule that matters. The parent's visibility setting governs what the parent's reasoning shows to clients and has no effect on what the parent can read about the child. The parent can never read more than the child allows: `hidden` returns nothing (transcript items show `[reasoning omitted]`); `summary` returns only items of `kind: summary`; `full` returns `raw` reasoning only at inspect level `full`, otherwise `summary`. A parent with `visibility: full` and a child with `summary` therefore sees summaries. `redacted` and opaque provider material are never returned.

Everything returned to the parent agent is untrusted data (Section 14.4) and passes the same redaction as client-bound events.

#### 8.2.8 Feedback and provenance

`message_agent` delivers by `delivery`: `steer` injects at the child's next model-call boundary without cancelling; `queue` waits for the current turn to end; `interrupt` cancels the current turn and starts a new one. By child state:

| Child state | Effect |
| --- | --- |
| `IDLE` | Starts a new turn, whatever the `delivery`. |
| `RUNNING` | As described above. |
| `AWAITING_INPUT` | The message is the answer to `ask_delegator` (reported as `delivery: answer`), whatever the mode. |
| `AWAITING_APPROVAL` | `steer` and `queue` are held until the approval resolves; `interrupt` cancels the turn and denies the call. |
| terminal | Tool error `CHILD_NOT_ACTIVE`. |
| delegator suspended | The message is held (Section 8.2.11); the result has `held: true`. |

A message from the parent reaches the child as a history item with `origin` (`kind: delegator`, `session_id`, `tool_call_id`) and the `delivery` used (the example is in Appendix A). The child treats it as delegator instruction: it cannot widen the child's scopes, budgets or approval policy, and it ranks below the user's own input. A human message written to the child directly carries `origin.kind: user`, and the parent receives `CHILD_SESSION_UPDATED` with `reason: user_message`.

#### 8.2.9 Approvals and questions

A child's `TOOL_APPROVAL_REQUEST` is raised in the child session and announced on the parent stream as `CHILD_SESSION_UPDATED` (`state: AWAITING_APPROVAL`, `reason: approval_pending`, `pending`). The request's `expires_at` is authoritative.

**`approval_routing: human`** (default). Only a human controller on the child answers it. An agent never approves.

**`approval_routing: parent`.** The routing applies to `risk: low` requests only; `medium` and `high` always go to a human.

1. `pending.routed_to` is `parent`. The request is recorded in the parent's history as a system event and delivered by `child_turn_policy`: injected at the next model-call boundary of an active parent turn, or starting a new turn.
2. The parent agent answers with `approve_child_call` (`approve` or `deny`). It cannot answer `approve_for_session`: grants never pass between sessions, and a grant given in a child applies to that child only.
3. If the parent has not answered within `approval_parent_timeout_s` (never longer than half of the time left to `expires_at`), the request escalates: `CHILD_SESSION_UPDATED` with `reason: approval_escalated` and `routed_to: human`. Any attached controller of the child can then answer.
4. If nobody answers by `expires_at`, the request counts as `deny`. The child's tool call finishes with status `DENIED` and the model gets tool error `APPROVAL_TIMEOUT`; the child returns to `RUNNING`. The timeout does not cancel the turn.
5. A parent's answer on a `medium` or `high` request returns `RISK_TOO_HIGH`. An answer after resolution returns `APPROVAL_NOT_PENDING`.
6. Each decision, with its author (`parent` agent or user), is written to the audit log (Section 24).

**`approval_routing: group` and `policy`.** These routes follow Section 13.6: the request is announced with an `approval_id` and its `approvers`, any eligible approver can answer it, also over HTTP without attaching, and an agent never answers `medium` or `high` risk requests.

**Questions.** `ask_delegator` (`question`, `options`, `timeout_s`) moves the child to `AWAITING_INPUT` and the parent is notified (`reason: input_requested`, `pending.kind: input`, `pending.question`). The parent answers with `message_agent`; a human with controller rights can answer with `USER_INPUT` on the child. Without an answer by `timeout_s`, the child's call returns `answered: false, timed_out: true` and the child continues. Only the parent agent is asked: to escalate further, the parent calls its own `ask_delegator` or asks a human.

#### 8.2.10 Parent stream events

The parent stream carries summaries only, never a mirror of child events. Payloads are specified in Appendix B and illustrated in Appendix A.

- `CHILD_SESSION_STARTED`: `child_session_id*`, `agent_id*`, `tool_call_id*`, `lifetime`, `task_summary` (at most 512 characters), `depth`, `name`, `budget_reserved`.
- `CHILD_SESSION_UPDATED`: `child_session_id*`, `state*`, `reason*` (`state_change`, `approval_pending`, `approval_escalated`, `input_requested`, `budget_warning`, `user_message`, `turn_finished`, `delegator_control`, `error`), `summary`, `usage`, `budget_remaining`, `pending`, `result`, `child_seq`, `error`.

`child_seq` is the child's `last_seq` when the update was emitted. A client attached to both sessions can use it to place the child's events against the parent's. There is no global order across sessions: `seq` is per session.

#### 8.2.11 Client access and takeover

For detail, a client attaches to the child like to any session. A connection MAY attach to several sessions: each frame is routed by `session_id`, and `seq` and `ACK` are per session (over SSE, one stream per session). A client authorised on the parent may attach to its descendants; authorisation on the root covers the tree and is revoked with it. A client with no right on the root cannot attach to a child even if it knows the id.

`delegation.child_access` decides the default mode: `observer` (default) gives read access only. `controller` lets the client use `CANCEL_TURN`, `SESSION_CLOSE`, `USER_INPUT`, `HISTORY_GET` and `TOOL_APPROVAL_RESPONSE` on the child, subject to the token's own scopes and to the single-controller rule of Section 10. The delegating agent acts through tools, not through an attachment, so it does not count as a controller.

A human who wants the child to themselves uses **takeover**. A controller of the child sends `DELEGATOR_CONTROL` with `action: suspend`. The server sets `delegator_suspended`, emits `DELEGATOR_STATE_CHANGED` in the child and `CHILD_SESSION_UPDATED` with `reason: delegator_control` in the parent, and while suspended:

- `message_agent` is held (at most 20 messages; more fail with `DELEGATOR_SUSPENDED`), and delivered in order after `release`;
- `stop_agent` fails with `DELEGATOR_SUSPENDED`, unless the parent session is closing, which always wins;
- `inspect_agent`, `wait_agent` and `list_agents` keep working.

`action: release`, a detach of the controlling client (after a grace period of the server's choosing) or the end of the child session lifts the suspension. A suspended child still counts against every limit and budget.

#### 8.2.12 Accounting, retention and observability

- **Usage.** Every `usage` object counts the consumption of its own session only. `usage_rollup` (optional in `TURN_STATE_CHANGED`, `SESSION_STATE` and `child_status`) adds the usage of all descendants, so a parent total is never double counted. `SESSION_STATE.usage_total` is the session's own cumulative usage.
- **Retention.** When a child reaches `CLOSED`, the server keeps its history, event log, job records and job output, and the artifacts of its last result, for `limits.child_retention_s` (default: `job_output_retention_s`). Jobs are killed at closure (Section 14.5); their retained output stays readable by `inspect_agent` (`view: jobs`) and `JOB_GET` for authorised parents and clients. A client MAY attach to a closed child as an observer during retention. After retention, requests fail with `SESSION_NOT_FOUND` (clients) or `CHILD_NOT_FOUND` with `details.reason: expired` (tools). Purging the parent purges its children.
- **Observability.** Children are sessions: they produce their own logs, metrics and traces, linked to the tree (Section 28). Child events are audited like any session's, plus `parent_session_id` and the delegating `tool_call_id`.

### 8.3 Handoff

Handoff transfers the rest of the conversation to another agent in the same session. It starts in one of two ways:

- The model calls the `handoff_agent` tool (`agent_id*`, `reason`, `carry_context`). The target MUST appear in the current agent's `handoff_targets`, and the session's scopes MUST allow it; otherwise the tool returns `HANDOFF_NOT_ALLOWED`. An unknown target returns `AGENT_NOT_FOUND`.
- The controller sends `SESSION_CONFIGURE` with `agent_id` (Section 8.1), which is checked against the token's scopes only.

The switch takes effect at the next turn boundary: the turn in which `handoff_agent` was called finishes under the current agent, and the following turn uses the new one. The server emits `AGENT_CHANGED` (`from_agent`, `to_agent`, `reason`, `effective` summary) at the switch. The session id, history, usage, jobs and children stay; the new agent's `system_prompt`, tools, scopes and budgets apply, scopes are recomputed as in Section 7, and the budget already used carries over.

`carry_context` decides what the new agent sees: `full` (default) the whole history, `summary` a server-generated summary in place of older history, `none` only the handoff `reason`, as a system event. The stored history is the same in all three cases.

### 8.4 Notices

Notices are informational server events. Clients need not render them, and unknown notices are ignored.

- `BUDGET_WARNING`: `scope` (`turn` or `session`), `metric` (`tokens`, `cost`, `tool_calls`, `duration`), `used`, `limit`, `threshold` (percent, default 80). Sent once per threshold crossing. `SESSION_STATE` also reports `budget_remaining` per metric.
- `HISTORY_COMPACTED`: `up_to_turn_id`, `strategy` (`summarize` or `truncate`), `trigger` (`auto` or `manual`), `tokens_before`, `tokens_after`. Sent when the server shrinks the model context; the stored history is unaffected.
- `AUTH_EXPIRING`: see Section 4.6.

## 9. Out-of-Band Notifications

A session can be `SUSPENDED` while a job or a child finishes, or while a tool call waits for approval. A client that is not connected cannot learn it. Notifications let the server tell a registered endpoint that something needs attention. A server that supports them advertises `notify` and `limits.max_notify_targets`.

A notification is only a **nudge**. It carries no content beyond a short summary, it never starts a turn and never wakes work that would not otherwise run. The client reattaches with `resume_from_seq` and reads the events through the normal channel (Section 16).

**Configuration.** The controller sets `options.notify` with `SESSION_CONFIGURE` (Section 8.1) or `SESSION_ATTACH.options`. The object is `notify_config` (Appendix B):

| Field | Meaning |
| --- | --- |
| `channel*` | `webhook`. Other channels (for example mobile push) are extensions named `X_<NS>_` (Section 26.2). |
| `endpoint_id` or `url` | A server-registered endpoint (preferred), or an HTTPS URL. Servers MAY forbid `url` by policy. Exactly one is required. |
| `secret` | Write-only, at least 32 characters, used to sign deliveries with `url`. Never echoed, redacted from logs and replay buffers like `AUTH_REFRESH.token`. A registered endpoint carries its own secret. |
| `events*` | Event types to notify: any of `TURN_STATE_CHANGED` (terminal states only), `JOB_FINISHED`, `TOOL_APPROVAL_REQUEST`, `CHILD_SESSION_UPDATED`, `BUDGET_WARNING`, `SESSION_CLOSED`, `ERROR` (turn- or job-scoped). |
| `when` | `detached` (default: only when no client is attached to the root session) or `always`. |
| `include` | `summary` (default: up to 256 characters of redacted text) or `none`. |
| `include_children` | Default true: events of descendants are notified too. |
| `coalesce_s` | Default 5. Notifications of the same session and type inside the window merge into one. |

The server stores the configuration with the session; it survives detach and suspension. `SESSION_STATE.notify` shows a `notify_summary` without secret or full URL (only `url_host`). On configuring a `url`, the server sends a verification `POST` with `type: PING`, and the configuration is rejected with `ERROR INVALID_FRAME` and `details.reason: endpoint_unreachable` unless the endpoint answers `2xx` within 5 s.

**Delivery.** One `POST` per notification with `Content-Type: application/json` and a body of at most 4 KiB, the `webhook_body` object: `notification_id`, `session_id`, `root_session_id`, `seq`, `type`, `turn_id`, `job_id`, `child_session_id`, `occurred_at`, `summary`, `attempt`. Headers: `RAEP-Notification-Id`, `RAEP-Delivery-Attempt` and

```
RAEP-Signature: t=<unix seconds>,v1=<hex HMAC-SHA256(secret, t + "." + raw body)>
```

The receiver MUST verify the signature over the raw body, MUST reject a `t` more than 300 s from its clock, and MUST deduplicate by `notification_id`. During a secret rotation the server MAY send two `v1` values for 10 minutes.

**Reliability.** Delivery is at least once and unordered; `seq` lets the receiver sort. A response `2xx` is success. Anything else, or no answer in 10 s, is a failure and is retried with exponential backoff (base 1 s, factor 2, cap 5 min) for at most 12 attempts or 24 h. `410` disables the target at once. Redirects are not followed. After the final failure the target is `disabled`; the server persists `NOTIFICATION_STATUS` (`notify_state`, `consecutive_failures`, `last_error`, `undelivered`) and sends it on the next attach.

**Security.** The SSRF protections of Section 12.4 (HTTPS only, no private or link-local addresses, DNS pinning) apply to `url`, and deliveries SHOULD leave through an egress path separate from the sandboxes. Servers rate-limit deliveries per session (at most 60 per minute) and per tenant. A notification MUST NOT contain tool arguments, tool output, model text or reasoning.

## 10. Session Lifecycle

States: `ACTIVE`, `SUSPENDED`, `CLOSED`.

- A session is `ACTIVE` while at least one client is attached, or a turn or job is running, or any of its descendants (Section 8.2) is not in a terminal state. A parent with a running `lifetime: session` child is therefore never `SUSPENDED`.
- With no clients attached and no running work for the idle timeout (server policy), it becomes `SUSPENDED`. Background jobs keep running; their events are persisted. Attaching resumes it.
- `CLOSED` is terminal. It results from `SESSION_CLOSE`, retention expiry, or administrative action. The server then emits `SESSION_CLOSED` and releases sandboxes.
- `SESSION_DETACH` ends this client's attachment without affecting the session.
- Attaching as `observer` needs the scope `session:attach`, attaching as `controller` needs `session:control`, and an attach that creates a session also needs `session:create` (Section 25.2).
- `SESSION_CLOSE` (controller only; `reason`, `purge`) closes the session. The server answers `SESSION_CLOSED` with a `reason`: `client`, `ttl`, `retention`, `admin` or `parent_closed`. Retention is server policy, advertised as `limits.session_retention_s`.
- Closing a session closes its descendants first (Section 8.2.3). A child session is closed by its own `SESSION_CLOSE` (from a controller client), by `stop_agent`, by its TTL, or with its parent.

**Multiple clients.** A session MAY have many `observer` attachments but at most one `controller`, unless collaboration is enabled (Section 21). A second controller attach MUST fail with `ERROR SESSION_BUSY` unless the payload sets `takeover: true`, in which case the previous controller is closed with `4009`. Observers receive all server events but MUST NOT send `USER_INPUT`, `CANCEL_*` or approvals (`FORBIDDEN`).

**Persistence.** The server persists, per session: history, turn records, job records and output (within retention), tool-approval grants, the event log needed for replay, the parent link and child list, the tool-call records that deduplicate delegation tools, the notification configuration (without secret in the replay log), generated outputs (Section 12.5), workspace snapshots (Section 18.4), and the durable event log that backs replay (Section 17).

## 11. Session Export and Import

A session, with its children, can move between servers. Export produces a **bundle**; import creates new sessions from it. Both are optional capabilities (`export`, `import`) and need the scopes `session:export` and `session:import`. This is a one-shot transfer: it is neither synchronisation nor a way to share a live session.

### 11.1 Export

A controller sends `SESSION_EXPORT` (`include`, `quiesce`, `wait_timeout_s`, `encrypt`). `include` selects `children`, `uploads`, `job_output`, `outputs` (all default true) and `workspace` (default false; it needs the capability `workspace.snapshots` and uses the snapshot of Section 18.4, within `limits.max_workspace_bytes`). Exporting a non-root session exports it as a new root, with its `parent` link dropped.

A session cannot be exported while it is active. `quiesce` decides what happens when it is: `reject` (default) fails with `ERROR SESSION_BUSY`; `wait` waits up to `wait_timeout_s` for the subtree (every turn and job) to end; `kill_jobs` cancels running jobs and waits for turns. The server then replies `SESSION_EXPORT_READY` (`export_id`, `download_url`, `bytes`, `sha256`, `expires_at`, `sessions`, `encrypted`, `signed`). The bundle is fetched with an authenticated `GET /raep/v1/exports/{export_id}`, which supports `Range`, and expires after `limits.export_ttl_s` (default 3600). A bundle over `limits.max_export_bytes` fails with `PAYLOAD_TOO_LARGE`.

**Bundle.** A ZIP archive, media type `application/vnd.raep.bundle+zip`:

```
manifest.json                              bundle_manifest (Appendix B)
signature.jws                              OPTIONAL detached JWS over manifest.json
sessions/<session_id>/session.json         agent reference, options, delegation settings
sessions/<session_id>/history.jsonl        one history_item per line
sessions/<session_id>/jobs/<job_id>.json   job record (and <job_id>.out when included)
sessions/<session_id>/uploads/<upload_id>  uploaded media
sessions/<session_id>/outputs/<output_id>  generated media (Section 12.5)
sessions/<session_id>/workspace.tar        workspace snapshot, with include.workspace (Section 18.4)
```

`manifest.files` lists every other file with its `sha256`; the signature, if present, covers the manifest and therefore the files. `encrypt.recipient_jwk` encrypts the archive to that key (JWE). Never exported: credentials and tokens, `approve_for_session` grants, notification secrets, provider-opaque reasoning material, system prompts, replay buffers, running processes, and the sandbox filesystem except the workspace snapshot requested with `include.workspace`. The manifest names what was excluded in `excluded`.

### 11.2 Import

Import is two steps over HTTP and works with either transport.

1. `POST /raep/v1/imports` with the bundle (raw, or resumable as in Section 12.4) validates it without creating anything and returns `import_report`: `import_id`, `signature` (`valid`, `invalid`, `absent` or `untrusted`), the agents it needs and whether each is `available`, and warnings. The preview expires after one hour.
2. `POST /raep/v1/imports/{import_id}/commit` with `import_commit_request` (`agent_map`, `carry_usage`, `decryption_jwk`) creates the sessions and returns `import_result`: the new root `session_id`, the `id_map` of every old id to its new id, and warnings. `DELETE /raep/v1/imports/{import_id}` discards a preview.

Rules:

- **New identities.** Every session, upload and output gets a new id, so ids never collide and a bundle can be imported more than once. Links between parents and children are remapped. `seq` restarts at 1; `SESSION_STATE.imported_from` records the exporter, the original id and the times.
- **Agents.** Each session binds to `agent.id` and `agent.version` from the manifest, or to the target in `agent_map` (keys `id@version`). If an agent is unavailable and unmapped, the commit fails with `AGENT_NOT_FOUND`.
- **No privilege comes from the bundle.** Effective scopes and budgets are computed from the importer's agent definitions and the importing token. The bundle cannot widen them.
- **Usage.** With `carry_usage` (default true) the imported `usage_total` counts against session budgets, so an export and import cannot be used to reset a budget.
- **State.** The root is created `SUSPENDED`. Children keep `IDLE` (with a fresh TTL) or their terminal state. No turn or job is running.
- **Workspace.** A bundled workspace snapshot seeds the new session's sandbox as its initial workspace. It is never executed during import.
- **Trust.** Imported history is data from another system. The server MUST keep each item's `role` and `origin` as recorded, MUST NOT promote any item to a higher trust level (for example a `tool` item to `user`), and labels the content with its provenance.
- **Signature.** A server MAY require a valid signature from a trusted issuer (`import.require_signature`); a bundle that fails fails with `BUNDLE_SIGNATURE_INVALID`. Unsigned bundles are accepted only if policy allows.
- **Versions.** A bundle with an unknown `bundle_version` fails with `BUNDLE_VERSION_UNSUPPORTED`; a corrupt one (hash mismatch, missing file) with `BUNDLE_INVALID`.

## 12. Turn Lifecycle

### 12.1 Concurrency rules

- A session has **at most one active turn**.
- The client generates `turn_id` on `USER_INPUT`. Reusing a `turn_id` is idempotent: the server returns the existing turn's state instead of starting a new one.
- `USER_INPUT` during an active turn is governed by `queue_policy`: `reject` (default; `ERROR SESSION_BUSY`), `queue` (server enqueues; state `QUEUED`), or `interrupt` (equivalent to `CANCEL_TURN` followed by the new input).
- At most `limits.max_queued_inputs` inputs wait in the queue; one more fails with `ERROR SESSION_BUSY`.
- Server-initiated turns (for example triggered by a finished job or a child result) use a server-generated `turn_id`.
- The rule is per session: each child session (Section 8.2) has its own active turn, so a tree can run many turns at once.

### 12.2 State machine

```
QUEUED -> CALLING_PROVIDER <-> EXECUTING_TOOL
               |    ^              |
               |    +-- AWAITING_APPROVAL (from EXECUTING_TOOL, before run)
               v
  COMPLETED | FAILED | CANCELLED   (terminal)
```

Any non-terminal state MAY move to `CANCELLED` on cancel, or to `FAILED` on unrecoverable error. A terminal state is emitted exactly once per turn, and no further events with that `turn_id` follow it except late job events, which carry `job_id`. A turn blocked in `wait_agent` stays in `EXECUTING_TOOL`.

### 12.3 Events

`TURN_STATE_CHANGED` payload: `state` (required), and on terminal states: `stop_reason` (`end_turn`, `max_tokens`, `cancelled`, `error`, `tool_limit`, `budget_exceeded`), `usage` (`input_tokens`, `output_tokens`, `cached_tokens`, `reasoning_tokens`, `tool_calls`, `duration_ms`, `cost` as `{amount, currency}`, `extra`; the session's own consumption only), optional `usage_rollup` (the same including descendants), `error` (an `ERROR` payload) when `FAILED`, and `output` with `output_schema_valid` when the turn used a `response_format` (Section 12.6).

`TOKEN_DELTA` payload: `delta` (string), `index` (integer, per turn, from 0), `channel` (`text` or `reasoning`, default `text`). Reasoning content is emitted only if the server policy allows it.

**Reasoning channel.** Reasoning output is standardised rather than left to extensions. `TOKEN_DELTA` with `channel: reasoning` carries a `kind`: `summary` (a provider-neutral digest), `raw` (the full reasoning text, only when the agent's `reasoning.visibility` is `full`) or `redacted` (a placeholder with no text, so a client can show that reasoning took place). Clients MUST treat reasoning as display-only: they MUST NOT send it back as input or store it as authoritative. The server keeps whatever opaque material a provider needs for round-tripping and never exposes it. With `visibility: hidden` no reasoning deltas are sent, and `usage.reasoning_tokens` still reports the count when the provider supplies it.

```json
{"version":"1.0","id":"01JB0000000000000000000009","seq":14,"ref":null,"session_id":"sess_88392","turn_id":"turn_101","job_id":null,"timestamp":1793404803500,"type":"TURN_STATE_CHANGED","payload":{"state":"COMPLETED","stop_reason":"end_turn","usage":{"input_tokens":1820,"output_tokens":241,"cached_tokens":1500,"tool_calls":1,"duration_ms":2410}}}
```

### 12.4 User Input Content and Modalities

`USER_INPUT` carries exactly one of these:

- `prompt`: a string, shorthand for a single `text` part.
- `content`: an ordered array of parts (1 to `limits.max_parts`).

Optional fields: `queue_policy` (Section 12.1), `overrides` (Section 7, bounded by `overridable`), `output` (modalities the client wants back, default `["text"]`), `response_format` (Section 12.6), `replaces_turn_id` (Section 18.2) and `metadata` (an opaque object of at most 4 KiB, stored with the turn and never shown to the model).

| Part `type` | Fields |
| --- | --- |
| `text` | `text` |
| `image` | `source`, `mime_type`, `alt`, `detail` (`low`, `high`, `auto`) |
| `document` | `source`, `mime_type`, `name`, `pages` (optional range) |
| `audio` | `source`, `mime_type`, `duration_ms`, `transcript` (optional, client-provided) |
| `video` | `source`, `mime_type`, `duration_ms` |

`source` is an object whose `kind` selects how the bytes arrive:

| Kind | Extra fields | Use |
| --- | --- | --- |
| `inline` | `data` (base64) | Small payloads; the whole frame must stay under `max_frame_bytes`. |
| `upload` | `upload_id` | Preferred for anything large; works on both transports. |
| `url` | `url` | The server fetches it. HTTPS only, no private or link-local addresses, with size and time limits (SSRF protection). |
| `output` | `output_id` | A non-text part the agent produced earlier in this session (Section 12.5), for example to ask for an edit. |
| `stream` | `stream_id` | A committed realtime input stream (Section 12.7). |

**Acceptance.** When a `USER_INPUT` passes validation, the server emits `USER_INPUT_ACCEPTED` to every attached client, the sender included: `turn_id`, `origin` (`kind` and, for users, `user_id` and `display_name`), `content` as parts (an `inline` source is replaced by an `upload` reference, so media bytes are not echoed), `position` (the queue position when the input waits) and `replaces_turn_id`. It tells observers and other controllers what was asked, and it tells the sender that the input was accepted. Turns started by a job, a child, a schedule or a trigger are announced the same way, with the matching `origin.kind`.

**Uploads.** `POST /raep/v1/sessions/{session_id}/uploads` with the raw bytes and a `Content-Type` returns `upload_id`, `mime_type`, `bytes`, `sha256` and `expires_at`. An upload not referenced by a turn expires after 1 hour; a referenced one lives as long as the session. Uploads are session-scoped, so another session cannot use the id. The server MUST reject types it does not accept and SHOULD scan content before use.

**Resumable uploads.** A `POST` to the uploads endpoint with `Content-Type: application/json` (`size`, `mime_type`, `name`, optional `sha256`) creates a resumable upload and returns `upload_id`, `upload_url` and `offset: 0`; any other content type is a single-shot upload of that content. The client sends bytes with `PUT upload_url` and `Content-Range: bytes <start>-<end>/<size>`. `HEAD upload_url` returns the current `Upload-Offset`, so an interrupted transfer resumes from there. The upload completes when the last range arrives and the `sha256`, if given, matches. Resumable uploads work the same way on both transports.

**Validation.** Before a turn starts, the server checks every part against the effective agent's `capabilities.input` and `limits`:

- Unsupported modality: `ERROR UNSUPPORTED_MODALITY`, with `details.part_index` and `details.supported`.
- Disallowed MIME type: `ERROR UNSUPPORTED_MIME_TYPE`.
- Size, count or duration over a limit: `ERROR PAYLOAD_TOO_LARGE`.
- Unknown, expired or foreign `upload_id` or `output_id`: `ERROR UPLOAD_NOT_FOUND`.

A rejected input starts no turn and consumes no budget.

**History.** Media is stored once, server-side. `SESSION_STATE.history_summary` shows non-text parts as short placeholders in `content` (for example `[image: diagram.png]`), and `HISTORY_PAGE` returns parts with `source` replaced by an `upload` reference.

**Output.** Text output flows through `TOKEN_DELTA`. Non-text output (image, audio, video, document) uses the events of Section 12.5, and only when both sides negotiated the modality through `capabilities.output` and `render`.

```json
{"version":"1.0","id":"01JB0000000000000000000070","session_id":"sess_88392","turn_id":"turn_104","job_id":null,"timestamp":1793404820000,"type":"USER_INPUT","payload":{"content":[{"type":"text","text":"What is wrong with this screenshot and the attached log?"},{"type":"image","source":{"kind":"upload","upload_id":"upl_7f3a"},"mime_type":"image/png","alt":"build screenshot"},{"type":"document","source":{"kind":"upload","upload_id":"upl_81c2"},"mime_type":"text/plain","name":"build.log"}],"output":["text"],"overrides":{"reasoning":{"effort":"high"}}}}
```

### 12.5 Output Parts

Non-text output uses a small set of events and an output store. A server MUST NOT emit parts unless the agent lists the modality in `capabilities.output`, the client lists it in `capabilities.render` and declares the `output.parts` feature (Section 6), and `USER_INPUT.output` asks for it. A server that supports them advertises `output.parts`. If a requested modality is not supported, the input fails with `ERROR UNSUPPORTED_MODALITY` before the turn starts.

| Event | Payload |
| --- | --- |
| `OUTPUT_PART_STARTED` | `part_id*`, `index*` (order among the turn's parts, from 0), `type*` (`image`, `audio`, `video`, `document`), `mime_type*`, `streamed*`, `name`, `alt`, `duration_ms`, `width`, `height`, `bytes_expected` |
| `OUTPUT_PART_DELTA` | `part_id*`, `index*` (per part, from 0), `data*` (base64). Only if `streamed` is true |
| `OUTPUT_PART_FINISHED` | `part_id*`, `status*` (`OK`, `ERROR`, `CANCELLED`), `source`, `bytes`, `sha256`, `transcript`, `error` |

Part events carry the `turn_id` and take their place in the session `seq` order, interleaved with `TOKEN_DELTA`. `streamed: true` is for live audio or video, where deltas are chunks in order, each below `max_frame_bytes`; everything else is sent with `streamed: false`, no deltas, and fetched after `FINISHED`. Large media SHOULD NOT be streamed in frames.

**Store.** On success the server keeps the bytes once, session-scoped, and `source` is `{kind: output, output_id}` (or `{kind: url, url, expires_at}` for a signed URL). Fetch with `GET /raep/v1/sessions/{session_id}/outputs/{output_id}`, authenticated, with `Range` support. Outputs live as long as the session history and under the same retention and purge rules. `HISTORY_PAGE` returns parts with an `output` source, `SESSION_STATE.history_summary` shows placeholders, and a later `USER_INPUT` part may reference an earlier output with `source.kind: output` (Section 12.4), for example to ask for an edit.

**Rules.** `transcript` or `alt` SHOULD accompany audio and images, for clients that cannot render them. Cancelling a turn finishes open parts with `status: CANCELLED`; partial bytes are discarded unless the part is `streamed`. Parts count against `limits.max_output_part_bytes`; breaking it ends the part with `ERROR` and code `SANDBOX_LIMIT_EXCEEDED`. The server SHOULD scan generated content as it does uploads. Usage for generated media goes in `usage.extra`. Over SSE each part event is an ordinary event; the same size guidance applies.

### 12.6 Structured Output

`USER_INPUT.response_format` is `{type: "text"}` (the default) or `{type: "json_schema", schema, strict}`. `schema` is a JSON Schema (2020-12, at most 16 KiB, local `$ref` only). The capability is `structured_output` and the agent must list `structured_output` in `capabilities.features`; otherwise the input fails with `ERROR INVALID_FRAME` and `details.reason: response_format_unsupported`.

With `json_schema`, the turn's final assistant message MUST be one JSON value that satisfies the schema. The server validates it. If validation fails and `strict` is not true, the server runs one correction attempt inside the turn, charged to the turn's budgets; if it still fails, or `strict` is true, the turn ends `FAILED` with `error.code: OUTPUT_SCHEMA_INVALID` and at most 10 validation paths in `details.paths`. On success the terminal `TURN_STATE_CHANGED` carries `output` (the validated value, at most `limits.max_result_bytes`) and `output_schema_valid: true`. The text still streams through `TOKEN_DELTA`. The format applies to the final message only; tool calls are unaffected. A value larger than the limit is a failure of the same kind.

### 12.7 Realtime Input Streams

For live audio or video, a controller opens an input stream instead of sending a finished part. The capability is `input.stream`, the client declares the `input.stream` feature, and the agent lists `audio` or `video` in `capabilities.input` (natively or `via` conversion).

| Frame | Payload |
| --- | --- |
| `INPUT_STREAM_START` | `stream_id*`, `type*` (`audio` or `video`), `mime_type*`, `params` (for example `sample_rate`, `channels`, `frame_rate`), `turn_detection` (`manual`, the default, or `server`) |
| `INPUT_STREAM_DATA` | `stream_id*`, `index*` (from 0, consecutive), `data*` (base64), `timestamp_ms` |
| `INPUT_STREAM_END` | `stream_id*`, `commit` (default true) |

- At most one input stream is open per session. An `index` that skips or repeats fails with `ERROR INVALID_FRAME`. A stream above `limits.max_input_stream_bytes` is closed with `PAYLOAD_TOO_LARGE`. Each data frame stays under `max_frame_bytes`.
- **`manual`.** After `INPUT_STREAM_END` with `commit: true`, the stream is a part that a `USER_INPUT` references with `source.kind: stream` and the `stream_id`. The input MUST arrive within 60 s, or the stream is discarded. `commit: false` discards it at once.
- **`server`.** The server segments the stream into utterances. Each utterance starts a turn with a server-generated `turn_id`, announced by `USER_INPUT_ACCEPTED` (Section 12.4) and `TURN_STATE_CHANGED`. When speech starts while a turn is running, the server cancels that turn (`stop_reason: cancelled`) so the user can interrupt the agent (barge-in).
- Spoken replies use output parts with `streamed: true` (Section 12.5).
- The frames are ordinary frames over SSE too, but latency makes WebSocket the intended binding.

## 13. Tools

### 13.1 Synchronous tool calls

A tool call that runs inside the turn is bracketed by:

- `TOOL_CALL_STARTED`: `tool_call_id`, `tool_name`, `arguments` (object, redacted per Section 24), `requires_approval` (boolean), `executor` (`server`, the default, or `client`), `attempt` (from 1) and `timeout_s`.
- `TOOL_OUTPUT_DELTA`: `tool_call_id`, `tool_name`, `stream` (`stdout` or `stderr`), `delta`, `index` (per tool call).
- `TOOL_CALL_FINISHED`: `tool_call_id`, `status` (`OK`, `ERROR`, `DENIED`, `CANCELLED`, `TIMED_OUT`), `exit_code` (integer or null), `duration_ms`, `attempts`, `result_summary`, `result_truncated`.

### 13.2 Detaching

A tool MAY return a job instead of a result. The server then emits `JOB_STARTED` and `TOOL_CALL_FINISHED` with `status` `OK` and `result_summary` naming the job. The turn continues and can complete while the job runs (Section 14).

### 13.3 Human approval

When policy requires approval, the server emits `TOOL_APPROVAL_REQUEST` (`tool_call_id`, `tool_name`, `arguments`, `risk` of `low`, `medium` or `high`, `expires_at`) and moves the turn to `AWAITING_APPROVAL`. The controller answers with `TOOL_APPROVAL_RESPONSE` (`tool_call_id`, `decision` of `approve`, `deny` or `approve_for_session`, optional `reason`).

- No response by `expires_at` counts as `deny`.
- `CANCEL_TURN` while awaiting approval cancels the turn and denies the call.
- A denial is returned to the model as a tool error so it can adapt.
- Approval frames from observers are rejected with `FORBIDDEN`.
- A controller that did not declare the `approvals` client feature (Section 6) never receives `TOOL_APPROVAL_REQUEST`: a call that needs approval finishes with `status: DENIED`, unless server policy makes approvals mandatory, in which case the attach fails with `CAPABILITY_REQUIRED`.

### 13.4 Tool Descriptors and Reliability

Every tool, whether built-in, from an MCP server, executed by a client or remote, is described by a **tool descriptor**:

| Field | Meaning |
| --- | --- |
| `name*` | Unique within the agent's tool set. |
| `description` | For the model. |
| `input_schema*` | JSON Schema of the arguments. |
| `output_schema` | Optional JSON Schema of the result. |
| `executor` | `server` (default) or `client` (Section 13.5). |
| `risk` | `low`, `medium` (default) or `high`; drives approval (Section 13.6). |
| `side_effects` | `none`, `local` or `external` (default). |
| `idempotent` | Whether repeating the call with the same arguments is safe. Default false. |
| `timeout_s` | Per attempt. Default 60 (client tools: 120). |
| `retry` | `{max_attempts, backoff_ms, retry_on}`; default one attempt. `retry_on` holds `TIMED_OUT` and `ERROR`. |
| `scopes[]` | Scopes the call needs (Section 25.2). |
| `detachable` | Whether the tool may return a job (Section 13.2). |

`tools.policy` in the agent spec overrides `timeout_s`, `retry` and the approval mode per tool name, and can only make them stricter than the descriptor's `risk` allows (it may lower `timeout_s` and `max_attempts`, and may require approval where the descriptor does not). The agent summary lists tool names only.

**Rules.**

1. **Write-ahead.** The server persists the tool-call record (id, tool, arguments, attempt) before it executes, and the result before it returns it to the model (Section 17).
2. **Idempotency key.** `tool_call_id` is the idempotency key passed to the backend: the `Idempotency-Key` header for HTTP tools, `_meta.idempotency_key` for MCP tools.
3. **Timeout.** When `timeout_s` passes, the server terminates the call and finishes it `TIMED_OUT`; the model gets tool error `TOOL_TIMEOUT`.
4. **Retry.** A call is retried only if the tool is `idempotent` or has `side_effects: none`, up to `retry.max_attempts`, with the `backoff_ms` delay. Each attempt emits `TOOL_CALL_STARTED` with the same `tool_call_id` and an increasing `attempt`; clients discard the output of earlier attempts when a new one starts. `TOOL_CALL_FINISHED` is emitted once, with `attempts`.
5. **Result size.** A result above `limits.max_tool_result_bytes` is truncated for the model, and `TOOL_CALL_FINISHED` sets `result_truncated`.
6. **Crash.** A call in flight during a crash is not re-executed unless it is idempotent (Section 17.3).

### 13.5 Client-Side Tools

A client-side tool runs on the client (an IDE reading a file, a CLI opening a diff). The capability is `client_tools`.

**Declaration.** The controller declares tools in `SESSION_ATTACH.capabilities.client_tools[]`, each a tool descriptor (Section 13.4) with `executor: client`. A re-attach replaces the declaration. An agent may use them only if its tool set allows it: `tools.client` is `false` (default), `true` or a list of tool names. A name that collides with a server tool fails the attach with `ERROR INVALID_FRAME`. Sessions of child agents never inherit client tools.

**Flow.**

1. The model calls the tool. The server emits `TOOL_CALL_STARTED` with `executor: client` and `CLIENT_TOOL_REQUEST` (`tool_call_id`, `tool_name`, `arguments`, `expires_at`), then waits; the turn stays in `EXECUTING_TOOL`.
2. The controller runs the tool and answers with `CLIENT_TOOL_RESULT` (`tool_call_id*`, `status*` `OK` or `ERROR`, `content` as text or parts, `result_summary`).
3. The server emits `TOOL_CALL_FINISHED` and the turn continues.

**Rules.**

- Only a controller that declared the tool may answer. The first valid result wins and repeats are ignored; a result for a call that is not pending fails with `ERROR TOOL_CALL_NOT_PENDING`.
- Tools are offered to the model only while a controller that declared them is attached. If the controller detaches during a call, the call stays pending until `expires_at`, and a re-attach with `resume_from_seq` replays the request. At `expires_at` the call finishes `TIMED_OUT`.
- With several controllers (Section 21), the request goes to the declaring controller that sent the latest input, otherwise the first declaring one.
- The result is untrusted data (Section 24) and is limited to `limits.max_client_tool_result_bytes`. Approval follows `tools.approval` like any tool; the same controller answers it.
- A client tool never gives the model server-side authority: it can neither widen scopes nor read what the server holds.

### 13.6 Approval Policy and Routing

`approval.routing` in the agent spec chooses who decides: `human` (the default: the attached controllers), `group` (a named group of approvers) or `policy` (a policy service decides, and may ask humans). `approval_routing` of Section 8.2.1 uses the same values, plus `parent`.

**Request.** Every `TOOL_APPROVAL_REQUEST` carries an `approval_id` (unique, server-generated) and optionally `approvers` (`{kind, group_id, quorum, eligible}` with `kind` `controller`, `group` or `user`) and `reason` (the policy rule that required approval).

**Answering.** An eligible approver answers in one of two ways, and both reach the same decision:

- A controller sends `TOOL_APPROVAL_RESPONSE` with the `tool_call_id` or the `approval_id`.
- Any authenticated approver, attached or not, uses `GET /raep/v1/approvals?status=pending` to find requests and `POST /raep/v1/approvals/{approval_id}` with `{decision, reason}` to answer. The scope is `approve:low`, `approve:medium` or `approve:high`, matching the request's `risk`.

**Quorum.** `quorum` (default 1) is the number of distinct approvers that must approve. Any eligible approver's `deny` ends the request as denied (veto). With `approval.separation_of_duties: true` (the default for `high` risk) the user whose input led to the call does not count toward the quorum. Without a quorum by `expires_at`, the request counts as `deny`.

**Resolution.** The server emits `APPROVAL_RESOLVED` (`approval_id`, `tool_call_id`, `decision`, `decided_by[]` each `{user_id, decision, at}`, `quorum_met`) to the session stream, so other approvers see that the request is closed. A later answer returns `ERROR APPROVAL_NOT_PENDING`.

**Policy decision point.** With `approval.routing: policy`, before asking anyone the server sends its policy service this request, whose transport is server-private:

```json
{"tenant":"t_1","subject":{"user_id":"u_9","scopes":["fs:read"]},"session_id":"sess_88392","agent":{"id":"example-agent","version":"3"},"tool":{"name":"shell_exec","risk":"high","side_effects":"external"},"arguments":{"command":"rm -rf target"},"context":{"depth":0,"budget_remaining":{"tokens":120000}}}
```

and receives `{decision, reason, rule_id, ttl_s, ask}`, where `decision` is `allow`, `deny` or `ask`, `ttl_s` lets the server reuse the decision for identical requests, and `ask` (`{group_id, quorum}`) names who to ask. An `allow` decision is final: the policy is part of the server's trusted configuration. If the policy service is unreachable the server treats the answer as `ask`, and denies when no approver exists. Arguments reach the policy service already redacted (Section 24). Each decision is audited with its `rule_id`.

An agent never answers `medium` or `high` risk requests, in any routing (Section 8.2.9).

## 14. Background Jobs

### 14.1 Lifecycle

`RUNNING` -> one of `COMPLETED`, `FAILED`, `KILLED`, `TIMED_OUT`.

### 14.2 Events

- `JOB_STARTED`: `job_id`, `tool_call_id`, `command`, `detached` (const true), `started_at`, `limits` (`timeout_s`, `max_output_bytes`).
- `JOB_PROGRESS_DELTA`: `stream`, `delta`, `index` (per job). Carries `job_id` and `turn_id: null`.
- `JOB_FINISHED`: `status` (`COMPLETED`, `FAILED`, `KILLED`, `TIMED_OUT`), `exit_code` (integer or null), `signal` (string or null, for example `SIGTERM`), `summary`, `output_snapshot` (last 16 KiB), `output_truncated` (boolean), `output_bytes_total`, `duration_ms`, `triggered_new_turn` (boolean), `new_turn_id` (string or null), `injected_into_turn_id` (string or null).

### 14.3 Querying jobs

- `JOB_LIST` (`status` filter, `limit`) returns `JOB_LIST_RESULT` with job summaries.
- `JOB_GET` (`job_id`, `from_index`) returns `JOB_STATE`: current status and the retained output from `from_index`, so a reconnecting client can rebuild a log view. Output is retained for `job_output_retention_s`; later requests fail with `ERROR JOB_OUTPUT_EXPIRED`. Jobs of a closed child session follow Section 8.2.12.

### 14.4 Injection into the agent

On termination the server persists a system event in the history, labelled with its provenance (job id, command, status). Behaviour depends on `job_turn_policy` (session option, default `always`):

| Policy | Behaviour |
| --- | --- |
| `always` | Every terminal job state may start a new turn. |
| `on_failure` | Only `FAILED`, `KILLED` and `TIMED_OUT` start a turn. |
| `never` | The event is only recorded and delivered at the next user turn. |

If a turn is already active, the event is injected at the next model-call boundary of that turn (`injected_into_turn_id` set, `triggered_new_turn` false). Otherwise a new turn starts (`triggered_new_turn` true). Job output is untrusted data and MUST be passed to the model as data, never as instructions. Child results and routed approvals use the same mechanism, governed by `child_turn_policy` (Section 8.2.6).

### 14.5 Cancellation and limits

`CANCEL_JOB` sends `SIGTERM`, waits `grace_ms` (default 5000), then `SIGKILL`. The result is `JOB_FINISHED` with `status: KILLED`. Cancelling a finished job is idempotent: the server re-sends the existing `JOB_FINISHED` or answers `ERROR JOB_NOT_RUNNING`. The server enforces `max_concurrent_jobs`, per-job timeout and output cap, returning `SANDBOX_LIMIT_EXCEEDED` when exceeded. Jobs outliving their session's `CLOSED` state MUST be killed.

## 15. Cancellation Semantics

- `CANCEL_TURN` targets `turn_id`; if omitted, it targets the active turn. With no active turn the server answers `ERROR TURN_NOT_FOUND` (`ref` set).
- Cancellation is cooperative and best-effort-fast: the server stops the provider stream, kills synchronous tool processes, then emits `TURN_STATE_CHANGED: CANCELLED`. Partial output already delivered stays in history, marked incomplete.
- Race: if the turn reaches a terminal state before the cancel is processed, the original terminal state stands and the cancel is answered with `ERROR TURN_NOT_ACTIVE`.
- `CANCEL_TURN` does **not** kill detached jobs; use `CANCEL_JOB`.
- Both cancel commands are idempotent.

## 16. Delivery, Ordering and Resume

- **Ordering.** The server assigns `seq` to every server-to-client event, strictly increasing by 1 per session. Clients MUST process in `seq` order and treat a gap as a fault: reconnect with `resume_from_seq`.
- **Acknowledgement.** The client SHOULD send `ACK` (`last_seq`) at least every 100 events or 2 s. The server may discard replay data up to the acknowledged `seq`.
- **Replay.** On attach with `resume_from_seq`, the server replays events from `resume_from_seq + 1`. If they are no longer buffered, `SESSION_ATTACHED.replay.truncated` is true and the server follows with a full `SESSION_STATE`; the client MUST then discard local derived state and rebuild from it (and from `HISTORY_GET` if needed). After a crash, `replay.lost` marks events that were delivered but never made durable (Section 17.3).
- **Idempotency.** The server deduplicates client frames by `id` within a window of at least 10 minutes. A repeated `id` returns the original outcome and MUST NOT repeat side effects. Tool calls issued by the model are not client frames; where they have side effects the server deduplicates them by `tool_call_id` (for delegation tools, Section 8.2.5). Every session, a child included, keeps its own `seq`, `ACK` and replay buffer.
- **Flow control.** The server MAY coalesce deltas of the same stream under load (never reordering). If a client's unacknowledged backlog exceeds `replay_buffer_events`, the server closes with `4008`.
- **History.** `HISTORY_GET` (`before_turn_id`, `limit`) returns `HISTORY_PAGE`. `SESSION_STATE.history_summary` is a bounded view, not the full history.

## 17. Durability and Recovery

A session outlives connections, and a server can fail at any point of a turn. This section defines what a server persists, how ownership moves between nodes, and what clients see after a failure.

### 17.1 Event Log and Ordering Invariants

- Each session has a durable, append-only event log with one writer at a time (Section 17.2).
- The writer assigns `seq`. Values are strictly increasing, never reused, and the durable log has no gaps.
- **Durable before delivery.** Every event that changes session state, or is needed to rebuild it, MUST be durable before it is delivered. Only deltas (`TOKEN_DELTA`, `TOOL_OUTPUT_DELTA`, `JOB_PROGRESS_DELTA`, `OUTPUT_PART_DELTA`) MAY be delivered before they are durable.
- **Reserved blocks.** The writer reserves `seq` ranges durably (blocks of at least 1000) and assigns from them, so a value delivered but not durable is never reissued after a crash.
- **History is the source of truth.** Before a turn's terminal event, the server persists the aggregated output as history items. A delta that was lost never changes the stored history.

### 17.2 Session Ownership and Failover

- A session is owned by one server node at a time through a lease with a fencing token. Every write to the event log carries the token, and the store rejects writes with a stale one.
- A node that loses its lease MUST stop writing at once and close the session's connections with `1001`.
- The next owner loads the durable log, runs recovery (Section 17.3), and accepts attaches. `seq` continues from the end of the highest reserved block.
- Which node a client reaches is a server matter; every node routes to the owner. The client behaves the same on any node: reconnect, `SESSION_ATTACH` with `resume_from_seq`.
- The deduplication table of client frame ids (Section 16) and the tool-call records are part of the durable state and survive a failover.

### 17.3 Recovery after a Crash

On the first attach after a recovery, `SESSION_ATTACHED.replay` can carry `lost`, `{from_seq, to_seq}`: the range of values delivered before the crash but never made durable. A client that sees `lost` MUST discard its local state for the turn that was running and rebuild from `SESSION_STATE` and `HISTORY_GET`.

| State at the crash | Outcome |
| --- | --- |
| A turn was running | The recovery ends it with `TURN_STATE_CHANGED` `FAILED`, `stop_reason: error`, `error.code: INTERNAL`, `error.details.reason: interrupted`, `retryable: true`. Output that was not durable is dropped. |
| A tool call was in flight | The call finishes with `status: ERROR`, `result_summary: interrupted`, and the model sees tool error `TOOL_INTERRUPTED`. It is not re-executed unless it is idempotent and its `retry` policy allows another attempt (Section 13.4). |
| A job was running | If the job runner survived, the owner reattaches and resumes streaming from the last recorded `index`. If the runner was lost, the job finishes `FAILED` with `exit_code: null` and `summary: runner lost`. |
| A child session existed | Children recover independently. The parent then reconciles: a `CHILD_SESSION_UPDATED` is emitted for every state change it missed, and children of a failed turn with `lifetime: turn` are stopped by the usual cascade (Section 8.2.3). |
| An approval or client tool call was pending | It stays pending until its `expires_at`. |
| A notification was undelivered | Its retries continue (Section 9). |
| Uploads and outputs | They are stored once and are durable by definition. |

### 17.4 Delivery Guarantees

- Client frames: at most once per frame `id` (Section 16), also across a failover.
- Tool side effects: at most once by default. At least once, deduplicated by the idempotency key, for idempotent tools with a retry policy. RAEP gives no exactly-once guarantee beyond what a backend does with the key.
- Server events: delivered in `seq` order, at least once across reconnects (a replay can repeat events the client already processed, and clients ignore a `seq` they have seen).

## 18. History Control: Fork, Rewind, Compaction and Snapshots

### 18.1 Fork

`SESSION_FORK` (controller only; scope `session:fork`; capability `fork`) creates a new root session from a point in an existing one. Payload: `from_turn_id` (default: the last terminal turn), `inclusive` (default true), `title`, `agent_id` (default: the source's current agent, and it must be visible to the token), `carry_usage` (default true) and `include` (`{uploads, outputs, workspace}`; the first two default true, `workspace` false).

- `from_turn_id` MUST name a terminal turn, otherwise `ERROR TURN_NOT_ACTIVE`; an unknown turn gives `ERROR TURN_NOT_FOUND`.
- The new session gets a copy of the history up to that turn, without rewound turns. Uploads and outputs the history refers to are copied (copy-on-write is allowed). With `include.workspace` the snapshot of that turn is copied (Section 18.4).
- Jobs, children, pending approvals and the notification configuration are not carried. Results of children stay in the history as text.
- `carry_usage` makes the fork start with the source's `usage_total`, so a fork cannot reset a budget (the same rule as import, Section 11.2).
- The fork is created `SUSPENDED`, with `SESSION_STATE.forked_from` (`session_id`, `turn_id`, `at`). The source is unaffected and can be active. The reply is `SESSION_FORKED` (`session_id`, `from_session_id`, `from_turn_id`, `title`) on the source's stream.

### 18.2 Rewind and Edit

`SESSION_REWIND` (controller only; scope `session:rewind`; capability `rewind`) removes the tail of the conversation from the model's view. Payload: `to_turn_id*`, `inclusive` (default false: keep that turn; true: remove it as well), `cancel_pending` (default false) and `workspace` (`keep`, the default, or `restore`).

- Turns after the point (and the turn itself with `inclusive: true`) are **rewound**: marked `rewound` in the history, excluded from the model context and from `history_summary`, and still readable with `HISTORY_GET` and `include_rewound: true`.
- **Preconditions.** There is no active turn, and no running job or non-terminal child that belongs to a rewound turn. With `cancel_pending: true` the server first cancels the active turn, kills those jobs and stops those children. Otherwise it fails with `ERROR REWIND_BLOCKED` and `details.blocking` listing what stands in the way.
- **Side effects stay.** Effects outside the session are not undone. `HISTORY_REWOUND.side_effects` lists the tool calls of the rewound turns whose `side_effects` is not `none` (`tool_call_id`, `tool_name`, `side_effects`) so a client can warn the user.
- **Workspace.** With `workspace: restore` and the capability `workspace.snapshots`, the sandbox workspace is restored from the snapshot taken at the end of the last kept turn. A missing snapshot fails the request with `ERROR SNAPSHOT_NOT_FOUND`, and nothing is rewound.
- **Budgets.** Usage and budgets are not refunded. Rewound pins are dropped (Section 18.5).
- **Reply.** `HISTORY_REWOUND` (`to_turn_id`, `inclusive`, `rewound_turn_ids`, `side_effects`, `workspace_restored`). Rewinding to a point that is already the end of the conversation is a no-op with an empty list.

**Edit and re-run.** `USER_INPUT.replaces_turn_id` does a `SESSION_REWIND` to that turn with `inclusive: true` and `cancel_pending: false`, then runs the new input as a new turn, in one step. `queue_policy` does not apply. The new `turn_id` MUST be new, and the same preconditions and errors apply.

### 18.3 Compaction

`SESSION_COMPACT` (controller only; capability `compaction`) triggers compaction of the model context. Payload: `strategy` (`summarize` or `truncate`), `keep_last_turns` and `instructions` (at most 1 KiB, guidance for the summary). It fails with `ERROR SESSION_BUSY` while a turn is active. The result is `HISTORY_COMPACTED` with `trigger: manual`; automatic compaction (Section 7.2) uses `trigger: auto`. Stored history is unchanged, and at most one compaction runs at a time.

### 18.4 Workspace Snapshots

With `workspace.snapshots: per_turn` in the agent spec (capability `workspace.snapshots`), the server snapshots the sandbox workspace at the end of every terminal turn. A snapshot is identified by its `turn_id`.

- The server keeps at most `limits.max_snapshots` snapshots, dropping the oldest. A snapshot that would exceed `limits.max_workspace_bytes` is skipped, and `SESSION_STATE.workspace.last_snapshot_turn_id` still names the last good one.
- Snapshots exclude files that match the server's exclusion globs; the defaults are `.env*`, `*.pem`, `*.key` and `*credentials*`. The same exclusions apply to export (Section 11.1) and fork.
- `GET /raep/v1/sessions/{session_id}/snapshots` lists them (`turn_id`, `created_at`, `bytes`), and `GET /raep/v1/sessions/{session_id}/snapshots/{turn_id}` downloads one as a TAR archive. Both need the session authorization of Section 24.
- Snapshots are stored under the same retention and purge rules as the session. They are used by rewind (Section 18.2), fork (Section 18.1) and export (Section 11.1).

### 18.5 Pins

`CONTEXT_PIN` (controller only; capability `compaction`) pins or unpins turns: `turn_ids*` and `pinned` (default true). A pinned turn stays verbatim through compaction and truncation. At most `limits.max_pinned_turns` (default 20) turns can be pinned; beyond that the request fails with `ERROR PAYLOAD_TOO_LARGE`. The reply is `CONTEXT_UPDATED` (`pinned_turn_ids`), and `SESSION_STATE` reports the same list.

## 19. Management API

The management API lets a client list, find, organise and share sessions and discover agents without opening an event channel. The capability is `manage`. All resources use the authentication and error conventions of Section 4.7.

### 19.1 Sessions

| Operation | Request |
| --- | --- |
| Create | `POST /raep/v1/sessions` with `{agent_id*, agent_override, title, labels, options}`; returns a `session_summary`. Accepts `Idempotency-Key`. Scope `session:create`. |
| List | `GET /raep/v1/sessions`. Scope `session:list`. |
| Read | `GET /raep/v1/sessions/{session_id}`; returns a `session_summary`. |
| Update | `PATCH /raep/v1/sessions/{session_id}` with a JSON merge patch of `title`, `labels`, `pinned`, `archived`. Scope `session:manage`. |
| Delete | `DELETE /raep/v1/sessions/{session_id}`; the same as `SESSION_CLOSE` with `purge: true`. Scope `session:manage`. |

**List parameters.** `status`, `agent_id`, `root` (default true: only root sessions; false includes children), `parent_session_id`, `label.<key>=<value>`, `q` (title search, server-defined matching), `archived` (default false), `updated_since`, `order` (`updated_desc`, the default, or `created_desc`), `cursor` and `limit` (at most 100). The result is `{sessions, next_cursor}`, and it contains only sessions the token may attach to.

**`session_summary`.** `session_id`, `title`, `title_source` (`user` or `auto`), `labels`, `pinned`, `archived`, `status`, `agent` (`{id, version}`), `parent`, `forked_from`, `created_at`, `updated_at`, `last_activity_at`, `turn_count`, `usage_total` and `has_pending` (an approval, question or client tool call waits).

**Labels.** At most 16 pairs; a key has at most 64 characters, a value at most 256. A server MAY generate a title after the first turn when none is set, with `title_source: auto`; a user-set title is never overwritten. Attached clients receive `SESSION_UPDATED` (`title`, `labels`, `pinned`, `archived`, `updated_at`) when these change.

### 19.2 Agents

| Operation | Request |
| --- | --- |
| List | `GET /raep/v1/agents` with `q`, `cursor`, `limit`; returns `{agents, next_cursor}` of `agent_summary` objects. Scope `agent:list`. |
| Read | `GET /raep/v1/agents/{agent_id}`; returns an `agent_summary` and `latest_version`. |
| Versions | `GET /raep/v1/agents/{agent_id}/versions`; returns `{versions}`, each `{version, created_at, deprecated}`. |

Agents are defined out of band by the server (Section 7); this API is read-only and lists only agents visible to the token. It never returns credentials, endpoints or `system_prompt`.

## 20. Headless Runs, Schedules and Triggers

Not every caller keeps a connection open. Runs, schedules and triggers start agent work over plain HTTP. Each of them creates an ordinary session, so every other part of this document applies to it.

### 20.1 Runs

`POST /raep/v1/runs` (scope `run:create`; capability `runs`) starts one turn and optionally waits for its result. The body is a `run_request`:

| Field | Meaning |
| --- | --- |
| `agent_id*` | The agent. |
| `session_id` | An existing session to continue. Omit it to create one. |
| `input*` | `{prompt}` or `{content}` as in Section 12.4. |
| `response_format` | As in Section 12.6. |
| `overrides`, `options` | As in `USER_INPUT` and `SESSION_ATTACH`. |
| `wait_s` | Seconds to wait for the result, 0 to 120. Default 0. |
| `on_approval` | `deny` (default), `wait` or `fail` (below). |
| `ephemeral` | When true, the session is purged after the run and its retention. |
| `title`, `labels`, `metadata` | As for sessions. |

The response is a `run`: `run_id`, `session_id`, `turn_id`, `status` (`queued`, `running`, `awaiting_approval`, `completed`, `failed`, `cancelled`), `output` (`text`, `parts`, and `structured` when a `response_format` was used), `usage`, `error`, `created_at`, `finished_at`. It is `200` when the run finished within `wait_s` and `202` with a `Location` header otherwise. `GET /raep/v1/runs/{run_id}` returns the run (with `wait_s` as a long poll), and `POST /raep/v1/runs/{run_id}/cancel` cancels it. A client can follow a run live by attaching to its session as an observer. The `Idempotency-Key` header deduplicates `POST` for at least 24 hours; the same key with a different body fails with `ERROR IDEMPOTENCY_CONFLICT`.

**Approvals without a controller.** `deny` finishes calls that need approval with `status: DENIED`. `wait` sets the run to `awaiting_approval` until someone answers through the approvals resource (Section 13.6) or the request expires. `fail` ends the run `failed` with `error.code: APPROVAL_REQUIRED`. Client tools (Section 13.5) are not available in runs. Runs count against the same quotas as sessions (Section 25.4) and are kept for `limits.run_retention_s` (default 604800).

### 20.2 Schedules

`POST /raep/v1/schedules` (scope `schedule:manage`; capability `schedules`) starts runs on a timer. The body is a `schedule_request`: `agent_id*`, `input*`, `trigger*`, `session_mode` (`new`, the default, or `reuse` with `session_id`), `overlap`, `on_approval`, `catchup`, `notify`, `enabled`, `max_runs`, `expires_at`, `disable_after_failures` (default 5), `title` and `labels`.

- `trigger` is `{type: "cron", expression, timezone}` (five fields, evaluated in the time zone), `{type: "interval", every_s}` or `{type: "once", at}`.
- `overlap` decides what happens when the previous run is still active: `skip` (default), `queue` or `cancel_previous`. `catchup` decides what happens to fires missed while the server was down: `none` (default) or `one`.
- Each fire creates a run with `metadata.schedule_id` and `origin.kind: schedule`.
- The `schedule` resource reports `schedule_id`, `enabled`, `next_run_at`, `last_run` (`{run_id, status, at}`) and `consecutive_failures`. A schedule is disabled after `disable_after_failures` consecutive failed runs.
- `GET /raep/v1/schedules`, `GET`, `PATCH` and `DELETE /raep/v1/schedules/{schedule_id}` manage it.
- A schedule runs with the scopes of its creator as they were when the schedule was created, narrowed. They are checked again at every fire, and the schedule is disabled if the creator no longer holds them.

### 20.3 Triggers

A trigger starts a run when an external system calls it. `POST /raep/v1/triggers` (scope `trigger:manage`; capability `schedules`) creates one from `{agent_id*, input_template*, session_mode, on_approval, title, labels}`. The response includes a `secret` (at least 32 characters), returned once. `GET /raep/v1/triggers` lists triggers and `DELETE /raep/v1/triggers/{trigger_id}` removes one.

The external system calls `POST /raep/v1/triggers/{trigger_id}/fire` with a JSON body of at most 64 KiB, signed with the scheme of Section 9 (`RAEP-Signature`, HMAC-SHA256, 300 s tolerance). An `Idempotency-Key` or `X-Event-Id` header deduplicates deliveries.

`input_template` is text with `{{payload.<pointer>}}` placeholders, where `<pointer>` is a dot path into the body; no expression or code is evaluated. The rendered text is untrusted external data: the server inserts it as a turn whose content is labelled with `origin.kind: trigger` and the `trigger_id`, and nothing in the payload can change scopes, budgets or approval policy. Fires are rate-limited per trigger.

## 21. Collaboration and Presence

Several people can work in one session. The capability is `collaboration`.

### 21.1 Sharing and Roles

A session has an access list. Each entry is a principal (a user or a group) with a role:

| Role | Can |
| --- | --- |
| `viewer` | Attach as `observer`, read history, see presence. |
| `editor` | Everything a `viewer` can, plus attach as `controller`, send input, cancel, answer approvals and client tool calls, fork and rewind. |
| `owner` | Everything an `editor` can, plus configure, share, export, close and delete. |

`GET /raep/v1/sessions/{session_id}/shares` lists the entries, `PUT /raep/v1/sessions/{session_id}/shares/{principal}` with `{role}` sets one, and `DELETE` removes one. The scope is `session:share`. Removing an entry or lowering its role takes effect on live attachments at once: the server demotes the attachment or closes it with `4003`. A creator of a session is its first `owner`.

### 21.2 Several Controllers

A session accepts one `controller` unless it enables `collaboration.controllers: multiple` in its options (at most `limits.max_participants`, default 8). With `multiple`:

- Every attached `editor` can attach as a controller. `takeover` has no meaning and the close code `4009` is not used.
- Each input is attributed: `USER_INPUT_ACCEPTED` and the history item carry `origin` with `kind: user`, `user_id` and `display_name`. `queue_policy` applies across all controllers; `collaboration.interrupt` (`any`, the default, or `owner`) says who may use `interrupt`.
- Any controller can send `CANCEL_TURN` and `CANCEL_JOB`.
- Any eligible controller can answer an approval; the first valid answer decides unless a quorum applies (Section 13.6). Everyone then sees `APPROVAL_RESOLVED`.
- A client tool call goes to one controller (Section 13.5).

### 21.3 Presence

`PRESENCE_UPDATE` (`status*`: `active`, `idle` or `typing`) tells the others what the sender is doing. The server answers every change in the participant set with `PRESENCE` (`participants[]`, each `{user_id, display_name, role, mode, status, connected_at, last_seen_at}`) to all attached clients, at most once per second. A client sees only participants it is allowed to see (viewers see names only if server policy says so). `SESSION_STATE.participants` reports the same list.

## 22. Interoperability and Federation

RAEP is one protocol among others. This section says how it meets MCP, remote agents and agent-to-agent protocols.

### 22.1 MCP Mapping

- **Agent as MCP client.** `mcp_servers` in the agent spec (Section 7) connects MCP servers. Their tools join the tool set under the name `mcp.<server>.<tool>`. The descriptor (Section 13.4) is derived from the MCP tool: `input_schema` and `output_schema` as declared, `risk` `low` for a read-only tool, `high` for a destructive one, `medium` otherwise, `idempotent` from the idempotent hint, and `side_effects: none` for a read-only tool. Scopes are `mcp:<server>:<action>` (Section 25.2).
- **Sampling and elicitation.** An MCP server's sampling request is routed to the agent's provider and counts against the budgets. An elicitation request is shown to a controller as an approval-style question; it is never answered automatically.
- **Agent as MCP server.** A server MAY expose an agent as an MCP tool `run_agent` whose arguments are a `run_request` (Section 20.1) and whose result is a `run`. The call is made with the token of the MCP client, subject to every rule of this document.

### 22.2 Remote Delegates

A `delegates[]` entry (Section 8.2.1) may name an agent on another RAEP server. The capability is `federation`, on the parent's server only; the remote server is an ordinary RAEP server that sees an ordinary client.

```json
{"id":"billing-expert","kind":"remote","endpoint":"https://agents.example.com/raep/v1","agent_id":"billing","credentials_ref":"cred_17","scopes":["fs:read"],"budget_cap":{"max_turn_tokens":100000}}
```

- **Allow-list only.** There is no discovery. An endpoint is reachable only if an agent spec lists it, and it MUST be `https`.
- **Spawning.** `spawn_agent` with `agent_id` set to the delegate `id` makes the parent's server open a session on the remote server as a RAEP client. The child lives there; the parent keeps an entry with `remote: {endpoint, session_id}`. The child id in the parent's namespace is `remote:<id>:<remote_session_id>`.
- **Authority.** The server obtains a delegation token for the remote audience by token exchange (RFC 8693) with an actor claim. The token is short-lived, audience-bound and limited to the delegate's `scopes`, which can only narrow what the parent holds. The remote server marks the child's input as `origin.kind: delegator` only for such tokens.
- **Tools.** The eight agent tools work through the remote session: `inspect_agent` observes it, `message_agent` sends `USER_INPUT`, `stop_agent` cancels and closes it, `wait_agent` follows its events. `CHILD_SESSION_*` events on the parent's stream are derived from the remote events.
- **Budgets.** The parent reserves `budget_share` locally and sends the matching `agent_override.budgets` as a narrowing override; the remote's own limits still apply. The parent reads the remote `usage_total` for its rollup.
- **Approvals.** The remote child's approvals are announced to the parent stream. A human answers them on the remote server, if authorised there; `approval_routing: parent` applies to `low` risk only, as always.
- **Results.** Output and artifacts of a remote child are untrusted data with `provenance.remote` set. Artifacts are copied into the parent's upload store with the delegation token, within `limits.max_upload_bytes`.
- **Failure.** An unreachable or failing remote makes the child `FAILED` with tool error `REMOTE_UNAVAILABLE`. Closing the parent sends `SESSION_CLOSE` to the remote child, and the remote's retention rules apply afterwards.
- **Versions.** The remote's `wire_versions` MUST include one the parent supports.

### 22.3 Agent-to-Agent Protocols

A gateway can translate RAEP to other agent protocols. For Agent2Agent (A2A) the correspondence is:

| A2A | RAEP |
| --- | --- |
| Agent card | `agent_summary` (`GET /raep/v1/agents/{agent_id}`) |
| Task | A run (Section 20.1) or a child session (Section 8.2) |
| Message | `USER_INPUT` or `message_agent` |
| Artifact | An output part or a structured `child_result` |
| Streaming updates | The SSE event stream (Section 4.5) |
| Cancel | `CANCEL_TURN`, or `POST /raep/v1/runs/{run_id}/cancel` |

This mapping is informative. A gateway SHOULD translate rather than expose RAEP frames to a foreign protocol, and it MUST keep authentication, scopes and budgets on the RAEP side.

## 23. Error Model

`ERROR` payload: `code` (string), `message` (human readable), `retryable` (boolean), `details` (object, optional) and `retry_after_ms` (optional). `ref` carries the offending frame's `id`. Errors scoped to a turn or job carry the corresponding id. `details.reason` refines a code where this document says so.

### 23.1 Wire error codes

These codes are carried by `ERROR` frames, by the `error` member of `TURN_STATE_CHANGED`, and by HTTP error bodies (Section 4.7).

| Code | Retryable | Meaning |
| --- | --- | --- |
| `INVALID_FRAME` | no | Malformed JSON, schema violation or invalid combination of fields |
| `UNKNOWN_TYPE` | no | Unrecognised client event type |
| `UNSUPPORTED_VERSION` | no | Wire version not supported |
| `CAPABILITY_REQUIRED` | no | A required capability is missing |
| `UNAUTHENTICATED` | no | Missing, invalid or expired credentials |
| `FORBIDDEN` | no | Authenticated, but not allowed on this session or in this role |
| `SCOPE_DENIED` | no | The token or agent lacks a named scope (`details.scope`) |
| `SESSION_NOT_FOUND` | no | Session missing, or purged |
| `SESSION_CLOSED` | no | Session closed |
| `SESSION_BUSY` | yes | Turn active, queue full, controller already attached, or subtree active on export |
| `DUPLICATE_TURN` | no | `turn_id` reused with different content |
| `TURN_NOT_FOUND` | no | Cancel target does not exist |
| `TURN_NOT_ACTIVE` | no | Cancel target already finished |
| `JOB_NOT_FOUND` | no | Unknown job |
| `JOB_NOT_RUNNING` | no | Job already finished |
| `JOB_OUTPUT_EXPIRED` | no | Job output past retention |
| `UPLOAD_NOT_FOUND` | no | Unknown, expired or foreign `upload_id` or `output_id` |
| `PAYLOAD_TOO_LARGE` | no | Prompt, frame, upload or bundle over a limit |
| `UNSUPPORTED_MODALITY` | no | Input or requested output modality not supported (`details.supported`) |
| `UNSUPPORTED_MIME_TYPE` | no | MIME type not accepted |
| `AGENT_NOT_FOUND` | no | Unknown agent or agent version |
| `AGENT_OVERRIDE_DENIED` | no | Override of a field not listed in `overridable`, or an attempt to widen scopes, tools or budgets |
| `BUDGET_EXCEEDED` | no | Session or turn budget exhausted (`details.metric`) |
| `RATE_LIMITED` | yes | Client rate limit |
| `PROVIDER_ERROR` | yes | Upstream LLM failure |
| `PROVIDER_RATE_LIMITED` | yes | Upstream LLM rate limit |
| `SANDBOX_LIMIT_EXCEEDED` | no | CPU, memory, time, output or job-count cap |
| `REPLAY_UNAVAILABLE` | no | Resume window lost |
| `BUNDLE_INVALID` | no | Bundle corrupt: hash mismatch, missing file or malformed manifest |
| `BUNDLE_SIGNATURE_INVALID` | no | Signature missing where required, invalid or from an untrusted issuer |
| `BUNDLE_VERSION_UNSUPPORTED` | no | Unknown `bundle_version` |
| `IMPORT_NOT_FOUND` | no | Unknown or expired `import_id` or `export_id` |
| `NOT_FOUND` | no | Unknown or foreign HTTP resource (`details.resource`) |
| `IDEMPOTENCY_CONFLICT` | no | An `Idempotency-Key` was reused with a different body |
| `QUOTA_EXCEEDED` | yes | A tenant quota is exhausted (`details.dimension`, `retry_after_ms`) |
| `APPROVAL_REQUIRED` | no | A run needed approval and `on_approval` was `fail` |
| `APPROVAL_NOT_PENDING` | no | The approval is already resolved |
| `TOOL_CALL_NOT_PENDING` | no | A client tool result for a call that is not pending |
| `REWIND_BLOCKED` | yes | Active work stands in the way of a rewind (`details.blocking`) |
| `SNAPSHOT_NOT_FOUND` | no | No workspace snapshot for the requested turn |
| `OUTPUT_SCHEMA_INVALID` | no | The turn's output failed its `response_format` schema (`details.paths`) |
| `INTERNAL` | yes | Server fault |
| `UNAVAILABLE` | yes | Server overloaded or draining (`retry_after_ms`) |

Provider secrets, raw provider errors and stack traces MUST NOT appear in `message` or `details`.

### 23.2 Tool error codes

A failed tool call is not a protocol error: the turn continues and the model receives the failure in the tool result, while `TOOL_CALL_FINISHED` reports `status: ERROR` or `DENIED`. These codes are carried in the tool result and are not `ERROR` frames. A code that is also a wire code (`AGENT_NOT_FOUND`, `BUDGET_EXCEEDED`, `SCOPE_DENIED`, `SANDBOX_LIMIT_EXCEEDED`, `OUTPUT_SCHEMA_INVALID`, `APPROVAL_NOT_PENDING`) has the same meaning in both forms.

| Code | Meaning |
| --- | --- |
| `TOOL_ERROR` | The tool failed |
| `TOOL_DENIED` | Approval was denied or timed out |
| `SCOPE_DENIED` | The agent lacks the scope the tool needs |
| `BUDGET_EXCEEDED` | A budget or delegation limit was reached (`details.metric`: `child_depth`, `children`, `concurrent_children`, `descendants`, `spawns_per_turn` or `budget_share`) |
| `SANDBOX_LIMIT_EXCEEDED` | A sandbox cap was reached |
| `AGENT_NOT_FOUND` | Unknown agent id |
| `HANDOFF_NOT_ALLOWED` | The target is not in `handoff_targets`, or scopes forbid it |
| `DELEGATE_NOT_ALLOWED` | The agent is not in `delegates` |
| `CHILD_NOT_FOUND` | Unknown, foreign or expired child session (`details.reason: expired` after retention) |
| `CHILD_NOT_ACTIVE` | The child is in a terminal state |
| `DELEGATOR_SUSPENDED` | A human holds the child (Section 8.2.11) |
| `OUTPUT_SCHEMA_INVALID` | The child's output failed its schema (Section 8.2.6) |
| `APPROVAL_TIMEOUT` | A routed approval expired (Section 8.2.9) |
| `APPROVAL_NOT_PENDING` | The approval was already resolved, or the decision differs from the recorded one |
| `RISK_TOO_HIGH` | A parent agent tried to approve a `medium` or `high` risk call |
| `DELEGATOR_TIMEOUT` | The delegator did not answer `ask_delegator` in time |
| `TOOL_TIMEOUT` | The attempt exceeded `timeout_s` |
| `TOOL_INTERRUPTED` | A crash interrupted the call (Section 17.3) |
| `REMOTE_UNAVAILABLE` | A remote delegate is unreachable or failed (Section 22.2) |
| `INVALID_ARGUMENT` | A tool argument is invalid |

## 24. Security Requirements

1. **Transport:** TLS only; `Origin` allow-list; short-lived single-use browser tickets.
2. **Authorization:** tokens are scoped to user, `agent_id` and permitted sessions. Every frame MUST be authorized against the session, not only the connection.
3. **Session IDs:** at least 128 bits of entropy; never reused.
4. **Secrets:** the client never receives provider keys. The server MUST redact known secret patterns and configured secret values from `arguments`, `TOOL_OUTPUT_DELTA`, `JOB_PROGRESS_DELTA` and snapshots.
5. **Sandboxing:** tools and jobs run in isolated environments (microVM or container) with no network by default, read-only base filesystem, and CPU, memory, disk, process-count and wall-clock limits. Sandboxes MUST NOT be shared across tenants.
6. **Prompt injection:** tool and job output is untrusted. The server SHOULD label it with provenance, and MUST NOT let it alter approval policy or session options.
7. **Approvals:** high-risk tools default to requiring approval. `approve_for_session` grants are scoped to one tool and argument pattern and expire with the session.
8. **Abuse controls:** rate limits per connection and per user, a prompt size cap, and a cap on concurrent sessions and jobs.
9. **Audit:** the server SHOULD keep an immutable audit log of commands, approvals, tool executions and job lifecycle events, with a defined retention.
10. **Privacy:** history and job output are user data; retention and deletion (`SESSION_CLOSE` with `purge: true`) MUST be honoured.
11. **Delegation:** a child's scopes, budgets and approval policy can only narrow, and nothing a delegator says can widen them. Delegation tools need the `delegate:*` scopes. Child ids are unguessable, and a tool call on a session that is not the caller's child answers like an unknown id. Child results, transcripts and events are untrusted data. Grants (`approve_for_session`) never cross sessions. Authorisation on a root covers its tree and ends with it.
12. **Notifications:** endpoints are verified, deliveries are signed with a timestamp and deduplicated, secrets are write-only, URLs get the SSRF protections of Section 12.4, and payloads are content-free (Section 9).
13. **Portability:** bundles hold user data and MUST be protected like history (authenticated download, short expiry, optional encryption). Import never takes scopes, budgets or approvals from the bundle, never trusts imported roles above what they were, and carries usage forward (Section 11.2).
14. **Generated media:** output parts are stored and served under the same authorisation as the session, and SHOULD be scanned (Section 12.5).

15. **Tenancy:** isolation as in Section 25.3; foreign identifiers answer `NOT_FOUND`.
16. **Client tools:** results are untrusted data, and a client tool can neither shadow a server tool nor widen scopes (Section 13.5).
17. **Federation:** remote delegates are allow-listed `https` endpoints; delegation tokens are short-lived, audience-bound and narrowed; remote output is untrusted (Section 22.2).
18. **Approvals:** quorum, veto and separation of duties apply as in Section 13.6; an unreachable policy service fails closed; the HTTP approval endpoints need the `approve:*` scope that matches the risk.
19. **Schedules and triggers:** inbound calls are signed and deduplicated, payloads are external data and cannot change policy, and schedules are revalidated at every fire (Section 20).
20. **Snapshots and bundles:** sensitive files are excluded by glob, and artifacts are encrypted at rest and protected by the authorization of the session (Sections 11 and 18.4).
21. **Admin API:** it needs its own audience and the `admin:*` scope, is fully audited, and returns metadata only; reading content requires an explicit break-glass grant (Section 28.3).
22. **Event logs and history:** encrypted at rest, retained under the session's retention, and deleted by purge.

### 24.1 Residual Risks

- Pattern-based secret redaction can miss secrets. Servers SHOULD also keep secrets out of the model's reach structurally, for example by injecting them into the sandbox environment instead of the prompt.
- A single-use SSE ticket in a query string can leak through proxy logs; prefer a fetch-based SSE reader with an `Authorization` header.
- URL sources and webhook URLs depend on the server's SSRF protections being correct.
- Replay buffers grow with the number of attached observers; servers need a per-session cap.
- A webhook receiver that does not verify signatures or deduplicate can be tricked or can act twice; the protocol can only offer the tools.
- An unsigned bundle can be forged. Servers that import bundles SHOULD require signatures from trusted issuers.
- Structured output is validated for shape, not for truth; a parent MUST still treat `output` as untrusted data.
- Generated media safety depends on the server's content scanning.
- A remote delegate's server is outside this server's control: its behaviour, retention and enforcement of budgets are those of its operator (Section 22.2).

### 24.2 Threat Model

| Threat | Asset | Mitigation |
| --- | --- | --- |
| A stolen bearer token | Sessions and tools of one user | Short lifetimes, audience check, refresh (Section 4.6), single-use browser tickets (Section 4.1), scope narrowing (Section 25.2) |
| Cross-tenant access by id guessing | Another tenant's data | Unguessable ids, `NOT_FOUND` for foreign resources, tenant-keyed caches (Section 25.3) |
| Prompt injection through tool, job, child, client-tool, trigger or retrieved content | Agent behaviour, secrets, side effects | All such content is data with provenance; it cannot change scopes, budgets or approvals (Sections 8.2.10, 12.4, 13.5, 20.3) |
| Server-side request forgery through URLs | Internal network | HTTPS-only fetches, no private or link-local addresses, DNS pinning, allow-list for delegates and webhooks (Sections 9, 12.4, 22.2) |
| Sandbox escape or resource abuse | Host and neighbours | Isolated sandboxes, no network by default, hard CPU, memory, disk, process and time limits (Section 24) |
| Event replay or reordering | Client state | `seq`, durable-before-delivery, fencing (Sections 16 and 17) |
| Oversized or flood frames | Availability | Frame limits, command rate limits, overload shedding (Sections 4.2, 25.5, 25.6) |
| Forged webhooks or trigger calls | Agent actions | HMAC signatures, timestamp tolerance, deduplication (Sections 9 and 20.3) |
| Forged or tampered bundles | Imported sessions | Hashes, signatures from trusted issuers, import preview (Section 11.2) |
| Spawn bombs and cost runaway | Budgets, quotas | Delegation limits, budget shares, quotas, `on_exceed` (Sections 7, 8.2.3, 25.4) |
| A colluding or confused approver | High-risk actions | Quorum, separation of duties, `approve:*` scopes, audit (Section 13.6) |
| Secret exfiltration through output | Credentials | Redaction, structural secret handling, exclusion globs (Sections 18.1 and 24.1) |
| Operator over-reach | Tenant content | Admin API metadata only, break-glass for content, audit (Section 28.3) |

## 25. Tenancy, Identity, Quotas and Rate Limits

### 25.1 Identity and Tokens

RAEP treats the bearer token as opaque, but the server MUST derive from it: `tenant_id`, a `subject` (user or service), `scopes[]`, an expiry, and optionally the agents (`agents[]`) the token may use. A server that uses JWT access tokens (RFC 9068) SHOULD map these claims: `iss`, `sub`, `aud` (`raep`), `exp`, `iat`, `jti`, `tenant`, `scope` (space-separated), `agents` and `act` (the actor of a delegation). The audience MUST be checked. Tokens SHOULD live at most one hour and are renewed with `AUTH_REFRESH` (Section 4.6).

### 25.2 Scopes and Roles

Scopes are strings. The same namespace names what a token may do and what an agent may do; effective authority is always the intersection (Section 7).

| Scope | Allows |
| --- | --- |
| `session:create`, `session:list`, `session:manage`, `session:share` | The management API (Sections 19.1 and 21.1) |
| `session:attach`, `session:control` | Attach as `observer`, attach as `controller` |
| `session:fork`, `session:rewind` | Sections 18.1 and 18.2 |
| `session:export`, `session:import` | Section 11 |
| `agent:list` | Section 19.2 |
| `run:create`, `schedule:manage`, `trigger:manage` | Section 20 |
| `approve:low`, `approve:medium`, `approve:high` | Answer approvals up to that risk (Section 13.6) |
| `delegate:spawn`, `delegate:inspect`, `delegate:message`, `delegate:stop` | The agent tools of Section 8.2.5; `delegate:*` grants all four |
| `fs:read`, `fs:write`, `net:egress`, `shell:exec`, `jobs:spawn` | Tool authority in the sandbox |
| `mcp:<server>:<action>` | Tools of an MCP server |
| `context:read`, `context:write` | Context sources (Section 7.2) |
| `notify:configure` | Section 9 |
| `quota:read` | Section 25.4 |
| `admin:*`, `admin:content` | The admin API, and break-glass content access (Section 28.3) |

Roles are bundles of scopes and are server configuration. A typical mapping:

| Role | Scopes |
| --- | --- |
| viewer | `session:list`, `session:attach`, `agent:list` |
| editor | viewer plus `session:create`, `session:control`, `session:fork`, `session:rewind`, `run:create`, `approve:low` |
| owner | editor plus `session:manage`, `session:share`, `session:export`, `notify:configure` |
| approver | `approve:medium`, `approve:high` and `session:attach` |
| admin | `admin:*` (on a separate audience) |

### 25.3 Tenant Isolation

- Sessions, uploads, outputs, snapshots, bundles, logs, caches and sandboxes belong to one tenant and are never shared. Caches that depend on content, such as provider prompt caches, are keyed by tenant.
- Identifiers are unguessable and are not interpreted across tenants.
- A request for another tenant's resource gets `NOT_FOUND`, never `FORBIDDEN`, so identifiers cannot be probed.
- Encryption keys SHOULD be per tenant where the deployment supports it.
- Audit logs carry the tenant, and an operator's view is filtered by it.

### 25.4 Quotas

A tenant has quotas. The dimensions are `sessions` (concurrently active), `turns` (concurrently running), `jobs` (concurrently running), `tokens` and `cost` (per period), `storage_bytes`, `upload_bytes`, `runs_per_minute`, `schedules` and `webhook_deliveries` (per period).

`GET /raep/v1/quota` (scope `quota:read`; capability `quota`) returns a `quota`: `tenant`, `period` (`{start, end}`) and `dimensions[]` of `{name, limit, used, remaining, resets_at}`. The server sends `QUOTA_WARNING` (`dimension`, `used`, `limit`, `threshold`, `resets_at`) when use crosses a threshold (default 80%). When a quota is exhausted, the call fails with `ERROR QUOTA_EXCEEDED`, `details.dimension` and `retry_after_ms`. Quotas and the budgets of the agent both apply, and the stricter one wins.

### 25.5 Rate Limits

- **Commands.** A connection is limited to `limits.max_commands_per_s` (default 30, with a burst of twice that). Beyond it, the server answers `ERROR RATE_LIMITED` with `retry_after_ms`; repeated violations close the connection with `1008`.
- **HTTP.** The server sets `RateLimit-Limit`, `RateLimit-Remaining` and `RateLimit-Reset`, and `Retry-After` on `429` and `503`.
- **Control frames** (`ACK`, `CANCEL_TURN`, `CANCEL_JOB`, `TOOL_APPROVAL_RESPONSE`, `CLIENT_TOOL_RESULT`, `AUTH_REFRESH`) are always admitted, within a higher cap, so that a client can stop work and keep a connection alive under load.

### 25.6 Overload

When the server is overloaded it sheds load in this order: new sessions, runs, schedule fires and trigger fires (`UNAVAILABLE` with `retry_after_ms`, HTTP `503` with `Retry-After`, close code `1013` on a WebSocket attach); then new turns on idle sessions; never a running turn, except at a drain deadline (Section 28.4). Control frames are always processed. Admission SHOULD be fair across tenants.

## 26. Versioning, Extensibility and Registries

### 26.1 Versions

- **Wire version** (`version` field, `1.0`): identifies the envelope and the semantics of this document, and changes only on incompatible changes. Servers list supported versions in `HELLO`. A client whose version is unsupported is closed with `4426`. While the document version is below 1.0.0, wire version `1.0` is provisional. It becomes stable, and the document becomes 1.0.0, when the conformance suite (Section 31) passes in at least two independent server implementations and two independent client implementations.
- **Document version** (`0.1.0`): semantic version of this specification. Additive changes (new optional fields, new event types gated by a capability) do not change the wire version.
- **Bundle version**: the export bundle format has its own `bundle_version` (Section 11).
- **Schema `$id`** follows the wire major: `https://raep.vyde.dev/v1/`.

### 26.2 Extensions

Extension event types MUST be prefixed `X_<NS>_`, where NS is a namespace the implementer chooses. Capability names, error codes and tool names defined by an extension use the same prefix. Extra envelope data goes in `ext`. Receivers ignore unknown `ext` keys and unknown payload fields. New standard types require a capability flag so older peers can opt out.

Receivers MUST treat an unknown value of an enumeration in a server event as an opaque, non-fatal value: a client shows an unknown `state` as received and never fails on it.

### 26.3 Deprecation

A field, event, capability, error code or enumeration value is deprecated in three steps. First, the server lists it in `HELLO.deprecations` (`[{name, kind, replacement, sunset_at}]`, and `X-RAEP-Deprecated` on HTTP resources). Second, it keeps working for at least six months while the document version is below 1.0.0 and at least twelve months afterwards. Third, it is removed only with a new wire version.

### 26.4 Registries

Six registries hold the names that implementations share: capabilities (Section 6.1), limits (6.2), error codes (23), event types (27), scopes (25.2) and extension namespaces. A name enters a registry by a revision of this specification, or, for extension namespaces, by registration with the maintainers; a namespace is a short identifier that the registrant owns, for example one derived from its domain. `GET /raep/v1/info` reports the registry versions a server implements (Section 28.2).

## 27. Message Catalog

Required fields are marked with an asterisk. Normative schemas are in Appendix B. Every frame uses the envelope of Section 5.

### 27.1 Client to server

| Type | Payload |
| --- | --- |
| `SESSION_ATTACH` | `agent_id*`, `agent_override`, `create_if_missing`, `mode`, `takeover`, `resume_from_seq`, `capabilities`, `client_info`, `options` |
| `SESSION_DETACH` | `reason` |
| `SESSION_CLOSE` | `reason`, `purge` |
| `SESSION_CONFIGURE` | `agent_id`, `agent_override`, `options`, `apply` |
| `SESSION_EXPORT` | `include`, `quiesce`, `wait_timeout_s`, `encrypt` |
| `SESSION_FORK` | `from_turn_id`, `inclusive`, `title`, `agent_id`, `carry_usage`, `include` |
| `SESSION_REWIND` | `to_turn_id*`, `inclusive`, `cancel_pending`, `workspace` |
| `SESSION_COMPACT` | `strategy`, `keep_last_turns`, `instructions` |
| `CONTEXT_PIN` | `turn_ids*`, `pinned` |
| `USER_INPUT` | `prompt` or `content` (exactly one), `queue_policy`, `overrides`, `output`, `response_format`, `replaces_turn_id`, `metadata`; the envelope `turn_id` is required |
| `INPUT_STREAM_START` | `stream_id*`, `type*`, `mime_type*`, `params`, `turn_detection` |
| `INPUT_STREAM_DATA` | `stream_id*`, `index*`, `data*`, `timestamp_ms` |
| `INPUT_STREAM_END` | `stream_id*`, `commit` |
| `CANCEL_TURN` | `reason` |
| `CANCEL_JOB` | `job_id*`, `reason`, `grace_ms` |
| `TOOL_APPROVAL_RESPONSE` | `tool_call_id` or `approval_id` (at least one), `decision*`, `reason` |
| `CLIENT_TOOL_RESULT` | `tool_call_id*`, `status*`, `content`, `result_summary` |
| `JOB_LIST` | `status`, `limit` |
| `JOB_GET` | `job_id*`, `from_index` |
| `HISTORY_GET` | `before_turn_id`, `limit`, `include_rewound` |
| `ACK` | `last_seq*` |
| `AUTH_REFRESH` | `token*` |
| `DELEGATOR_CONTROL` | `action*` (`suspend`, `release`), `reason` |
| `PRESENCE_UPDATE` | `status*` |

### 27.2 Server to client

| Type | Payload |
| --- | --- |
| `HELLO` | `server_version*`, `wire_versions*`, `capabilities*`, `limits*`, `heartbeat_s`, `draining`, `deprecations` |
| `SESSION_ATTACHED` | `created*`, `mode*`, `last_seq*`, `replay`, `negotiated_capabilities`, `agent` |
| `SESSION_STATE` | `status*`, `history_summary*`, `title`, `labels`, `pinned`, `archived`, `usage_total`, `usage_rollup`, `budget_remaining`, `parent`, `children`, `forked_from`, `pinned_turn_ids`, `participants`, `workspace`, `delegator_suspended`, `notify`, `imported_from` |
| `SESSION_CLOSED` | `reason` |
| `SESSION_CONFIGURED` | `agent*`, `applied` |
| `SESSION_UPDATED` | `title`, `labels`, `pinned`, `archived`, `updated_at*` |
| `SESSION_FORKED` | `session_id*`, `from_session_id*`, `from_turn_id*`, `title` |
| `SESSION_EXPORT_READY` | `export_id*`, `download_url*`, `bytes*`, `sha256*`, `expires_at*`, `sessions`, `encrypted`, `signed` |
| `AGENT_CHANGED` | `from_agent*`, `to_agent*` (each `{id, version}`), `reason`, `effective` |
| `USER_INPUT_ACCEPTED` | `turn_id*`, `origin*`, `content*`, `position`, `replaces_turn_id` |
| `TURN_STATE_CHANGED` | `state*`, `stop_reason`, `usage`, `usage_rollup`, `output`, `output_schema_valid`, `error` |
| `TOKEN_DELTA` | `delta*`, `index*`, `channel`, `kind` |
| `PROVIDER_RETRY` | `attempt*`, `reason*`, `provider*`, `fallback*`, `discard_from_index` |
| `TOOL_CALL_STARTED` | `tool_call_id*`, `tool_name*`, `arguments*`, `requires_approval*`, `executor`, `attempt`, `timeout_s` |
| `TOOL_OUTPUT_DELTA` | `tool_call_id*`, `tool_name*`, `stream*`, `delta*`, `index*` |
| `TOOL_CALL_FINISHED` | `tool_call_id*`, `status*`, `exit_code`, `duration_ms`, `attempts`, `result_summary`, `result_truncated` |
| `TOOL_APPROVAL_REQUEST` | `approval_id*`, `tool_call_id*`, `tool_name*`, `arguments*`, `risk*`, `expires_at*`, `approvers`, `reason` |
| `APPROVAL_RESOLVED` | `approval_id*`, `tool_call_id*`, `decision*`, `decided_by*`, `quorum_met` |
| `CLIENT_TOOL_REQUEST` | `tool_call_id*`, `tool_name*`, `arguments*`, `expires_at*` |
| `JOB_STARTED` | `job_id*`, `tool_call_id*`, `command*`, `detached*`, `started_at`, `limits` |
| `JOB_PROGRESS_DELTA` | `stream*`, `delta*`, `index*` |
| `JOB_FINISHED` | `status*`, `exit_code*` (nullable), `signal`, `summary*`, `output_snapshot`, `output_truncated`, `output_bytes_total`, `duration_ms`, `triggered_new_turn*`, `new_turn_id`, `injected_into_turn_id` |
| `JOB_LIST_RESULT` | `jobs*` |
| `JOB_STATE` | `job_id*`, `status*`, `output`, `next_index` |
| `HISTORY_PAGE` | `items*`, `has_more*` |
| `HISTORY_REWOUND` | `to_turn_id*`, `inclusive*`, `rewound_turn_ids*`, `side_effects`, `workspace_restored` |
| `HISTORY_COMPACTED` | `up_to_turn_id*`, `strategy*`, `trigger`, `tokens_before`, `tokens_after` |
| `CONTEXT_UPDATED` | `pinned_turn_ids*` |
| `ERROR` | `code*`, `message*`, `retryable*`, `details`, `retry_after_ms` |
| `AUTH_EXPIRING` | `expires_at*`, `refresh_before` |
| `AUTH_REFRESHED` | `expires_at*` |
| `BUDGET_WARNING` | `scope*`, `metric*`, `used*`, `limit*`, `threshold*` |
| `QUOTA_WARNING` | `dimension*`, `used*`, `limit*`, `threshold*`, `resets_at` |
| `SERVER_DRAINING` | `deadline_at*`, `reason` |
| `PRESENCE` | `participants*` |
| `CHILD_SESSION_STARTED` | `child_session_id*`, `agent_id*`, `tool_call_id*`, `lifetime`, `task_summary`, `depth`, `name`, `budget_reserved` |
| `CHILD_SESSION_UPDATED` | `child_session_id*`, `state*`, `reason*`, `summary`, `usage`, `budget_remaining`, `pending`, `result`, `child_seq`, `error` |
| `DELEGATOR_STATE_CHANGED` | `suspended*`, `by`, `queued_messages`, `reason` |
| `NOTIFICATION_STATUS` | `notify_state*`, `consecutive_failures`, `last_error`, `last_attempt_at`, `undelivered` |
| `OUTPUT_PART_STARTED` | `part_id*`, `index*`, `type*`, `mime_type*`, `streamed*`, `name`, `alt`, `duration_ms`, `width`, `height`, `bytes_expected` |
| `OUTPUT_PART_DELTA` | `part_id*`, `index*`, `data*` |
| `OUTPUT_PART_FINISHED` | `part_id*`, `status*`, `source`, `bytes`, `sha256`, `transcript`, `error` |

### 27.3 Agent tools

Agent tools are called by the model, not sent as frames: `spawn_agent`, `inspect_agent`, `message_agent`, `stop_agent`, `wait_agent`, `list_agents`, `approve_child_call`, `ask_delegator` (Section 8.2.5), `handoff_agent` (Section 8.3), and `context_search` and `context_write` (Section 7.2). Their arguments and results are in Appendix B.

### 27.4 Enumerations

| Enumeration | Values |
| --- | --- |
| `SESSION_STATE.status` | `ACTIVE`, `SUSPENDED`, `CLOSED` |
| `SESSION_CLOSED.reason` | `client`, `ttl`, `retention`, `admin`, `parent_closed` |
| `TURN_STATE_CHANGED.state` | `QUEUED`, `CALLING_PROVIDER`, `EXECUTING_TOOL`, `AWAITING_APPROVAL`, `COMPLETED`, `CANCELLED`, `FAILED` |
| `TURN_STATE_CHANGED.stop_reason` | `end_turn`, `max_tokens`, `cancelled`, `error`, `tool_limit`, `budget_exceeded` |
| `TOKEN_DELTA.channel` | `text`, `reasoning` |
| `TOKEN_DELTA.kind` (reasoning only) | `summary`, `raw`, `redacted` |
| `TOOL_CALL_STARTED.executor` | `server`, `client` |
| `TOOL_CALL_FINISHED.status` | `OK`, `ERROR`, `DENIED`, `CANCELLED`, `TIMED_OUT` |
| Tool `risk` | `low`, `medium`, `high` |
| Tool `side_effects` | `none`, `local`, `external` |
| `TOOL_APPROVAL_RESPONSE.decision` | `approve`, `deny`, `approve_for_session` |
| `approvers.kind` | `controller`, `group`, `user` |
| Job status | `RUNNING`, `COMPLETED`, `FAILED`, `KILLED`, `TIMED_OUT` |
| `USER_INPUT.queue_policy` | `reject`, `queue`, `interrupt` |
| `origin.kind` | `user`, `delegator`, `system`, `trigger`, `schedule` |
| `job_turn_policy`, `child_turn_policy` | `always`, `on_failure`, `never` |
| Child state | `RUNNING`, `AWAITING_APPROVAL`, `AWAITING_INPUT`, `IDLE`, `FAILED`, `STOPPED`, `CLOSED` |
| Modality | `text`, `image`, `document`, `audio`, `video` |
| `SESSION_REWIND.workspace` | `keep`, `restore` |
| `HISTORY_COMPACTED.trigger` | `auto`, `manual` |
| Share role | `viewer`, `editor`, `owner` |
| `PRESENCE_UPDATE.status` | `active`, `idle`, `typing` |
| `INPUT_STREAM_START.turn_detection` | `manual`, `server` |
| Run status | `queued`, `running`, `awaiting_approval`, `completed`, `failed`, `cancelled` |

## 28. Observability and Operations

### 28.1 Observability

- Every terminal turn reports `usage`. The server SHOULD also expose cumulative session usage in `SESSION_STATE` (`usage_total`) and, for trees, `usage_rollup`.
- Servers SHOULD propagate a trace id in `ext.trace_id` and log `id`, `seq`, `session_id`, `turn_id`, `job_id` on every frame.
- Servers SHOULD emit metrics for turn latency, time to first `TOKEN_DELTA`, replay rate, slow-consumer closes and job outcomes.

**Delegation.** A child is a session and is observed as one: it has its own logs, metrics and audit trail, and is not folded into its parent's. The tree is linked by labels and by trace context:

- Every log line and metric of a child SHOULD carry `session_id`, `parent_session_id`, `root_session_id`, `depth` and `agent_id`. The delegating `tool_call_id` is on the child's first audit record.
- A child inherits the parent's `ext.trace_id` and opens a new span with `ext.parent_span_id` set to the span of the `spawn_agent` call, so one trace covers the whole tree.
- Metrics SHOULD include spawn rate, children by terminal state, child duration, time from spawn to first output, tree size and depth, structured-output validation failures, approvals by routing and outcome, and escalations.
- Cost and token metrics are reported per session (own usage); dashboards use `root_session_id` for tree totals.

**Other features.** SHOULD also be measured: webhook deliveries by outcome and attempt, delivery latency, disabled targets; output parts by type and size; exports and imports by result and bytes.

### 28.2 Discovery and Health

| Resource | Behaviour |
| --- | --- |
| `GET /raep/v1/info` | No authentication needed. Returns `server_version`, `wire_versions`, `capabilities`, the public part of `limits`, `transports` (the WebSocket and SSE URLs), `registries` (versions), `deprecations` and `draining`. It lets a client choose a binding and check compatibility before connecting. |
| `GET /raep/v1/health/live` | `200` while the process runs. Returns `{status: "ok"}`. |
| `GET /raep/v1/health/ready` | `200` with `{status: "ready"}` when the server can accept new sessions (storage and leases reachable). `503` with `{status: "draining"}` or `{status: "degraded", checks}` otherwise. A provider outage makes the server `degraded` in `checks` but does not fail readiness. |

The health resources need no authentication and never return tenant data.

### 28.3 Admin API

The admin API lives under `/raep/admin/v1` and needs a token with the `admin:*` scope for the audience `raep-admin`. Capability: `admin`.

| Operation | Request |
| --- | --- |
| List sessions | `GET /raep/admin/v1/sessions` (all tenants, filtered) |
| Session tree | `GET /raep/admin/v1/sessions/{session_id}/tree` |
| Close a session | `POST /raep/admin/v1/sessions/{session_id}/close` with `{reason, purge}` |
| Cancel a turn | `POST /raep/admin/v1/sessions/{session_id}/turns/{turn_id}/cancel` |
| Kill a job | `POST /raep/admin/v1/sessions/{session_id}/jobs/{job_id}/kill` |
| Drain | `POST /raep/admin/v1/drain` with `{deadline_s}` |
| Tenant quota | `GET` and `PUT /raep/admin/v1/quota/{tenant}` |
| Suspend a tenant | `POST /raep/admin/v1/tenants/{tenant}/suspend` |

The API returns metadata only: ids, states, usage and timing. Reading message content, outputs or snapshots needs the extra scope `admin:content` and a `reason`, is recorded in the audit log with the operator's identity, and the server notifies the tenant when its policy requires it. Every call is audited.

### 28.4 Graceful Shutdown and Draining

A server drains on `POST /raep/admin/v1/drain` or on a termination signal:

1. Readiness changes to `draining` and `HELLO.draining` is true for new connections.
2. New sessions, runs, schedule fires and trigger fires are refused with `UNAVAILABLE` and `retry_after_ms`; a WebSocket attach is closed with `1013`.
3. Attached clients receive `SERVER_DRAINING` (`deadline_at`, `reason`).
4. Running turns continue until `deadline_at`. Idle connections are closed with `1001` after the notice.
5. At the deadline the server closes the remaining connections with `1001` and releases its leases. A turn still running is recovered as in Section 17.3.

Clients reconnect with backoff, honouring `retry_after_ms`, and resume (Section 16). Jobs in the job runner keep running.

### 28.5 Metrics

Servers SHOULD expose metrics with these names (Prometheus style). Labels never carry session ids or user ids; a `tenant` label is allowed only on aggregated metrics.

| Metric | Labels |
| --- | --- |
| `raep_sessions_active` | |
| `raep_turns_total` | `state` |
| `raep_turn_duration_seconds` | |
| `raep_time_to_first_token_seconds` | |
| `raep_tool_calls_total` | `status` |
| `raep_tool_call_duration_seconds` | `executor` |
| `raep_jobs_running` | |
| `raep_events_delivered_total` | `type` |
| `raep_replay_requests_total` | `truncated`, `lost` |
| `raep_slow_consumer_closes_total` | |
| `raep_provider_requests_total` | `provider`, `outcome` |
| `raep_provider_retries_total` | `reason`, `fallback` |
| `raep_child_sessions_total` | `state` |
| `raep_approvals_total` | `routing`, `outcome` |
| `raep_runs_total` | `status` |
| `raep_quota_rejections_total` | `dimension` |
| `raep_rate_limited_total` | `scope` |
| `raep_recoveries_total` | `result` |
| `raep_webhook_deliveries_total` | `outcome` |

## 29. Execution Flow

### 29.1 Session, turn and job

```
Client                    Server                 Job Runner           LLM Provider
  |-- WS open (raep.v1) -->|                          |                    |
  |<------- HELLO ---------|                          |                    |
  |-- SESSION_ATTACH ----->|                          |                    |
  |<-- SESSION_ATTACHED ---|                          |                    |
  |<---- SESSION_STATE ----|                          |                    |
  |-- USER_INPUT --------->|                          |                    |
  |<-- TURN_STATE_CHANGED (CALLING_PROVIDER)            |-- generate ------->|
  |<-- TOKEN_DELTA* -------|<------------------ tool call: detached_shell --|
  |<-- TOOL_CALL_STARTED --|                          |                    |
  |<-- TURN_STATE_CHANGED (EXECUTING_TOOL)            |                    |
  |                        |-- spawn job ------------>|                    |
  |<-- JOB_STARTED --------|                          |                    |
  |<-- TOOL_CALL_FINISHED -|-- tool result ------------------------------->|
  |<-- TURN_STATE_CHANGED (COMPLETED, usage)          |                    |
  |                        |<-- output ---------------|                    |
  |<-- JOB_PROGRESS_DELTA* |                          |                    |
  |-- CANCEL_JOB --------->|-- SIGTERM / SIGKILL ---->X                    |
  |<-- JOB_FINISHED (KILLED, signal SIGTERM)          |                    |
  |<-- TURN_STATE_CHANGED (QUEUED -> CALLING_PROVIDER)  |-- job result ctx ->|
  |   (new turn, per job_turn_policy)                 |                    |
  |-- (disconnect) ... reconnect: SESSION_ATTACH{resume_from_seq} --------->|
  |<-- SESSION_ATTACHED{replay} + replayed events ----|                    |
```

**Delegation flow**

```
Client            Parent session            Child session              LLM Provider
  |-- USER_INPUT ---->|                          |                          |
  |<-- TURN_STATE_CHANGED (CALLING_PROVIDER) ----------------------------->|
  |<-- TOOL_CALL_STARTED (spawn_agent) ------    |                          |
  |<-- CHILD_SESSION_STARTED                     |-- first turn from task ->|
  |<-- TOOL_CALL_FINISHED (child_session_id)     |                          |
  |<-- CHILD_SESSION_UPDATED (AWAITING_APPROVAL, routed_to=parent)          |
  |                   |-- approve_child_call --->|  (low risk: parent agent) |
  |<-- CHILD_SESSION_UPDATED (RUNNING)           |                          |
  |-- SESSION_ATTACH (child, observer) ----------|  (optional, for detail)  |
  |<-- TOOL_CALL_STARTED (wait_agent)            |                          |
  |<-- CHILD_SESSION_UPDATED (IDLE, result)      |                          |
  |<-- TOOL_CALL_FINISHED (wait_agent: child_result)                        |
  |<-- TURN_STATE_CHANGED (COMPLETED)  -> lifetime:turn children stopped   |
```

### 29.2 Client-side tool

```
Client (controller)          Server                          Model provider
  |-- SESSION_ATTACH{client_tools} ->|                              |
  |-- USER_INPUT ------------------->|-- generate ----------------->|
  |<-- TURN_STATE_CHANGED (CALLING_PROVIDER)                         |
  |                                  |<-- tool call: read_file -----|
  |<-- TOOL_CALL_STARTED (executor: client)                         |
  |<-- CLIENT_TOOL_REQUEST ----------|                              |
  |   (runs the tool locally)        |                              |
  |-- CLIENT_TOOL_RESULT ----------->|                              |
  |<-- TOOL_CALL_FINISHED -----------|-- tool result -------------->|
  |<-- TOKEN_DELTA* ... TURN_STATE_CHANGED (COMPLETED)              |
```

### 29.3 Recovery after a crash

```
Client                       Node A (owner)         Store            Node B
  |<-- TOKEN_DELTA seq 500 ------|                    |
  |                              X  crash             |
  |                                                   |-- lease expires -->|
  |                                                   |<-- lease + fence --|
  |                                                   |   recovery: turn -> FAILED (interrupted)
  |-- SESSION_ATTACH{resume_from_seq: 500} ---------------------------->|
  |<-- SESSION_ATTACHED{replay.lost: 498..520} -----------------------|
  |<-- SESSION_STATE ------------------------------------------------|
  |   (client rebuilds the turn from history)                          |
```

## 30. Conformance Levels

A server declares what it implements through `HELLO.capabilities` (Section 6.1). Conformance has four levels, a set of profiles and a production profile that combines them.

**Levels**

| Level | Requirements |
| --- | --- |
| **Core** | Sections 4.1 to 4.4 and 4.6, 5, 6, 10, 12.1 to 12.3, the `prompt` form of 12.4, 15, 23 and 24 (items 1 to 4) |
| **Tools** | Core plus Sections 13.1 to 13.4 |
| **Jobs** | Tools plus Section 14 |
| **Resilient** | Core plus Sections 16 and 17 (`seq`, `ACK`, replay, idempotency, durability, recovery) |

**Profiles**

| Profile | Requirements |
| --- | --- |
| **SSE** | Section 4.5, advertised as `transport.sse`. A server that offers it MUST pass the same Core and Resilient tests over it. WebSocket (Section 4.1) is always required. |
| **Reconfigure** | Sections 8.1 and 8.3, advertised as `reconfigure`. Agent descriptors themselves stay optional (Section 7). |
| **Multimodal** | Section 12.4 for every input modality the agent advertises, plus uploads (`uploads`, and `uploads.resumable` for resumable ones). |
| **Structured output** | Section 12.6, advertised as `structured_output`. |
| **Realtime input** | Section 12.7, advertised as `input.stream`. Requires Multimodal. |
| **Delegation** | Section 8.2, advertised as `delegation`. Requires Tools and Resilient. `delegation.structured_result` is an optional feature of this profile (`output_schema`, Section 8.2.6). |
| **Federation** | Section 22.2, advertised as `federation`. Requires Delegation. |
| **Notifications** | Section 9, advertised as `notify`. Requires Resilient. |
| **Output parts** | Section 12.5, advertised as `output.parts`. Requires Multimodal. |
| **Portability** | Section 11, advertised as `export` and `import`. Requires Resilient; delegation bundles require Delegation. |
| **Client tools** | Section 13.5, advertised as `client_tools`. Requires Tools. |
| **Approval policy** | Section 13.6, advertised as `approvals.policy`. Requires Tools. |
| **History control** | Sections 18.1 to 18.3 and 18.5, advertised as `fork`, `rewind` and `compaction`; Section 18.4 as `workspace.snapshots`. Requires Resilient. |
| **Management** | Section 19, advertised as `manage`. |
| **Runs** | Section 20.1, advertised as `runs`. Requires Resilient. |
| **Schedules** | Sections 20.2 and 20.3, advertised as `schedules`. Requires Runs. |
| **Collaboration** | Section 21, advertised as `collaboration`. Requires Resilient. |
| **Quotas** | Sections 25.4 to 25.6, advertised as `quota`. |
| **Operations** | Sections 28.2 to 28.4, with `admin` for Section 28.3. |

**Production.** A server claims the Production profile when it conforms to Core, Tools, Jobs and Resilient, and to the profiles Operations and Quotas, and implements Sections 24 (all items), 25.1 to 25.3 and 28.5.

## 31. Conformance Test Suite

### 31.1 Structure

The suite is a set of **scenarios** and a set of **frame vectors**. A scenario is a JSON document (`conformance_scenario`, Appendix B) with `id`, `title`, `profile`, `requires` (capabilities), `setup` and `steps`. A step is one of:

| Step | Meaning |
| --- | --- |
| `send` | A frame, or an HTTP request, the harness sends. Values written `{"$capture": "name"}` store a value from an earlier frame, and `{"$use": "name"}` inserts it. |
| `expect` | A frame the harness expects next on the stream: a `type` and a partial `payload` match. Matcher values: `{"$any": true}`, `{"$regex": "..."}`, `{"$gte": n}` and `{"$exists": bool}`. Order of frames is checked by `seq`. |
| `expect_none` | No frame of the given `type` arrives within `within_ms`. |
| `action` | An out-of-band step: `disconnect`, `reconnect`, `crash_server`, `advance_time`, `fail_provider`, or `lose_lease`. |

`setup` names the agent (a spec), the tools the server must offer, and the **provider script**: a deterministic list of model outputs and tool calls that a stub provider plays. A server exposes the stub provider only in a test deployment, selected by the `RAEP-Test-Script` header on attach or by its admin endpoint. Actions that need control of the server (`crash_server`, `advance_time`, `lose_lease`) are available only through the same test deployment hook.

The harness is a client. It never needs to know the server's internals.

### 31.2 Scenarios

A server claims a level or profile only if it passes every scenario of that level or profile.

| ID | Scenario | Level or profile |
| --- | --- | --- |
| C-001 | `HELLO`, attach, `SESSION_ATTACHED`, `SESSION_STATE` in order | Core |
| C-002 | Session id allocation on an empty id | Core |
| C-003 | Attach with an agent that does not match an existing session | Core |
| C-004 | Unsupported wire version closes with `4426` | Core |
| C-010 | Text turn: states, deltas, terminal state with usage | Core |
| C-011 | `queue_policy` `reject`, `queue` and `interrupt`; queue limit | Core |
| C-012 | Repeated `turn_id` and repeated frame `id` are idempotent | Core |
| C-013 | Cancel after the turn finished returns `TURN_NOT_ACTIVE` | Core |
| C-014 | Cancel an active turn | Core |
| C-015 | Token refresh before expiry; close `4001` after it | Core |
| C-020 | Tool call with approval: allow, deny, timeout | Tools |
| C-021 | Tool timeout finishes `TIMED_OUT` | Tools |
| C-022 | Retry of an idempotent tool; no retry of a non-idempotent one | Tools |
| C-030 | Job lifecycle and progress deltas | Jobs |
| C-031 | `CANCEL_JOB` and its idempotency | Jobs |
| C-032 | Job result injection under each `job_turn_policy` | Jobs |
| C-040 | `seq` is strictly increasing without gaps | Resilient |
| C-041 | Resume replays exactly the missed events | Resilient |
| C-042 | Resume past the buffer returns `truncated` | Resilient |
| C-043 | Crash: `lost` range, interrupted turn, interrupted tool call | Resilient |
| C-044 | `ACK` lets the server trim the buffer | Resilient |
| C-045 | A slow consumer is closed with `4008` and resumes | Resilient |
| C-046 | Failover keeps the frame-id deduplication | Resilient |
| C-050 | Observers cannot send commands; a second controller is refused | Core |
| C-060 | Spawn, wait, inspect, message and stop a child | Delegation |
| C-061 | Spawn limits and budget shares | Delegation |
| C-062 | Child approval routing, with and without a human | Delegation |
| C-063 | Suspend and release of a child by a human | Delegation |
| C-064 | Closing a parent closes its children | Delegation |
| C-070 | The same Core scenarios over SSE | SSE |
| C-071 | `Last-Event-ID` resume over SSE | SSE |
| C-080 | Resumable upload and resume of an interrupted one | Multimodal |
| C-081 | Modality, MIME type and size validation | Multimodal |
| C-082 | `USER_INPUT_ACCEPTED` carries origin and content | Core |
| C-090 | Output parts: started, delta, finished; fetching an output | Output parts |
| C-100 | Webhook signature, retry and deduplication | Notifications |
| C-110 | Export, import preview, import commit round trip | Portability |
| C-120 | Fork from a turn; usage carried | History control |
| C-121 | Rewind blocked, then allowed with `cancel_pending` | History control |
| C-122 | Edit and re-run with `replaces_turn_id` | History control |
| C-123 | Compaction and pins | History control |
| C-130 | Client-side tool: request, result, expiry, replay after reattach | Client tools |
| C-140 | Run with `wait_s`, cancel, idempotency key | Runs |
| C-141 | Schedule fire, overlap and disable after failures | Schedules |
| C-142 | Trigger signature and untrusted payload labelling | Schedules |
| C-150 | Two controllers, attribution, shared approval | Collaboration |
| C-160 | Quota exhaustion and rate limits | Quotas |
| C-170 | Drain: notice, refusal of new sessions, deadline | Operations |
| C-180 | Structured output: success, correction, failure | Structured output |
| C-181 | Streamed audio input with `manual` and `server` detection | Realtime input |
| C-190 | Provider retry, fallback and `discard_from_index` | Reconfigure |
| C-200 | Remote delegate: spawn, budgets, failure | Federation |
| C-210 | Tenant isolation: foreign ids answer `NOT_FOUND` | Production |

### 31.3 Frame Vectors

Appendix C holds vectors for frame validation: valid frames that MUST pass `frame.json`, and invalid frames that MUST fail it, each with the reason. A conforming implementation, client or server, validates its own output against the vectors in its test run.

### 31.4 Reports

A run produces a `conformance_report` (Appendix B): the implementation, its version, the wire version, the levels and profiles it claims, and for each scenario `pass`, `fail` or `skip` with a reason. A `skip` is allowed only for a scenario whose `requires` capability the server does not advertise. Reports are the evidence for Section 26.1.

## 32. Open Questions

1. Continuous replication of a session between servers, instead of one-shot export.
2. Push channels for mobile platforms; only `webhook` is normative.
3. Streaming of partial structured output before a turn or child finishes.
4. Cost attribution and billing across federated servers.
5. Turn-taking in realtime audio beyond barge-in: overlap and backchannel.
6. Resuming an interrupted turn from its last durable boundary instead of failing it.

---

## Appendix A. Examples

### A.1 Core flow

**`USER_INPUT` with queue policy**

```json
{"version":"1.0","id":"01JB0000000000000000000002","session_id":"sess_88392","turn_id":"turn_101","job_id":null,"timestamp":1793404801000,"type":"USER_INPUT","payload":{"prompt":"Run a build in the background and analyze the log","queue_policy":"reject"}}
```

**`JOB_FINISHED` after cancellation**

```json
{"version":"1.0","id":"01JB0000000000000000000040","seq":57,"ref":"01JB0000000000000000000004","session_id":"sess_88392","turn_id":null,"job_id":"job_3344","timestamp":1793404810000,"type":"JOB_FINISHED","payload":{"status":"KILLED","exit_code":null,"signal":"SIGTERM","summary":"Job cargo build --release terminated by user","output_snapshot":"Compiling crate v0.1.0...\n","output_truncated":false,"output_bytes_total":26,"duration_ms":7500,"triggered_new_turn":true,"new_turn_id":"turn_102","injected_into_turn_id":null}}
```

**`ERROR` for a busy session**

```json
{"version":"1.0","id":"01JB0000000000000000000050","seq":58,"ref":"01JB0000000000000000000011","session_id":"sess_88392","turn_id":"turn_103","job_id":null,"timestamp":1793404811000,"type":"ERROR","payload":{"code":"SESSION_BUSY","message":"A turn is already active","retryable":true,"retry_after_ms":1000}}
```

**`TOOL_APPROVAL_REQUEST`**

```json
{"version":"1.0","id":"01JB0000000000000000000060","seq":21,"ref":null,"session_id":"sess_88392","turn_id":"turn_101","job_id":null,"timestamp":1793404802200,"type":"TOOL_APPROVAL_REQUEST","payload":{"approval_id":"apr_01JB","tool_call_id":"call_sh_02","tool_name":"shell_exec","arguments":{"command":"rm -rf target"},"risk":"high","expires_at":1793404862200}}
```

### A.2 Delegation, notifications, output parts and portability

**`TOOL_CALL_STARTED` for `spawn_agent` (parent stream)**

```json
{"version":"1.0","id":"01JB0000000000000000001001","seq":28,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404901000,"type":"TOOL_CALL_STARTED","payload":{"tool_call_id":"call_sp_01","tool_name":"spawn_agent","arguments":{"agent_id":"example-reviewer","name":"auth-review","task":"Review src/auth for SQL injection. Return findings as JSON.","lifetime":"turn","budget_share":0.25,"scopes":["fs:read"],"inspect":"events","output_schema":{"type":"object","required":["findings"],"properties":{"findings":{"type":"array","items":{"type":"object"}}}},"context":{"mode":"none"},"workspace":"shared_ro"},"requires_approval":false}}
```

**`CHILD_SESSION_STARTED` (parent stream)**

```json
{"version":"1.0","id":"01JB0000000000000000001002","seq":29,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404902000,"type":"CHILD_SESSION_STARTED","payload":{"child_session_id":"sess_c_4412","agent_id":"example-reviewer","tool_call_id":"call_sp_01","lifetime":"turn","task_summary":"Review the auth module for injection flaws","depth":1,"name":"auth-review","budget_reserved":{"tokens":50000,"cost":2.5,"tool_calls":40}}}
```

**`CHILD_SESSION_UPDATED`, approval routed to the parent agent**

```json
{"version":"1.0","id":"01JB0000000000000000001003","seq":34,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404903000,"type":"CHILD_SESSION_UPDATED","payload":{"child_session_id":"sess_c_4412","state":"AWAITING_APPROVAL","reason":"approval_pending","summary":"Wants to run: git log --stat","pending":{"kind":"approval","tool_call_id":"call_sh_11","tool_name":"shell_exec","risk":"low","expires_at":1793404962200,"routed_to":"parent"},"usage":{"input_tokens":9100,"output_tokens":820,"cached_tokens":6000,"reasoning_tokens":300,"tool_calls":6,"duration_ms":14200,"cost":{"amount":0.41,"currency":"USD"}},"budget_remaining":{"tokens":40080,"cost":2.09},"child_seq":63}}
```

**`CHILD_SESSION_UPDATED`, child finished with a structured result**

```json
{"version":"1.0","id":"01JB0000000000000000001004","seq":41,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404904000,"type":"CHILD_SESSION_UPDATED","payload":{"child_session_id":"sess_c_4412","state":"IDLE","reason":"turn_finished","summary":"Found 2 issues","result":{"status":"completed","summary":"Two injection risks found: login.rs:42 (high) and query.rs:7 (medium).","output":{"findings":[{"file":"login.rs","line":42,"severity":"high"},{"file":"query.rs","line":7,"severity":"medium"}]},"output_schema_valid":true,"artifacts":[{"name":"report.md","mime_type":"text/markdown","bytes":3120,"source":{"kind":"upload","upload_id":"upl_c91d"}}],"usage":{"input_tokens":9100,"output_tokens":820,"cached_tokens":6000,"reasoning_tokens":300,"tool_calls":6,"duration_ms":14200,"cost":{"amount":0.41,"currency":"USD"}},"final_seq":97,"child_turn_id":"turn_c_1"},"usage":{"input_tokens":9100,"output_tokens":820,"cached_tokens":6000,"reasoning_tokens":300,"tool_calls":6,"duration_ms":14200,"cost":{"amount":0.41,"currency":"USD"}},"child_seq":97}}
```

**`inspect_agent` result, `view: status`** (the body of the tool result)

```json
{"view":"status","child_session_id":"sess_c_4412","status":{"state":"RUNNING","activity":"Running tool: grep_search","last_output_excerpt":"Checking query builders...","usage":{"input_tokens":9100,"output_tokens":820,"cached_tokens":6000,"reasoning_tokens":300,"tool_calls":6,"duration_ms":14200,"cost":{"amount":0.41,"currency":"USD"}},"usage_rollup":{"input_tokens":9100,"output_tokens":820,"cached_tokens":6000,"reasoning_tokens":300,"tool_calls":6,"duration_ms":14200,"cost":{"amount":0.41,"currency":"USD"}},"budget_remaining":{"tokens":40080,"cost":2.09,"tool_calls":34},"jobs":[{"job_id":"job_c_7","command":"cargo test","status":"RUNNING"}],"children":[],"last_seq":63,"delegator_suspended":false,"updated_at":1793404910000},"next_cursor":null,"truncated":false,"bytes":612}
```

**Delegator feedback as it appears in the child's history** (`HISTORY_PAGE` item or `inspect_agent` transcript)

```json
{"role":"system_event","turn_id":"turn_c_1","origin":{"kind":"delegator","session_id":"sess_88392","tool_call_id":"call_msg_03"},"delivery":"steer","content":"Focus only on the auth module.","timestamp":1793404905000}
```

**Delegation tool error returned to the model** when the depth limit is hit

```json
{"code":"BUDGET_EXCEEDED","message":"Maximum child depth reached","details":{"metric":"child_depth","limit":2,"current":2}}
```

**`DELEGATOR_CONTROL` (child session, human controller)**

```json
{"version":"1.0","id":"01JB0000000000000000001005","ref":null,"session_id":"sess_c_4412","turn_id":null,"job_id":null,"timestamp":1793404905000,"type":"DELEGATOR_CONTROL","payload":{"action":"suspend","reason":"Human taking over to debug"}}
```

**`DELEGATOR_STATE_CHANGED` (child session)**

```json
{"version":"1.0","id":"01JB0000000000000000001006","seq":70,"ref":null,"session_id":"sess_c_4412","turn_id":null,"job_id":null,"timestamp":1793404906000,"type":"DELEGATOR_STATE_CHANGED","payload":{"suspended":true,"by":"client","queued_messages":0}}
```

**`SESSION_CONFIGURE` enabling a webhook**

```json
{"version":"1.0","id":"01JB0000000000000000001007","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404907000,"type":"SESSION_CONFIGURE","payload":{"options":{"notify":{"channel":"webhook","url":"https://hooks.example.com/raep","secret":"ssssssssssssssssssssssssssssssssssssssss","events":["TURN_STATE_CHANGED","JOB_FINISHED","CHILD_SESSION_UPDATED","TOOL_APPROVAL_REQUEST"],"when":"detached","include":"summary","include_children":true,"coalesce_s":5}},"apply":"now"}}
```

**Webhook delivery** (HTTP `POST` to the registered URL)

```
POST /raep HTTP/1.1
Content-Type: application/json
RAEP-Notification-Id: ntf_01JB0000000000000000000900
RAEP-Delivery-Attempt: 1
RAEP-Signature: t=1793404990,v1=<hex HMAC-SHA256 of "1793404990." + body>

{"notification_id":"ntf_01JB0000000000000000000900","session_id":"sess_c_4412","root_session_id":"sess_88392","seq":97,"type":"CHILD_SESSION_UPDATED","turn_id":null,"job_id":null,"child_session_id":"sess_c_4412","occurred_at":1793404990000,"summary":"Child auth-review finished: completed","attempt":1}
```

**`OUTPUT_PART_STARTED`**

```json
{"version":"1.0","id":"01JB0000000000000000001008","seq":52,"ref":null,"session_id":"sess_88392","turn_id":"turn_106","job_id":null,"timestamp":1793404908000,"type":"OUTPUT_PART_STARTED","payload":{"part_id":"part_01","index":0,"type":"image","mime_type":"image/png","alt":"Architecture diagram","width":1280,"height":720,"bytes_expected":184220,"streamed":false}}
```

**`OUTPUT_PART_FINISHED`**

```json
{"version":"1.0","id":"01JB0000000000000000001009","seq":53,"ref":null,"session_id":"sess_88392","turn_id":"turn_106","job_id":null,"timestamp":1793404909000,"type":"OUTPUT_PART_FINISHED","payload":{"part_id":"part_01","status":"OK","source":{"kind":"output","output_id":"out_5a1c"},"bytes":184220,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}
```

**`SESSION_EXPORT`**

```json
{"version":"1.0","id":"01JB0000000000000000001010","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404910000,"type":"SESSION_EXPORT","payload":{"include":{"children":true,"uploads":true,"job_output":true,"outputs":true},"quiesce":"wait","wait_timeout_s":120}}
```

**`SESSION_EXPORT_READY`**

```json
{"version":"1.0","id":"01JB0000000000000000001011","seq":141,"ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404911000,"type":"SESSION_EXPORT_READY","payload":{"export_id":"exp_77d2","download_url":"/raep/v1/exports/exp_77d2","bytes":2481920,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","expires_at":1793408400000,"sessions":3,"encrypted":false,"signed":true}}
```

**Bundle `manifest.json`**

```json
{"bundle_version":"1","raep_document_version":"0.1.0","wire_version":"1.0","exported_at":1793404999000,"exporter":{"server":"raep.vyde.dev","kid":"k-2026-10"},"root_session_id":"sess_88392","sessions":[{"session_id":"sess_88392","parent_session_id":null,"agent":{"id":"example-agent","version":"3"},"state":"IDLE","history_file":"sessions/sess_88392/history.jsonl","last_seq":140,"usage_total":{"input_tokens":9100,"output_tokens":820,"cached_tokens":6000,"reasoning_tokens":300,"tool_calls":6,"duration_ms":14200,"cost":{"amount":0.41,"currency":"USD"}}},{"session_id":"sess_c_4412","parent_session_id":"sess_88392","agent":{"id":"example-reviewer","version":"1"},"state":"IDLE","history_file":"sessions/sess_c_4412/history.jsonl","last_seq":97}],"files":[{"path":"sessions/sess_88392/history.jsonl","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","bytes":90211}],"excluded":["credentials","approval_grants","notify_secrets","provider_opaque_state","live_processes","sandbox_filesystem"]}
```

**Import preview (`POST /raep/v1/imports` response) and commit body**

```json
{"import_id":"imp_3b90","bundle_version":"1","sessions":2,"signature":"valid","agents":[{"agent_id":"example-agent","version":"3","available":true},{"agent_id":"example-reviewer","version":"1","available":false}],"warnings":["agent example-reviewer@1 not available; supply agent_map"],"expires_at":1793408400000}
```

```json
{"agent_map":{"example-reviewer@1":"example-reviewer@2"},"carry_usage":true}
```

### A.3 History control, client tools, runs and operations

**`USER_INPUT_ACCEPTED`**

```json
{"version":"1.0","id":"01JB0000000000000000000100","seq":60,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404900000,"type":"USER_INPUT_ACCEPTED","payload":{"turn_id":"turn_105","origin":{"kind":"user","user_id":"u_9","display_name":"Ana"},"content":[{"type":"text","text":"Summarize the build log"}],"position":0}}
```

**`PROVIDER_RETRY` with a fallback**

```json
{"version":"1.0","id":"01JB0000000000000000000101","seq":61,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404900000,"type":"PROVIDER_RETRY","payload":{"attempt":2,"reason":"PROVIDER_RATE_LIMITED","provider":{"id":"example-provider-b","model":"example-model-b"},"fallback":true,"discard_from_index":37}}
```

**`CLIENT_TOOL_REQUEST` and `CLIENT_TOOL_RESULT`**

```json
{"version":"1.0","id":"01JB0000000000000000000102","seq":62,"ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404900000,"type":"CLIENT_TOOL_REQUEST","payload":{"tool_call_id":"call_ct_01","tool_name":"read_file","arguments":{"path":"src/main.rs"},"expires_at":1793405020000}}
```

**`CLIENT_TOOL_RESULT` (client to server)**

```json
{"version":"1.0","id":"01JB0000000000000000000103","ref":null,"session_id":"sess_88392","turn_id":"turn_105","job_id":null,"timestamp":1793404900000,"type":"CLIENT_TOOL_RESULT","payload":{"tool_call_id":"call_ct_01","status":"OK","content":"fn main() {}"}}
```

**`SESSION_REWIND` and its reply**

```json
{"version":"1.0","id":"01JB0000000000000000000104","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"SESSION_REWIND","payload":{"to_turn_id":"turn_103","inclusive":false,"cancel_pending":true,"workspace":"keep"}}
```

**`HISTORY_REWOUND`**

```json
{"version":"1.0","id":"01JB0000000000000000000105","seq":63,"ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"HISTORY_REWOUND","payload":{"to_turn_id":"turn_103","inclusive":false,"rewound_turn_ids":["turn_104","turn_105"],"side_effects":[{"tool_call_id":"call_sh_02","tool_name":"shell_exec","side_effects":"external"}],"workspace_restored":false}}
```

**`USER_INPUT` with a structured response format**

```json
{"version":"1.0","id":"01JB0000000000000000000106","ref":null,"session_id":"sess_88392","turn_id":"turn_106","job_id":null,"timestamp":1793404900000,"type":"USER_INPUT","payload":{"prompt":"List the failing crates","response_format":{"type":"json_schema","schema":{"type":"object","properties":{"crates":{"type":"array","items":{"type":"string"}}},"required":["crates"]},"strict":false}}}
```

**`TURN_STATE_CHANGED` with structured output**

```json
{"version":"1.0","id":"01JB0000000000000000000107","seq":70,"ref":null,"session_id":"sess_88392","turn_id":"turn_106","job_id":null,"timestamp":1793404900000,"type":"TURN_STATE_CHANGED","payload":{"state":"COMPLETED","stop_reason":"end_turn","output":{"crates":["core","net"]},"output_schema_valid":true}}
```

**`APPROVAL_RESOLVED` with a quorum**

```json
{"version":"1.0","id":"01JB0000000000000000000108","seq":71,"ref":null,"session_id":"sess_88392","turn_id":"turn_101","job_id":null,"timestamp":1793404900000,"type":"APPROVAL_RESOLVED","payload":{"approval_id":"apr_01JB","tool_call_id":"call_sh_02","decision":"approve","decided_by":[{"user_id":"u_3","decision":"approve","at":1793404910000},{"user_id":"u_4","decision":"approve","at":1793404915000}],"quorum_met":true}}
```

**`SESSION_ATTACHED` after a crash**

```json
{"version":"1.0","id":"01JB0000000000000000000109","seq":521,"ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"SESSION_ATTACHED","payload":{"created":false,"mode":"controller","last_seq":520,"replay":{"from_seq":501,"to_seq":520,"truncated":false,"lost":{"from_seq":498,"to_seq":500}},"negotiated_capabilities":["jobs","resume"]}}
```

**`PRESENCE`**

```json
{"version":"1.0","id":"01JB0000000000000000000110","seq":72,"ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"PRESENCE","payload":{"participants":[{"user_id":"u_9","display_name":"Ana","role":"owner","mode":"controller","status":"typing","connected_at":1793404800000,"last_seen_at":1793404899000}]}}
```

**`run_request`**

```json
{"agent_id":"example-agent","input":{"prompt":"Run the test suite and report failures"},"wait_s":30,"on_approval":"deny","ephemeral":true}
```

**`run`**

```json
{"run_id":"run_7f1c","session_id":"sess_91000","turn_id":"turn_1","status":"completed","output":{"text":"2 tests failed: net::timeout, core::parse"},"usage":{"input_tokens":2100,"output_tokens":180},"created_at":1793404900000,"finished_at":1793404921000}
```

## Appendix B. Payload Schemas

This appendix gives the normative JSON Schemas (dialect `https://json-schema.org/draft/2020-12/schema`) for every payload in Section 27 and for the shared objects they use. Each `$id` is `https://raep.vyde.dev/v1/payloads/<NAME>.json`; `$ref` values use the same base. `https://raep.vyde.dev/v1/frame.json` validates a whole frame. As in Section 5, payload schemas do not set `additionalProperties: false`: receivers MUST ignore unknown fields. Names in lower case (`usage`, `child_result`) are shared objects or agent-tool bodies and are not envelope `type` values.

### Common definitions

**`usage`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/usage.json","type":"object","properties":{"input_tokens":{"type":"integer","minimum":0},"output_tokens":{"type":"integer","minimum":0},"cached_tokens":{"type":"integer","minimum":0},"reasoning_tokens":{"type":"integer","minimum":0},"tool_calls":{"type":"integer","minimum":0},"duration_ms":{"type":"integer","minimum":0},"cost":{"type":"object","properties":{"amount":{"type":"number","minimum":0},"currency":{"type":"string"}},"required":["amount","currency"]},"extra":{"type":"object"}}}
```

**`error_payload`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/error_payload.json","type":"object","properties":{"code":{"type":"string"},"message":{"type":"string"},"retryable":{"type":"boolean"},"details":{"type":"object"},"retry_after_ms":{"type":"integer","minimum":0}},"required":["code","message","retryable"]}
```

**`budget_map`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/budget_map.json","type":"object","additionalProperties":{"type":"number","minimum":0}}
```

**`agent_ref`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/agent_ref.json","type":"object","properties":{"id":{"type":"string"},"version":{"type":"string"}},"required":["id"]}
```

**`agent_summary`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/agent_summary.json","type":"object","properties":{"id":{"type":"string"},"version":{"type":"string"},"provider":{"type":"object","properties":{"id":{"type":"string"}}},"model":{"type":"object","properties":{"id":{"type":"string"}}},"sampling":{"type":"object"},"reasoning":{"type":"object"},"budgets":{"type":"object"},"delegation":{"type":"object"},"tools":{"type":"array","items":{"type":"string"}},"scopes":{"type":"array","items":{"type":"string"}},"capabilities":{"type":"object"},"handoff_targets":{"type":"array","items":{"type":"string"}}},"required":["id"]}
```

**`origin`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/origin.json","type":"object","properties":{"kind":{"type":"string","enum":["user","delegator","system","trigger","schedule"]},"session_id":{"type":"string"},"tool_call_id":{"type":"string"},"user_id":{"type":"string"},"display_name":{"type":"string"},"trigger_id":{"type":"string"},"schedule_id":{"type":"string"}},"required":["kind"]}
```

**`history_item`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/history_item.json","type":"object","properties":{"role":{"type":"string","enum":["user","assistant","system_event","tool"]},"content":{"oneOf":[{"type":"string"},{"type":"array","items":{"type":"object"}}]},"turn_id":{"type":"string"},"origin":{"$ref":"https://raep.vyde.dev/v1/payloads/origin.json"},"delivery":{"type":"string","enum":["steer","queue","interrupt"]},"provenance":{"type":"object"},"timestamp":{"type":"integer"},"rewound":{"type":"boolean"}},"required":["role","content"]}
```

### Frame and core payloads

The frame schema validates the envelope and selects the payload schema by `type`. A frame with an unknown `type` passes it; Section 5 says how receivers treat it.

**`frame`**

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://raep.vyde.dev/v1/frame.json","allOf":[{"$ref":"https://raep.vyde.dev/v1/envelope.json"},{"if":{"properties":{"type":{"const":"SESSION_ATTACH"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_ATTACH.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_DETACH"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_DETACH.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_CLOSE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_CLOSE.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_CONFIGURE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_CONFIGURE.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_EXPORT"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_EXPORT.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_FORK"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_FORK.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_REWIND"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_REWIND.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_COMPACT"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_COMPACT.json"}}}},{"if":{"properties":{"type":{"const":"CONTEXT_PIN"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CONTEXT_PIN.json"}}}},{"if":{"properties":{"type":{"const":"USER_INPUT"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/USER_INPUT.json"}}}},{"if":{"properties":{"type":{"const":"INPUT_STREAM_START"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/INPUT_STREAM_START.json"}}}},{"if":{"properties":{"type":{"const":"INPUT_STREAM_DATA"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/INPUT_STREAM_DATA.json"}}}},{"if":{"properties":{"type":{"const":"INPUT_STREAM_END"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/INPUT_STREAM_END.json"}}}},{"if":{"properties":{"type":{"const":"CANCEL_TURN"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CANCEL_TURN.json"}}}},{"if":{"properties":{"type":{"const":"CANCEL_JOB"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CANCEL_JOB.json"}}}},{"if":{"properties":{"type":{"const":"TOOL_APPROVAL_RESPONSE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TOOL_APPROVAL_RESPONSE.json"}}}},{"if":{"properties":{"type":{"const":"CLIENT_TOOL_RESULT"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CLIENT_TOOL_RESULT.json"}}}},{"if":{"properties":{"type":{"const":"JOB_LIST"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_LIST.json"}}}},{"if":{"properties":{"type":{"const":"JOB_GET"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_GET.json"}}}},{"if":{"properties":{"type":{"const":"HISTORY_GET"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/HISTORY_GET.json"}}}},{"if":{"properties":{"type":{"const":"ACK"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/ACK.json"}}}},{"if":{"properties":{"type":{"const":"AUTH_REFRESH"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/AUTH_REFRESH.json"}}}},{"if":{"properties":{"type":{"const":"DELEGATOR_CONTROL"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/DELEGATOR_CONTROL.json"}}}},{"if":{"properties":{"type":{"const":"PRESENCE_UPDATE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/PRESENCE_UPDATE.json"}}}},{"if":{"properties":{"type":{"const":"HELLO"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/HELLO.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_ATTACHED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_ATTACHED.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_STATE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_STATE.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_CLOSED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_CLOSED.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_CONFIGURED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_CONFIGURED.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_UPDATED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_UPDATED.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_FORKED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_FORKED.json"}}}},{"if":{"properties":{"type":{"const":"SESSION_EXPORT_READY"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SESSION_EXPORT_READY.json"}}}},{"if":{"properties":{"type":{"const":"AGENT_CHANGED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/AGENT_CHANGED.json"}}}},{"if":{"properties":{"type":{"const":"USER_INPUT_ACCEPTED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/USER_INPUT_ACCEPTED.json"}}}},{"if":{"properties":{"type":{"const":"TURN_STATE_CHANGED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TURN_STATE_CHANGED.json"}}}},{"if":{"properties":{"type":{"const":"TOKEN_DELTA"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TOKEN_DELTA.json"}}}},{"if":{"properties":{"type":{"const":"PROVIDER_RETRY"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/PROVIDER_RETRY.json"}}}},{"if":{"properties":{"type":{"const":"TOOL_CALL_STARTED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TOOL_CALL_STARTED.json"}}}},{"if":{"properties":{"type":{"const":"TOOL_OUTPUT_DELTA"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TOOL_OUTPUT_DELTA.json"}}}},{"if":{"properties":{"type":{"const":"TOOL_CALL_FINISHED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TOOL_CALL_FINISHED.json"}}}},{"if":{"properties":{"type":{"const":"TOOL_APPROVAL_REQUEST"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/TOOL_APPROVAL_REQUEST.json"}}}},{"if":{"properties":{"type":{"const":"APPROVAL_RESOLVED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/APPROVAL_RESOLVED.json"}}}},{"if":{"properties":{"type":{"const":"CLIENT_TOOL_REQUEST"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CLIENT_TOOL_REQUEST.json"}}}},{"if":{"properties":{"type":{"const":"JOB_STARTED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_STARTED.json"}}}},{"if":{"properties":{"type":{"const":"JOB_PROGRESS_DELTA"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_PROGRESS_DELTA.json"}}}},{"if":{"properties":{"type":{"const":"JOB_FINISHED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_FINISHED.json"}}}},{"if":{"properties":{"type":{"const":"JOB_LIST_RESULT"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_LIST_RESULT.json"}}}},{"if":{"properties":{"type":{"const":"JOB_STATE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/JOB_STATE.json"}}}},{"if":{"properties":{"type":{"const":"HISTORY_PAGE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/HISTORY_PAGE.json"}}}},{"if":{"properties":{"type":{"const":"HISTORY_REWOUND"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/HISTORY_REWOUND.json"}}}},{"if":{"properties":{"type":{"const":"HISTORY_COMPACTED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/HISTORY_COMPACTED.json"}}}},{"if":{"properties":{"type":{"const":"CONTEXT_UPDATED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CONTEXT_UPDATED.json"}}}},{"if":{"properties":{"type":{"const":"ERROR"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/ERROR.json"}}}},{"if":{"properties":{"type":{"const":"AUTH_EXPIRING"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/AUTH_EXPIRING.json"}}}},{"if":{"properties":{"type":{"const":"AUTH_REFRESHED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/AUTH_REFRESHED.json"}}}},{"if":{"properties":{"type":{"const":"BUDGET_WARNING"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/BUDGET_WARNING.json"}}}},{"if":{"properties":{"type":{"const":"QUOTA_WARNING"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/QUOTA_WARNING.json"}}}},{"if":{"properties":{"type":{"const":"SERVER_DRAINING"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/SERVER_DRAINING.json"}}}},{"if":{"properties":{"type":{"const":"PRESENCE"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/PRESENCE.json"}}}},{"if":{"properties":{"type":{"const":"CHILD_SESSION_STARTED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CHILD_SESSION_STARTED.json"}}}},{"if":{"properties":{"type":{"const":"CHILD_SESSION_UPDATED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/CHILD_SESSION_UPDATED.json"}}}},{"if":{"properties":{"type":{"const":"DELEGATOR_STATE_CHANGED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/DELEGATOR_STATE_CHANGED.json"}}}},{"if":{"properties":{"type":{"const":"NOTIFICATION_STATUS"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/NOTIFICATION_STATUS.json"}}}},{"if":{"properties":{"type":{"const":"OUTPUT_PART_STARTED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/OUTPUT_PART_STARTED.json"}}}},{"if":{"properties":{"type":{"const":"OUTPUT_PART_DELTA"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/OUTPUT_PART_DELTA.json"}}}},{"if":{"properties":{"type":{"const":"OUTPUT_PART_FINISHED"}},"required":["type"]},"then":{"properties":{"payload":{"$ref":"https://raep.vyde.dev/v1/payloads/OUTPUT_PART_FINISHED.json"}}}}]}
```

**`source`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/source.json","type":"object","properties":{"kind":{"type":"string","enum":["inline","upload","url","output","stream"]},"data":{"type":"string"},"upload_id":{"type":"string"},"url":{"type":"string","pattern":"^https://"},"output_id":{"type":"string"},"expires_at":{"type":"integer","minimum":0},"stream_id":{"type":"string"}},"required":["kind"],"allOf":[{"if":{"properties":{"kind":{"const":"inline"}},"required":["kind"]},"then":{"required":["data"]}},{"if":{"properties":{"kind":{"const":"upload"}},"required":["kind"]},"then":{"required":["upload_id"]}},{"if":{"properties":{"kind":{"const":"url"}},"required":["kind"]},"then":{"required":["url"]}},{"if":{"properties":{"kind":{"const":"output"}},"required":["kind"]},"then":{"required":["output_id"]}},{"if":{"properties":{"kind":{"const":"stream"}},"required":["kind"]},"then":{"required":["stream_id"]}}]}
```

**`content_part`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/content_part.json","type":"object","properties":{"type":{"type":"string","enum":["text","image","document","audio","video"]},"text":{"type":"string"},"source":{"$ref":"https://raep.vyde.dev/v1/payloads/source.json"},"mime_type":{"type":"string"},"name":{"type":"string"},"alt":{"type":"string"},"detail":{"type":"string","enum":["low","high","auto"]},"pages":{"type":"string"},"duration_ms":{"type":"integer","minimum":0},"transcript":{"type":"string"}},"required":["type"],"allOf":[{"if":{"properties":{"type":{"const":"text"}},"required":["type"]},"then":{"required":["text"]},"else":{"required":["source","mime_type"]}}]}
```

**`job_summary`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/job_summary.json","type":"object","properties":{"job_id":{"type":"string"},"command":{"type":"string"},"status":{"type":"string","enum":["RUNNING","COMPLETED","FAILED","KILLED","TIMED_OUT"]},"started_at":{"type":"integer","minimum":0},"finished_at":{"type":"integer","minimum":0},"exit_code":{"type":["integer","null"]},"turn_id":{"type":"string"},"tool_call_id":{"type":"string"}},"required":["job_id","status"]}
```

**`SESSION_ATTACH`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_ATTACH.json","type":"object","properties":{"agent_id":{"type":"string","minLength":1},"agent_override":{"type":"object"},"create_if_missing":{"type":"boolean"},"mode":{"type":"string","enum":["controller","observer"]},"takeover":{"type":"boolean"},"resume_from_seq":{"type":"integer","minimum":0},"capabilities":{"type":"object","properties":{"render":{"type":"array","items":{"type":"string","enum":["text","image","document","audio","video"]}},"features":{"type":"array","items":{"type":"string","enum":["approvals","output.parts","presence","input.stream"]}},"client_tools":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/tool_descriptor.json"}}}},"client_info":{"type":"object","properties":{"name":{"type":"string"},"version":{"type":"string"}}},"options":{"type":"object"}},"required":["agent_id"]}
```

**`SESSION_DETACH`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_DETACH.json","type":"object","properties":{"reason":{"type":"string","maxLength":256}}}
```

**`SESSION_CLOSE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_CLOSE.json","type":"object","properties":{"reason":{"type":"string","maxLength":256},"purge":{"type":"boolean"}}}
```

**`USER_INPUT`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/USER_INPUT.json","type":"object","properties":{"prompt":{"type":"string","minLength":1},"content":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/content_part.json"},"minItems":1},"queue_policy":{"type":"string","enum":["reject","queue","interrupt"]},"overrides":{"type":"object"},"output":{"type":"array","items":{"type":"string","enum":["text","image","document","audio","video"]}},"metadata":{"type":"object"},"response_format":{"$ref":"https://raep.vyde.dev/v1/payloads/response_format.json"},"replaces_turn_id":{"type":"string"}},"oneOf":[{"required":["prompt"]},{"required":["content"]}]}
```

**`CANCEL_TURN`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CANCEL_TURN.json","type":"object","properties":{"reason":{"type":"string","maxLength":256}}}
```

**`CANCEL_JOB`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CANCEL_JOB.json","type":"object","properties":{"job_id":{"type":"string"},"reason":{"type":"string","maxLength":256},"grace_ms":{"type":"integer","minimum":0}},"required":["job_id"]}
```

**`TOOL_APPROVAL_RESPONSE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TOOL_APPROVAL_RESPONSE.json","type":"object","properties":{"tool_call_id":{"type":"string"},"decision":{"type":"string","enum":["approve","deny","approve_for_session"]},"reason":{"type":"string","maxLength":256},"approval_id":{"type":"string"}},"required":["decision"],"anyOf":[{"required":["tool_call_id"]},{"required":["approval_id"]}]}
```

**`JOB_LIST`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_LIST.json","type":"object","properties":{"status":{"type":"array","items":{"type":"string","enum":["RUNNING","COMPLETED","FAILED","KILLED","TIMED_OUT"]}},"limit":{"type":"integer","minimum":1,"maximum":100}}}
```

**`JOB_GET`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_GET.json","type":"object","properties":{"job_id":{"type":"string"},"from_index":{"type":"integer","minimum":0}},"required":["job_id"]}
```

**`HISTORY_GET`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/HISTORY_GET.json","type":"object","properties":{"before_turn_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100},"include_rewound":{"type":"boolean"}}}
```

**`ACK`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/ACK.json","type":"object","properties":{"last_seq":{"type":"integer","minimum":0}},"required":["last_seq"]}
```

**`SESSION_ATTACHED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_ATTACHED.json","type":"object","properties":{"created":{"type":"boolean"},"mode":{"type":"string","enum":["controller","observer"]},"last_seq":{"type":"integer","minimum":0},"replay":{"type":"object","properties":{"from_seq":{"type":"integer","minimum":0},"to_seq":{"type":"integer","minimum":0},"truncated":{"type":"boolean"},"lost":{"type":"object","properties":{"from_seq":{"type":"integer","minimum":0},"to_seq":{"type":"integer","minimum":0}},"required":["from_seq","to_seq"]}}},"negotiated_capabilities":{"type":"array","items":{"type":"string"}},"agent":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_summary.json"}},"required":["created","mode","last_seq"]}
```

**`SESSION_CLOSED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_CLOSED.json","type":"object","properties":{"reason":{"type":"string","enum":["client","ttl","retention","admin","parent_closed"]}}}
```

**`TURN_STATE_CHANGED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TURN_STATE_CHANGED.json","type":"object","properties":{"state":{"type":"string","enum":["QUEUED","CALLING_PROVIDER","EXECUTING_TOOL","AWAITING_APPROVAL","COMPLETED","CANCELLED","FAILED"]},"stop_reason":{"type":"string","enum":["end_turn","max_tokens","cancelled","error","tool_limit","budget_exceeded"]},"usage":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"usage_rollup":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"error":{"$ref":"https://raep.vyde.dev/v1/payloads/error_payload.json"},"output":{},"output_schema_valid":{"type":"boolean"}},"required":["state"]}
```

**`TOKEN_DELTA`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TOKEN_DELTA.json","type":"object","properties":{"delta":{"type":"string"},"index":{"type":"integer","minimum":0},"channel":{"type":"string","enum":["text","reasoning"]},"kind":{"type":"string","enum":["summary","raw","redacted"]}},"required":["delta","index"]}
```

**`TOOL_CALL_STARTED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TOOL_CALL_STARTED.json","type":"object","properties":{"tool_call_id":{"type":"string"},"tool_name":{"type":"string"},"arguments":{"type":"object"},"requires_approval":{"type":"boolean"},"executor":{"type":"string","enum":["server","client"]},"attempt":{"type":"integer","minimum":1},"timeout_s":{"type":"integer","minimum":1}},"required":["tool_call_id","tool_name","arguments","requires_approval"]}
```

**`TOOL_OUTPUT_DELTA`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TOOL_OUTPUT_DELTA.json","type":"object","properties":{"tool_call_id":{"type":"string"},"tool_name":{"type":"string"},"stream":{"type":"string","enum":["stdout","stderr"]},"delta":{"type":"string"},"index":{"type":"integer","minimum":0}},"required":["tool_call_id","tool_name","stream","delta","index"]}
```

**`TOOL_CALL_FINISHED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TOOL_CALL_FINISHED.json","type":"object","properties":{"tool_call_id":{"type":"string"},"status":{"type":"string","enum":["OK","ERROR","DENIED","CANCELLED","TIMED_OUT"]},"exit_code":{"type":["integer","null"]},"duration_ms":{"type":"integer","minimum":0},"result_summary":{"type":"string"},"attempts":{"type":"integer","minimum":1},"result_truncated":{"type":"boolean"}},"required":["tool_call_id","status"]}
```

**`TOOL_APPROVAL_REQUEST`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/TOOL_APPROVAL_REQUEST.json","type":"object","properties":{"tool_call_id":{"type":"string"},"tool_name":{"type":"string"},"arguments":{"type":"object"},"risk":{"type":"string","enum":["low","medium","high"]},"expires_at":{"type":"integer","minimum":0},"approval_id":{"type":"string"},"approvers":{"type":"object","properties":{"kind":{"type":"string","enum":["controller","group","user"]},"group_id":{"type":"string"},"quorum":{"type":"integer","minimum":1},"eligible":{"type":"integer","minimum":0}}},"reason":{"type":"string"}},"required":["approval_id","tool_call_id","tool_name","arguments","risk","expires_at"]}
```

**`JOB_STARTED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_STARTED.json","type":"object","properties":{"job_id":{"type":"string"},"tool_call_id":{"type":"string"},"command":{"type":"string"},"detached":{"const":true},"started_at":{"type":"integer","minimum":0},"limits":{"type":"object","properties":{"timeout_s":{"type":"integer","minimum":1},"max_output_bytes":{"type":"integer","minimum":0}}}},"required":["job_id","tool_call_id","command","detached"]}
```

**`JOB_PROGRESS_DELTA`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_PROGRESS_DELTA.json","type":"object","properties":{"stream":{"type":"string","enum":["stdout","stderr"]},"delta":{"type":"string"},"index":{"type":"integer","minimum":0}},"required":["stream","delta","index"]}
```

**`JOB_FINISHED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_FINISHED.json","type":"object","properties":{"status":{"type":"string","enum":["COMPLETED","FAILED","KILLED","TIMED_OUT"]},"exit_code":{"type":["integer","null"]},"signal":{"type":["string","null"]},"summary":{"type":"string"},"output_snapshot":{"type":"string"},"output_truncated":{"type":"boolean"},"output_bytes_total":{"type":"integer","minimum":0},"duration_ms":{"type":"integer","minimum":0},"triggered_new_turn":{"type":"boolean"},"new_turn_id":{"type":["string","null"]},"injected_into_turn_id":{"type":["string","null"]}},"required":["status","exit_code","summary","triggered_new_turn"]}
```

**`JOB_LIST_RESULT`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_LIST_RESULT.json","type":"object","properties":{"jobs":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/job_summary.json"}}},"required":["jobs"]}
```

**`JOB_STATE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/JOB_STATE.json","type":"object","properties":{"job_id":{"type":"string"},"status":{"type":"string","enum":["RUNNING","COMPLETED","FAILED","KILLED","TIMED_OUT"]},"output":{"type":"string"},"next_index":{"type":"integer","minimum":0}},"required":["job_id","status"]}
```

**`HISTORY_PAGE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/HISTORY_PAGE.json","type":"object","properties":{"items":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/history_item.json"}},"has_more":{"type":"boolean"}},"required":["items","has_more"]}
```

**`ERROR`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/ERROR.json","$ref":"https://raep.vyde.dev/v1/payloads/error_payload.json"}
```

### Additional payloads

**`SESSION_FORK`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_FORK.json","type":"object","properties":{"from_turn_id":{"type":"string"},"inclusive":{"type":"boolean"},"title":{"type":"string"},"agent_id":{"type":"string"},"carry_usage":{"type":"boolean"},"include":{"type":"object","properties":{"uploads":{"type":"boolean"},"outputs":{"type":"boolean"},"workspace":{"type":"boolean"}}}}}
```

**`SESSION_REWIND`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_REWIND.json","type":"object","properties":{"to_turn_id":{"type":"string"},"inclusive":{"type":"boolean"},"cancel_pending":{"type":"boolean"},"workspace":{"type":"string","enum":["keep","restore"]}},"required":["to_turn_id"]}
```

**`SESSION_COMPACT`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_COMPACT.json","type":"object","properties":{"strategy":{"type":"string","enum":["summarize","truncate"]},"keep_last_turns":{"type":"integer","minimum":0},"instructions":{"type":"string","maxLength":1024}}}
```

**`CONTEXT_PIN`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CONTEXT_PIN.json","type":"object","properties":{"turn_ids":{"type":"array","items":{"type":"string"},"minItems":1},"pinned":{"type":"boolean"}},"required":["turn_ids"]}
```

**`INPUT_STREAM_START`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/INPUT_STREAM_START.json","type":"object","properties":{"stream_id":{"type":"string"},"type":{"type":"string","enum":["audio","video"]},"mime_type":{"type":"string"},"params":{"type":"object"},"turn_detection":{"type":"string","enum":["manual","server"]}},"required":["stream_id","type","mime_type"]}
```

**`INPUT_STREAM_DATA`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/INPUT_STREAM_DATA.json","type":"object","properties":{"stream_id":{"type":"string"},"index":{"type":"integer","minimum":0},"data":{"type":"string"},"timestamp_ms":{"type":"integer","minimum":0}},"required":["stream_id","index","data"]}
```

**`INPUT_STREAM_END`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/INPUT_STREAM_END.json","type":"object","properties":{"stream_id":{"type":"string"},"commit":{"type":"boolean"}},"required":["stream_id"]}
```

**`CLIENT_TOOL_RESULT`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CLIENT_TOOL_RESULT.json","type":"object","properties":{"tool_call_id":{"type":"string"},"status":{"type":"string","enum":["OK","ERROR"]},"content":{"oneOf":[{"type":"string"},{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/content_part.json"}}]},"result_summary":{"type":"string"}},"required":["tool_call_id","status"]}
```

**`PRESENCE_UPDATE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/PRESENCE_UPDATE.json","type":"object","properties":{"status":{"type":"string","enum":["active","idle","typing"]}},"required":["status"]}
```

**`SESSION_UPDATED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_UPDATED.json","type":"object","properties":{"title":{"type":"string"},"labels":{"type":"object","additionalProperties":{"type":"string"}},"pinned":{"type":"boolean"},"archived":{"type":"boolean"},"updated_at":{"type":"integer","minimum":0}},"required":["updated_at"]}
```

**`SESSION_FORKED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_FORKED.json","type":"object","properties":{"session_id":{"type":"string"},"from_session_id":{"type":"string"},"from_turn_id":{"type":"string"},"title":{"type":"string"}},"required":["session_id","from_session_id","from_turn_id"]}
```

**`USER_INPUT_ACCEPTED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/USER_INPUT_ACCEPTED.json","type":"object","properties":{"turn_id":{"type":"string"},"origin":{"$ref":"https://raep.vyde.dev/v1/payloads/origin.json"},"content":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/content_part.json"},"minItems":1},"position":{"type":"integer","minimum":0},"replaces_turn_id":{"type":"string"}},"required":["turn_id","origin","content"]}
```

**`PROVIDER_RETRY`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/PROVIDER_RETRY.json","type":"object","properties":{"attempt":{"type":"integer","minimum":2},"reason":{"type":"string"},"provider":{"type":"object","properties":{"id":{"type":"string"},"model":{"type":"string"}},"required":["id"]},"fallback":{"type":"boolean"},"discard_from_index":{"type":"integer","minimum":0}},"required":["attempt","reason","provider","fallback"]}
```

**`APPROVAL_RESOLVED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/APPROVAL_RESOLVED.json","type":"object","properties":{"approval_id":{"type":"string"},"tool_call_id":{"type":"string"},"decision":{"type":"string","enum":["approve","deny"]},"decided_by":{"type":"array","items":{"type":"object","properties":{"user_id":{"type":"string"},"decision":{"type":"string","enum":["approve","deny"]},"at":{"type":"integer","minimum":0}},"required":["user_id","decision"]}},"quorum_met":{"type":"boolean"}},"required":["approval_id","tool_call_id","decision","decided_by"]}
```

**`CLIENT_TOOL_REQUEST`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CLIENT_TOOL_REQUEST.json","type":"object","properties":{"tool_call_id":{"type":"string"},"tool_name":{"type":"string"},"arguments":{"type":"object"},"expires_at":{"type":"integer","minimum":0}},"required":["tool_call_id","tool_name","arguments","expires_at"]}
```

**`HISTORY_REWOUND`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/HISTORY_REWOUND.json","type":"object","properties":{"to_turn_id":{"type":"string"},"inclusive":{"type":"boolean"},"rewound_turn_ids":{"type":"array","items":{"type":"string"}},"side_effects":{"type":"array","items":{"type":"object","properties":{"tool_call_id":{"type":"string"},"tool_name":{"type":"string"},"side_effects":{"type":"string","enum":["local","external"]}},"required":["tool_call_id","tool_name","side_effects"]}},"workspace_restored":{"type":"boolean"}},"required":["to_turn_id","inclusive","rewound_turn_ids"]}
```

**`CONTEXT_UPDATED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CONTEXT_UPDATED.json","type":"object","properties":{"pinned_turn_ids":{"type":"array","items":{"type":"string"}}},"required":["pinned_turn_ids"]}
```

**`QUOTA_WARNING`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/QUOTA_WARNING.json","type":"object","properties":{"dimension":{"type":"string"},"used":{"type":"number"},"limit":{"type":"number"},"threshold":{"type":"number","minimum":0,"maximum":100},"resets_at":{"type":"integer","minimum":0}},"required":["dimension","used","limit","threshold"]}
```

**`SERVER_DRAINING`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SERVER_DRAINING.json","type":"object","properties":{"deadline_at":{"type":"integer","minimum":0},"reason":{"type":"string"}},"required":["deadline_at"]}
```

**`PRESENCE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/PRESENCE.json","type":"object","properties":{"participants":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/participant.json"}}},"required":["participants"]}
```

### Management, runs, operations and conformance objects

**`tool_descriptor`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/tool_descriptor.json","type":"object","properties":{"name":{"type":"string","minLength":1},"description":{"type":"string"},"input_schema":{"type":"object"},"output_schema":{"type":"object"},"executor":{"type":"string","enum":["server","client"]},"risk":{"type":"string","enum":["low","medium","high"]},"side_effects":{"type":"string","enum":["none","local","external"]},"idempotent":{"type":"boolean"},"timeout_s":{"type":"integer","minimum":1},"retry":{"type":"object","properties":{"max_attempts":{"type":"integer","minimum":1},"backoff_ms":{"type":"integer","minimum":0},"retry_on":{"type":"array","items":{"type":"string","enum":["TIMED_OUT","ERROR"]}}}},"scopes":{"type":"array","items":{"type":"string"}},"detachable":{"type":"boolean"}},"required":["name","input_schema"]}
```

**`response_format`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/response_format.json","oneOf":[{"type":"object","properties":{"type":{"const":"text"}},"required":["type"]},{"type":"object","properties":{"type":{"const":"json_schema"},"schema":{"type":"object"},"strict":{"type":"boolean"}},"required":["type","schema"]}]}
```

**`participant`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/participant.json","type":"object","properties":{"user_id":{"type":"string"},"display_name":{"type":"string"},"role":{"type":"string","enum":["viewer","editor","owner"]},"mode":{"type":"string","enum":["controller","observer"]},"status":{"type":"string","enum":["active","idle","typing"]},"connected_at":{"type":"integer","minimum":0},"last_seen_at":{"type":"integer","minimum":0}},"required":["user_id","role","mode"]}
```

**`session_summary`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/session_summary.json","type":"object","properties":{"session_id":{"type":"string"},"title":{"type":"string"},"title_source":{"type":"string","enum":["user","auto"]},"labels":{"type":"object","additionalProperties":{"type":"string"}},"pinned":{"type":"boolean"},"archived":{"type":"boolean"},"status":{"type":"string","enum":["ACTIVE","SUSPENDED","CLOSED"]},"agent":{"type":"object","properties":{"id":{"type":"string"},"version":{"type":"string"}},"required":["id"]},"parent":{"type":"object"},"forked_from":{"type":"object"},"created_at":{"type":"integer","minimum":0},"updated_at":{"type":"integer","minimum":0},"last_activity_at":{"type":"integer","minimum":0},"turn_count":{"type":"integer","minimum":0},"usage_total":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"has_pending":{"type":"boolean"}},"required":["session_id","status","agent","created_at","updated_at"]}
```

**`session_list_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/session_list_result.json","type":"object","properties":{"sessions":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/session_summary.json"}},"next_cursor":{"type":["string","null"]}},"required":["sessions"]}
```

**`session_create`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/session_create.json","type":"object","properties":{"agent_id":{"type":"string","minLength":1},"agent_override":{"type":"object"},"title":{"type":"string"},"labels":{"type":"object","additionalProperties":{"type":"string"},"maxProperties":16},"options":{"type":"object"}},"required":["agent_id"]}
```

**`session_patch`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/session_patch.json","type":"object","properties":{"title":{"type":["string","null"]},"labels":{"type":"object","additionalProperties":{"type":["string","null"]}},"pinned":{"type":"boolean"},"archived":{"type":"boolean"}}}
```

**`agent_list_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/agent_list_result.json","type":"object","properties":{"agents":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_summary.json"}},"next_cursor":{"type":["string","null"]}},"required":["agents"]}
```

**`share`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/share.json","type":"object","properties":{"principal":{"type":"string"},"role":{"type":"string","enum":["viewer","editor","owner"]}},"required":["principal","role"]}
```

**`run_request`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/run_request.json","type":"object","properties":{"agent_id":{"type":"string","minLength":1},"session_id":{"type":"string"},"input":{"type":"object","oneOf":[{"required":["prompt"]},{"required":["content"]}],"properties":{"prompt":{"type":"string"},"content":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/content_part.json"},"minItems":1}}},"response_format":{"$ref":"https://raep.vyde.dev/v1/payloads/response_format.json"},"overrides":{"type":"object"},"options":{"type":"object"},"wait_s":{"type":"integer","minimum":0,"maximum":120},"on_approval":{"type":"string","enum":["deny","wait","fail"]},"ephemeral":{"type":"boolean"},"title":{"type":"string"},"labels":{"type":"object","additionalProperties":{"type":"string"}},"metadata":{"type":"object"}},"required":["agent_id","input"]}
```

**`run`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/run.json","type":"object","properties":{"run_id":{"type":"string"},"session_id":{"type":"string"},"turn_id":{"type":"string"},"status":{"type":"string","enum":["queued","running","awaiting_approval","completed","failed","cancelled"]},"output":{"type":"object","properties":{"text":{"type":"string"},"parts":{"type":"array","items":{"type":"object"}},"structured":{}}},"usage":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"error":{"$ref":"https://raep.vyde.dev/v1/payloads/error_payload.json"},"created_at":{"type":"integer","minimum":0},"finished_at":{"type":"integer","minimum":0}},"required":["run_id","session_id","status","created_at"]}
```

**`schedule_request`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/schedule_request.json","type":"object","properties":{"agent_id":{"type":"string"},"input":{"type":"object"},"trigger":{"oneOf":[{"type":"object","properties":{"type":{"const":"cron"},"expression":{"type":"string"},"timezone":{"type":"string"}},"required":["type","expression"]},{"type":"object","properties":{"type":{"const":"interval"},"every_s":{"type":"integer","minimum":1}},"required":["type","every_s"]},{"type":"object","properties":{"type":{"const":"once"},"at":{"type":"integer","minimum":0}},"required":["type","at"]}]},"session_mode":{"type":"string","enum":["new","reuse"]},"session_id":{"type":"string"},"overlap":{"type":"string","enum":["skip","queue","cancel_previous"]},"on_approval":{"type":"string","enum":["deny","wait","fail"]},"catchup":{"type":"string","enum":["none","one"]},"notify":{"$ref":"https://raep.vyde.dev/v1/payloads/notify_config.json"},"enabled":{"type":"boolean"},"max_runs":{"type":"integer","minimum":1},"expires_at":{"type":"integer","minimum":0},"disable_after_failures":{"type":"integer","minimum":1},"title":{"type":"string"},"labels":{"type":"object","additionalProperties":{"type":"string"}}},"required":["agent_id","input","trigger"]}
```

**`schedule`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/schedule.json","type":"object","properties":{"schedule_id":{"type":"string"},"enabled":{"type":"boolean"},"next_run_at":{"type":["integer","null"]},"last_run":{"type":"object","properties":{"run_id":{"type":"string"},"status":{"type":"string"},"at":{"type":"integer","minimum":0}}},"consecutive_failures":{"type":"integer","minimum":0}},"required":["schedule_id","enabled"]}
```

**`trigger_request`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/trigger_request.json","type":"object","properties":{"agent_id":{"type":"string"},"input_template":{"type":"string"},"session_mode":{"type":"string","enum":["new","reuse"]},"on_approval":{"type":"string","enum":["deny","wait","fail"]},"title":{"type":"string"},"labels":{"type":"object","additionalProperties":{"type":"string"}}},"required":["agent_id","input_template"]}
```

**`trigger`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/trigger.json","type":"object","properties":{"trigger_id":{"type":"string"},"secret":{"type":"string"},"fire_url":{"type":"string"},"created_at":{"type":"integer","minimum":0}},"required":["trigger_id"]}
```

**`quota`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/quota.json","type":"object","properties":{"tenant":{"type":"string"},"period":{"type":"object","properties":{"start":{"type":"integer","minimum":0},"end":{"type":"integer","minimum":0}}},"dimensions":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"limit":{"type":"number"},"used":{"type":"number"},"remaining":{"type":"number"},"resets_at":{"type":"integer","minimum":0}},"required":["name","limit","used"]}}},"required":["tenant","dimensions"]}
```

**`deprecation`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/deprecation.json","type":"object","properties":{"name":{"type":"string"},"kind":{"type":"string","enum":["field","event","capability","error_code","enum_value"]},"replacement":{"type":"string"},"sunset_at":{"type":"integer","minimum":0}},"required":["name","kind"]}
```

**`info`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/info.json","type":"object","properties":{"server_version":{"type":"string"},"wire_versions":{"type":"array","items":{"type":"string"},"minItems":1},"capabilities":{"type":"array","items":{"type":"string"}},"limits":{"type":"object"},"transports":{"type":"object","properties":{"ws":{"type":"string"},"sse":{"type":"string"}}},"registries":{"type":"object","additionalProperties":{"type":"string"}},"deprecations":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/deprecation.json"}},"draining":{"type":"boolean"}},"required":["server_version","wire_versions","capabilities"]}
```

**`health`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/health.json","type":"object","properties":{"status":{"type":"string","enum":["ok","ready","draining","degraded"]},"checks":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"status":{"type":"string","enum":["ok","degraded","failed"]},"detail":{"type":"string"}},"required":["name","status"]}}},"required":["status"]}
```

**`conformance_scenario`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/conformance_scenario.json","type":"object","properties":{"id":{"type":"string","pattern":"^C-[0-9]{3}$"},"title":{"type":"string"},"profile":{"type":"string"},"requires":{"type":"array","items":{"type":"string"}},"setup":{"type":"object","properties":{"agent":{"type":"object"},"tools":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/tool_descriptor.json"}},"provider_script":{"type":"array","items":{"type":"object"}}}},"steps":{"type":"array","items":{"oneOf":[{"type":"object","properties":{"send":{"type":"object"}},"required":["send"],"additionalProperties":false},{"type":"object","properties":{"expect":{"type":"object","properties":{"type":{"type":"string"},"payload":{"type":"object"},"envelope":{"type":"object"}},"required":["type"]}},"required":["expect"],"additionalProperties":false},{"type":"object","properties":{"expect_none":{"type":"object","properties":{"type":{"type":"string"},"within_ms":{"type":"integer","minimum":1}},"required":["type","within_ms"]}},"required":["expect_none"],"additionalProperties":false},{"type":"object","properties":{"action":{"type":"object","properties":{"name":{"type":"string","enum":["disconnect","reconnect","crash_server","advance_time","fail_provider","lose_lease"]},"args":{"type":"object"}},"required":["name"]}},"required":["action"],"additionalProperties":false}]},"minItems":1}},"required":["id","title","profile","steps"]}
```

**`conformance_report`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/conformance_report.json","type":"object","properties":{"implementation":{"type":"string"},"version":{"type":"string"},"wire_version":{"type":"string"},"claims":{"type":"array","items":{"type":"string"}},"results":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"result":{"type":"string","enum":["pass","fail","skip"]},"reason":{"type":"string"}},"required":["id","result"]}}},"required":["implementation","version","wire_version","claims","results"]}
```

### Handshake and session state

**`HELLO`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/HELLO.json","type":"object","properties":{"server_version":{"type":"string"},"wire_versions":{"type":"array","items":{"type":"string"},"minItems":1},"capabilities":{"type":"array","items":{"type":"string"}},"limits":{"type":"object","properties":{"max_frame_bytes":{"type":"integer","minimum":1},"max_prompt_bytes":{"type":"integer","minimum":1},"max_concurrent_jobs":{"type":"integer","minimum":0},"replay_buffer_events":{"type":"integer","minimum":1},"job_output_retention_s":{"type":"integer","minimum":0},"max_parts":{"type":"integer","minimum":1},"max_inline_bytes":{"type":"integer","minimum":0},"max_upload_bytes":{"type":"integer","minimum":0},"max_inspect_bytes":{"type":"integer","minimum":1024},"max_result_bytes":{"type":"integer","minimum":1024},"child_retention_s":{"type":"integer","minimum":0},"max_notify_targets":{"type":"integer","minimum":0},"max_output_part_bytes":{"type":"integer","minimum":0},"max_export_bytes":{"type":"integer","minimum":0},"export_ttl_s":{"type":"integer","minimum":1},"max_queued_inputs":{"type":"integer","minimum":0},"session_retention_s":{"type":"integer","minimum":0},"max_commands_per_s":{"type":"integer","minimum":1},"max_client_tool_result_bytes":{"type":"integer","minimum":0},"max_input_stream_bytes":{"type":"integer","minimum":0},"max_workspace_bytes":{"type":"integer","minimum":0},"max_snapshots":{"type":"integer","minimum":0},"max_participants":{"type":"integer","minimum":1},"max_tool_result_bytes":{"type":"integer","minimum":0},"run_retention_s":{"type":"integer","minimum":0},"max_pinned_turns":{"type":"integer","minimum":0}}},"heartbeat_s":{"type":"integer","minimum":1},"draining":{"type":"boolean"},"deprecations":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/deprecation.json"}}},"required":["server_version","wire_versions","capabilities","limits"]}
```

**`SESSION_STATE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_STATE.json","type":"object","properties":{"status":{"type":"string","enum":["ACTIVE","SUSPENDED","CLOSED"]},"history_summary":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/history_item.json"}},"usage_total":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"usage_rollup":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"budget_remaining":{"$ref":"https://raep.vyde.dev/v1/payloads/budget_map.json"},"parent":{"type":"object","properties":{"session_id":{"type":"string"},"turn_id":{"type":"string"},"tool_call_id":{"type":"string"},"depth":{"type":"integer","minimum":1}},"required":["session_id","tool_call_id"]},"children":{"type":"array","items":{"type":"object","properties":{"child_session_id":{"type":"string"},"agent_id":{"type":"string"},"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"lifetime":{"type":"string","enum":["turn","session"]}},"required":["child_session_id","state"]}},"delegator_suspended":{"type":"boolean"},"notify":{"$ref":"https://raep.vyde.dev/v1/payloads/notify_summary.json"},"imported_from":{"$ref":"https://raep.vyde.dev/v1/payloads/imported_from.json"},"title":{"type":"string"},"labels":{"type":"object","additionalProperties":{"type":"string"}},"pinned":{"type":"boolean"},"archived":{"type":"boolean"},"forked_from":{"type":"object","properties":{"session_id":{"type":"string"},"turn_id":{"type":"string"},"at":{"type":"integer","minimum":0}},"required":["session_id","turn_id"]},"pinned_turn_ids":{"type":"array","items":{"type":"string"}},"participants":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/participant.json"}},"workspace":{"type":"object","properties":{"snapshots":{"type":"boolean"},"last_snapshot_turn_id":{"type":"string"},"bytes":{"type":"integer","minimum":0}}}},"required":["status","history_summary"]}
```

**`notify_summary`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/notify_summary.json","type":"object","properties":{"channel":{"type":"string"},"endpoint_id":{"type":"string"},"url_host":{"type":"string"},"events":{"type":"array","items":{"type":"string"}},"when":{"type":"string"},"include":{"type":"string"},"include_children":{"type":"boolean"},"coalesce_s":{"type":"integer","minimum":0},"state":{"type":"string","enum":["active","failing","disabled"]}},"required":["channel","events","state"]}
```

**`imported_from`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/imported_from.json","type":"object","properties":{"exporter":{"type":"string"},"original_session_id":{"type":"string"},"exported_at":{"type":"integer"},"imported_at":{"type":"integer"}},"required":["original_session_id","exported_at"]}
```

### Authentication, configuration and notices

**`AUTH_EXPIRING`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/AUTH_EXPIRING.json","type":"object","properties":{"expires_at":{"type":"integer"},"refresh_before":{"type":"integer"}},"required":["expires_at"]}
```

**`AUTH_REFRESH`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/AUTH_REFRESH.json","type":"object","properties":{"token":{"type":"string","minLength":1}},"required":["token"]}
```

**`AUTH_REFRESHED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/AUTH_REFRESHED.json","type":"object","properties":{"expires_at":{"type":"integer"}},"required":["expires_at"]}
```

**`SESSION_CONFIGURE`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_CONFIGURE.json","type":"object","properties":{"agent_id":{"type":"string"},"agent_override":{"type":"object"},"options":{"type":"object"},"apply":{"type":"string","enum":["now","next_turn"]}}}
```

**`SESSION_CONFIGURED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_CONFIGURED.json","type":"object","properties":{"agent":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_summary.json"},"applied":{"type":"boolean"}},"required":["agent"]}
```

**`AGENT_CHANGED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/AGENT_CHANGED.json","type":"object","properties":{"from_agent":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_ref.json"},"to_agent":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_ref.json"},"reason":{"type":"string"},"effective":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_summary.json"}},"required":["from_agent","to_agent"]}
```

**`BUDGET_WARNING`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/BUDGET_WARNING.json","type":"object","properties":{"scope":{"type":"string","enum":["turn","session"]},"metric":{"type":"string","enum":["tokens","cost","tool_calls","duration"]},"used":{"type":"number","minimum":0},"limit":{"type":"number","minimum":0},"threshold":{"type":"integer","minimum":1,"maximum":100}},"required":["scope","metric","used","limit","threshold"]}
```

**`HISTORY_COMPACTED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/HISTORY_COMPACTED.json","type":"object","properties":{"up_to_turn_id":{"type":"string"},"strategy":{"type":"string","enum":["summarize","truncate"]},"tokens_before":{"type":"integer","minimum":0},"tokens_after":{"type":"integer","minimum":0},"trigger":{"type":"string","enum":["auto","manual"]}},"required":["up_to_turn_id","strategy"]}
```

### Delegation: events and results

**`child_state`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/child_state.json","type":"string","enum":["RUNNING","AWAITING_APPROVAL","AWAITING_INPUT","IDLE","FAILED","STOPPED","CLOSED"]}
```

**`pending`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/pending.json","type":"object","properties":{"kind":{"type":"string","enum":["approval","input"]},"tool_call_id":{"type":"string"},"tool_name":{"type":"string"},"risk":{"type":"string","enum":["low","medium","high"]},"question":{"type":"string","maxLength":1024},"options":{"type":"array","items":{"type":"string"}},"expires_at":{"type":"integer"},"routed_to":{"type":"string","enum":["human","parent"]}},"required":["kind","tool_call_id"]}
```

**`child_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/child_result.json","type":"object","properties":{"status":{"type":"string","enum":["completed","failed","stopped","budget_exceeded"]},"summary":{"type":"string","maxLength":2048},"summary_truncated":{"type":"boolean"},"output":{},"output_schema_valid":{"type":"boolean"},"artifacts":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"mime_type":{"type":"string"},"bytes":{"type":"integer","minimum":0},"source":{"type":"object","properties":{"kind":{"type":"string","enum":["upload"]},"upload_id":{"type":"string"}},"required":["kind","upload_id"]}},"required":["name","source"]}},"usage":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"error":{"$ref":"https://raep.vyde.dev/v1/payloads/error_payload.json"},"final_seq":{"type":"integer","minimum":0},"child_turn_id":{"type":"string"}},"required":["status","summary"]}
```

**`CHILD_SESSION_STARTED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CHILD_SESSION_STARTED.json","type":"object","properties":{"child_session_id":{"type":"string"},"agent_id":{"type":"string"},"tool_call_id":{"type":"string"},"lifetime":{"type":"string","enum":["turn","session"]},"task_summary":{"type":"string","maxLength":512},"depth":{"type":"integer","minimum":1},"name":{"type":"string","maxLength":64},"budget_reserved":{"$ref":"https://raep.vyde.dev/v1/payloads/budget_map.json"}},"required":["child_session_id","agent_id","tool_call_id"]}
```

**`CHILD_SESSION_UPDATED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/CHILD_SESSION_UPDATED.json","type":"object","properties":{"child_session_id":{"type":"string"},"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"reason":{"type":"string","enum":["state_change","approval_pending","approval_escalated","input_requested","budget_warning","user_message","turn_finished","delegator_control","error"]},"summary":{"type":"string","maxLength":1024},"usage":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"budget_remaining":{"$ref":"https://raep.vyde.dev/v1/payloads/budget_map.json"},"pending":{"$ref":"https://raep.vyde.dev/v1/payloads/pending.json"},"result":{"$ref":"https://raep.vyde.dev/v1/payloads/child_result.json"},"child_seq":{"type":"integer","minimum":0},"error":{"$ref":"https://raep.vyde.dev/v1/payloads/error_payload.json"}},"required":["child_session_id","state","reason"]}
```

**`DELEGATOR_CONTROL`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/DELEGATOR_CONTROL.json","type":"object","properties":{"action":{"type":"string","enum":["suspend","release"]},"reason":{"type":"string","maxLength":256}},"required":["action"]}
```

**`DELEGATOR_STATE_CHANGED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/DELEGATOR_STATE_CHANGED.json","type":"object","properties":{"suspended":{"type":"boolean"},"by":{"type":"string","enum":["client","server"]},"queued_messages":{"type":"integer","minimum":0},"reason":{"type":"string","maxLength":256}},"required":["suspended"]}
```

### Delegation: agent tool arguments and results

**`spawn_agent_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/spawn_agent_args.json","type":"object","properties":{"agent_id":{"type":"string","minLength":1},"task":{"oneOf":[{"type":"string","minLength":1},{"type":"array","items":{"type":"object"},"minItems":1}]},"name":{"type":"string","maxLength":64},"lifetime":{"type":"string","enum":["turn","session"]},"budget_share":{"oneOf":[{"type":"number","exclusiveMinimum":0,"maximum":1},{"$ref":"https://raep.vyde.dev/v1/payloads/budget_map.json"}]},"scopes":{"type":"array","items":{"type":"string"}},"inspect":{"type":"string","enum":["summary","events","full"]},"output_schema":{"type":"object"},"context":{"type":"object","properties":{"mode":{"type":"string","enum":["none","summary"]},"attachments":{"type":"array","items":{"type":"string"},"maxItems":16}}},"workspace":{"type":"string","enum":["isolated","shared_ro","shared_rw"]},"ttl_s":{"type":"integer","minimum":1}},"required":["agent_id","task"]}
```

**`spawn_agent_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/spawn_agent_result.json","type":"object","properties":{"child_session_id":{"type":"string"},"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"depth":{"type":"integer","minimum":1},"budget_reserved":{"$ref":"https://raep.vyde.dev/v1/payloads/budget_map.json"},"agent":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_summary.json"}},"required":["child_session_id","state","depth"]}
```

**`child_status`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/child_status.json","type":"object","properties":{"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"activity":{"type":"string","maxLength":512},"last_output_excerpt":{"type":"string","maxLength":2048},"pending":{"$ref":"https://raep.vyde.dev/v1/payloads/pending.json"},"usage":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"usage_rollup":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"},"budget_remaining":{"$ref":"https://raep.vyde.dev/v1/payloads/budget_map.json"},"jobs":{"type":"array","items":{"type":"object","properties":{"job_id":{"type":"string"},"command":{"type":"string"},"status":{"type":"string"}},"required":["job_id","status"]}},"children":{"type":"array","items":{"type":"object","properties":{"child_session_id":{"type":"string"},"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"}},"required":["child_session_id","state"]}},"last_seq":{"type":"integer","minimum":0},"delegator_suspended":{"type":"boolean"},"updated_at":{"type":"integer"}},"required":["state"]}
```

**`inspect_agent_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/inspect_agent_args.json","type":"object","properties":{"child_session_id":{"type":"string"},"view":{"type":"string","enum":["status","transcript","events","jobs"]},"from_seq":{"type":"integer","minimum":0},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":200},"job_id":{"type":"string"},"from_index":{"type":"integer","minimum":0}},"required":["child_session_id"]}
```

**`inspect_agent_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/inspect_agent_result.json","type":"object","properties":{"view":{"type":"string","enum":["status","transcript","events","jobs"]},"child_session_id":{"type":"string"},"status":{"$ref":"https://raep.vyde.dev/v1/payloads/child_status.json"},"items":{"type":"array","items":{"type":"object"}},"next_cursor":{"type":["string","null"]},"next_seq":{"type":"integer","minimum":0},"truncated":{"type":"boolean"},"bytes":{"type":"integer","minimum":0}},"required":["view","child_session_id"]}
```

**`message_agent_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/message_agent_args.json","type":"object","properties":{"child_session_id":{"type":"string"},"content":{"oneOf":[{"type":"string","minLength":1},{"type":"array","items":{"type":"object"},"minItems":1}]},"delivery":{"type":"string","enum":["steer","queue","interrupt"]}},"required":["child_session_id","content"]}
```

**`message_agent_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/message_agent_result.json","type":"object","properties":{"accepted":{"type":"boolean"},"delivery":{"type":"string","enum":["steer","queue","interrupt","answer"]},"child_state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"held":{"type":"boolean"}},"required":["accepted","child_state"]}
```

**`stop_agent_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/stop_agent_args.json","type":"object","properties":{"child_session_id":{"type":"string"},"mode":{"type":"string","enum":["graceful","kill"]},"reason":{"type":"string","maxLength":512},"timeout_s":{"type":"integer","minimum":1}},"required":["child_session_id"]}
```

**`stop_agent_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/stop_agent_result.json","type":"object","properties":{"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"result":{"$ref":"https://raep.vyde.dev/v1/payloads/child_result.json"}},"required":["state"]}
```

**`wait_agent_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/wait_agent_args.json","type":"object","properties":{"child_session_id":{"type":"string"},"until":{"type":"string","enum":["idle","finished"]},"timeout_s":{"type":"integer","minimum":1}},"required":["child_session_id"]}
```

**`wait_agent_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/wait_agent_result.json","type":"object","properties":{"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"timed_out":{"type":"boolean"},"result":{"$ref":"https://raep.vyde.dev/v1/payloads/child_result.json"}},"required":["state","timed_out"]}
```

**`list_agents_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/list_agents_args.json","type":"object","properties":{"state":{"type":"array","items":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"}},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}}}
```

**`list_agents_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/list_agents_result.json","type":"object","properties":{"children":{"type":"array","items":{"type":"object","properties":{"child_session_id":{"type":"string"},"agent_id":{"type":"string"},"name":{"type":"string"},"state":{"$ref":"https://raep.vyde.dev/v1/payloads/child_state.json"},"task_summary":{"type":"string","maxLength":512},"depth":{"type":"integer","minimum":1},"activity":{"type":"string","maxLength":512},"usage":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"}},"required":["child_session_id","agent_id","state"]}},"next_cursor":{"type":["string","null"]}},"required":["children"]}
```

**`approve_child_call_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/approve_child_call_args.json","type":"object","properties":{"child_session_id":{"type":"string"},"tool_call_id":{"type":"string"},"decision":{"type":"string","enum":["approve","deny"]},"reason":{"type":"string","maxLength":512}},"required":["child_session_id","tool_call_id","decision"]}
```

**`ask_delegator_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/ask_delegator_args.json","type":"object","properties":{"question":{"type":"string","minLength":1,"maxLength":4096},"options":{"type":"array","items":{"type":"string"},"maxItems":10},"timeout_s":{"type":"integer","minimum":1}},"required":["question"]}
```

**`ask_delegator_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/ask_delegator_result.json","type":"object","properties":{"answered":{"type":"boolean"},"answer":{"type":"string"},"answered_by":{"type":"string","enum":["parent","user"]},"timed_out":{"type":"boolean"}},"required":["answered"]}
```

**`tool_error`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/tool_error.json","type":"object","properties":{"code":{"type":"string","enum":["TOOL_ERROR","TOOL_DENIED","SCOPE_DENIED","BUDGET_EXCEEDED","SANDBOX_LIMIT_EXCEEDED","AGENT_NOT_FOUND","HANDOFF_NOT_ALLOWED","DELEGATE_NOT_ALLOWED","CHILD_NOT_FOUND","CHILD_NOT_ACTIVE","DELEGATOR_SUSPENDED","OUTPUT_SCHEMA_INVALID","APPROVAL_TIMEOUT","APPROVAL_NOT_PENDING","RISK_TOO_HIGH","DELEGATOR_TIMEOUT","INVALID_ARGUMENT","TOOL_TIMEOUT","TOOL_INTERRUPTED","REMOTE_UNAVAILABLE"]},"message":{"type":"string"},"details":{"type":"object"}},"required":["code","message"]}
```

### Handoff

**`handoff_agent_args`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/handoff_agent_args.json","type":"object","properties":{"agent_id":{"type":"string","minLength":1},"reason":{"type":"string","maxLength":512},"carry_context":{"type":"string","enum":["full","summary","none"]}},"required":["agent_id"]}
```

**`handoff_agent_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/handoff_agent_result.json","type":"object","properties":{"accepted":{"type":"boolean"},"takes_effect":{"const":"turn_end"}},"required":["accepted"]}
```

### Out-of-band notifications

**`notify_config`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/notify_config.json","type":"object","properties":{"channel":{"type":"string","enum":["webhook"]},"endpoint_id":{"type":"string"},"url":{"type":"string","pattern":"^https://"},"secret":{"type":"string","minLength":32},"events":{"type":"array","items":{"type":"string","enum":["TURN_STATE_CHANGED","JOB_FINISHED","TOOL_APPROVAL_REQUEST","CHILD_SESSION_UPDATED","BUDGET_WARNING","SESSION_CLOSED","ERROR"]},"minItems":1},"when":{"type":"string","enum":["detached","always"]},"include":{"type":"string","enum":["none","summary"]},"include_children":{"type":"boolean"},"coalesce_s":{"type":"integer","minimum":0,"maximum":3600}},"required":["channel","events"],"oneOf":[{"required":["endpoint_id"]},{"required":["url"]}]}
```

**`NOTIFICATION_STATUS`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/NOTIFICATION_STATUS.json","type":"object","properties":{"notify_state":{"type":"string","enum":["active","failing","disabled"]},"consecutive_failures":{"type":"integer","minimum":0},"last_error":{"type":"string","maxLength":256},"last_attempt_at":{"type":"integer"},"undelivered":{"type":"integer","minimum":0}},"required":["notify_state"]}
```

**`webhook_body`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/webhook_body.json","type":"object","properties":{"notification_id":{"type":"string"},"session_id":{"type":"string"},"root_session_id":{"type":"string"},"seq":{"type":"integer","minimum":1},"type":{"type":"string"},"turn_id":{"type":["string","null"]},"job_id":{"type":["string","null"]},"child_session_id":{"type":["string","null"]},"occurred_at":{"type":"integer"},"summary":{"type":"string","maxLength":256},"attempt":{"type":"integer","minimum":1}},"required":["notification_id","session_id","root_session_id","seq","type","occurred_at"]}
```

### Output parts

**`OUTPUT_PART_STARTED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/OUTPUT_PART_STARTED.json","type":"object","properties":{"part_id":{"type":"string"},"index":{"type":"integer","minimum":0},"type":{"type":"string","enum":["image","audio","video","document"]},"mime_type":{"type":"string"},"name":{"type":"string"},"alt":{"type":"string"},"duration_ms":{"type":"integer","minimum":0},"width":{"type":"integer","minimum":1},"height":{"type":"integer","minimum":1},"bytes_expected":{"type":"integer","minimum":0},"streamed":{"type":"boolean"}},"required":["part_id","index","type","mime_type","streamed"]}
```

**`OUTPUT_PART_DELTA`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/OUTPUT_PART_DELTA.json","type":"object","properties":{"part_id":{"type":"string"},"index":{"type":"integer","minimum":0},"data":{"type":"string"}},"required":["part_id","index","data"]}
```

**`OUTPUT_PART_FINISHED`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/OUTPUT_PART_FINISHED.json","type":"object","properties":{"part_id":{"type":"string"},"status":{"type":"string","enum":["OK","ERROR","CANCELLED"]},"source":{"type":"object","properties":{"kind":{"type":"string","enum":["output","url"]},"output_id":{"type":"string"},"url":{"type":"string"},"expires_at":{"type":"integer"}},"required":["kind"]},"bytes":{"type":"integer","minimum":0},"sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"transcript":{"type":"string"},"error":{"$ref":"https://raep.vyde.dev/v1/payloads/error_payload.json"}},"required":["part_id","status"]}
```

### Session export and import

**`SESSION_EXPORT`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_EXPORT.json","type":"object","properties":{"include":{"type":"object","properties":{"children":{"type":"boolean"},"uploads":{"type":"boolean"},"job_output":{"type":"boolean"},"outputs":{"type":"boolean"},"workspace":{"type":"boolean"}}},"quiesce":{"type":"string","enum":["reject","wait","kill_jobs"]},"wait_timeout_s":{"type":"integer","minimum":1},"encrypt":{"type":"object","properties":{"recipient_jwk":{"type":"object"}},"required":["recipient_jwk"]}}}
```

**`SESSION_EXPORT_READY`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/SESSION_EXPORT_READY.json","type":"object","properties":{"export_id":{"type":"string"},"download_url":{"type":"string"},"bytes":{"type":"integer","minimum":0},"sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"expires_at":{"type":"integer"},"sessions":{"type":"integer","minimum":1},"encrypted":{"type":"boolean"},"signed":{"type":"boolean"}},"required":["export_id","download_url","bytes","sha256","expires_at"]}
```

**`bundle_manifest`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/bundle_manifest.json","type":"object","properties":{"bundle_version":{"const":"1"},"raep_document_version":{"type":"string"},"wire_version":{"type":"string"},"exported_at":{"type":"integer"},"exporter":{"type":"object","properties":{"server":{"type":"string"},"kid":{"type":"string"}}},"root_session_id":{"type":"string"},"sessions":{"type":"array","items":{"type":"object","properties":{"session_id":{"type":"string"},"parent_session_id":{"type":["string","null"]},"agent":{"$ref":"https://raep.vyde.dev/v1/payloads/agent_ref.json"},"state":{"type":"string"},"history_file":{"type":"string"},"last_seq":{"type":"integer","minimum":0},"usage_total":{"$ref":"https://raep.vyde.dev/v1/payloads/usage.json"}},"required":["session_id","parent_session_id","agent","history_file"]},"minItems":1},"files":{"type":"array","items":{"type":"object","properties":{"path":{"type":"string"},"sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"},"bytes":{"type":"integer","minimum":0}},"required":["path","sha256","bytes"]}},"excluded":{"type":"array","items":{"type":"string"}}},"required":["bundle_version","exported_at","root_session_id","sessions","files"]}
```

**`import_report`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/import_report.json","type":"object","properties":{"import_id":{"type":"string"},"bundle_version":{"type":"string"},"sessions":{"type":"integer","minimum":1},"signature":{"type":"string","enum":["valid","invalid","absent","untrusted"]},"agents":{"type":"array","items":{"type":"object","properties":{"agent_id":{"type":"string"},"version":{"type":"string"},"available":{"type":"boolean"}},"required":["agent_id","available"]}},"warnings":{"type":"array","items":{"type":"string"}},"expires_at":{"type":"integer"}},"required":["import_id","signature","agents"]}
```

**`import_commit_request`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/import_commit_request.json","type":"object","properties":{"agent_map":{"type":"object","additionalProperties":{"type":"string"}},"carry_usage":{"type":"boolean"},"decryption_jwk":{"type":"object"}}}
```

**`import_result`**

```json
{"$id":"https://raep.vyde.dev/v1/payloads/import_result.json","type":"object","properties":{"session_id":{"type":"string"},"id_map":{"type":"object","properties":{"sessions":{"type":"object","additionalProperties":{"type":"string"}},"uploads":{"type":"object","additionalProperties":{"type":"string"}},"outputs":{"type":"object","additionalProperties":{"type":"string"}}}},"state":{"type":"string","enum":["SUSPENDED"]},"warnings":{"type":"array","items":{"type":"string"}}},"required":["session_id","id_map"]}
```

## Appendix C. Conformance Vectors

### C.1 Frame vectors

Each vector is `{id, description, valid, reason, frame}`. A frame with `valid: true` MUST pass `https://raep.vyde.dev/v1/frame.json`; a frame with `valid: false` MUST fail it, for the stated reason.

**Valid frames**

```json
[
{"id":"V-001","description":"Connection-level HELLO without seq","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V001","ref":null,"session_id":"","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"HELLO","payload":{"server_version":"example-server 1.0.0","wire_versions":["1.0"],"capabilities":["transport.ws","jobs"],"limits":{"max_frame_bytes":1048576}}}},
{"id":"V-002","description":"SESSION_ATTACH asking the server to allocate the id","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V002","ref":null,"session_id":"","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"SESSION_ATTACH","payload":{"agent_id":"example-agent","mode":"controller","resume_from_seq":0}}},
{"id":"V-003","description":"USER_INPUT with an uploaded image","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V003","ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"USER_INPUT","payload":{"content":[{"type":"text","text":"What is wrong here?"},{"type":"image","source":{"kind":"upload","upload_id":"upl_7f3a"},"mime_type":"image/png"}]}}},
{"id":"V-004","description":"TOKEN_DELTA on the reasoning channel","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V004","seq":5,"ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"TOKEN_DELTA","payload":{"delta":"Checking the log","index":0,"channel":"reasoning","kind":"summary"}}},
{"id":"V-005","description":"JOB_FINISHED for a process killed by a signal","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V005","seq":9,"ref":null,"session_id":"sess_88392","turn_id":null,"job_id":"job_1","timestamp":1793404900000,"type":"JOB_FINISHED","payload":{"status":"KILLED","exit_code":null,"signal":"SIGTERM","summary":"terminated by user","triggered_new_turn":false}}},
{"id":"V-006","description":"TOOL_CALL_STARTED for a client tool","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V006","seq":6,"ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"TOOL_CALL_STARTED","payload":{"tool_call_id":"call_1","tool_name":"read_file","arguments":{"path":"a.txt"},"requires_approval":false,"executor":"client","attempt":1}}},
{"id":"V-007","description":"TOOL_APPROVAL_RESPONSE answering by approval id","valid":true,"frame":{"version":"1.0","id":"01JB00000000000000000V007","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"TOOL_APPROVAL_RESPONSE","payload":{"approval_id":"apr_1","decision":"approve"}}}
]
```

**Invalid frames**

```json
[
{"id":"V-101","description":"USER_INPUT with both prompt and content","valid":false,"reason":"exactly one of prompt and content","frame":{"version":"1.0","id":"01JB00000000000000000V101","ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"USER_INPUT","payload":{"prompt":"hi","content":[{"type":"text","text":"hi"}]}}},
{"id":"V-102","description":"USER_INPUT with neither prompt nor content","valid":false,"reason":"exactly one of prompt and content","frame":{"version":"1.0","id":"01JB00000000000000000V102","ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"USER_INPUT","payload":{"queue_policy":"queue"}}},
{"id":"V-103","description":"TOKEN_DELTA without index","valid":false,"reason":"index is required","frame":{"version":"1.0","id":"01JB00000000000000000V103","seq":5,"ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"TOKEN_DELTA","payload":{"delta":"x"}}},
{"id":"V-104","description":"JOB_STARTED with detached false","valid":false,"reason":"detached is always true","frame":{"version":"1.0","id":"01JB00000000000000000V104","seq":7,"ref":null,"session_id":"sess_88392","turn_id":null,"job_id":"job_1","timestamp":1793404900000,"type":"JOB_STARTED","payload":{"job_id":"job_1","tool_call_id":"call_1","command":"make","detached":false}}},
{"id":"V-105","description":"A url source over plain HTTP","valid":false,"reason":"url sources are HTTPS only","frame":{"version":"1.0","id":"01JB00000000000000000V105","ref":null,"session_id":"sess_88392","turn_id":"turn_1","job_id":null,"timestamp":1793404900000,"type":"USER_INPUT","payload":{"content":[{"type":"image","source":{"kind":"url","url":"http://example.com/a.png"},"mime_type":"image/png"}]}}},
{"id":"V-106","description":"Envelope with an unknown top-level property","valid":false,"reason":"the envelope allows only its defined fields and ext","frame":{"version":"1.0","id":"01JB00000000000000000V106","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"ACK","payload":{"last_seq":3},"extra":true}},
{"id":"V-107","description":"Envelope without session_id","valid":false,"reason":"session_id is required","frame":{"version":"1.0","id":"01JB00000000000000000V107","ref":null,"turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"ACK","payload":{"last_seq":3}}},
{"id":"V-108","description":"TOOL_APPROVAL_RESPONSE with neither tool_call_id nor approval_id","valid":false,"reason":"tool_call_id or approval_id is required","frame":{"version":"1.0","id":"01JB00000000000000000V108","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"TOOL_APPROVAL_RESPONSE","payload":{"decision":"deny"}}},
{"id":"V-109","description":"INPUT_STREAM_DATA with a negative index","valid":false,"reason":"index starts at 0","frame":{"version":"1.0","id":"01JB00000000000000000V109","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"INPUT_STREAM_DATA","payload":{"stream_id":"s1","index":-1,"data":"AAAA"}}},
{"id":"V-110","description":"SESSION_REWIND without to_turn_id","valid":false,"reason":"to_turn_id is required","frame":{"version":"1.0","id":"01JB00000000000000000V110","ref":null,"session_id":"sess_88392","turn_id":null,"job_id":null,"timestamp":1793404900000,"type":"SESSION_REWIND","payload":{"inclusive":true}}}
]
```

### C.2 Scenario examples

Each scenario is a `conformance_scenario` (Section 31.1). A harness fills in ids and timestamps; values the scenario does not mention are not compared.

**`C-013`**

```json
{"id":"C-013","title":"Cancel after the turn finished returns TURN_NOT_ACTIVE","profile":"Core","requires":[],"setup":{"agent":{"id":"test-agent","tools":{"builtin":["echo"]}},"provider_script":[{"output":"done"}]},"steps":[{"send":{"type":"SESSION_ATTACH","session_id":"","payload":{"agent_id":"test-agent"}}},{"expect":{"type":"HELLO"}},{"expect":{"type":"SESSION_ATTACHED","payload":{"created":true}}},{"expect":{"type":"SESSION_STATE"}},{"send":{"type":"USER_INPUT","turn_id":"t1","payload":{"prompt":"hi"}}},{"expect":{"type":"TURN_STATE_CHANGED","payload":{"state":"COMPLETED"},"envelope":{"turn_id":"t1"}}},{"send":{"type":"CANCEL_TURN","turn_id":"t1","payload":{}}},{"expect":{"type":"ERROR","payload":{"code":"TURN_NOT_ACTIVE"}}},{"expect_none":{"type":"TURN_STATE_CHANGED","within_ms":500}}]}
```

**`C-041`**

```json
{"id":"C-041","title":"Resume replays exactly the missed events","profile":"Resilient","requires":["resume"],"setup":{"agent":{"id":"test-agent","tools":{"builtin":["echo"]}},"provider_script":[{"output":"one two three"}]},"steps":[{"send":{"type":"SESSION_ATTACH","session_id":"","payload":{"agent_id":"test-agent"}}},{"expect":{"type":"SESSION_ATTACHED","envelope":{"seq":{"$capture":"attached_seq"}}}},{"expect":{"type":"SESSION_STATE"}},{"send":{"type":"USER_INPUT","turn_id":"t1","payload":{"prompt":"go"}}},{"expect":{"type":"TOKEN_DELTA","envelope":{"seq":{"$capture":"last_seen"}}}},{"action":{"name":"disconnect"}},{"action":{"name":"reconnect"}},{"send":{"type":"SESSION_ATTACH","payload":{"agent_id":"test-agent","resume_from_seq":{"$use":"last_seen"}}}},{"expect":{"type":"SESSION_ATTACHED","payload":{"replay":{"truncated":false}}}},{"expect":{"type":"TURN_STATE_CHANGED","payload":{"state":"COMPLETED"}}}]}
```

**`C-043`**

```json
{"id":"C-043","title":"Crash: lost range, interrupted turn, interrupted tool call","profile":"Resilient","requires":["resume","jobs"],"setup":{"agent":{"id":"test-agent","tools":{"builtin":["echo"]}},"provider_script":[{"tool_call":{"name":"echo","arguments":{"text":"x"},"hang":true}}]},"steps":[{"send":{"type":"SESSION_ATTACH","session_id":"","payload":{"agent_id":"test-agent"}}},{"expect":{"type":"SESSION_ATTACHED"}},{"expect":{"type":"SESSION_STATE"}},{"send":{"type":"USER_INPUT","turn_id":"t1","payload":{"prompt":"go"}}},{"expect":{"type":"TOOL_CALL_STARTED","payload":{"tool_name":"echo"}}},{"action":{"name":"crash_server"}},{"action":{"name":"reconnect"}},{"send":{"type":"SESSION_ATTACH","payload":{"agent_id":"test-agent","resume_from_seq":3}}},{"expect":{"type":"SESSION_ATTACHED","payload":{"replay":{"lost":{"from_seq":{"$exists":true}}}}}},{"expect":{"type":"TOOL_CALL_FINISHED","payload":{"status":"ERROR","result_summary":"interrupted"}}},{"expect":{"type":"TURN_STATE_CHANGED","payload":{"state":"FAILED","error":{"code":"INTERNAL","details":{"reason":"interrupted"}}}}}]}
```

**`C-130`**

```json
{"id":"C-130","title":"Client-side tool: request, result, finish","profile":"Client tools","requires":["client_tools"],"setup":{"agent":{"id":"test-agent","tools":{"client":true}},"provider_script":[{"tool_call":{"name":"read_file","arguments":{"path":"a.txt"}}},{"output":"ok"}]},"steps":[{"send":{"type":"SESSION_ATTACH","session_id":"","payload":{"agent_id":"test-agent","capabilities":{"client_tools":[{"name":"read_file","input_schema":{"type":"object"},"executor":"client"}]}}}},{"expect":{"type":"SESSION_ATTACHED"}},{"expect":{"type":"SESSION_STATE"}},{"send":{"type":"USER_INPUT","turn_id":"t1","payload":{"prompt":"read it"}}},{"expect":{"type":"TOOL_CALL_STARTED","payload":{"executor":"client"}}},{"expect":{"type":"CLIENT_TOOL_REQUEST","payload":{"tool_name":"read_file","tool_call_id":{"$capture":"call"}}}},{"send":{"type":"CLIENT_TOOL_RESULT","turn_id":"t1","payload":{"tool_call_id":{"$use":"call"},"status":"OK","content":"hello"}}},{"expect":{"type":"TOOL_CALL_FINISHED","payload":{"status":"OK"}}},{"expect":{"type":"TURN_STATE_CHANGED","payload":{"state":"COMPLETED"}}}]}
```
