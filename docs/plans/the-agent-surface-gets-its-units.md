---
slug: the-agent-surface-gets-its-units
worktree: /home/marvin/Projects/reprise-the-agent-surface-gets-its-units
branch: feature/the-agent-surface-gets-its-units
phase: shipped
codex_session:
created: 2026-08-29
---

# The agent surface gets its units

Follow-up to `the-sync-bar-counts-work-not-bytes.md`, section 6 ("Die
Agenten-Oberfläche behält ihre Bytes und bekommt Einheiten"). That plan
required `units_done`, `units_total` and the estimated remaining time to reach
the agent surface, and named `music_get_device_sync_state` as the acceptance
measuring point for the whole change. The work landed everywhere except the
last hop.

## The gap, measured

Every production site carries the fields:

```
crates/reprise-core/src/agent_device_sync.rs:31-33
crates/reprise-platform-linux/src/mpris/device_sync_control.rs:129
crates/reprise-gnome/src/ui/device_sync/device_sync_agent.rs:197-199
crates/reprise-runtime-protocol/src/device_sync.rs:129
crates/reprise-runtime/src/devices.rs:128-131
```

The MCP shim drops them:

```
crates/reprise-mcp/src/device_dto.rs:145-149   DeviceSyncProgressDto has only
                                               bytes_done, bytes_total,
                                               bytes_per_second
crates/reprise-mcp/src/device_sync.rs:198-202  the mapping copies only those
                                               three, although `device.progress`
                                               already carries the unit fields
```

Verified live on 2026-08-29 by driving a freshly built `reprise-mcp`
(`--features mpris`, the device tools sit behind that compile feature —
`server.rs:127`) over stdio against the running app: a complete sync run
reported `bytes_done`, `bytes_total` and `bytes_per_second` and nothing else,
so the plan's own acceptance criterion ("`units_done` muss wachsen") could not
be read at all.

## Tasks

1. Add `units_done: u32`, `units_total: u32` and
   `estimated_remaining_seconds: Option<u64>` to `DeviceSyncProgressDto`
   (`crates/reprise-mcp/src/device_dto.rs`), keeping the existing byte fields —
   as *byte* counters they are honest and `music_get_device_sync_state` is a
   read interface others rely on.
2. Map them in `crates/reprise-mcp/src/device_sync.rs` from `device.progress`,
   next to the three byte fields.
3. Extend the tool description so the new fields are discoverable, in the same
   register as the surrounding text. The description lives in
   `crates/reprise-mcp/src/device_tools.rs` (the `#[tool(...)]` attribute on
   `music_get_device_sync_state`), which this strand therefore owns too.
4. Extend the existing DTO mapping test in
   `crates/reprise-mcp/src/device_sync.rs` (the fixture near the end of the
   file already sets `units_done: 4`, `units_total: 12`,
   `estimated_remaining_seconds: Some(16)`) so it asserts the three new fields
   arrive in the DTO. The fixture proves nothing today because the mapping
   discards them — that is exactly the mutation this test must catch.

## Verifikation

- `cargo test -p reprise-mcp` green.
- Control arm: with task 2 reverted, the new assertions must fail. A test that
  passes in both arms proves nothing.
- The change is additive to a serialized surface: no existing field is renamed
  or removed.

## Parallelität

Not cuttable — two files in one crate, and task 4's test asserts task 2's
mapping. One strand.

**Dateien dieses Strangs:**

```
crates/reprise-mcp/src/device_dto.rs
crates/reprise-mcp/src/device_sync.rs
crates/reprise-mcp/src/device_tools.rs
```
