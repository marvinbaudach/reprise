---
slug: consolidation-wave-0-runbook
worktree: .worktrees/consolidation-wave-0
branch: feat/consolidation-wave-0
phase: runbook
codex_session:
created: 2026-07-31
base: 577765b
foundation_schema: 51
foundation_ux_section: I, G
---
# Runbook — Welle 0 und 1, Schritt für Schritt

Dies ist das Dokument, das eine Session **von oben nach unten abarbeitet**. Es
setzt `AGENTS.md` voraus und wiederholt daraus nur, was beim wörtlichen
Befolgen sonst schiefgeht.

- *Warum* diese Arbeit: `docs/plans/architecture-consolidation.md`
- *Welche Wellen in welcher Reihenfolge*: `docs/plans/consolidation-implementation.md`
- *Wie Welle 0 und 1 konkret gebaut werden*: dieses Dokument

**Lebensdauer.** Ein Runbook stirbt mit seiner Welle. `AGENTS.md` hält
Stage-Pläne bewusst aus dem Repo heraus; diese Datei ist die Ausnahme, weil sie
an eine andere Maschine übergeben wird. **Sie wird gelöscht, sobald Welle 0 und
1 in `dev` sind** — im selben Commit, der die letzte Ledger-Zeile schreibt. Die
beiden anderen Dokumente bleiben.

---

## 0. Preflight — bevor eine Zeile Code entsteht

### 0.1 Sicherheitsregeln, die auf dieser Maschine scharf sind

Auf dem Zielrechner liegen echte Daten. Diese vier Regeln sind nicht
verhandelbar und der häufigste Weg, sie zu brechen, ist ein wörtlich befolgtes
Kommando ohne sie.

1. **Die echte Datenbank ist tabu.** `~/.local/share/reprise/reprise.db`
   (1686 Titel), Bibliothekswurzel `/home/marvin/Music`. Nicht scannen, nicht
   mutieren, kein Werkzeug darauf richten.
2. **Jeder App-Start ist vollständig isoliert.** Jede Run-/Smoke-Kommandozeile
   muss **alle** diese Teile enthalten:

   ```bash
   dbus-run-session -- xvfb-run -a env \
     XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
     XDG_STATE_HOME=$(mktemp -d) \
     GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
     cargo run
   ```

   `XDG_STATE_HOME` ist in Welle 0 **neu und load-bearing**: Task 0.1 legt dort
   die Logdatei an. Ohne die Variable schreibt ein Testlauf in das echte
   `~/.local/state/reprise/`. Vor jedem Lauf das eigene Kommando nach
   `XDG_DATA_HOME` **und** `XDG_STATE_HOME` durchsuchen.
3. **Nie unter `/tmp` klonen oder bauen.** `/tmp` ist ein 16 G tmpfs; ein
   `target/` dort lebt im RAM. Worktree nach `.worktrees/consolidation-wave-0`.
   Die kleinen `$(mktemp -d)` aus der Isolationszeile oben sind ausdrücklich in
   Ordnung.
4. **Kein `CARGO_TARGET_DIR` teilen.** Cargo nimmt eine exklusive Sperre; ein
   geteiltes Zielverzeichnis serialisiert parallele Agenten.

### 0.2 Basis herstellen

```bash
cd ~/Projects/reprise
git fetch origin dev
git worktree add .worktrees/consolidation-wave-0 -b feat/consolidation-wave-0 origin/dev
cd .worktrees/consolidation-wave-0
git log --oneline -1        # muss 577765b oder neuer sein
```

Ist `dev` weitergelaufen, ist das in Ordnung — dann gegen den neuen Stand
arbeiten und die Abweichungen zu diesem Runbook beim ersten Task notieren.

### 0.3 Basis-Gate: erst beweisen, dass grün grün ist

Bevor irgendetwas geändert wird, einmal das volle Gate auf der **unveränderten**
Basis laufen lassen und das Ergebnis notieren:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
```

Das ist keine Formalie. Der Ledger dokumentiert mehrere Fälle, in denen ein
Gate auf der unveränderten Basis rot war und die Rotheit fälschlich der eigenen
Arbeit zugeschrieben wurde. **Die Testzahl aus diesem Lauf notieren** — jeder
Task unten sagt, wie sie sich ändern soll.

Bekannt und akzeptiert:
- `cargo audit`: einzig RUSTSEC-2024-0436 (`paste`, über `lofty`). Eine **neue**
  Advisory heißt STOP.
- `scripts/check-stem-runtime-packaging.sh` ist auf der Basis rot (fehlende
  ONNX-Marker in `build-aux/meson-cargo-build.sh`). Es gehört nicht zum
  Merge-Gate; Task 0.8 macht es sauber abschaltbar.

### 0.4 Reihenfolge

```
0.1 diagnostics ──► 0.2 Logdatei ──► 0.3 Panic-Hook ──► 0.4 Diagnose kopieren ──► 0.5 Startpfad
                                                                                     │
