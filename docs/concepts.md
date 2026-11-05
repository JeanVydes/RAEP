# Concepts

| Term | One line ([RFC §2](../rfc.md#2-terminology)) |
| --- | --- |
| **Client** | Stateless UI/CLI. Renders events, sends commands, holds no provider keys. |
| **Server** | The Remote Agent Execution Service. Owns state, history, provider access, tools, jobs. |
| **Session** | Persistent server-side conversation bound to one `agent_id`. |
| **Turn** | One unit of agent work: input (or system trigger) → terminal state. |
| **Delta** | Incremental fragment of streamed output. |
| **Tool call** | One tool invocation the model makes inside a turn. |
| **Job** | Detached background process started by a tool; outlives its turn. |
| **Agent** | Configuration (provider, model, settings, tool set) that drives a session. |
| **Snapshot** | Copy of a session workspace at end of turn. |

## The moving parts

- A **session** is the durable thing; **turns** happen inside it.
- A turn streams **deltas** (`TOKEN_DELTA`, tool output) and ends in a
  terminal `TURN_STATE_CHANGED` ([Lifecycle](lifecycle.md)).
- A tool that outlives the turn becomes a **job**; finishing injects its
  result into the agent history as a system event.
- **Agents** are pure configuration ([RFC §7](../rfc.md#7-agent-definition)):
  provider/model are opaque strings, so the protocol stays vendor-neutral.

## Invariants

1. The client never persists authoritative session state ([RFC §3](../rfc.md#3-core-constraints-and-guarantees)).
2. Every server event carries a per-session `seq`; delivery is ordered and
   resumable ([Delivery](delivery.md)).
3. No provider credentials or native provider payloads ever reach the client
   ([Security](security.md)).
