# Library Doctor — Bestandsaufnahme (Basis `origin/dev` = d0054ab5b5)

Inventur für den Redesign nach `library-doctor-redesign-brief.md`. Alle Pfade
relativ zum Worktree. Zeilennummern vom 2026-08-05 — vor dem Anfassen prüfen.

## 1. GTK-Oberfläche — `crates/reprise-gnome/src/ui/library_doctor/`

| Datei | Zeilen | Zweck |
|---|---:|---|
| `mod.rs` | 765 | Koordinator, Navigation, Scan/Apply/Revert-Jobs |
| `summary_page.rs` | 554 | Root-Seite: 9 ActionRows + Buttons |
| `review_page.rs` | 537 | ListView + unresolved Groups + Apply-Footer |
| `review_model.rs` | 303 | `ReviewRowModel`, Confidence, `layout_for_width()` |
| `review_row.rs` | 272 | Zeilen-Factory, `ValueWidgets`, `BreakpointBin` |
| `progress_card.rs` | 213 | Sidebar-Karte (Spinner/Progress/Cancel) |
| `jobs.rs` | 75 | `run_scan` / `run_apply` / `run_revert` |
| `job_page.rs` | 60 | Vollbild-`StatusPage` für Job-Zustand |
| `tests.rs` | 114 | Scope-, Progress-Tests |
| `review_row_contract_tests.rs` | 7 | Quelltext-Assertion für DOC-3b |

**Navigation:** drei `NavigationPage`-Tags — `library-doctor` (Summary,
`summary_page.rs:256`), `library-doctor-review` (`review_page.rs:353`),
`library-doctor-job` (`job_page.rs:22`). `mod.rs:224` sucht die Seite über
`navigation.find_page("library-doctor")`. `open_review()` `mod.rs:383–416`,
`open_job_page()` `mod.rs:441–447`.

**Summary heute:** 4 feste Zeilen (`DOCTOR_SAFE_FIXES`, `DOCTOR_SUGGESTIONS`,
`DOCTOR_UNRESOLVED_GROUPS`, `DOCTOR_TRACKS_CHECKED`) + versteckte
`DOCTOR_CLEANUP_STATUS` + 5 ProblemClass-Zeilen (`summary_page.rs:184–193`),
alle als `adw::ActionRow` über `summary_row()` `summary_page.rs:432`.
Sichtbarkeit der Remote-Zeilen hängt an `remote.is_active()` (`:412`).

**Review heute:** `gtk4::ListView` + `SingleSelection` + `gio::ListStore`
(`review_page.rs:222–245`), Factory `review_row::factory()`, Stack
`rows`/`empty`, Footer mit `doctor_apply_summary()`-Label + Apply-Button
(`:264–280`). Unresolved Groups als `adw::ComboRow` je Gruppe (`:107–162`).
Presets `All safe` / `None` im Header (`:304–324`).

**Zeilenaufbau (`review_row.rs:113–186`):**
`Box[ CheckButton, BreakpointBin[ Box( track_field, current, proposed,
source_box[warning, source] ) ], edit-Button ]`. Jede Zelle ist ein
`ValueWidgets`-Paar aus Caption-Label + Value-Label (`:188–207`) — genau das,
was der Brief pro Zeile entfernt haben will. Breakpoint 640 px in
`review_model.rs:10`; `layout_for_width()` `:20–26` ist `#[cfg(test)]`.

**Confidence (`review_model.rs:37–77`):** Local → Accent, MB/AcoustID ≥85
neutral, 50–84 Warning, <50 Error + Warnsymbol. Bleibt unverändert.

**Section-Vorbilder im Repo:** kein `SectionModel` im GTK-Crate. Queue nutzt
`ColumnView::set_sections()` (`queue_sections.rs`), Podcasts nutzt
`SortListModel` (`podcasts_model.rs`). Achtung: das `SectionModel`-Interface
ist in `reprise-gnome` unter `cfg(test)` abgeschaltet — Section-Header sind
im Crate **nicht testbar**, Beweis nur über ein `examples/`-Programm
(Vorbild: `examples/queue_section_shift_repro.rs`). Teil-Deltas brauchen ein
zusätzliches `sections_changed`.

