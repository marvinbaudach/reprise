---
slug: consolidation-implementation
worktree: .worktrees/consolidation
branch: feat/consolidation-wave-0
phase: planned
codex_session:
created: 2026-07-31
base: 577765b
foundation_schema: 51
foundation_ux_section: I, G, K, AF
---
# Implementierungsplan — Konsolidierung und Testfreigabe

Ausführungsdokument zu `docs/plans/architecture-consolidation.md`. Das Review
sagt *was* und *warum*; dieser Plan sagt *in welcher Reihenfolge, mit welchem
roten Test, in welchen Dateien und mit welchem Gate*. Wer nur eine Welle
umsetzt, liest deren Abschnitt plus §2.

Basis `origin/dev` @ `577765b`. Schemastand 50, Ziel dieses Plans: 51.

**Tiefenstaffelung, bewusst:** Welle 0 und 1 sind task-genau ausgeschrieben,
weil sie als Nächstes laufen und die Testfreigabe blockieren. Welle 2 bis 5
sind auf Paketebene mit Datei-Ownership, Abnahmekriterium und Reihenfolge
festgelegt; ihre Task-Zerlegung entsteht beim Paket-Start gegen den dann
aktuellen Stand. Ein Plan, der Arbeit von in drei Monaten zeilengenau
beschreibt, beschreibt Arbeit, die es so nicht geben wird.

---

## 1. Geltungsbereich

| Welle | Inhalt | Blockiert die Testfreigabe? |
| --- | --- | --- |
| 0 | Freigabe-Blocker: Startpfad, Logging, Absturzbericht, MSRV, Auslieferungsumfang | **ja** |
| 1 | Der fehlende Index der Standardsortierung | nein, aber sofort spürbar |
| 2 | Quellen-Grammatik: HTTP-Boundary, Filterleiste, Add-Dialog | nein |
| 3 | Kern-API für die zweite App: `CoreError`, Ports | nein |
| 4 | Runtime-Entscheidung ausführen | nein |
| 5 | FTS und Keyset-Paginierung — erst nach Messung | nein |

Nicht Gegenstand dieses Plans: neue Features, UI-Redesign, das Stem-Feature
über 0.7 hinaus.

---

## 2. Arbeitsweise — verbindlich für jeden Task

Gilt unverändert aus `AGENTS.md`; hier nur die Punkte, an denen dieser Plan
scharf ist.

**Zyklus je Task.** Roten Test schreiben → laufen lassen und **rot sehen** →
minimale Implementierung → grün sehen → volles Gate → ein Commit. Keine
Sammelcommits über Tasks hinweg; Nachbesserungen bekommen eigene Commits.

**Gate je Commit** (aus dem Repo-Root):

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit                       # einzige akzeptierte Advisory: RUSTSEC-2024-0436
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
```

Vor dem PR zusätzlich `scripts/check-merge-readiness.sh`. Display-Tests laufen
einzeln über `scripts/check-display-tests.sh` — die Herde ist flaky, nur
Einzelläufe sind Beleg.

**UX-Regeln.** Jede Änderung an sichtbarem Verhalten braucht eine Regel in
`docs/ux-rules.md`. Neue IDs werden in diesem Plan vorgeschlagen (§9) und
kommen zuerst als `[planned]` in den Abschnitt; der implementierende Commit
flippt sie auf `[active]` **und** bringt den regelbenannten Test mit. Der
Testname muss `fn <prefix>_<nr>_…` lauten (`start_3_…`), sonst findet ihn
`check-ux-traceability.sh` nicht.

**Budgets sind Decke und Boden.** `check-frontend-thinness.sh` steht auf
`rusqlite=112, filesystem=17, threads=14, workers=7`. Ein Task, der eine
Kategorie bewegt, ändert die Zahl **im selben Commit** und begründet sie in der
Commit-Message. Task 0.2 tut das (`filesystem`).

**Neue Stringdateien** müssen in `po/POTFILES.in` stehen —
`check-architecture.sh` prüft das für einen Teil der Dateien namentlich, der
gettext-Check für alle.

**Dateigröße** < 800 Zeilen; `window.rs`, `track_list.rs`, `sidebar.rs` < 600.

**Sprache.** Code, Kommentare, Log-/Fehler-/UI-Strings und Commit-Messages
englisch. Dieses Dokument und `docs/ux-rules.md` deutsch.

---

## 3. Welle 0 — Freigabe-Blocker

Branch `feat/consolidation-wave-0` von `dev`. Ein PR, squashed, zehn Commits
darin. Geschätzt 1–2 Arbeitstage.

**Reihenfolge ist nicht beliebig.** 0.1 und 0.2 legen die Logdatei an, auf die
0.3 schreibt; ohne 0.2/0.3 ist jeder spätere Testbericht blind. Deshalb:

```
0.1 ──► 0.2 ──► 0.3 ──► 0.4
                          └─► 0.5 … 0.10 (frei parallelisierbar)
