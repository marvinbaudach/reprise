# SDD Progress Ledger — Android Sync (MTP)

Branch: feat/synch-android-settings
Plan: PLAN-android-sync.md
Started: 2026-07-16

## Pre-existing work (before SDD)
- Core: settings, delta, sanitize, m3u, transfer — COMPLETE
- Platform: device_sync, device_transfer (Opus encoder) — COMPLETE
- Runtime: device_sync_runtime state machine — COMPLETE
- DB: Schema V9 migration — COMPLETE
- Tests: 70+ tests across all layers — COMPLETE
- UI scaffold: preference_sync_planned.rs — HAS COMPILE ERROR

## Tasks

- Task 1: complete (commit 9691de5, base f9a8283, persisted per-device settings and inventory, computed deltas, safe paths, named playlists, and transfer plans)
- Task 2: complete (commit a95a2e5, base 9691de5, added safe MTP replacement/removal/eject, partial cleanup, and the bounded two-worker Opus pipeline)
- Task 3: complete (commit e8a932c, base a95a2e5, orchestrated the planned sync flow and connected Preferences, sidebar cards, Device View, shared action, progress, pinning, and lifecycle feedback)
- Main integration: complete (merge 668f292, merged origin/main at 7097222; follow-up 170b22d restores rustfmt and strict-clippy gates)

## Stage review

- Automated verification: complete — fmt, strict workspace clippy, 1,037 workspace tests, audit (only accepted RUSTSEC-2024-0436), core purity, diff check, and isolated Xvfb startup smoke.
- Assumption: the storage bar derives its total from managed Reprise bytes plus GVfs-reported free bytes because the current MTP backend does not expose a reliable total-capacity attribute.
- Manual checks: real Android/GVfs MTP copy progress, cable-pull behavior, pointer context menu, adaptive header spinner, animations, and final GNOME rendering remain for a hardware desktop pass.
- Residual risk: MTP backends may report progress and stable UUIDs differently across Android vendors; the URI fallback deliberately does not claim resumability.

## Android Sync hardening follow-up

- Task H1: complete (commit 20acc1d, base 273fa21, rejected unknown selection JSON before it can collapse to an empty removal selection; preserved explicit `[]` semantics)
- Task H2: complete (commit c403c4e, base 20acc1d, moved worker completion accounting and consumer wakeup into an unwind-safe guard; added a bounded worker-panic regression test)
- Task H3: complete (commit b4a7dd1, base c403c4e, made external cancellation observable to parked workers and consumers and removed buffered encoded temporary files on drop; real `probe_copy` unavailable because GVfs exposed no MTP device)
- Task H4: complete (commit 5b34ddb, base b4a7dd1, rejected legacy enqueue while a planned sync owns the device and taught the legacy starter to respect planned ownership)
- Task H5: complete (commit 4397477, base 5b34ddb, compared and persisted planned transfer-size fingerprints so Opus bitrate changes recopy once and remain stable, including unknown-duration tracks)
- Task H6: complete (commit b413ab0, base 4397477, refreshed device contents and available capacity after planned sync without clearing completion failures)
- Task H7: complete (commit e075054, base b413ab0, assigned fresh collision suffixes by track id and preserved all existing inventory slots across replans)
- Task H8: complete (commit 2342888, base e075054, removed trailing dots and whitespace introduced by UTF-8 component truncation)
- Task H9: complete (commit 32ffe79, base 2342888, carried planned-run generations through phase and byte-progress callbacks to ignore stale updates)
- Task H10: complete (commit 5a6e91f, base 32ffe79, rejected active-run settings updates before persistence or phase mutation)
- Task H11: complete (commit f911afd, base 5a6e91f, showed Music, projected additions, Other, and Free in a themed segmented bar with an optional-GVfs-capacity fallback)
- Task H12: complete (commit dba7c23, base f911afd, interpolated live byte progress and crossfaded card detail, indicator, percentage, and bar states with an immediate reduced-motion path)
- Task H13: complete (commit a818027, base dba7c23, surfaced the Opus encoder wait as an explicit Transcoding sync step)
- Task H14: complete (commit a818027, base dba7c23, carried title and artist into live sync activity text for transcoding and copying)
- Task H15: deferred (the shared Scan + Sync bottom slot is explicitly V2 scope and requires maintainer authorization before implementation)

## Android Sync hardening stage review

- Automated verification: complete — fmt, strict workspace clippy, 1,073 workspace tests, audit (only accepted RUSTSEC-2024-0436), core purity, diff check, and file-size checks.
- Display verification: complete for the changed widgets — storage CSS parsing, four cumulative storage segments, animated progress interpolation, and the reduced-motion immediate path each passed as an isolated exact Xvfb test.
- Hardware verification: unavailable — `gio mount -li` exposed no MTP volume and `probe_copy` returned `NO DEVICE`; no device file was written, so no cleanup was necessary.
- Assumption: when GVfs omits or misreports total MTP capacity, the storage bar shows only proven Music and Free values and labels Other unavailable instead of inventing a value.
- Deferred: the optional P4 refactors and lower-severity findings remain outside the required P1-P3 hardening scope; the V2 shared bottom slot awaits the stage-review decision.
