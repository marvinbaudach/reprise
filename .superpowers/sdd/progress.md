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
- Queue+Nav-Plan (docs/superpowers/plans/2026-07-17-queue-nav-fixes.md): complete (commits 28774ff..HEAD, base 8bcd060). ux-rules.md angelegt; NAV-5 View-State-Memory (9aa2e5b); Play-Origin-Threading (6e09108); Composite-Queue-View QUE-1/2/4/5 (c5200e1); QUE-3-Interaktionen + Play-next (2a54066); NAV-9 Jump + NAV-2 Back + Ctrl+L inkl. Review-Fixes (d1ba456); Size-Gate-Extraktionen (HEAD). Abnahme headless verifiziert: Composite 1+PlayNext+UpNext·from-Origin, Play-next-Reihenfolge, Jump→select+center, Back→Queue, Stop→EmptyQueue-StatusPage, NAV-5-Restore-Log. PLAY-3-Filteranteil über geteilten Query-Pfad + bestehende Tests abgedeckt (Smoke-Hook-Reihenfolge erlaubt kein Filter-vor-Activate-E2E). Adversarial-Review: 2 blocking Findings gefixt (stale Now-Playing nach Stop; Queue-Aktivierungs-Reseed durch QUE-3-Jump ersetzt), purge-Notify + Dead-now-playing-Skip nachgezogen.

## 2026-07-17 — UX-Regelwerk Task 1 (docs/ux-rules.md)

- Verbindliches UX-Regelwerk eingecheckt: 60 Regelzeilen (Sektionen A–J,
  alle `[geplant]`, PLAY-5 als Ersetzt-Wegweiser), mit Prozessregeln
  (Status, append-only IDs, Ebenen-Tags,
  Traceability, Änderungsprotokoll). Härtung gemäß Grilling 2026-07-17
  (docs/plans/ux-rules-acceptance-tests.md). QUE-1–5/NAV-9 aus dem
  Queue-Fix-Prompt wörtlich übernommen — Implementierung läuft parallel.

- AGENTS.md: binding-UX-rules section added (contract, flip rule, proposal
  protocol) — UX-Regelwerk Task 2.

- Traceability-Lint eingeführt (scripts/check-ux-traceability.sh, 3 Richtungen)
  und in check-merge-readiness verdrahtet; TESTING.md dokumentiert das Gate —
  UX-Regelwerk Task 3. Der Aktiv-Zähler ist auf Regelzeilen verankert, damit
  Prozessbeschreibungen mit `[aktiv]` nicht fälschlich als aktive Regeln zählen.

- Bereich-C-Audit: PLAY-1 ist über `queue_ids_for_activation` und
  `play_from_view` implementiert, aber noch nicht regelbenannt getestet;
  PLAY-1a ist für Album-Container implementiert und anderweitig getestet,
  für alle beschriebenen Container jedoch noch nicht als Gesamtvertrag
  nachgewiesen; PLAY-2/3/5a sind implementiert und jetzt regelbenannt getestet
  und deshalb im selben Commit auf `[aktiv]` gesetzt. PLAY-4a ist über
  Missing-ausschließende Listenabfragen und den Playback-Fault-Skip teilweise
  implementiert, aber nicht regelbenannt als stiller Gesamtvertrag getestet;
  PLAY-4b und PLAY-5b sind nicht vollständig implementiert; PLAY-6 ist samt
  Off→All→One-Zyklus implementiert und anderweitig getestet, aber noch nicht
  regelbenannt. Pilot-Regeltests stehen in `queue_tests.rs`, QUE-1 bleibt als
  `[geplant]`-Demo ignored — UX-Regelwerk Task 4.

- cua-e2e: `play-2-doubleclick-row`-Szenario und
  `cua_double_click_label`-Helper ergänzen den Verdrahtungsbeweis für PLAY-2
  über den Marker `queue set from view`. Das Szenario läuft vor dem
  `nomatch`-Filter, weil die Fixture-Row danach absichtlich verborgen ist.
  Deferred host check: `cargo build && scripts/cua-e2e/run.sh` startete Xvfb
  und Reprise isoliert, aber der private CUA/AT-SPI-Pfad listete innerhalb der
  Smoke-Frist kein Reprise-Fenster; deshalb wurde kein grüner CUA-Lauf
  behauptet — UX-Regelwerk Task 5.