```

### Task 0.1 — `diagnostics`-Modul: Ort und Rotation der Logdatei

**Ziel.** Ein Ort, an dem Logzeilen und ein Absturzbericht landen, ohne dass
Aufrufer den Pfad kennen.

**Dateien.**
- neu `crates/reprise-gnome/src/ui/diagnostics/mod.rs`
- neu `crates/reprise-gnome/src/ui/diagnostics/paths.rs`
- neu `crates/reprise-gnome/src/ui/diagnostics/paths_tests.rs`
- `crates/reprise-gnome/src/ui/mod.rs` (Modul deklarieren)

**Roter Test** (`paths_tests.rs`, display-frei):

```rust
#[test]
fn log_path_follows_xdg_state_home() { … }
#[test]
fn log_path_falls_back_to_the_data_dir_when_state_home_is_unset() { … }
#[test]
fn rotation_keeps_exactly_one_previous_run() { … }
#[test]
fn rotation_never_fails_the_caller_when_the_directory_is_read_only() { … }
```

**Implementierung.** `log_path()` → `$XDG_STATE_HOME/reprise/reprise.log`,
Fallback `dirs::data_dir()/reprise/`. `rotate_on_start()` verschiebt eine
vorhandene Datei nach `reprise.log.1` und gibt `io::Result<()>` zurück, das der
Aufrufer bewusst verwirft — ein nicht schreibbares Verzeichnis darf den Start
nie verhindern. Größe des laufenden Logs bei 8 MB gedeckelt (danach wird nur
noch verworfen, nicht rotiert — eine zweite Rotation mitten im Lauf würde beim
Auslesen ein halbes Log liefern).

**Kein GTK.** Reines `std::fs` + `dirs`; testbar ohne Display.

**Budget.** Dieser Task bewegt die Kategorie `filesystem` in
`check-frontend-thinness.sh` (heute 17). Die neue Zahl ist der **gemessene**
Wert nach der Änderung, nicht ein geschätzter — das Skript ausführen, den
gemeldeten Ist-Wert eintragen, Begründung in die Commit-Message.

**Commit.** `feat(diagnostics): give the app one log file with a bounded size`

### Task 0.2 — Logging schreibt zusätzlich in die Datei

**Ziel.** Die 793 `tracing`-Aufrufe des GTK-Crates erreichen einen Ort, den ein
Tester ausleiten kann.

**Dateien.** `crates/reprise-gnome/src/main.rs` (`init_logging`),
`crates/reprise-gnome/src/ui/diagnostics/mod.rs`.

**Roter Test.** Die Subscriber-Konstruktion ist schwer zu testen; getestet wird
stattdessen die *Entscheidung*: `diagnostics::file_writer()` liefert
`Option<File>` und ist `None`, wenn das Zielverzeichnis nicht schreibbar ist.

```rust
#[test]
fn file_writer_is_absent_when_the_state_directory_cannot_be_created() { … }
```

**Implementierung.** `tracing_subscriber::registry()` mit zwei Layern: stderr
wie bisher, plus `fmt::layer().with_writer(file).with_ansi(false)`. Filter
bleibt `REPRISE_LOG`, Default `info,lofty=error`. Fehlt die Datei, läuft alles
unverändert nur auf stderr — Logging darf den Start nie scheitern lassen.

**Redaktion.** Der Datei-Layer bekommt dieselbe Regel wie `SourceError`: keine
Pfade aus der Musiksammlung, keine Tokens. Da beides heute schon nicht geloggt
wird, ist das ein Review-Punkt im PR, kein Filter im Code.

**Commit.** `feat(diagnostics): mirror the log to a file testers can send`

### Task 0.3 — Panic-Hook, Absturzmarker, Wiederaufnahme

**Ziel.** Ein Absturz hinterlässt etwas.

**Regel.** `START-4` (neu, `[gtk]`) — siehe §9.

**Dateien.**
- neu `crates/reprise-gnome/src/ui/diagnostics/crash.rs`
- neu `crates/reprise-gnome/src/ui/diagnostics/crash_tests.rs`
- `crates/reprise-gnome/src/main.rs`
- `crates/reprise-gnome/src/ui/strings_app_shell.rs`
- `docs/ux-rules.md` (START-4 `[planned]` → `[active]`)

**Roter Test.**

```rust
#[test]
fn start_4_a_crash_marker_written_by_the_previous_run_is_offered_once() { … }
#[test]
fn start_4_a_clean_shutdown_removes_the_marker() { … }
#[test]
fn crash_report_contains_version_schema_and_the_panic_location() { … }
#[test]
fn crash_report_never_contains_a_library_path() { … }
```

**Implementierung.**
1. `std::panic::set_hook` **vor** allem anderen in `main`: Nachricht, Ort und
   `std::backtrace::Backtrace::force_capture()` in die Logdatei, dann eine
   Markerdatei `$XDG_STATE_HOME/reprise/last-crash` mit Version, Schemastand
   und Zeitstempel. Der Hook setzt seine Backtrace-Erfassung selbst — ein
   Tester setzt nie `RUST_BACKTRACE`.
2. Beim nächsten Start: existiert der Marker, zeigt das Fenster **einen**
   Toast „Reprise wurde beim letzten Mal unerwartet beendet" mit Aktion
   „Diagnose kopieren". Danach wird der Marker gelöscht — genau einmal
   angeboten, nie ein Dauerbanner.
3. Sauberes Beenden löscht den Marker im `close`-Handler.

**Grenze.** Der Hook läuft vor dem Abbruch; ein `abort()` aus C-Code
(GTK-Assertion) erreicht ihn nicht. Das ist bekannt und akzeptiert — die
Rust-seitige Panik ist die häufige Klasse.

**Commit.** `feat(diagnostics): a crash leaves a report and offers it once`

### Task 0.4 — „Diagnose kopieren" im Hauptmenü

**Ziel.** Der Tester kommt ohne Terminal an das Log.

**Regel.** `FB-9` (neu, `[gtk]`).

**Dateien.** `crates/reprise-gnome/src/ui/primary_menu.rs`,
`crates/reprise-gnome/src/ui/diagnostics/report.rs` (neu),
`crates/reprise-gnome/src/ui/strings_app_shell.rs`, `po/POTFILES.in`
(unverändert, Datei ist gelistet), `docs/ux-rules.md`.

**Roter Test.**

```rust
#[test]
fn fb_9_the_report_carries_version_schema_modules_and_the_log_tail() { … }
#[test]
fn fb_9_the_report_is_capped_so_the_clipboard_stays_usable() { … }
#[test]
fn fb_9_the_report_omits_the_library_root_and_track_paths() { … }
```

**Implementierung.** Neue Aktion `ACTION_COPY_DIAGNOSTICS = "copy-diagnostics"`
in der Settings-Sektion des Primärmenüs, neben „Über Reprise". `report::build()`
setzt zusammen: App-Version, Schemastand, GTK-/libadwaita-Version, aktive
Module, Sprache, und die letzten 64 KB der Logdatei. Kopieren über
`gdk::Display::clipboard()`, Bestätigungs-Toast.

**Warum kopieren statt „Ordner öffnen":** unter Flatpak ist ein Dateimanager
nicht garantiert erreichbar, die Zwischenablage schon.

**Commit.** `feat(diagnostics): let a tester copy a diagnostics report`

### Task 0.5 — Der Startpfad scheitert sichtbar statt zu paniken

**Ziel.** Kein `expect` auf dem einzigen Weg in die App.

**Regel.** `START-3` (neu, `[gtk]`).

**Dateien.** `crates/reprise-gnome/src/main.rs`,
neu `crates/reprise-gnome/src/ui/startup_failure.rs`,
neu `crates/reprise-gnome/src/ui/startup_failure_tests.rs`,
`crates/reprise-gnome/src/ui/strings_app_shell.rs`, `docs/ux-rules.md`.

**Roter Test** (reine Abbildung `DbError` → Präsentation, display-frei):

```rust
#[test]
fn start_3_a_newer_schema_names_the_downgrade_and_never_migrates() { … }
#[test]
fn start_3_an_io_failure_names_the_path_and_offers_diagnostics() { … }
#[test]
fn start_3_a_corrupt_database_offers_diagnostics_not_a_repair() { … }
#[test]
fn start_3_the_failure_copy_never_contains_the_technical_cause() { … }
```

**Implementierung.**

```rust
let db = match db::Db::open_migrated(Some(&path)) {
    Ok(db) => db,
    Err(error) => return startup_failure::present(&app, &path, &error),
};
```

**Darstellung — bewusst kein parentloser Dialog.** `AdwAlertDialog` ist die
Hausform (`ui/dialogs.rs`, `issues/missing_dialogs.rs`), braucht aber ein
Elternwidget; ohne Hauptfenster gibt es keins. `present` öffnet deshalb ein
minimales `adw::ApplicationWindow` mit einer `adw::StatusPage`: Icon, Kopfzeile
je Fall, der Datenbankpfad als sekundäre Zeile, und zwei Knöpfe — „Diagnose
kopieren" (Task 0.4) und „Schließen". Das ist zugleich die Form, die `START-2`
für den unerreichbaren Bibliotheksordner bereits vorschreibt, also keine zweite
Sprache für denselben Zustand. Die technische Ursache steht nur im Bericht, nie
auf der Seite — dieselbe Trennung, die `SourceError` schon zieht. Scheitert
schon die GTK-Initialisierung, bleibt `eprintln!` plus Exitcode 1.

**Bewusst nicht:** automatische Reparatur, Umbenennen der Datei, Fallback auf
eine leere Datenbank. Die Datenbank eines Nutzers wird nie ohne Auftrag
angefasst.

**Commit.** `fix(startup): report a database failure instead of panicking`

### Task 0.6 — MSRV ehrlich machen

**Ziel.** Die deklarierte Toolchain ist die, mit der es baut.

**Dateien.** `scripts/tests/msrv.sh`, `Cargo.toml` (`rust-version`),
optional neu `rust-toolchain.toml`, `README.md`.

**Vorgehen — Messung zuerst, Entscheidung danach.**
1. Die tatsächlich nötige Version bestimmen: `cargo build --locked` mit der
   Toolchain aus `org.freedesktop.Sdk.Extension.rust-stable` (GNOME-Runtime 50),
   oder ersatzweise ein `flatpak-builder`-Lauf.
2. **Fall A** — die SDK-Toolchain baut: `rust-version` im Workspace auf diese
   Version heben. `msrv.sh` bekommt einen echten Build
   (`cargo +$expected build --locked --workspace`), nicht nur die
   Metadatenprüfung. Der Fehlertext nennt beide Zahlen.
3. **Fall B** — die SDK-Toolchain baut nicht: `rusqlite`/`libsqlite3-sys` auf
   die letzte Version zurücknehmen, die es tut, `Cargo.lock` und
   `flatpak/cargo-sources.json` neu erzeugen, `scripts/check-release.sh`
   laufen lassen (es vergleicht die Checksummen beider Dateien).

**Roter Test.** `msrv.sh` selbst: vor der Änderung besteht es mit der falschen
Zahl, danach schlägt es fehl, wenn `rust-version` nicht baut.

**Commit.** `fix(build): make the declared MSRV the one that actually builds`

### Task 0.7 — `AGENTS.md` auf den Stand bringen

**Ziel.** Das erste Dokument, das ein Agent liest, beschreibt dieses Projekt.

**Dateien.** `AGENTS.md`.

**Änderungen.**
1. „Three-crate Cargo workspace" → neun Crates mit je einer Zeile Zuständigkeit
   (Tabelle aus `README.md` übernehmen, dort stimmt sie).
2. Roadmap-Abschnitt auf den tatsächlichen Stand (Podcasts, YouTube, Radio,
   Concerts, New Releases, Device-Sync, Library Doctor, My Stats, Stems,
   Runtime) und die offene Runtime-Entscheidung aus Task 0.10.
3. Abschnitt „Not released yet — no backwards compatibility" **ersetzen** durch
   die Stichtagsregel:

   > Ab Schema 50 / Version 0.1.1 existieren Installationen. Migrationen sind
   > vorwärtsgerichtet und verlustfrei. Ein Feld darf entfallen, sobald eine
   > Migration seinen Inhalt überführt hat. Settings-Keys werden migriert, nicht
   > verworfen. Ein sauberes Datenmodell rechtfertigt keinen Datenverlust in
   > einer fremden Bibliothek.

4. Die Baseline-Testzahl („390 passed") auf den aktuellen Stand oder auf
   „siehe letzten Ledger-Eintrag" — eine falsche Zahl ist schlechter als keine.

**Kein Test.** Reine Dokumentation. Das Gate läuft trotzdem vollständig.

**Commit.** `docs: describe the workspace and the compatibility rule as they are`

### Task 0.8 — Auslieferungsumfang: Runtime und Stems

**Ziel.** Was ausgeliefert wird, wird auch benutzt.

**Dateien.** `meson.build`, `data/meson.build`, `meson_options.txt`,
`org.reprise.Reprise.yml`, `scripts/check-runtime-service-install.sh`,
`scripts/check-release.sh`.

**Implementierung.**
1. Neue Meson-Option `runtime_service` (Default `false`). Das
   `reprise-runtime`-Target und beide `.service`-Dateien hängen daran.
   `check-runtime-service-install.sh` prüft nur noch, *wenn* die Option an ist —
   und prüft dann unverändert streng.
2. `stem_backend` für die Testrunde auf `false`; `check-release.sh` überspringt
   `check-stem-runtime-packaging.sh` konsequent, wenn die Option aus ist. Damit
   ist der rote Release-Check (§9.4 des Reviews) kein Blocker mehr, ohne dass
   jemand ihn stillgelegt hat.
3. Die Crates bleiben im Workspace und werden weiter gebaut und getestet — nur
   installiert werden sie nicht.

**Roter Test.** `check-runtime-service-install.sh` mit `-Druntime_service=false`
darf nicht fehlschlagen und mit `-Druntime_service=true` muss es weiterhin
beide Präfixe prüfen.

**Commit.** `build: ship only what a surface uses (runtime service, stems opt-in)`

### Task 0.9 — MCP standardmäßig aus, Capabilities sichtbar

**Ziel.** Kein Agentenzugriff, den niemand eingeschaltet hat.

**Dateien.** `crates/reprise-gnome/src/ui/preferences/` (Plugins-Seite),
`crates/reprise-core/src/modules.rs`, `docs/ux-rules.md` (Abschnitt T,
Netzwerk-Opt-in).

**Implementierung.** Ein Modul-Deskriptor für die Agenten-Oberfläche mit
`default_enabled: false`, und eine Zeile auf der Plugins-Seite, die die aktuell
erteilten Capabilities benennt (lesen / Mixplanung / Playlist-Erzeugung — die
drei aus `CONTEXT.md`). Keine neue Mechanik, nur Sichtbarkeit.

**Prüfen vor der Umsetzung:** ob `reprise-mcp` heute überhaupt ohne
Nutzerhandlung erreichbar ist. Ist es das nicht (der Server wird extern
gestartet), schrumpft der Task auf die Sichtbarkeitszeile.

**Commit.** `feat(preferences): name the agent capabilities that are granted`

### Task 0.10 — Runtime-Entscheidung festschreiben

**Ziel.** Der Schwebezustand endet mit einer Entscheidung, nicht mit einem
Vergessen.

**Dateien.** `docs/plans/architecture-consolidation.md` (§2.2 um die
Entscheidung ergänzen), `docs/adr/003-runtime-ownership.md` (neu).

**Inhalt des ADR.** Kontext (Zahlen aus dem Review), Entscheidung (A Cutover /
B zurückstellen), Konsequenzen, und bei B: das Auslösekriterium für die
Wiederaufnahme — etwa „sobald ein zweites Frontend beginnt" oder „sobald ein
Agent Playback ohne laufendes Fenster steuern soll". Ohne benanntes Kriterium
wird aus „zurückgestellt" stillschweigend „aufgegeben", und dann liegen 15.000
Zeilen ohne Besitzer im Repo.

**Empfehlung des Plans:** B für die Testrunde, mit Kriterium.

**Commit.** `docs: record the runtime ownership decision as an ADR`

### Abnahme Welle 0

- Ein absichtlich ausgelöster Panik-Pfad hinterlässt Logzeile, Marker und wird
  beim nächsten Start genau einmal angeboten.
- Eine Datenbank mit `user_version = 99` erzeugt einen Dialog, keinen Absturz.
- „Diagnose kopieren" liefert einen Bericht ohne Bibliothekspfade.
- `scripts/tests/msrv.sh` schlägt fehl, wenn man `rust-version` künstlich
  senkt.
- `meson setup` ohne Optionen installiert weder `reprise-runtime` noch die
  `.service`-Dateien.
- Volles Gate grün, `check-merge-readiness.sh` grün.

---

## 4. Welle 1 — Der fehlende Index

Branch `feat/library-sort-index` von `dev`. Ein Task, ein Commit.

### Task 1.1 — Migration 51: Index für die Standardsortierung

**Ziel.** Die meistbenutzte Abfrage der App wird index-bedient.

**Dateien.**
- neu `crates/reprise-core/src/db_sort_indexes.rs`
- `crates/reprise-core/src/db.rs` (`SUPPORTED_SCHEMA_VERSION` 50 → 51, Aufruf
  am Ende von `migrate_with_cache_dirs`)
- `crates/reprise-core/src/lib.rs` (`mod db_sort_indexes;`)

**Vorbild.** `db_recently_added.rs::migrate_v35` — gleiche Form: Versionsprüfung,
`unchecked_transaction`, `execute_batch`, `pragma_update`, `commit`.

**Roter Test.**

```rust
#[test]
fn v51_serves_the_default_artist_sort_from_an_index() {
    let db = Db::open_in_memory().unwrap();
    // EXPLAIN QUERY PLAN über clauses::build_track_query("artist", "ASC", false)
    // darf kein "USE TEMP B-TREE FOR ORDER BY" enthalten.
}
#[test]
fn v51_is_idempotent_and_bumps_the_schema_version() { … }
```

Der Plan-Test ist der Kern: er prüft nicht eine Laufzeit (die schwankt), sondern
dass SQLite den Index *wählt*. Das ist deterministisch und bleibt es.

**Implementierung.**

```sql
CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
WHERE missing_since IS NULL AND removed_at IS NULL;
```

Die Spaltenfolge muss `SORT_WHITELIST["artist"]` exakt entsprechen. Ändert
jemand das Sortiertupel, fällt der Test — genau richtig.

**Belegen, nicht behaupten.** Vor und nach dem Commit
`scripts/performance-query-compare.sh` für 10k und 100k laufen lassen und beide
Berichte in die Commit-Message aufnehmen. Die Messung im Review (0,4 / 1,95 /
3,37 ms gegen 14,9 / 312 / 380 ms) stammt aus einem Replikat; die Zahlen dieses
Laufs sind die echten.

**Bewusst nur ein Index.** `added_at` ist der zweite Kandidat, aber jeder Index
kostet Schreiblast beim Scan. Erst messen, dann entscheiden — als eigener Task
1.2, nur wenn der Vergleich es trägt.

**Commit.** `perf(db): serve the default library sort from an index`

### Task 1.2 (bedingt) — `PRAGMA optimize` nach großen Scans

Nur wenn 1.1 zeigt, dass der Planer daneben liegt. `PRAGMA optimize` am Ende
von `scanner::scan_folder`, nach dem Commit, Fehler geloggt und verworfen.
Roter Test: nach einem Scan liegen Statistiken in `sqlite_stat1` vor.

### Abnahme Welle 1

Der Plan-Test ist grün, die beiden Performance-Berichte liegen bei, und die
Scroll-Zeit im 100k-Profil ist im `performance-baseline.sh`-Lauf messbar
gefallen.

---

## 5. Welle 2 — Quellen-Grammatik konsolidieren

Branch je Paket von `dev`, in dieser Reihenfolge. **Paket 2.1 zuerst** — es
schafft die Stelle, an der die Sicherheits- und Richtlinienpunkte einmal statt
sechzehnmal stehen.

### Paket 2.1 — `reprise_core::net`: eine HTTP-Boundary

**Owner-Dateien.** neu `crates/reprise-core/src/net/{mod,client,rate,breaker,fixtures}.rs`;
umgestellt: `podcasts/http.rs`, `radio/http.rs`, `concerts/http.rs`,
`musicbrainz.rs`, `artist_portrait/deezer.rs`, `lyrics/lrclib.rs`,
`lyrics/netease.rs`, `podcasts/source_artwork.rs`, `cover_download.rs`.
**Ausgenommen:** `scrobbling*` und `library/listenbrainz.rs` (eigene
Auth-Signatur, eigener Rhythmus) — erst nach dem Rest, wenn überhaupt.

**Inhalt.**
- `SourceClient { agent, user_agent, timeout }` als einziger Ort, an dem ein
  `ureq::Agent` entsteht.
- **Ein** Ratenbegrenzer, **pro Host** geschlüsselt statt pro Modul. Heute sind
  es fünf getrennte `static LAST_REQUEST`-Mutexe ohne gemeinsames Budget.
- Der Circuit Breaker wird **nicht neu geschrieben**: `lyrics/breaker.rs`
  (`#189`) ist bereits host-geschlüsselt und richtig geformt — er wird nach
  `net/breaker.rs` gehoben und alle Quellen schließen sich an.
