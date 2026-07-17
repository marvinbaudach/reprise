# Missing files / Import errors — Taskplan (Pakete 3–6)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development
> (empfohlen) oder superpowers:executing-plans. Checkboxen (`- [ ]`) fürs Tracking.
> Normativer Kontext: [2026-07-17-missing-import-errors-beschluesse.md](2026-07-17-missing-import-errors-beschluesse.md)
> — bei Widerspruch gewinnt `docs/ux-rules.md`, danach das Beschlussdokument, zuletzt dieser Plan.

**Ziel:** „Missing files" und „Import errors" werden selbstheilende Zustandslisten (Design
18a). Der **Kern ist fertig** (Pakete 1–2, in `main`); offen ist die gesamte UI plus Locate
und Mount-Events.

**Branch:** `feat/missing-import-errors`, Worktree `/home/marvin/Projects/reprise-issues`.
**Basis:** `main` gemerged (`061a5d5`). Testbaseline: **1236 passed, 0 failed, 87 ignored.**

**Design-Referenz 18a:** claude_design-MCP-Projekt
`https://claude.ai/design/p/17d84d41-87d7-44cf-8de8-3f1e4e350f09?file=Reprise+Redesign.dc.html`
(Share-Link ist kanonisch; PDFs unter `docs/design` ignorieren). Werte: Cards Radius 12 /
Fläche weiß 3.5 % / Header-Zeile weiß 3 % (Titel bold 13 + Meta weiß 50 %); Rows 42 px Grid
(Cover 30 · Titel · Artist · Album · rechts); „since Jul 12" 11.5 / weiß 40 %; Hover-Pills
Crossfade 100 ms; Rosa `#f38ba8` auf 10 %-Fläche; Teal-Infokarte Akzent 7 % Fläche / 18 %
Border.

---

## Was schon steht (NICHT neu bauen)

Der Kern ist implementiert, getestet und reviewed. **Vor jedem Task: `git log --oneline` und
das Ledger `.superpowers/sdd/progress.md` lesen.** Verfügbare API:

```rust
// models.rs (pub, crate-übergreifend erreichbar)
pub enum MissingReason { Unmounted, Deleted, Unknown }          // as_str()/parse() (parse -> Unknown als Fallback)
pub enum ImportErrorKind { UnreadableTags, PermissionDenied, UnsupportedFormat, Io, Unknown }
pub struct Track { pub missing_since: Option<i64>, pub missing_reason: Option<MissingReason>,
                   pub untagged: bool, /* ... */ }
impl Track { pub fn is_missing(&self) -> bool }

// queries/clauses.rs (pub(crate)) — NIE handschriftlich kopieren
const PRESENT: &str = "missing_since IS NULL AND removed_at IS NULL";
const MISSING: &str = "missing_since IS NOT NULL AND removed_at IS NULL";

// queries/issues.rs
pub enum MissingGroupKind { Unavailable { mount_point: Option<String> }, Deleted }
pub struct MissingGroup { pub kind: MissingGroupKind, pub track_count: u32 }
pub fn query_missing_groups(conn) -> Result<Vec<MissingGroup>, rusqlite::Error>;   // Reihenfolge: per-mount unavailable -> unknown -> deleted
pub fn query_missing_rows(conn, kind: &MissingGroupKind, offset: u32, limit: u32) -> Result<Vec<Track>, _>;
pub fn count_missing(conn) -> Result<u32, _>;                    // ISSUES-Sichtbarkeit (Gesamt)
pub fn count_new_missing(conn, last_viewed: i64) -> Result<u32, _>;   // Badge
pub fn auto_clean_eligible(conn, now: i64) -> Result<Vec<i64>, _>;
pub fn run_auto_clean(conn: &mut Connection, now: i64) -> Result<Vec<i64>, _>;   // gibt ids für Queue-Purge zurück

// queries/maintenance.rs
pub fn tombstone_tracks(conn, ids: &[i64], now: i64) -> Result<usize, _>;
pub fn undo_tombstone(conn, ids: &[i64]) -> Result<usize, _>;
pub fn purge_tombstones(conn: &mut Connection) -> Result<Vec<i64>, _>;   // gibt ids für Queue-Purge zurück
pub fn remove_tracks(conn: &mut Connection, ids: &[i64]) -> Result<Vec<i64>, _>;   // hart, + Playlist-Kompaktierung

// queries/import_errors.rs
pub struct ImportErrorEntry { pub path: String, pub kind: ImportErrorKind, pub detail: String,
    pub first_seen: i64, pub last_seen: i64, pub seen_count: i64, pub is_hint: bool }
pub fn query_import_errors_grouped(conn) -> Result<Vec<(ImportErrorKind, Vec<ImportErrorEntry>)>, _>;
pub fn query_dismissed_import_errors(conn) -> Result<Vec<ImportErrorEntry>, _>;
pub fn count_dismissed_import_errors(conn) -> Result<u32, _>;
pub fn count_import_errors_active(conn) -> Result<u32, _>;       // non-dismissed INKL. Hinweise (Sichtbarkeit)
pub fn count_new_import_errors(conn, last_viewed: i64) -> Result<u32, _>;   // Badge, OHNE Hinweise
pub fn dismiss_import_error(conn, path: &str, mtime: i64, size: i64) -> Result<(), _>;
pub fn dismiss_all_import_errors(conn, now_stat: &dyn Fn(&str) -> Option<(i64,i64)>) -> Result<u32, _>;
pub fn restore_import_error(conn, path: &str) -> Result<(), _>;  // nullt nur dismissed_*; Retry ist Sache der UI

// library/settings.rs
pub enum AutoCleanSetting { Off, Days(u32) }
// Keys: missing_auto_clean ("off"|"30"|"90"), auto_clean_armed_at, last_viewed_missing, last_viewed_import_errors
// getrennte typisierte Getter/Setter vorhanden — vor Gebrauch in settings.rs nachlesen

// library/scanner.rs
pub enum ScanOutcome { Completed(ScanReport), RootUnavailable { root: PathBuf } }
pub struct ScanReport { pub added, updated, skipped_unchanged, errors, moved, vanished, healed: u32 }
pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanOutcome, ScanError>;
pub fn scan_folder_with_progress(conn, root, on_progress) -> Result<ScanOutcome, ScanError>;
// scanner ist ein atomarer Reconcile inkl. Root-Guard. mark_vanished_under_root existiert NICHT mehr.
```

