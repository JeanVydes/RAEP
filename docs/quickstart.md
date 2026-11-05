# Quickstart

One session, five frames. Replace `example-agent` and the token with yours.

## WebSocket

```text
client → GET /raep/v1/ws           Sec-WebSocket-Protocol: raep.v1
client → HELLO                     capabilities, limits, wire_versions
client → SESSION_ATTACH            { agent_id, resume_from_seq }
server → SESSION_ATTACHED + SESSION_STATE
client → USER_INPUT                { prompt }  (envelope turn_id required)
server → TURN_STATE_CHANGED        QUEUED → CALLING_PROVIDER
server → TOKEN_DELTA …             streamed fragments, in seq order
server → TURN_STATE_CHANGED        COMPLETED
```

The `turn_id` on `USER_INPUT` opens the turn; deltas stream until the
terminal `TURN_STATE_CHANGED`. If the socket drops mid-turn, reconnect and
send `SESSION_ATTACH` with the last `seq` you processed as `resume_from_seq`
(see [Delivery](delivery.md)).

## SSE alternative

For networks that block WebSockets ([RFC §4.5](../rfc.md#45-sse-transport-server-sent-events)):

```bash
curl -N -H "Accept: text/event-stream" -H "Authorization: Bearer $TOKEN" \
  "https://raep.vyde.dev/raep/v1/sessions/$SESSION/events"
# then, in another shell:
curl -X POST -H "Content-Type: application/json" -H "Authorization: Bearer $TOKEN" \
  -d '{"version":"1.0","id":"01JB…","session_id":"'$SESSION'","timestamp":0,
       "type":"USER_INPUT","payload":{"prompt":"Hello"}}' \
  "https://raep.vyde.dev/raep/v1/sessions/$SESSION/commands"
```

Opening the stream *is* the attach. The server sends `HELLO` and
`SESSION_ATTACHED` first, then the command result arrives on the stream.
Reconnect with `Last-Event-ID: <last seq>`.

## Envelope

Every frame is one JSON object with `version`, `id`, `seq` (server only),
`ref`, `session_id`, `turn_id`, `job_id`, `timestamp`, `type`, `payload`,
`ext`. Full table: [Envelope](envelope.md).

## Where to next

- [Concepts](concepts.md) — vocabulary
- [Frame index](frame-index.md) — every message type and its payload
- [Implementation guide](implementation-guide.md) — before you write code