- `SourceTransportError` als gemeinsamer Rückgabetyp; die Domänen-Enums
  (`PodcastError`, `RadioError`, `ProviderError`, …) verlieren ihre HTTP-Arme
  und behalten nur die fachlichen.
- **Eine** Fixture-Variable `REPRISE_HTTP_FIXTURE_DIR` mit Unterordner je
  Provider, statt fünf getrennter.
- Weiterleitungsziele prüfen: Loopback, Link-Local und private Bereiche werden
  abgelehnt und als `Unreachable` gemeldet (Review §7.4).

**Roter Test.** Zuerst die Richtlinientests gegen die neue Boundary —
Ratenbudget, Breaker-Öffnung, abgelehnte Weiterleitung, Größenlimit —, dann je
umgestellte Quelle deren bestehende Tests unverändert grün.

**Migrationsschnitt.** Quelle für Quelle, jede in eigenem Commit. Der alte Pfad
wird im selben Commit gelöscht, in dem der neue greift — zwei Boundaries
nebeneinander wären genau der Zustand, den dieses Paket beseitigt.

**Abnahme.** `rg -c 'ureq::Agent::config_builder' crates/reprise-core/src` steht
bei 1 (plus die ausgenommenen Scrobbling-Pfade), eine neue Gate-Zeile deckelt
die Zahl (§7 des Reviews, Gate-Vorschlag 4).

