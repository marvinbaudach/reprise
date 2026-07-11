# Reprise — Etappe 1 (Fortsetzung): GTK4/libadwaita — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Kontext:** Frontend-Pivot 2026-07-11 — natives GTK4 + libadwaita statt Tauri + React (Nutzer-Entscheidung; GNOME-Puristen, keine Glasoptik, nie portieren). Tasks 1–5 des ursprünglichen Plans (`2026-07-11-etappe-1-hoerbarer-kern.md`, dort Tasks 6–9 sind **superseded**) sind umgesetzt; der Rust-Backend-Code (db, models, scanner, Query-Builder, GStreamer-Player-Kern) wird übernommen.

**Goal:** Eine startbare native GTK4/libadwaita-App „Reprise", die einen Musikordner scannt, die Titel in einem sortierbaren `GtkColumnView` zeigt und per Doppelklick über GStreamer abspielt — mit Playerleiste (Play/Pause, Seekbar, Lautstärke, Zeitanzeige) und Statusleiste.

**Tech Stack:** Rust (eine Crate), gtk4-rs + libadwaita-rs (System: GTK 4.22, libadwaita 1.9 — Crate-Features passend wählen, z. B. `v4_18`/`v1_7` oder höher je nach Crate-Release), rusqlite, lofty, walkdir, gstreamer-rs, thiserror, tracing.

**Spec:** `docs/superpowers/specs/2026-07-11-reprise-design.md` (GTK4-Fassung)

## Global Constraints

- Projektpfad `/home/marvin/Projects/reprise`, Branch `main`; Commits direkt auf main
- **Alles Englisch:** Commits (`<type>: <description>`, keine Attribution), Code-Kommentare, Log-/Fehlermeldungen, UI-Strings. UI-Strings zentral in `src/ui/strings.rs` (const &str; gettext folgt später), keine Literale in Widgets
- Fehlertoleranz: kein `unwrap()`/`expect()` außerhalb von Tests und `main()`-Startup; `thiserror` + `Result`; Fehler nie verschlucken (tracing + UI-Meldung); fehlende/defekte Dateien crashen nie
- SQL nur parametrisiert, Sortierfelder per Whitelist (übernommen aus `queries.rs`), Limits gedeckelt
- Logging: tracing bleibt initialisiert (stderr, `REPRISE_LOG`, Default info); UI-Aktionen und Fehler loggen
- DB-Pfad `~/.local/share/reprise/reprise.db`; Tests In-Memory
- **Verifikation headless:** GTK-Starts ausschließlich `xvfb-run -a` (nie auf dem Live-Desktop!); Audio in Tests/Verifikation über `REPRISE_AUDIO_SINK=fakesink` (Env-Override im Player), niemals hörbar
- App-ID exakt `org.reprise.Reprise`; GTK-Main-Thread blockiert nie (Scan in Worker-Thread, Ergebnisse via `glib::MainContext`-Channel bzw. `glib::spawn_future_local`)
- Kein npm/Node mehr im Projekt

---

### Task 6: Restrukturierung — eine native Rust-Crate, Web-Stack raus

**Files:**
- Move: `src-tauri/src/*` → `src/`, `src-tauri/tests/fixtures/` → `tests/fixtures/`, `src-tauri/Cargo.toml` → `Cargo.toml` (angepasst), Cargo.lock neu
- Delete: alles Web/Tauri — `src-tauri/` (Rest: tauri.conf.json, capabilities/, build.rs, icons/), React-`src/` (tsx/ts/css), `index.html`, `package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig*.json`, `node_modules/`, `public/`, `.vscode/` falls Tauri-spezifisch; `Musikplayer.pdf` (Designreferenz obsolet, Nutzer-Freigabe liegt vor)
- Modify: `.gitignore` (neu: `/target`, `.superpowers/`, `.claude/settings.local.json`; npm-Einträge raus — den bereits modifizierten Working-Tree-Stand übernehmen und bereinigen)