Schema v11: `tracks(missing_since, missing_reason, mount_point, removed_at, untagged, …)`,
`import_errors(path PK, reason_kind, reason_detail, first_seen, last_seen, seen_count,
dismissed_mtime, dismissed_size)`.

## Vier Korrekturen gegenüber dem ursprünglichen Plan

1. **PLAY-5a ist schon gebaut** (Queue-Purge bei deleted, spielender Track wird nie gestoppt):
   `Queue::remove_ids` (`reprise-core/src/queue.rs:407`), `PlayerController::purge_queue_ids`
   (`ui/playback/queue_transport.rs:462`, inkl. `successor_after_purge`), verdrahtet aus
   `ui/window/window_action_wiring.rs:105,365`, Test `play_5a_deleted_tracks_leave_queue_silently`
   (`reprise-core/src/queue_ux_rules_tests.rs:43`). **Nicht neu bauen** — nur PLAY-5b (unmounted) fehlt.
2. **`remove_missing_track(s)` / `remove_all_missing_tracks` sind NICHT retired.** Sie sind die
   lebende Hard-Delete-API mit echten Callern (`ui/track_list/track_actions.rs:243`,
   `ui/sidebar/sidebar_issue_cleanup.rs:149`). Nur retiren, was nachweislich (grep!) seinen
   letzten Caller verliert.
3. **Badge-Kern hat null GUI-Caller.** Die Sidebar zeigt weiter Rohsummen
   (`sidebar_rebuild.rs:30-35`, Sichtbarkeit `:118`).
4. **Tombstone-Kern hat null GUI-Caller.** `ui/toasts.rs::show(overlay, text)` kann nur
   Plain-Text / fix 4 s / **kein Button**; das Modul-Doc sagt: Sites mit Button/Timeout bauen
   ihren `adw::Toast` lokal. Es gibt **keinen Undo-Toast-Präzedenzfall im Projekt** — der 10-s-
   Undo-Toast ist Neubau. Einmal bauen, wiederverwenden.

## Globale Constraints

- **Gates vor JEDEM Commit** (aus dem Repo-Root):
  `cargo fmt --check` · `cargo clippy --all-targets --workspace -- -D warnings` ·
  `cargo test --workspace` (nie bares `cargo test`) · `cargo audit` (akzeptiert **nur**
  RUSTSEC-2024-0436; eine NEUE Advisory = STOP) · `bash scripts/check-ux-traceability.sh` ·
  `bash scripts/check-architecture.sh`.
- Nach Kern-Änderungen: `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'`
  MUSS leer sein.
- Code-Dateien **< 800 Zeilen**; kohäsives Sibling-Modul extrahieren statt Doku kürzen.
  Markdown ist ausgenommen.
- **RefCell-Disziplin:** nie ein `borrow()` über einen GTK-/Callback-Aufruf halten (Skill
  `building-gtk4-rust-apps` — jede Regel dort ist ein real gefangener Bug).
- Sichtbare Copy via `strings.rs`-Konstanten; `strings.rs` ist **append-only**
  (Kollisionsvermeidung mit parallelen Branches `feat/tag-editor-rework`,
  `feat/global-search-rework`).
- **Nie** die echte DB (`~/.local/share/reprise/reprise.db`) oder `/home/marvin/Music`
  anfassen. Headless nur mit dem vollen Isolations-Präfix aus AGENTS.md
  (`dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) …`) — vorm Ausführen den
  eigenen Befehl auf `XDG_DATA_HOME` greppen.
- TDD: erst roter Test, laufen lassen, **Fehlschlag sehen**, dann implementieren.
- Ein Commit pro Task, englische Message, **kein** Attribution-Footer, **nicht pushen**.
- Nach jedem Task eine Zeile in `.superpowers/sdd/progress.md`.

### UX-Regelwerk (verbindlich, neu seit dem main-Merge)

