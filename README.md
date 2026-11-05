# RAEP — Remote Agent Execution Protocol

**Documentation:** <https://raep.vyde.dev> · **Spec:** [`rfc.md`](rfc.md) ·
**Schemas:** [`schemas.json`](schemas.json) · **Vectors:** [`vectors.json`](vectors.json)

RAEP is a vendor-neutral, event-driven protocol for remote agent execution.
It defines how a stateless client (UI, CLI, or another service) talks to a
Remote Agent Execution Service that owns sessions, context, provider access
and tool execution.

```text
client ──wss / SSE──▶ session ──▶ provider + tools + jobs
          one envelope, strictly increasing seq, resumable delivery
```

## Status

| | |
| --- | --- |
| Document | Draft `0.1.0` |
| Wire version | `1.0` |
| Canonical base | `https://raep.vyde.dev/v1/` |

The normative text is `rfc.md`. Start with the [quickstart](docs/quickstart.md)
or read Sections 1–6 of the RFC.

## Design in one page

- **Transports.** One JSON envelope over WebSocket, or over HTTP SSE + POST
  for networks that block WebSockets ([RFC §4](rfc.md#4-transport)).
- **Ordering.** Every server event carries a per-session `seq` starting at 1.
  Clients process in order, treat a gap as a fault, and resume after the
  cursor ([RFC §16](rfc.md#16-delivery-ordering-and-resume)).
- **Resume.** `SESSION_ATTACH.resume_from_seq`, or `Last-Event-Id` on SSE;
  replay covers `(cursor, …]`, truncation rebuilds from `HISTORY_GET`.
- **Vendor neutrality.** No vendor names on the wire; provider-specific
  data lives under `provider.options` or `ext` ([RFC §1.1](rfc.md#11-vendor-neutrality)).
- **Async work.** Tools may detach into jobs; results are injected back into
  the agent history ([RFC §13–§14](rfc.md#13-tools)).

## Repository layout

| Path | What |
| --- | --- |
| `rfc.md` | Normative specification |
| `schemas.json` | JSON Schema (2020-12) for every payload |
| `vectors.json` | Conformance frame/scenario vectors |
| `docs/` | Guide: quickstart, concepts, transport, delivery, security |
| `raep-types` | Public wire envelopes and typed core payloads |
| `raep-core` | Sequencing, replay windows, ACK, idempotency |
| `raep-domain` | Session/turn invariants (no I/O) |
| `raep-ports` | Storage, provider, tool, job, auth, artifact ports |
| `raep-application` | Session lifecycle use cases |
| `raep-transport` | WebSocket text and SSE codecs |
| `raep-server` / `raep-http` | Reference composition and HTTP binding |
| `raep-memory` | Deterministic in-memory adapters for tests |
| `raep-conformance` | Vector/schema loaders and checks |

Dependencies point inward; adapters never leak into the protocol or domain.
See [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Verify

```bash
cargo test --workspace
pip install jsonschema referencing
python validate.py
python scripts/generate_frame_index.py --check
```

## License

MIT. The Rust workspace declares MIT OR Apache-2.0 per crate; the
specification and documentation in this repository are MIT.
