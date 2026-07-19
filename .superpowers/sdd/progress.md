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
=== MERGE main -> feat/missing-import-errors (commit 061a5d5). Conflict-free; all gates green afterwards: fmt, clippy -D warnings, 1236 passed/0 failed/87 ignored, audit (only RUSTSEC-2024-0436), core purity empty, check-ux-traceability "3 active rules covered", check-architecture passed. ===
IMPORTANT context change: main brought docs/ux-rules.md — a BINDING UX rulebook that already encodes this feature's 13 grilled decisions as rules (FB-4 = badges, FB-7 = tombstone/undo, PLAY-4a/4b = missing in lists, PLAY-5b = unmounted hygiene, FB-5 = StatusPages, FB-6 = watcher, SET-4 = auto-clean arming). Rules flip [geplant]->[aktiv] in the SAME commit that implements them AND lands a passing, non-ignored rule-named test (fn play_5a_...). scripts/check-ux-traceability.sh is a merge gate. Packages 3-6 were REPLANNED accordingly — see memory/reprise-missing-import-errors-replan.md (also /tmp scratchpad copy).
Four corrections the replan records: (1) PLAY-5a (queue purge on deleted) ALREADY SHIPPED on main — do not rebuild; only PLAY-5b (unmounted) remains. (2) remove_missing_track(s)/remove_all_missing_tracks are NOT retired — they are the live hard-delete API with real callers; only retire what actually loses its last caller. (3) badge core exists with ZERO gnome callers — sidebar_rebuild.rs:30-35 still shows raw totals. (4) tombstone core exists with ZERO gnome callers; ui/toasts.rs has no button support and there is no Undo-toast precedent anywhere — the 10s Undo toast is bespoke.
=== HANDOVER (2026-07-17): packages 3-6 handed to Codex. Two committed docs are now the contract:
  docs/superpowers/plans/2026-07-17-missing-import-errors-beschluesse.md — the 13 grilled decisions with their WHY (normative context; docs/ux-rules.md still outranks it).
  docs/superpowers/plans/2026-07-17-missing-import-errors-taskplan.md — packages 3-6, task by task, with the frozen core API, the four corrections vs. the original plan, rule-flip ownership, and file ownership.
  Claude's role from here: review only, per task. Packages 1-2 stay as delivered. ===
Task 3.1: complete (commit dea619d, base dae75d2, added shared issue cards, GTK button hover actions, native multiple selection, and lazy two-plus-fifty row paging with localized copy; the post-review string-catalog exception was reversed by the maintainer and replaced with the thematic strings_issues.rs sibling catalog).
Task 3.2: complete (commit 1d27cc2, base 8f3bdc8, added the grouped Missing-files view, tombstone Undo/startup purge, queue purge, safe auto-clean activation, and the FB-5a split; activated FB-7, FB-5a, and SET-4 with rule-named tests).
Task 3.3: complete (commit bd00787, base 1d27cc2, rebuilt Import errors on grouped issue cards with human taxonomy, Hint-to-Tag-Editor, off-thread Retry all, stat-bound dismiss/restore, export, and a single actionable scan-failure toast; activated FB-3 with fb_3_scan_failures_produce_one_actionable_completion_notice).
Task 3.4: complete (commit b2b3b94, base bd00787, wired new-since-viewed sidebar badges, immediate last-viewed clearing, dismissed-footer reachability, clean-source fallback, and the new-error attention dot; activated FB-4 with fb_4_badges_count_new_since_viewed_and_reactivated_episode_is_new).
Task 3.5: complete (commit 14994fb, base b2b3b94, routed sidebar Dismiss all through stat-bound dismissals and Remove all missing through the shared tombstone, 10-second Undo, expiry cascade, and queue-purge path; package 3 complete).
Task 3.5 review fix A: complete (commit c5a99e3, base c0e2986, restricted the sidebar bulk removal plan to proven-deleted tracks; committed separately because the P-6 evidence filter is independent of the dialog TOCTOU guard).
Task 3.5 review fix B: complete (commit c7b9845, base c5a99e3, revalidated stale dialog selections as still proven-deleted inside the UI transaction before tombstoning; kept the frozen generic core API unchanged because FB-7 also removes present tracks, and kept this separate from fix A so each destructive invariant has its own regression proof).
Package 3 destructive-ID audit: complete (reviewed every production track hard-delete/tombstone caller and destructive confirmation dialog; existing guarded MissingOnly, TombstonedOnly, and AutoCleanEligible paths are safe, while the pre-existing generic track remove/trash dialog and playlist-delete dialog retain stale reusable-ID race candidates outside package 3 for maintainer follow-up).