- `docs/ux-rules.md` ist die einzige UX-Wahrheitsquelle und schlägt Bestandscode.
- **Regel-Flip ist atomar:** `[geplant] → [aktiv]` im **selben Commit**, der das Verhalten
  liefert **und** einen grünen, nicht-ignorierten regelbenannten Test landet. Nie nachträglich.
  Halb umgesetzt → Regel in `a`/`b` splitten (Präzedenz: PLAY-3→3a/3b, PLAY-5→5a/5b), **nie**
  gegen eine halbe Regel testen.
- **Testname:** `fn <prefix>_<nummer>[<buchstabe>]_<beschreibung>()` direkt unter `#[test]`,
  Doc-Kommentar darüber beginnt `// UX <ID>: <Paraphrase>`. Vorlage wörtlich imitieren:
  `crates/reprise-core/src/queue_ux_rules_tests.rs:43`. Nur echte `#[test]`-fns zählen — eine
  gleichnamige Helper-fn oder ein Kommentar greent das Gate nicht.
- `#[ignore]` nur auf `[geplant]`-Regeln, Wortlaut zwingend `#[ignore = "UX <ID> [geplant] — …"]`.
- Getestet wird auf der **niedrigsten Ebene, die die Regel widerlegen kann** ([core] vor [gtk]
  vor [e2e]). Timing-Zahlen sind Design-Intent, keine Assertions: das *Was* automatisieren, das
  *Wie-schnell* manuell.
- Der Rules-Branch ist vollständig in `main` — dieser Branch flippt **direkt in
  `docs/ux-rules.md`** (Präzedenz `80c7f4e`). Achtung: `feat/tag-editor-rework` und
  `feat/global-search-rework` laufen parallel; nur die eigenen Regelzeilen anfassen.

### Regel-Ownership (welcher Task macht welche Regel `[aktiv]`)

| Regel | Ebene | Task | Vorbedingung |
|---|---|---|---|
| FB-7 | core | 3.2 | Undo-Toast + Purge-Verdrahtung real |
| FB-5 | gtk | 3.2 | „No missing files ✓"; „Library folder unavailable — Retry" erst in 5.5 → ggf. splitten |
| SET-4 | gtk | 3.2 | Arming-Dialog + benannte Kaskade in **beiden** Dialogen |
| FB-3 | core | 3.3 | gesammelte Fehler, ein End-Toast, Persistentes als Badge+ISSUES |
| FB-4 | core | 3.4 | Sidebar nutzt die Counts **und** schreibt `last_viewed_*` |
| PLAY-4b | gtk | 4.3 | |
| PLAY-4a | core | 4.4 | |
| PLAY-5b | core | 4.5 | |
| FB-2 | gtk | 5.3 | Fortschrittskarte für den Relink-Suchlauf |
| FB-6, P-6 | core | 5.4 | beide Regeltexte vorher lesen |

**Vor dem Flip den vollen Regeltext lesen** — die Tabelle ist ein Index, keine Anforderung.
Kann ein Task eine Regel nicht vollständig beweisen, **splitten statt flippen**.

## Paket-Ownership

| Paket | Reihenfolge | Ownership (exklusiv) |
|---|---|---|
| **3** Issue-Views | zuerst | `ui/issues/**` (neu), `ui/import_errors_view.rs`, `ui/sidebar/*`, `ui/strings*` (append), Stack-Arm in `ui/track_list/track_list_reload.rs` + `track_list_empty_state.rs` |
| **4** Playlist/Queue | nach 3 | `queries/playlist.rs`, `queue.rs`, `up_next.rs`, `ui/track_list/**` (außer den Stack-Arm aus 3), `ui/playback/**` |
| **5** Locate & Events | nach 3+4 | `library/relink.rs` (neu), `ui/mounts.rs` (neu), `ui/scan/*`, Locate-Anteile in `ui/issues/**`, `tag_edit`-Hook |
| **6** Abnahme | zuletzt | workspace-weit |

Pakete 3 und 4 sind theoretisch parallelisierbar (disjunkte Dateien bis auf den Stack-Arm),
praktisch aber **nicht im selben Worktree** — zwei Agents teilen sich sonst den Git-Index und
greifen sich gegenseitig die Staging-Area ab. Parallel nur mit getrennten Worktrees.

---

## Paket 3 — Issue-Views

### Task 3.1: `ui/issues/`-Bausatz

**Dateien:** neu `ui/issues/mod.rs`, `issue_card.rs`, `issue_row.rs`, `issue_collapse.rs`;
CSS über den bestehenden `ui/style/`-Mechanismus.

**Liefert (für 3.2/3.3; KEINE Import-Error-Spezifika — 17a-Device-View ist der dritte Nutzer):**
```rust
pub(in crate::ui) struct IssueCard;    // new(icon, title, meta, header_action: Option<gtk4::Widget>) -> body_listbox()
pub(in crate::ui) struct IssueRow;     // new(RowSpec{cover, primary, secondary, tertiary, right_idle, pills})
pub(in crate::ui) struct CollapsedList; // new(total, build_row: Rc<dyn Fn(u32) -> gtk4::Widget>)
```

- [ ] **Schritt 1: Rote Tests für die puren Anteile.** Collapse-Mathematik: sichtbare Range
  bei `total` = 1/2/3/120 nach 0/1/2 Expansionen (zeigt 2, dann je 50). Pill-Label-Bau.
