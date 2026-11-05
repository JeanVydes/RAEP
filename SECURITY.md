# Security policy

## Supported versions

RAEP is a Draft (`0.1.0`, wire `1.0`). Bugs in the specification or in the
reference implementation are fixed in `main`.

## Reporting a vulnerability

Open a private security advisory through GitHub
(`Security → Report a vulnerability`) on this repository. Include the
affected section or crate and a minimal reproduction.

Do not open a public issue for a vulnerability until it is fixed.

## Scope

- Authentication, authorization, tenancy and token handling (§4, §24, §25).
- Sandbox, job and artifact boundaries (§13, §14).
- The protocol must never expose provider credentials, native provider
  payloads or sandbox internals to clients (§1.1).
