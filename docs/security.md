# Security

([RFC §4](../rfc.md#4-transport) transport auth, §24 requirements, §25 tenancy)

## Principles

1. **The wire never carries secrets.** Provider credentials, native provider
   payloads/error bodies and sandbox internals stay server-side; errors are
   normalized to `PROVIDER_ERROR` / `PROVIDER_RATE_LIMITED`.
2. **Vendor neutrality is a security boundary.** Provider-specific data is
   quarantined under `provider.options` or `ext` — never smeared across core
   fields.
3. **Every session resource is authorized per session.** A client only sees
   sessions its principal is authorized on.

## Authentication

- Bearer tokens (`Authorization: Bearer <token>`); browser WS upgrades use
  single-use ≤60 s tickets (`raep.auth.<ticket>`).
- `Origin` checked against an allow-list for browsers.
- Refresh: `AUTH_EXPIRING` → `AUTH_REFRESH` → `AUTH_REFRESHED`; exceeding
  the deadline closes with `4001` and the client re-authenticates + resumes.
- Refresh tokens and credentials must be redacted from logs and replay
  buffers (§4.6).

## Tenancy, quotas and rate limits ([§25](../rfc.md#25-tenancy-identity-quotas-and-rate-limits))

Tenants own sessions, quotas and credentials. `QUOTA_WARNING`, `BUDGET_WARNING`
and `429` + `Retry-After` are part of the contract, not extensions.

## Authorization is a port

RAEP defines `Authorizer` and scope checks as ports (`raep-ports`): the
server composition root picks the identity system and policy. The protocol
never assumes a role model.

## Reporting

Vulnerabilities: see [SECURITY.md](../SECURITY.md). Do not open a public
issue until a fix exists.