- [ ] **Schritt 2:** `cargo test -p reprise-gnome issues` → Fehlschlag sehen.
- [ ] **Schritt 3: Implementieren.**
  - `IssueRow`-Hover: `gtk4::Stack` mit `crossfade` 100 ms zwischen Idle-Label und Pill-Box,
    getrieben von `EventControllerMotion`.
  - `CollapsedList` baut die eingeklappten Rows **lazy** (erst beim Expand), paginiert je 50
    („Show 50 more") — Beschluss 10: struktureller Deckel, nicht nur visuell.
  - Mehrfachauswahl: **zuerst prüfen**, ob `gtk4::ListBox` mit `SelectionMode::Multiple`
    Ctrl/Shift nativ liefert; nur nachbauen, was fehlt (vermutlich nur Ctrl+A und die
    Kontextmenü-Anbindung).
  - Factory-Lifecycle-Präzedenz: `ui/track_list/queue_sections.rs:175` (`connect_unbind`);
    idempotente CSS-Klassen nach `ui/track_list/track_list_columns.rs:42` (`toggle_class`).
- [ ] **Schritt 4:** Tests grün. **Schritt 5:** Gates + Commit
  `feat(ui): shared issue-card building blocks (cards, rows, lazy collapse)`

### Task 3.2: MissingFilesView + FB-7 + FB-5 + SET-4

**Dateien:** neu `ui/issues/missing_view.rs` (+ ggf. `missing_dialogs.rs`); Stack-Arm in
`ui/track_list/track_list_reload.rs` für `ViewSource::Missing`; `ui/strings.rs` (append).

**Nutzt:** `query_missing_groups`, `query_missing_rows`, `tombstone_tracks`, `undo_tombstone`,
`purge_tombstones`, `auto_clean_eligible`, `run_auto_clean`, `AutoCleanSetting`.

- [ ] **Inhalt (18a exakt):**
  - **Unavailable-Cards** pro Mount: „⏏ On unavailable drive · `<mount>` — not mounted · N
    tracks", rechts Hinweistext „return automatically when the drive is mounted"; Rows Opacity
    0.65, **keine Aktionen**. Unknown-Variante (`mount_point: None`): „unknown location" +
    **„will be verified on next scan"** — kein Rückkehr-Versprechen (Beschluss 3).
  - **Deleted-Card:** „🗑 Deleted from disk · folder still exists · N tracks", Header-Button
    „Remove all N from library" (rosa `#f38ba8` auf 10 %-Fläche, Pill) → Bestätigungsdialog,
    der die **Kaskade benennt**: „This removes N tracks from the library — their ratings and
    listening history go with them. Files are never touched." → `tombstone_tracks` → Toast
    „N removed · Undo" (10 s).
  - **Undo-Toast:** bespoke `adw::Toast` (kein Präzedenzfall im Projekt, `toasts::show` kann
    keine Buttons). Undo → `undo_tombstone`. Timeout → `purge_tombstones` + Queue-Purge der
    zurückgegebenen ids (`PlayerController::purge_queue_ids`, existiert). FB-1: Toasts **mit**
    Aktion sind unverdrängbar und laufen ihre vollen 10 s.
  - **Startup-Purge:** beim App-Start `purge_tombstones` (Beschluss 7: committed, nie
    zurückgerollt) — dort verdrahten, wo Session-Restore initialisiert.
  - **Rows** (42 px Grid): rechts idle „since Jul 12" (11.5, weiß 40 %) ↔ Hover-Pill „Remove"
    (rosa). „Locate…" kommt in 5.3. Kontextmenü mit denselben Aktionen. Mehrfachauswahl.
  - **Headerbar-MenuButton** „Auto-clean: off ▾" (off/30/90). Aktivierung **mit Bestand** →
    SET-4-Dialog: „This will remove N tracks now (deleted more than 30 days ago) — their
    ratings and listening history go with them." Buttons „Remove now" (= `run_auto_clean` +
    `armed_at = now`) / „Start counting from today" (= **nur** `armed_at = now`).
  - **Teal-Infokarte** (Akzent 7 % Fläche, 18 % Border): Auto-Relink-Erklärung + „Last scan
    relinked N tracks" (Settings-Key `last_scan_relinked`; fehlt er → Zeile ausblenden,
    geschrieben wird er in 5.5). Wenn Auto-clean off **und** Deleted-Gruppe nicht leer:
    „Tracks deleted from disk stay listed until you remove them — enable auto-clean to do
    this automatically." (Beschluss 9).
  - **Fußnote:** „Remove only removes library entries — never files."
  - **Leerzustand:** `adw::StatusPage` „No missing files ✓" (Icon teal).
- [ ] **Regel-Flips in `docs/ux-rules.md` — in DIESEM Commit**, je mit grünem regelbenanntem
  Test: **FB-7** (`fb_7_…`, [core]: Row + Ratings + Playlist-Positionen überleben, Undo ist
  exakt, Timeout committed), **SET-4** (`set_4_…`), **FB-5** (`fb_5_…`). **Ehrlich prüfen, ob
  ein Commit alle drei beweisen kann** — FB-5 nennt auch „Library folder unavailable — Retry",
  das erst 5.5 liefert. Kann er es nicht: **Regel splitten** (Prozessregel des Regelwerks),
  nie eine Regel flippen, die der Test nicht deckt.
