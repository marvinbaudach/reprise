---
slug: p1a-welle2-lyrics
worktree: ~/Projects/reprise-p1a-welle2-lyrics
branch: feature/p1a-welle2-lyrics
phase: refactored
codex_session:
created: 2026-08-02
---
# P1a Welle 2 — Lyrics: die erste echte Zustandslogik

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `LyricsState` — die Zustandsmaschine hinter der Lyrics-Oberfläche —
zieht nach `reprise-view`. Es ist der erste Umzug mit echtem Zustand statt
reiner Werte, und er beweist die Muster aus Welle 1 an etwas, das mehr kann
als eine Zeichenkette zu formatieren.

**Architecture:** Wie Welle 1: `reprise-view` bekommt den Bereich, in
`reprise-gnome` bleibt eine Adapterdatei, Aufrufstellen ändern sich nicht.

**Basis:** `feature/p1a-welle1-umzugsmechanik` bzw. `dev`, sobald Welle 1 und
P0 dort gelandet sind. Der Wellenplan verlangt vor dem Start eine neue Messung
— die unten steht, gemessen am 2026-08-02.

**Tech Stack:** Rust 1.92, Crates `reprise-gnome`, `reprise-core`, `reprise-view`.

**Spec:** `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md`
**Muster aus Welle 1:** `docs/superpowers/plans/2026-08-02-p1a-welle1-umzugsmechanik.md`

## Der Zuschnitt, neu gemessen (2026-08-02)

