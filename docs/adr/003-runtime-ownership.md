# ADR 003: Reprise shelves the headless runtime

## Status

Accepted on 2026-08-31.

## Context

The experimental runtime path duplicated ownership that the shipped desktop
path already exercises directly. `reprise-runtime` had one workspace
dependent: `reprise-platform-linux`, solely for the service binary that the
packaging no longer installs. The GNOME adapter lived under a module-wide
dead-code allowance; its `RuntimeSession::from_client` constructor was called
only by its own test. The GTK application was therefore not a runtime client.

The MCP surface already reaches the live D-Bus interfaces directly and uses
only `reprise-runtime-protocol` for wire DTOs. Keeping the unused runtime and
client would retain two command surfaces and two ownership models for the same
domain without a shipped consumer. The architectural review estimated roughly
15,400 lines across the runtime, client, service, and adapter paths.

`docs/plans/consolidation-plan.md`, lines 711-733 at the time of this decision,
recommended shelving the runtime for the test round and required this ADR
before Wave 4 could proceed.

## Decision

Shelve the unused runtime implementation. Delete `reprise-runtime`,
`reprise-runtime-client`, the Linux runtime service and binary, and the dormant
GNOME adapter. The workspace returns to nine crates.

Keep `reprise-runtime-protocol`. It remains the shared DTO layer for the
direct-path D-Bus interfaces and for `reprise-mcp`; keeping that wire vocabulary
does not keep a second owner of playback or queue state.

## Consequences

- The direct in-process desktop path remains the sole owner used by the GNOME
  application.
- The repository stops building and testing an unshipped alternate runtime and
  its client.
- Runtime-service parity tests disappear with the implementation they tested;
  protocol schema tests remain with `reprise-runtime-protocol`.
- A future runtime must be justified from current consumers instead of reviving
  dead adapters by default.

## Resumption trigger

Reconsider a headless owner when a second frontend needs to control playback
without the GNOME process, or when an agent must control playback while no
window is running. At that point, design the owner and client boundary against
the actual second consumer and the retained protocol DTOs.

## Alternatives considered

- Cut every current surface over before the test round. Rejected because no
  second shipped consumer currently pays for the additional owner, service,
  lifecycle, and failure modes.
- Keep the unused crates built but merely uninstalled. Rejected because this
  preserves the duplicate model and its maintenance burden without gathering
  production evidence.