**Schritte:**
- [ ] **Step 1:** Dateien verschieben (`git mv`), `Cargo.toml` als Root-Manifest: `name = "reprise"`, `edition = "2021"`, `license = "GPL-3.0-or-later"`, `[lib]`-Sektion und `reprise_scaffold_lib`-Namen entfernen (reines Binary: `src/main.rs`), Tauri-Dependencies raus (`tauri`, `tauri-build`, `tauri-plugin-opener`), `serde` bleibt (models), `serde_json` raus falls ungenutzt nach Step 2, `dirs`, `rusqlite`, `thiserror`, `walkdir`, `lofty`, `gstreamer`, `tracing`, `tracing-subscriber` bleiben
- [ ] **Step 2:** Code entkoppeln: `ipc.rs` → `queries.rs` — `#[tauri::command]`-Wrapper und `AppState`/`State` entfernen, die puren Funktionen (`build_track_query`, `query_track_window`, `query_library_stats(conn)`) samt Tests behalten. `player.rs`: Tauri-`Emitter`/`AppHandle` und die fünf `#[tauri::command]`-Fns entfernen; `Player` bekommt stattdessen einen Event-Callback: `Player::new(on_event: Box<dyn Fn(PlayerEvent) + Send + 'static>)` mit `enum PlayerEvent { StateChanged(PlaybackState), Position { position_ms: i64, duration_ms: i64 }, TrackFinished, Error(String) }`; Bus-Watch und Ticker rufen den Callback. `REPRISE_AUDIO_SINK`-Env-Override: wenn gesetzt, `playbin.set_property("audio-sink", &gst::ElementFactory::make(sink).build()?)`. `lib.rs`-Inhalt → `main.rs`: vorerst nur `init_logging()`, Startup-Banner, DB open/migrate — kein Fenster (kommt in Task 7). `models.rs`, `db.rs`, `library/` unverändert
- [ ] **Step 3:** RED→GREEN: `cargo test` — alle übernommenen Tests (db, scanner ×5, queries ×3, path_to_uri) müssen wieder grün sein; `cargo clippy --all-targets` warnungsfrei; `cargo run` gibt Banner aus und beendet sich
- [ ] **Step 4:** Commit: `refactor: restructure to native single-crate app, drop Tauri/React web stack and design PDF`

---

