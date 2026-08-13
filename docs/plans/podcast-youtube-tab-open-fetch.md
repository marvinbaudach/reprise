---
slug: podcast-youtube-tab-open-fetch
worktree: /home/marvin/Projects/reprise-podcast-youtube-tab-open-fetch
branch: feature/podcast-youtube-tab-open-fetch
phase: planned
codex_session:
created: 2026-08-13
spec: docs/superpowers/specs/2026-08-13-podcast-youtube-tab-open-fetch-design.md
---
# Podcasts und YouTube holen beim Öffnen des Tabs neue Folgen — Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> oder `superpowers:executing-plans` und arbeite Aufgabe für Aufgabe. Schritte sind Checkboxen.
> Der Entwurf (`spec` im Frontmatter) beschreibt das WAS und ist freigegeben und
> nicht verhandelbar. Dieser Plan beschreibt das WIE.

**Goal:** Wer den Podcasts- oder den YouTube-Tab öffnet, löst einen Netz-Fetch
**nur der eigenen Quelle** aus, wenn deren letzter Fetch länger als 15 Minuten
her ist, das Gerät online und das Netz nicht getaktet ist. Läuft der Fetch, ist
er im Footer **und** am Refresh-Knopf sichtbar. Ist nichts zu holen, passiert
nichts — kein Spinner, keine Statuszeile, kein Netzverkehr, kein yt-dlp-Prozess.

**Architecture:** `pipeline::refresh*` verliert seinen `force: bool` und nimmt
stattdessen einen `RefreshRequest { policy, kind }`. Der Schleifenkopf über die
Abos filtert zuerst nach Art (ein Abo fremder Art wird übersprungen, bevor
irgendetwas geprüft wird, und zählt nicht in `summary.attempted`), dann
entscheidet die Politik über die Fälligkeit: `Force` wie das heutige
`force == true`, `Due` wie das heutige `force == false`, `StaleFor { seconds }`
mit **demselben** Retry-Backoff wie `Due`, aber ohne Jitter und gegen ein
Sekundenintervall statt `refresh_hours`. Zeitplan und Tab-Öffnen teilen eine
gemeinsame Buchhaltung (`scope_status`), die pro Zuschnitt zählt und Fälligkeit
beantwortet. Der View bekommt `request_tab_open_refresh()` mit vier
Vorbedingungen und hält den Refresh-Knopf endlich als Feld, damit er einen
Spinner tragen kann.

**Tech Stack:** Rust, gtk4-rs, libadwaita, rusqlite (nur in reprise-core).
Keine neuen Abhängigkeiten, kein Schema, keine Migration, kein neuer
nutzersichtbarer String.

**Baseline:** `origin/dev` @ `6521524489`. **Jede** Zeilenangabe unten ist gegen
genau diesen Stand geprüft. Der Haupt-Checkout hängt **251 Commits** zurück und
enthält Dinge, die es auf `origin/dev` **nicht** gibt (z. B. ein
`priority`-Feld in `PodcastsRequest`). Im Worktree von `origin/dev` abzweigen,
nie von einem lokalen Branch, und nie den lokalen Arbeitsbaum als Wahrheit
nehmen:

```bash
git fetch origin dev
git worktree add /home/marvin/Projects/reprise-podcast-youtube-tab-open-fetch -b feature/podcast-youtube-tab-open-fetch origin/dev
```

---

## Global Constraints

### Verifikation läuft headless

Kein App-Fenster öffnen. Keinen Emulator starten. Keine langen Builds ohne Not.
Alles, was ein `DISPLAY` braucht, ist ein `#[ignore]`-Test und wird nur gezielt
über `xvfb-run` gefahren (Task 8).

### `cargo test -p reprise-gnome --lib` läuft ins Leere

`crates/reprise-gnome/Cargo.toml` deklariert **kein** `[lib]`, nur
`[[bin]] name = "reprise"` (Zeile 10–12). `--lib` bricht mit
`error: no library targets found in package reprise-gnome` ab — auf **stderr**,
**ohne** `test result:`-Zeile. Wer auf `^test result: FAILED` prüft, sieht dann
weder rot noch grün, sondern nichts, und hält das für grün. Richtig ist immer:

```bash
cargo test -p reprise-gnome --bin reprise <filter>
```

Zweite Falle derselben Familie: `-- --exact <name>` braucht den **vollen
Modulpfad** (`ui::podcasts::podcasts_view::tests::…`). Mit dem bloßen
Funktionsnamen matcht `--exact` nichts und der Lauf meldet
`test result: ok. 0 passed; … 1891 filtered out`. Nach **jedem** gefilterten
Lauf die Zahl vor `passed` prüfen — sie muss ≥ 1 sein. Namen vorher holen mit
`cargo test -p reprise-gnome --bin reprise -- --list | grep <name>`.

### Rot war vielleicht schon vorher rot — erst messen, dann urteilen

Auf `origin/dev` sind mehrere Gates und Display-Tests **ohne Zutun eines
Feature-Branches** rot (u. a. `browse_bar` Chips, zwei
`src_14_…`-Podcast-Menütests, `preferences_are_a_dialog_with_a_page_sidebar`,
`nav_10a_centering_lands_exactly_on_the_target`, gelegentlich
`check-frontend-thinness.sh` und `scripts/tests/gettext-catalogs.sh`).

**Aufgabe 0 dieses Plans ist deshalb eine Nullmessung** (siehe Task 0). Ein
Test oder Gate, der schon vor der ersten Änderung rot war, ist **nicht** deine
Schuld und wird **nicht** von dir repariert. Repariere nur, was durch deine
Änderung rot wurde, und schreib in den Abschlussbericht, welche Rotstellen
schon in der Nullmessung standen. Nicht stashen, um zu vergleichen —
`git stash` ist in diesem Repo **repo-global** und reißt fremden Worktrees den
Boden weg.

### Dateien bleiben unter 800 Zeilen

`scripts/check-architecture.sh` (Zeile 18–24) verwirft **jede** Datei unter
`crates/` mit `>= 800` Zeilen. Gemessen auf `origin/dev`:

| Datei | Zeilen heute | Headroom | Plan |
|---|---:|---:|---|
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` | **772** | 27 | Task 4 lagert den Footer-Bau aus → schrumpft, bevor etwas dazukommt |
| `crates/reprise-core/src/podcasts/pipeline_youtube_tests.rs` | **777** | 22 | **nur** In-Place-Argumenttausch, kein neuer Test |
| `crates/reprise-core/src/podcasts/pipeline_refresh_tests.rs` | **754** | 45 | **nur** In-Place-Argumenttausch, kein neuer Test |
| `crates/reprise-mcp/src/source_actions.rs` | **780** | 19 | genau **eine** Zeile geändert, keine dazu |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view_tests.rs` | **747** | 52 | unberührt; neue GTK-Tests kommen in eine neue Datei |
| `crates/reprise-core/src/podcasts/pipeline.rs` | 643 | 156 | Schleifenkopf, ~+20 |
| `crates/reprise-core/src/podcasts/refresh.rs` | 158 | 641 | neue Typen + Prädikat |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view_requests.rs` | 140 | 659 | `request_tab_open_refresh` |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs` | 365 | 434 | Operation-Variante |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_worker_tests.rs` | 526 | 273 | ein Testaufruf |
| `crates/reprise-gnome/src/ui/window/podcast_refresh_scheduler.rs` | 79 | 720 | wird kürzer |
| `crates/reprise-gnome/src/ui/window/library_shell.rs` | 541 | 258 | +2 Zeilen |

Die vier fett gesetzten Dateien sind die Gefahr. Neue Tests gehören in **neue**
Dateien, nicht in `pipeline_refresh_tests.rs` oder `pipeline_youtube_tests.rs`.
Nach jeder Aufgabe:
`wc -l $(git diff --name-only origin/dev -- '*.rs') | sort -n | tail`.

### `scripts/check-frontend-thinness.sh` — Budgets sind Decke **und** Boden

Werte auf `origin/dev` (Zeile 36 und 48–53):

```
view_floor=2115          # reprise-view Produktionszeilen, ein FLOOR
rusqlite=110  filesystem=13  threads=15  workers=7
```

Jedes Budget muss den gemessenen Wert **exakt** treffen: zu hoch **und** zu
niedrig ist rot. Konkret für diese Änderung:

- **`rusqlite` (110)** wird über das Muster
  `rusqlite::|use rusqlite|params!|\.prepare\(|\.query_row\(|Connection`
  in `crates/reprise-gnome/src` gezählt (Kommentarzeilen, `*_tests.rs` und
  `#[cfg(test)]`-Blöcke auf Spalte 0 zählen nicht). **Der neue Code in
  reprise-gnome darf keines dieser Muster enthalten.** Core-Aufrufe geben
  `Result<_, rusqlite::Error>` zurück — nimm das Ergebnis mit
  `match`/`unwrap_or(false)`/`unwrap_or_default()` entgegen, **ohne** den
  Fehlertyp zu benennen, genau wie `podcasts_worker.rs:109-120`
  (`any_source_dispatchable`) es schon tut. Achtung: `Connectivity` matcht das
  Muster `Connection` **nicht** — das ist in Ordnung.
- **`.conn(` ist in reprise-gnome-Produktionscode komplett verboten**
  (`check_ban db_handle_access`, Zeile 129). Der neue Code übergibt `&self.conn`
  (ein `Rc<Db>`) an Core und ruft **nie** `.conn()`.
- **`workers` = 7** zählt Dateien, deren Name `worker` enthält
  (`find crates/reprise-gnome/src -name '*worker*.rs' -not -name '*_tests.rs'`).
  **Keine neue Datei mit „worker" im Namen anlegen** — sonst 8 ≠ 7.
- `filesystem`, `threads`, `view_floor` werden von dieser Änderung nicht
  berührt. `reprise-view` bleibt unangetastet.

### `scripts/check-ux-traceability.sh`

Es gibt für den Spinner im Refresh-Knopf **keine** Regel in
`docs/ux-rules.md`, und du legst **keine** an — Regel-IDs sind ein gepflegtes
Dokument und append-only, das entscheidet der Mensch. Konsequenz für Task 8:
der neue Display-Test bekommt **keinen** Regel-Präfix im Namen (also nicht
`pod_…`, `src_…`, `net_…`), sondern einen sprechenden Namen ohne ID, und den
exakten Marker

```rust
#[ignore = "requires a display; run via xvfb-run"]
```

Damit greift Richtung 2 des Gates nicht (kein unbekannter Regelbezug) und
Richtung 3 auch nicht (die iteriert nur über regel-benannte Tests). Nebenwirkung,
die du im Bericht erwähnst: `check-display-tests.sh --rule-named` filtert den
Test heraus, er läuft also nicht im Merge-Gate mit; er wird in Task 8 einmal
gezielt gefahren.

### Clippy: `RefreshRequest` und `RefreshPolicy` müssen `Copy` sein

Der Gate ist `cargo clippy --locked --all-targets --workspace -- -D warnings`,
und `Cargo.toml` schaltet `clippy::needless_pass_by_value = "warn"` scharf
(`[workspace.lints.clippy]` ab Zeile 37, `needless_pass_by_value` auf Zeile 38). Ein `RefreshRequest`, der **nicht** `Copy` ist und
per Wert durch fünf Funktionen wandert, produziert an ~35 Stellen einen harten
Fehler. Deshalb:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
```

auf **beiden** Typen. `PodcastKind` ist bereits `Clone, Copy, PartialEq, Eq`
(`crates/reprise-core/src/podcasts.rs:41-46`), und `PodcastsOperation`
(`podcasts_worker.rs:10-15`) leitet `Clone, Copy, Debug, PartialEq, Eq` ab — die
neue Variante muss das tragen können.

### Immutability & Fehlerbehandlung

`RefreshRequest`/`RefreshPolicy` werden per Wert genommen und nie mutiert. Die
reinen Entscheider nehmen Slices und geben Werte zurück. Kein Fehler wird still
geschluckt: jeder DB-Lesefehler auf dem neuen Pfad geht als `tracing::warn!`
raus und führt zu „kein Fetch", nie zu einem Panic — genau wie
`podcast_refresh_scheduler.rs:52-79` es heute vormacht.

---

## Dateikarte

| Datei | Verantwortung nach dieser Änderung |
|---|---|
| `crates/reprise-core/src/podcasts/refresh.rs` | **+** `RefreshPolicy`, `RefreshRequest`, `refresh_due_after_seconds` |
| `crates/reprise-core/src/podcasts/pipeline.rs` | vier Einstiege nehmen `RefreshRequest`; Schleifenkopf mit Kind-Filter und Politik-Zweig |
| `crates/reprise-core/src/podcasts/pipeline_refresh_policy_tests.rs` (neu) | Kind-Filter, `StaleFor`, Backoff unter `StaleFor` |
| `crates/reprise-core/src/podcasts/pipeline_refresh_tests.rs`, `pipeline_youtube_tests.rs` | nur Argumenttausch, Erwartungen unverändert |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_refresh_decision.rs` (neu) | `TAB_OPEN_STALE_SECONDS`, `RefreshWindow`, `ScopeStatus`, `scope_status`, `tab_open_refresh_allowed`, `RefreshButtonState` |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_footer.rs` (neu) | baut die Footer-Zeile inkl. Refresh-Knopf mit Spinner-Stack |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` | hält Knopf, Stack, Spinner und den In-Flight-Zähler als Felder; `wire_controls` ohne Parameter |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_view_requests.rs` | `request_refresh_with`, `request_tab_open_refresh`, Knopf-Feedback-Guard |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_refresh_button_tests.rs` (neu) | der Display-Test für den Knopf |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs` | `Refresh { policy, kind }` statt `Refresh { force }` |
| `crates/reprise-gnome/src/ui/podcasts/mod.rs` | deklariert die neuen Module |
| `crates/reprise-gnome/src/ui/window/podcast_refresh_scheduler.rs` | `decision_inputs` wird ein dünner Adapter auf `scope_status` |
| `crates/reprise-gnome/src/ui/window/library_shell.rs` | zwei Zeilen im Routing |
| `crates/reprise-mcp/src/source_actions.rs` | ein Argument |

**Diese Tabelle ist Orientierung, keine abgeschlossene Liste.** Sie ist der
Stand von `6521524489`. Wenn eine weitere Datei angefasst werden muss, damit
das Verhalten stimmt oder der Workspace baut, fass sie an. Die
Aufrufersuche in Task 3 ist verbindlich, die Tabelle nicht.

---

## Aufrufermenge der Signaturänderung (Stand `6521524489`)

`pipeline::refresh`, `pipeline::refresh_with_download_progress`,
`pipeline::refresh_to_root` und
`pipeline::refresh_to_root_with_download_progress` verlieren ihren `force: bool`.
Vollständig ermittelt über `origin/dev`:

**Produktionscode: 2 Aufrufstellen in 2 Crates**

| Ort | Aufruf | heute `force` |
|---|---|---|
| `crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs:249` | `refresh_with_download_progress` | Variable `force` aus `PodcastsOperation::Refresh` |
| `crates/reprise-mcp/src/source_actions.rs:327` (Argument auf `:332`) | `refresh_to_root` | Literal `true` |

**Interne Weiterleitungen in `pipeline.rs`: 3**
`:249` (aus `refresh`), `:269` (aus `refresh_with_download_progress`), `:289`
(aus `refresh_to_root`) — alle in
`refresh_to_root_with_download_progress` (Definition `:322`).

**Tests: 30 Aufrufstellen in 2 Dateien**

- `crates/reprise-core/src/podcasts/pipeline_refresh_tests.rs` — 22× `refresh_to_root`
  auf den Zeilen 140, 143, 149, 174, 182, 204, 208, 231, 276, 286, 290, 297,
  318, 331, 345, 407, 494, 562, 583, 608, 632, 684 sowie 1×
  `refresh_to_root_with_download_progress` auf Zeile 368.
  `false` steht auf 276, 286, 290, 297, 318, 331; überall sonst `true`.
- `crates/reprise-core/src/podcasts/pipeline_youtube_tests.rs` — 7× `refresh_to_root`
  auf 176, 232, 318, 385, 502, 605, 718, alle mit `true`.

**Summe: 35 Stellen in 5 Dateien.** `pipeline::refresh` (die `Db`-Variante ohne
Download-Fortschritt, `:241`) hat **null** Aufrufer — sie ist `pub` und muss
mitgezogen werden, kostet aber keine Sweep-Arbeit.

**Reprise-platform-linux, reprise-android-ffi, reprise-cli, reprise-view,
reprise-runtime\*, reprise-stems berühren die Podcast-Pipeline nicht.** Die
einzigen Crates mit `podcasts::pipeline`-Bezug sind reprise-gnome (3 Dateien)
und reprise-mcp (2 Dateien, davon nur `source_actions.rs` mit einem
Refresh-Aufruf).

**Verlass dich nicht auf diese Zahlen — verifiziere sie selbst** (Task 3), denn
`origin/dev` bewegt sich:

```bash
git grep -n "refresh_to_root_with_download_progress\|refresh_with_download_progress\|refresh_to_root\b" -- '*.rs'
git grep -rn "podcasts::pipeline" -- '*.rs'
git grep -n "PodcastsOperation::Refresh\|Refresh {" -- '*.rs'
```

**Fertig ist der Sweep erst, wenn**
`cargo check --locked --workspace --all-targets` ohne Fehler durchläuft. Das ist
das einzige verlässliche Fertig-Kriterium; eine Dateiliste ist es nicht.

---

### Task 0: Nullmessung, bevor irgendetwas geändert wird

**Files:** keine.

- [ ] **Schritt 1: Zeilenzahlen und Budgets festhalten**

```bash
scripts/check-architecture.sh            > /tmp/base-arch.log 2>&1;      echo "arch=$?"
scripts/check-frontend-thinness.sh       > /tmp/base-thinness.log 2>&1;  echo "thinness=$?"
scripts/check-ux-traceability.sh         > /tmp/base-ux.log 2>&1;        echo "ux=$?"
```

- [ ] **Schritt 2: die Core-Suite und die betroffene GTK-Suite messen**

```bash
cargo test --locked -p reprise-core podcasts:: > /tmp/base-core.log 2>&1; echo "core=$?"
cargo test --locked -p reprise-gnome --bin reprise podcasts > /tmp/base-gnome.log 2>&1; echo "gnome=$?"
```

- [ ] **Schritt 3: die Podcast-Display-Tests einzeln messen**

Zwei davon (`src_14_opening_a_menu_inside_the_selection_keeps_it` und
`src_14_opening_a_menu_outside_the_selection_takes_the_selection_over`) sind auf
`dev` bekannt rot. Messen, nicht reparieren:

```bash
xvfb-run -a cargo test --locked -p reprise-gnome --bin reprise \
  -- --ignored ui::podcasts:: > /tmp/base-display.log 2>&1; echo "display=$?"
grep -c "^test result: FAILED" /tmp/base-display.log
grep -E "^(test .* FAILED|failures:)" /tmp/base-display.log
```

**Done when:** Die fünf Logs liegen vor, und du hast in einer Notiz stehen,
welche Tests und Gates **vor** der ersten Änderung rot waren. Diese Liste
kommt am Ende in den Abschlussbericht. Wenn `xvfb-run` in dieser Umgebung nicht
verfügbar ist: notieren, dass die Display-Nullmessung nicht möglich war, und in
Task 8 nicht behaupten, ein Display-Test sei gelaufen.

---

### Task 1: `RefreshPolicy`, `RefreshRequest` und das Sekundenprädikat in Core

**Files:**
- Modify: `crates/reprise-core/src/podcasts/refresh.rs` (158 Zeilen)
- Test: im vorhandenen `#[cfg(test)] mod tests` derselben Datei (Zeile 89 ff.)

**Interfaces:**
- Consumes: `super::config::DEFAULT_REFRESH_HOURS`, `PodcastKind`.
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum RefreshPolicy {
      Due,
      StaleFor { seconds: i64 },
      Force,
  }

  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct RefreshRequest {
      pub policy: RefreshPolicy,
      pub kind: Option<crate::podcasts::PodcastKind>,
  }

  impl RefreshRequest {
      #[must_use] pub const fn force() -> Self;
      #[must_use] pub const fn due() -> Self;
      #[must_use] pub const fn stale_for(seconds: i64, kind: Option<PodcastKind>) -> Self;
      #[must_use] pub const fn with_kind(self, kind: Option<PodcastKind>) -> Self;
  }

  #[must_use]
  pub fn refresh_due_after_seconds(last_fetch_at: Option<i64>, now: i64, seconds: i64) -> bool;
  ```

Die drei Konstruktoren sind kein Luxus: `force()` und `due()` halten die 30
Testaufrufstellen in Task 2 bei **einer** Zeile und `source_actions.rs` bei
seinen 780 Zeilen.

- [ ] **Schritt 1: die fehlschlagenden Tests schreiben**

In `mod tests` von `refresh.rs`, neben den vorhandenen. Genau diese Fälle:

```rust
#[test]
fn refresh_due_after_seconds_treats_a_never_fetched_subscription_as_due() { … }
// last_fetch_at == None  -> true

#[test]
fn refresh_due_after_seconds_has_an_exact_boundary_and_no_jitter() {
    let now = 100_000;
    assert!(!refresh_due_after_seconds(Some(now - 899), now, 900));
    assert!(refresh_due_after_seconds(Some(now - 900), now, 900));
}

#[test]
fn refresh_due_after_seconds_refuses_a_clock_that_moved_backwards() {
    // Spiegelt refresh_due_with_hours (refresh.rs:63-65): elapsed < 0 -> false
    assert!(!refresh_due_after_seconds(Some(100_001), 100_000, 900));
}

#[test]
fn refresh_request_constructors_carry_policy_and_scope() {
    assert_eq!(RefreshRequest::force(), RefreshRequest { policy: RefreshPolicy::Force, kind: None });
    assert_eq!(RefreshRequest::due(), RefreshRequest { policy: RefreshPolicy::Due, kind: None });
    assert_eq!(
        RefreshRequest::stale_for(900, Some(PodcastKind::Rss)).kind,
        Some(PodcastKind::Rss)
    );
}
```

Erst mit `todo!()`/`unimplemented!()` in `refresh_due_after_seconds` anlegen,
Test läuft rot, dann implementieren.

- [ ] **Schritt 2: implementieren**

`refresh_due_after_seconds` ist bewusst **nicht** `refresh_due_with_hours` mit
umgerechnetem Argument: dort steckt ein `clamp(1, 24)` auf Stunden und ein
Jitter-Term, beides ist für `StaleFor` falsch (der Nutzer hat gerade selbst
gehandelt; Jitter existiert nur, um automatische Läufe über die Zeit zu
verteilen). Gleich bleiben muss die Behandlung von „nie geholt" (true) und
einer rückwärts gelaufenen Uhr (false).

- [ ] **Schritt 3: Doku-Kommentar**

Schreib in den Modulkopf, **warum** es zwei Prädikate gibt (Stunden+Jitter für
den Zeitplan, Sekunden ohne Jitter für das Tab-Öffnen), damit das nicht in
einem Jahr als Duplikat „aufgeräumt" wird.

**Hinweis, damit du nicht am falschen Prädikat baust:** in dieser Datei steht
auf Zeile 80 ein `should_auto_refresh(enabled, count, metered, due)`, das
**keinen** Produktionsaufrufer hat — der lebende Zwilling ist
`automatic_refresh_allowed` in
`crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs:201`. Baue in Task 5
auf **dem** auf. Den toten Zwilling **nicht** löschen und **nicht** anfassen;
das ist eine eigene Aufräumarbeit.

**Done when:**
```bash
cargo test --locked -p reprise-core podcasts::refresh
```
läuft grün und führt ≥ 4 neue Tests aus (Zahl vor `passed` prüfen).
`wc -l crates/reprise-core/src/podcasts/refresh.rs` < 400.

---

### Task 2: Kind-Filter und Politik-Zweig im Schleifenkopf der Pipeline

**Files:**
- Modify: `crates/reprise-core/src/podcasts/pipeline.rs` (643 Zeilen)
- Create: `crates/reprise-core/src/podcasts/pipeline_refresh_policy_tests.rs`
- Modify: `crates/reprise-core/src/podcasts/pipeline_refresh_tests.rs` (nur Argumente)
- Modify: `crates/reprise-core/src/podcasts/pipeline_youtube_tests.rs` (nur Argumente)

**Interfaces:**
- `pub fn refresh(db, feed_fetcher, youtube_fetcher, now, request: RefreshRequest)` (`:241`)
- `pub fn refresh_with_download_progress(db, …, now, request: RefreshRequest, on_download)` (`:260`)
- `pub fn refresh_to_root(db, …, now, request: RefreshRequest, download_root)` (`:280`)
- `fn refresh_to_root_with_download_progress(conn, …, now, request: RefreshRequest, download_root, on_download)` (`:322`)

Der `RefreshRequest` steht an genau der Stelle, an der heute `force: bool`
steht — zwischen `now` und `download_root`/`on_download`. Damit bleibt jeder
Aufruf ein Einzeiler-Tausch.

- [ ] **Schritt 1: das neue Testmodul anmelden und die fehlschlagenden Tests schreiben**

`pipeline.rs` deklariert seine Testmodule am Ende (`:634`, `:638`, `:642`).
Dazu kommt:

```rust
#[cfg(test)]
#[path = "pipeline_refresh_policy_tests.rs"]
mod refresh_policy_tests;
```

Die neue Datei braucht ihre **eigenen** Fakes und Helfer — die in
`pipeline_refresh_tests.rs` sind dateilokal. Übernimm das Muster von dort
(`conn()` auf `:83`, `add_subscription()` auf `:93`, `feed_response()` auf
`:110`), mit **zwei** Unterschieden:

1. `conn()` muss **beide** Module scharf schalten, sonst ist die
   YouTube-Kontrollzeile schon durch `NET-1a` blockiert und der Test beweist
   nichts:
   ```rust
   crate::online_sources::set_enabled(&conn, true).unwrap();
   crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
   crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, true).unwrap();
   ```
2. Ein **zählender** YouTube-Fake, dessen `resolve_channel_url` und `list`
   einen `Cell<usize>` hochzählen. Die YouTube-Abo-URL muss eine
   **Handle**-Form sein (`https://www.youtube.com/@kontrolle`), damit
   `youtube::long_form_feed_url` `None` liefert und der Pipeline-Pfad
   tatsächlich `resolve_channel_url` aufrufen **würde** — nur dann ist „null
   Aufrufe" ein Beweis und nicht ein Zufall.

Die Tests, alle vier aus dem Entwurf, jeder mit Kontrollzeile:

```rust
#[test] fn rss_scope_never_touches_a_youtube_subscription()
// Abos: 1× Rss, 1× Youtube(@handle). refresh_to_root(…, RefreshRequest::force().with_kind(Some(PodcastKind::Rss)), …)
// erwartet: summary.attempted == 1, youtube_calls == 0,
//           store::subscription(youtube_id).last_fetch_at == None (unberührt)

#[test] fn youtube_scope_never_touches_an_rss_subscription()
// Gegenrichtung. Der FeedFetcher-Fake darf für das RSS-Abo NICHT aufgerufen
// werden — leg seine `responses`-Liste leer an, dann panickt `remove(0)`,
// falls der Filter versagt. Kontrollzeile: RSS-`last_fetch_at` bleibt None.

#[test] fn stale_for_below_the_threshold_fetches_nothing_and_above_it_fetches()
// ein Abo, erst mit Force auf now=1000 holen (setzt last_fetch_at=1000),
// dann StaleFor{seconds:900} auf now=1899 -> attempted == 0,
// dann auf now=1900 -> attempted == 1

#[test] fn stale_for_respects_an_open_retry_backoff()
// Abo scheitert unter Due/Force (Transport) -> last_outcome == "failed",
// dann StaleFor mit weit überschrittener Schwelle, aber vor `retry_at`
// -> attempted == 0; nach `retry_at` -> attempted == 1
```

> **Falle im Retry-Test, ausdrücklich dokumentiert in
> `crates/reprise-core/src/podcasts/pipeline_retry.rs` (Modulkopf):** der
> Backoff-Schlüssel enthält die **Adresse** der `Connection`, nicht ihre
> Identität. In Tests, die dauernd Datenbanken anlegen und wegwerfen, kann ein
> frischer DB an derselben Adresse den Backoff des Vorgängers erben. Halte die
> `Db` des Retry-Tests deshalb über die ganze Testfunktion am Leben (eine
> Bindung, kein temporärer Wert), und verlass dich nicht darauf, dass ein
> anderer Test seinen Zustand aufgeräumt hat. Wenn der Test flatterhaft wird,
> ist das die Ursache — nicht der neue Code.

- [ ] **Schritt 2: den Schleifenkopf umbauen**

Der heutige Kopf steht in `refresh_to_root_with_download_progress` auf
`:337-364`: `for subscription in subscriptions {` → `retry_key` → `if !force {`
(`:342`) mit `pending_retry`/`clear_retry` und
`refresh_due_with_hours(subscription.last_fetch_at, now, config.refresh_hours, jitter)`
(`:351-356`) → `if !due { continue; }` → `summary.attempted += 1;` (`:364`).

Neu, in dieser Reihenfolge:

1. **Kind-Filter zuerst**, vor `retry_key`, vor allem anderen:
   ```rust
   if let Some(kind) = request.kind {
       if subscription.kind != kind {
           continue;
       }
   }
   ```
   Vor `summary.attempted += 1`, also zählt ein Abo fremder Art nicht als
   Versuch. Und vor dem Retry-Block, also rührt ein fremder Zuschnitt den
   Backoff-Zustand nicht an — wichtig, sonst löscht ein Podcasts-Tab-Fetch
   still den Backoff eines YouTube-Abos.
2. **Politik statt `bool`:** der Block bleibt strukturell wie er ist, nur die
   Bedingung und das Fälligkeitsprädikat werden zur Politik:
   ```rust
   if !matches!(request.policy, RefreshPolicy::Force) {
       let retry = /* unverändert: pending_retry / clear_retry */;
       let due = retry.map_or_else(
           || match request.policy {
               RefreshPolicy::Due => super::refresh::refresh_due_with_hours(
                   subscription.last_fetch_at, now, config.refresh_hours, jitter,
               ),
               RefreshPolicy::StaleFor { seconds } => {
                   super::refresh::refresh_due_after_seconds(
                       subscription.last_fetch_at, now, seconds,
                   )
               }
               RefreshPolicy::Force => true,
           },
           |retry| retry.is_due(now),
       );
       if !due { continue; }
   }
   ```
   Damit gilt für `StaleFor` **derselbe** Retry-Backoff wie für `Due`
   (inklusive `clear_retry` bei `last_outcome != Some("failed")`) — genau, was
   der Entwurf verlangt: ein Feed, der dauerhaft 404 liefert, darf nicht bei
   jedem Tab-Wechsel neu angefragt werden und die Fehlermeldung neu erzeugen.
   Der `Force`-Arm im `match` ist unerreichbar, aber billiger und
   zukunftssicherer als ein `unreachable!()`.

Der Jitter (`:334`) wird weiter unverändert einmal pro Lauf berechnet; er wird
im `StaleFor`-Zweig nur nicht benutzt. Kein `#[allow(unused)]` nötig, weil
`Due` ihn weiter liest.

- [ ] **Schritt 3: die 30 Testaufrufe migrieren**

Mechanisch, Erwartungen **unverändert**:

```
… , now, true,  …   →   … , now, RefreshRequest::force(), …
… , now, false, …   →   … , now, RefreshRequest::due(),   …
```

Beide Dateien brauchen den Import (`use super::*;` steht schon da; ggf.
`use crate::podcasts::refresh::RefreshRequest;` ergänzen). **Keine Zeile
dazu** — die beiden Dateien liegen bei 754 und 777 Zeilen.

Ein Ergebnis, das du erwarten sollst: `pipeline_refresh_tests.rs` hat sechs
`false`-Stellen (276, 286, 290, 297, 318, 331), alle im
`net_3d_retryable_refresh…`- und Due-Umfeld — sie müssen ohne jede Änderung
ihrer Assertions grün bleiben. Wenn einer davon rot wird, hat der
Politik-Zweig `Due` verändert, und das ist ein Fehler, keine Testanpassung.

**Done when:**
```bash
cargo test --locked -p reprise-core podcasts::                 # grün
cargo test --locked -p reprise-core refresh_policy_tests       # ≥ 4 Tests, grün
wc -l crates/reprise-core/src/podcasts/pipeline*.rs            # alle < 800
```
`reprise-gnome` und `reprise-mcp` bauen zu diesem Zeitpunkt **nicht** — das ist
richtig und wird in Task 3 behoben. `cargo test -p reprise-core` baut die
beiden Crates nicht mit, also ist dieser Zwischenstand messbar grün.

---

### Task 3: Der repo-weite Sweep

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_worker.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_worker_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_requests.rs`
- Modify: `crates/reprise-mcp/src/source_actions.rs`
- und **alles weitere, was die Suche findet**

- [ ] **Schritt 1: die Aufrufermenge selbst ermitteln**

```bash
git grep -n "refresh_to_root_with_download_progress\|refresh_with_download_progress\|refresh_to_root\b" -- '*.rs'
git grep -rn "podcasts::pipeline" -- '*.rs'
git grep -n "PodcastsOperation::Refresh\|Refresh { force\|Refresh {" -- '*.rs'
git grep -n "refresh_to_root\|refresh_with_download_progress" -- '*.md' '*.sh' '*.toml'
```

Vergleiche das Ergebnis mit der Tabelle im Abschnitt „Aufrufermenge" oben. Wenn
du **mehr** findest, bediene alles. Wenn du **weniger** findest, hat sich
`origin/dev` bewegt — halte das im Bericht fest.

- [ ] **Schritt 2: `PodcastsOperation::Refresh` bekommt Politik und Zuschnitt**

`podcasts_worker.rs:10-15`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PodcastsOperation {
    Refresh { policy: RefreshPolicy, kind: Option<PodcastKind> },
    LoadMore { subscription_id: i64, end: usize },
    Download { episode_id: i64 },
}
```

`request_generation` (`:17-27`) matcht mit `Refresh { .. }` und bleibt
unverändert — die Politik ist damit weiter Teil der Anfrage-Identität, was der
Entwurf ausdrücklich will: zwei Anfragen mit unterschiedlicher Politik sind
unterschiedliche Anfragen.

Der Worker-Arm (`:243-266`) baut aus beiden Feldern den `RefreshRequest`:

```rust
PodcastsOperation::Refresh { policy, kind } => {
    …
    podcasts::pipeline::refresh_with_download_progress(
        conn, &podcasts::pipeline::HttpFeedFetcher, &ytdlp,
        chrono::Utc::now().timestamp(),
        podcasts::refresh::RefreshRequest { policy, kind },
        &mut |episode_id, state| { … },
    )
    …
}
```

`podcasts_worker_tests.rs:142` (`request_generation(9, PodcastsOperation::Refresh { force: true })`)
zieht mit. Die fünf `automatic_refresh_allowed`-Assertions auf `:113-117`
bleiben unberührt — die Funktion ändert sich nicht.

- [ ] **Schritt 3: `request_refresh` behält seine Signatur, bekommt aber einen inneren Weg**

In `podcasts_view_requests.rs`: `request_refresh(self: &Rc<Self>, force: bool)`
(`:6`) **bleibt** öffentlich bestehen — der Knopf
(`podcasts_view.rs:354`), der Status-Knopf (`podcasts_view.rs:375`), der
Retry-Knopf (`podcasts_failure_ui.rs:175`) und der Zeitplan
(`podcast_refresh_scheduler.rs:31`) rufen ihn weiter so. Er wird ein Adapter:

```rust
pub(in crate::ui) fn request_refresh(self: &Rc<Self>, force: bool) -> bool {
    let policy = if force { RefreshPolicy::Force } else { RefreshPolicy::Due };
    self.request_refresh_with(RefreshRequest { policy, kind: None })
}

fn request_refresh_with(self: &Rc<Self>, request: RefreshRequest) -> bool { /* der heutige Körper */ }
```

**`kind: None` für Knopf und Zeitplan ist eine bewusste Nicht-Änderung.** Der
Knopf holt heute beide Arten, auch wenn er auf dem Podcasts-Tab gedrückt wird;
der Entwurf verlangt für ihn keinen Zuschnitt („`request_refresh(force: bool)`
bleibt als Weg des Knopfes bestehen"). Nur der Tab-Fetch schneidet zu. Wer das
ändern will, ändert Verhalten, das niemand bestellt hat.

- [ ] **Schritt 4: reprise-mcp**

`source_actions.rs:332` — das `true,` (im Aufruf ab `:327`) wird zu

```rust
        reprise_core::podcasts::refresh::RefreshRequest::force(),
```

**Genau eine Zeile, keine dazu.** Die Datei steht bei 780 Zeilen.

**Done when:**
```bash
cargo check --locked --workspace --all-targets     # keine Fehler
cargo clippy --locked --all-targets --workspace -- -D warnings
git grep -n "force: bool\|Refresh { force\|, true,\s*$" -- crates/reprise-gnome crates/reprise-mcp
```
Der letzte Befehl darf nichts mehr zeigen, was zur Podcast-Pipeline gehört.
Wenn `clippy` `needless_pass_by_value` für `RefreshRequest` meldet, fehlt
`Copy` — nachtragen, nicht `#[allow]`.

---

### Task 4: Der Footer zieht in eine eigene Datei, der Refresh-Knopf wird ein Feld

Diese Aufgabe **vor** Task 6 erledigen: sie schafft den Zeilen-Headroom in
`podcasts_view.rs` (772 von 799) **und** legt den Knopf dorthin, wo Task 6 ihn
braucht.

**Files:**
- Create: `crates/reprise-gnome/src/ui/podcasts/podcasts_footer.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/mod.rs` (Moduldeklaration)
- Test: `podcasts_refresh_decision.rs` deckt den reinen Teil ab (Task 5); der
  Widget-Beweis ist Task 8.

**Interfaces:**
```rust
// podcasts_footer.rs
pub(super) struct PodcastsFooter {
    pub root: gtk4::Box,
    pub add: gtk4::Button,
    pub status: gtk4::Label,
    pub spinner: gtk4::Spinner,
    pub refresh_button: gtk4::Button,
    pub refresh_stack: gtk4::Stack,
    pub refresh_spinner: gtk4::Spinner,
}

pub(super) fn build(kind: PodcastKind) -> PodcastsFooter;
```

- [ ] **Schritt 1: den heutigen Footer-Bau wortgleich verschieben**

`podcasts_view.rs:170-194` (Box, Margins, `footer_add` mit
`buttons::arm(&footer_add, buttons::ADD_ACTION_CLASS)` und
`set_action_name(Some("podcasts.open-add"))`, `footer_spinner`,
`footer_status` mit `caption`/`dim-label`/`hexpand`/`xalign(0.0)`, der
Refresh-Knopf `:192-194` mit Label `strings::PODCAST_REFRESH_NOW` und Klasse
`flat`) wandert unverändert nach `podcasts_footer::build`. Keine
Verhaltensänderung in diesem Schritt, nur Umzug — das ist der Teil, der
`podcasts_view.rs` um ~25 Zeilen entlastet.

- [ ] **Schritt 2: den Refresh-Knopf spinnerfähig machen**

Vorbild ist `concerts_view.rs:446-473` (`build_footer`), das seinen Fetch-Knopf
und einen Spinner in einen `gtk4::Stack` mit zwei Seiten legt und mit
`set_visible_child_name` umschaltet; `:500` setzt `set_sensitive(false)`, `:558`
und `:559` nehmen es zurück. Der Unterschied hier: der Podcast-Knopf trägt ein
**Label**, kein Icon (der Entwurf sagt „Spinner statt seines Icons" — auf
`origin/dev` ist es `Button::with_label(PODCAST_REFRESH_NOW)`). Deshalb sitzt
der Stack **im Knopf**, nicht neben ihm:

```rust
let refresh_stack = gtk4::Stack::new();
refresh_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
refresh_stack.add_named(&label,   Some(REFRESH_LABEL_PAGE));
refresh_stack.add_named(&spinner, Some(REFRESH_SPINNER_PAGE));
refresh_stack.set_visible_child_name(REFRESH_LABEL_PAGE);
let refresh_button = gtk4::Button::new();
refresh_button.set_child(Some(&refresh_stack));
refresh_button.add_css_class("flat");
```

`GtkStack` ist standardmäßig h/v-homogen und fordert die Größe seines größten
Kindes — der Knopf behält also seine Breite, wenn der Spinner erscheint, und
die Footer-Zeile springt nicht. Zwei Seitennamen als `const` in
`podcasts_footer.rs`, kein Stringliteral an der Nutzungsstelle.

**Kein neuer nutzersichtbarer String.** Das Label ist weiter
`strings::PODCAST_REFRESH_NOW`; der Spinner hat keinen Text.

- [ ] **Schritt 3: die Felder am View**

In `PodcastsView` (`:80-137`) kommen dazu:

```rust
refresh_button: gtk4::Button,
refresh_stack: gtk4::Stack,
refresh_spinner: gtk4::Spinner,
/// Wie viele Refresh-Anfragen dieses Views gerade laufen. Der Knopf ist
/// insensitiv, solange das > 0 ist — ein Zähler und nicht ein Bool, weil
/// Zeitplan, Knopf und Tab-Öffnen sich überlappen können und der Knopf sonst
/// vom ältesten Ausgang freigegeben würde, während der jüngste noch fetcht.
refresh_in_flight: Cell<usize>,
```

`footer`, `footer_add`, `footer_status`, `footer_spinner` behalten **exakt**
ihre heutigen Feldnamen — sie werden im ganzen Modul benutzt, und ein
Umbenennen wäre nutzlose Blast-Radius. Im `Self { … }` (`:208-253`) wird die
`PodcastsFooter` destrukturiert und zugewiesen.

- [ ] **Schritt 4: `wire_controls` verliert seinen Parameter**

`wire_controls(self: &Rc<Self>, refresh_button: &gtk4::Button)` (`:350`) wird
`wire_controls(self: &Rc<Self>)` und liest `self.refresh_button`. Die einzige
Aufrufstelle auf `origin/dev` ist `:255` — **prüfe das selbst** mit
`git grep -n "wire_controls" -- crates/reprise-gnome/src`, es kann inzwischen
mehr geben.

- [ ] **Schritt 5: die veraltete Doku am Connectivity-Feld korrigieren**

`podcasts_view.rs:130-135` behauptet, `connectivity` sei „not wired to any real
OS signal yet". Das ist auf `origin/dev` falsch:
`crates/reprise-gnome/src/ui/window/source_connectivity.rs:107-126` projiziert
`gio::NetworkMonitor` an der Fensterkante und ruft `set_connectivity` für
beide Podcast-Views (`:54-59`). Der Kommentar wird richtiggestellt — Task 6
verlässt sich auf diesen Seam, und ein Kommentar, der ihn für toten Code
erklärt, ist eine Falle für die nächste Lesung.

**Done when:**
```bash
cargo clippy --locked --all-targets -p reprise-gnome -- -D warnings
cargo test --locked -p reprise-gnome --bin reprise podcasts     # ≥ 1 Test, grün
wc -l crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs     # < 772, also GESCHRUMPFT
scripts/check-frontend-thinness.sh                              # wie in der Nullmessung
```
Wenn `podcasts_view.rs` nach dieser Aufgabe **nicht** kleiner ist als vorher,
ist der Umzug unvollständig.

---

### Task 5: Ein Entscheider für Zeitplan und Tab-Öffnen

**Files:**
- Create: `crates/reprise-gnome/src/ui/podcasts/podcasts_refresh_decision.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/mod.rs`
- Modify: `crates/reprise-gnome/src/ui/window/podcast_refresh_scheduler.rs`
- Test: `#[cfg(test)] mod tests` **in** `podcasts_refresh_decision.rs`, auf
  Spalte 0 geöffnet und geschlossen (dann zählt der Thinness-Scanner den Block
  nicht mit, siehe `check-frontend-thinness.sh:70-86`)

**Interfaces:**
```rust
pub(in crate::ui) const TAB_OPEN_STALE_SECONDS: i64 = 15 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RefreshWindow {
    /// Zeitplan: `sources.refresh_hours` plus Datenbank-Jitter.
    Hours { refresh_hours: i64, jitter_seconds: i64 },
    /// Tab-Öffnen: Sekunden, ohne Jitter.
    Seconds(i64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct ScopeStatus { pub count: usize, pub due: bool }

#[must_use]
pub(in crate::ui) fn scope_status(
    subscriptions: &[reprise_core::podcasts::SubscriptionRow],
    kind: Option<reprise_core::podcasts::PodcastKind>,
    window: RefreshWindow,
    now: i64,
) -> ScopeStatus;

#[must_use]
pub(in crate::ui) fn tab_open_refresh_allowed(
    network_allowed: bool,
    connectivity: reprise_core::connectivity::Connectivity,
    metered: bool,
    refresh_running: bool,
    status: ScopeStatus,
) -> bool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum RefreshButtonState { Idle, Busy }

#[must_use]
pub(in crate::ui) const fn refresh_button_state(in_flight: usize) -> RefreshButtonState;
```

`crates/reprise-gnome/src/ui/podcasts/mod.rs` (65 Zeilen) deklariert das neue
Modul **und** re-exportiert, was außerhalb von `ui::podcasts` gebraucht wird:
`podcast_refresh_scheduler.rs` liegt in `ui/window/` und kommt sonst nicht an
`scope_status`, `RefreshWindow` und `ScopeStatus` heran. Das Muster steht schon
in der Datei (`pub(in crate::ui) use podcasts_worker::PodcastsRuntime;`).
`TAB_OPEN_STALE_SECONDS`, `tab_open_refresh_allowed` und
`RefreshButtonState`/`refresh_button_state` braucht nur das Podcast-Modul
selbst — die müssen **nicht** re-exportiert werden.

`scope_status` ist die **gemeinsame Buchhaltung**, die der Entwurf verlangt:
`count` sind die Abos **im Zuschnitt** (`kind == None` heißt alle), `due` ist
„mindestens eines im Zuschnitt ist nach diesem Fenster fällig". `Hours` ruft
`reprise_core::podcasts::refresh::refresh_due_with_hours`, `Seconds` ruft
`refresh_due_after_seconds` aus Task 1 — die beiden Formeln bleiben in Core,
hier liegt nur die Auswahl.

`tab_open_refresh_allowed` baut auf dem **lebenden** Prädikat auf:

```rust
connectivity == Connectivity::Online
    && !refresh_running
    && super::podcasts_worker::automatic_refresh_allowed(
        network_allowed, status.count, metered, status.due,
    )
```

`refresh_running` ist die **fünfte Vorbedingung**, und sie ist im Grill
ausdrücklich beschlossen worden — sie steht nicht im Entwurfstext, sondern geht
über ihn hinaus. Ohne sie passiert Folgendes: der Zeitplan fetcht gerade, der
Nutzer öffnet den Tab, `last_fetch_at` ist noch nicht geschrieben, die
Vorprüfung sagt „stale" und stellt eine zweite Anfrage. Der Worker hat eine
Lane, die zweite läuft also hinterher — und findet jedes Abo frisch, überspringt
alles und hat nur den Spinner für eine leere Runde laufen lassen.

Core hat für genau diesen Fehler schon eine Antwort:
`crates/reprise-core/src/updates.rs:76`, `fetch_allowed(enabled, fetching, due)`,
mit dem Kommentar, dass die Frontends das früher je mit einem eigenen
`Cell<bool>` bewachten „and each of them had to remember to". Dies ist dieselbe
Frage in derselben Form; `fetching` heißt hier `refresh_running`.

Die Quelle der Antwort ist der Zähler, den Task 4 als `refresh_in_flight` am
View anlegt und den Task 6 hoch- und runterzählt: der Aufrufer übergibt
`self.refresh_in_flight.get() > 0`. Ein `bool` statt des Zählers ist Absicht —
das Prädikat soll nicht zwei Bedeutungen desselben `usize` kennen müssen.

**Der Zeitplan bekommt dieses Gate nicht.** Er ruft `tab_open_refresh_allowed`
nicht auf, und `automatic_refresh_allowed` bleibt unverändert — sein
Doppellauf-Schutz ist heute die Stundengrenze, und „der Zeitplan bleibt
inhaltlich unverändert" gilt weiter.

Warum nicht **eine** Funktion für beide Auslöser: der Zeitplan hat keinen
Connectivity-Seam (`source_connectivity.rs:102-106` erklärt ausdrücklich, dass
er als einziger direkt `is_network_metered()` liest und dass Connectivity
gepusht statt gezogen wird), und der Entwurf sagt „Der Zeitplan bleibt
inhaltlich unverändert". Geteilt wird deshalb genau das, was sonst
auseinanderdriftet — die Buchhaltung (`scope_status`) und das
Torwächter-Prädikat (`automatic_refresh_allowed`) —, nicht die
Connectivity-Frage, die nur einer der beiden stellen kann.

- [ ] **Schritt 1: die fehlschlagenden Tests schreiben**

Alles reine Funktionen, also normale `#[test]`s ohne `#[ignore]`, ohne
`gtk4::init()`. Baue `SubscriptionRow`-Werte direkt als Structliteral
(`crates/reprise-core/src/podcasts.rs:55-74`, alle Felder `pub`) — keine
Datenbank, kein Fixture.

```rust
#[test] fn scope_status_counts_only_the_requested_kind()
// 2× Rss, 1× Youtube; Some(Rss) -> count == 2, Some(Youtube) -> count == 1, None -> 3

#[test] fn scope_status_is_not_due_when_every_subscription_in_scope_was_just_fetched()
// alle last_fetch_at == now - 60, Seconds(900) -> due == false

#[test] fn scope_status_is_due_when_one_subscription_in_scope_is_stale()
// eines auf now - 901 -> due == true

#[test] fn scope_status_ignores_a_stale_subscription_of_another_kind()
// Youtube-Abo ist uralt, Some(Rss) mit frischem RSS-Abo -> due == false
// (das ist der Test, der beweist, dass ein Tab-Wechsel keinen yt-dlp-Lauf auslöst)

#[test] fn scope_status_measures_the_schedule_in_hours_plus_jitter()
// Hours { refresh_hours: 6, jitter_seconds: 3_600 }: 25_199 -> false, 25_200 -> true
// (spiegelt refresh.rs:99-100, damit der Zeitplan beweisbar unverändert bleibt)

#[test] fn tab_open_refuses_offline_metered_disabled_empty_fresh_and_already_running()
// sechs Verweigerungen, eine je Vorbedingung — Offline, getaktet, Quelle nicht
// erlaubt, keine Abos im Zuschnitt, alles frisch geholt, und ein Refresh läuft
// schon (refresh_running == true) —, plus ein Fall, in dem alles erfüllt ist.
// Der letzte Verweigerungsfall muss mit `status.due == true` gebaut werden,
// sonst beweist er nichts: mit einem nicht fälligen Status wäre das Ergebnis
// auch ohne das neue Gate `false`.

#[test] fn refresh_button_stays_busy_until_the_last_refresh_finished()
// 0 -> Idle, 1 -> Busy, 2 -> Busy
```

- [ ] **Schritt 2: implementieren, ohne die Thinness-Muster zu berühren**

Die Datei darf **kein** `rusqlite`, `use rusqlite`, `params!`, `.prepare(`,
`.query_row(`, `Connection` und **kein** `.conn(` enthalten. Sie sieht keine
Datenbank — sie bekommt einen `&[SubscriptionRow]` herein. Der Name enthält
kein „worker".

- [ ] **Schritt 3: den Zeitplan auf den Entscheider stellen**

`podcast_refresh_scheduler.rs:52-79` (`decision_inputs`) gibt heute
`(usize, bool)` zurück und wiederholt die Jitter- und Fälligkeitsrechnung.
Neu: der Adapter lädt und delegiert.

```rust
fn decision_inputs(db: &Db, db_path: &Path) -> ScopeStatus {
    let subscriptions = match reprise_core::podcasts::store::active_subscriptions(db) {
        Ok(subscriptions) => subscriptions,
        Err(error) => {
            tracing::warn!(%error, "could not inspect podcast refresh schedule");
            return ScopeStatus::default();
        }
    };
    let config = match reprise_core::podcasts::config::load(db) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, "could not read podcast refresh interval");
            return ScopeStatus { count: subscriptions.len(), due: false };
        }
    };
    let jitter = reprise_core::podcasts::refresh::jitter_seconds(&db_path.to_string_lossy());
    scope_status(
        &subscriptions,
        None,
        RefreshWindow::Hours { refresh_hours: config.refresh_hours, jitter_seconds: jitter },
        chrono::Utc::now().timestamp(),
    )
}
```

Beide Fehlerausgänge sind **wortgleich** die heutigen `(0, false)` und
`(subscriptions.len(), false)` — das ist die Stelle, an der still Verhalten
kippen könnte. Der Aufrufer (`:29-32`) wird:

```rust
let status = decision_inputs(&conn, &db_path);
if runtime.automatic_refresh_allowed(status.count, metered, status.due) {
    view.request_refresh(false);
}
```

Also: dieselbe Funktion, dieselben Argumente, dieselbe Semantik — nur die
Fälligkeitsrechnung liegt jetzt an einer Stelle statt an zwei. `kind: None`
und `RefreshPolicy::Due` bleiben, wie der Entwurf sagt.

**Done when:**
```bash
cargo test --locked -p reprise-gnome --bin reprise podcasts_refresh_decision   # ≥ 7 Tests, grün
cargo clippy --locked --all-targets -p reprise-gnome -- -D warnings
scripts/check-frontend-thinness.sh                                             # wie Nullmessung
```

---

### Task 6: `request_tab_open_refresh` mit den fünf Vorbedingungen und dem sichtbaren Knopf

**Files:**
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view_requests.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` (nur Sichtbarkeiten/Helfer)

**Interfaces:**
```rust
pub(in crate::ui) fn request_tab_open_refresh(self: &Rc<Self>) -> bool;
```

- [ ] **Schritt 1: die Vorprüfung**

```rust
pub(in crate::ui) fn request_tab_open_refresh(self: &Rc<Self>) -> bool {
    let network_allowed =
        podcasts::config::source_network_allowed(&self.conn, self.kind).unwrap_or(false);
    let metered = gio::NetworkMonitor::default().is_network_metered();
    let subscriptions = match podcasts::store::active_subscriptions(&self.conn) {
        Ok(subscriptions) => subscriptions,
        Err(error) => {
            tracing::warn!(%error, "could not inspect podcast subscriptions for a tab-open refresh");
            return false;
        }
    };
    let status = scope_status(
        &subscriptions,
        Some(self.kind),
        RefreshWindow::Seconds(TAB_OPEN_STALE_SECONDS),
        chrono::Utc::now().timestamp(),
    );
    if !tab_open_refresh_allowed(
        network_allowed,
        self.connectivity.get(),
        metered,
        self.refresh_in_flight.get() > 0,
        status,
    ) {
        return false;
    }
    self.request_refresh_with(RefreshRequest {
        policy: RefreshPolicy::StaleFor { seconds: TAB_OPEN_STALE_SECONDS },
        kind: Some(self.kind),
    })
}
```

Die fünf Vorbedingungen: (1) `source_network_allowed(self.kind)` — Modul **und**
globale Online-Quellen-Freigabe, eine Frage, die Core schon beantwortet; (2)
`self.connectivity.get() == Connectivity::Online`, geprüft in
`tab_open_refresh_allowed`; (3) `is_network_metered()`, wie beim Zeitplan; (4)
mindestens ein Abo **dieser Art** mit `last_fetch_at` älter als 15 Minuten; (5)
es läuft nicht schon ein Refresh (`refresh_in_flight == 0`) — im Grill
beschlossen, Begründung in Task 5.

**Die Verweigerung ist stumm.** Wenn irgendeine der fünf Bedingungen nicht
erfüllt ist, passiert *nichts Sichtbares*: kein Spinner, keine Statuszeile, kein
Text über das Alter der Daten, kein Layoutsprung. Auch das ist eine
Grill-Entscheidung — sichtbar wird nur echte Arbeit, und frische Daten sind der
Normalfall, der keine Bestätigung braucht. Schreib hier also keine „schon
aktuell"-Meldung in den Footer, auch nicht als Freundlichkeit.

**Punkt 4 ist der Grund, warum ein Tab-Wechsel nichts flackert.**
`request_refresh_with` startet den Footer-Spinner, sobald die Anfrage in der
Queue liegt — unabhängig davon, ob die Pipeline danach etwas zu tun findet.
Ohne die Vorprüfung würde jeder Tab-Wechsel einen Spinner für eine leere Runde
zeigen. Wenn du die Vorprüfung „vereinfachst", baust du genau diesen Fehler.

**Offline wird nichts vorgemerkt.** `request_load_more` (`:77-88`) stellt
offline auf `DeferredAction` um und meldet „später" — für einen Refresh ist das
falsch, weil ein offline geöffneter Tab Minuten später beim Reconnect
überraschend zu fetchen anfinge. Der Offline-Zustand hat mit
`should_show_offline_notice` schon seine eigene, richtige Anzeige. **Kein
`DeferredAction` in diesem Pfad.**

`gio` und `strings` sind über `use super::*;` (Zeile 3) schon da; ergänze nur,
was fehlt.

- [ ] **Schritt 2: der Knopf wird sichtbar beschäftigt — und in JEDEM Ausgang frei**

Zwei Helfer am View (in `podcasts_view_requests.rs`, damit `podcasts_view.rs`
nicht wächst):

```rust
fn begin_refresh_feedback(&self) {
    self.refresh_in_flight.set(self.refresh_in_flight.get().saturating_add(1));
    self.apply_refresh_button_state();
}

fn end_refresh_feedback(&self) {
    self.refresh_in_flight.set(self.refresh_in_flight.get().saturating_sub(1));
    self.apply_refresh_button_state();
}

fn apply_refresh_button_state(&self) {
    match refresh_button_state(self.refresh_in_flight.get()) {
        RefreshButtonState::Busy => { self.refresh_spinner.start(); /* Stack -> Spinner-Seite */ self.refresh_button.set_sensitive(false); }
        RefreshButtonState::Idle => { /* Stack -> Label-Seite */ self.refresh_spinner.stop(); self.refresh_button.set_sensitive(true); }
    }
}
```

Und — das ist der Kern der Aufgabe — die Freigabe hängt **nicht** an den
Match-Armen, sondern an einem Guard mit `Drop`:

```rust
struct RefreshFeedbackGuard(std::rc::Weak<PodcastsView>);

impl Drop for RefreshFeedbackGuard {
    fn drop(&mut self) {
        if let Some(view) = self.0.upgrade() {
            view.end_refresh_feedback();
        }
    }
}
```

`request_refresh_with` ruft nach erfolgreichem `queued`
`self.begin_refresh_feedback()` und **moved den Guard in das
`glib::spawn_future_local`-Future**. Damit wird der Knopf in *jedem* Ausgang
freigegeben:

- terminaler `Ok(Refreshed)`-Arm (`:44-54`),
- `Ok(LoadedMore)`-Arm (`:55-63`),
- **`Err`-Arm** (`:64-70`) — der Entwurf nennt ihn ausdrücklich: ein Knopf, der
  nach einem fehlgeschlagenen Fetch insensitiv bleibt, nimmt dem Nutzer genau
  die Handlung weg, die er dann braucht,
- **veraltete Generation** (`:28-30`, heute ein nacktes `return`),
- Kanal geschlossen, ohne dass eine terminale Antwort kam (die `while let`-Schleife
  endet),
- Future wird verworfen, weil das Fenster zugeht.

`glib::spawn_future_local` läuft im Main-Thread, `Weak<PodcastsView>` ist
deshalb in Ordnung. Der nicht-terminale
`Ok(DownloadState { … })`-Arm (`:32-43`) darf den Guard **nicht** fallen
lassen — er läuft in derselben Schleifeniteration weiter.

**Was du hier NICHT anfasst:** der Footer-Spinner
(`footer_spinner.start()`/`stop()`) und die Statuszeile
`strings::PODCAST_REFRESHING` bleiben genau, wo und wie sie heute sind. Dass
der Footer-Spinner im Pfad „veraltete Generation" heute nicht gestoppt wird,
ist ein Vorzustand, kein Auftrag dieses Plans. Ändere ihn nicht mit; erwähne ihn
im Bericht.

- [ ] **Schritt 3: der Zusammenhang zwischen Anzeige und Auslöser**

Beide Anzeigen (Footer und Knopf) hängen an „ein Refresh läuft", **nicht**
daran, wer ihn ausgelöst hat. Ein Fetch vom Zeitplan sieht damit genauso aus
wie einer vom Knopf oder vom Tab-Öffnen. Das ergibt sich automatisch, weil
`begin_refresh_feedback` in `request_refresh_with` sitzt und alle drei Wege
dort durchlaufen — keine Sonderbehandlung pro Auslöser einbauen.

**Done when:**
```bash
cargo test --locked -p reprise-gnome --bin reprise podcasts    # grün, ≥ 1 Test
cargo clippy --locked --all-targets -p reprise-gnome -- -D warnings
wc -l crates/reprise-gnome/src/ui/podcasts/podcasts_view*.rs   # alle < 800
```

---

### Task 7: Zwei Zeilen im Routing

**Files:**
- Modify: `crates/reprise-gnome/src/ui/window/library_shell.rs` (541 Zeilen)

- [ ] **Schritt 1: die beiden Zweige**

Im `sidebar.set_on_select`-Closure (`:192` ff.):

```rust
} else if matches!(source, ViewSource::Podcasts) {
    podcasts_view.refresh();
    podcasts_view.request_tab_open_refresh();          // neu
    super::window_navigation::show_content_page(&content_navigation, &content_stack, "podcasts");
} else if matches!(source, ViewSource::Youtube) {
    youtube_view.refresh();
    youtube_view.request_tab_open_refresh();           // neu
    super::window_navigation::show_content_page(&content_navigation, &content_stack, "youtube");
}
```

`refresh()` (die reine DB-Neuladung, `podcasts_view.rs:318`) bleibt **vor** dem
Fetch: der Nutzer sieht sofort den zwischenzeitlich gewachsenen Stand aus der
Datenbank, und der Netz-Fetch legt danach nach. Der Rückgabewert ist ein
`bool` ohne `#[must_use]`, wird also wie `request_refresh` nackt aufgerufen.

**Der Radio-Zweig (`:256-262`) bleibt unberührt.** Concerts und Releases
ebenfalls.

- [ ] **Schritt 2: Routing-Tests suchen und mitnehmen**

Auf `origin/dev` gibt es **keinen** Test, der diesen Podcast-Zweig direkt
fährt; die nächsten Verwandten sind
`crates/reprise-gnome/src/ui/sidebar/sidebar_source_tests.rs` und die
`navback_*`/`reveal_track_*`-Display-Tests der Track-Liste. Suche selbst und
ziehe mit, was du findest:

```bash
git grep -rn "ViewSource::Podcasts\|ViewSource::Youtube" -- crates/reprise-gnome/src
git grep -rln "library_shell" -- crates/reprise-gnome/src
```

Wenn dabei ein Test auffällt, der `set_on_select` fährt, ergänze ihn; wenn es
keinen gibt, ist die Abdeckung dieser zwei Zeilen die Kompilierung plus die
Entscheider-Tests aus Task 5 — schreib das so in den Bericht, statt einen
Beweis zu behaupten, den es nicht gibt.

**Done when:** `cargo check --locked -p reprise-gnome --all-targets` grün,
`wc -l crates/reprise-gnome/src/ui/window/library_shell.rs` < 600.

---

### Task 8: Der sichtbare Beweis am Knopf

Hausregel dieses Projekts: **grüne Tests beweisen keine UI.** Task 5 beweist
die Entscheidung, Task 8 beweist das Widget.

**Files:**
- Create: `crates/reprise-gnome/src/ui/podcasts/podcasts_refresh_button_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` (nur die
  Testmodul-Deklaration, ~4 Zeilen)

- [ ] **Schritt 1: das Testmodul anmelden**

`podcasts_view.rs` deklariert seine Testmodule mit `#[path]`-Attributen
(`:55-70`). Dazu kommt:

```rust
#[cfg(test)]
#[path = "podcasts_refresh_button_tests.rs"]
mod refresh_button_tests;
```

Nicht in `podcasts_view_tests.rs` schreiben — die Datei liegt bei 747 Zeilen.

- [ ] **Schritt 2: der Display-Test**

Muster: `podcasts_view_tests.rs:110-113` (`#[test]`, dann
`#[ignore = "requires a display; run via xvfb-run"]`, dann
`gtk4::init().unwrap()`). **Kein Regel-Präfix im Namen** (siehe Global
Constraints), also z. B.:

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_refresh_button_carries_a_spinner_while_a_fetch_runs_and_recovers_after_one() {
    gtk4::init().unwrap();
    // 1. View bauen
    // 2. begin_refresh_feedback():
    //    - refresh_button.is_sensitive() == false
    //    - refresh_stack.visible_child_name() == Spinner-Seite
    //    - refresh_spinner.is_spinning()
    // 3. zweimal begin, einmal end: immer noch beschäftigt (der Zähler trägt)
    // 4. letztes end: sensitiv, Label-Seite, Spinner steht
}
```

Wenn `begin_refresh_feedback`/`end_refresh_feedback` privat sind, gib ihnen
`pub(super)`-Sichtbarkeit statt eine Testhilfe zu erfinden, die am Produktivpfad
vorbeimisst.

- [ ] **Schritt 3: gezielt fahren, mit Namen aus `--list`**

```bash
cargo test -p reprise-gnome --bin reprise -- --list | grep refresh_button
xvfb-run -a cargo test --locked -p reprise-gnome --bin reprise \
  -- --ignored --exact ui::podcasts::podcasts_view::refresh_button_tests::the_refresh_button_carries_a_spinner_while_a_fetch_runs_and_recovers_after_one
```

Die Zahl vor `passed` muss ≥ 1 sein. `0 passed … filtered out` ist **kein**
grüner Lauf, sondern ein nicht gelaufener Test — dann stimmt der Modulpfad
nicht, hol ihn aus `--list`.

Wenn `xvfb-run` in dieser Umgebung fehlt: **nicht behaupten**, der Test sei
gelaufen. Schreib in den Bericht „geschrieben, nicht ausgeführt (kein Xvfb)"
und nenne den vollen Testnamen, damit der Mensch ihn mit einem Befehl fahren
kann. Kein App-Fenster öffnen, um es „von Hand zu sehen".

**Done when:** Der Test existiert, trägt den exakten Display-Marker, und der
Lauf ist entweder mit ≥ 1 `passed` dokumentiert oder als nicht ausführbar
begründet.

---

## Verifikation

In dieser Reihenfolge. Jede Stufe kann die nächste maskieren, also nach jeder
Reparatur die vorherige Stufe **neu** messen, nicht nur die reparierte. Lange
Ausgaben in eine Datei umleiten und die Frage per `grep`/`wc` beantworten —
nicht ganze Logs zurücklesen.

```bash
# 0. Formatierung — billigster Fehlschlag zuerst
cargo fmt --check
#    grün = keine Ausgabe, Exit 0

# 1. Der Sweep ist vollständig, wenn der ganze Workspace baut
cargo check --locked --workspace --all-targets > /tmp/v-check.log 2>&1; echo $?
#    grün = Exit 0. Das ist das einzige verlässliche Fertig-Kriterium für Task 3.

# 2. Lints — hier schlägt fehlendes `Copy` auf RefreshRequest zu
cargo clippy --locked --all-targets --workspace -- -D warnings > /tmp/v-clippy.log 2>&1; echo $?
grep -c "^error" /tmp/v-clippy.log
#    grün = Exit 0 und 0 Fehler

# 3. Core: die neue Politik und die migrierten Pfade
cargo test --locked -p reprise-core podcasts:: > /tmp/v-core.log 2>&1; echo $?
grep -c "^test result: FAILED" /tmp/v-core.log
grep -E "^test result:" /tmp/v-core.log
#    grün = 0 Zeilen "FAILED"; die Zahl vor "passed" muss > 0 sein.
#    Insbesondere müssen die sechs Due-Tests aus pipeline_refresh_tests.rs
#    ohne geänderte Erwartungen grün sein.

# 4. Die neuen Core-Tests liefen wirklich
cargo test --locked -p reprise-core refresh_policy_tests > /tmp/v-policy.log 2>&1
grep -E "^test result:" /tmp/v-policy.log
#    grün = ≥ 4 passed. "0 passed … filtered out" heißt: nicht gelaufen.

# 5. GTK: der Entscheider und die Podcast-Suite (NIE --lib!)
cargo test --locked -p reprise-gnome --bin reprise podcasts > /tmp/v-gnome.log 2>&1; echo $?
grep -c "^test result: FAILED" /tmp/v-gnome.log
grep -E "^test result:" /tmp/v-gnome.log
#    grün = 0 "FAILED" und > 0 passed. Fehlt die "test result:"-Zeile ganz,
#    ist der Lauf INCONCLUSIVE (z. B. Zielname falsch) und gilt als rot.

# 6. Der ganze Workspace, so wie das Merge-Gate ihn fährt
tmp=$(mktemp -d)
env XDG_DATA_HOME="$tmp/data" XDG_CACHE_HOME="$tmp/cache" REPRISE_AUDIO_SINK=fakesink \
  cargo test --locked --workspace --exclude reprise-platform-linux > /tmp/v-workspace.log 2>&1; echo $?
grep -c "^test result: FAILED" /tmp/v-workspace.log
#    grün = 0. Jede Rotstelle gegen /tmp/base-*.log aus Task 0 prüfen,
#    bevor du sie dir zuschreibst.

# 7. Architektur: keine Datei >= 800 Zeilen
scripts/check-architecture.sh > /tmp/v-arch.log 2>&1; echo $?
#    grün = Exit 0. Bei Rot: welche Datei? Wenn es eine der vier fett
#    gesetzten aus den Global Constraints ist, wandert Inhalt in eine neue
#    kleine Datei — Budgets werden nicht "erklärt".

# 8. Frontend-Thinness: Budgets sind Decke UND Boden
scripts/check-frontend-thinness.sh > /tmp/v-thin.log 2>&1; echo $?
diff /tmp/base-thinness.log /tmp/v-thin.log
#    grün = identisch zur Nullmessung. Eine NEUE Meldung über rusqlite,
#    workers oder .conn( kommt aus deinem Code — behebe sie im Code, nicht
#    im Skript. Eine Meldung, die schon in der Nullmessung stand, gehört
#    dir nicht.

# 9. UX-Traceability
scripts/check-ux-traceability.sh > /tmp/v-ux.log 2>&1; echo $?
diff /tmp/base-ux.log /tmp/v-ux.log
#    grün = identisch zur Nullmessung. "test references unknown rule" heißt:
#    der Display-Test aus Task 8 trägt versehentlich einen Regel-Präfix.

# 10. Der Display-Beweis am Knopf (Task 8) — einzeln, nie die Suite am Stück
cargo test -p reprise-gnome --bin reprise -- --list | grep refresh_button
xvfb-run -a cargo test --locked -p reprise-gnome --bin reprise \
  -- --ignored --exact <voller::Modulpfad::aus::--list> 2>&1 | grep -E "^test result:"
#    grün = "1 passed". Ohne Xvfb: als nicht ausführbar dokumentieren,
#    nicht als grün.

# 11. Die vorhandenen Podcast-Display-Tests gegen die Nullmessung
xvfb-run -a cargo test --locked -p reprise-gnome --bin reprise \
  -- --ignored ui::podcasts:: > /tmp/v-display.log 2>&1
diff <(grep -E "^test .* (ok|FAILED)" /tmp/base-display.log | sort) \
     <(grep -E "^test .* (ok|FAILED)" /tmp/v-display.log  | sort)
#    grün = keine Zeile wechselt von "ok" nach "FAILED". Die zwei bekannt
#    roten src_14_*-Tests bleiben rot und sind nicht deine Schuld.
```

**Nicht** ausführen: `scripts/check-merge-readiness.sh` am Stück (läuft in
diesem Projekt praktisch nie durch und kostet Stunden), kein `cargo audit`,
kein Emulator, kein App-Fenster.

## Abnahme

Die Änderung ist fertig, wenn **alles** davon zutrifft:

1. Ein Wechsel auf den Podcasts-Tab kann keinen einzigen yt-dlp-Prozess
   starten — bewiesen durch
   `rss_scope_never_touches_a_youtube_subscription` (Aufrufzähler == 0 **und**
   unberührtes `last_fetch_at` der Kontrollzeile) und
   `scope_status_ignores_a_stale_subscription_of_another_kind`.
2. `Due` und `Force` verhalten sich unverändert — bewiesen durch die 30
   migrierten Tests mit **unveränderten** Erwartungen.
3. `StaleFor` respektiert den Retry-Backoff und kennt keinen Jitter.
4. Ein Tab-Wechsel ohne fälliges Abo zeigt **keinen** Spinner und stellt
   **keine** Anfrage — bewiesen durch die Vorprüfung und
   `tab_open_refuses_offline_metered_disabled_empty_fresh_and_already_running`.
   Dasselbe gilt, während ein Refresh schon läuft: kein zweiter Anlauf, keine
   leere Runde.
5. Der Refresh-Knopf ist während jedes Fetches insensitiv mit Spinner und
   danach in **jedem** Ausgang wieder frei — inklusive `Err` und veralteter
   Generation, strukturell garantiert durch den `Drop`-Guard, nicht durch
   aufgezählte Match-Arme.
6. Offline wird nichts vorgemerkt; kein `DeferredAction` im Refresh-Pfad.
7. Radio, Concerts und Releases sind unberührt; kein Schema, keine Migration,
   keine neue Einstellung, kein neuer nutzersichtbarer String.
8. Alle Verifikationsstufen sind grün **oder** nachweislich schon in der
   Nullmessung aus Task 0 rot gewesen — und der Abschlussbericht sagt für jede
   Rotstelle, welche der beiden Fälle es ist.
