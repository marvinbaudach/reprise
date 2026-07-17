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

## Android Sync V2 shared activity slot follow-up

- Task H15: complete (commit c411b97, base 334a589, stacked connected-device sync and scan cards in one stable bottom-pinned sidebar activity slot while preserving in-place card updates)

## Android Sync V2 shared activity slot stage review

- Automated verification: complete — fmt, strict workspace clippy, 1,073 workspace tests, audit (only accepted RUSTSEC-2024-0436), core purity, diff check, and file-size checks.
- Display verification: complete for the shared layout contract — an exact isolated Xvfb test proves Devices → Scan ordering independent of construction order and simultaneous visibility; an isolated CUA launch/snapshot confirms the connected-device section renders at the sidebar bottom without touching the live desktop or user database.
- Assumption: connected devices remain above the scan card inside the shared slot, preserving the scan card as the absolute bottom activity while moving both activities behind one layout seam.
- Manual check: final rendering during a genuinely simultaneous long library scan and real MTP sync remains for the hardware desktop pass; no Android device was accessed during this follow-up.
- Residual risk: the isolated CUA session exposed only the top-level AT-SPI node, so semantic accessibility-tree verification of the two nested cards remains part of the manual desktop pass; the pixel snapshot and GTK hierarchy test were both successful.

---

# SDD Progress — Project Refactoring

Plan: session plan approved on 2026-07-16
Branch: feat/refactoring-durch-codex-in-reprise
Merge base: 071254b
Lock: claimed by Codex in this worktree on 2026-07-16
Stage: Project-wide refactoring and guardrails

- Task 1: complete (commit 65428f5, base 071254b, restored mandatory gates, centralized album placeholder CSS, removed an orphan module, and split every Rust file below 800 lines)
- Task 2: complete (commit 05c067a, base d637bd0, added architecture/frontend linters, merge-readiness QA, documentation, and a versioned optional pre-push hook)
- Task 3: complete (subsumed by commit 65428f5, extracted core unit-test modules below the file-size limit)
- Task 4: complete (subsumed by commit 65428f5, extracted GTK unit-test modules below the file-size limit)
- Task 5: complete (commit ec95d7e, base 05c067a, centralized artist avatar gradients, replaced dynamic per-widget glow CSS with drawing, and removed deprecated Artist style-context debt)
- Task 5a: complete (commit 0aa3ca7, base ec95d7e, documented merge gates, 75 isolated GTK tests, prioritized automation gaps, manual release evidence, and harness constraints)
- Task 6: complete (commit f04667b, base 0aa3ca7, split AlbumView composition, state transitions, and action wiring with pure and isolated GTK coverage)
- Task 7: complete (commit a0b7cb5, base f04667b, split tag-editor orchestration into form, dirty-state, lookup, save, and widget modules)
- Task 7a: complete (commit 7df365c, base a0b7cb5, normalized all six settings-page insets and disabled Gapless with an explanatory subtitle while Crossfade is active)
- Task 8: complete (commit abdb2f5, base 7df365c, split scan orchestration, progress/cancellation controls, worker reconciliation, and watcher lifecycle)
- Task 9: complete (commit 3ac5ebe, base abdb2f5, moved waveform extraction behind a core contract and Linux backend while removing direct GStreamer dependencies from the GNOME crate)
- Task 10: complete (commit 17fc674, base 3ac5ebe, moved Linux player, media, and waveform construction to the window composition root and injected only core contracts into playback and scan features)
- Task 11: complete (commit 1cca6b6, base 17fc674, reduced the main composition root from 735 to 488 lines by extracting post-composition runtime, menu, navigation, scan, session, and smoke wiring)
- Task 12: complete (commit 982b167, base 1cca6b6, reduced TrackList and Sidebar orchestrators below 600 lines by extracting one-time construction and sidebar query/row projection)
- Task 13: complete (commit 5c52b24, base 982b167, moved feature SQL, atomic audio-effect persistence, and worker migration readiness behind focused core database facades)
- Task 14: complete (commit 3007cf7, base 5c52b24, replaced the flattened UI path registry with 18 true feature modules and explicit crate-local surfaces)
- Task 15: complete (commit c72e389, base 3007cf7, introduced a cancellation-safe named one-shot task helper, migrated seven duplicate UI workers, and enforced the boundary in the architecture gate)
- Main integration: complete (merge 04b71c2, integrated main at 273fa21 while preserving Android sync, waveform, column-order, queue-refill, session, and toast behavior through the refactored boundaries)