0.6 MSRV · 0.7 AGENTS.md · 0.8 Auslieferung · 0.9 MCP · 0.10 ADR  ◄──────────────────┘ (frei)
1.1 Index (eigener Branch, unabhängig von Welle 0)
```

0.1–0.5 sind eine Kette: jeder Task benutzt den vorigen. 0.6–0.10 sind
unabhängig und können in beliebiger Reihenfolge oder parallel laufen. Task 1.1
hängt an nichts aus Welle 0 und kann zuerst gemacht werden, wenn ein schneller
sichtbarer Gewinn gewünscht ist.

### 0.5 Zyklus je Task — ohne Ausnahme

1. Roten Test schreiben.
2. `cargo test -p <crate> <testname>` → **rot sehen**. Nicht überspringen; ein
   Test, der nie rot war, beweist nichts.
3. Minimale Implementierung.
4. Denselben Test → grün.
5. Volles Gate (§0.3).
6. **Ein** Commit mit dem angegebenen Titel.
7. Eine Zeile an `.superpowers/sdd/progress.md` anhängen:
   `Task N: complete (commit <hash>, base <hash>, <ein Satz>)`.

---

## Task 0.1 — `diagnostics`: Ort und Rotation der Logdatei

**Ziel.** Ein Ort für Logzeilen und Absturzberichte, den Aufrufer nicht kennen
müssen. Kein GTK, damit es ohne Display testbar ist.

### Dateien

| Datei | Aktion |
| --- | --- |
| `crates/reprise-gnome/src/ui/diagnostics/mod.rs` | neu |
| `crates/reprise-gnome/src/ui/diagnostics/paths.rs` | neu |
| `crates/reprise-gnome/src/ui/diagnostics/paths_tests.rs` | neu |
| `crates/reprise-gnome/src/ui/mod.rs` | `mod diagnostics;` alphabetisch einsortieren (zwischen `device_sync` und `dialogs`) |
| `scripts/check-frontend-thinness.sh` | Budget `filesystem` auf den gemessenen Wert |

### API, die dieser Task festlegt

```rust
// paths.rs
/// `$XDG_STATE_HOME/reprise/reprise.log`, or the data dir when the state
/// home is unset. Never fails: a caller that cannot write simply logs to
/// stderr alone.
pub(in crate::ui) fn log_path() -> Option<PathBuf>;

/// The previous run's log, kept as exactly one generation.
pub(in crate::ui) fn previous_log_path() -> Option<PathBuf>;

/// `$XDG_STATE_HOME/reprise/last-crash`, written by the panic hook.
pub(in crate::ui) fn crash_marker_path() -> Option<PathBuf>;

/// Moves an existing log aside so a run starts on a clean file. Returns the
/// io error for logging; callers deliberately ignore it — a read-only state
/// directory must never keep the app from starting.
pub(in crate::ui) fn rotate_on_start() -> io::Result<()>;

