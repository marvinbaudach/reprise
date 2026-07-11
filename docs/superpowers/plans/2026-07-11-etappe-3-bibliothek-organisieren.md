# Reprise — Etappe 3: Bibliothek organisieren — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aus dem Player wird eine Bibliotheksverwaltung: Navigations-Seitenleiste (Mockup 7a), manuelle + intelligente Playlists (befüllbar über **Drag & Drop UND Kontextmenü** — explizites Nutzer-Kriterium), Warteschlangen-Ansicht, M3U-Import/-Export, Echtzeit-Ordnerüberwachung (notify, nutzt die Move-Detection aus Etappe 2 wieder) und Tastatur-Shortcuts.

**Architecture:** Eine `ViewSource`-Abstraktion (Library | Playlist(id) | Smart(id) | Queue | Missing | ImportErrors) parametrisiert die bestehende Query-Schicht und das `TrackListModel`; die Sidebar (AdwNavigationSplitView) wählt die Quelle. Playlists sind Schema v3 (drei Tabellen aus der Spec). Der Watcher läuft als notify-Thread mit Debounce und speist dieselbe Scan-/Move-Logik wie der manuelle Scan.

**Tech Stack:** bestehend + `notify = "8"` (inotify). Kein neues UI-Framework.

**Spec:** `docs/superpowers/specs/2026-07-11-reprise-design.md` · **Ledger/Backlog:** `.superpowers/sdd/progress.md`

## Global Constraints

- Branch `main`; Commit-Format `<type>: <description>`, englisch, keine Attribution
- Alles Englisch; UI-Strings nur über `src/ui/strings.rs`
- Kein `unwrap()`/`expect()` außerhalb Tests + `main()`-Startup; hoisted borrows (dokumentierte Invariante — DREI frühere Review-Funde dieser Klasse); Fehler nie verschluckt; Dateien < 800 Zeilen (Task 1 stellt das wieder her)
- SQL nur parametrisiert; Sortier-Whitelist bleibt einzige ORDER-BY-Quelle; Logik als pure Funktionen testbar
- **Gates ab Task 1 erweitert:** `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check && cargo audit` vor jedem Commit
- Verifikation headless (xvfb-run, fakesink, Scratch-XDG_DATA_HOME, dbus-run-session für MPRIS); DnD-Gesten sind NICHT headless fahrbar → Drop-/Menü-Handler als testbare Funktionen schneiden + manuelle Nutzer-Checks am Etappenende
- Bestehende Env-Hooks weiterverwenden; neue im selben Stil dokumentieren

---

### Task 1: Auftakt — Controller-Split, Lint-Härtung, Backlog-Einzeiler

**Files:**
- Create: `src/ui/mpris_mirror.rs` (Spiegel-Update + MprisCommand-Handling), `src/ui/playback_faults.rs` (handle_unplayable_track/skip_after_failure/should_stop_skipping)
- Modify: `src/ui/player_controller.rs` (auf < 500 Zeilen schrumpfen; delegiert), `Cargo.toml` (`[lints.clippy]`), `src/ui/rating.rs`/`src/ui/track_list.rs` (Einzeiler), `src/queue.rs` (Guard-Reihenfolge), `src/library/scanner.rs` (`file_stat` → `Option`-Felder)