### Task 7: GTK-Skelett — AdwApplication, Fenster, Headerbar

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/window.rs`, `src/ui/strings.rs`
- Modify: `src/main.rs`, `Cargo.toml` (gtk4-, libadwaita-, glib-Crates)

**Schritte:**
- [ ] **Step 1:** Dependencies: aktuelle `gtk4`- und `libadwaita`-Crates (Features an System 4.22/1.9 anpassen, höchstes vom Crate unterstütztes Feature-Level wählen); `cargo check` muss sauber durchlaufen
- [ ] **Step 2:** `main.rs`: `adw::Application` mit `application_id("org.reprise.Reprise")`, `connect_activate` baut `ui::window::build(app, conn)`; DB-Connection via `Rc<RefCell<rusqlite::Connection>>` (Single-Thread-UI; Scans klonen sich eine eigene Connection über den DB-Pfad)
- [ ] **Step 3:** `window.rs`: `adw::ApplicationWindow` 1280×800 (min. 900×600), `adw::ToolbarView` mit `adw::HeaderBar`: Titel „Reprise", `gtk::SearchEntry` (Platzhalter aus `strings.rs`: "Search all fields"), Button "Scan folder…" (noch ohne Funktion, disabled). `strings.rs`: `pub const APP_NAME`, `SEARCH_PLACEHOLDER`, `SCAN_FOLDER`, … (englisch)
- [ ] **Step 4:** Headless-Verifikation: `xvfb-run -a cargo run` + `glib::timeout_add_seconds_local(3, || app.quit())` hinter Env-Flag `REPRISE_SMOKE_QUIT=1` (kleiner, dauerhafter Testhelfer im Code — dokumentieren); Erwartung: App startet, Log zeigt Fenster-Aufbau, Exit 0 nach 3 s
- [ ] **Step 5:** Commit: `feat: GTK4/libadwaita application skeleton with header bar and search entry`

---

### Task 8: Track-Liste — GtkColumnView mit SQL-Windowing

**Files:**
- Create: `src/ui/track_object.rs` (GObject-Wrapper um `models::Track` — `glib::Object`-Subclass mit Properties oder `glib::BoxedAnyObject`), `src/ui/track_list.rs`
- Modify: `src/ui/window.rs`, `src/ui/strings.rs`

**Schritte:**
- [ ] **Step 1:** `track_list.rs`: `GtkColumnView` mit Spalten Title / Artist / Album / Year / Length / Rating (Header aus `strings.rs`); Daten via `gio::ListStore` (Etappe 1: Fenster von 200 Zeilen wie im React-Plan; echtes lückenloses Windowing = Etappe 2), gefüllt über `queries::query_track_window`; Länge formatiert `mm:ss` bzw. `h:mm:ss` (`format_duration(ms)` in `src/format.rs` — **TDD: Unit-Tests zuerst**, Erwartungen: 181_000→"3:01", 59_000→"0:59", 3_753_000→"1:02:33", -5/NaN-Äquivalent→"0:00"); Rating als "★".repeat(n)-Label (klickbares Widget = Etappe 2)
- [ ] **Step 2:** Sortierung: Klick auf Spaltenkopf toggelt (Feld+Richtung im UI-State, neu laden via Query-Schicht — die Whitelist-Sortierung aus `queries.rs` bleibt die Wahrheit, kein GTK-Sorter über den vollen Datensatz)
- [ ] **Step 3:** Suche: `SearchEntry` → `filter`-Parameter der Query, Debounce 200 ms (`glib::timeout`), Liste neu laden
- [ ] **Step 4:** Doppelklick/Aktivierung (`connect_activate` der Zeile): loggt vorerst `info!("activate {path}")` (Player kommt in Task 9)
- [ ] **Step 5:** Headless-Verifikation: temporären Musikordner mit 2 getaggten Fixture-Kopien scannen (kleines Dev-Subkommando `reprise --scan <dir>` oder Test-Setup), `xvfb-run` Start, Log bestätigt „loaded N tracks"; `cargo test` grün (format-Tests + bestehende)
- [ ] **Step 6:** Commit: `feat: sortable track list (GtkColumnView) with SQL windowing and live search`

---

### Task 9: Player-Anbindung — Playerleiste unten

**Files:**
- Create: `src/ui/player_bar.rs`
- Modify: `src/ui/window.rs` (`ToolbarView.add_bottom_bar`), `src/player.rs` (nur falls Callback-API nachjustiert werden muss), `src/ui/strings.rs`

**Schritte:**
- [ ] **Step 1:** `player_bar.rs`: `gtk::ActionBar` — links Titel/Interpret-Labels, Mitte Play/Pause-`gtk::Button` + `gtk::Scale` (Seekbar) + Zeit-Labels „1:07 / 3:01", rechts `gtk::ScaleButton`/VolumeButton; Strings englisch (aria/tooltip: "Play", "Pause", "Playback position", "Volume")
- [ ] **Step 2:** Player-Integration: `Player` lebt im Fenster-Controller (`Rc`); `PlayerEvent`-Callback marshallt via `glib::MainContext::default().spawn_local`/Channel auf den UI-Thread und aktualisiert Leiste (Zustand, Position/Dauer, Titelwechsel aus der DB per Pfad). Doppelklick in der Liste → `player.play(path)`; Abspielfehler/fehlende Datei → `tracing::error!` + Zustand stopped, App läuft weiter (Fehlertoleranz-Konstante!)
- [ ] **Step 3:** Seek: `Scale`-`change-value` → `player.seek_to(ms)`; Lautstärke → `player.set_volume`; Play/Pause-Button → `toggle_pause`, Icon folgt Zustand
- [ ] **Step 4:** Headless-Verifikation: `REPRISE_AUDIO_SINK=fakesink xvfb-run -a cargo run` mit gescannter Fixture; programmatisch (Smoke-Flag) ersten Titel aktivieren; Log bestätigt state playing → position ticks → track finished (EOS der 1-s-Fixture) — damit ist auch die in Task 5 offene Bus-Watch-Lieferung im echten MainLoop verifiziert
- [ ] **Step 5:** Commit: `feat: bottom player bar with play/pause, seek, time display, and volume`

---

### Task 10: Scan-Flow — Ordnerwahl, Hintergrund-Scan, Statusleiste

**Files:**
- Create: `src/ui/status_bar.rs`
- Modify: `src/ui/window.rs`, `src/library/mod.rs` (falls Scan-Thread-Helfer nötig), `src/format.rs`, `src/ui/strings.rs`

**Schritte:**
- [ ] **Step 1:** **TDD:** `format_total_duration(ms)` in `src/format.rs` — Rhythmbox-Englisch: `"4 days, 6 hours and 28 minutes"`, `"1 hour and 30 minutes"`, `"5 minutes"`, `"2 days and 5 minutes"` (0 h), Unsinn/0 → `"0 minutes"`; Singular/Plural day/hour/minute; letzter Teil mit " and "
- [ ] **Step 2:** Scan-Button aktivieren: `gtk::FileDialog::select_folder` (XDG-Portal-fähig) → Scan in `std::thread` mit eigener DB-Connection (Pfad), Fortschritt/Ergebnis (`ScanReport`) via Channel auf den UI-Thread; Button zeigt „Scanning…" und ist währenddessen disabled; danach Liste + Stats neu laden; Fehler → Log + `adw::Toast`
- [ ] **Step 3:** `status_bar.rs`: schlanke Statuszeile rechtsbündig über der Playerleiste (Design-Mockup 7a): "1,704 tracks · 4 days, 6 hours and 28 minutes" (Mittelpunkt-Trenner; Speichergröße „43,4 GB" folgt in einer späteren Etappe — braucht eine size-Spalte im Schema) aus `queries::query_library_stats` + `format_total_duration`; aktualisiert nach Scan/Filter
- [ ] **Step 4:** Headless-E2E: Fixture-Ordner (2 valide Kopien + 1 defekte Datei) → `xvfb-run` Smoke-Lauf: scan → Log „2 added, 1 error" → Liste 2 Titel → Aktivierung → playing → EOS. `sqlite3`-Check: `import_errors` hat 1 Zeile. `cargo test && cargo clippy --all-targets` grün/sauber
- [ ] **Step 5:** Commit: `feat: scan flow with folder dialog, background scanning, and status bar — stage 1 complete`

---

## Verifikation Etappe 1 (Definition of Done, GTK4-Fassung)

- [ ] `cargo test` grün, `cargo clippy --all-targets` warnungsfrei
- [ ] Headless-E2E aus Task 10 Step 4 bestanden (scan → list → play → EOS, fakesink)
- [ ] Zweiter Scan desselben Ordners inkrementell (skipped, nichts verdoppelt)
- [ ] **Hörprobe durch den Nutzer** (einziger nicht-headless Schritt): `cargo run`, echten Musikordner scannen, Doppelklick → Musik ist hörbar, Seek/Pause/Lautstärke funktionieren

**Nicht in Etappe 1** (unverändert): virtuelles Voll-Windowing, Browse-Leiste, Cover, Sidebar, Playlists, Bewertungen setzen, Warteschlange, MPRIS, Rhythmbox-Import, Tag-Editor, Löschen, Watcher, Einstellungen, EQ/ReplayGain, gettext, Flatpak.
