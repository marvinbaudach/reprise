# A run that is not remembered still has to be recorded

## What happened, and why it could not be seen

On 7 August 2026 a Pixel 10 Pro XL ran four synchronizations from the device
page. Every one of them did nothing, and reported success. The cause has since
been fixed on `dev` (#336 resolves the USB serial from the volume identifier, so
the phone has a stable identity again) — but the *reason nobody could tell* is
still here:

```rust
let run = if !persist_device_state { None } else { start_run(...) };
```

`persist_device_state` is `descriptor.persistent_id.is_some()`. A device without
a stable identity therefore writes no `sync_runs` row, no deviations and no
outcome. `sync_runs` held exactly one row, from 28 July; four runs on 7 August
left no trace at all, while the page said "Verified · 214 tracks on device". The
diagnosis needed a hand-written probe and a read of the accessibility tree
because the app's own diary was switched off for that device.

The gate conflates two different things:

- **"May I write a durable record *about this device*?"** — inventory rows,
  verification timestamps, remembered settings. These are genuinely
  identity-bound: a row keyed by a volatile address is worse than no row,
  because the next phone plugged into the same bus address inherits it.
- **"Did a run happen, and what did it do?"** — a diary entry about this
  session. It needs no durable identity at all, and it is exactly what is
  missing when something goes wrong.

## What to change

Separate them. The run log is written for **every** run, keyed by
`descriptor.id` — the identifier that always exists, stable when there is a
serial and the transport URI otherwise. The inventory and verification writes
stay gated exactly as they are today.

`sync_log::start_run` already takes `device_serial`, and `RETAINED_RUNS` already
caps how many rows a device keeps, so an unidentified device cannot accumulate
rows without bound.

Read `device_sync_planned.rs` and `device_sync_run_log.rs` as they are on `dev`
first: #336 landed `record_rejected_start` and reworked the history surface, and
this change has to fit that shape rather than the one from before it.

## Proof

1. A run on a device with `persistent_id: None` writes a `sync_runs` row with a
   started time and, at the end, an outcome — and its deviations are recorded.
2. The same run still writes **no** `device_files` row and no
   `last_verified_at`. This is the half that must stay gated; a test that only
   checks the new behaviour would let someone "simplify" the remaining gate away
   later.
3. A rejected start on such a device is recorded too (the path #336 added).
4. Deleting a device's settings still removes its run rows — the cascade in
   `settings.rs` already covers `sync_runs` by `device_serial`; prove it holds
   for a URI-keyed device as well.

Say for each which production line you reverted and which named test went red.

## Out of scope

The wording of an empty run. `dev` already says "Nothing to transfer" and
"Nothing pending"; do not touch that copy.