## Stage review

- Automated verification: complete — formatting, architecture and frontend lint, core purity, strict workspace clippy, 1,074 workspace tests (573 core, 449 GNOME, 52 platform; 78 ignored), warning-free Rustdoc, QA linters, diff checks, and the under-800-lines source gate all pass; the dependency audit reports only the accepted RUSTSEC-2024-0436 warning. Focused isolated GTK tests for the device-card CSS and Preferences device subpage plus an isolated startup/shutdown smoke also pass.
- Final adversarial review: complete (commit 5de9c9b, independent Standards and Spec reviews found no specification gap; two RefCell borrows that crossed GTK-facing calls were fixed test-first and the fix diff passed a second Standards review with no findings).
- Assumptions: Main's newer Android-sync, waveform, queue-refill, persistent-column, session, and toast behavior is intentionally preserved behind the refactored module and platform-contract boundaries; Task 14's explicit root compatibility re-exports remain a conservative call-site migration surface rather than a second implementation tree; the existing accepted `paste` advisory remains project policy rather than stage-specific debt.
- Manual checks: real Android/GVfs MTP transfer and reconnect behavior, physical audio output and media keys, pointer drag/reorder interactions, and final GNOME rendering remain for a hardware desktop pass because the isolated headless harness cannot verify them.
- Residual risks: Android vendors can expose inconsistent MTP progress and stable identifiers, and headless GTK coverage cannot prove compositor-specific rendering or pointer behavior; no additional automated regression or security advisory is known at stage close.

---

# SDD Progress — GUI Acceptance Hardening

Branch: feat/gui-acceptance-tests
Base: e5538b5
Started: 2026-07-16

- Task 1: complete (commit 226e41f, base e5538b5, added a private CUA/AT-SPI acceptance harness for fresh and populated libraries, enforced snapshot-action-snapshot semantics, retained screenshots and diagnostic logs, and added searchable first-run completion logging)

## Stage review

- Automated verification: complete — formatting, strict workspace Clippy, 1,095 workspace tests (580 core, 460 GNOME, 55 platform; 83 ignored), Rustdoc with warnings denied, QA/architecture linters, the CUA fake-driver contract, diff checks, and the under-800-lines source gate pass; dependency audit reports only the accepted RUSTSEC-2024-0436 warning.
- CUA execution: attempted — the managed Codex sandbox rejects the Unix sockets required by Xvfb and `dbus-run-session` with `Operation not permitted`; the runner now fails fast with bounded diagnostics instead of producing an unbounded X server log.
- Isolation: the runner creates private XDG data/cache/config/runtime roots, D-Bus and AT-SPI sessions, Xvfb/Openbox, a fake audio sink, and copied FLAC fixtures; it never touches the maintainer's desktop, database, music, accounts, or session bus.
- Logging: each scenario retains its own app log plus JSON snapshots and screenshots; a minimal manifest records only commit, build profile, CUA version, platform, display backend, and timestamp. Acceptance requires startup, database-ready, workflow, scan, and clean smoke-shutdown markers and rejects GTK/GLib criticals, panics, and RefCell failures.
- Deferred host check: run `cargo build && scripts/cua-e2e/run.sh` outside the managed sandbox to collect the first real AT-SPI screenshots and confirm the exact `Search all fields`, fixture-title, and empty/no-results labels exposed by the installed GTK stack.
- Residual risk: the deterministic driver contract proves orchestration and safety but cannot substitute for the deferred host CUA run; native Wayland rendering, portals, pointer feel, media keys, and audible playback remain release-manual checks.
- Bugfix: complete (commit 8bcd060, base 3bd0eee, table no longer scroll-centers the row when playback starts from a double-click/Enter/queue activation — one-shot id-matched suppression consumed by the now-playing follow; auto-advance/skips/title-click/restore still center. Includes chore bc4b631: rustfmt 1.9 drift in window.rs.)