## 2. Einstiegspunkte

- **⋮-Menü:** `ui/primary_menu.rs` — `ACTION_LIBRARY_DOCTOR` (`:18`),
  Callback (`:32`), Menüaufbau `update_library_section()` (`:50–66`, drei
  Einträge: Rescan/Cancel, Library Doctor, Sync Device), GAction-Installation
  (`:180–185`) als `win.library-doctor`.
- **Preferences:** `preferences/preference_library_doctor.rs`, `plugin_row()`
  `:255–362` baut eine `adw::ExpanderRow` mit Scope-Combo, Remote-Switch,
  Hinweiszeile, Run- und Revert-Button. Registriert in
  `preference_plugins.rs:105` (`LOCAL_PLUGIN_ID = "library_doctor"`) und
  `:164`. Kontextfelder `doctor_controls` / `library_doctor_job_running` in
  `preferences.rs:70,103`; `set_job_running()` (`preference_library_doctor.rs:62`).
- **Sidebar-Fortschrittskarte:** `sidebar_activity_slot.rs:85–96`
  (`set_doctor_card()`), öffentliche API `Sidebar::append_doctor_card()`
  (`:181–183`), Reihenfolge `scan_card → doctor_card → relink_card`.
  Gemeinsames CSS `.scan-card` aus `ui/scan/scan_card_css.rs`.

## 3. Sidebar-Umfeld

- **ISSUES-Sektion:** `sidebar/sidebar_issues_section.rs:48–86` baut Heading +
  Listbox. Zeilen kommen aus `sidebar_rebuild.rs:348–356` über
  `add_issue_row()`, gefüttert von `queries::count_missing()` (`:59–62`) und
  `queries::count_new_missing()` (`:67–71`). Badge-Logik in
  `sidebar_presentation.rs:94–98` (`issue_row_presentation()`), Identität über
  `ViewSource::Missing` + `NavIcon::Missing`. **Das ist das Muster für den
  neuen Doctor-Eintrag.**
- **PLAYLISTS-Sektion:** Header über `sidebar_presentation::append_header()`
  (`sidebar_rebuild.rs:272–274`) — heute ein reines Label mit
  `caption-heading`, **trägt keinen Button**. Die beiden Aktionszeilen entstehen
  in `sidebar_rebuild.rs:285–287` → `append_playlist_action_row()`
  (`sidebar_presentation.rs:156–174`). Anlegen läuft über einen Dialog:
  `sidebar_playlist_creation.rs:20–34` (`show_new_playlist_dialog()` →
  `dialogs::prompt_name()`), dann `create_playlist_and_stay()` (`:38–57`).
  **Ein Inline-Rename in der Sidebar existiert nicht** — das ist Neubau.
- **Toasts:** `ui/toasts.rs:16–20` (`show()`, 4 s). Toast **mit Action-Button**
  wird heute nicht über diesen Helfer gebaut, sondern direkt an einzelnen
  Stellen (u. a. Track-Kontextmenü, Löschen) — für das `{n} tags fixed`-Toast
  mit Undo also ein eigener Pfad.

## 4. Core — `crates/reprise-core/src/library/library_doctor/`

