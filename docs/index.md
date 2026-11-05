# RAEP documentation

RAEP (Remote Agent Execution Protocol) is a vendor-neutral, event-driven
protocol for remote agent execution. The normative specification is
[`rfc.md`](../rfc.md); these pages orient you in it.

| Page | What you get |
| --- | --- |
| [Quickstart](quickstart.md) | First session in five frames |
| [Concepts](concepts.md) | Session, turn, job, agent, delta |
| [Transport](transport.md) | WebSocket and SSE bindings |
| [Envelope](envelope.md) | The one frame format |
| [Delivery](delivery.md) | seq, ordering, replay, truncation |
| [Lifecycle](lifecycle.md) | Sessions, turns, tools and jobs |
| [Security](security.md) | Auth, tenancy, scopes, redaction |
| [Conformance](conformance.md) | Levels and the vector suite |
| [Frame index](frame-index.md) | Every message type, generated from the catalog |
| [Implementation guide](implementation-guide.md) | Checklist for servers and clients |

**Canonical base:** `https://raep.vyde.dev/v1/`. Status: Draft `0.1.0`,
wire version `1.0`.

If anything these pages say disagrees with `rfc.md`, the RFC wins — and the
pages are the bug.
