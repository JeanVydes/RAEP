# Lifecycle

## Sessions ([RFC §10](../rfc.md#10-session-lifecycle))

`SESSION_ATTACH` binds a client (controller or observer) to a session.
Detach, close, configure (reconfigure at runtime, §8.1), export/import (§11),
fork/rewind/compact (§18) all ride the same envelope. Shares and presence
are §21.

## Turns ([RFC §12](../rfc.md#12-turn-lifecycle))

```text
USER_INPUT ─▶ QUEUED ─▶ CALLING_PROVIDER ─▶ EXECUTING_TOOL
              │              │                       │
              │              └──▶ AWAITING_APPROVAL ──┤
              └──▶ terminal: COMPLETED · CANCELLED · FAILED
```

- One active turn per session at a time; `queue_policy` decides what a
  second input does (`reject` / `queue` / `interrupt`).
- Deltas stream (`TOKEN_DELTA`, `TOOL_OUTPUT_DELTA`, `JOB_PROGRESS_DELTA`)
  until the terminal `TURN_STATE_CHANGED`.
- `CANCEL_TURN` stops the active turn; cancellation is not failure.

## Tools ([RFC §13](../rfc.md#13-tools))

1. `TOOL_CALL_STARTED` (with `requires_approval`).
2. Optional `TOOL_APPROVAL_REQUEST` → `TOOL_APPROVAL_RESPONSE`.
3. `TOOL_OUTPUT_DELTA` … `TOOL_CALL_FINISHED` (`OK` / `ERROR` / `DENIED` / …).
4. A call that outlives the turn may detach into a **job**.

Client-side tools invert step 3: `CLIENT_TOOL_REQUEST` → `CLIENT_TOOL_RESULT`.

## Background jobs ([RFC §14](../rfc.md#14-background-jobs))

`JOB_STARTED` → `JOB_PROGRESS_DELTA` → `JOB_FINISHED` (`RUNNING` … terminal).
`JOB_LIST` / `JOB_GET` query them. On completion the result is injected into
the agent history as a system event, possibly starting a new turn
(`triggered_new_turn`).

## History control ([RFC §18](../rfc.md#18-history-control-fork-rewind-compaction-and-snapshots))

Fork, rewind, compaction and workspace snapshots let a client rewrite or
branch history; pins protect specific turns from compaction.

Every frame type and payload is in the [Frame index](frame-index.md).