| Datei | Zeilen | Zweck |
|---|---:|---|
| `types.rs` | 319 | `DoctorField` (7), `DoctorProposal`, `DoctorUnresolvedGroup`, `ProblemClass`, `ProposalSource` |
| `scan.rs` | 281 | Scanner, erzeugt Proposals + Gruppen |
| `local_rules.rs` | 192 | Casing, Album-Artist-Default, Genre, Year |
| `scope.rs` | 159 | `WholeLibrary` / `CurrentView` / `Selection` → `FrozenScope` |
| `review.rs` | 603 | `DoctorReviewSession`, Zeilenbau, Sortierung, `freeze_plan()` |
| `presentation.rs` | 311 | Tier-Einteilung + Summary-Projektion |
| `store.rs` | 423 | Persistenz |
| `write.rs` | 764 | `prepare_job` / `run_job` / `apply_review_plan` / `revert_last_cleanup` |
| `write_recovery.rs` | 173 | Wiederaufnahme unterbrochener Jobs |
| `preferences.rs` | 95 | `remote_enabled`, `skip_stale_tracks` |
| `remote/*` | ~2388 | Orchestrator, MusicBrainz, AcoustID, Cache, Arbitrierung |

**`DoctorField`:** `Title, Artist, Album, AlbumArtist, Year, Genre, RecordingMbid`
(`types.rs:72–80`).

**Tier-Entscheidung (`presentation.rs:129–137`):**
`safe = source == Local && preselected && !stale`. Alles andere ist Review.
Unresolved sind separate Gruppen. **Es gibt kein persistiertes Tier-Feld** —
die Einteilung entsteht bei jeder Projektion neu. Das erleichtert die
Migrationsfrage aus dem Brief erheblich.

**Zeilenbau:** `DoctorReviewSession::from_scan()` (`review.rs:160`), Filter
`LocalSafeOnly` (`:195–197`), Sortierschlüssel `RowSortKey`
(category, scope_position, field_position, sequence) `:205–215`.

**Schreibpfad:** `apply_review_plan()` (`write.rs:701`) → `prepare_job()`
(`:297–308`, `INSERT INTO tag_write_jobs`) → `run_job()` (`:554`) →
`commit_guarded_tag_changes()`. Job-Arten laut Schema-Check
(`db_tag_write_jobs.rs:4`): `tag_editor`, `doctor_apply`, `doctor_revert`.
`doctor_apply` verlangt `scan_id IS NOT NULL`, `doctor_revert` verlangt
`source_job_id` (`:14–16`).

**Revert (wichtig):** `revert_last_cleanup()` (`write.rs:742`) nimmt **genau
einen** Job zurück — den jüngsten mit `outcome='applied'` über `last_cleanup()`
(`:717`), Werte werden über `revert_inputs()` (`:667–697`) invertiert.
**Zwei Jobs desselben `scan_id` als Einheit zurückzunehmen kann der Code heute
nicht.** Genau das verlangt der Brief (§2, §5, §7).

**Lock:** über `tag_write_jobs.state` (`prepared|running|completed|cancelled|
interrupted`, `db_tag_write_jobs.rs:9`), beansprucht in `claim_file()`
(`write.rs:341–363`) unter Transaktion. Invariante: `state IN (prepared,
running) ⟺ finished_at IS NULL` (`:11`).

**Schema:** `SUPPORTED_SCHEMA_VERSION = 56` (`db.rs:26`). Doctor-Tabellen aus
v19 (`db_library_doctor.rs:85`), Tag-Write aus v20, Remote-Spalten v21,
Remote-Cache v22; registriert in `db.rs:701–704`, jede Migration idempotent.
`library_doctor_proposals` speichert u. a. `field, current_value,
proposed_value, source, confidence, preselected, problem_class, evidence_json`.

## 5. Strings, Regeln, Gates

- `ui/strings_library_doctor.rs`, 289 Zeilen: ~70 Konstanten + 22 Funktionen.
  **Ungenutzt schon heute:** `doctor_evidence_value()`, `doctor_duration_ms()`,
  `doctor_duration_delta_ms()`, `doctor_write_cancelled()`,
  `doctor_group_count()`. `doctor_apply_summary()` wurde vom Inventur-Agenten
  als unbenutzt gemeldet, obwohl `review_page.rs:52–55` es setzt — vor dem
  Löschen selbst nachsehen.
- `docs/ux-rules.md` §Y, Zeilen 3110–3385: 20 Regeln DOC-1a … DOC-7b, davon
  DOC-1d und DOC-6a bereits `[replaced by …]`, DOC-6c `[planned] [manual]`.
