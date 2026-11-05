# Delivery, ordering and resume

([RFC §16](../rfc.md#16-delivery-ordering-and-resume), §17 durability)

## Ordering

- The server assigns `seq` per session, strictly increasing by 1, across all
  server events.
- Clients **must** process in `seq` order and treat a gap as a fault:
  reconnect with `resume_from_seq`.
- `timestamp` never orders anything; it is informational.

## Resume

On `(re)attach` the client sends `resume_from_seq` = last `seq` processed
(or `Last-Event-ID: <seq>` on SSE — equivalent). The server replays frames
with `seq > cursor` and nothing older.

## Truncation and loss

If the cursor fell out of the retained window the server answers
`SESSION_ATTACHED` with `replay.truncated: true` and a full `SESSION_STATE`.
The client **must** discard derived state and rebuild from `HISTORY_GET`.
After a crash `replay.lost` marks frames delivered but never durable.

## Durability

The event log is append-only with ordering invariants ([§17.1](../rfc.md#171-event-log-and-ordering-invariants));
session ownership/failover and crash recovery are server-side ([§17.2–17.4](../rfc.md#172-session-ownership-and-failover)).

## Acknowledgement and dedup

- `ACK { last_seq }` tells the server the client consumed up to a cursor;
  replay retention may drop acked ranges.
- `Idempotency-Key` on creating POSTs, and envelope `id` for deduplication:
  a repeated command inside the window is answered idempotently; outside it
  conflicts (`409 IDEMPOTENCY_CONFLICT`).

## Slow consumers

A replay buffer that overflows gets close code `4008` — reconnect and resume.
There is no silent drop: gaps are always detectable via `seq`.

## Quick reference

| Situation | Client action |
| --- | --- |
| Clean reconnect | `resume_from_seq` = last seen |
| Gap in `seq` | Reconnect with resume; do not apply later frames |
| `replay.truncated` | Discard, `SESSION_STATE`, then `HISTORY_GET` |
| `4008` slow consumer | Reconnect (backoff) and resume |