### Paket 2.2 — Eine Filterleiste

**Owner-Dateien.** `ui/browse/*` (generische Leiste),
`ui/podcasts/podcasts_filter_bar.rs`, `ui/radio/radio_filter_bar.rs`,
`ui/releases/releases_filter_bar.rs`, `ui/concerts/concerts_filter_bar.rs`.

**Inhalt.** `FilterBar<F: FilterModel>` besitzt Geometrie, Chip-Aufbau,
Popover-Navigation (Facetten- und Werteseite), „Alle löschen" und die Zählzeile.
Je Quelle bleibt ein `FilterModel`-Impl: Facetten, Labels, Werte, Persistenz-Key
— erwartet 60–120 statt 300–570 Zeilen.

Die duplizierten Konstanten (`FILTER_BAR_MIN_HEIGHT` 5×, `FACET_PAGE`/
`VALUE_PAGE` 3×) verschwinden dabei; die Gate-Zeile aus §12 des Reviews hält
sie fern.

**Wichtig.** Abschnitt K von `docs/ux-rules.md` gilt danach zum ersten Mal für
alle Quellen — bisher erreicht er nur `browse_bar`. Jede K-Regel, die für eine
Quelle nicht gelten soll, braucht dort eine ausdrückliche Ausnahme, keine
stillschweigende.