- UX-Regelwerk-Fundament komplett: Dokument (60 Regelzeilen, 3 `[aktiv]`,
  1 ersetzt), AGENTS.md-Bindung, Traceability-Gate, Pilot Bereich C (core +
  e2e), QUE-1-Aktivierungs-Demo. Verhaltensänderungen laufen als `[geplant]`
  in Folge-Branches (Queue-Branch parallel in Arbeit) — UX-Regelwerk Task 6.

## 2026-07-17 — UX-Regelwerk: Review-Korrekturen

Zwei-Achsen-Review (Standards + Spec) des Branches `feat/ux-rules-acceptance-tests`.
Beide Achsen bestätigten das Fundament; die folgenden Findings wurden umgesetzt.
Vom User verworfen: 800-Zeilen-Regel für `.md` (Docs sind davon ausgenommen) und
„Deutsch in Doku" (das Regelwerk ist bewusst deutsch — Arbeitssprache).

- **PLAY-3 → PLAY-3a/PLAY-3b gesplittet.** Task 4 hatte PLAY-3 komplett auf
  `[aktiv]` geflippt, obwohl der Test nur die Treffer-Shuffle-Klausel deckt und
  die Filter-Nachträglichkeits-Klausel keine Assertion hat — ein Verstoß gegen
  die eigene Prozessregel „Halb umgesetzt → a/b-Split". Jetzt: PLAY-3
  `[ersetzt durch PLAY-3a/PLAY-3b]`, PLAY-3a `[aktiv] [core]` (Test
  `play_3a_shuffle_stays_inside_filtered_snapshot`, umbenannt), PLAY-3b
  `[geplant] [gtk]`. Weiterhin 3 `[aktiv]`-Regeln, jetzt 2 ersetzte.