- [ ] Gates + Commit `feat(ui): missing-files view with grouped cards, remove-all undo, auto-clean`

### Task 3.3: Import-errors-View neu + FB-3

**Dateien:** `ui/import_errors_view.rs` (300 Zeilen, **noch auf der alten API**:
`query_import_errors`/`ImportErrorRow`/`delete_import_error` — Datei bleibt, Inhalt neu);
`ui/strings.rs` (append).

**Nutzt:** `query_import_errors_grouped`, `query_dismissed_import_errors`,
`count_dismissed_import_errors`, `dismiss_import_error`, `dismiss_all_import_errors`,
`restore_import_error`. Einzeldatei-Retry: synchroner `scan_folder(<datei>)` — das etablierte
Muster steht im Modul-Doc der Datei (walkdir besucht dann nur diesen Eintrag).

- [ ] **Inhalt:**
  - **Gruppen-Cards nach `kind`** mit humanen Labels: UnreadableTags → „Unreadable tags"
    (Zeilentext: „Tags unreadable — the file itself can usually still be played"),
    PermissionDenied → „Permission denied", UnsupportedFormat → „Unsupported format", Io →
    „Read error", Unknown → „Unclassified". Test iteriert **alle** Enum-Varianten (Vollständigkeit).
  - **Row:** Dateiname bold + Pfad-Rest ellipsized (Tooltip = voller Pfad) · humaner
    Fehlertext · „seen in N scans" · **`reason_detail` nur im Tooltip/Expander, nie in der
    Zeile** (Beschluss 5). Hover-Pills: Retry · Dismiss (mit aktuellem `stat`) · Show in Files
    (`gtk4::FileLauncher::open_containing_folder`).
  - **Hinweis-Rows** (`is_hint`): eigene Optik („imported without metadata"), **kein Retry**,
    Primäraktion „Open in Tag Editor" — **nur wenn der Tag-Editor in diesem Branch existiert**
    (`feat/tag-editor-rework` läuft parallel; zur Ausführungszeit prüfen). Fehlt er: Aktion
    weglassen + Ledger-Notiz.
  - **Kopf:** „Retry all" (Akzent-Pill; iteriert Einzeldatei-Scans off-thread nach dem
    `one_shot_task`-Muster) · „Dismiss all" (flat) · „Export list…" (`GtkFileDialog`-Save,
    eine Pfadzeile pro non-dismissed Eintrag, .txt).
  - **Fußzeile:** „N dismissed · Show" → expandierte Liste mit „Restore"-Pill (=
    `restore_import_error` **+ sofortiger Einzeldatei-Retry**, Beschluss 8).
- [ ] **FB-3 flippen** (`fb_3_…`): Einzelfehler werden gesammelt, nie einzeln getoastet; am
  Ende **ein** Toast „N failed · Details" → öffnet die View. Regeltext vorher lesen.
- [ ] Gates + Commit `feat(ui): rebuild import-errors view on issue cards with hints and dismissed footer`

### Task 3.4: Sidebar-Badges + FB-4

**Dateien:** `ui/sidebar/sidebar_rebuild.rs` (Counts `:30-35`, Sichtbarkeit `:118`),
`sidebar_presentation.rs`, `ui/view_session.rs` (`last_viewed_*` beim View-Wechsel).

- [ ] ISSUES-Sektion nur wenn `count_missing > 0 || count_import_errors_active > 0` (Gesamt).
      Badge-Zahl = `count_new_missing` / `count_new_import_errors` (neu seit `last_viewed`).
      Gelber Punkt bei Import errors **nur bei Badge > 0**. View öffnen → `last_viewed_* = now`
      + Badge-Refresh (bestehende Refresh-Hooks: Scan-Ende, View-Mutation — siehe
      `sidebar.rs`-Doc). Leergewordene Missing-View → Eintrag verschwindet, der bestehende
      Vanished-Source-Fallback wählt Music.
- [ ] **FB-4 flippen** (`fb_4_…`, [core]). Der Kern ist getestet — der Flip braucht den Beweis
      der **Regel**, inkl. Episoden-Reaktivierung (dismissed + Datei geändert → badgt wieder).
- [ ] Gates + Commit `feat(ui): issue badges count new-since-viewed, section hides when clean`

### Task 3.5: Alte Cleanup-Pfade umhängen

**Dateien:** `ui/sidebar/sidebar_issue_cleanup.rs` (`:109` dismiss-all, `:149` remove-all).

- [ ] „Remove all missing" → Tombstone-Pfad mit Undo-Toast (statt `remove_all_missing_tracks`
      hart), Kaskaden-Text im Dialog. „Dismiss all import errors" → `dismiss_all_import_errors`
      (Stat-Callback liefert die UI).
- [ ] Gates + Commit `refactor(ui): route sidebar issue cleanup through tombstone and dismiss`

---

## Paket 4 — Playlist/Queue

### Task 4.1: Playlist-Queries zeigen Missing

**Dateien:** `queries/playlist.rs` (4 Stellen), Tests `queries/tests_playlist.rs`.

- [ ] `{PRESENT}` fällt aus Window (`:40`) und Count (`:81`) — Missing-Rows erscheinen an
      fester `pt.position`. SELECT ergänzt `missing_since, missing_reason, untagged`.
