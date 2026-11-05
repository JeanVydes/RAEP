# Implementation guide

Checklist for building a server or a client against RAEP. Normative text
lives in `rfc.md`; this is the short version. **Canonical base:
`https://raep.vyde.dev/v1/`.**

## Server

- [ ] Speak the envelope exactly (see [Envelope](envelope.md)); reject
      binary WS frames, enforce the advertised frame limit.
- [ ] `HELLO` with `wire_versions`, `capabilities`, `limits` before anything
      else (RFC §6).
- [ ] Assign per-session `seq` from 1, strictly +1; persist the event log
      before acknowledging (`ACK`) — durability (§17).
- [ ] Honor `resume_from_seq` / `Last-Event-ID`; implement truncation and
      `replay.lost` per [Delivery](delivery.md).
- [ ] Normalize provider/tool errors; never leak credentials or native
      provider payloads (§1.1, §23).
- [ ] Keep tools, jobs, auth and artifacts behind ports; pick adapters at
      the composition root (`ARCHITECTURE.md`).
- [ ] Emit `AUTH_EXPIRING`, `BUDGET_WARNING`, `SERVER_DRAINING` when their
      conditions hold.

## Client

- [ ] Hold no provider keys and no authoritative session state (§3).
- [ ] Process strictly in `seq` order; on a gap, stop applying and resume.
- [ ] Send `resume_from_seq` (or `Last-Event-ID`) on every (re)connect.
- [ ] On `replay.truncated`: discard local state, reload `SESSION_STATE`
      + `HISTORY_GET`.
- [ ] Await each SSE `202` before the next command in the same session;
      use `Idempotency-Key` when retrying creates.
- [ ] Reconnect with exponential backoff + jitter (500 ms → 30 s) and
      honor close-code actions (§4.4).

## Testing

- [ ] Pass the vector suite at your claimed level ([Conformance](conformance.md)).
- [ ] `cargo test --workspace` / `python validate.py` green in CI.
- [ ] Frame index in sync: `python scripts/generate_frame_index.py --check`.

## Cheat sheet

| Need | Start at |
| --- | --- |
| Bind a socket | [Transport](transport.md) |
| Parse/write a frame | [Envelope](envelope.md), [Frame index](frame-index.md) |
| Stream an answer | [Lifecycle](lifecycle.md) |
| Survive disconnects | [Delivery](delivery.md) |
| Lock it down | [Security](security.md) |