**Schritte:**
- [ ] **Step 1:** Reiner Umzug (Verhalten identisch): MPRIS-Spiegel + Command-Drain nach `mpris_mirror.rs`, Fehlertoleranz/Skip nach `playback_faults.rs`; `player_controller.rs` behält Player/Queue-Besitz + Event-Drain und delegiert. ALLE 121 Tests bleiben unverändert grün (das ist der Beweis der Verhaltensgleichheit); Modul-Doc-Kommentare (Borrow-Invariante!) wandern mit.
- [ ] **Step 2:** `[lints.clippy]` in Cargo.toml: kuratierte pedantic-Auswahl (mindestens: `needless_pass_by_value`, `redundant_closure_for_method_calls`, `semicolon_if_nothing_returned`, `uninlined_format_args`, `map_unwrap_or`, `unnested_or_patterns` = "warn" — mit `-D warnings` effektiv Fehler); Verstöße im Bestand fixen. `cargo audit` installieren falls fehlt (`cargo install cargo-audit --locked` oder System-Paket prüfen), einmal laufen lassen, Findings dokumentieren (Advisories in Transitiv-Deps: notieren, nicht panisch bumpen).
- [ ] **Step 3 (Backlog-Einzeiler, je mit Mini-Test wo sinnvoll):** (a) Rating-Schreibfehler → Toast (Overlay-Seam existiert); (b) `queue.rs::set_shuffle`: `shuffled = true` erst NACH dem defensiven Guard setzen; (c) Statuszeilen-Test mit filtered_count >= 1000 (Komma-Format); (d) `scanner.rs::file_stat` → `Option<(u64,u64,u64)>`, dev/inode-Schritt bei `None` überspringen (Tests anpassen).
- [ ] **Step 4:** Gates: test/clippy/fmt/audit sauber; ein Standard-Smoke-E2E (Regression). Commit: `refactor: split player controller into mpris mirror and playback faults modules; harden lints`

---

### Task 2: Schema v3 + Playlist-Backend (pur, TDD)

**Files:**
- Create: `src/library/playlists.rs`
- Modify: `src/db.rs` (v2→v3), `src/main.rs` (`mod`-Wiring)

**Interfaces (Task 3/4/6/7 bauen darauf):**
- Schema v3 (idempotent): `playlists(id INTEGER PRIMARY KEY, name TEXT NOT NULL, position INTEGER NOT NULL)`, `playlist_tracks(playlist_id, track_id, position, PRIMARY KEY(playlist_id, position))` mit FK + `ON DELETE CASCADE`, `smart_playlists(id, name, rules_json TEXT NOT NULL, sort_field TEXT NOT NULL, sort_dir TEXT NOT NULL, limit_count INTEGER)`; Seed der drei vordefinierten Smart-Playlists (Recently played / Top rated / Recently added — englische Namen via Konstanten, UI-Strings kommen aus strings.rs beim Anzeigen)
- `playlists::create(conn, name) -> Result<i64>`, `rename(conn, id, name)`, `delete(conn, id)`, `list(conn) -> Vec<PlaylistSummary { id, name, track_count }>`
- `playlists::add_tracks(conn, playlist_id, track_ids: &[i64]) -> Result<u32>` (ans Ende, Positionen fortlaufend; Duplikate erlaubt wie Rhythmbox), `remove_positions(conn, playlist_id, positions: &[u32])`, `move_position(conn, playlist_id, from: u32, to: u32)` (Reorder — Positions-Neuvergabe in einer Transaktion)
- `playlists::smart_rules_to_sql(rules_json) -> Result<(String, Vec<Param>)>` — generisches Regelsystem (Feld/Operator/Wert, UND-verknüpft; Felder whitelisted wie gehabt); die drei Seeds sind normale Regeln (`last_played_at not-null order desc limit 50`, `rating >= 4`, `added_at order desc limit 50` — genaue Regeln im Test festschreiben)

**Schritte:** TDD strikt — Migrationstest (v2-DB → v3, idempotent, Seeds vorhanden), CRUD-Tests, add/remove/move-Positions-Invarianten (lückenlos, stabil bei Duplikaten), rules→SQL-Tests (inkl. Injection-Versuch im Feldnamen → Whitelist-Fallback/Fehler). Commit: `feat: playlists schema v3 with manual playlist CRUD and generic smart-playlist rules`

---

### Task 3: ViewSource-Abstraktion — eine Liste, viele Quellen

**Files:**
- Create: `src/ui/view_source.rs` (`enum ViewSource { Library, Playlist(i64), Smart(i64), Queue, Missing, ImportErrors }`)
- Modify: `src/queries.rs` (Quelle-parametrisierte Fenster/Count/Ids-Queries), `src/ui/track_list_model.rs` (`set_query` bekommt `ViewSource`), `src/ui/track_list.rs`