**Vorbild im Repo.** `#193` hat mit `ui/source_reveal.rs` genau diesen Schnitt
schon vorgemacht: geteilte Entscheidung, quellenspezifische Ausführung.

**Abnahme.** Netto-Reduktion messbar; die K-Regeln haben Tests, die über alle
vier Quellen laufen.

### Paket 2.3 — Ein Add-Dialog

**Owner-Dateien.** neu `ui/source_add_dialog.rs`; umgestellt
`ui/podcasts/add_dialog*.rs`, `ui/radio/add_dialog.rs`,
`ui/radio/radio_add_input.rs`.

**Inhalt.** Die Phasenmaschine (`Idle → Searching → Results → Previewing →
Preview → Error`), der Generationszähler und die Ergebnisliste wandern in den
gemeinsamen Dialog. Je Quelle ein Trait mit `classify_input`, `search`,
`preview`, `commit` und den Copy-Identitäten.

**Netz.** Beide Dialoge haben eigene Tests (`add_dialog_tests.rs` je Quelle) —
die bleiben und werden zum Beweis, dass die Zusammenlegung nichts verändert hat.

### Paket 2.4 — Kleine Schulden derselben Familie

Je ein Commit, unabhängig voneinander:

| # | Inhalt | Review |
| --- | --- | --- |
| 2.4a | `has_place_pill()` / `has_sidebar_row()` zu einer Funktion mit zwei Aufrufern | §5.3.3 |
| 2.4b | `youtube_channel_detail` gegen FIL-1c prüfen und, falls nötig, angleichen | §5.3.1 |
| 2.4c | `--` vor jedes yt-dlp-Positionsargument, Debug-Assertion auf `http(s)://` | §7.2 |
| 2.4d | `image::Limits` (Kantenlänge, `max_alloc`) an jeder Dekodierstelle | §7.6 |
| 2.4e | `recv_or_fault` in `one_shot_task`; `delete_tracks.rs` als erster Aufrufer | §8.3 |
| 2.4f | `--cookies-from-browser`: Copy im Plugin-Bereich, Env-Override nur in Debug-Builds | §7.3 |