- [ ] **M3U-Export (`:141`): Filter BLEIBT** — der load-bearing Kommentar bleibt erhalten
      (Beschluss 11).
- [ ] ids zweigleisig: `query_playable_track_ids_playlist` (nur `{PRESENT}` — Play all/Shuffle/
      Enqueue; das ist die heutige `:104`) vs. `query_visible_track_ids_playlist` (inkl.
      missing — Selektion/DnD, deckungsgleich mit den sichtbaren Rows). Bestehende Caller per
      grep zuordnen: Playback → playable, Auswahl/Reorder → visible.
- [ ] Rote Tests zuerst: Window enthält missing an fester Position · Count zählt sie · M3U
      exportiert sie **nicht** · playable-ids exkludieren · visible-ids inkludieren.
- [ ] Gates + Commit `feat(queries): manual playlists list missing tracks, playback ids stay playable-only`

### Task 4.2: Graues Rendering

**Dateien:** `ui/track_list/track_list_columns.rs` (`append_title_column`, `connect_bind`
`:235-270`), CSS.

- [ ] CSS-Klasse (Opacity 0.5) + Pango-Strikethrough auf dem Titel, wenn `track.is_missing()`.
      **Zwingend über das bestehende idempotente `toggle_class`-Muster (`:42`), berechnet aus
      dem frisch gebundenen Track bei JEDEM bind** — nie set-once beim setup, sonst leckt die
      Klasse beim Recycling auf fremde Rows (GTK recycelt ListItems; das Doc-Kommentar dort
      warnt genau davor).
- [ ] Tooltip nach Reason: `Unmounted` → „On unavailable drive — returns when mounted";
      `Deleted`/`Unknown` → „File missing since {format_unix_timestamp(missing_since)}".
      Puren Tooltip-Text-Helfer testen.
- [ ] Gates + Commit `feat(ui): render missing playlist rows greyed with reason tooltip`

### Task 4.3: PLAY-4b — Doppelklick erklärt, Einreihen deaktiviert

**Dateien:** `ui/track_list/track_actions.rs`, `track_list_context_menu.rs`,
`ui/track_list/track_list_activation.rs`. **Existiert heute zu 0 %.**

- [ ] Doppelklick/Aktivierung auf missing Row: **kein Play**; `adw::Toast` mit Reason-Text +
      Button „Show in Missing files" → Sidebar-Navigation auf `ViewSource::Missing`.
- [ ] Kontextmenü: „Play Next"/„Add to Queue" für Selektionen ohne playable Rows **deaktiviert**;
      gemischte Selektion → auf playable filtern (konsistent mit 4.1), **keine Toast-Kaskade**.
- [ ] **PLAY-4b flippen** (`play_4b_…`, [gtk]). Pure Regel testen (Selektion → enabled/gefilterte ids).
- [ ] Gates + Commit `feat(ui): missing rows explain instead of play, enqueue filters to playable`

### Task 4.4: PLAY-4a — stiller Skip beim Advance

**Dateien:** `reprise-core/src/queue.rs` und/oder `ui/playback/up_next_transport.rs`.
**Existiert heute zu 0 %.**

- [ ] `Queue::advance_auto` (`queue.rs:100`) und `UpNextQueue` sind bewusst presence-blind
      (opake ids). **Entscheiden und begründen:** `Queue` ein Presence-Prädikat beibringen
      **oder** `next_target` (`up_next_transport.rs:13`) umschließen. Reaktives Analogon, das
      bereits über wiederholte Faults loopt: `handle_unplayable_track`
      (`ui/playback/playback_faults.rs:53`).
- [ ] **Kein Toast im Advance** (Beschluss 11) — der Toast ist FB-6s Ausnahme, nur wenn der
      **spielende** Track faultet.
- [ ] **PLAY-4a flippen** (`play_4a_…`, [core]).
- [ ] Gates + Commit `feat(playback): silent skip of missing tracks on queue advance`

### Task 4.5: PLAY-5b — Unmounted-Hygiene

**Dateien:** `queue.rs`/`up_next.rs`, `ui/playback/queue_transport.rs`.

- [ ] **Die deleted-Hälfte (PLAY-5a) ist FERTIG — nicht anfassen.** Nur: unmounted Tracks
      **bleiben** grau in der Queue (Reihenfolge!), werden beim Advance übersprungen, heilen
      beim Mount-Event (P-6, kommt in 5.4). Grau kommt über 4.2 automatisch (Queue ist eine
      ViewSource).
- [ ] **Kein Hintergrundereignis stoppt den spielenden Track** — explizite Nutzeraktionen
      wechseln die Wiedergabe natürlich (Regeltext PLAY-5b).
- [ ] **PLAY-5b flippen** (`play_5b_…`, [core]).
- [ ] Gates + Commit `feat(queue): keep unmounted tracks greyed in place, heal on mount`

---

## Paket 5 — Locate & Events

### Task 5.1: Core-Relink Einzeldatei

**Dateien:** neu `reprise-core/src/library/relink.rs`; Tests inline.