**Kern:** Die bestehenden Query-Builder bekommen eine Quellen-Klausel (Library: `missing=0`; Playlist: JOIN playlist_tracks ORDER BY position — Playlist-Reihenfolge schlägt Spalten-Sortierung, Spaltenklick sortiert temporär; Smart: rules-SQL; Queue: `id IN (...)` in Queue-Reihenfolge (über Positions-CASE oder temp table — pragmatisch: Reihenfolge im Modell aus der Queue, Fenster aus der id-Liste schneiden); Missing: `missing=1`; ImportErrors: eigene, schmalere Ansicht — Task 8 baut die Spalten). TDD für jede Quelle (Fenster+Count konsistent). Aktivierung setzt die Queue weiterhin aus der AKTUELLEN Quelle (`query_track_ids` quellen-parametrisiert). Commit: `feat: view-source abstraction — track list serves library, playlists, smart lists, and queue`

---

### Task 4: Sidebar — Navigation, Zähler, Problem-Quellen

**Files:**
- Create: `src/ui/sidebar.rs`
- Modify: `src/ui/window.rs` (AdwNavigationSplitView statt ToolbarView-only), `src/ui/strings.rs`

**Kern (Mockup 7a):** Sektionen LIBRARY (Music, Queue mit Zähler), PLAYLISTS (aus `playlists::list`, Track-Zähler, „New playlist"-Zeile → AdwAlertDialog mit Namenseingabe), SMART (drei Seeds), darunter nur-bei-Einträgen: Import errors (Badge = count) und Missing files (Badge). Auswahl → `ViewSource` an die Liste; Titel in der Headerbar folgt der Quelle. Zähler-Refresh nach Scan/Playlist-Änderung (on_reload-Seam erweitern). Headless-E2E: neuer Hook `REPRISE_SMOKE_SOURCE=missing|queue|playlist:<name>` wählt die Quelle programmatisch → Log bestätigt Quellen-Wechsel + Zeilenzahl. Commit: `feat: navigation sidebar with playlists, smart lists, and problem sources`

---

### Task 5: Kontextmenü + Mehrfachauswahl

**Files:**
- Modify: `src/ui/track_list.rs` (`gtk::MultiSelection` statt NoSelection!, GestureClick button=3 → `gtk::PopoverMenu` aus einem `gio::Menu`), `src/ui/strings.rs`, neue Aktions-Schicht `src/ui/track_actions.rs` (testbare Funktionen: selektierte Positionen → ids → Aktion)

**Kern:** Menüpunkte: Play, Add to queue (ans Queue-Ende — `Queue::append_tracks` ergänzen, TDD), Add to playlist → Untermenü (bestehende Playlists + „New playlist…"), bei Playlist-Quelle zusätzlich Remove from playlist. Mehrfachauswahl: Menü wirkt auf ALLE selektierten Zeilen (Rechtsklick auf nicht-selektierte Zeile selektiert sie erst — GNOME-Konvention). Rating-Klick + Doppelklick-Aktivierung dürfen nicht brechen (Selection-Umstellung! Regressions-E2E). Handler-Logik als pure Funktionen mit Tests (Positionen→ids, Ziel-Playlist-Aufruf); das Popover selbst ist manueller Check. **DoD-Kriterium (Nutzer): Playlist übers Kontextmenü befüllbar.** Commit: `feat: context menu with multi-select — play, queue, playlist actions`

---

### Task 6: Drag & Drop — Playlists befüllen, Listen umsortieren

**Files:**
- Modify: `src/ui/track_list.rs` (DragSource auf Zeilen: Content = selektierte Track-Ids als eigenes GType/String-Format), `src/ui/sidebar.rs` (DropTarget auf Playlist-Zeilen), Playlist-/Queue-Ansicht (DropTarget zwischen Zeilen für Reorder)

**Kern:** (a) Titel (Mehrfachauswahl) auf Sidebar-Playlist ziehen → `playlists::add_tracks` + Zähler-Refresh + Toast „N tracks added to <playlist>"; (b) innerhalb Playlist-Ansicht Zeilen umsortieren → `move_position`; (c) Queue-Ansicht umsortieren → `Queue::move_item` (ergänzen, TDD — Semantik: aktueller Titel bleibt aktuell). Drop-Handler als testbare Funktionen (Payload-Parsing, Ziel-Auflösung, Positions-Berechnung) mit Unit-Tests; die Geste selbst = manueller Check. **DoD-Kriterium (Nutzer): Playlist per DnD befüllbar.** Commit: `feat: drag and drop — fill playlists, reorder playlist and queue`

---

### Task 7: M3U-Import/-Export

**Files:**
- Create: `src/library/m3u.rs` (pur: parse/serialize, `#EXTM3U`/`#EXTINF` tolerant, relative+absolute Pfade, UTF-8 mit Latin-1-Fallback-Toleranz beim Lesen)
- Modify: `src/ui/sidebar.rs` (Kontextmenü der Playlist: Export…; globaler „Import playlist…"-Menüpunkt), Datei-Dialoge wie beim Scan

**Kern:** TDD am Parser (Roundtrip, kaputte Zeilen → übersprungen + gezählt, Pfade außerhalb der Bibliothek → Import-Bericht „skipped"); Export schreibt absolute Pfade + EXTINF (Dauer, Artist - Title). Import matched Pfade gegen die DB (exakt; nicht-gefundene → Toast mit Anzahl, kein Auto-Scan — Etappe 4+). Headless-E2E via temporärer M3U-Datei + direktem Funktionsaufruf-Test; Dialog = manuell. Commit: `feat: M3U playlist import and export`

---

### Task 8: Watcher (notify) + Problem-Quellen-Aktionen

**Files:**
- Create: `src/library/watcher.rs`
- Modify: `src/main.rs`/`src/ui/window.rs` (Start nach Scan-Root-Wahl; Persistenz des Roots in `settings`-Tabelle — Mini-Migration v3→v4: `settings(key PRIMARY KEY, value)` + gespeicherter Bibliotheksordner, damit der Watcher beim Start weiß, was er überwachen soll), `src/ui/track_list.rs` (ImportErrors-Quelle: Spalten Pfad/Grund/Zeit + Aktionen Retry/Dismiss), Missing-Quelle: Aktionen „Rescan library"/„Remove from library"

**Kern:** notify-Thread (recommended watcher, recursive) → Debounce 2 s (Ereignisse sammeln, dann EIN inkrementeller Scan des Roots — die bestehende Move-Detection erledigt Verschiebungen; Löschungen: nach dem Scan Pfade unter dem Root prüfen, verschwundene → `missing=1` — neue Funktion `mark_vanished_under_root(conn, root)` mit TDD); Ergebnisse via Channel auf den UI-Thread → Liste/Sidebar-Badges refresh. Ignorierliste-Infrastruktur (`watcher::ignore(path, duration)`) für den Tag-Editor der Etappe 4 vorbereiten (API + Test, noch kein Konsument). Watcher-Ausfall (inotify-Limit) → warn + App läuft (Fehlertoleranz). Headless-E2E: App läuft, Datei ins überwachte Verzeichnis kopieren → Log „watcher scan: added=1", Badge-Refresh; Datei löschen → missing=1. Settings-Migration testet v3→v4. Commit: `feat: folder watcher with debounced incremental rescan and problem-source actions`

---

### Task 9: Shortcuts + Etappen-DoD-E2E

**Files:**
- Modify: `src/ui/window.rs` (gtk::ShortcutController / `gtk::Application::set_accels_for_action`), `src/ui/strings.rs`

**Kern:** Space = Play/Pause (wenn Suchfeld NICHT fokussiert — Key-Handling-Priorität beachten), Ctrl+F = Suchfeld fokussieren, Escape = Suche leeren/Fokus zurück, Enter/Doppelklick = bereits nativ. Aktionen als `gio::SimpleAction`s (Grundlage für spätere Menüs). Abschluss-E2E der Etappe: kombinierter headless Lauf (scan → Playlist per Backend befüllen → Quelle Playlist → Aktivierung spielt in Playlist-Reihenfolge → Watcher-Add → Badge). Commit: `feat: keyboard shortcuts and stage-3 closing verification`

---

### Task 10: MPRIS-Vollausbau — Shuffle, Repeat, Position/Seek (Nutzer-Anforderung 2026-07-11)

> Nutzer zeigt GNOME-Media-Controls-Popup (Shuffle | Prev | Play | Next | Repeat + Positions-Slider): „du hast bisher glaube nur vor und zurück und play und pause. gern erweitern wie hier."

**Files:**
- Modify: `src/mpris.rs`, `src/ui/mpris_mirror.rs`, `src/ui/player_controller.rs` (Seek-Methode + Shuffle/Repeat-Setter für externe Kommandos), `src/player.rs` (nur falls Positions-Abfrage-Seam fehlt)

**Kern:**
- **Position/Seek:** `Position`-Property (µs, read via Spiegel — der 500-ms-Tick aktualisiert ihn bereits), `CanSeek = true`, Methoden `Seek(offset_µs)` und `SetPosition(trackid, position_µs)` (trackid-Abgleich gegen den aktuellen — Mismatch = ignorieren laut Spec), **`Seeked`-Signal** nach jedem erfolgreichen Seek (auch app-internen! Der Slider im Shell-Popup verlässt sich darauf)
- **Shuffle** (b, read/write): read aus Queue-Zustand via Spiegel; write → MprisCommand::SetShuffle(bool) → bestehende Controller-Methode (Bar-ToggleButton muss folgen — programmatischer Set-Guard jetzt nötig, der in E2-Task 4 per YAGNI übersprungen wurde!)
- **LoopStatus** (s, read/write): "None"/"Playlist"/"Track" ↔ Repeat::Off/All/One, beide Richtungen; ungültige Strings → zbus-Fehler, nicht Panik
- **Pflicht-Properties nachziehen** (Backlog aus E2-Final-Review): `Rate`/`MinimumRate`/`MaximumRate` (je 1.0), `Volume` (read/write → set_volume; read aus Spiegel)
- PropertiesChanged für die neuen Properties über den bestehenden Diff-Poll (Position ist von PropertiesChanged AUSGENOMMEN laut MPRIS-Spec — nur Seeked signalisiert Sprünge; Doku-Kommentar!)
- `mpris:artUrl` bleibt Etappe 4 (Cover-Pipeline) — das Platzhalter-Icon im Popup füllt sich dann

**Verifikation (headless, dbus-run-session + busctl):** get Position steigt zwischen zwei Abfragen; `SetPosition` → Seeked-Signal (busctl monitor) + Position springt; Shuffle set true → get true + Queue-Log; LoopStatus "Playlist" → Repeat All im Log; Volume 0.5 → Player-Log. TDD für die reine Mapping-Logik (LoopStatus↔Repeat, µs↔ms). Commit: `feat: full MPRIS surface — position/seek, shuffle, loop status, rate and volume`

---

## Verifikation Etappe 3 (Definition of Done)

- [ ] Gates: `cargo test` grün, clippy `-D warnings` + fmt + `cargo audit` sauber; alle Dateien < 800 Zeilen
- [ ] **Playlist befüllbar über BEIDE Wege** (Nutzer-Kriterium): Kontextmenü (headless: Aktions-Funktion + E2E-Log) UND Drag & Drop (Unit-Tests der Drop-Handler + manueller Check)
- [ ] Headless: Quellen-Wechsel (Library/Playlist/Smart/Queue/Missing/ImportErrors) via REPRISE_SMOKE_SOURCE; Playlist-Reihenfolge beim Abspielen; Smart-Seeds liefern korrekte Treffer; Watcher add/delete/move (Move-Detection greift live); M3U-Roundtrip
- [ ] **Manuell (Nutzer):** DnD-Gesten (Befüllen + Umsortieren), Kontextmenü-Gefühl mit Mehrfachauswahl, Sidebar-Navigation, Space/Ctrl+F, Badges erscheinen/verschwinden

**Nicht in Etappe 3:** Cover, Tag-Editor, Löschen (Papierkorb), Browse-Leiste, Rhythmbox-Import, Erster-Start-Assistent, Session-Restore, Einstellungs-Dialog, EQ/ReplayGain, gettext, Flatpak.
