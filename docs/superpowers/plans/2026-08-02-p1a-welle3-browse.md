---
slug: p1a-welle3-browse
worktree: /home/marvin/Projects/reprise-p1a-welle3-browse
branch: feature/p1a-welle3-browse
phase: planned
codex_session:
created: 2026-08-02
---
# P1a Welle 3 — Browse: wo die Wahl des Textes selbst Logik ist

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Sichtbarkeitsregeln der Filterzeile und der Filter-Katalog ziehen
nach `reprise-view`. Neu gegenüber Welle 1 und 2: hier hängt die **Auswahl**
eines msgid von Daten ab, und ein Rückgabewert ist teilweise
oberflächenspezifisch.

**Basis:** `feature/p1a-welle2-lyrics` (#214).

**Spec:** `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md` — insbesondere
§2.5, die Neuvermessung, die diesen Zuschnitt begründet.

## Der Zuschnitt, neu gemessen (2026-08-02)

Der Wellenplan nennt für Welle 3 „`cover`, `browse`, `scan` (~3.200 LOC),
untereinander unabhängig, daher parallelisierbar". **Beides hält der Messung
nicht stand.**

| Datei | LOC | Urteil |
| --- | --- | --- |
| `browse/filter_restriction.rs` | 199 | **zieht um** |
| `browse/browse_filter_strings.rs` | 146 | **zieht um** |
| `browse/browse_bar_chips.rs` | 170 | bleibt — `use gtk4::prelude::*` |
| `browse/browse_filter_count.rs` | 136 | bleibt — nimmt `&Rc<BrowseBar>`, ein Widget |
| `cover/cover_download_worker.rs` | 438 | bleibt — eigener Thread, `Rc<Cell<bool>>` |
| `scan/scan_card_css.rs` | 48 | bleibt — ist wörtlich GTK-CSS |
| `cover/mod.rs`, `browse/mod.rs`, `scan/mod.rs` | 22 | bleiben, verdrahten nur |

**Welle 3 bewegt damit 345 LOC in zwei Dateien, alle in `browse`.** Nicht
3.200, und nicht parallelisierbar — es gibt nur einen Bereich mit Inhalt.
`cover` und `scan` tragen zusammen sechs bewegliche Zeilen.

Die drei Ausschlüsse haben drei verschiedene Gründe, und keiner davon ist „es
steht `gtk` drin":

1. `browse_bar_chips.rs` importiert `gtk4::prelude` — die alte Messung mit
   Wortgrenze auf `gtk` hat das nie getroffen (§2.5, Vorbehalt 1).
2. `browse_filter_count.rs` nimmt `&Rc<BrowseBar>` entgegen. `BrowseBar` ist
   ein Widget ohne das Wort `gtk` im Typnamen — §2.3 in Reinform.
3. `cover_download_worker.rs` besitzt einen Thread, wie `lyrics_worker.rs` in
   Welle 2. Der Hausbefund gilt: Worker behalten ihren Thread in
   `reprise-gnome`.

## Was diese Welle neu lernt

### N1 — Die Auswahl des msgid ist Logik und gehört nach `reprise-view`

`result_count(filtered, total)` wählt heute zwischen **zwei msgid-Paaren**:
`"{total} track"/"{total} tracks"` wenn nichts gefiltert ist, sonst
`"{filtered} of {total} track"/"…tracks"`. Diese Entscheidung ist keine
Übersetzung, sondern eine Regel über Daten — sie muss auf Android dieselbe
sein. Sie zieht mit, und `Message` trägt sie: die Funktion gibt das gewählte
`id`/`plural`/`args`-Tripel zurück, nicht den gerenderten Text.

Das ist der erste Fall, in dem `Message` mehr tut, als einen konstanten msgid
weiterzureichen — und der Grund, warum der Typ ein Wert und keine Konstante
ist.

### N2 — Markup ist oberflächenspezifisch und quert die Grenze nicht

`result_count_markup` gibt `(String, bool)` zurück; der `String` enthält
Pango-Markup. Pango ist GTK. Der `bool` („ist eine Einschränkung aktiv?") ist
dagegen eine reine Datenaussage und gehört in die geteilte Schicht.

**Der Schnitt liegt zwischen den beiden Rückgabewerten**, nicht vor der
Funktion: `reprise-view` liefert die Aussage, `reprise-gnome` macht daraus
Markup. Ein Android-Compose-Aufrufer würde denselben `bool` bekommen und
`AnnotatedString` daraus bauen.

### N3 — `place_pill_label` übersetzt gar nicht

Es formatiert nur `ViewSource`-Felder (`"{album} — {album_artist}"`). Kein
msgid, kein `Message` — es zieht unverändert um. Wert der Feststellung: nicht
jede Funktion, die einen `String` zurückgibt, ist Übersetzungsarbeit.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe lesen.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Bekannt rot und nicht von dieser Welle:**
  `scripts/tests/gettext-catalogs.sh` scheitert auf jedem Branch an einem
  fehlenden `po/ar.po`-Eintrag für `"Play this track"`. Vor dem Commit gegen
  die Basis gegenprüfen, dass es **derselbe eine** Fehler ist.
- **Baseline:** 3914 passed, 0 failed, 410 ignored, 56 Suiten (Welle 2).
- **Keine Aufrufstelle im `ui`-Baum ändert sich.**
- **Kein `#[allow(…)]`** gegen eine Warnung des eigenen Umbaus.
- **`view_floor` steigt im selben Commit** (steht bei 224).

---

## Task 1: `filter_restriction.rs` — die Sichtbarkeitsregeln

199 LOC, sechs Funktionen, zehn Aufrufstellen, einzige Abhängigkeit
`reprise_core`. Es trägt die FIL-1a/FIL-2-Regeln aus `docs/ux-rules.md` K.

**Files:**
- Create: `crates/reprise-view/src/browse.rs`
- Modify: `crates/reprise-view/src/lib.rs`
- Modify: `crates/reprise-gnome/src/ui/browse/mod.rs`
- Delete: `crates/reprise-gnome/src/ui/browse/filter_restriction.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die sechs Funktionen umziehen**

`filters_restrict`, `has_place_pill`, `is_restricted`, `place_pill_label`
(N3 — zieht unverändert), `is_track_source`, `row_visible` samt ihren Tests.
Sichtbarkeiten werden `pub`.

- [ ] **Step 3: Die `Send + Sync`-Zusicherung**

Wie in Welle 2 für `LyricsState`: eine `const`-Zusicherung, die einen späteren
`Rc` unmöglich macht. Sie ist nur dann etwas wert, wenn sie beweisbar greift —
also einmal mutieren (ein `Rc<()>`-Feld einführen), den Compile-Fehler sehen,
zurücknehmen, und beides in der Commit-Nachricht belegen.

- [ ] **Step 4: Die Naht in `reprise-gnome`**

Re-Export nach Welle-1-Muster, sodass alle zehn Aufrufstellen unverändert
auflösen.

- [ ] **Step 5: `view_floor` anheben, volle Gates, Commit**

---

## Task 2: Der Filter-Katalog, beide Mechanismen in einer Datei

`browse_filter_strings.rs` ist die erste Datei, in der argumentfreie Labels
(Welle-2-Regel) und Texte mit Argumenten (Welle-1-Regel) nebeneinander
liegen. Sie beweist, dass die Grenze zwischen beiden trägt.

**Files:**
- Create: `crates/reprise-view/src/strings/browse.rs`
- Modify: `crates/reprise-view/src/strings.rs`
- Modify: `crates/reprise-gnome/src/ui/browse/browse_filter_strings.rs` (Adapter)
- Modify: `po/POTFILES.in`

- [ ] **Step 1: Die argumentfreien Konstanten**

`FILTERS`, `ADD_FILTER`, `CLEAR_ALL`, `BACK`, `SEARCH_VALUES`,
`NO_FILTERS_AVAILABLE`, `BROWSE_GENRE`, `BROWSE_ARTIST` und die weiteren
ziehen als `pub const` msgid (Welle-2-Regel). Das lokale `N_!`-Duplikat
entfällt.

- [ ] **Step 2: Die Funktionen mit Argumenten**

`chip_label`, `remove_filter_label`, `search_chip_label`, `remove_search_label`
und `leave_place_label` geben `Message` zurück. Der GTK-Adapter rendert.

- [ ] **Step 3: `result_count` mit seiner Auswahl (N1)**

Die Verzweigung zwischen den zwei msgid-Paaren zieht mit. `format_thousands`
kommt aus `reprise_core::format` und ist erlaubt. Achtung auf die vorhandenen
`try_from(...).unwrap_or(MAX)`-Sättigungen: sie ziehen unverändert mit, damit
der Umzug verhaltensgleich bleibt — sie zu „reparieren" wäre eine andere
Änderung und gehört nicht hierher.

- [ ] **Step 4: `result_count_markup` aufteilen (N2)**

`reprise-view` liefert `Message` plus den `bool`; das Pango-Markup baut der
GTK-Adapter. Der bestehende Kommentar („Numbers are digits and commas only, so
the inserted count is markup-safe") ist eine **Sicherheitsaussage** und muss
auf der Seite stehen bleiben, die das Markup zusammensetzt — dort gilt sie.

- [ ] **Step 5: Übersetzungsnachweis**

Abbruchbedingung wie in Welle 1 und 2: jeder msgid vor und nach dem Umzug
zeichengleich, alle in `de`, `es`, `fr`, `ar`, `bn`, `hi` weiterhin vorhanden.
Hier besonders zu prüfen, weil die Plural-msgids (`"{total} track"` /
`"{total} tracks"`) bisher **inline im Funktionsrumpf** stehen und nicht als
Konstante — beim Umzug dürfen sie sich nicht ändern.

- [ ] **Step 6: `view_floor` anheben, volle Gates, Commit**

---

## Task 3: Das Muster festschreiben

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: N1 und N2 als Regeln in §4**

- [ ] **Step 2: Welle 3 in §3 auf den gemessenen Zuschnitt korrigieren**

Der Eintrag verspricht drei Bereiche und Parallelisierbarkeit. Beides ist
widerlegt. Der Text wird korrigiert, nicht gelöscht — mit dem Grund, damit
Welle 4 bis 7 nicht dieselbe Erwartung erben.

- [ ] **Step 3: Ledger-Eintrag**

- [ ] **Step 4: Volle Gates und Commit**

---

## Nach dieser Welle

Welle 4 (`now_playing`, `player_bar`) hat nach §2.5 zusammen **424 wirklich
bewegliche LOC** — `now_playing` trägt 64, `player_bar` 360. Der Wellenplan
nennt ~2.400. Vor dem Start gilt Regel 1 erneut.