Der Wellenplan nennt für Welle 2 „`lyrics`, 8 Dateien, ~1.560 LOC". Gemessen
sind es heute **14 Dateien mit 3.468 LOC** — #206 hat den Lyrics-Batch nach
`reprise-core` verschoben und dabei die Dateiaufteilung verändert. Regel 1 des
Wellenplans („Dateiliste beim Wellenstart neu messen") war hier nicht
formal, sondern nötig.

| Datei | LOC | Toolkit-Bezüge | Urteil |
| --- | --- | --- | --- |
| `lyrics/lyrics_state.rs` | 263 | 0 | **zieht um** — der Kern dieser Welle |
| `lyrics/lyrics_strings.rs` | 31 | 0 | **zieht um** |
| `lyrics/lyrics_worker.rs` | 203 | 0 | **bleibt** — siehe unten |
| `lyrics/mod.rs` | 16 | 0 | bleibt, verdrahtet nur |
| `lyrics/lyrics_batch_tests.rs` | 74 | 0 | folgt seinem Subjekt, nicht dieser Welle |
| `lyrics/lyrics_batch_progress_tests.rs` | 101 | 0 | dito |
| `lyrics/lyrics_view.rs` | 747 | 6 | bleibt |
| `lyrics/player_lyrics.rs` | 411 | 4 | bleibt, wird Aufrufer |
| `lyrics/lyrics_scroll.rs` | 293 | 3 | bleibt |
| `lyrics/lyrics_smoke.rs` | 124 | 9 | bleibt |
| `lyrics/lyrics_view_tests.rs` | 513 | 2 | bleibt |
| `lyrics/player_lyrics_tests.rs` | 349 | 5 | bleibt |
| `lyrics/lyrics_batch.rs` | 241 | 2 | bleibt |
| `lyrics/lyrics_batch_progress.rs` | 102 | 2 | bleibt |

### Warum `lyrics_worker.rs` nicht mitzieht, obwohl die Heuristik es freigibt

Es enthält kein `gtk`, `adw` oder `glib` — und ist trotzdem unbeweglich:

- `std::thread::Builder::new().spawn(…)` in `from_lookup` — es **besitzt einen
  Thread**.
- `Rc<Self>` als Rückgabe von `setup` — es ist **weder `Send` noch `Sync`**.

Das ist §2.3 des Wellenplans in Reinform („Die Heuristik ist nicht der
Schnitt"). Zusätzlich gilt der bereits im Ledger festgehaltene Hausbefund aus
`lyrics-batch-to-core`: **jeder Worker in diesem Repository behält den Thread
in `reprise-gnome`** und konsumiert eine synchrone, rückrufgetriebene Funktion
aus dem Kern. Der Worker gehört damit nicht in `reprise-view`, sondern bleibt
Adapter — genau wie `playlist_io.rs` in Welle 1.

**Nebenwirkung, die dieser Welle fehlt:** Weil der Worker bleibt, senkt Welle 2
weder das `workers`- (7 Dateien) noch das `threads`-Budget (15). Die
Fortschrittsmessung läuft wie in Welle 1 über `view_floor`.

## Die Entscheidung, die diese Welle trifft

### V1 — Argumentfreie Labels queren als msgid, nicht als `Message`

Welle 1 hat `Message` für Texte mit Platzhaltern und Pluralformen etabliert
und `RETRY` als **Ausnahme** durchgelassen: ein Label ohne Argumente, das die
Oberfläche direkt benennt. `lyrics_strings.rs` besteht aus **18 solchen
Labels** — „Lyrics", „Instrumental", „No lyrics found", „synced · .lrc" …
Keines hat einen Platzhalter, keines eine Pluralform.

Achtzehn Funktionen zu schreiben, die je ein `Message { id, plural: None,
args: vec![] }` zurückgeben, wäre Zeremonie ohne Gegenwert. Deshalb wird aus
der Welle-1-Ausnahme eine Regel:

> **Ein Text ohne Platzhalter und ohne Pluralform quert die Crate-Grenze als
> `pub const` msgid. Sobald er ein Argument oder eine Pluralform hat, quert er
> als `Message`.** Beide Wege enden in derselben `.po`-Datei; die Oberfläche
> ruft für den einen ihr `text(msgid)`, für den anderen ihren `render`.

Die Grenze ist prüfbar und nicht Geschmackssache: Ein `Message` ohne `args`
und ohne `plural` trägt keine Information, die der msgid nicht schon trägt.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`.
- **Exit-Codes in der Commit-Nachricht festhalten** — nicht die Ausgabe durch
  `tail` oder eine Pipe schicken, sonst berichtet die Pipe ihren eigenen
  Erfolg. (In Welle 1 ist genau das zweimal passiert, einmal bei Codex und
  einmal bei mir.)
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Baseline** in Task 1 messen. Referenz aus Welle 1: 3912 passed, 0 failed,
  410 ignored, 56 Suiten.
- **Keine Aufrufstelle im `ui`-Baum ändert sich.**
- **Kein `#[allow(…)]`, um eine Warnung des eigenen Umbaus stumm zu stellen.**
  Ist ein Export ungenutzt, wird er gelöscht, nicht erlaubt.
- **`view_floor` steigt im selben Commit**, der Code bewegt.

---

## Task 1: `LyricsState` nach `reprise-view`

Der eigentliche Umzug. 263 Zeilen Zustandsmaschine, null `Rc`, null `RefCell`,
null Closures — sie erfüllt das Welle-0-Muster bereits, ohne dass jemand sie
dafür umbauen musste.

**Files:**
- Create: `crates/reprise-view/src/lyrics.rs`
- Modify: `crates/reprise-view/src/lib.rs`
- Modify: `crates/reprise-gnome/src/ui/lyrics/mod.rs`
- Delete: `crates/reprise-gnome/src/ui/lyrics/lyrics_state.rs`
- Modify: `crates/reprise-gnome/src/ui/lyrics/player_lyrics.rs` (Import)
- Modify: `crates/reprise-gnome/src/ui/lyrics/player_lyrics_tests.rs` (Import)

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die Zustandsmaschine umziehen**

`LyricsTrack`, `RequestIntent`, `LyricsState` samt ihren Tests wandern
unverändert. Sichtbarkeiten `pub(in crate::ui)` → `pub` (Welle-1-Regel).
`reprise_core::lyrics` ist die einzige Abhängigkeit und erlaubt.

- [ ] **Step 3: Die `Send + Sync`-Zusicherung mitgeben**

Welle 0 hat für `QueueViewModel` eine `const`-Zusicherung auf `Send + Sync`
eingeführt, die Closures dauerhaft draußen hält. `LyricsState` erfüllt sie
heute schon — die Zusicherung gehört trotzdem dazu, sonst kann ein späterer
Commit ein `Rc` einführen, ohne dass etwas widerspricht.

- [ ] **Step 4: Die Naht in `reprise-gnome`**

`lyrics/mod.rs` bekommt den Re-Export nach Welle-1-Muster, sodass
`super::lyrics_state::{LyricsState, LyricsTrack, RequestIntent}` in
`player_lyrics.rs` und `player_lyrics_tests.rs` **unverändert** auflöst.

- [ ] **Step 5: `view_floor` anheben, volle Gates, Commit**

---

## Task 2: Der Lyrics-Katalog

**Files:**
- Create: `crates/reprise-view/src/strings/lyrics.rs`
- Modify: `crates/reprise-gnome/src/ui/lyrics/lyrics_strings.rs` (wird Adapter)
- Modify: `po/POTFILES.in`

- [ ] **Step 1: Die 18 msgids umziehen**

Nach V1 als `pub const`. `text()` bleibt in `reprise-gnome` — es ruft gettext.

- [ ] **Step 2: Das `N_!`-Makro nicht doppeln**

`lyrics_strings.rs` definiert heute sein **eigenes** `N_!`, obwohl `strings.rs`
bereits eins hat und `reprise-view` seit Welle 1 auch. Beim Umzug wird das
Duplikat entfernt, nicht mitgenommen.

- [ ] **Step 3: Übersetzungsnachweis**

Wie in Welle 1 die Abbruchbedingung: die **18 msgids müssen vor und nach dem
Umzug zeichengleich** sein, sonst verwaisen die Einträge in `de.po`, `es.po`,
`fr.po`, `ar.po`, `bn.po` und `hi.po`. Zeichengleichheit prüfen, nicht
annehmen — `synced · .lrc` enthält ein U+00B7, das eine unachtsame Umschrift
still zu einem ASCII-Punkt macht.

- [ ] **Step 4: `RETRY` entdoppeln — oder begründet stehenlassen**

`lyrics_strings.rs` trägt ein eigenes `RETRY: &str = N_!("Retry")`, und
`strings_scan.rs` trägt seit Welle 1 dasselbe. Zwei Konstanten, ein msgid, ein
Katalogeintrag. Ob sie zusammenfallen oder bewusst getrennt bleiben, entscheidet
dieser Task an den Aufrufstellen — **beides ist vertretbar, stillschweigend
beides zu behalten nicht.**

- [ ] **Step 5: `view_floor` anheben, volle Gates, Commit**

---

## Task 3: Das Muster festschreiben

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: V1 als Regel in §4 des Wellenplans**

- [ ] **Step 2: Den Worker-Befund in §2.3 nachtragen**

`lyrics_worker.rs` ist nach `window/source_views.rs` der zweite belegte Fall,
in dem die Toolkit-Heuristik eine Datei freigibt, die nicht ziehen kann — hier
nicht wegen eines versteckten Widgets, sondern wegen Thread-Besitz und `Rc`.
Die Gegenprobe jeder Welle heißt damit nicht mehr nur „hält der Typ transitiv
ein Widget?", sondern zusätzlich „ist er `Send + Sync`, und wem gehört der
Thread?".

- [ ] **Step 3: Ledger-Eintrag**

- [ ] **Step 4: Volle Gates und Commit**

---

## Nach dieser Welle

Welle 3 (`cover`, `browse`, `scan`, ~3.200 LOC) ist die erste, die laut
Wellenplan parallelisierbar ist. Vor ihrem Start gilt Regel 1 erneut: neu
messen. Der Verdacht aus dieser Welle ist, dass auch dort Dateien liegen, die
die Heuristik freigibt und der Thread-Besitz zurückhält — `scan` hat einen
Worker.
