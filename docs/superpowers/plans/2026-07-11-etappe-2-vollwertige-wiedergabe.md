# Reprise — Etappe 2: Vollwertige Wiedergabe & Bibliothek — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aus dem hörbaren Kern wird ein benutzbarer Player: Bibliotheken beliebiger Größe (echtes SQL-Windowing statt 200-Zeilen-Deckel), Warteschlange mit Auto-Weiter/Shuffle/Repeat, klickbare Bewertungen + Play-Count-Tracking, MPRIS (GNOME-Mediensteuerung), robuste Wiedergabefehler-Behandlung und „N of M tracks"-Statistik.

**Architecture:** Ein eigenes `GListModel`-Subclass (`TrackListModel`) ersetzt den `gio::ListStore` und holt Fenster à 200 Zeilen lazy aus `queries.rs` — `GtkColumnView` virtualisiert die Widgets, das Modell virtualisiert die Daten. Die Warteschlange ist ein pures Rust-Modul (TDD), verdrahtet im `PlayerController`. MPRIS läuft als zbus-Blocking-Server auf eigenem Thread, Kommandos fließen über den bestehenden async-channel-Pfad auf den GTK-Main-Thread.

**Tech Stack:** bestehend (gtk4 0.11/v4_22, libadwaita 0.9, gstreamer 0.25, rusqlite, tracing) + `zbus` (blocking, eigener Thread) für MPRIS.

**Spec:** `docs/superpowers/specs/2026-07-11-reprise-design.md` · **Ledger/Backlog:** `.superpowers/sdd/progress.md`

## Global Constraints

