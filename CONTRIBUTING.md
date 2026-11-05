# Contributing to RAEP

RAEP is a protocol: changes are normative. Two kinds of contribution exist.

## Documentation and diagrams

- Fix typos, broken links and unclear prose.
- Every behavioral claim must trace back to a section of `rfc.md`.
- Documentation edits that change a normative rule must also update the
  relevant RFC section and the affected schemas/vectors.

## Spec changes

- Section 27 is the message catalog; Appendix B holds normative payload
  schemas. A new frame type needs a catalog row, a schema, and a conformance
  vector (`valid` and `invalid` where applicable).
- Wire rules: new fields are optional or go in `ext`; never silently change
  the meaning of an existing field (see `ARCHITECTURE.md` compatibility
  policy). A wire change bumps the wire version.
- Vendor neutrality (§1.1): no vendor names on the wire, no vendor-specific
  payloads outside `provider.options` or `ext`.

## Code changes

- `cargo test --workspace` must pass; public API docs must be present.
- `python validate.py` must pass (`pip install jsonschema referencing`).
- `python scripts/generate_frame_index.py` must produce no diff.

## Pull requests

- One logical change per PR.
- Update `CHANGELOG.md` in the same commit as any user-visible change.