- **Sprache korrigiert:** `queue_tests.rs`, `check-ux-traceability.sh` und der
  cua-e2e-Kommentar sind Code und jetzt englisch (AGENTS.md „English
  everywhere"). Regel-IDs/Status-Token bleiben als Zitate deutsch. AGENTS.md und
  `docs/ux-rules.md` halten die Grenze jetzt explizit fest.
- **Traceability-Gate gehärtet** — vier Löcher, jedes mit Negativprobe belegt:
  Präfixe werden aus dem Dokument abgeleitet statt hartkodiert (neue Sektion =
  automatisch gegated, verifiziert mit einer `ZZZ-1`-Testregel); nur echte
  `#[test]`-fns zählen (Helper-fn allein → FEHLER); Kommentarzeilen in
  `scripts/cua-e2e` zählen nicht mehr (Kommentar allein → FEHLER); das
  Ignore-Format `UX <ID> [geplant] — …` wird erzwungen (`#[ignore = "later"]`
  → FEHLER). Bestehende Proben (Ignore auf `[aktiv]`, Test auf ersetzte Regel)
  weiterhin rot.
- **Duplikat entfernt:** `cua_click_label`/`cua_double_click_label` teilen sich
  `cua_pointer_action_label <verb>` (`scripts/cua-e2e/lib.sh`); der
  Kontrakttest `scripts/tests/cua-e2e.sh` bleibt grün.
- **AGENTS.md-Widerspruch aufgelöst:** „keine Pläne im Repo" galt wörtlich gegen
  die bestehende Praxis (`docs/plans/android-sync.md`). Jetzt: Wegwerf-Pläne
  bleiben in der Session, überdauernde Pläne leben in `docs/plans/`, Verträge
  wie `docs/ux-rules.md` sind keine Pläne. 800-Zeilen-Regel gilt explizit nur
  für Code.
- **Falscher Sanity-Check** in Task 1 Step 2 des Plans korrigiert
  (`grep -c '[aktiv]'` war nie `0`); Review-Nachtrag ans Plan-Doc angehängt.

Verifiziert: `check-ux-traceability.sh` grün (3 aktive Regeln) + 6 Negativproben
rot · `cargo test -p reprise-core --lib` 583 passed / 1 ignored · Assertion-Flip
in `play_3a` beweist Biss (rot → zurück → grün) · `scripts/tests/cua-e2e.sh`
grün · `cargo fmt --check` sauber · `check-merge-readiness.sh` grün.

Offen (bewusst, kein Blocker): PLAY-2s gatender Core-Test beweist
`set_tracks`-Semantik, nicht die Doppelklick-Verdrahtung — deren Beweis liegt im
nicht-gatenden cua-e2e-Szenario, dessen grüner Lauf weiterhin am Host-Gate hängt.

## 2026-07-17 — UX-Regelwerk: Merge mit dem Queue+Nav-Stand aus main

Der Queue+Nav-Agent hatte parallel eine eigene `docs/ux-rules.md` angelegt (69
Zeilen, 13 Regeln, alle `[aktiv]`, ⟲-markiert als aus dem Gedächtnis
rekonstruiert, ohne regelbenannte Tests, eigenes Format) und nach main gemergt —
add/add-Kollision auf dem Vertragsdokument. **User-Beschluss: das Regelwerk
dieses Branches gewinnt** (Superset: 60 Regeln, Sektionen A–J, Prozessregeln,
Ebenen-Tags, Gate). Inhaltlich ging nichts verloren: QUE-1..5, NAV-9 und FB-5
waren in beiden Dokumenten wortgleich; die übrigen Regeln des Queue-Docs sind in
diesem Dokument präziser oder feiner geschnitten (PLAY-1/1a, PLAY-3a/3b, NAV-3).

**Statuswahrheit nach der Vertragsregel:** implementiert ohne regelbenannten Test
= `[geplant]` (nicht einklagbar). Das Queue-Doc führte 13 Regeln als `[aktiv]`,
ohne dass eine davon einen regelbenannten Test hat — genau das False-Green, das
das Gate verhindern soll. Sie stehen hier daher `[geplant]`.

**ACHTUNG — implementiert, aber `[geplant]` (nicht neu bauen, nur testen!):**

- **QUE-1/2/4/5** — Composite-Queue-View mit Now Playing · Play Next · Up Next ·
  aus <Quelle>, Sidebar-Zähler, Leerzustand: implementiert in c5200e1.
- **QUE-3** — DnD-Reorder, Remove, Playhead-Jump, Clear: implementiert in
  2a54066 (Core-Tests vorhanden, aber nicht regelbenannt:
  `remaining_after_current_*`, `remove_order_positions_*`,
  `jump_to_order_position_*`).
- **NAV-9 / NAV-2** — Jump to Now Playing + Back-Stack + Ctrl+L: implementiert in
  d1ba456.
- **NAV-5** — View-State-Memory: implementiert in 9aa2e5b.
- **PLAY-1** — Kontext-Snapshot beim Wiedergabestart: implementiert
  (Play-Origin-Threading 6e09108).

Ihr Flip auf `[aktiv]` braucht je einen regelbenannten Test (`fn que_1_…`,
`fn nav_9_…`, …) im selben Commit — das ist die naheliegende Folgearbeit und
wäre größtenteils Umbenennen/Aufsetzen auf die schon vorhandenen Tests.
QUE-1s ignored Core-Demo wurde ehrlich gemacht: die Drei-Sektionen-Queue
existiert, aber der Core-Stub kann sie nicht beweisen — der Flip braucht einen
`[gtk]`-Sektionstest.
Task 1.3: complete (commit 19c9810, review clean) — schema v11 drops the legacy missing column; missing_since is now physically the only truth.
Task 1.4: complete (commit 1fea712, review clean) — library/mounts.rs: lstat ancestor walk, mount_point_of, classify_missing (st_dev vs tracks.device). Pure core, no trait, testable without root.
  Minor findings (carried into 1.5): (a) the three fns carry #[allow(dead_code)] until callers land — must be removed in 1.5; (b) "capped at /" only holds for absolute paths — add a doc note/debug_assert when a caller wires in.
Task 1.5: complete (commits 1fea712..e6a4b55, review clean after one fix pass) — scan_folder is now an atomic reconcile (walk+upsert+vanish-mark+classify in ONE tx), returns ScanOutcome{Completed,RootUnavailable}; root guard is root-only; mark_vanished_under_root is gone; mark_track_missing uses classify_missing. Marking logic extracted to scanner_vanish.rs. Fix pass closed 2 Important (stale #[allow(dead_code)]+false doc on classify_missing; root-guard evidence set narrowed to PRESENT instead of the binding removed_at IS NULL — cost the RootUnavailable signal when all rows were already missing) + 2 test gaps (watcher RootUnavailable path, mark_track_missing device-bearing branches).
  NOTE for Task 5.6: tag_edit.rs::apply_patch_batch ALREADY calls scan_folder per track after a tag write — the "re-read after tag-editor save" hook largely exists; verify rather than rebuild.
  NOTE for Task 1.6: mount_point_of is currently wired only into a diagnostic tracing field in scanner_vanish.rs (done to discharge a dead-code lint); Task 1.6 is its real caller and should record mount_point per track.
Task 1.6: complete (commit 0772f48, review clean) — mount_point recorded on scan in all three arms (upsert, move, fast-path restore), memoized per scan run on parent dir (scanner_mount.rs). Never backfilled for untouched rows (NULL = "unknown location" group).
  Minor findings (final-review triage): restore-branch test asserts only the prefix half of the mount_point invariant, not dev equality; cache-hit behavior untested (design-verified only).
  WARNING for 1.7/1.8/1.9: scanner.rs is at 796/800 lines — the next task touching it MUST extract a cohesive sibling module first (precedent: scanner_vanish.rs, scanner_mount.rs).
Task 1.7: complete (TDD, 7 new tests, gates green) — new library/import_errors.rs: ImportErrorKind taxonomy classified at the source (lofty::error::ErrorKind, walkdir::Error::io_error/kind — never Display text), episode upsert (record_error/clear_error), and the dismiss-skip fast path (check_dismissed, stat-only, reactivates a changed file into a fresh episode). ScanError::Tags(String) → ScanError::Import{kind,detail}. scanner.rs's directory-traversal and per-file error branches now call into import_errors instead of inline DELETE+INSERT SQL. Split scanner.rs's own test growth into scanner_import_errors_tests.rs (precedent followed) to stay under 800 lines (799 final). Full report: task-1.7-report.md.
Task 1.7: complete (commit b61e7fd, review clean) — library/import_errors.rs: ImportErrorKind taxonomy classified at the source from lofty ErrorKind (all 21 variants of lofty 0.22.4 explicitly mapped, catch-all warns), classify_walkdir, record_error episode upsert, clear_error, check_dismissed (stat-cheap skip before read_meta, self-reactivating new episode on file change). ScanError::Tags(String) -> ScanError::Import{kind,detail}. Provisional "io"/"tag" literals from Task 1.1 are gone.
  FINDING vs. the original spec: the claimed "directory errors show the error index instead of the path (shows '1')" bug does NOT exist in the code — err.path() was already bound correctly. Nothing to fix; likely a misread of the old UI. Report to the user at the review halt.
  Minor findings (final-review triage): dir-dedup test asserts only the row for the dir path, not a table-wide count; check_dismissed compares file_size which is 0 on a stat race (a dismissed zero-byte file in that window would false-positive as unchanged; very narrow, untested).
  WARNING: scanner.rs now 799/800 lines. Task 1.8 MUST extract before adding.
Task 1.8: complete (commit b340d6c, review clean) — two-pass metadata read: pass 1 fails -> pass 2 ParseOptions read_tags(false)+Relaxed; container parses => import untagged=1 with REAL duration/bitrate, title=file stem, album=parent dir. Hint coexistence: pass-2 success keeps the import_errors row (cleared only when tags become readable). Hint contract documented (derivable via EXISTS untagged track, no is_hint column). Fixture built programmatically (WAV + corrupt ID3v2 chunk), verified against lofty source. Extracted scanner_meta.rs + scanner_move.rs; scanner.rs back to 713 lines.
  Controller decision on the reviewer's ⚠️: pass 2 runs on ANY pass-1 Import failure rather than gated to UnreadableTags — accepted. PermissionDenied/UnsupportedFormat fail pass 2 identically, so it is behaviour-equivalent, costs one open attempt on the error path only, and presumes nothing about which kinds are salvageable.
  Minor findings (final-review triage): the double-failure test cannot discriminate pass-1 vs pass-2 kind (both yield Io on the garbage fixture; implementation verified correct by inspection); doc comment cites queries::PRESENT instead of queries::clauses::PRESENT.
Task 1.9: complete (commit 1874fdb, review clean) — tombstone resurrect in all 3 arms (fast-path condition widened to missing||removed), ScanReport.healed (pass-1 success that cleared an error row; pass-2 hints never count), apply_file_identity extracted to scanner_move.rs as the ONE row-refresh (move arm + later Locate), FileIdentity{file_mtime,file_size,device,inode,mount_point}. Delivered signature adds title:&str + untagged:bool beyond the plan sketch (TrackMeta carries neither) — matches the module's existing tag_param_values convention.
  Minor findings (final-review triage): fast-path tombstone test asserts updated >= 1 instead of == 1; two avoidable clones/to_string_lossy in the move arm; move-arm old-path clear_error is not covered by a healed assertion.
=== PACKAGE 1 COMPLETE (9/9). Suite: 1131 passed, 0 failed, 83 ignored (baseline 1095, +36). Core API is now frozen for packages 2/3/4. ===
Task 1.10 (added at the package-1 review halt, not in the original plan): complete (commit 045161a) — ImportErrorKind moved from library/import_errors.rs (a pub(crate) module, so the type was unnameable outside reprise-core) to models.rs next to its symmetric counterpart MissingReason. Without this, package 2's ImportErrorEntry{kind} could not be returned to reprise-gnome and a pub field of a crate-private type would trip private_interfaces under -D warnings. Cross-crate reachability proven from reprise-gnome with a temporary test.
=== REVIEW HALT PASSED. Frozen core API for packages 2/3/4/5: ===
  models: MissingReason{Unmounted,Deleted,Unknown}::{as_str,parse}; ImportErrorKind{UnreadableTags,PermissionDenied,UnsupportedFormat,Io,Unknown}::{as_str,parse}; Track{missing_since:Option<i64>, missing_reason:Option<MissingReason>, untagged:bool, ..}::is_missing()
  queries::clauses (pub(crate)): PRESENT="missing_since IS NULL AND removed_at IS NULL"; MISSING="missing_since IS NOT NULL AND removed_at IS NULL"
  library::scanner (pub): ScanOutcome{Completed(ScanReport),RootUnavailable{root:PathBuf}}; ScanReport{added,updated,skipped_unchanged,errors,moved,vanished,healed}; ScanError{Db,Sqlite,Import{kind,detail},Io}; scan_folder / scan_folder_with_progress -> Result<ScanOutcome,ScanError>
  library::mounts / library::import_errors / library::scanner_move: pub(crate) — core-internal only (mount_point_of, classify_missing, record_error, clear_error, check_dismissed, apply_file_identity)
  Schema v11. tracks: missing_since, missing_reason, mount_point, removed_at, untagged. import_errors(path PK, reason_kind, reason_detail, first_seen, last_seen, seen_count, dismissed_mtime, dismissed_size).
Task 2.1: complete (commits 045161a..49e6c08, review clean after one test-coverage fix pass) — queries/issues.rs (new sibling; maintenance.rs untouched): MissingGroupKind{Unavailable{mount_point:Option<String>},Deleted}, MissingGroup{kind,track_count}, query_missing_groups (order: per-mount unavailable -> unknown -> deleted), query_missing_rows (artist/album/track_no, paginated). unknown NEVER counted in Deleted (verified by hand + test). Fix pass closed 1 Important (per-mount branch untested — the headline "N drives = N cards" path) + 2 Minor (present-track exclusion, empty state). No defect revealed.
Task 2.2: complete (commits 49e6c08..691b00d, review clean after one fix pass) — tombstone_tracks/undo_tombstone/purge_tombstones in maintenance.rs (tests in tests_issues.rs; tests_maintenance.rs was at 702/800 — signposts added both ways). Fix pass closed a REAL TOCTOU race found by review: purge_tombstones SELECTed tombstoned ids then deleted WHERE id=? with no recheck, so the watcher thread resurrecting a row mid-purge (own thread, own connection, WAL) would still have it hard-deleted with its playlist positions + listen history — the exact failure the tombstone exists to prevent. Fixed by extending the file's existing state-guard pattern: remove_tracks_impl's missing_only:bool became RemoveGuard{Any,MissingOnly,TombstonedOnly}; the tombstone path deletes AND removed_at IS NOT NULL. All prior callers verified byte-identical. New test proven to fail against the unguarded delete.
Task 2.3: complete (commits 691b00d..83886cb, review clean after one fix pass) — AutoCleanSetting{Off,Days(u32)} in settings.rs (keys missing_auto_clean, auto_clean_armed_at; parse falls back to Off), auto_clean_eligible + run_auto_clean in issues.rs. deleted ONLY (never unmounted/unknown), deadline max(missing_since,armed_at)+days*86400<=now, inert without armed_at, off by default, now is a parameter. Fix pass closed the SAME TOCTOU class as 2.2, this time in the feature's most destructive function: run_auto_clean deleted via RemoveGuard::Any. Added RemoveGuard::AutoCleanEligible (re-checks {MISSING} AND missing_reason='deleted' at delete time) + remove_auto_clean_eligible_tracks wrapper. All 4 prior callers verified unchanged. Test proven to fail under RemoveGuard::Any.
  Controller decision: the reviewer labelled this plan-mandated (the brief named remove_tracks literally). Overruled as a non-conflict: the brief predates RemoveGuard (introduced by 2.2's fix pass). The plan's intent (hard delete, no tombstone, one established path) is fully preserved by the guard; it only stops deleting a row the scanner has since proven live.
  Doc note: deadline is deliberately NOT re-checked at delete time (time only moves forward) — recorded in 3 doc comments so nobody "fixes" it with redundant date logic.
Task 2.4: complete (commit ae4bc54, review clean, no fix pass) — new sibling queries/import_errors.rs: ImportErrorEntry{path,kind,detail,first_seen,last_seen,seen_count,is_hint}, query_import_errors_grouped (groups in enum-declaration order; rows last_seen DESC, path COLLATE NOCASE ASC), query_dismissed_import_errors, count_dismissed_import_errors, dismiss_import_error, dismiss_all_import_errors(stat callback; a path that fails to stat is SKIPPED, state untouched — never NULL-dismissed), restore_import_error (nulls dismissed_* only; the retry is the UI's job). is_hint is derived via EXISTS composing PRESENT (no is_hint column). +19 tests (reviewer confirmed genuine coverage, no padding).
  Minor findings (final-review triage): module doc says "path ASC" but SQL uses "path COLLATE NOCASE ASC"; is_hint tests cover the missing_since half of PRESENT but not removed_at; dismiss_all does one UPDATE per path without a wrapping transaction.
Task 2.5: complete (commit 9861769, review clean, no fix pass) — count_missing + count_new_missing (issues.rs), count_import_errors_active + count_new_import_errors (import_errors.rs), typed last_viewed_missing/last_viewed_import_errors settings accessors. Badge = new-since-last-viewed (first_seen > last_viewed, NOT last_seen: a permanently broken file would otherwise re-badge after every scan forever). active INCLUDES hints (row must stay reachable); new EXCLUDES them (the app solved it — asking for tags, not for help). Reused Task 2.4's is_hint_expr()/NOT_DISMISSED, no second copy. Episode reactivation badges again (verified against check_dismissed's real SQL). Badge tests split into tests_issues_badges.rs (tests_issues.rs stayed at 712, byte-identical).
=== PACKAGE 2 COMPLETE (5/5). Suite: 1186 passed, 0 failed, 83 ignored. ===
=== NEXT: integrate main (f7dcf55) — it advanced 11 commits (queue+nav rework, QUE-1..5/NAV-2/5/9) touching exactly the files packages 3/4/5 own: player_controller.rs, track_list.rs, sidebar.rs, strings.rs, queue.rs, up_next.rs, view_source.rs, ui/mod.rs. User confirmed main is stable and integration happens now, before the UI packages. Package 4 must be REPLANNED against the new queue transport layer afterwards. ===
