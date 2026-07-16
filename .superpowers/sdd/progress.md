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

- Automated verification: complete — formatting, architecture and frontend lint, core purity, strict workspace clippy, 1,072 workspace tests (573 core, 447 GNOME, 52 platform; 78 ignored), warning-free Rustdoc, QA linters, diff checks, and the under-800-lines source gate all pass; the dependency audit reports only the accepted RUSTSEC-2024-0436 warning. Focused isolated GTK tests for the device-card CSS and Preferences device subpage plus an isolated startup/shutdown smoke also pass.
- Assumptions: Main's newer Android-sync, waveform, queue-refill, persistent-column, session, and toast behavior is intentionally preserved behind the refactored module and platform-contract boundaries; the existing accepted `paste` advisory remains project policy rather than stage-specific debt.
- Manual checks: real Android/GVfs MTP transfer and reconnect behavior, physical audio output and media keys, pointer drag/reorder interactions, and final GNOME rendering remain for a hardware desktop pass because the isolated headless harness cannot verify them.
- Residual risks: Android vendors can expose inconsistent MTP progress and stable identifiers, and headless GTK coverage cannot prove compositor-specific rendering or pointer behavior; no additional automated regression or security advisory is known at stage close.