- `scripts/check-ux-traceability.sh`: jede `[active]`-Regel braucht ≥1 Test,
  dessen Name die Id trägt (`doc_3b_*` in Rust, `doc-3b-*` in cua-e2e).
  Tests dürfen keine unbekannten oder ersetzten Ids nennen. `#[ignore]` nur
  mit den zwei erlaubten Begründungstexten.
- `scripts/check-frontend-thinness.sh`: Budgets sind **Decke UND Boden** —
  `rusqlite=112, filesystem=19, threads=15, workers=7`, `view_floor=1352`.
  Wer Frontend-Code löscht, **muss** die Zahlen im selben Commit senken.
  Nulltoleranz für `gstreamer`, `zbus`, `.conn()`.
- `scripts/check-accessibility-semantics.sh`: vor jedem `set_focusable(true)`
  muss ein `// a11y-semantics: role=… name=… state=… action=…`-Marker stehen.
- `scripts/ci-quality.sh` → `check-merge-readiness.sh` (18 Stufen inkl. fmt,
  clippy `-D warnings`, `cargo doc`, `cargo test --workspace`, drei
  Display-Test-Läufe, `cargo audit`).
- `doctor_review_row_description()` (`strings_library_doctor.rs:103–120`)
  speist `ReviewRowModel::accessible_description()` (`review_model.rs:97–111`)
  und den Zeilen-Tooltip (`review_row.rs:251`).
- i18n: `po/POTFILES.in:5` listet die Strings-Datei; 92 Doctor-Einträge je
  `.po`, acht Sprachen.

## 6. MCP — `crates/reprise-mcp`

- **Router-Muster:** `#[tool_router(router = source_tool_router, vis =
  "pub(crate)")] impl RepriseServer` (`source_tools.rs:13`, `device_tools.rs:9`);
  zusammengesetzt in `server.rs:134–138` (`Self::tool_router() +
  Self::source_tool_router() + …`).
- **Tool-Körper:** `db_path` klonen → `spawn_blocking` → `join_error` →
  `structured_ok(&result, summary)` bzw. `into_tool_outcome(err)`
  (`server.rs:164–188` als Muster, `error.rs:26–65`).
- **Capabilities** (`capability.rs:14–49`): `library:read` (an),
  `playlist:create`, `playlist:manage`, `ai:create`, `sources:manage`,
  `device:sync` (aus), `playback:control` (an). Prüfung ist
  `effective(startup_snapshot, live_value)` (`:96–98`) — ein neuer Grant
  braucht Neustart, ein Entzug wirkt sofort. Snapshot in `startup.rs:29–60`.
  Ablehnungstext in `error.rs:54–58`.
- **Resources:** sieben `reprise://…`-Uris (`server.rs:38–51`), gelistet in
  `list_resources()` (`:633–679`), gelesen in `read_resource()` (`:686–759`)
  nach demselben spawn_blocking-Muster.
- **Prozessmodell:** eigener Prozess über stdio; jede Anfrage öffnet ihre
  eigene kurzlebige `Db` (`data.rs:85–87`, `Db::open_ready`). Playback läuft
  per D-Bus gegen die laufende GUI. Ein GUI/MCP-Schreib-Lock muss also über
  die Datenbank laufen (`tag_write_jobs.state`), nicht über Prozessspeicher.
- **Leak-Regel D19:** keine Pfade, Credentials, Lyrics, Seriennummern in
  Antworten; `tests/leak_matrix.rs` prüft das. Ein Doctor-Tool darf also
  **keine Dateipfade** zurückgeben, obwohl der Doctor intern mit Pfaden
  arbeitet.
- **Tests:** `tests/common/` mit `McpClient` (spawn/handshake/call_tool),
  Fixture-Vergleiche gegen `tests/fixtures/*.json`.
- 21 `music_*`-Tools heute.
