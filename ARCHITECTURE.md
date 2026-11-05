# RAEP architecture

RAEP is split into stable layers. Dependencies point inward; adapters never
leak into the protocol or domain.

```text
raep-server
  -> transport adapter
  -> application services
      -> domain
      -> ports
          <- memory, database, provider, tool, job, auth and artifact adapters
  -> raep-types
```

## Boundaries

- `raep-types` is the public wire contract. It owns versioned envelopes,
  payloads and identifiers, but no execution policy.
- `raep-core` owns delivery invariants such as sequencing, replay windows,
  acknowledgements and bounded idempotency. It does not own sessions.
- `raep-domain` owns entities, state transitions and invariants. It has no
  serialization, network, clock, database or provider dependency.
- `raep-ports` defines inbound and outbound application contracts. Ports use
  domain values and explicit errors.
- `raep-application` orchestrates commands and domain transitions through
  ports. It does not choose an adapter.
- `raep-memory` is a deterministic adapter for tests and local development,
  not part of the domain contract.
- `raep-conformance` loads the repository's official vectors and checks
  envelope structure and Draft 2020-12 schemas.
- `raep-transport` implements WebSocket text framing and SSE event framing
  without choosing a socket, HTTP, TLS, or async runtime.
- `raep-server` will compose the application with transport and deployment
  adapters. It is intentionally not imported by the libraries.

## Execution boundaries

Provider generation, tool execution, detached jobs, authorization, and
artifacts are all ports. Their contracts intentionally use owned values and
boxed standard-library futures, so an implementation may use Tokio, another
runtime, synchronous workers, a remote service, or a test double without
changing the public core.

`JobStore` owns durable job state and retained output. `JobRunnerPort` owns
process or remote execution. Keeping them separate makes crash recovery,
leases, output retention, and sandboxing adapter policies instead of hidden
domain assumptions. `ArtifactStore` returns opaque references; the protocol
never needs to know whether bytes live in a database, filesystem, or object
store. `Authorizer` receives a principal and an explicit scope, leaving
identity verification and policy evaluation to the composition root.

Transport connections, authentication, leases, notifications, management
queries, and headless scheduling are also ports. This means WebSocket/SSE,
token validation, failover ownership, webhooks, admin APIs, and cron/queue
systems can be selected independently by a server composition.

Storage is represented by ports, not by a storage abstraction with a presumed
shape. An adapter may use one database, several stores, an event log, or an
external service as long as it preserves the port invariants.

## Compatibility policy

The wire contract is versioned independently from crate versions. New wire
fields must be optional or introduced through `ext`; existing meanings and
state transitions are not silently changed. Public Rust types are documented
and tested from outside their defining crate.
