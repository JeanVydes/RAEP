# Transport

Two bindings, one envelope. An implementation may offer either or both;
both are advertised in `HELLO.capabilities` as `transport.ws` / `transport.sse`.

## WebSocket ([RFC §4.1](../rfc.md#41-connection))

- Endpoint `/raep/v1/ws`, `wss://` (TLS 1.2 min, 1.3 RECOMMENDED).
- Subprotocol `raep.v1`; the server must reject upgrades without it.
- One JSON envelope per **text** frame; binary frames are rejected (`4400`).
- Default 1 MiB frame limit, advertised in `HELLO.limits`.

Auth: `Authorization: Bearer <token>` (non-browser). Browsers cannot set
headers on upgrades, so they send a single-use ≤60 s ticket as a second
subprotocol value `raep.auth.<ticket>`. Origins are allow-listed.

Keepalive: server pings every 20 s, closes if no pong within 60 s.
Close codes carry the client action (backoff, re-auth, do-not-retry):
see the table in [RFC §4.4](../rfc.md#44-close-codes).

## SSE + HTTP POST ([RFC §4.5](../rfc.md#45-sse-transport-server-sent-events))

For networks that block WebSockets.

| Purpose | Request |
| --- | --- |
| Stream + attach | `GET /raep/v1/sessions/{id}/events` with `Accept: text/event-stream`. Opening the stream *is* `SESSION_ATTACH`; the server sends `HELLO` then `SESSION_ATTACHED`. |
| Command | `POST /raep/v1/sessions/{id}/commands` → `202 Accepted`; the result arrives on the stream. |
| Upload | `POST /raep/v1/sessions/{id}/uploads` (RFC §12.4). |

- Each envelope is one SSE event: `id` is `seq`, `event` is the envelope
  `type`, `data` is the whole envelope on one line.
- Await each `202` before sending the next command in the same session;
  `Idempotency-Key` may repeat the envelope `id`.
- `ACK` is an ordinary command (`last_seq`).
- A rejected command returns the matching HTTP status with an `ERROR`
  envelope; a stream-level fault emits a final `ERROR` with
  `details.close_code`, then ends.

**Resume on reconnect:** send `Last-Event-ID: <seq>` — equivalent to
`resume_from_seq`. `EventSource` sends it automatically.

## HTTP resources ([RFC §4.7](../rfc.md#47-http-resources))

Sessions, shares, agents, uploads, outputs, snapshots, exports, imports,
runs, schedules, triggers, approvals, quota, discovery and health live under
`/raep/v1/` (admin under `/raep/admin/v1/`). Failures return an `ERROR`
envelope with the mapped status (`404 NOT_FOUND`, `409`, `422 INVALID_FRAME`,
`429`, `503`).

## Auth refresh ([RFC §4.6](../rfc.md#46-authentication-refresh))

The server sends `AUTH_EXPIRING` before a token lapses; the client answers
`AUTH_REFRESH` and gets `AUTH_REFRESHED` (or `ERROR UNAUTHENTICATED` +
close `4001`). On SSE a fresh `Authorization` header on any POST is treated
as a refresh. Tokens must be redacted from logs and replay buffers.