```rust
pub struct RelinkMismatch { pub old_duration_ms: i64, pub new_duration_ms: i64,
    pub old_title: String, pub new_title: Option<String> }
/// Mismatch gdw. |Δduration| > 2000ms (DIESELBE Toleranz wie find_move_candidate —
/// eine Wahrheit, Beschluss 12) ODER lesbarer neuer Titel weicht ab. None = sauberer Treffer.
pub fn probe_relink(conn, track_id: i64, new_path: &Path) -> Result<Option<RelinkMismatch>, ScanError>;
/// Bedingungsloses Anwenden über scanner_move::apply_file_identity — der User darf die Warnung überstimmen.
pub fn relink_track(conn, track_id: i64, new_path: &Path) -> Result<(), ScanError>;
```
`apply_file_identity` existiert (`library/scanner_move.rs`, `pub(crate)`) — **dieselbe
Funktion, die der Move-Arm ruft**; nicht duplizieren. Signatur dort nachlesen (sie nimmt
`title` und `untagged` zusätzlich).

- [ ] Rote Tests: sauberer Treffer bei Dauer ±2 s · Mismatch-Struktur bei Abweichung · relink
      behält id/rating, cleart missing/removed, setzt mount_point.
- [ ] Gates + Commit `feat(core): single-file relink with matcher-tolerance mismatch probe`

### Task 5.2: Core `relink_from_folder`

```rust
pub struct FolderRelinkReport { pub relinked: u32, pub group_size: u32 }
/// Walkt rekursiv; pro Audiodatei read_meta + find_move_candidate, beschränkt auf group_ids;
/// importiert NIE (sonst wäre es ein verstecktes "Add folder"); bricht ab, sobald alle
/// gematcht sind (Beschluss 12). cancel: AtomicBool pro Datei geprüft.
pub fn relink_from_folder(conn, folder: &Path, group_ids: &[i64],
    cancel: &AtomicBool, on_progress: impl FnMut(u32, u32)) -> Result<FolderRelinkReport, ScanError>;
```
- [ ] Rote Tests: Ordner-Umzug → alle N gematcht · **Kurzschluss** (Zähler beweist Abbruch nach
      letztem Match) · Fremddatei ohne Match wird **nicht** importiert · cancel stoppt.
- [ ] Gates + Commit `feat(core): folder relink matching only the missing group, never importing`

### Task 5.3: Locate-UI + FB-2

**Dateien:** `ui/issues/missing_view.rs` (+ `missing_dialogs.rs`), Off-Thread-Wiring nach dem
`ui/scan/scan_worker.rs`-Muster.

- [ ] „Locate…"-Pill + Kontextmenü **auf Deleted-/Unknown-Rows** (unavailable bleibt
      actionless): `GtkFileDialog` (File-Mode, `initial_folder` = Parent des alten Pfads) →
      `probe_relink` → bei Mismatch `adw::AlertDialog` „This looks like a different recording"
      mit **zwei Zeilen alt → neu** (Dauer, Titel), Buttons „Cancel" / **„Relink anyway"** →
      `relink_track` + Refresh.
- [ ] Pfad außerhalb `library_root` (Setting): Hinweiszeile „This file is outside your library
      folder — it won't be watched or rescanned." (erlaubt, aber ehrlich — Beschluss 12).
- [ ] „Search folder…" (Kontextmenü der Deleted-Card): Folder-Mode → `relink_from_folder`
      off-thread **mit Fortschritt + Abbrechen** → Toast „5 of 9 tracks relinked".
- [ ] **FB-2 flippen** (`fb_2_…`): Fortschrittskarte im Sidebar-Bottom-Slot — **den
      bestehenden Slot `ui/sidebar/sidebar_activity_slot.rs` wiederverwenden**, keinen zweiten
      bauen. Regeltext vorher lesen (Spinner + Titel + % + 3-px-Balken + Detailzeile; Klick →
      View, Cancel bricht ab).
- [ ] Gates + Commit `feat(ui): locate flow with honest mismatch warning and folder search`

### Task 5.4: GVolumeMonitor-Wiring + FB-6 + P-6

**Dateien:** neu `ui/mounts.rs`; `queries/issues.rs` (zwei Helfer); `ui/window/window.rs` (Init).

```rust
// core:
/// Existenz-Check für jede Zeile mit reason unmounted/unknown; cleart missing bei Fund
/// (mtime-Drift holt der nächste Scan). Gibt geräumte ids zurück (Queue-Heilung + Refresh).
pub fn verify_unmounted_tracks(conn) -> Result<Vec<i64>, rusqlite::Error>;
/// Eager Unmount-Marking (Beschluss 13): alle PRESENT-Rows unter mount_point -> unmounted.
pub fn mark_mount_unavailable(conn, mount_point: &str, now: i64) -> Result<u32, rusqlite::Error>;
```
- [ ] gnome: `gio::VolumeMonitor::get()` (Muster: `reprise-platform-linux/src/device_sync.rs:61`).
      `mount-added` → off-thread `verify_unmounted_tracks` → Sidebar/View-Refresh; war der
      `library_root` unavailable (Merker im UI-State aus 5.5) → `spawn_scan`-Reconcile.
      `mount-removed` → `mark_mount_unavailable(mount.root().path())` → Refresh. Queue bleibt
      (grau); **der spielende Track wird NIE proaktiv gestoppt** — GStreamers Fault-Pfad ist
      die eine Wahrheit.