2.4a und 2.4c sind je unter einer Stunde und schließen je einen Befund
vollständig — gute Einstiegstasks.

---

## 6. Welle 3 — Kern-API für die zweite App

Erst nach Welle 2, weil 2.1 den Fehlertyp der Netzschicht bereits neu schneidet
und beide sonst dieselben Signaturen anfassen.

### Paket 3.1 — `CoreError`

858 öffentliche Signaturen geben heute `Result<_, rusqlite::Error>` zurück.
Ziel: `reprise_core::CoreError` mit `NotFound`, `Conflict`, `Busy`, `Invalid`,
`Backend(String)`; `rusqlite::Error` wird `#[from]` gefaltet und nie
durchgereicht.

**Schnitt.** Modul für Modul, `From`-Impl trägt die Zwischenstände, jede Etappe
kompiliert. Reihenfolge nach Aufrufhäufigkeit: `queries/` zuerst (die breiteste
Fläche), dann `library/`, dann der Rest.

**`Busy` ist kein Kosmetikfall.** `reprise-cli` inspiziert heute
Busy-/Lock-Codes direkt — deshalb ist das eine eigene Variante und kein
`Backend(String)`.

### Paket 3.2 — `rusqlite` aus den headless-Oberflächen entfernen

Möglich, sobald 3.1 steht. Danach eine Gate-Zeile in
`check-architecture.sh`, die `rusqlite` in `reprise-cli`/`reprise-mcp` verbietet
— aus einem Kommentar wird eine Prüfung.