/// Bytes the running log may reach before further writes are dropped.
pub(in crate::ui) const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
```

### Roter Test — `paths_tests.rs`

Alle vier ohne Display, mit `tempfile::TempDir` und gesetztem `XDG_STATE_HOME`.

```rust
#[test] fn log_path_follows_xdg_state_home()
#[test] fn log_path_falls_back_to_the_data_dir_when_state_home_is_unset()
#[test] fn rotation_keeps_exactly_one_previous_run()
#[test] fn rotation_reports_but_survives_a_read_only_directory()
```

**Achtung Umgebungsvariablen in Tests.** `std::env::set_var` ist seit Rust 2024
`unsafe` und die Tests laufen parallel im selben Prozess. Zwei Wege, beide
akzeptabel — den zweiten bevorzugen:

1. Ein `Mutex`-serialisierter Test-Guard, der die Variable setzt und
   zurücksetzt.
2. **Besser:** die Pfadlogik gegen ein injiziertes Wurzelverzeichnis testen —
   `fn log_path_in(root: &Path) -> PathBuf` als reine Funktion, und
   `log_path()` liest die Umgebung genau einmal und ruft sie auf. Dann braucht
   kein Test eine Umgebungsvariable.

### Warum eine Deckelung statt fortlaufender Rotation

Bei 8 MB werden weitere Schreibvorgänge verworfen, es wird **nicht** ein zweites
Mal rotiert. Eine Rotation mitten im Lauf würde beim späteren Auslesen ein
halbes Log liefern, und genau das Log will man beim Absturz vollständig haben.

### Budget

`scripts/check-frontend-thinness.sh` ausführen, den gemeldeten Ist-Wert für
`filesystem` eintragen (heute 17), Begründung in die Commit-Message. **Nicht
raten** — das Skript nennt die Zahl.

### Commit

```
feat(diagnostics): give the app one log file with a bounded size
```

---

## Task 0.2 — Das Logging schreibt zusätzlich in die Datei

**Ziel.** Die 793 `tracing`-Aufrufe des GTK-Crates erreichen einen Ort, den ein
Tester verschicken kann.

### Dateien

| Datei | Aktion |
| --- | --- |
| `crates/reprise-gnome/src/main.rs` | `init_logging` erweitern |
| `crates/reprise-gnome/src/ui/diagnostics/mod.rs` | `file_writer()` |
| `crates/reprise-gnome/src/ui/diagnostics/paths_tests.rs` | ein Test dazu |

### Ausgangszustand (`main.rs`, heute)

```rust
fn init_logging() {
    let filter = EnvFilter::try_from_env("REPRISE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,lofty=error"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
```

### Zielzustand

`tracing_subscriber::registry()` mit zwei Layern statt `fmt()`: stderr wie
bisher, plus ein Datei-Layer mit `.with_ansi(false)`. Filter und Default bleiben
wortgleich. Fehlt die Datei, läuft alles unverändert nur auf stderr — Logging
darf den Start **nie** scheitern lassen.

`init_logging` läuft weiterhin als erste Anweisung in `main`, vor `i18n::init`.
Die Rotation aus 0.1 passiert darin, vor dem ersten Schreiben.

### Roter Test

Die Subscriber-Konstruktion ist schlecht testbar; getestet wird die
Entscheidung davor:

```rust
#[test] fn file_writer_is_absent_when_the_state_directory_cannot_be_created()
```

### Review-Punkt statt Codefilter

Der Datei-Layer bekommt keine Redaktionsschicht. Pfade aus der Musiksammlung
und Tokens werden heute nicht geloggt; das ist im PR zu prüfen, nicht zur
Laufzeit zu filtern. Ein Filter, der Freitext scannt, wiegt in falscher
Sicherheit.

### Commit

```
feat(diagnostics): mirror the log to a file testers can send
```

---

## Task 0.3 — Panic-Hook, Absturzmarker, einmaliges Angebot

**Ziel.** Ein Absturz hinterlässt etwas. Heute: nichts.

**Regel.** `START-4`, neu. Exakter Text in §9 unten — **englisch**, siehe dort.

### Dateien

| Datei | Aktion |
| --- | --- |
| `crates/reprise-gnome/src/ui/diagnostics/crash.rs` | neu |
| `crates/reprise-gnome/src/ui/diagnostics/crash_tests.rs` | neu |
| `crates/reprise-gnome/src/main.rs` | Hook als allererste Anweisung |
| `crates/reprise-gnome/src/ui/window/window.rs` | Marker beim Start prüfen, beim Schließen löschen |
| `crates/reprise-gnome/src/ui/strings_app_shell.rs` | zwei Konstanten |
| `docs/ux-rules.md` | START-4 hinzufügen, `[active]` |

### Implementierung

**1. Der Hook, vor allem anderen in `main`:**

```rust
fn main() -> glib::ExitCode {
    diagnostics::crash::install_hook();   // ERSTE Anweisung
    init_logging();
    …
```

`install_hook` setzt `std::panic::set_hook`. Der Hook schreibt Nachricht, Ort
und `std::backtrace::Backtrace::force_capture()` in die Logdatei und legt dann
`crash_marker_path()` an, mit Version, Schemastand und Zeitstempel.

`force_capture` ist bewusst: ein Tester setzt nie `RUST_BACKTRACE`, und ein
Absturzbericht ohne Backtrace ist ein Absturzbericht ohne Inhalt.

**2. Beim nächsten Start**, in `window::build` nach dem Aufbau: existiert der
Marker, genau **ein** Toast mit Aktion „Diagnose kopieren" (Task 0.4), danach
Marker löschen. Kein Banner, keine Wiederholung.

**3. Sauberes Beenden** löscht den Marker im `close-request`-Handler.

### Roter Test — `crash_tests.rs`

```rust
#[test] fn start_4_a_crash_marker_written_by_the_previous_run_is_offered_once()
#[test] fn start_4_a_clean_shutdown_removes_the_marker()
#[test] fn crash_report_contains_version_schema_and_the_panic_location()
#[test] fn crash_report_never_contains_a_library_path()
```

Die ersten beiden tragen die Regel-ID und müssen sie tragen — sonst findet
`check-ux-traceability.sh` sie nicht (`fn start_4_…`, snake_case, mit
`#[test]` in den fünf Zeilen darüber).

Getestet wird die **Zustandsmaschine** (Marker vorhanden → Angebot → Marker
weg), nicht ein echter Prozessabbruch. Der Hook selbst wird über
`crash::write_report(&PanicInfoLike, &path)` getestet, mit einer synthetischen
Eingabe.

### Grenze, die im Code stehen muss

Ein `abort()` aus C-Code (GTK-Assertion, Wayland-Protokollfehler) erreicht den
Rust-Hook nicht. Das ist bekannt und akzeptiert — die Rust-seitige Panik über
`RefCell` ist die häufige Klasse. Als Kommentar in `crash.rs`, damit niemand
später glaubt, es sei lückenlos.

### Commit

```
feat(diagnostics): a crash leaves a report and offers it once
```

---

## Task 0.4 — „Diagnose kopieren" im Hauptmenü

**Ziel.** Der Tester kommt ohne Terminal an das Log.

**Regel.** `FB-9`, neu. Text in §9.

### Dateien

| Datei | Aktion |
| --- | --- |
| `crates/reprise-gnome/src/ui/diagnostics/report.rs` | neu |
| `crates/reprise-gnome/src/ui/diagnostics/report_tests.rs` | neu |
| `crates/reprise-gnome/src/ui/primary_menu.rs` | Aktion + Menüeintrag |
| `crates/reprise-gnome/src/ui/strings_app_shell.rs` | Konstanten |
| `docs/ux-rules.md` | FB-9 |

### Menüeintrag — exakte Stelle

`primary_menu.rs` hat drei Sektionen (View, Library, Settings). Der Eintrag
gehört in die Settings-Sektion, **nach** „Help" und **vor** „About Reprise":

```rust
fn settings_section_entries() -> Vec<(String, &'static str)> {
    vec![
        (strings::text(strings::PREFERENCES), "win.preferences"),
        (strings::text(strings::KEYBOARD_SHORTCUTS), "win.keyboard-shortcuts"),
        (strings::text(strings::HELP), "win.help"),
        (strings::text(strings::COPY_DIAGNOSTICS), "win.copy-diagnostics"),   // neu
        (strings::text(strings::ABOUT_REPRISE), "win.about"),
    ]
}
```

Dazu `pub(super) const ACTION_COPY_DIAGNOSTICS: &str = "copy-diagnostics";` bei
den anderen Aktionskonstanten, und die `gio::SimpleAction` in `install`
registrieren — dem Muster von `ACTION_ABOUT` folgen, inklusive
`window.downgrade()`.

### Strings

```rust
// strings_app_shell.rs, bei "Primary menu items."
pub const COPY_DIAGNOSTICS: &str = N_!("Copy Diagnostics");
pub const DIAGNOSTICS_COPIED: &str = N_!("Diagnostics copied to the clipboard");
pub const CRASH_LAST_RUN: &str = N_!("Reprise closed unexpectedly last time");
```

`strings_app_shell.rs` steht bereits in `po/POTFILES.in` — nichts zu tun. Eine
**neue** Stringdatei müsste dort ergänzt werden, und `check-architecture.sh`
prüft das für vier Dateien namentlich.

### Berichtsinhalt

App-Version, Schemastand, GTK-/libadwaita-Version, aktive Module, Sprache,
und die letzten 64 KB der Logdatei. Nichts aus der Bibliothek.

### Roter Test — `report_tests.rs`

```rust
#[test] fn fb_9_the_report_carries_version_schema_modules_and_the_log_tail()
#[test] fn fb_9_the_report_is_capped_so_the_clipboard_stays_usable()
#[test] fn fb_9_the_report_omits_the_library_root_and_track_paths()
```

`report::build()` nimmt seine Eingaben als Parameter (Version, Schemastand,
Modulliste, Logpfad) statt sie selbst zu holen — dann sind alle drei Tests
display- und umgebungsfrei.

### Kopieren statt „Ordner öffnen"

Unter Flatpak ist ein Dateimanager nicht garantiert erreichbar, die
Zwischenablage schon. Über `gdk::Display` → `clipboard()`, danach ein Toast
über `toasts::show`.

### Commit

```
feat(diagnostics): let a tester copy a diagnostics report
```

---

## Task 0.5 — Der Startpfad scheitert sichtbar statt zu paniken

**Ziel.** Kein `expect` auf dem einzigen Weg in die App.

**Regel.** `START-3`, neu. Text in §9.

### Dateien

| Datei | Aktion |
| --- | --- |
| `crates/reprise-gnome/src/main.rs` | `expect` ersetzen |
| `crates/reprise-gnome/src/ui/startup_failure.rs` | neu |
| `crates/reprise-gnome/src/ui/startup_failure_tests.rs` | neu |
| `crates/reprise-gnome/src/ui/strings_app_shell.rs` | Kopfzeilen je Fall |
| `docs/ux-rules.md` | START-3 |
| `scripts/check-architecture.sh` | Verbot von `expect`/`unwrap` in `main.rs` |

### Ausgangszustand

```rust
let conn = db::Db::open_migrated(Some(&path)).expect("failed to open or migrate database");
```

### Zielzustand

```rust
let db = match db::Db::open_migrated(Some(&path)) {
    Ok(db) => db,
    Err(error) => return startup_failure::present(&app, &path, &error),
};
```

### Darstellung — bewusst kein parentloser Dialog

`AdwAlertDialog` ist die Hausform (`ui/dialogs.rs`,
`ui/issues/missing_dialogs.rs`), braucht aber ein Elternwidget; ohne
Hauptfenster gibt es keins. `present` öffnet deshalb ein minimales
`adw::ApplicationWindow` mit einer `adw::StatusPage`:

- Icon `dialog-error-symbolic`
- Kopfzeile je `DbError`-Fall
- Datenbankpfad als sekundäre Zeile
- Zwei Knöpfe: „Diagnose kopieren" (0.4) und „Schließen"

Das ist dieselbe Form, die `START-2` für den unerreichbaren Bibliotheksordner
bereits vorschreibt — keine zweite Sprache für denselben Zustand. Scheitert
schon die GTK-Initialisierung, bleibt `eprintln!` plus Exitcode 1.

### Die vier Fälle

| `DbError` | Kopfzeile (sinngemäß) | Realistisch weil |
| --- | --- | --- |
| `SchemaTooNew` | Bibliothek stammt aus einer neueren Version | Tester probiert einen neueren Build und geht zurück |
| `SchemaNotReady` | Bibliothek ist nicht bereit | sollte nicht vorkommen, wird aber benannt statt verschluckt |
| `Io` | Bibliothek kann nicht geöffnet werden | Platte voll, Home auf NFS, Rechte |
| `Sqlite` | Bibliotheksdatei ist beschädigt | hartes Ausschalten |

Die technische Ursache steht **nur** im Bericht, nie auf der Seite — dieselbe
Trennung, die `SourceError` schon zieht und mit Tests absichert.

### Bewusst nicht

Keine automatische Reparatur, kein Umbenennen der Datei, kein Fallback auf eine
leere Datenbank. Die Datenbank eines Nutzers wird nie ohne Auftrag angefasst.

### Roter Test — `startup_failure_tests.rs`

Reine Abbildung `DbError` → Präsentation, display-frei:

```rust
#[test] fn start_3_a_newer_schema_names_the_downgrade_and_never_migrates()
#[test] fn start_3_an_io_failure_names_the_path_and_offers_diagnostics()
#[test] fn start_3_a_corrupt_database_offers_diagnostics_not_a_repair()
#[test] fn start_3_the_failure_copy_never_contains_the_technical_cause()
```

### Manuelle Verifikation — geht nur auf deiner Maschine

```bash
scratch=$(mktemp -d ~/.cache/reprise-scratch/startup.XXXXXX)
mkdir -p "$scratch/data/reprise"
sqlite3 "$scratch/data/reprise/reprise.db" "PRAGMA user_version = 99;"
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$scratch/data" XDG_CACHE_HOME=$(mktemp -d) \
  XDG_STATE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  cargo run
```

Erwartung: StatusPage statt Panik, Prozess endet mit 1, das Log enthält eine
`error!`-Zeile mit dem Fall.

### Gate-Zeile im selben Commit

In `scripts/check-architecture.sh`, bei den übrigen Frontend-Verboten:

```bash
if rg --quiet '\.expect\(|\.unwrap\(\)' crates/reprise-gnome/src/main.rs; then
  echo "the startup path must report failures, not panic on them" >&2
  exit 1
fi
```

### Commit

```
fix(startup): report a database failure instead of panicking
```

---

## Task 0.6 — MSRV ehrlich machen

**Ziel.** Die deklarierte Toolchain ist die, mit der es baut.

### Der Befund, den es zu schließen gilt

Jedes Manifest sagt `rust-version = "1.92"`. `cargo build -p reprise-core --locked`
scheitert reproduzierbar auf rustc 1.94.1:

```
Compiling libsqlite3-sys v0.38.1
error[E0658]: use of unstable library feature `cfg_select`
  --> libsqlite3-sys-0.38.1/build.rs:110:9
```

`scripts/tests/msrv.sh` fängt das nicht, weil es nur `cargo metadata` liest und
prüft, dass das **Feld** überall `1.92` sagt. Es baut nie.

### Schritt 1 — messen, nicht raten

```bash
rustc --version                            # was die Maschine hat
cargo build --locked --workspace 2>&1 | tail -20
```

Und mit der Toolchain, die das Flatpak benutzt
(`org.freedesktop.Sdk.Extension.rust-stable` unter GNOME-Runtime 50):

```bash
flatpak run --command=sh --devel org.gnome.Sdk//50 -c \
  '/usr/lib/sdk/rust-stable/bin/rustc --version'
```

**Diese zweite Zahl entscheidet.** `org.reprise.Reprise.yml` baut mit
`CARGO_NET_OFFLINE=true` gegen `flatpak/cargo-sources.json`, also exakt gegen
den gepinnten Baum.

### Schritt 2 — Fall A: die SDK-Toolchain baut

- `rust-version` in `[workspace.package]` auf die gemessene Version heben.
- `scripts/tests/msrv.sh` bekommt einen echten Build:

  ```bash
  rustup toolchain install "$expected_msrv" --profile minimal
  cargo "+$expected_msrv" build --locked --workspace
  ```

  Die bestehende Metadatenprüfung bleibt — sie fängt ein Manifest, das die
  Zahl nicht mitzieht.
- `README.md`-Zeile zur Rust-Version anpassen, falls vorhanden.

### Schritt 2' — Fall B: die SDK-Toolchain baut nicht

- `rusqlite`/`libsqlite3-sys` auf die letzte Version zurücknehmen, die mit ihr
  baut.
- `cargo update -p rusqlite --precise <version>`, dann `Cargo.lock` prüfen.
- `flatpak/cargo-sources.json` neu erzeugen (das Werkzeug dafür steht in
  `RELEASING.md`).
- `scripts/check-release.sh` laufen lassen — es vergleicht die Checksummen aus
  `Cargo.lock` und `cargo-sources.json` und schlägt fehl, wenn sie
  auseinanderlaufen.

**Abbruchkriterium.** Betrifft die Rücknahme mehr als `rusqlite`, ist es ein
eigener PR und nicht ein Task in Welle 0. Dann Welle 0 ohne 0.6 abschließen und
0.6 als eigenen Branch führen.

### Optional, empfohlen

`rust-toolchain.toml` mit der gemessenen Version, damit Entwickler und CI
dieselbe Toolchain sehen. Prüfen, ob das mit dem Flatpak-Build kollidiert (die
SDK-Extension bringt ihre eigene mit).

### Commit

```
fix(build): make the declared MSRV the one that actually builds
```

---

## Task 0.7 — `AGENTS.md` auf den Stand bringen

**Ziel.** Das erste Dokument, das ein Agent liest, beschreibt dieses Projekt.

### Vier Korrekturen — alle geprüft

1. **„Three-crate Cargo workspace"** → es sind neun. Die Tabelle aus
   `README.md` übernehmen, dort stimmt sie.
2. **Roadmap** endet bei „GUI-A2 (Cover-Download)". Tatsächlich gelandet:
   Podcasts, YouTube, Radio, Concerts, New Releases, Device-Sync, Library
   Doctor, My Stats, Tag-Editor, Stems, Runtime. Dazu die offene
   Runtime-Entscheidung aus Task 0.10 verlinken.
3. **„`docs/ux-rules.md` is the single UX source of truth (German)"** — das
   Dokument ist **englisch**. Stichprobe über alle Abschnitte: FIL, PLAY, NAV,
   SET, STATS, RUN, FX sind durchweg englische Regeltexte. Das ist die
   gefährlichste der vier Falschaussagen, weil sie eine Session dazu bringt,
   eine neue Regel auf Deutsch zu schreiben und damit den Stil zu brechen.
   Ersetzen durch: englische Regeltexte, deutsche Planungsdokumente.
4. **„Not released yet — no backwards compatibility"** ersetzen durch die
   Stichtagsregel:

   > **Released to testers — compatibility starts here.** Ab Schema 50 /
   > Version 0.1.1 existieren Installationen. Migrationen sind
   > vorwärtsgerichtet und verlustfrei. Ein Feld darf entfallen, sobald eine
   > Migration seinen Inhalt überführt hat; Settings-Keys werden migriert,
   > nicht verworfen. Ein sauberes Datenmodell rechtfertigt keinen
   > Datenverlust in einer fremden Bibliothek.

   **Diesen Punkt erst mit der tatsächlichen Freigabe setzen**, nicht vorher —
   solange niemand installiert hat, ist die alte Regel noch richtig und
   nützlich.

5. Die Baseline-Testzahl („390 passed; 1 ignored") ist um Größenordnungen
   veraltet. Entweder auf den gemessenen Stand oder auf „siehe letzten
   Ledger-Eintrag" — eine falsche Zahl ist schlechter als keine.

### Kein Test

Reine Dokumentation. Das volle Gate läuft trotzdem.

### Commit

```
docs: describe the workspace and the compatibility rule as they are
```

---

## Task 0.8 — Auslieferungsumfang: Runtime und Stems

**Ziel.** Was ausgeliefert wird, wird auch benutzt.

### Dateien

`meson.build`, `data/meson.build`, `meson_options.txt`,
`scripts/check-runtime-service-install.sh`, `scripts/check-release.sh`.

### Implementierung

1. Neue Option in `meson_options.txt`:

   ```meson
   option('runtime_service', type: 'boolean', value: false,
          description: 'Install the headless runtime service (no surface uses it yet)')
   ```

2. Das `reprise-runtime`-Target in `meson.build` und beide `.service`-Dateien
   in `data/meson.build` hinter `get_option('runtime_service')`.

3. `scripts/check-runtime-service-install.sh` prüft nur noch, **wenn** die
   Option an ist — und dann unverändert streng in beiden Präfixen.

4. `stem_backend` für die Testrunde auf `false`; `check-release.sh` überspringt
   `check-stem-runtime-packaging.sh` konsequent, wenn die Option aus ist. Damit
   ist der rote Release-Check kein Blocker mehr, **ohne** dass jemand ihn
   stillgelegt hat.

Die Crates bleiben im Workspace und werden weiter gebaut und getestet — nur
installiert werden sie nicht.

### Verifikation

```bash
meson setup build-off . --prefix="$HOME/.local"
meson introspect build-off --installed | grep -c reprise-runtime   # 0

meson setup build-on . --prefix="$HOME/.local" -Druntime_service=true
scripts/check-runtime-service-install.sh                            # grün
```

### Commit

```
build: ship only what a surface uses (runtime service, stems opt-in)
```

---

## Task 0.9 — MCP standardmäßig aus, Capabilities sichtbar

**Ziel.** Kein Agentenzugriff, den niemand eingeschaltet hat.

### Erst prüfen, dann bauen

Vor der Umsetzung klären, ob `reprise-mcp` überhaupt ohne Nutzerhandlung
erreichbar ist. Der Server ist ein eigenes Binary, das ein MCP-Client startet —
wenn er nur extern gestartet wird, gibt es nichts abzuschalten und der Task
schrumpft auf die Sichtbarkeitszeile. **Diesen Befund im Commit festhalten**,
damit die Frage nicht ein drittes Mal gestellt wird.

### Umfang bei „schrumpft"

Eine Zeile auf der Plugins-Seite, die die erteilten Capability-Klassen benennt
(lesen / Mixplanung / Playlist-Erzeugung — die drei aus `CONTEXT.md`). Keine
neue Mechanik, nur Sichtbarkeit.

### Umfang bei „ist erreichbar"

Zusätzlich ein Modul-Deskriptor in `crates/reprise-core/src/modules.rs` mit
`default_enabled: false`, dem Muster der übrigen `ONLINE_MODULES` folgend, und
das Gate in `capability.rs` daran hängen.

### Commit

```
feat(preferences): name the agent capabilities that are granted
```

---

## Task 0.10 — Runtime-Entscheidung als ADR

**Ziel.** Der Schwebezustand endet mit einer Entscheidung, nicht mit einem
Vergessen.

### Dateien

`docs/adr/003-runtime-ownership.md` (neu),
`docs/plans/architecture-consolidation.md` (§2.2 um die getroffene Entscheidung
ergänzen).

### Aufbau, dem Muster von ADR 002 folgend

- **Status** — angenommen am, mit Datum.
- **Context** — die Zahlen aus dem Review: ~15.400 Zeilen über fünf Orte, GTK
  ist Client von nichts, MCP/CLI gehen über MPRIS, zwei Kommandoflächen für
  eine Fachlichkeit, ein D-Bus-Dienst geht mit aus, den nichts benutzt.
- **Decision** — A (Cutover) oder B (zurückstellen). Der Plan empfiehlt **B für
  die Testrunde**.
- **Consequences** — bei B: die Crates bleiben gebaut und getestet, werden aber
  nicht installiert (Task 0.8); die Paritätstests bleiben als Beleg, dass die
  Runtime das Verhalten trifft.
- **Auslösekriterium für die Wiederaufnahme** — der wichtigste Abschnitt.
  Zum Beispiel: „sobald ein zweites Frontend beginnt" oder „sobald ein Agent
  Playback ohne laufendes Fenster steuern soll". Ohne benanntes Kriterium wird
  aus „zurückgestellt" stillschweigend „aufgegeben", und dann liegen 15.000
  Zeilen ohne Besitzer im Repo.

### Commit

```
docs: record the runtime ownership decision as an ADR
```

---

## Abnahme Welle 0

Alles davon muss stimmen, bevor der PR aufgemacht wird:

- [ ] Ein absichtlich ausgelöster Panik-Pfad hinterlässt eine Logzeile mit
      Backtrace, einen Marker, und wird beim nächsten Start **genau einmal**
      angeboten.
- [ ] `PRAGMA user_version = 99` erzeugt eine StatusPage, keine Panik
      (Kommando in Task 0.5).
- [ ] „Diagnose kopieren" liefert einen Bericht ohne Bibliothekspfade.
- [ ] `scripts/tests/msrv.sh` schlägt fehl, wenn man `rust-version` künstlich
      senkt.
- [ ] `meson setup` ohne Optionen installiert weder `reprise-runtime` noch die
      `.service`-Dateien.
- [ ] `scripts/check-ux-traceability.sh` zählt drei aktive Regeln mehr.
- [ ] Volles Gate grün, `scripts/check-merge-readiness.sh` grün.
- [ ] Display-Tests: die Herde über `scripts/check-display-tests.sh --rule-named`
      (das Skript nimmt `--rule-named | --motion | --css`, **keinen**
      Testnamen). Reißt einer, ihn **einzeln** nachfahren — nur der Einzellauf
      ist Beleg:

      ```bash
      env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
          XDG_CONFIG_HOME=$(mktemp -d) XDG_STATE_HOME=$(mktemp -d) \
          GIO_USE_VFS=local GTK_USE_PORTAL=0 \
          GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
        dbus-run-session -- xvfb-run -a \
          cargo test -p reprise-gnome <testname> -- --ignored --exact
      ```

---

## Task 1.1 — Migration 51: Index für die Standardsortierung

Eigener Branch `feat/library-sort-index` von `dev`, unabhängig von Welle 0.

### Dateien

| Datei | Aktion |
| --- | --- |
| `crates/reprise-core/src/db_sort_indexes.rs` | neu |
| `crates/reprise-core/src/lib.rs` | `mod db_sort_indexes;` bei den übrigen `db_*` |
| `crates/reprise-core/src/db.rs` | `SUPPORTED_SCHEMA_VERSION` 50 → 51, Aufruf ans Ende von `migrate_with_cache_dirs` |

### Vorbild im Repo

`crates/reprise-core/src/db_recently_added.rs::migrate_v35` — gleiche Form:
Versionsprüfung, `unchecked_transaction`, `execute_batch`, `pragma_update`,
`commit`. Diese Datei als Schablone nehmen, nicht neu erfinden.

### Der Index

```sql
CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
WHERE missing_since IS NULL AND removed_at IS NULL;
```

Die Spaltenfolge muss `SORT_WHITELIST["artist"]` aus
`crates/reprise-core/src/queries/clauses.rs` **exakt** entsprechen — heute
`artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no`. Der `WHERE`-Teil
muss `clauses::PRESENT` entsprechen.

### Roter Test

```rust
#[test] fn v51_serves_the_default_artist_sort_from_an_index()
#[test] fn v51_is_idempotent_and_bumps_the_schema_version()
```

Der erste ist der Kern. Er prüft **nicht** eine Laufzeit — die schwankt je
Maschine — sondern dass SQLite den Index **wählt**:

```rust
let db = Db::open_in_memory().unwrap();
// `clauses` ist ein privates Modul; der Builder ist als
// `queries::build_track_query` re-exportiert (queries/mod.rs:119).
let sql = crate::queries::build_track_query("artist", "ASC", false);
// EXPLAIN QUERY PLAN über sql; die Ausgabe darf
// "USE TEMP B-TREE FOR ORDER BY" nicht enthalten und muss
// "idx_tracks_present_artist_order" nennen.
```

Der Builder erwartet zwei gebundene Parameter (`LIMIT ?1 OFFSET ?2`) — beim
`EXPLAIN QUERY PLAN` mitgeben, sonst schlägt das Prepare fehl.

Das ist deterministisch und bleibt es. Ändert jemand das Sortiertupel, ohne den
Index mitzuziehen, fällt genau dieser Test — was gewollt ist.

**Statistiken.** Bei leerer Tabelle kann der Planer den Index verschmähen. Den
Test mit ein paar hundert Zeilen füllen und `ANALYZE` laufen lassen, oder das
Fixture aus `queries/tests.rs` benutzen. Wenn der Test bei leerer Tabelle grün
ist: gut, dann ist er noch strenger als nötig.

### Belegen, nicht behaupten

`performance-query-compare.sh` vergleicht **zwei Verzeichnisse**, die
`performance-baseline.sh` erzeugt — es misst nicht selbst. Der Ablauf ist
deshalb dreiteilig, und der Baum muss für beide Läufe sauber sein (das Skript
schreibt den Commit in sein Manifest):

```bash
# 1. auf der unveränderten Basis, VOR der Migration
git stash -u                                    # falls nötig
scripts/performance-baseline.sh ~/perf/before

# 2. mit dem Index, nach dem Commit
scripts/performance-baseline.sh ~/perf/after

# 3. Vergleich
scripts/performance-query-compare.sh ~/perf/before ~/perf/after
```

Negative Zeitdeltas sind Verbesserungen, positive Datenbank-Byte-Deltas sind
der Preis des Index. `--quick` misst nur 10k — für diesen Task ist der
100k-Lauf der interessante, also **ohne** `--quick`.
**Das Vergleichs-JSON in die Commit-Message.** Die Zahlen aus dem
Review (0,44 / 1,95 / 3,37 ms gegen 14,9 / 312 / 380 ms) stammen aus einem
Replikat mit synthetischen Daten in Python-SQLite; die Zahlen dieses Laufs sind
die echten. Wenn sie deutlich abweichen, ist **die Messung** die Wahrheit und
das Review die Schätzung.

### Bewusst nur ein Index

`added_at` ist der nächste Kandidat, aber jeder Index kostet Schreiblast beim
Scan. Erst messen, dann entscheiden — als eigener Task 1.2, nur wenn der
Vergleich es trägt.

### Commit

```
perf(db): serve the default library sort from an index
```

---

## 9. Die drei neuen UX-Regeln — Text zum Übernehmen

**Englisch**, wie der Rest des Dokuments. `AGENTS.md` behauptet Deutsch; das ist
falsch (Task 0.7, Korrektur 3). Format exakt einhalten, sonst findet
`check-ux-traceability.sh` sie nicht: `- **ID** [status] [level] — text`.

Jede Regel kommt als `[active]` in **denselben** Commit wie ihr Test — nie
nachträglich, nie ohne Test.

### Abschnitt I. Start state — nach START-2 anfügen

```markdown
- **START-3** [active] [gtk] — A database that cannot be opened is
  reported, never a panic. The startup path presents a StatusPage
  naming the case — library from a newer version, library not ready,
  library cannot be opened, library file damaged — with the database
  path as a secondary line and two actions: copy diagnostics (FB-9)
  and close. The technical cause appears only in the diagnostics
  report, never on the page, the same separation SourceError draws.
  Reprise never repairs, renames or replaces the file on its own: a
  library it cannot read is still the user's library.
- **START-4** [active] [gtk] — A run that ended in a panic leaves a
  report in the log and a marker. The next start offers to copy the
  diagnostics exactly once — a toast with one action, not a banner —
  and clears the marker whether or not the offer was taken. A clean
  shutdown clears the marker too, so the offer only ever follows an
  actual crash.
```

### Abschnitt G. Feedback vocabulary — nach FB-8 anfügen

```markdown
- **FB-9** [active] [gtk] — "Copy Diagnostics" in the primary menu puts
  one self-contained report on the clipboard: app version, schema
  version, toolkit versions, enabled modules, interface language, and
  the tail of the log. The report is capped so the clipboard stays
  usable, and it never carries the library root or any track path —
  what is wrong with Reprise is diagnosable without shipping what the
  user listens to.
```

### Testnamen, die diese Regeln greenen

| Regel | Test |
| --- | --- |
| `START-3` | `fn start_3_a_newer_schema_names_the_downgrade_and_never_migrates()` u. a. |
| `START-4` | `fn start_4_a_crash_marker_written_by_the_previous_run_is_offered_once()` |
| `FB-9` | `fn fb_9_the_report_carries_version_schema_modules_and_the_log_tail()` |

Der Gate sucht `fn <prefix>_<nr>_` mit einem `#[test]` in den fünf Zeilen
darüber. Ein Hilfs-`fn` mit passendem Namen zählt nicht — das ist Absicht.

---

## 10. Wenn etwas schiefgeht

- **Ein Gate ist rot und du weißt nicht, ob es an dir liegt.** Den Basis-Lauf
  aus §0.3 gegen `origin/dev` wiederholen. War es dort schon rot, ist es nicht
  deine Arbeit — als Baseline im Ledger festhalten und nicht mitreparieren.
- **Ein Display-Test flackert.** Nur Einzelläufe sind Beleg — das Kommando
  steht in der Abnahmeliste oben. Ein Herdenlauf, der vier Tests reißt, ist
  kein Befund; vier Einzelläufe, die reißen, sind einer.
- **Ein Task wächst über seinen Umfang.** Abbrechen, den erreichten Stand
  committen, den Rest als eigenen Task anhängen. Ein Commit pro Task ist die
  Regel; zwei kleine Commits sind besser als ein Task, der Welle 0 aufhält.
- **`cargo audit` meldet eine neue Advisory.** STOP. Nicht filtern, nicht
  akzeptieren — das ist eine eigene Entscheidung mit eigenem Commit.
- **Der Ledger widerspricht dem Code.** Der Code gewinnt. `git log` ist die
  Wahrheit; der Ledger ist die Erzählung darüber.

---

## 11. Zum Schluss von Welle 0 und 1

1. `scripts/check-merge-readiness.sh` grün, aus einem sauberen Baum.
2. Je Welle ein squashed PR gegen `dev`, Titel als Conventional Commit.
3. Ledger-Zeilen für alle Tasks.
4. **Dieses Runbook löschen** — im selben Commit wie die letzte Ledger-Zeile.
   `architecture-consolidation.md` und `consolidation-implementation.md`
   bleiben; in Letzterem den Status von Welle 0 und 1 auf erledigt setzen.
