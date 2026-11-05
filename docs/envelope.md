# Envelope

Every frame — client or server, WebSocket or SSE — is this single shape
([RFC §5](../rfc.md#5-protocol-envelope)).

| Field | Type | Who | Meaning |
| --- | --- | --- | --- |
| `version` | string | both | Wire version, constant `"1.0"` |
| `id` | string 1–128 | both | Unique per sender (ULID RECOMMENDED); deduplication key |
| `seq` | integer ≥ 1 | server only | Monotonic per session; absent on client and connection-level frames |
| `ref` | string \| null | both | `id` of the frame this one answers (e.g. an `ERROR`) |
| `session_id` | string | both | Session identifier, ≥128 bits entropy |
| `turn_id` | string \| null | both | Turn scope, if any |
| `job_id` | string \| null | both | Job scope, if any |
| `timestamp` | integer | both | Unix epoch ms — informational only; ordering uses `seq` |
| `type` | string | both | Event type from the catalog ([Frame index](frame-index.md)) |
| `payload` | object | both | Type-specific body (schema per type) |
| `ext` | object | both | Optional extension data |

## Conventions

- `snake_case` fields; `*_at` = epoch ms, `*_ms` = ms, `*_s` = seconds,
  `*_bytes` / `bytes` = bytes.
- Absent and `null` mean the same for optional fields unless stated otherwise.
- Unknown `type`: client frames get `ERROR UNKNOWN_TYPE`; server frames are
  ignored. Unknown payload fields are always ignored.
- Schemas live at `https://raep.vyde.dev/v1/payloads/<TYPE>.json`; the frame
  schema (`https://raep.vyde.dev/v1/frame.json`) selects the payload schema
  by `type` (RFC Appendix B).

```json
{
  "version": "1.0",
  "id": "01JB0000000000000000000040",
  "seq": 57,
  "ref": null,
  "session_id": "sess_88392",
  "turn_id": "turn_105",
  "job_id": null,
  "timestamp": 1793404901000,
  "type": "JOB_FINISHED",
  "payload": { "status": "COMPLETED", "exit_code": 0, "summary": "tests passed" }
}
```