- Branch `main`, Commits direkt; Commit-Format `<type>: <description>`, englisch, keine Attribution
- **Alles Englisch** (Code, Kommentare, Logs, UI-Strings); UI-Strings nur über `src/ui/strings.rs`
- Fehlertoleranz: kein `unwrap()`/`expect()` außerhalb Tests + `main()`-Startup; `thiserror` + `Result`; Fehler geloggt, nie verschluckt; externe Zustände (Dateien!) crashen nie
- SQL nur parametrisiert; Sortier-Whitelist bleibt die einzige Quelle für ORDER BY; Limits gedeckelt (`MAX_WINDOW_LIMIT`)
- Logik testbar halten: Entscheidungen als pure Funktionen (Muster: `empty_state_for`, `should_apply_position_tick`)
- **Verifikation headless:** `xvfb-run -a`, `REPRISE_AUDIO_SINK=fakesink`, Scratch-`XDG_DATA_HOME`; für MPRIS/D-Bus zusätzlich `dbus-run-session`; niemals Fenster/Audio auf dem Live-Desktop
- Bestehende Env-Hooks (`REPRISE_SCAN_DIR`, `REPRISE_SMOKE_ACTIVATE`, `REPRISE_SMOKE_QUIT`) weiterverwenden; neue Hooks im selben Stil dokumentieren
- Vor jedem Commit: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`

---

### Task 1: TrackListModel — echtes SQL-Windowing (GListModel-Subclass)

**Files:**
- Create: `src/ui/track_list_model.rs`
- Modify: `src/queries.rs` (Count-Query), `src/ui/track_list.rs` (ListStore → TrackListModel), `src/ui/mod.rs`

**Interfaces:**
- Consumes: `queries::query_track_window(conn, sort_field, sort_dir, filter, offset, limit)`
- Produces:
  - `queries::query_track_count(conn: &Connection, filter: &str) -> Result<i64, rusqlite::Error>` — `SELECT count(*) FROM tracks WHERE missing = 0` + identische Filter-Klausel wie `build_track_query` (Filter-SQL in eine gemeinsame Helferfunktion extrahieren, DRY)
  - `TrackListModel` (glib::Object-Subclass, implementiert `gio::ListModel`): `new(conn: Rc<RefCell<Connection>>) -> Self`, `set_query(&self, sort_field: &str, sort_dir: &str, filter: &str)` (lädt Count neu, leert Cache, feuert `items_changed(0, old_n, new_n)`), `track_at(&self, position: u32) -> Option<models::Track>` (Klon aus dem Cache; für Aktivierung/Rating)
  - Items sind `glib::BoxedAnyObject` um `models::Track` (wie bisher)

- [ ] **Step 1 (TDD):** Failing Tests für `query_track_count` in `queries.rs`: leere DB → 0; 3 Tracks eingefügt → 3; Filter "zu" (Fixture-Titel „Zulu") → 1; `missing = 1`-Zeile wird nicht gezählt. RED bestätigen.
- [ ] **Step 2:** `query_track_count` implementieren (gemeinsame Filter-Klausel-Helferfunktion mit `build_track_query`). GREEN.
- [ ] **Step 3:** `TrackListModel` als ObjectSubclass mit `ListModelImpl`: innerer `RefCell<ModelState { total: u32, sort_field: String, sort_dir: String, filter: String, cache: BTreeMap<u32, Vec<Track>> }>`; Fenstergröße `WINDOW_SIZE: u32 = 200` (aus Task-8-Kommentar der Etappe 1 übernehmen); `item(position)`: Fensterindex `position / WINDOW_SIZE`, bei Cache-Miss synchron `query_track_window` (offset = Fensterstart, limit = WINDOW_SIZE) laden, Cache auf max. 8 Fenster begrenzen (ältestes raus — BTreeMap-Iteration reicht); Fehler → `tracing::error!` + `None` (nie panic). `n_items()` aus `total`.
- [ ] **Step 4:** `track_list.rs` umstellen: `gio::ListStore` ersetzen, `reload()` ruft `model.set_query(...)` (+ Count für `empty_state_for` aus `model.n_items()`); Aktivierung nutzt `track_at(position)`. Der 200-Zeilen-Kommentar aus Etappe 1 wird entfernt.
- [ ] **Step 5:** Initialen Sortier-Indikator setzen (Backlog #7 der finalen Review): nach `wire_sort_clicks` einmalig `column_view.sort_by_column(Some(&artist_column), gtk4::SortType::Ascending)` — die bestehende Dedup-Guard verhindert die Doppel-Query.
- [ ] **Step 6:** Headless-E2E: Fixture-Ordner mit **250** Kopien (Schleife im Test-Setup, Dateinamen track_000.flac …) → `REPRISE_SCAN_DIR` + Smoke-Run: Log zeigt `total=250`; Aktivierung von Zeile 0 spielt. `cargo test`/clippy/fmt sauber.
- [ ] **Step 7:** Commit `feat: lazy TrackListModel with SQL windowing removes the 200-row cap`

---

### Task 2: Bewertungen setzen + Play-Count-Tracking

**Files:**
- Create: `src/ui/rating.rs` (Stern-Widget)
- Modify: `src/db.rs` oder neu `src/library/stats.rs` (Schreib-Helfer), `src/ui/track_list.rs` (Rating-Spalte interaktiv), `src/ui/player_controller.rs` (Hör-Tracking), `src/format.rs` nicht betroffen

**Interfaces:**
- Produces:
  - `library::stats::set_rating(conn, track_id: i64, rating: i32) -> Result<(), rusqlite::Error>` — clamp 0..=5
  - `library::stats::record_play(conn, track_id: i64, now_unix: i64) -> Result<(), rusqlite::Error>` — `play_count += 1`, `last_played_at = now`
  - `library::stats::should_count_play(max_position_ms: i64, duration_ms: i64) -> bool` — pure: true wenn `duration_ms > 0 && max_position_ms * 2 >= duration_ms` (Spec: „>50 % gehört")
  - `ui::rating::RatingWidget` — 5 Sterne, Klick auf Stern n setzt Rating n; Klick auf den aktuellen Wert setzt 0 (Rhythmbox-Verhalten); Callback `set_on_changed(Fn(i32))`

- [ ] **Step 1 (TDD):** Failing Tests: `should_count_play` (0/1000→false, 500/1000→true, 499/1000→false, x/0→false); `set_rating` clamp (7→5, -1→0, persistiert); `record_play` (count inkrementiert, last_played_at gesetzt). RED → implementieren → GREEN.
- [ ] **Step 2:** `RatingWidget`: `gtk::Box` mit 5 `gtk::Image`s (`starred-symbolic`/`non-starred-symbolic`), eine `GestureClick` auf der Box (x-Position → Sternindex), Tooltip aus strings.rs („Rating"). Kein Panic-Pfad.
- [ ] **Step 3:** Rating-Spalte in `track_list.rs` auf `RatingWidget` umstellen; on_changed → `set_rating` + `tracing::debug!`; Modell-Zeile aktualisieren (Cache-Fenster invalidieren + `items_changed(pos, 1, 1)`).
- [ ] **Step 4:** Hör-Tracking im `PlayerController`: `max_position_ms: Cell<i64>` pro Titel (bei `Position`-Events maximieren, bei Titelwechsel/`TrackFinished` auswerten): wenn `should_count_play` → `record_play` (now via `std::time::SystemTime`, wie `now_unix()` im Scanner). Loggen (debug).
- [ ] **Step 5:** Headless-E2E: Smoke-Lauf — Fixture (1 s) spielt bis EOS → Log `play recorded`; sqlite3-Check `play_count = 1`. `set_rating` via Test abgedeckt (Klick-Pfad ist manuelle Prüfung).
- [ ] **Step 6:** Commit `feat: clickable star ratings and play-count tracking with 50% listen threshold`

---

### Task 3: Queue-Engine (pures Modul, TDD)

**Files:**
- Create: `src/queue.rs`
- Modify: `src/main.rs` (`mod queue;`)

**Interfaces:**
- Produces (alles pur, kein GTK/DB):
  - `queue::Repeat { Off, All, One }` (Copy, Default Off), `queue::Queue`
  - `Queue::set_tracks(ids: Vec<i64>, start_index: usize)` — Warteschlange = aktuelle Ansicht, Start beim Doppelklick-Titel
  - `Queue::current(&self) -> Option<i64>`
  - `Queue::advance_auto(&mut self) -> Option<i64>` — Titelende: `Repeat::One` → selber Titel; sonst nächster (Shuffle-Reihenfolge falls aktiv); am Ende: `Repeat::All` → von vorn, `Off` → `None`
  - `Queue::next_manual(&mut self) -> Option<i64>` — Nutzer-Klick: ignoriert `One`, sonst wie auto; am Ende bei `Off` → `None`
  - `Queue::previous(&mut self) -> Option<i64>` — voriger Titel (Shuffle-Reihenfolge rückwärts); am Anfang → erster Titel bleibt
  - `Queue::set_shuffle(&mut self, on: bool)` — an: zufällige Permutation der verbleibenden Titel, aktueller bleibt aktuell (Seed via übergebenem `&mut impl rand-artigem Generator`? Nein: einfacher deterministischer Fisher-Yates mit `fastrand`-Crate ODER `std`-basiert über `SystemTime`-Seed — Entscheidung: **`fastrand`** (winzig, kein rand-Stack); Tests injizieren `fastrand::seed(42)`)
  - `Queue::set_repeat(&mut self, r: Repeat)`, Getter für UI-Zustand

- [ ] **Step 1 (TDD):** Failing Table-Tests (mindestens): linear advance bis Ende → None; Repeat::All wrap; Repeat::One bei advance_auto wiederholt / bei next_manual weiter; previous am Anfang; Shuffle: alle Titel genau einmal pro Durchlauf (Seed fix), aktueller Titel bleibt beim Einschalten aktuell; leere Queue → alles None. RED.
- [ ] **Step 2:** Implementieren (Struktur: `ids: Vec<i64>`, `order: Vec<usize>` — identisch oder permutiert —, `pos: Option<usize>` als Index in `order`, `repeat: Repeat`). GREEN.
- [ ] **Step 3:** Commit `feat: queue engine with shuffle, repeat modes, and auto-advance semantics`

---

### Task 4: Queue-Verdrahtung — Auto-Weiter, Transport-Buttons

**Files:**
- Modify: `src/ui/player_controller.rs` (Queue besitzen, TrackFinished → advance), `src/ui/player_bar.rs` (Prev/Next/Shuffle/Repeat-Buttons, Mockup 7a: Shuffle | Prev | Play | Next | Repeat mittig), `src/ui/track_list.rs`/`src/ui/window.rs` (Doppelklick liefert View-IDs + Startindex), `src/ui/strings.rs`

**Interfaces:**
- Consumes: `queue::Queue` (Task 3), `TrackListModel::track_at` + neue Methode `TrackListModel::visible_ids(&self) -> Vec<i64>` (IDs der aktuellen Query-Reihenfolge — via `SELECT id` Variante von `query_track_window` OHNE Limit… **Achtung `MAX_WINDOW_LIMIT`**: eigene `queries::query_track_ids(conn, sort_field, sort_dir, filter) -> Result<Vec<i64>, _>` mit eigenem Deckel `QUEUE_LIMIT: i64 = 10_000` + Log-Hinweis beim Kappen)
- Produces: Aktivierung → `controller.play_from_view(ids, start_index)`; `TrackFinished` → `advance_auto` → nächster Titel spielt (Pfad via `track path` aus DB: `queries::query_track_path(conn, id) -> Result<Option<String>, _>`); Buttons rufen `next_manual`/`previous`/`set_shuffle`/`set_repeat` (Repeat-Button cycelt Off→All→One, Icons `media-playlist-repeat-symbolic`/`-song-symbolic`, Shuffle `media-playlist-shuffle-symbolic`, Toggle-Zustand via CSS-Klasse `suggested-action` oder `ToggleButton`)

- [ ] **Step 1 (TDD):** Failing Tests für `query_track_ids` (Reihenfolge = Whitelist-Sortierung, Filter wirkt, Deckel greift) und `query_track_path` (vorhanden/nicht vorhanden). RED → implementieren → GREEN.
- [ ] **Step 2:** `play_from_view` im Controller: Queue setzen, Titel spielen, Play-Tracking-Reset (Task 2). `TrackFinished`-Arm: statt Reset → `advance_auto`; `None` → Stopped-Reset wie bisher.
- [ ] **Step 3:** Player-Bar-Buttons (Reihenfolge laut Mockup), alle Tooltips/Labels aus strings.rs; insensitive wenn Queue leer; Repeat/Shuffle-Zustand optisch erkennbar.
- [ ] **Step 4:** Headless-E2E: 3 Fixture-Kopien, Smoke-Activate von Zeile 0 → Log zeigt drei aufeinanderfolgende `Playing`-Titel (Auto-Advance ×2) und danach Stopped (Repeat Off). Zweiter Lauf mit neuem Hook `REPRISE_SMOKE_REPEAT=all` (dev hook, dokumentieren): nach Titel 3 kommt wieder Titel 1 (mind. 4 Playing-Events), Smoke-Quit beendet.
- [ ] **Step 5:** Commit `feat: queue wiring — auto-advance, previous/next, shuffle and repeat controls`

---

### Task 5: Wiedergabefehler-Toleranz — Toast, missing-Markierung, Auto-Skip

**Files:**
- Modify: `src/ui/player_controller.rs`, `src/queries.rs` (`mark_missing`), `src/ui/window.rs` (ToastOverlay-Zugriff für Controller), `src/ui/strings.rs`, `src/player.rs` (nur falls Fehlerpfad-Infos fehlen)

**Interfaces:**
- Produces:
  - `queries::mark_track_missing(conn, track_id: i64) -> Result<(), rusqlite::Error>` — `UPDATE tracks SET missing = 1 WHERE id = ?1`
  - Controller-Verhalten (Spec „Fehlerbehandlung"): `play()`-Err ODER `PlayerEvent::Error` während eines Queue-Titels → `tracing::error!`, Toast („Could not play <title> — skipping" aus strings.rs mit Platzhalter), wenn die Datei nicht mehr existiert (`std::path::Path::exists` — billig, keine TOCTOU-Illusion nötig) zusätzlich `mark_track_missing` + Toast „File not found — marked as missing", dann `next_manual`-Skip (Endlosschleifen-Schutz: max. so viele Skips in Folge wie Queue-Länge, dann Stopped + Toast)
  - Nach `mark_track_missing`: Track-Liste neu laden (missing-Zeilen verschwinden aus der Ansicht — Query filtert `missing = 0`; die Sidebar-Quelle „Fehlende Dateien" kommt in Etappe 3)

- [ ] **Step 1 (TDD):** Failing Tests: `mark_track_missing` persistiert; danach zählt `query_track_count` den Titel nicht mehr; Skip-Schleifen-Schutz als pure Funktion `should_stop_skipping(consecutive_skips: usize, queue_len: usize) -> bool` mit Tests. RED → GREEN.
- [ ] **Step 2:** Controller-Verdrahtung (Fehlerpfade wie oben; Toast über die bestehende Overlay-Referenz — per `Weak` in den Controller geben).
- [ ] **Step 3:** Headless-E2E (der Fehlertoleranz-Kernfall des Nutzers!): 3 Fixture-Kopien scannen, dann Datei 2 **physisch löschen**, Smoke-Activate Zeile 0 → Log: Titel 1 spielt → Titel 2 Fehler/missing markiert/geskippt → Titel 3 spielt → Exit 0, App nie gecrasht; sqlite3: `missing = 1` für Titel 2.
- [ ] **Step 4:** Commit `feat: playback fault tolerance — toast, missing flag, and auto-skip on broken files`

---

### Task 6: MPRIS (zbus) — GNOME-Mediensteuerung

**Files:**
- Create: `src/mpris.rs`
- Modify: `Cargo.toml` (`zbus = "5"` — Feature-Minimierung: default-features prüfen, blocking reicht), `src/main.rs`, `src/ui/player_controller.rs` (Zustands-Spiegel + Kommando-Empfang)

**Interfaces:**
- Produces:
  - Busname `org.mpris.MediaPlayer2.reprise`; Interfaces `org.mpris.MediaPlayer2` (Identity "Reprise", DesktopEntry "org.reprise.Reprise", CanQuit/CanRaise false vorerst) und `org.mpris.MediaPlayer2.Player` (PlaybackStatus, Metadata: `mpris:trackid`, `mpris:length` (µs!), `xesam:title`, `xesam:artist`, `xesam:album`; Play/Pause/PlayPause/Next/Previous/Stop; CanPlay/CanPause/CanGoNext/CanGoPrevious dynamisch aus Queue-Zustand; Position read-only + `Seeked`-Signal reicht NICHT für Etappe 2 → **Position/SetPosition/Seek weglassen, CanSeek=false** — YAGNI, kommt mit Sperrbildschirm-Feinschliff)
  - Architektur: eigener Thread mit `zbus::blocking::connection::Builder`; Shared State `Arc<Mutex<MprisState { status, title, artist, album, duration_ms, track_id, can_next, can_prev }>>`; Controller schreibt bei jedem Zustands-/Titelwechsel + ruft `ctx.emit_properties_changed` über einen `zbus`-Handle (oder: MPRIS-Thread pollt den Mutex alle 500 ms und diffed — einfacher, wählen wenn PropertiesChanged-Plumbing hakelig wird; Entscheidung dokumentieren); Kommandos (Play/Pause/Next/…) → `async_channel::Sender<MprisCommand>` → bestehender Drain im Controller
  - Modul-Ausfall ist nie fatal: kein D-Bus (z. B. Test-Env ohne Session-Bus) → `tracing::warn!`, App läuft ohne MPRIS weiter

- [ ] **Step 1:** `zbus`-Dependency + Skelett: Threadstart, Busname claimen, leere Interfaces; Failing-Check: `dbus-run-session -- sh -c 'xvfb-run -a cargo run & sleep 5; busctl --user introspect org.mpris.MediaPlayer2.reprise /org/mpris/MediaPlayer2'` zeigt beide Interfaces.
- [ ] **Step 2:** Metadata/Status-Spiegel + Kommandos verdrahten (MprisCommand → Controller-Methoden aus Task 4).
- [ ] **Step 3:** Headless-E2E (dbus-run-session + xvfb): App mit 2 Fixtures, Smoke-Activate; `busctl --user get-property ... PlaybackStatus` → "Playing"; `busctl --user call ... Pause` → Log `state=Paused` + Property "Paused"; `Next` → zweiter Titel. Alles ins Report-Log.
- [ ] **Step 4:** Fehlerfall-Test: Lauf OHNE dbus-run-session-Bus (env `DBUS_SESSION_BUS_ADDRESS` auf ungültig) → warn-Log, App funktioniert (Smoke-Lauf grün).
- [ ] **Step 5:** Commit `feat: MPRIS integration (metadata, playback status, transport commands)`

---

### Task 7: Gefilterte Statistik + Etappen-Politur

**Files:**
- Modify: `src/queries.rs` (`filtered_count` real), `src/ui/status_bar.rs`, `src/ui/strings.rs`, `src/models.rs` + `src/library/scanner.rs` (Serde-Reste), `Cargo.toml` (authors, dirs), `src/ui/track_list.rs` (`REPRISE_SMOKE_FILTER`-Hook)

**Interfaces:**
- Produces:
  - `query_library_stats(conn, filter: &str)` — Signaturänderung: `filtered_count = Some(query_track_count(conn, filter))` wenn Filter nicht leer, sonst `None`; `track_count`/`total_duration_ms` bleiben ungefiltert
  - Statuszeile: ohne Filter wie bisher; mit Filter `"{filtered} of {total} tracks · {total_duration}"` (Spec „42 von 1.704"); Format-Funktion pur + getestet
  - Politur (finale-Review-Backlog): `Serialize`-Derives + camelCase-Kommentar („frontend expects…") aus `models.rs`/`scanner.rs`/`queries.rs` entfernen, `serde`-Dependency streichen falls dann ungenutzt; `Cargo.toml` `authors = ["Marvin Baudach"]` (Git-Config-Name) + `dirs = "6"`-Bump (API-Drift via Compiler prüfen); Dev-Hook `REPRISE_SMOKE_FILTER=<text>`: setzt nach dem Initial-Load den Suchfilter programmatisch (damit ist der NoResults-Zustand headless fahrbar)

- [ ] **Step 1 (TDD):** Failing Tests: Stats-Format-Funktion (mit/ohne Filter, en-US-Tausender); `query_library_stats` mit Filter liefert `filtered_count = Some(n)` konsistent zu `query_track_count`. RED → GREEN.
- [ ] **Step 2:** Statuszeile + Refresh-Pfade anpassen (Filter-Änderung aktualisiert die Zeile bereits über on_reload).
- [ ] **Step 3:** Politur-Punkte umsetzen; `cargo test`/clippy/fmt sauber; Headless-Smoke mit `REPRISE_SMOKE_FILTER=nomatch` → Log `state=NoResults` (der bisher ungetestete Zweig live).
- [ ] **Step 4:** Commit `feat: filtered track statistics in status bar; drop web-era serde remnants and polish`

---

### Task 8: Scanner-Move-Detection — verschobene Alben behalten ihre Metadaten

> Nutzer-Anforderung 2026-07-11: „wenn ich ein Album verschiebe, sollte der
> Scanner das finden und die Musik sollte in meiner Datenbank erhalten
> bleiben mit dem neuen Ortsverweis anstatt es als neue Dateien zu sehen
> ohne meine gespeicherten Meta-Daten." — **wichtig: explizite Testcases.**

**Files:**
- Modify: `src/db.rs` (Schema v2), `src/library/scanner.rs` (Move-Logik + Tests), `src/models.rs` (neue Felder)

**Interfaces:**
- Produces:
  - Schema-v2-Migration (idempotent, `PRAGMA user_version` 1→2): `ALTER TABLE tracks ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0` / `device INTEGER` / `inode INTEGER`; Index `idx_tracks_dev_inode ON tracks(device, inode)`
  - Scanner erfasst `file_size`/`device`/`inode` (`std::os::unix::fs::MetadataExt`: `size()`, `dev()`, `ino()`) bei Insert/Update
  - Neuer Rescan-Zweig für unbekannte Pfade — VOR dem Insert:
    1. `SELECT` Kandidat via `(device, inode)` unter Zeilen, deren alter Pfad nicht mehr existiert (`!Path::new(&old_path).exists()`) oder `missing = 1` → **Move**: `UPDATE tracks SET path=?, file_mtime=?, file_size=?, missing=0` + Tag-Felder refresh; `rating`/`play_count`/`added_at`/`last_played_at` unangetastet
    2. sonst Fingerprint: genau EIN Kandidat mit gleichem `title`+`artist`+`album`, `ABS(duration_ms - ?) <= 2000`, gleicher `file_size`, alter Pfad weg → Move wie oben; MEHRERE Kandidaten → kein Move (Ambiguität `tracing::warn!`), normal einfügen
    3. sonst normaler Insert (wie bisher)
  - `ScanReport` + Feld `moved: u32`; beim Move zusätzlich alten `import_errors`-Eintrag des neuen UND alten Pfads räumen (bestehende Lifecycle-Helfer)

- [ ] **Step 1 (TDD — die vom Nutzer geforderten Testcases, zuerst schreiben, RED bestätigen):**
  - `move_via_rename_preserves_metadata`: Fixture-Kopie scannen → `rating=5, play_count=7` via SQL setzen → Datei mit `std::fs::rename` in einen NEUEN Unterordner verschieben (Inode bleibt) → Rescan → Assertions: gleiche Zeilen-`id`, neuer `path`, `rating=5`, `play_count=7`, `added_at` unverändert, `report.moved == 1`, `report.added == 0`, Gesamtzahl Zeilen unverändert
  - `move_via_copy_delete_preserves_metadata` (Fingerprint-Pfad, simuliert Cross-Filesystem): Datei **kopieren + Original löschen** (Inode ändert sich!) → Rescan → gleiche Assertions wie oben
  - `ambiguous_duplicates_are_not_guessed`: ZWEI identische Kopien (gleicher Fingerprint) scannen, beide löschen, EINE neue Kopie an neuem Ort → Rescan → `moved == 0`, `added == 1`, warn-Log; die alten Zeilen bleiben (missing-Markierung ist Watcher-/Etappe-3-Thema)
  - `unchanged_files_are_not_matched_as_moves`: normale Bibliothek, nichts verschoben → Rescan → `moved == 0`, `skipped_unchanged` wie bisher
  - Migrations-Test: v1-DB (Schema per SQL-Snapshot anlegen, user_version=1) → `migrate` → v2-Spalten vorhanden, Daten intakt, zweiter Lauf idempotent
- [ ] **Step 2:** Schema v2 + Metadaten-Erfassung implementieren; bestehende Tests müssen unverändert grün bleiben (Upsert-Preservation-Test!). 
- [ ] **Step 3:** Move-Logik implementieren → alle neuen Tests GREEN; `cargo clippy`/`fmt` sauber.
- [ ] **Step 4:** Headless-E2E: Bibliothek scannen → App zu → Album-Ordner umbenennen → App mit `REPRISE_SCAN_DIR` auf denselben Root → Log `moved=N added=0`; sqlite3: `path` aktualisiert, `rating` erhalten.
- [ ] **Step 5:** Commit `feat: scanner move detection — relocated files keep ratings, play counts, and added date`

---

## Verifikation Etappe 2 (Definition of Done)

**✅ ETAPPE 2 ABGESCHLOSSEN (2026-07-11)** — alle headless-prüfbaren Punkte von der finalen Whole-Branch-Review unabhängig verifiziert (6 eigene E2E-Läufe); manuelle Nutzer-Checks ausstehend:

- [x] `cargo test` grün (121 + 1 ignored), `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` sauber
- [x] Headless: 250-Track-Bibliothek lädt (Windowing), Auto-Advance über 3 Titel, Repeat-All-Wrap, gelöschter Titel → missing + Skip ohne Crash, MPRIS Play/Pause/Next/Stop via busctl, NoResults via SMOKE_FILTER
- [x] sqlite3-Checks: play_count nach EOS, rating persistiert, missing gesetzt — und Restore beim Rescan am alten Pfad (Close-out-Fix)
- [x] Move-Detection: Album-Ordner umbenannt + Rescan → `moved=3, added=0`, Bewertungen/Play Counts/added_at erhalten, keine Duplikate
- [ ] **Manuell (Nutzer):** Medientasten + GNOME-Schnellmenü/Sperrbildschirm steuern Reprise, Sterne klicken (inkl. Löschen per Re-Klick), Shuffle/Repeat-Button-Gefühl und Icon-Zustände, große echte Bibliothek scrollt flüssig, Album verschieben + Rescan behält Bewertungen

**Nicht in Etappe 2:** Sidebar (Playlisten/Warteschlangen-Ansicht/Fehlende Dateien), Browse-Leiste, Cover, Rhythmbox-Import, Tag-Editor, Löschen, Watcher, Einstellungs-Dialog (inkl. Layout-Optionen), EQ/ReplayGain, gettext, Flatpak.