=== PACKAGE 3 REVIEWED & FIXED (Claude review + Codex fixes). ===
5 tasks (dea619d..c0e2986) + 2 fix commits (c5a99e3, c7b9845). Suite 1255 passed, 0 failed, 88 ignored (Claude's headless count; Codex reports 1256 incl. one display-gated test). All gates independently verified by Claude: fmt, clippy 0, test --workspace, architecture, qa-linters, ux-traceability "8 active rules covered", core purity 0, audit only RUSTSEC-2024-0436.
Rule flips (all verified honest — each test proves the WHOLE rule, not a convenient part): FB-7, FB-5a, SET-4, FB-3, FB-4. FB-5 split into FB-5a [aktiv] (No missing files ✓) / FB-5b [geplant] (Library folder unavailable — Retry, deferred to 5.5) — split is honest, not drawn to enable a flip.
Review found 2 defects on destructive paths, both PLAN gaps (not Codex deviations — the taskplan for 3.5 never said "deleted-only"):
  CRITICAL (fixed c5a99e3): sidebar "Remove all missing entries…" fed from query_track_ids(Missing) = presence_clause(1) = ALL three reasons. Unplugging a NAS + right-click would tombstone→hard-delete tracks on the absent drive (ratings + listen history gone) while the Unavailable card promised they'd return on mount. Violated P-6 and Beschluss 1. Fixed: missing_ids_for_cleanup now uses query_missing_rows(&MissingGroupKind::Deleted). Regression test sidebar_bulk_cleanup_selects_only_proven_deleted_tracks (red against the old code).
  IMPORTANT (fixed c7b9845): confirm_remove collected ids, opened AlertDialog (human-length delay), then tombstoned those exact ids with no recheck — a mount/scan resurrection mid-dialog would delete a present track. 4th occurrence of the SELECT-then-delete-without-recheck class (2 in package 2, 2 here). Fixed via tombstone_still_deleted: re-queries Deleted and tombstones in ONE transaction (a resurrection lands before or fails the tx safely). Codex chose the UI-recheck over a core guard, correctly reasoned: tombstone_tracks stays generic because FB-7 also removes present tracks; the Deleted precondition belongs only to this dialog surface. Regression test stale_tombstone_request_skips_track_resurrected_while_dialog_was_open.
FOLLOW-UP for package 6 (Codex audit found 2 more of the same race class OUTSIDE this feature — reusable INTEGER PRIMARY KEY ids, id held across a dialog then used destructively without an identity recheck):
  - ui/delete_tracks.rs:95 — selection captured before dialog; "Remove" deletes later with RemoveGuard::Any, no path/identity recheck; trash path also has a window between physical trash and DB delete.
  - ui/sidebar/sidebar_export.rs:137 — playlist id held across the dialog, deleted afterwards with no name/identity recheck.
  These are pre-existing (not introduced by this branch) — decide in package 6 whether they are in scope or a separate issue for the maintainer.

=== PACKAGE 4 (Codex): Playlist/Queue behaviour. 5 tasks (9f8c145..4df949a). Codex hit a "model at capacity" error at the very end, so no Codex-authored package-4 ledger entry / final report — but all 5 commits landed and the tree was clean. This entry written by Claude after review.
Suite 1265 passed, 0 failed, 88 ignored (+10). All gates independently verified: fmt, clippy 0, architecture, ux-traceability "11 active rules covered", core purity 0.
Rule flips (all verified honest AND atomic — each commit flips its rule in ux-rules.md AND lands its test in the same commit, at the correct crate level): PLAY-4a [core] (70e19f0, test in queue_ux_rules_tests.rs), PLAY-4b [gtk] (2d509df, test in track_playback_selection.rs), PLAY-5b [core] (4df949a, test in queue_ux_rules_tests.rs). PLAY-5a untouched.
Query split verified correct at all 5 variants: window+count drop {PRESENT} (missing rows at fixed position), M3U export KEEPS {PRESENT} (no dead paths exported), playable-ids keep {PRESENT} (Play all/Shuffle), visible-ids new without filter (selection/DnD). GTK recycling clean (apply_missing_title recomputes every bind). RefCell discipline clean. session_player restore switched to query_queue_retained_track_ids — without it PLAY-5b would break on every app restart.
Review found 1 Important (fix in progress): context-menu "Play" action used current_selection_ids (unfiltered), unlike the adjacent Play-next/Add-to-queue which correctly use current_playable_selection — right-click "Play" on a missing row attempts to play a known-gone file, bypassing PLAY-4a/4b's toast+explain. 5th occurrence-adjacent to the recurring "one entry point missed the guard the siblings have" pattern. Plus 1 Minor: PLAY-4b test missing its // UX PLAY-4b doc comment.

Task 4.3 review fix: complete (commit 0091b54, base cd46d7b, context-menu Play and its Queue/smoke variants now share the missing notice/playable-selection policy; expanded the PLAY-4b rule test and added its required rule doc comment; suite remains 1265 passed, 0 failed, 88 ignored).
=== PACKAGE 4 REVIEWED & FIXED. All gates green: fmt, clippy -D warnings, 1265 passed/0 failed/88 ignored, audit only RUSTSEC-2024-0436, architecture, qa-linters, ux-traceability "11 active rules covered", core purity empty. Package 5 has not started. ===
PACKAGE 4 fix (0091b54): context-menu Play now routes through handle_context_play → context_play_decision (filter to playable; any playable → Play those from the first playable position; none but a missing → Explain notice; else Noop) — same policy as double-click and Play-next. Folded into the PLAY-4b rule test (missing-only → Explain, mixed → playable-only ids). All gates green: 1265 passed/0 failed, clippy 0, traceability "11 active rules covered". Minor (PLAY-4b doc comment) fixed in the same commit. PACKAGE 4 COMPLETE & APPROVED.
LESSON (now twice): the taskplan enumerates entry points ("double-click", "enqueue") and Codex guards exactly those; a sibling entry point the plan did NOT name (pkg3 sidebar remove-all; pkg4 context-menu Play) gets missed. For package 5, the prompt must say "every path that plays/enqueues/deletes/relinks", not name a subset.
Task 5.1: complete (commit 749f72d, base b9f8cf1, added single-file relink with matcher-tolerance probe, stale id/path/presence guards, returned-old-path protection, and preservation of the existing track identity and user data; suite 1265 -> 1270).
Task 5.2: complete (commit fff990b, base 749f72d, added cancellable off-thread-ready folder relink scoped strictly to one missing group, with no imports, path-identity rechecks, and immediate termination after the last match; suite 1270 -> 1277).
Task 5.3: complete (commit 865872b, base fff990b, wired single-file Locate and folder search through the existing sidebar activity slot, bounded progress delivery, mismatch confirmation, cancellation, and outside-root honesty; suite 1277 -> 1281). FB-2 was split rather than half-flipped: FB-2a [aktiv] is proven by fb_2a_relink_search_uses_the_complete_sidebar_progress_card_contract; FB-2b remains [geplant] because package 5 does not deliver the sidebar's online cover-download half.
Task 5.4: complete (commit cbc5496, base 865872b, added ordered GVolumeMonitor reconciliation, mount-evidence healing, eager unmount marking, root rescan, one playback-fault notice followed by skip, and stale playback-path write guards; the playing track is never proactively stopped; suite 1281 -> 1286). Activated P-6 with p_6_mount_evidence_heals_existing_marks_ejected_and_never_deletes_guesses and FB-6 with fb_6_watcher_is_silent_and_playing_track_fault_has_one_notice_then_skips.
Task 5.5: complete (commit 3bb43ee, base cbc5496, added the unavailable-root scan card and StatusPage Retry state, nonzero-only aggregated heal toast, last_scan_relinked persistence, and guarded auto-clean plus queue purge after completed manual and watcher library scans only; single-file Retry/tag rereads deliberately do not run library postprocessing; suite 1286 -> 1291). Activated FB-5b with fb_5b_unavailable_library_root_shows_status_page_with_retry_only.
Task 5.6: complete (commit c25e1e0, base 3bb43ee, verified and locked the existing per-track immediate scanner reread after tag writes: readable tags clear both untagged and the import hint without a healing toast; the import-hint editor now suppresses its generic success toast while retaining error feedback; normal tag edits retain their success toast; suite 1291 -> 1294). Sibling-entry audit found and fixed one new instance of the stale identity class: the post-file-write file_mtime invalidation now rechecks id, expected path, and live-row state before mutating the database. Tests live in tag_edit_reread_tests.rs to keep tag_edit.rs below 800 lines.
=== PACKAGE 5 COMPLETE (6/6). Suite: 1294 passed, 0 failed, 88 ignored. Gates green: fmt, clippy -D warnings, workspace tests, architecture, qa-linters, UX traceability "15 active rules covered", core purity empty, audit only RUSTSEC-2024-0436. Rule activations: FB-2a, P-6, FB-6, FB-5b; FB-2b remains planned. Package 6 has not started. ===
Package 5 review fixes: complete (commit f719157, base 3e4e614). Kept in one commit because all three findings correct the already-reviewed package contract and its active-rule evidence together: (1) scanner fingerprint SQL and Locate mismatch probing now share the single MOVE_MATCH_TOLERANCE_MS source; (2) FB-2a removed decorative target/slot assertions and routes the real Missing target through the Gesture-owned activation controller into window navigation, with the non-ignored rule test exercising that controller plus a display-level signal regression test; (3) RootUnavailable overrides only an already-empty view, while populated Library/Playlist/Queue rows remain visible. The empty-state test was verified red against the prior unconditional override. Suite remains 1294 passed, 0 failed; ignored count is 89 after adding the display-level Gesture signal test. All standard gates green. The isolated display-test invocation itself is deferred to review because this sandbox rejects every private D-Bus Unix socket with Operation not permitted; compilation and the non-ignored rule test are green. Package 6 has not started.

=== PACKAGE 5 (Codex): Locate & Events. 6 tasks (749f72d..c25e1e0). Suite 1294 passed, 0 failed, 88 ignored (+29). All gates independently verified: fmt, clippy 0, architecture, ux-traceability "15 active rules covered", core purity 0, audit 0.
Rule flips (all honest, correctly leveled): FB-2a [gtk] (FB-2 split into FB-2a delivered / FB-2b geplant — honest, the relink card is delivered, the other long-runners aren't unified yet), FB-6 [core], P-6 [core], FB-5b [gtk]. Split precedent matches FB-5→5a/5b.
STRENGTHS the review confirmed: Locate TOCTOU is mustergültig — write-time recheck inside the tx in BOTH relink_track (rechecks old_path.exists + id/path/MISSING) and relink_from_folder (per-candidate still_missing before write), each with a race-reproducing test. This is the 5th encounter with the SELECT-then-write-without-recheck class and the FIRST where the implementer closed it proactively without being told — the "every entry point / recheck at write time" lesson baked into the package-5 prompt worked. Mount events: own thread + own connection, playing track never proactively stopped (grepped — no second stop path). Task 5.6 genuinely verified-and-fixed (added a stale-identity recheck the old apply_patch_batch lacked). locate_actions(kind) routes pill AND context menu through one function — the pkg3/pkg4 "sibling entry point missed" defect did NOT recur.
Review found 3 defects (fix in progress):
  CRITICAL: two independent 2000ms thresholds — relink.rs:56 (>2_000) vs scanner_move.rs:114 (<=2000 in SQL), no shared constant. Exactly the "zweite Wahrheit" Beschluss 12 forbids by name. Fix: one MOVE_MATCH_TOLERANCE_MS const referenced by both.
  IMPORTANT: FB-2a rule test asserts on RelinkProgressState.target/.slot_role fields show() never reads — the click→navigate clause of the rule has no real test. Flip must prove the whole rule.
  IMPORTANT: empty_state_for_availability ignores row_count when library_root_unavailable → blanks EVERY view (Library/Playlists/Queue) behind the StatusPage the moment the library drive ejects mid-session, hiding valid cached rows. Reintroduces the empty-library experience Beschluss 4's root guard was built to prevent; Task 5.5 only asked for the sidebar Scan-Card. Fix: only show LibraryUnavailable when row_count==0.
PACKAGE 5 fix (f719157): all 3 findings closed, independently verified by Claude.
  CRITICAL fixed: MOVE_MATCH_TOLERANCE_MS is now a single pub(crate) const in scanner_move.rs (line 18), referenced by both the SQL (via {MOVE_MATCH_TOLERANCE_MS} interpolation) and probe_relink — one source of truth, incl. the test helper.
  IMPORTANT 1 fixed: FB-2a test now proves the WHOLE rule — RelinkProgressActivation.primary_click() → asserts activated_target == Some(ViewSource::Missing) (the click→navigate clause), RelinkCancellation token flip (the abort clause), plus the card-contract formatting. The decorative .target/.slot_role fields show() never read are gone. One new display-gated widget test added (+1 ignored; no previously-passing test disabled — passed held at 1294).
  IMPORTANT 2 fixed: empty_state_for_availability now `if library_root_unavailable && row_count == 0` — the StatusPage only overrides an already-empty view; views with cached rows keep them during a root outage. The mid-session blank-screen is closed; the sidebar Scan-Card stays the sole "something's wrong" signal per Task 5.5's actual scope.
All gates: 1294 passed/0 failed/89 ignored, clippy 0, traceability "15 active rules covered", architecture, core purity 0.
PACKAGE 5 COMPLETE & APPROVED. All 15 UX rules this feature owns are [aktiv]. Only package 6 (sync-delta audit, dead-path cleanup, acceptance) + the final whole-branch review remain before merge to main.
Task 6.1: complete (commit bf8e448, base 05e1b89, sync-delta audit found no non-present copy selector: both copy callers funnel through query_sync_tracks with PRESENT; added the missing tombstone contract test; suite 1294 -> 1295).
Task 6.2: complete (commit 81f8f14, base bf8e448, grep-proven cleanup removed query_import_errors, ImportErrorRow, delete_import_error, delete_all_import_errors, remove_missing_track, and remove_all_missing_tracks plus their obsolete tests; remove_missing_tracks remains because track_actions.rs calls it; suite 1295 -> 1288).
Task 6.4: complete (commit e685312, base 81f8f14, closed the two remaining dialog ID-reuse races in one commit because both use the same write-time identity invariant: track removal and post-trash cleanup now recheck id+path inside the deletion transaction, playlist deletion rechecks id+name inside its deletion/compaction transaction; both regression tests were red against the old paths; suite 1288 -> 1290).
Task 6.3: complete (commit edb578a, base e685312, acceptance matrix maps all eight scenarios to existing P1-P5 tests without duplication; existing CUA harness cannot seed/drive the new Issues-card, badge, or Undo-toast flows, so real NAS/GVolumeMonitor, 18a optics, and those widget interactions are documented as the maintainer's manual remainder; suite 1290 -> 1290).
=== PACKAGE 6 COMPLETE (Tasks 6.1, 6.2, 6.4, 6.3). Suite: 1290 passed, 0 failed, 89 ignored. Gates green: fmt, clippy -D warnings, workspace tests, architecture, qa-linters, UX traceability "15 active rules covered", core purity empty, audit only RUSTSEC-2024-0436. No UX rule changed. Final whole-branch review is next; do not start another implementation package. ===

=== PACKAGE 6 (Codex): Integration & acceptance. 4 tasks (bf8e448..edb578a). Reviewed by Claude: APPROVED, no Critical/Important, 2 Minor carried to the whole-branch review.
Suite 1290 passed, 0 failed, 89 ignored (net -4: 7 dead-API tests removed, 2 race regression tests + 1 sync-guard test added). All gates independently verified.
6.1 sync audit: HONEST no-fix — query_sync_tracks already carried {PRESENT}; the display-only enrich query (already-synced files) correctly out of scope. Pinned with a new test asserting the removed_at half of PRESENT (the pre-existing test only covered missing_since).
6.2 cleanup: grep-proven. Removed query_import_errors, ImportErrorRow, delete_import_error, delete_all_import_errors, remove_missing_track (singular), remove_all_missing_tracks. Kept remove_missing_tracks (plural, track_actions.rs caller). Nothing silently reimplemented.
6.4 (maintainer-added) two pre-existing races fixed — BOTH recheck identity in the DELETE's own WHERE clause inside one transaction (strongest form): delete_tracks.rs → WHERE id=?1 AND path=?2 (+ trash path binds the DB delete to the actually-trashed path via trash_tracks.rs); sidebar_export.rs → WHERE id=?1 AND name=?2. Both regression tests confirmed RED against the pre-fix id-only delete. No sibling entry points left unguarded in either module.
6.3 acceptance: all 8 cases mapped to existing P1-P5 tests (verified 3/8 on spot-check), acceptance report at docs/superpowers/plans/2026-07-17-missing-import-errors-acceptance.md. CUA harness can't seed issue episodes / Missing card / Undo toast → documented as manual (real NAS/GVolumeMonitor, 18a optics, issues visibility/badges/undo), no fake smoke.
TWO MINOR for whole-branch triage:
  (a) remove_tracks (id-only RemoveGuard::Any) lost its last production caller in 6.4 (delete_tracks switched to remove_tracks_matching_paths) — now test-only; a follow-up cleanup candidate, not clippy-caught (pub lib API).
  (b) sidebar_export delete_playlist returns Ok(()) both on real delete and on a race-defeated no-op — UI shows "Playlist deleted" even when the guard rejected a stale request. Pre-existing contract the fix extended; minor toast-honesty gap vs Beschluss 7, narrower window (playlists not touched by the watcher thread).
=== ALL 6 PACKAGES COMPLETE & APPROVED. 15/15 feature UX rules [aktiv]. Next: whole-branch review, then merge to main. ===

=== WHOLE-BRANCH REVIEW (Claude, final gate before merge). Everything traced clean EXCEPT one real cross-package seam. Merge diff: 93 files, +9259/-964.
Traced end-to-end reachable: FB-7 (tombstone/undo/purge chain — exemplary, queue-purge ids threaded), FB-4 (badge active/new split correct), P-6 (mount heal/mark wired via ui/mounts.rs), PLAY-4a/4b (skip+explain wired), SET-4 (arming menu real). Cross-package contracts hold: active-includes-hints/new-excludes-hints, playable-vs-visible playlist ids. No parallel presence predicate (PRESENT/MISSING stays the one truth; query_queue_retained_track_ids is a documented DIFFERENT concept, not a drifted copy). No TODO/FIXME/todo!() in the 93 files. Migration v9→v10→v11 has real-upgrade tests, atomic per step. 2 carried minors triaged as follow-ups (orphaned remove_tracks; playlist-delete no-op toast honesty) — neither blocks.
CRITICAL (verified by Claude directly): PLAY-5a [aktiv] "deleted tracks leave the queue silently" is NOT reachable for its documented scenario. finalize_completed_scan returns only run_auto_clean ids (opt-in, default OFF); scan_watcher purges the queue only for auto_cleaned_ids; the two real purge_queue_ids callers are startup-tombstone (window.rs:202) and the Remove-from-library action (window_action_wiring.rs:371). So a track the watcher marks missing_reason='deleted' mid-session stays greyed in the queue exactly like unmounted (apply_missing_title greys both, differs only in tooltip) and is only skipped on advance — it never "leaves the queue". Violates PLAY-5a and Beschluss 11's explicit deleted-vs-unmounted asymmetry (deleted→raus, unmounted→bleibt grau). ROOT: my replan "correction #1" said PLAY-5a was already fully built and only PLAY-5b remained — that was incomplete: PLAY-5a as shipped on main only covered the remove-ACTION path, never the scan-DETECTION path Beschluss 11 requires. Fix is small (reuse query_queue_retained_track_ids + the existing notify_library_purged/purge_queue_ids plumbing at the scan postprocess points). DECISION PENDING WITH MAINTAINER: fix now vs. follow-up vs. split PLAY-5a to match reality.
Whole-branch PLAY-5a review fix: complete (commit 658d4cf, base 24ea694, both manual-scan and watcher reconciliation now project the complete Queue/Up-Next snapshot through query_queue_retained_track_ids and send deleted/tombstoned ids together with auto-clean through the existing silent purge path; unmounted/unknown and the playing present track remain untouched; play_5a_scan_detection_purges_deleted_but_retains_unmounted_and_playing_tracks was red against the prior auto-clean-only path; suite 1290 -> 1291, 0 failed, 89 ignored; no UX rule text/status changed).

=== PLAY-5a FIX (658d4cf), verified by Claude directly:
New query_queue_purge_track_ids(conn, candidates) = candidates minus query_queue_retained_track_ids (reuses the existing retain predicate — NO second truth). Wired into the shared scan postprocess (manual + watcher) alongside auto_cleaned_ids, through the existing notify_library_purged → purge_queue_ids plumbing. No toast, no extra stop path.
New rule test play_5a_scan_detection_purges_deleted_but_retains_unmounted_and_playing_tracks: real scan detects deletion (report.vanished==1), then purge leaves queue=[playing,unmounted] (deleted GONE, unmounted STAYS grey), up_next=[unmounted], current=playing untouched. Confirmed RED against old code ([3,1,2] retained instead of [3,2]). PLAY-5a stays [aktiv], now covered against its real scan-detection trigger, not just the pure Queue::remove_ids mechanism. PLAY-5b (unmounted stays) proven intact by the same test.
All gates: 1291 passed/0 failed/89 ignored, clippy 0, traceability "15 active rules covered", architecture, core purity 0, audit 0.
=== FEATURE COMPLETE & MERGE-READY. Whole-branch review's one Critical is closed. ===

## 2026-07-17 — FIL-Filter-Sichtbarkeit, Tasks 1–10

Task FIL-1: complete (commit 6fad03d, base 8a61842, Sichtbarkeitsgesetz für die permanente Filterzeile test-first ergänzt).
Task FIL-2: complete (commits 463b8ce, 2f39e8f, base 6fad03d, Browse-Chooser mechanisch extrahiert und Filterzeile mit Such-Chip, Clear-all und ruhigem Idle-Zustand aufgebaut).
Task FIL-3: complete (commit 1030f4a, base 2f39e8f, gefilterte Trefferzahlen mit quellspezifischen Gesamtzahlen gepaart).
Task FIL-4: complete (commit 3d883c9, base 1030f4a, atomarer Clear-all-Pfad für Suche und Facetten; FIL-1a aktiv).
Task FIL-5: complete (commit 72c2144, base 3d883c9, Status-Overlay auf neutrale Bibliotheksstatistik vereinheitlicht).
Task FIL-6: complete (commit 2904f99, base 72c2144, Suchfeld-Akzent bei nichtleerem getrimmtem Text; FIL-4 aktiv).
Task FIL-7: complete (commit 3ae270b, base 2904f99, Suchtreffer in Titel, Künstler, Album und Genre markiert; FIL-5 aktiv).
Task FIL-8: complete (commit 41f56cb, base 3ae270b, Ende-der-Ergebnisse-Hinweis mit Show-all-Aktion; FIL-3 aktiv).
Task FIL-9: complete (commit 96a7c0a, base 41f56cb, Nulltreffer-Zustand mit genau einem Show-all-Schritt; FIL-6 aktiv).
Task FIL-10: complete (commit 78d32be, base d485036, FIL-2 nach vollständiger Abdeckung auf aktiv gesetzt).

Pflichtprüfungen: Workspace 652 passed / 1 ignored (Core), 515 passed / 87 ignored (GNOME), 55 passed (Linux-Plattform); 87/87 isolierte Display-Tests grün; UX-Traceability 9 aktive Regeln; Architektur- und Audit-Gates grün, einzig erlaubte Audit-Warnung RUSTSEC-2024-0436 (`paste`). Die Xvfb-Abnahme nutzte ausschließlich ein temporäres Ein-Track-Profil und bestätigte Bibliothek, Playlist und Nulltreffer-Zustand sichtbar. Commit d485036 repariert sieben durch den Rebase auf `origin/main` offengelegte, veraltete oder zeitabhängige Display-Assertions und extrahiert den Toast-Helfer für das 600-Zeilen-Architekturlimit. FIL-1b bleibt bewusst `[geplant]`.
Merge-Readiness-Nachtrag: commit 388245e korrigiert zehn ungültige öffentliche Rustdoc-Links auf private `main`-Symbole; Rustdoc und Core-Purity-Proof sind danach grün.
Fix Playleiste-Abgrenzung: complete (commit 7e6dcdc, base ac093bd, Player-Leiste von GtkOverlay auf strukturelle vertikale Box umgebaut — Content/rechte Spalte laufen nicht mehr hinter der Leiste; PLAY-7 als [geplant]-Entwurf ergänzt. Verifiziert via fmt + clippy --workspace -D warnings; GNOME-Testlink in dieser Umgebung nicht möglich, da System-GTK 4.14 < gefordertem 4.22 — Display-Abnahme steht aus).
Dependabot-Merges: complete (commits b1f89e9..HEAD, lofty 0.24 / ureq 3.3 / rusqlite 0.40 / md-5 0.11 gemergt und Code migriert: lofty Timestamp-date-Accessor + PictureBuilder, ureq-3-Agent/Error/Body-API, rusqlite ohne usize-FromSql/ToSql, manuelle Hex-Signatur; drei neue clippy-1.97-Lints behoben. Gates: fmt/clippy --workspace -D warnings grün, Core 651 passed (1 umgebungsbedingter Inode-Recycling-Flake in ambiguous_duplicates_are_not_guessed, nur Cloud-Container), audit einzig RUSTSEC-2024-0436. GNOME-Link/Display-Tests weiter offen: System-GTK < 4.22).

## UX-Tooltips — Sektion M (Branch feat/ux-rules-tooltips, 2026-07-17)

Plan: `docs/superpowers/plans/2026-07-17-ux-tooltips-taskplan.md`. Konsistenz-
Sektion, kein neues Feature. Codex kam bis Task 3 + Tooling-Fix, brach dann
kapazitätsbedingt ab; Tasks 4–9 von Opus fertiggestellt (Codex' halbfertige
Task-4-Übersetzungen waren teils falsch — korrigiert).

**Aktiv (einklagbar):**
- **TIP-1a** [gtk] (f0b7699) — Icon-only ⇒ Tooltip, gelabelt ⇒ keiner, Ellipsis
  ⇒ Volltext. Test-Walk `tooltip_discipline.rs`, fünf `tip_1a_*`-Display-Tests.
- **TIP-2a** [gtk] (cfed981) — disabled icon-only nennt Grund
  (`eject_tooltip`); pure Test `tip_2a_eject_tooltip_names_reason_while_syncing`.
- **TIP-3/4/5** [manuell] (6f328c8) — RELEASING.md-Checkliste, Gate deckt sie
  über wörtliche ID-Referenz (Erweiterung in 836f486/eb9b7cd).

**Geplant (bewusst NICHT geflippt — Flip-Kriterium in gesperrten Verzeichnissen):**
- **TIP-1b** [manuell] — Verb+Objekt-Wortlaut. Verbalisierung Transport/Panel
  umgesetzt (771b02c), aber „Previous/Next" im Tag-Editor und „Back" in
  browse_bar (fremde Branches) noch Substantive.
- **TIP-2b** [manuell] — gelabelt disabled nennt Grund sichtbar. Preferences-
  Gründe umgesetzt (7e455d7), aber Save/„Change cover…" (tag_edit) und
  „Add filter" (browse) fehlen noch.

**Tooling:** `check-ux-traceability.sh` kennt jetzt die `[manuell]`-Ebene
(beidseitig geprüft); `check-display-tests.sh --rule-named` + Merge-Gate-Eintrag
machen regelbenannte Display-Tests zu Merge-Blockern; Display-Runner-Ignore gilt
als Abdeckung auch für `[aktiv]`.

**Container-Klausel-Beschluss:** Player-Bar prev/next bekommen KEINE
Einzel-Grund-Tooltips — sie werden nur mit der ganzen (dann leeren) Leiste
deaktiviert; die leere Leiste ist ihre eigene Aussage.

## 2026-07-17 — Track-Kontextmenü-Vereinheitlichung (Sektion N)

Plan: `docs/superpowers/plans/2026-07-17-context-menu-unification.md`. Vereinheitlichter Track-Row-Kontextmenü-Builder für alle fünf Kontexte via reiner, headless-testbarer Funktion. Pipeline: Fable-Plan (2 Design-Forks gegrillt), Codex-Code, 4 Opus-Reviewer, Opus-Refactor.

Tasks CTX-1–10: complete (commits 442880f..0af763a, base d14da79). Reiner Menü-Kern `track_menu.rs` (`build_track_menu` + `action_states` + `summarize_selection` + `MenuContext::from_source`, alle `ctx_*`-Tests display-frei); Adapter `build_context_menu_model` delegiert und graut Actions pro Öffnung auf beiden Pfaden (Maus + Shift+F10/Menü-Taste); „Play" und „Rescan library" (globaler Eintrag) aus dem Menü entfernt (Rescan lebt im Hamburger-Menü weiter); neue Aktionen Go-to-album/artist, Show in Files (FileManager1.ShowItems, 2 s-Timeout, Ordner-Fallback), Move to top, Show in Missing files mit Shared-Seams + Window-Wiring; Tag-Editor filtert Missing-Dateien (CTX-8). Zähl-Währung nur destruktiv (CTX-6).

Regelwerk: Sektion **N** (nach main-Merge, der L=Tag-Editor + M=Tooltips brachte). CTX-1/2/3/4/5a/6/8/9/10 `[aktiv]`, CTX-5b (sofort+Undo, hängt an FB-7) und CTX-7 (Hover/Popover-Fit, manuell) `[geplant]`. CTX-4 referenziert TIP-4 (jetzt aktiv nach Merge).

Review-Runde (4 Opus-Reviewer parallel): Korrektheit + GTK/RefCell + Spec voll sauber; 6 Test-/Kleinlücken gefunden und via Opus-Refactor (0af763a) geschlossen (CTX-4-Modell-Asserts, `from_source`-Tabellentest, Queue-Move-Edge-Tests, purer `playlist_entries`-Helper, D-Bus-Timeout, staler Doc-Kommentar). Bewusst offen: roter Remove-Dialog-Button (CTX-5a betrifft Menü-Einträge; CTX-5b räumt ihn ohnehin ab).

Gates (mehrfach nachgefahren, zuletzt nach main-Merge): fmt · clippy `--all-targets --workspace -D warnings` · `cargo test --workspace` · UX-Traceability grün. Offen: manuelle Abnahme (Hover-Farbe, Popover-Fit, Nautilus-Mehrfachmarkierung — headless nicht prüfbar, System-GTK-abhängig).
## 2026-07-17 — Queue-DnD: Up-Next-Reorder (QUE-3-Erweiterung, Owner-Entscheid)

Task Queue-Up-Next-Reorder: complete (commit 946e4d2, base d14da79, DnD-Reorder innerhalb „Up Next" erlaubt — QueueReorderOp::WithinUpNext über Queue::move_item mit Playhead-Base; Drop auf die Now-Playing-Row = „als Nächstes" (Promotion an Play-Next-Front bzw. Front-Move); Demotion und Now-Playing-Drag bleiben abgelehnt. Drop-Indikator zeigt nur noch echte Ziele: Drag-Ursprung wird in Shared::active_reorder_drag_from gestasht, connect_enter prüft dieselbe reorder_op wie der Drop-Handler. QUE-3-Text in ux-rules.md angepasst, bleibt [geplant]. Auslöser: Nutzer sah Drop-Marker in „Up Next", aber Drops wirkten nicht — die alte Regel lehnte Snapshot-interne Reorders bewusst ab, der Indikator leuchtete trotzdem überall. Gates grün (652/517/55, clippy -D warnings, audit nur RUSTSEC-2024-0436); drei isolierte Headless-Smokes: reorderqueue 2-4 moved=true, 2-0 Promotion moved=true, 0-3 rejected. Offen für manuelle Abnahme: echte Pointer-Geste + sichtbarer Indikator (headless nicht prüfbar).)

## 2026-07-17 — Queue-View-Absturz + Spalten-Header-DnD

Crash-Fix Sektions-Header: complete (commit 9c6dcde, base 946e4d2). Nutzer-Crash `gtklistitemmanager.c:1328 assertion (header != NULL && header->widget == NULL)` beim Wechsel in die Queue-View aus einer tief gescrollten größeren Ansicht. Root Cause: `reload` flippte die Header-Factory ZWISCHEN `set_sections` und `set_query` — GTKs `set_has_sections` ruft `ensure_items` synchron auf und sah neue (kleine) Queue-Ranges gegen den alten (großen) Zeilenbestand; für getrackte Positionen hinter dem Range-Ende lieferte `section_for` den überlappenden Fallback `(0, total)` → Assert. Deterministischer Pointer-Repro (Wheel-Scroll tief, Selektion oben = zwei Tracker-Bereiche, Sidebar-Queue-Klick): Abort auf altem Code, überlebt mit Fix. Dreiteiliger Fix: Factory-Flip strikt NACH dem Query-Swap; `section_for`-Fallback kachelt den Rest als eigene Tail-Sektion (nie überlappend); Factory-Wechsel nur noch bei echter Transition (kein Header-Rebuild pro Queue-Reload mehr). Stress-Regression: 57 Queue-Reloads, 24 Auto-Advances, 0 Criticals.

Spalten-Header-DnD: complete (commit 8fa08f5, base 9c6dcde, Umsetzung Sonnet-Agent, Review+Verifikation Fable). GTKs natives Column-Reorder ist in 4.22 tot (Title-Click claimt beim Press, cancelt die Threshold-Claim-Drag-Gesture der View — Stock-GTK-Python-Repro in scripts/upstream-repros/gtk-columnview-header-drag.py, auf gtk main unverändert). Eigenes Modul column_header_dnd: Capture-GestureDrag auf der ColumnView, Claim beim Press (Resize-Zone ±6px und Button 3 bleiben GTK), Live-Adjacent-Swap via remove/insert_column (Persistenz über bestehenden wire_order_persistence-Listener), activate_sort-Reimplementierung für den Plain-Click; set_reorderable(false) gegen künftiges Double-Handling. Neuer ptr-e2e-Flow PTR_E2E_COLREORDER_ONLY=1 komplett grün (Sort-Klick field=title, Hin-Drag persistiert, DB-Order-Check, Rück-Drag stellt Ausgangsordnung her, keine Criticals).

- OFFEN (Upstream): GTK-Issue einreichen — GtkColumnView reorderable=TRUE wirkungslos (Title claim-on-press vs. Threshold-Claim); Repro liegt in scripts/upstream-repros/. GNOME-GitLab-Account nötig.
- OFFEN (Harness): ptr-e2e geometry.sh ist seit Redesign/FIL-Filterzeile verschoben (Header real ~y=140 statt 120; x=500 = Artist statt Title) — HEADER_ONLY-Flow schlägt aktuell auch auf Baseline fehl. Rekalibrierung aller Flows als eigener Task.
- Gates beider Commits grün: fmt, clippy --workspace -D warnings, Tests 652/524/55, audit nur RUSTSEC-2024-0436.

## 2026-07-17 — Spalten-DnD-Feinschliff + Header-Rechtsklick (Nutzer-Feedback live)

Spalten-Drag Marker-Rework: complete (commit aeb8b9a, base 7678192, Umsetzung Sonnet-Agent, Review+Verifikation Fable). Nutzer-Feedback zum ersten Wurf: Live-Swap = Design-Chaos + laggy, dnd-Klasse färbte ganze Header dauerhaft (Klassen-Leak auf verwaiste Title-Widgets nach jedem Swap). Jetzt wie Row-DnD: Spalten stehen während des Drags, schmale Akzent-Einfügelinie (box-shadow-Idiom der Row-Indikatoren) auf exakt einem getrackten Widget (Cleanup bei End+Cancel), genau EIN remove/insert + EIN Persist beim Loslassen, Quell-Titel nur gedimmt. Slot-Mathematik pur + 14 Tests (column_header_dnd_tests.rs ausgelagert, 800er-Regel). ptr-e2e-Flow um Genau-ein-Persist-Assert erweitert, komplett grün.
Header-Rechtsklick-Popover: complete (commit ac81316). install_header_popover existierte, verlor aber die Title-Claim-Race (Bubble- statt Capture-Phase) — Rechtsklick zeigte GTKs nacktes Sichtbarkeitsmenü statt des Editors. Fix: Capture + Claim; zusätzlich Fenster-Buttons (─ □ ×) aus der Popover-Variante der Editor-HeaderBar entfernt (Dialog/Preferences behalten sie). Headless mit echtem Rechtsklick + Screenshots verifiziert.
- HINWEIS: ptr-e2e column-header-menu.sh (Flow 1b) testet das jetzt unerreichbare GTK-Nativmenü — bei der ohnehin anstehenden Geometrie-Rekalibrierung auf das Editor-Popover umschreiben.

## 2026-07-18 — UX-Motion Phase 2 (Sektion O)

Task T1: complete (commit 1158d56, base a74acb8, Sektion O mit MOT-1–MOT-7 zunächst geplant ergänzt; O war nach N der nächste freie Buchstabe).
Task T4: complete (commit 98fbeb2, base 1158d56, linke Sidebar auf OverlaySplitView/Start umgebaut, Breakpoint- und Fokusvertrag erweitert, innere und äußere Flächen auf Standard-Crossfade vereinheitlicht; MOT-3 aktiv).
Task T5: complete (commit c595860, base 98fbeb2, Device-Progress und Scan-Pulse zentral über animations_enabled gegatet, Rest-Transitions inklusive Missing-Progress tokenisiert, Motion-Lint-Allowlist geleert; MOT-7 aktiv).
Task T6: complete (commit eee49b1, base c595860, Scan-, Device-, Missing- und Lyrics-Hintergrundflächen per regelbenannten Tests als Crossfade/ohne Layoutverschiebung fixiert; MOT-2 aktiv).
Task T9: complete (commit 7dd1212, base eee49b1, MOT-4 in die manuelle GNOME-QA-Checkliste aufgenommen; MOT-1, MOT-4 und MOT-6 aktiv, MOT-5 bewusst geplant).
Task T10: complete (base 7dd1212, volle Gate-Batterie grün: fmt, clippy locked -D warnings, Workspace-Tests (735/643/55, 0 failed), UX-Traceability 49 aktive Regeln, Motion-Lint (Allowlist leer), Architektur; alle wesentlich geänderten Code-Dateien unter 800 Zeilen). Display-Tests headless auf displayfähigem Host (dbus-run-session + xvfb) durch den Orchestrator ausgeführt: alle 15 mot_-Tests grün, u. a. mot_3_left_sidebar_matches_the_info_panel_and_roundtrips_at_the_breakpoint (Sidebar-Umbau + Breakpoint/Fokus), mot_2_* (Hintergrundflächen crossfaden ohne Layoutverschiebung) und mot_7_disabled_animations_never_start_the_scan_pulse_timer. Rein manuell/optisch bleibt nur das gefühlte Anfühlen der bewussten Flächenänderung 150→250 ms und die MOT-4-Sichtprüfung mit einer 10k-Library. Folge-Branch: MOT-5-Neuverhalten (Play/Pause-Scale-Puls, Waveform-Crossfade beim Trackwechsel, Pause-Entsättigung des Waveform-Fills) plus Queue-Drop-/Single-Remove-Animation.

=== MERGE main (104 commits: queue-playlist-improvements, ux-rules-tooltips, tag-editor-rework, i18n) INTO feat/missing-import-errors. 11 conflicted files, 25 hunks, resolved preserving BOTH features' intents:
- ledger: union. playlist.rs: doc follows code (missing stays visible, my Beschluss 11). maintenance.rs: kept my mark_track_missing_if_current TOCTOU recheck, dropped main's dead remove_missing_track (grep-confirmed 0 callers).
- library/tag_edit.rs: kept my identity-guarded prepare_tag_reconciliation AND main's new kind:WriteErrorKind field.
- tag_edit_flow.rs (sub-agent, 8 hunks): begin_for_path ported to main's reworked present/SessionTrack API; my "import-hint success is silent" preserved via origin:ApplyOrigin.
- columns.rs: combined main's search-highlight (present rows) with my grey+strikethrough (missing rows) via a branch, respecting the shared Pango attribute list.
- empty_state.rs (6 hunks): union of both functions + both test sets; combined per-arm set_child (main) with my remove_css_class. Caught a real merge bug: dropping the builder's initial set_child (for main's per-arm model) meant LibraryUnavailable had to set the retry-actions child itself, and MissingClear had to clear to NONE.
- track_list.rs/builder.rs/reload.rs: unioned both features' struct fields + constructor init; reload combined main's new browse_filter_count signature with my availability-aware empty state.
Post-merge compile fixes: dropped a duplicate set_empty_scan_widget (kept main's field-storing version, moved it into track_list_missing.rs as a scan seam to keep the orchestrator <600 lines); removed an unused toasts import; added two trailing semicolons in the sub-agent's finish_apply closures.
Verified: 1417 passed / 0 failed / 96 ignored (both features' suites united), clippy 0, fmt clean, ux-traceability "34 active rules covered", architecture + qa-linters pass, core purity 0, audit 0. BOTH features' rule tests green — the merge broke neither.

=== MERGE ROUND 2: main advanced 11 more commits (context-menu unification / CTX rules, section N) that landed while round 1 was being resolved (all CTX commits timestamped <= 23:39, round-1 merge at 23:56 — main is NOT actively moving, this is the final catch-up). 7 conflicted files, 13 hunks:
- maintenance.rs doc: took mine (main's referenced remove_missing_track/remove_all_missing_tracks, both deleted in my package 6). strings.rs: kept my remove-from-library toasts. track_list.rs: unioned gio imports + the callback-alias set (dropped OnPlaySelected — main deleted it; kept both OnShowMissing (my PLAY-4b) and OnShowMissingFiles (main's CTX)).
- Context-menu cluster (sub-agent, 4 files/10 hunks): both main's CTX unification (unified adapter, Move-to-top, Show-in-Files, go-to-album/artist) AND the feature's PLAY-4b (play-explains/enqueue-filters-to-playable via context_play_decision) + Remove-from-library (remove_missing_selected → remove_missing_tracks → purge_queue_ids, ViewSource::Missing-guarded). Notable: remove_missing_selected + handle_remove_from_library + their tests had been silently dropped pre-merge outside any conflict marker; the sub-agent restored them. handle_play rewritten to shared.on_activate (main deleted on_play_selected). ACTION_RESCAN_LIBRARY deliberately NOT restored (main's ctx_2_no_global_entries asserts it's gone).
- queue_transport.rs hit 817 lines (my PLAY-5a purge + main's DnD/move-to-top united) → extracted the test module to queue_transport_tests.rs (repo's #[path] sibling pattern), back to 646.
Verified: 1431 passed / 0 failed / 96 ignored, clippy 0, fmt clean, ux-traceability "43 active rules covered", architecture + qa-linters pass, purity 0, audit 0. BOTH features' full rule sets green.

## 2026-07-18 — Generated-metadata scalability baseline

Branch: feat/performance-optimizations
Base: e0493d0
Lock: claimed by Codex in this worktree on 2026-07-18
Stage: isolated 10,000/100,000-track query and scroll/cache measurement baseline

- PERF-1: complete (commit f9d3ac4, base e0493d0, added a fail-closed synthetic query baseline with stable JSON timing output and a verified 10,000-track run).
- PERF-2: complete (commit 679f15a, base f9d3ac4, added 10,000/100,000-track scroll probes with independent hard budgets of eight cached windows and 1,600 retained rows).
- PERF-3: complete (commit 4ec74b9, base 679f15a, added the clean-worktree release orchestrator, reproducibility manifest, retained artifacts, CLI policy tests, and operator documentation).

Stage review: complete. A clean-commit release run at `4ec74b9` passed for both 10,000 and 100,000 generated tracks; TrackListModel retained exactly eight SQL windows / 1,600 rows at both sizes (about 21 ms and 251 ms respectively on this host). Final gates passed: fmt, clippy `--all-targets --workspace -D warnings`, workspace tests (core 758 passed / 1 ignored; GNOME 668 passed / 138 ignored; platform 55 passed), architecture, UX traceability (60 active rules), core purity, QA linter policy, and audit with only the accepted RUSTSEC-2024-0436 warning. Assumption: elapsed times are same-host comparison evidence, not portable pass/fail thresholds; only the cache bounds are hard budgets. Deferred by scope and recorded in `TESTING.md`: installed-app startup, live GTK row-widget/provider counts, queue-memory growth, and manual rendered scroll feel. Residual risk / next-stage candidate: deep title-sorted SQL `OFFSET` windows grow roughly linearly with position at 100,000 tracks; keyset or anchored paging needs a separate design stage rather than an unmeasured optimization here. No product behavior, real database, music file, or live desktop was touched.

Lock: released by Codex in this worktree on 2026-07-18

## 2026-07-18 — Installed-runtime scalability measurements

Branch: feat/performance-optimizations
Base: 217407e (merged local main at 1ce9405 before implementation)
Lock: claimed by Codex in this worktree on 2026-07-18
Stage: installed-app startup, live GTK row/provider counts, queue memory, and isolated visible scroll response at 10,000/100,000 generated tracks

- PERF-4: complete (commit 10894c7, base 217407e, added fail-closed installed-DESTDIR startup measurement with generated app-ready profiles and five CUA-observed starts per size; host execution remains explicitly deferred where the managed sandbox blocks private display sockets).
- PERF-5: complete (commit 1405eff, base 10894c7, added real-process GTK widget/type, row/cell, provider/model, and SQL-cache reporting with deterministic runtime bounds).
- PERF-6: complete (commit be8dfdf, base 1405eff, added fresh-process queue RSS and deterministic logical-payload measurement; local release medians were 159,744 bytes at 10,000 tracks and 1,609,728 bytes at 100,000 tracks).
- PERF-7: complete (commit bbd3255, base be8dfdf, added the snapshot-action-snapshot scroll benchmark, installed-runtime orchestration, deterministic bounds, retained evidence, operator documentation, and paired commit comparison; follow-up 40b568b makes blocked private-Xvfb hosts fail before the expensive build with bounded diagnostics).

Stage review: complete. The benchmark implementation and deterministic portions pass all required gates: fmt, strict workspace clippy, workspace tests (core 758 passed / 1 ignored; GNOME 669 passed / 138 ignored; platform 55 passed), both example suites, runtime/compare shell contracts, architecture, UX traceability (60 active rules), QA policy, core purity, and audit with only accepted RUSTSEC-2024-0436. Five fresh release processes measured Queue RSS medians of 159,744 bytes for 10,000 tracks and 1,609,728 bytes for 100,000 tracks (15.97 and 16.10 bytes/track; deterministic logical payloads 160,000 and 1,600,000 bytes). Installed startup, realized GTK rows/cells, and CUA scroll values are host-deferred: this managed sandbox rejects private Xvfb sockets before app launch; the clean-commit preflight now exits in under one second with a 588-byte diagnostic and never falls back to the real desktop/profile. Assumptions: timing comparisons use the same host/build conditions, database page cache is host-controlled and warm after profile seeding, and X11 snapshot response is an observable responsiveness proxy rather than native Wayland pointer feel. Residual risk: the host-deferred axes still need one complete `scripts/performance-runtime-baseline.sh` run before performance claims about installed GTK behavior; `scripts/performance-compare.sh` then provides exact before/after deltas.

Lock: released by Codex in this worktree on 2026-07-18

## 2026-07-18 — Benchmark-driven deep-window optimization

Branch: feat/performance-optimizations
Base: c336590
Lock: claimed by Codex in this worktree on 2026-07-18
Stage: identify, test-drive, and compare the highest-value reproducible 100,000-track query bottleneck

- PERF-OPT-1: complete (commit ddaa3f3, base c336590, captured the title-window SQLite plan in the stable benchmark contract; the 100,000-track before run measured 6,496/27,348/48,874 us medians for first/middle/final windows and confirmed a full scan plus temporary ORDER BY B-tree).
- PERF-OPT-2: complete (shadow commit bf8394d, base ddaa3f3, schema v12 adds a partial NOCASE title index for present tracks; migration, plan selection, data preservation, and visible ordering are regression-tested. The primary worktree Git metadata became read-only before this commit, so the preserved shadow commit is bundled at stage close-out).
- PERF-OPT-3: complete (shadow commit b3644cc, base bf8394d, added a fail-closed generated-query comparison report covering storage, open time, first/middle/final windows, playback IDs, and before/after SQLite plans).

Stage review: complete. A clean release pair compared committed baseline `ddaa3f3` with shadow candidate `b3644cc` at 10,000 and 100,000 generated tracks. At 100,000 tracks the first/middle/final title windows changed from 7,649/31,263/53,605 us to 157/635/1,333 us (-97.95/-97.97/-97.51%), and playback-id projection changed from 8,125 to 298 us (-96.33%). At 10,000 tracks the final window changed from 5,981 to 216 us (-96.39%). SQLite changed from a full scan plus temporary ORDER BY B-tree to `SCAN tracks USING INDEX idx_tracks_present_title_nocase`; the index adds 2,379,776 bytes at 100,000 tracks (+9.85%). Seven direct copies of the synthetic v11 database measured the one-time v12 index build at 36,634 us median (26,547–37,247 us). Repeated database-open timing did not regress in this pair (443 to 359 us at 100,000 tracks), but remains same-host noise rather than a performance claim.

Final gates passed: fmt, strict workspace clippy, workspace tests (core 758 passed / 1 ignored; GNOME 669 passed / 138 ignored; platform 55 passed), architecture, UX traceability (60 active rules), QA policy including both comparison contracts, core purity, and audit with only accepted RUSTSEC-2024-0436. One unrelated `one_shot_task` async test observed an intermediate progress value in the first full run; it passed five focused repetitions and the complete rerun. Adversarial benchmark review caught and rejected two candidate reports contaminated by an older baseline binary in the local Cargo target; a full target rebuild produced the accepted index-selecting comparison.

Assumptions: same-host release medians are comparison evidence, the partial index deliberately optimizes the default present-library title order only, and row ordering among equal NOCASE titles remains SQLite row-id order as before. Residual costs/risks: each visible-track insert/update maintains the additional index; its scan-write overhead is not yet measured, other sort fields and filtered searches still need separate plan evidence, and the installed GTK/startup/CUA axes remain host-deferred because this sandbox blocks private Xvfb sockets. No real database, music file, live desktop, or user profile was touched.

Git handoff: the primary worktree Git metadata became read-only after `ddaa3f3`; commits `bf8394d` and `b3644cc` plus this close-out are preserved on the local shadow branch and in `.superpowers/sdd/performance-optimization-stage.bundle`. The primary branch itself was not advanced or pushed.

Lock: released by Codex in this worktree on 2026-07-18

## 2026-07-18 — Theme-Flächenhierarchie + Theme-Akzent-Fallback

Tasks S1–S3: complete (commits `0bfe79e`, `38d1f02`, `34e27a9`; Dark-Paletten auf die 14a-Hierarchie gebracht, linke Library-Sidebar und Headerbar mit gescopten 1-px-Hairlines getrennt, statischen Player-Fallback je Dark-/Light-Palette mit dem Theme-Akzent vereinheitlicht und Cover-Override am Fallback-Endpunkt freigegeben). Verifikation: fmt, clippy locked `--all-targets --workspace -D warnings`, Workspace-Tests 760 Core + 659 GNOME + 55 Platform grün (0 fehlgeschlagen, 116 GNOME-Display-Tests ignoriert), UX-Traceability 49 aktive Regeln, Architektur und Audit grün (nur akzeptiertes RUSTSEC-2024-0436); alle berührten Code-Dateien <800 Zeilen, kein Orange-Fallback-Literal mehr im GNOME-Quelltext. Pending display verification: `chrome_separator_css_parses`, `library_split_is_scoped_for_chrome_separators`, `mot_6_replacing_an_accent_fade_skips_the_previous_animation` sowie Screenshots aller drei Dark-Themes für Hairlines, Flächenhierarchie und Player-Fallback — isolierter `dbus-run-session`/Xvfb-Versuch scheiterte im Sandbox mit `Operation not permitted`. Annahme: das bestehende `#1CA98F` in `artist_detail_pane.rs` ist ein separater Artist-Hero-Glow außerhalb dieser Lane und blieb unverändert.

## 2026-07-18 — Now-Playing-Panel (NPP)

Stage T1–T9 complete (base `0ff250d`; commits `90d44a4`, `326239b`, `2dbb729`, `fd23e54`, `a1d577e`, `700be6b`, `456ab0b`, `d7e68fb`, `d83f56a`): fixed 240/300 px side-panel geometry; renamed and player-bound the Now Playing panel; added the 21a stage/head/pill, session-only tabs, Up Next, lyric hierarchy/fallbacks, centered scroll pause/seek, and the shared reduced-motion-aware track crossfade; NPP-1–10 are active as planned. Final gates green: fmt, locked clippy `-D warnings`, workspace tests 1470 passed/124 ignored (core 757/1, GNOME 658/123, platform 55/0), UX traceability 49 active rules, architecture. Pending display verification (sandbox blocks D-Bus/Xvfb): 25 touched-area ignored tests, including `npp_2_no_volume_in_panel`, `npp_4_tab_persists_in_session`, `npp_7_user_scroll_pauses_autoscroll`, `npp_8_line_click_seeks`, `npp_9_errors_offer_only_inline_retry`, `npp_10_track_change_uses_one_shared_crossfade`, and `npp_10_new_lyrics_start_with_line_zero_centered`.

## 2026-07-18 — MOT-5 aktiv (Motion-Regelwerk vollständig)

MOT-5 Flip: complete (base 40b1492). Alle drei geforderten Verhalten sind implementiert und per regelbenanntem [gtk]-Test gedeckt: Scale-Puls beim Play/Pause-Wechsel (mot_5_play_pause_pulses_on_state_change), Waveform-Crossfade beim Trackwechsel (mot_5_waveform_crossfades_to_the_new_track_instead_of_rebuilding), Pause-Entsättigung des Waveform-Fills inkl. PlayerBar- und Compact-Wiring (mot_5_pause_desaturates_the_waveform_fill_and_play_restores_it, mot_5_player_bar_state_propagates_pause_to_waveform, mot_5_compact_player_state_propagates_pause_to_waveform). Damit sind MOT-1..MOT-7 alle [aktiv] und Sektion O ist abgeschlossen.

Bewusst NICHT umgesetzt: die MOT-4-Queue-Ausnahme (DnD-Drop/Einzel-Remove animieren). Sie ist erlaubend, nicht fordernd, und über den TAG-1-reload()-Pfad (SQL-Requery + Model-Swap) gäbe es keine animierbare Zeilenidentität — echte Umsetzung wäre ein eigener Architektur-Beschluss im Queue-Kontext.

Verifikation unter der vorgeschriebenen Isolation (dbus-run-session + xvfb-run + GDK_BACKEND=x11 + leeres WAYLAND_DISPLAY): --motion 25/25, --css 7/7, Workspace 759/659/55, fmt/clippy/traceability/motion-lint/architecture grün. Lektion aus diesem Branch: xvfb-run allein isoliert auf einem Wayland-Host nicht — ohne GDK_BACKEND=x11 und leeres WAYLAND_DISPLAY hängt sich GTK an den echten Compositor, und backend-abhängige Defekte (X11 verteilt neu registrierte Frame-Clock-Ticks synchron) bleiben unentdeckt.

## 2026-07-18 — Search Bar + New Releases

Stage A1–A5/B1–B7: complete (base `e0493d0`; task commits `eb06fcf`, `824811d`, `aa53e0e`, `f235d05`, `6a7503a`, `40c3ea9`, `e212b7e`, `a6fe27d`, `7642199`, `ddcfcb2`, `95039ca`, `bbcfbf5`). Search is now a revealed `GtkSearchBar` with active-query chip/toggle projection and two-stage Escape; New Releases has the v12 schema/MBID scan path, unified rate-limited MusicBrainz pipeline, lazy release-group covers with accent fallback, transient badge/popover, Back/Forward digest with hide/restore, and a default-off live `new_releases` plugin with “Top artists only / All artists”. B8/DISCOVER-1 and gating for cover, portrait, and lyrics remain deliberately deferred to `feat/network-opt-in` by the binding plan change. Final gates: fmt, locked clippy `-D warnings`, workspace tests 772 Core + 676 GNOME + 55 Platform green (0 failed; 1 Core and 143 GNOME ignored), UX traceability 72 active rules, architecture, core purity, and audit green except accepted `RUSTSEC-2024-0436`. Pending display verification: `search_1_idle_is_icon_not_field`, `search_2_ctrl_f_reveals_and_focuses`, `search_3_active_query_shows_chip_when_collapsed`, `search_4_escape_clears_then_collapses`, `nr_3_header_button_is_visible_only_when_releases_exist`, and `nr_7_header_button_stays_hidden_with_cached_releases_while_disabled`; fully isolated D-Bus/Xvfb attempts were blocked by sandbox socket binding (`Operation not permitted`).

## 2026-07-18 — Search Strip + Queue Unification

Stage A1–A3/B1–B6: complete (base `2783fa4`; commits `42ee650`, `1694634`, `b117563`, `ef1bd31`, `7225bb5`, `84fb69d`, `83f7dc3`, `b043061`, `24b7fa0`). Search is a full-width styled second toolbar with a 450 px clamp and active lens for an open bar or retained query. Queue management remains in the Sidebar ColumnView while the player-bar icon opens the shared Up Next panel projection with conditional manual/context headers, exact-entry jump/remove, batched metadata, lazy recycled rows, a closed-panel render guard, and shared thousands formatting; QUE-3/4 wording was aligned with the binding Beschlüsse 4/5 in B6. Final gates: fmt, locked clippy `-D warnings`, workspace tests 775 Core + 682 GNOME + 55 Platform green (0 failed; 147 GNOME ignored), UX traceability 78 active rules, architecture, core purity, and audit green except accepted `RUSTSEC-2024-0436`. Pending display verification: the formal one-test-per-process runner is blocked before its first test because sandboxed D-Bus cannot bind `/tmp/dbus-*` (`Operation not permitted`); touched live checks include the search-strip reveal/lens, queue-icon routing, conditional panel headers, and exact-row Remove control.

## 2026-07-18 — Netz-Features opt-in

Stage T1–T7: complete (base `c2569e8a`; commits `59237326`, `ada28db4`, `4daa3648`, `9ce36442`, `c524e3b6`, `0ee4da21`, `8df4cb8d`): Sektion T und die opt-in/live schaltbaren Netzmodule sind vollständig umgesetzt, Bestandsnutzung wird evidenzbasiert übernommen, Cover-/Portrait-/Lyrics-Abrufe sind korrekt gegatet, kontextuelle Einmal-Hinweise werden nicht gestapelt und ihre Deep-Links heben die neu aufgebauten Plugin-Zeilen kurz hervor. Abschlussprüfung: fmt, locked clippy `-D warnings`, Workspace-Tests, UX-Traceability (85 aktive Regeln), Architektur, Core-Purity, Übersetzungsprüfung und Audit grün bis auf das akzeptierte `RUSTSEC-2024-0436`; alle Quellfiles unter 800 und alle definierten UI-Orchestratoren unter 600 Zeilen. Pending display verification: der Ein-Prozess-pro-Test-Runner scheitert vor dem ersten GTK-Test, weil der Sandbox-D-Bus keinen `/tmp/dbus-*`-Socket binden darf (`Operation not permitted`); Rendering, Fokus/Scrollen und die kurzlebige Hervorhebung bleiben daher auf einem Host-Display manuell zu prüfen.
## 2026-07-18 — Album-Grid-Verbesserungen (GRID-1–5, NAV-9a)

Stage T1–T7 complete. T1: `e4d1dd09` (Regeln GRID-1–5/NAV-9a geplant und NAV-9 append-only ersetzt). T2: `ecea9a4c` (Disc-Persistenz und kanonische Albumreihenfolge). T3: `74dca794` (persistenter Playing-Layer, GRID-1). T4: `22e1fb65` (native Tastatur- und exakt fünf Kontextaktionen, GRID-2). T5: `956c2fab` (Fokus-/Hover-/Playing-Komposition, GRID-3), mit Recycler-Fix `a1bbad75`. T6: `2c611e61` (Bottom-Gradient, Metazeile und Ellipsis-Tooltip-Disziplin, GRID-4). T7: `2eae8c82` (Playerbar-/NPP-Reveal und NAV-9a-Aufteilung, GRID-5/NAV-9a). Der abschließende 23-Punkte-Review fand einen wirkungslosen doppelten History-Eintrag durch die interne Library/Tracks-Sidebar-Route; `4439e51f` reproduziert und unterdrückt ausschließlich diesen internen Push.

Abschluss-Gates: `cargo fmt --check`, Workspace-Clippy mit `-D warnings`, Workspace-Tests (Core 761 passed; GNOME 683 passed / 143 display-gated ignored; Linux-Plattform 55 passed), UX-Traceability mit 66 aktiven Regeln, Architektur, Core-Purity und alle Code-Dateien <800 Zeilen grün. `cargo audit` prüfte 1.166 Advisories aus einer beschreibbaren Cache-Kopie; ausschließlich das erlaubte `RUSTSEC-2024-0436` (`paste` via `lofty`) wurde ignoriert. Keine reale Bibliothek, Musikdatei oder Reprise-Datenbank wurde verwendet.

`deferred host check`: Die isolierten GRID-1/3/4/5-Render-, Fokus-, Pointer- und Pulsprüfungen sowie die gemeinsame Playerbar/NPP-Reveal-Abnahme bleiben auf einem displayfähigen Host auszuführen. `dbus-run-session`/Xvfb scheiterte in dieser Sandbox beim privaten Socket mit `Operation not permitted`; der private CUA/AT-SPI-Fallback konnte ebenfalls keinen isolierten Xvfb-Socket binden. Die vollständige manuelle Abnahme steht in `RELEASING.md`. Annahme: Falls die geladene Trackzeile während der Wiedergabe nicht mehr in der DB existiert, verwendet GRID-5 den geladenen Track-Artist nur als Lookup-Fallback, damit der vorgeschriebene NAV-9a-Fallback erreichbar bleibt.

Integration: complete (merge commit `9bf482b1`, base `c2569e8a`, feature tip `fdd733c8`). Die Konfliktauflösung bewahrt New Releases als Schema v12 und ergänzt Disc-Persistenz als v13; Queue-/NPP-Routing und GRID-5/NAV-9a bleiben gemeinsam erhalten. Der Merge-Readiness-Lauf ist bis zu den sandbox-blockierten Display-Tests grün: fmt, locked clippy, Rustdoc, 778 Core + 698 GNOME + 55 Plattform bestanden (153 GNOME display-gated ignoriert), 85 aktive UX-Regeln, Motion-Lint, Architektur/File-Size, Core-Purity und Audit mit ausschließlich der erlaubten Ausnahme. Weil das gemeinsame Live-`.git` keine Ref-Schreibrechte erlaubt, bleibt `main` dort unverändert; die vollständige geprüfte History ist als Git-Bundle gesichert.

Follow-up GRID-6: complete (commit `70e678ef`, base `e3e9d1bc`). Back aus einem Album-Detail stellt den Fokus auf der verlassenen, im aktuellen Filter sichtbaren Albumkachel wieder her, scrollt sie bei Bedarf sichtbar und löst weder Suchfeld-Clear noch GRID-5-Reveal-Puls aus. Der regelbenannte Headless-Test ist grün; der GTK-Fokustest bleibt display-gated. Alle Integrations-Gates sind grün: fmt, locked clippy, Rustdoc, Workspace-Tests (778 Core + 699 GNOME + 55 Plattform; 154 GNOME display-gated ignoriert), 86 aktive UX-Regeln, Motion-Lint, Architektur/File-Size, Core-Purity und Audit mit ausschließlich der erlaubten Ausnahme. Die manuelle Reduced-Motion-Anweisung verwendet jetzt GNOMEs System-Animationsschalter, weil XSettings eine isolierte GTK-`settings.ini` übersteuern kann.

## 2026-07-18 — Bilingual GitHub engineering showcase

Branch: feat/performance-optimizations
Base: a41c53f
Lock: claimed by Codex in this worktree on 2026-07-18
Stage: update the private source README and public reprise-showcase for application use

- SHOWCASE-1: complete (commit 5ac13860, base a41c53f, repaired six public Rustdoc comments that linked to private query helpers; the warning-denied documentation gate was red before the fix and green after it).
- SHOWCASE-2: complete (commit 787f9c6e, base 5ac13860, rewrote the source README as a bilingual evidence-led engineering case study, added the German README and a gated drift contract, updated the automated test baseline, and documented the measured performance/architecture/UX/AI roadmap without presenting planned work as shipped).
- Public showcase: complete (`marvinbaudach/reprise-showcase` main commits 54cb700b, a3218ab, and 3cfa7104; refreshed the English landing page, added the German translation, then corrected its locale-specific number formatting). Both files were fetched back from GitHub and checked for the project date, analyzer totals, benchmark results, test/UX counts, and roadmap status.

Stage review: complete. The Bewerbung analyzer measured committed source HEAD a41c53f at 88,789 Rust code lines (58,053 product and 30,736 test; CV display 58,100 + 30,700 = 88,800). Final verification passed: bilingual README contract, QA-linter policy, architecture, UX traceability (60 active rules), motion tokens, fmt, strict all-target workspace clippy, warning-denied workspace Rustdoc, core purity, workspace tests (758 core passed / 1 ignored; 669 GNOME passed / 138 ignored; 55 platform passed), diff checks, and audit with only accepted RUSTSEC-2024-0436. Assumption: same-host release medians are presented as comparison evidence, while cache/memory limits are the portable hard budgets. Manual remainder: replace the clearly labelled design-system previews in the public showcase with real running-app screenshots after the native GNOME visual pass; no fabricated screenshot was added. Residual GitHub metadata item: the connector verified the showcase is public but does not expose repository description/topic mutation, and shell GitHub access was network-blocked, so description/topics were not changed.

Lock: released by Codex in this worktree on 2026-07-18

## 2026-07-18 — Benchmark-driven album-window optimization

Branch: isolated `perf-album-integration` shadow branch
Base: `b5299a96` (album schema v12 integrated with the completed title-index performance stage)
Coordination: the repository-wide Album-grid T1–T7 lock remained owned by its active agent, so this stage touched neither live refs nor another worktree.
Stage: eliminate the measured album-sorted deep-window bottleneck and report its read, storage, and adjacent-query effects.

- PERF-ALBUM-1: complete (commit `b59af729`, extended the stable generated-query report with album-final-window timing and SQLite plan evidence).
- PERF-ALBUM-2: complete (commit `d45f8d59`, schema v14 adds the present-track album/track-number partial index with migration, ordering, data-preservation, and plan tests).
- PERF-ALBUM-3: complete (commit `8c0ea815`, comparison schema v2 reports album timing and before/after query plans fail-closed).
- PERF-ALBUM-4: complete (commit `b1c02c54`, adversarial measurement caught aggregate queries selecting the cache-hostile album index; a red planner regression test drove the atomic title-index recreation that restores the established aggregate scan locality).
- PERF-ALBUM-5: complete (commit `30f546a2`, comparison schema v3 now reports and validates filtered-count and library-stat deltas, closing the benchmark gap that exposed PERF-ALBUM-4 only during manual inspection).

Stage review: complete in the isolated shadow branch. A sequential release pair compared committed baseline `b59af729` with candidate `30f546a2` using 10,000 and 100,000 generated metadata rows. At 100,000 tracks the final album-sorted window changed from 177,583 to 1,850 us (-98.96%); SQLite changed from the title index plus a temporary ORDER BY B-tree to `SCAN tracks USING INDEX idx_tracks_present_album_order`. The index adds 2,392,064 bytes (+8.97%). The existing title-index path remained unchanged and its first/middle/final windows changed by -1.04%/+0.61%/-0.63%; database open changed by +5 us (+1.05%), playback-ID projection by +11 us (+3.06%), filtered count by +918 us (+6.13%), and library stats by +311 us (+4.97%). At 10,000 tracks the album-final window changed from 5,529 to 393 us (-92.89%) for 241,664 additional bytes (+8.98%). TrackListModel retained exactly eight SQL windows / 1,600 rows at both sizes; its 100,000-row synthetic traversal changed from 14,686 to 12,995 us, but the host was under elevated concurrent load, so that elapsed value is recorded as context rather than an improvement claim.

Final gates passed: fmt, strict workspace clippy, workspace tests (core 762 passed / 1 ignored; GNOME 669 passed / 138 ignored; platform 55 passed), architecture, UX traceability (60 active rules), QA policy including all performance comparison contracts, core purity, and audit with only the accepted RUSTSEC-2024-0436 warning. Assumptions: the index intentionally matches the established flat track-table album order (`album COLLATE NOCASE, track_no`) and does not change descending-sort semantics or adopt Album-grid disc ordering; same-host microsecond deltas are comparison evidence, while the deterministic plan and cache-bound assertions are the hard regression contracts. Residual costs/risks: visible-track writes maintain one additional index and still need a committed write-throughput benchmark; the small aggregate-query increases and storage cost are retained rather than hidden; installed-app startup, realized GTK widget/provider counts, queue RSS, and rendered scroll feel remain the previously documented host-deferred runtime axes. No real database, music file, live desktop, user profile, or remote was touched.

Git handoff: the shadow tip is preserved as `/tmp/reprise-performance-album-optimization.bundle`; the primary `feat/performance-optimizations` branch remains at `a41c53f4` until the active Album-grid lock is released and the current live `main` can be integrated without disturbing concurrent work.

## 2026-07-18 — Committed write-throughput benchmark

Branch: isolated `perf-write-candidate` shadow branch
Baseline: `86d6e8e6` (the generated write contract on the index-free album-query baseline)
Candidate: `2e67020c` (album-index history plus the fail-closed write comparison)
Coordination: live Album/showroom and network-opt-in locks remained owned by other active agents; this stage touched no live worktree or shared ref.
Stage: quantify the read/storage/write trade-off of the present-track album-order index and reject a lower-cost alternative if it compromises deep scrolling.

- PERF-WRITE-1: complete (commit `86d6e8e6`, schema-v4 query JSON measures committed insert, metadata-update, hide, and restore batches on disposable database copies; every iteration starts from identical generated state and leaves the read profile unchanged).
- PERF-WRITE-2: complete (commit `2e67020c`, comparison schema v4 reports all four write deltas and rejects unequal write-batch sizes instead of comparing unlike workloads).

Stage review: complete in the isolated shadow branch. A clean release pair compared baseline `86d6e8e6` with candidate `2e67020c` at 10,000 and 100,000 generated tracks. Because other agents still produced host I/O, the accepted write figures come from four additional counterbalanced 100,000-track AB/BA rounds using separately hashed release binaries; each report itself contains seven committed samples. For a 10,000-row batch, medians changed as follows: insert 35,088.5 to 42,399.5 us (+7,311 us / +20.84%), index-relevant metadata update 23,496.5 to 34,851 us (+11,354.5 us / +48.32%), hide 5,858.5 to 10,124.5 us (+4,266 us / +72.82%), and restore 6,228 to 10,490 us (+4,262 us / +68.43%). The dominant read result remained stable: the final album-sorted window changed from 134,406 to 1,418 us (-98.94%), while database size remains +2,392,064 bytes (+8.97%). Existing title-window, filtered-count, library-stat, and playback-ID medians varied between -0.49% and -5.70% in the counterbalanced control; their SQL plan stayed on `idx_tracks_present_title_nocase`, so these small negative deltas are treated as host/cache noise rather than improvement claims.

Adversarial alternative review: an album-only partial index reduced the 100,000-row database to 27,238,400 bytes (only +573,440 bytes / about +2.15% over baseline), but SQLite required `USE TEMP B-TREE FOR LAST TERM OF ORDER BY`. One hundred focused final-window ID queries took 2,196,113 us instead of 143,183 us with the composite index, about 15.3 times slower. The narrower index was therefore rejected: it saves writes/storage at the exact cost of the deep-scroll latency this stage exists to remove. The committed composite index remains the smallest index that satisfies the established `album COLLATE NOCASE, track_no` order directly.

Verification: both implementation commits passed fmt, strict workspace clippy, workspace tests (core 762 passed / 1 ignored; GNOME 669 passed / 138 ignored; platform 55 passed), architecture, UX traceability (60 active rules), QA policy including the schema-v4 comparison contract, core purity, and audit with only accepted RUSTSEC-2024-0436. All substantially edited code files remain below 800 lines. Assumptions: write timings include transaction begin, index maintenance, and commit for a 10,000-row batch; database copy/open/setup are deliberately outside the timer. The synthetic metadata update changes title, album, and track number, representing the worst index-relevant batch rather than a no-op field update. Residual risk: elapsed values are same-host evidence and real scanner/tag-writer throughput also includes filesystem/tag parsing; installed GTK/startup/CUA axes remain host-deferred. No real database, music file, live desktop, user profile, remote, or live branch was touched.

Git handoff: baseline and candidate refs are preserved in `/tmp/reprise-performance-write-benchmark.bundle`; live `feat/performance-optimizations` and `main` remain unchanged until the other active lanes finish.
## 2026-07-18 — Accessibility & Tastatur

Task KBD-1: complete (commit b258476, base 402be7f, fail-closed CUA keyboard/focus primitives plus complete GUI-surface manifest; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted; shellcheck unavailable on host).
Task KBD-2: complete (commit f74fe9a, base b258476, active-content focus routing for every shell view plus scoped Space, Escape, F10, Ctrl+W and Ctrl+Q behavior; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-3: complete (commit 0ffada9, base f74fe9a, native keyboard activation for artist top tracks, keyboard artist navigation in album menus, retained album selection, and collection-only tab stops for album/rating inline controls; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-4: complete (commit 7732921, base 0ffada9, native scan/relink/device actions, focus-visible issue pills, keyboard issue/device context menus, and named progress controls; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-5: complete (commit 54a73eb, base 7732921, native player metadata actions, range-semantic keyboard waveform seeking, roving lyrics rows, and explicit Now Playing tab semantics; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-6: complete (commit f3ab205, base 54a73eb, shared weak-reference focus guard with deterministic initial/restore focus, transient-local Ctrl+W, nested discard preservation, and lifecycle wiring across dialogs and custom popovers; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-7: complete (commit 03d796c, base f3ab205, Alt+Arrow and context-menu reorder commands delegate to the existing playlist/queue drop handlers, add commands share the sidebar drop paths, and Help/KeyShortcuts document the alternatives; workspace tests, fmt, clippy, audit, architecture, traceability and gettext format checks green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-8: complete (commit 4e4b61c, base 03d796c, semantic contracts and widget-walk primitives gate custom focus controls, input-parity markers gate every pointer/drag surface, focus-visible CSS is mandatory, and keyboard Stop/device-playlist actions share the pointer runtime paths; workspace tests, fmt, clippy, audit, architecture and traceability green; display/CUA host check deferred because private D-Bus socket creation is blocked with Operation not permitted).
Task KBD-9: complete (base 4e4b61c, the complete keyboard-only surface manifest now runs against an isolated populated library, focus/state/effect evidence is retained after every action, history return cannot be swallowed by stale sidebar-source deduplication, and ACC-1/2/3/4/5/6/8/9 are active while manual visible-focus rule ACC-7 remains planned. Verification: fmt, clippy, workspace tests 757 passed/1 ignored + 686 passed/153 ignored + 55 passed, audit with only allowed RUSTSEC-2024-0436, architecture, gettext, input parity, accessibility semantics, QA-linter contracts, UX traceability with 68 active rules, and file-size/diff checks green. Host evidence: all 43 rule-named display tests passed (`/tmp/reprise-display-host-run.log`, status 0); full populated keyboard sweep retained 714 ACC artifacts in `/tmp/reprise-cua-e2e/run-20260718T195014Z-1328476`; fresh install, TAG-1 and TAG-3 isolated CUA scenarios each exited 0. The display runner now requires an in-session success marker so a D-Bus bootstrap failure cannot report a false green. Manual GNOME/Wayland, High Contrast, Large Text, Orca, on-screen-keyboard and reduced-motion checks remain exclusively under ACC-7.)

Main integration: complete against `b0965905`; preserved the current SearchBar reveal/collapse contract while returning the second Escape to the active content view, retained the performance, album-grid, and accessibility QA gates together, moved the accessibility rules to section T after main's Q-S sections, and kept the album reveal surfaces native keyboard buttons with the current GRID-5 labeling and focus restoration. Integration verification: fmt, strict workspace clippy, warning-denied rustdoc, workspace tests 780 Core + 717 GNOME + 55 Platform green (171 GNOME display/performance tests ignored), architecture, core purity, gettext, input parity, accessibility semantics, QA-linter contracts, UX traceability with 94 active rules, motion tokens, CUA helper contract, diff/file-size checks, and audit green except accepted RUSTSEC-2024-0436. The prior feature-host display/CUA evidence remains recorded above; the final integration sandbox retry was blocked before its first display test because `dbus-daemon` cannot bind a private socket (`Operation not permitted`). ACC-7 remains the only planned manual rule.

Album-Grid current-main integration: complete (merge commit `84ccffb6`, album integration parent `48ebb760`, current-main parent `892437ab`). Der Merge bewahrt die bereits integrierten Performance-Migrationen v13/v14 unverändert und hängt Disc-Persistenz kompatibel als v15 an, sodass auch eine bereits von `main` erzeugte v14-Datenbank sicher nachgerüstet wird. Verifikation: fmt, locked clippy, Rustdoc, Workspace-Tests (780 Core + 700 GNOME + 55 Plattform; 155 GNOME display-gated ignoriert), 86 aktive UX-Regeln, Motion-Lint, Architektur, QA-Linter einschließlich Performance-Verträge, Core-Purity und Audit mit ausschließlich der erlaubten Ausnahme. Das echte `main` bleibt mangels Schreibzugriff auf das gemeinsame `.git` bei `892437ab`; der vollständige geprüfte Stand wird als Bundle gesichert.

## 2026-07-19 — My Stats editorial rebuild (Frame 25a)

Branch: `feat/mystats-optimization` (base `b0965905`). T1: complete (`d7f8a982`, rulebook and release QA). T2: complete (`23e5f303`, schema v16 join index). T3: complete (`25ee61f3`, deterministic Unicode/MBID grouping). T5: complete in its parallel-safe slot (`d8988263`, persistent layout and validated smart-playlist creation). T4: complete (`c297781a`, local timezone-aware period/snapshot model and listen-event-only aggregates). T6: complete (`b31d726c`, Cairo listening-time ribbon). T7: complete (`d5912465`, spotlight, genre, clock, and highlight widgets). T8: complete (`eb7c5697`, 1120 px editorial composer, empty state, fixed customization, top-track sorting, and view-local breakpoint). T9: complete (`f6bac394`, grouped spotlight playback, artist navigation, Smart Mix routing, tag-editor forwarding, and STATS-8 filter exclusion). T10: complete in this ledger commit.

Final gates: fmt, strict all-target workspace clippy, workspace tests (780 Core passed; 710 GNOME passed / 163 display-gated ignored; 55 Linux platform passed), UX traceability (96 active rules), core purity, file-size checks, and audit with only accepted `RUSTSEC-2024-0436`. The display gate was explicitly deferred by the supervising instruction and was not run; the new ignored display tests cover spotlight content, grouping hints, genre presentation/non-interactivity, highlights, customization persistence/order, empty state, and narrow breakpoint reflow. No real database, music files, live desktop, or user profile were touched.

Assumption: the current smart-playlist rule engine combines rules with `AND` and has no `OR`/`IN`, so the singular Smart Mix CTA creates a usable mix from the dominant top genre instead of emitting mutually exclusive rules for all five genres. Coordination: this checkout contains no repository `STATUS.md`/lock board, so there was no separate repository lock to claim or release.

## 2026-07-19 — Library Doctor / Tag Cleanup

Branch: feat/tag-rework
Lock: claimed by Codex in this worktree on 2026-07-19
Stage: implement the Library Doctor local/remote scan, mandatory per-field review, journaled apply/revert, and GTK navigation from the approved Frames 26a/26b/27 plan.

Task DOC-1: complete (commit 2ea322e, base 5182582, defined the split Library Doctor UX contracts DOC-1a through DOC-6c as planned rules in section W).
Task DOC-2: complete (commit 3e1578c, base 2ea322e, added read-only local scans with frozen scopes, conservative exact-spelling proposals, durable full-tag snapshots, stale detection, and restart-safe unresolved groups).