### Paket 3.3 — Parameterobjekte

`query_track_window` existiert in vier Überladungen mit bis zu elf Parametern;
`queries/mod.rs` trägt allein sieben `#[allow(clippy::too_many_arguments)]`. Ein
`TrackWindowQuery { source, sort, filter, browse, window, queue_items, ai }`
ersetzt sie. Rein mechanisch, hoher Lesbarkeitsgewinn, kein Verhaltensrisiko.

### Paket 3.4 — Ansichts-Ports statt `RuntimeWiring`

`RuntimeWiring` hat über 40 Felder und kennt jede Ansicht. Ziel: je Ansicht ein
schmales `…Ports`-Struct mit genau ihren Kollaborateuren; `RuntimeWiring` baut
diese Ports und übergibt sie, die Ansicht kennt `RuntimeWiring` nicht mehr.

**Inkrementell, eine Ansicht je Commit.** Beginnen mit einer kleinen
(`ConcertsView` oder `ReleasesView`), damit die Form sich an einem billigen Fall
bewährt, bevor `TrackList` folgt.

---

## 7. Welle 4 — Runtime

Nur wenn ADR 003 (Task 0.10) auf **Cutover** entschieden hat. Pakete analog zu
„episodes as queue citizens":

1. Ports verdrahten (GStreamer-Backend und Linux-Device-Effekte an
   `runtime::ports`).
2. `PlayerController` liest Snapshots statt eigenen Zustand.
3. Queue-Kommandos gehen an den Runtime; `queue_transport`/`up_next_transport`
   werden Projektionen.
4. MPRIS-Spiegel wird vom Runtime gespeist.
5. MCP/CLI von MPRIS auf `org.reprise.Reprise1` umstellen.
6. `transport_parity_tests` von einem Netz zu einem Vertrag befördern und die
   GTK-seitige Kopie löschen.
7. Meson-Option aus Task 0.8 auf Default `true`.

Jedes Paket eigener PR. Schritt 6 ist die Stelle, an der die Doppelung wirklich
verschwindet — vorher ist es eine Umleitung, keine Konsolidierung.

---

## 8. Welle 5 — nur nach Messung

Nach Welle 1 neu bewerten, nicht vorher:

- **FTS5** über `(title, artist, album, genre)`, contentless, per Trigger
  gepflegt. In `rusqlite`s `bundled` enthalten, also keine neue Abhängigkeit.
  Zwischenschritt, der vielleicht schon reicht: die Gesamtzahl oberhalb einer
  Schwelle nicht mehr exakt zählen, sondern `LIMIT`-basiert „mehr als N".
- **Keyset-Paginierung** statt `OFFSET`. Braucht stabile Tiebreaker in
  `SORT_WHITELIST` — ein größerer Eingriff, der sich nur lohnt, wenn Bibliotheken
  jenseits von 100k real vorkommen.

---

## 9. Neue und geänderte UX-Regeln

Als `[planned]` anlegen, im implementierenden Commit auf `[active]` flippen,
zusammen mit dem regelbenannten Test. IDs sind append-only.

