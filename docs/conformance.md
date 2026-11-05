# Conformance

([RFC §30](../rfc.md#30-conformance-levels) levels, §31 suite, Appendix C vectors)

## Levels

| Level | What it requires |
| --- | --- |
| Base | Envelope + handshake + attach + one turn end-to-end |
| Resilient | Ordering, resume, truncation, crash recovery (C-041…) |
| Full | Tools, jobs, approvals, delegation, federation |

## The suite

- `vectors.json` holds frame vectors (`V-###`, each `valid` true/false with
  a reason) and scenario vectors (`C-###`).
- `schemas.json` holds the normative JSON Schemas (2020-12).
- `validate.py` checks every vector against the schemas; `raep-conformance`
  does the same in Rust.

```bash
pip install jsonschema referencing
python validate.py
cargo test -p raep-conformance
```

## Rules for implementers

- A server claiming a level must pass every vector at that level.
- Unknown frame types from the client get `ERROR UNKNOWN_TYPE`; unknown
  fields everywhere are ignored.
- New types go through §27 + Appendix B + a vector pair (see
  [CONTRIBUTING.md](../CONTRIBUTING.md)).