- [ ] **FB-6 und P-6 flippen** (`fb_6_…`, `p_6_…`) — beide Regeltexte vorher vollständig lesen.
- [ ] Gates + Commit `feat(ui): mount events verify unmounted tracks and mark ejected mounts`

### Task 5.5: Scan-Flow — Status, Toast, Auto-Clean-Lauf

**Dateien:** `ui/scan/scan_flow.rs`, `scan_worker.rs`, `ui/strings.rs`.

- [ ] `ScanOutcome::RootUnavailable` → Scan-Karte zeigt „Library folder unavailable — `<root>`
      not mounted" **statt Fortschritt** (Beschluss 4; heute nur generischer Fehlertext).
      FB-5 nennt hier „Retry" als den einen nächsten Schritt.
- [ ] Bei `Completed`: aggregierter Toast **nur bei > 0** — „3 moved files relinked · 2
      previously failed files imported" (aus `report.moved`/`report.healed`; Teile einzeln
      weglassen bei 0). Puren Toast-Text-Bau testen (0/1/beide Teile).
- [ ] Settings `last_scan_relinked = report.moved` schreiben (Teal-Karte aus 3.2).
- [ ] Danach `run_auto_clean(now)` — **nur bei `Completed`, nie bei `RootUnavailable`** +
      Queue-Purge der zurückgegebenen ids + Sidebar-Refresh.
- [ ] Gates + Commit `feat(ui): scan status for unavailable root, aggregated heal toast, auto-clean run`

### Task 5.6: Tag-Editor-Re-Read (konditional, wahrscheinlich schon da)

- [ ] `library/tag_edit.rs::apply_patch_batch` ruft **bereits** pro Track `scan_folder` nach
      einem Tag-Write. **Verifizieren statt neu bauen:** deckt das Beschluss 6 ab (Tags wieder
      lesbar → Hinweiszeile weg, `untagged` weg, **kein** Toast — der User sieht die Heilung
      direkt)? Wenn ja: Test ergänzen, der es festnagelt, und Ledger-Notiz. Wenn nein: die
      Lücke schließen.
- [ ] Gates + Commit `feat(ui): tag-editor save re-reads the file immediately`

---

## Paket 6 — Integration & Abnahme

### Task 6.1: Sync-Delta-Audit

- [ ] `rg 'FROM tracks' crates/reprise-core/src/device_sync* crates/reprise-gnome/src/ui/device_sync/`
      — jede Query, die kopierbare Tracks auswählt, MUSS `{PRESENT}` tragen (Beschluss 7: kein
      „to copy" für tombstoned/missing Rows). Pro Fund erst roter Test, dann Fix.
- [ ] Commit `fix(sync): exclude missing and tombstoned tracks from sync deltas`

### Task 6.2: Tote Pfade entfernen — **nur nach grep-Beweis**

- [ ] `query_import_errors` / `ImportErrorRow` / `delete_import_error` /
      `delete_all_import_errors` verlieren ihre Caller in 3.3/3.5 → entfernen.
- [ ] `remove_missing_track(s)` / `remove_all_missing_tracks`: **NICHT blind entfernen**
      (Korrektur 2 oben). Erst greppen; nur entfernen, was keinen Caller mehr hat. Clippy fängt
      `dead_code`.
- [ ] Commit `chore: drop pre-18a missing/import-error paths`

### Task 6.3: Abnahme

- [ ] Automatisiert (Tests existieren größtenteils aus P1–P5): Rename+Scan → relinkt mit
      Ratings + Toast · Root-Unmount+Scan → `RootUnavailable`, nichts markiert · Teil-Mount weg
      → unavailable-Gruppe, nichts löschbar · Delete+Scan → deleted + „Remove all" mit Undo ·
      Tags fixen+Scan → Fehlerzeile weg · Dismiss+ändern → Eintrag zurück · **5 Scans → keine
      Duplikate** · Playlist zeigt grau an fester Position.
- [ ] Headless-Smoke (**voller Isolations-Präfix!**) für: ISSUES-Sichtbarkeit,
      Badge-Verhalten, Remove-all-Undo-Toast. CUA-Harness nur erweitern, wenn die bestehende
      Semantik das Muster hergibt.
- [ ] `bash scripts/check-ux-traceability.sh` — **jede** in diesem Branch beanspruchte Regel
      ist `[aktiv]` und gedeckt.
- [ ] Manuelle Restliste für den Maintainer dokumentieren: echtes NAS-Mount-Event,
      GVolumeMonitor auf echter Hardware, Optik-Review gegen 18a.
- [ ] Ledger-Abschluss + finale Gates. Commit `test: acceptance pass for self-healing issue lists`

---

## Offene Punkte für den Maintainer

- **Mock „Import-errors-View" (t19)** existiert nicht; 3.3 baut nach dem 18a-Vokabular plus
  Abschnitt 4 der Aufgabenstellung. Kommt der Mock später, taugt er als Design-Review gegen
  die gebaute View (so abgesprochen).
- **Mock „Locate…-Dialog"** ebenfalls nicht nötig — der Flow ist in Beschluss 12 spezifiziert.
- **Tag-Editor** (`feat/tag-editor-rework`) läuft parallel: 3.3s „Open in Tag Editor" und 5.6
  hängen davon ab, ob er zur Ausführungszeit gemerged ist.