| ID | Abschnitt | Level | Inhalt | Task |
| --- | --- | --- | --- | --- |
| `START-3` | I. Start state | `[gtk]` | Ein Datenbankfehler beim Start zeigt einen benannten Dialog mit Pfad und „Diagnose kopieren", nie eine Panik. Die technische Ursache erscheint nur im Bericht. | 0.5 |
| `START-4` | I. Start state | `[gtk]` | Nach einem Absturz bietet der nächste Start genau einmal an, die Diagnose zu kopieren; sauberes Beenden löscht den Marker. | 0.3 |
| `FB-9` | G. Feedback | `[gtk]` | „Diagnose kopieren" liefert Version, Schemastand, aktive Module und den Log-Auszug — gedeckelt und ohne Bibliothekspfade. | 0.4 |

`START-1` und `START-2` bleiben `[planned]`; dieser Plan rührt sie nicht an.

Abschnitt K wird durch Paket 2.2 zum ersten Mal für alle Quellen wirksam. Wo
eine K-Regel für Podcasts, Radio, Releases oder Concerts nicht gelten soll,
gehört die Ausnahme dort hinein — der Plan erwartet ein bis drei solcher
Ausnahmen und keine stille Abweichung.

---

## 10. Gate-Ergänzungen als eigene Tasks

Jede Zeile macht einen Befund unwiederholbar. Sie landen **mit** dem Task, der
den Befund schließt, nicht als Sammelcommit am Ende.

| Gate | Prüft | Mit Task |
| --- | --- | --- |
| `msrv.sh` baut wirklich | die deklarierte Toolchain | 0.6 |
| kein `expect`/`unwrap` in `main.rs` | Startpfad | 0.5 |
| `ureq`-Agenten-Budget in `reprise-core` | HTTP-Boundary, nur senkbar | 2.1 |
| yt-dlp-Positionsargumente hinter `--` | Argument-Injektion | 2.4c |
| duplizierte UI-Konstanten genau einmal definiert | Filterleisten | 2.2 |
| eindeutige Sektionsbuchstaben in `ux-rules.md` | zwei Abschnitte „T", kein „AC" | 0.7 |
| `rusqlite` verboten in `cli`/`mcp` | Kern-API | 3.2 |
| Runtime nur installiert, wenn benutzt | Auslieferungsumfang | 0.8 |
| `cargo deny` im Release-Gate | Lizenzen, Duplikate | eigener Task, Welle 2 |

---

## 11. Risiken und Abbruchkriterien

- **Task 0.6 kann eine Abhängigkeitsrücknahme erzwingen.** Fall B in 0.6 rührt
  `Cargo.lock` und `flatpak/cargo-sources.json` an — beides prüft
  `check-release.sh` per Checksummenvergleich. Wenn das Zurücknehmen mehr als
  `rusqlite` betrifft, ist es ein eigener PR, nicht ein Task in Welle 0.
- **Paket 2.1 ist das größte Einzelrisiko dieses Plans.** Neun Quellen mit je
  eigenen Fehlerpfaden und Fixtures. Deshalb: eine Quelle je Commit, alte
  Boundary im selben Commit löschen, und Scrobbling ausgenommen.
- **Paket 2.2 berührt sichtbares Verhalten in vier Ansichten.** Die
  Display-Tests sind herdenflaky; nur Einzelläufe zählen als Beleg. Zeit dafür
  einplanen.
- **Welle 3 fasst 858 Signaturen an.** Rein mechanisch, aber der PR wird groß.
  Modulweise Commits sind hier keine Stilfrage, sondern die Bedingung dafür,
  dass ihn jemand lesen kann.
- **Abbruchkriterium für Welle 4:** wenn ADR 003 auf B entschieden hat, wird
  Welle 4 nicht „vorsichtshalber angefangen". Ein halber Cutover ist schlechter
  als beide Enden.

---

## 12. Abnahmekriterien je Welle

**Welle 0.** Ein absichtlich ausgelöster Panikpfad hinterlässt Logzeile, Marker
und genau ein Angebot beim nächsten Start. `user_version = 99` erzeugt einen
Dialog. Der Diagnosebericht enthält keinen Bibliothekspfad. `msrv.sh` fällt bei
künstlich gesenkter `rust-version`. `meson setup` ohne Optionen installiert
weder Runtime noch `.service`-Dateien. Vollständiges Gate grün.

**Welle 1.** Der Planer wählt den neuen Index; beide Performance-Berichte liegen
der Commit-Message bei.

**Welle 2.** Eine `ureq`-Agenten-Konstruktion im Kern (plus Scrobbling), eine
Filterleistenimplementierung, ein Add-Dialog, ein `has_sidebar_row`. Alle
bestehenden Quellen-Tests unverändert grün — die Zusammenlegung darf kein
Verhalten ändern.

**Welle 3.** `rusqlite` steht in keinem `Cargo.toml` von `reprise-cli` und
`reprise-mcp`; das Gate prüft es. Kein
`#[allow(clippy::too_many_arguments)]` mehr in `queries/mod.rs`.

**Welle 4.** `crates/reprise-gnome/src/ui/playback/queue_transport.rs` und
`up_next_transport.rs` enthalten keine Queue-Semantik mehr, nur noch
Projektion. `transport_parity_tests` ist zum Vertragstest geworden.

**Welle 5.** Erst planen, wenn die Messung aus Welle 1 vorliegt.
