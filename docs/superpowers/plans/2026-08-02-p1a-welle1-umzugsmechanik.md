---
slug: p1a-welle1-umzugsmechanik
worktree: /home/marvin/Projects/reprise-p1a-welle1-umzugsmechanik
branch: feature/p1a-welle1-umzugsmechanik
phase: refactored
codex_session:
created: 2026-08-02
---
# P1a Welle 1 — Die Umzugsmechanik etablieren

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zum ersten Mal zieht Code aus `reprise-gnome` nach `reprise-view` —
und zwar so wenig, dass ein Fehlschlag nichts mitreißt. Am Ende stehen der
Modulpfad, die Sichtbarkeitsregel, die Konsum-Naht, das Übersetzungsformat
und ein Fortschrittsmaß, auf die sich alle sechs Folgewellen berufen.

**Architecture:** `reprise-view` bekommt zwei Bereiche: `playlists` (reine
Werte) und `strings::scan` (Katalog). `reprise-gnome` behält an beiden Stellen
eine dünne Adapterdatei, damit **keine einzige Aufrufstelle im Frontend sich
ändert** — die Naht liegt in der Adapterdatei, nicht verstreut im `ui`-Baum.

**Basis:** `feature/multi-surface-frontends` (PR #212), **nicht** `dev` — die
Crate `reprise-view` und ihr Gate liegen dort. Erst wenn #212 gelandet ist,
darf diese Welle auf `dev` rebased werden.

**Tech Stack:** Rust 1.92, bestehende Crates `reprise-gnome`, `reprise-core`,
`reprise-view`; gettext über `po/POTFILES.in`.

**Spec:** `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md`
**Muster aus Welle 0:** `docs/superpowers/plans/2026-08-01-p1a-welle0-queue-naht.md`

## Der Zuschnitt, neu gemessen (2026-08-02)

Der Wellenplan nannte Welle 1 „die billigsten Dateien". Gemessen gegen
`feature/multi-surface-frontends` stimmt das nur teilweise, deshalb dieser
engere Schnitt:

| Datei | LOC | Abhängigkeiten | Urteil |
| --- | --- | --- | --- |
| `ui/playlists/playlist_import_navigation.rs` | 15 | nur `reprise_core::view_source` | zieht um |
| `ui/playlists/playlist_io_names.rs` | 29 | `reprise_core::models` **und** `super::strings` | zieht um, nach Task 3 |
| `ui/strings_scan.rs` | 37 | `super::{formatted, plural, text}` | zieht um |
| `ui/playlists/playlist_io.rs` | 759 | 20 Toolkit-Bezüge | **bleibt** |
| `ui/strings.rs` | 795 | **1.731 Aufrufstellen** im `ui`-Baum | **bleibt**, eigene Welle |

**Warum `strings.rs` nicht in dieser Welle liegt:** Es ist das Modul mit dem
höchsten Fan-in des gesamten Frontends; nur 372 seiner Aufrufstellen liegen
überhaupt im Mobil-Zuschnitt. Die Mechanik am meistberührten Modul zu erproben
ist das Gegenteil von dem, wozu Welle 1 da ist.

**Warum `strings_scan.rs` trotzdem hinein muss:** `strings.rs` re-exportiert
seine Geschwister per Glob (`#[path = "strings_scan.rs"] mod scan; pub use
scan::*;`). Dadurch hat allein die Konstante `RETRY` aus dieser Datei
**4 Aufrufstellen** außerhalb ihrer eigenen Datei. Genau daran hängt die Frage,
die jede Folgewelle stellt: Wie konsumiert `reprise-gnome` die neue Crate, ohne
dass Aufrufstellen brechen? Eine Datei ohne diese Eigenschaft würde die Frage
nicht beantworten.

> **Korrektur (2026-08-02, nach der Umsetzung):** Hier stand zuerst „26
> Aufrufstellen". Das war eine Substring-Zählung, die `INLINE_RETRY_CLASS`,
> `RADIO_NO_CONNECTION_RETRY`, `PODCAST_RETRY_DOWNLOAD` und weitere mitzählte.
> `\bRETRY\b` ergibt **4**. Am Zuschnitt ändert das nichts — die Re-Export-Naht
> ist bei 4 dieselbe Frage wie bei 26 —, aber die Zahl war falsch.

## Entschiedene Vorfragen

### V1 — Spec-Punkt O2: `reprise-view` liefert Bausteine, keine fertigen Texte

**Entscheidung (2026-08-02, Eigentümer):** ViewModels geben `msgid` plus
benannte Argumente als **Wert** heraus; die Übersetzung macht jede Oberfläche
selbst — GTK per `gettext`/`ngettext`, Android per `strings.xml`/`plurals`.

Begründung und Gegenprobe: gettext *wäre* auf Android baubar — gemessen in
`docs/research/android-spike-2026-08.md` §Frage 6: statisch gelinkt, keine
externe Bibliothek, **+24.080 Bytes je ABI**. Die Entscheidung fällt also
nicht am Build, sondern am Übersetzer-Workflow: Play-Store- und
Compose-Werkzeuge erwarten `strings.xml`, und gettext läse zur Laufzeit vom
Dateisystem, wofür die `.mo`-Kataloge beim ersten Start aus dem APK
ausgepackt werden müssten. **Folge:** `reprise-view` nimmt `gettext-rs`
nicht auf und bleibt bei genau einer Kante — `reprise-core`.

### V2 — Regel 4 des Wellenplans ist für diese Welle nicht erfüllbar

Der Wellenplan verlangt: „`check-frontend-thinness.sh` senkt in jeder Welle
mindestens ein Budget." Das Gate führt vier Budgets — `rusqlite` 112,
`filesystem` 19, `threads` 15, `workers` 7 — plus die Verbote `gstreamer`,
`zbus`, `db_handle_access`. **Keins davon wird von verschobenen Strings oder
reinen Werten berührt.** Die Regel misst die Thin-Core-Migration, nicht P1a.

**Auflösung:** Statt die Regel zu beugen bekommt P1a sein eigenes,
mechanisches Maß (Task 1) — eine **Untergrenze** für die Produktionszeilen in
`reprise-view`, die jede Welle im selben Commit anhebt. Das spiegelt die
Philosophie des bestehenden Gates („ceiling AND floor") und macht den
Fortschritt von P1a zum ersten Mal prüfbar statt behauptet.

**Ausdrücklich nicht gewählt:** ein Budget über „toolkit-freie LOC, die noch
in `reprise-gnome` liegen". §2.3 des Wellenplans weist selbst nach, dass die
Toolkit-Heuristik den Schnitt nicht bestimmt (`window/source_views.rs` zählt
als toolkit-frei, hält aber GTK-Objekte). Ein Gate auf einer Heuristik, der
der Plan selbst misstraut, wäre ein Gate, das niemand glaubt.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Test-Baseline** in Task 1 messen; jeder Task nennt sie als Referenz.
  Referenzwert vom 2026-08-02 auf `feature/p1a-welle0-queue-naht`:
  3905 passed, 0 failed, 410 ignored, 54 Suiten.
- **Keine Aufrufstelle im `ui`-Baum ändert sich.** Wird das an einer Stelle
  unvermeidbar, ist das ein Befund für den Plan — nicht ein Commit nebenbei.
- **Kein ViewModel hält einen Closure** (Welle-0-Muster, Regel 6).
- **Bekannte Sandbox-Fehlschläge:** display-gebundene GTK-Tests,
  MCP-Radio-Socket-Bind, `ReadOnlyFilesystem` in Cover-/Cache-Tests.

---

## Task 1: Das Fortschrittsmaß, bevor etwas umzieht

Ein Maß, das erst nach dem Umzug entsteht, misst nichts. Es kommt zuerst und
wird an der leeren Crate rot gemacht.

**Files:**
- Modify: `scripts/check-frontend-thinness.sh`
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`

- [ ] **Step 1: Baseline messen und festhalten**

`cargo test --workspace` einmal unverändert laufen lassen und die Zahlen
notieren. Ebenso die heutigen Produktionszeilen von `reprise-view`
(`crates/reprise-view/src`, Kommentarzeilen abgezogen wie im Skript üblich).

- [ ] **Step 2: Die Untergrenze einbauen**

In `scripts/check-frontend-thinness.sh` einen Abschnitt `== Shared view layer
==` ergänzen, der die Produktionszeilen in `crates/reprise-view/src` zählt und
gegen `view_floor` prüft. Fehlschlag, wenn die Zahl **unter** der Grenze
liegt; Hinweis, wenn sie darüber liegt, ohne dass die Grenze mitgezogen wurde
— wortgleich zur bestehenden Budget-Logik.

- [ ] **Step 3: Mutationsbeweis**

Die Grenze testweise über den Ist-Wert setzen → Skript muss **rot** werden.
Zurücksetzen → **grün**. Beides in der Commit-Nachricht mit den gesehenen
Zahlen belegen, nicht behaupten.

- [ ] **Step 4: Regel 4 im Wellenplan korrigieren**

§4 Regel 4 um den gemessenen Befund aus V2 ergänzen: Die vier
Thinness-Budgets messen die Thin-Core-Migration, nicht P1a; für P1a gilt die
`reprise-view`-Untergrenze. Die alte Regel wird nicht gelöscht, sondern
präzisiert — sie gilt weiter für Wellen, die Datenbank-, Datei- oder
Thread-Zugriffe bewegen (Welle 6 wird das tun).

- [ ] **Step 5: Volle Gates und Commit**

```bash
cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings \
  && cargo test --workspace && cargo audit \
  && bash scripts/check-architecture.sh && bash scripts/check-frontend-thinness.sh
git add scripts docs
git commit -m "ci: measure P1a by a floor on the shared view layer

The four thinness budgets track the thin-core migration and none of them
move when presentation logic does. P1a gets its own mechanical measure."
```

---

## Task 2: Der erste Umzug — 15 Zeilen ohne Abhängigkeiten

`playlist_import_navigation.rs` hängt an nichts außer `reprise_core`. Es
klärt Modulpfad, Sichtbarkeit und Konsum-Naht, ohne dass Strings, Formate
oder Übersetzung mitreden.

**Files:**
- Create: `crates/reprise-view/src/playlists.rs`
- Modify: `crates/reprise-view/src/lib.rs`
- Modify: `crates/reprise-gnome/Cargo.toml`
- Modify: `crates/reprise-gnome/src/ui/playlists/mod.rs`
- Delete: `crates/reprise-gnome/src/ui/playlists/playlist_import_navigation.rs`

- [ ] **Step 1: Die Funktion samt Test nach `reprise-view`**

`target_for_import` wandert unverändert nach
`crates/reprise-view/src/playlists.rs`. Die Sichtbarkeit wird von
`pub(in crate::ui)` zu `pub` — das ist die erste der 577 Sichtbarkeiten, und
die Regel, die daraus folgt, gilt für alle: **jede Sichtbarkeit, die den
Crate-Wechsel macht, wird `pub`, keine engere Variante.** Der `#[cfg(test)]`-
Block zieht mit.

- [ ] **Step 2: `reprise-gnome` konsumiert die Crate**

`reprise-view` als Workspace-Abhängigkeit in `crates/reprise-gnome/Cargo.toml`
aufnehmen. In `ui/playlists/mod.rs` die Moduldeklaration durch einen
Re-Export ersetzen, sodass der bestehende Pfad
`playlist_import_navigation::target_for_import` weiter auflöst:

```rust
pub(in crate::ui) use reprise_view::playlists as playlist_import_navigation;
```

Die eine Aufrufstelle im Frontend bleibt damit **unverändert**. Das ist die
Naht, auf die sich alle Folgewellen berufen.

- [ ] **Step 3: Gate-Gegenprobe**

`bash scripts/check-architecture.sh` muss grün bleiben — es prüft, dass
`reprise-view` außer `reprise-core` keine `reprise-*`-Kante und keine der
Familien `gtk4|libadwaita|glib|gstreamer|zbus` zieht. Zusätzlich einmal
mutieren: probeweise `reprise-runtime-protocol` in
`crates/reprise-view/Cargo.toml` eintragen → Gate muss **rot** werden, dann
zurücknehmen.

- [ ] **Step 4: Untergrenze anheben und Commit**

`view_floor` in `scripts/check-frontend-thinness.sh` auf den neuen Ist-Wert
setzen — das ist die Bewegung, die Task 1 messbar gemacht hat.

```bash
cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings \
  && cargo test --workspace && cargo audit \
  && bash scripts/check-architecture.sh && bash scripts/check-frontend-thinness.sh
git add crates scripts
git commit -m "refactor(view): move playlist import navigation into reprise-view

The first crate crossing of P1a. Fifteen lines with a single reprise-core
edge, chosen so the module path, the visibility rule and the consuming
re-export can be settled without strings or formatting arguing too."
```

---

## Task 3: Das Übersetzungsformat — `Message` statt `String`

Der Kern dieser Welle. `reprise-view` beschreibt Texte, statt sie zu
übersetzen (V1). `reprise-gnome` behält eine Adapterdatei, die aus der
Beschreibung wieder einen `String` macht — deshalb ändert sich an den
35 Aufrufstellen nichts.

**Files:**
- Create: `crates/reprise-view/src/strings.rs`
- Create: `crates/reprise-view/src/strings/scan.rs`
- Modify: `crates/reprise-view/src/lib.rs`
- Modify: `crates/reprise-gnome/src/ui/strings_scan.rs` (wird zum Adapter)
- Modify: `crates/reprise-gnome/src/ui/strings.rs`
- Modify: `po/POTFILES.in`

- [ ] **Step 1: Der Werttyp**

In `crates/reprise-view/src/strings.rs`:

```rust
/// Ein übersetzbarer Text als Wert: die msgid, optional ihre Plural-msgid
/// samt Anzahl, und die benannten Platzhalter. Wer ihn rendert, entscheidet
/// die Oberfläche — GTK per gettext, Android per strings.xml.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: &'static str,
    pub plural_id: Option<&'static str>,
    pub count: Option<u64>,
    pub args: Vec<(&'static str, String)>,
}
```

Kein Closure, kein Trait-Objekt — Welle-0-Muster (Regel 6). Der Typ ist
absichtlich ein reiner Record, damit UniFFI ihn trägt (Spike-Befund Frage 3).

- [ ] **Step 2: Das Makro mitnehmen**

`N_!` ist heute in `ui/strings.rs` definiert und markiert Literale für
`xgettext`. `reprise-view` braucht eine eigene, gleichlautende Definition —
sonst findet die Extraktion die msgids nicht mehr.

- [ ] **Step 3: Der Katalog zieht um**

Die sieben Konstanten und vier Funktionen aus `ui/strings_scan.rs` wandern
nach `crates/reprise-view/src/strings/scan.rs`. Die Funktionen geben ab jetzt
`Message` zurück statt `String`; `formatted`/`plural`/`text` werden **nicht**
mitgenommen — sie bleiben in `reprise-gnome`, weil sie gettext rufen.

- [ ] **Step 4: Der Renderer in `reprise-gnome`**

`ui/strings_scan.rs` wird zur Adapterdatei. Sie re-exportiert die Konstanten
(darunter `RETRY` mit seinen 26 Aufrufstellen) und wickelt jede Funktion
einmal ein:

```rust
pub(super) fn render(message: &reprise_view::strings::Message) -> String {
    let template = match (message.plural_id, message.count) {
        (Some(plural_id), Some(count)) => {
            crate::i18n::ngettext(message.id, plural_id, count as u32)
        }
        _ => crate::i18n::gettext(message.id),
    };
    crate::i18n::format_message(&template, &borrowed(&message.args))
}
```

**Verhaltensgleichheit ist die Anforderung, nicht Ähnlichkeit:** dieselbe
msgid, dieselbe Plural-Auswahl, dieselben Platzhalter.

- [ ] **Step 5: Die Extraktion nachziehen**

In `po/POTFILES.in` den Pfad `crates/reprise-gnome/src/ui/strings_scan.rs`
durch `crates/reprise-view/src/strings/scan.rs` ersetzen. Danach die `.pot`
neu erzeugen und **beweisen, dass keine Übersetzung verlorengeht**: Die
sieben msgids dieser Datei müssen vor und nach dem Umzug zeichengleich sein.
Ändert sich auch nur eine, sind die bestehenden Einträge in `de.po`, `es.po`,
`fr.po`, `ar.po`, `bn.po`, `hi.po` verwaist — das wäre ein stiller
Übersetzungsverlust und ist ein Abbruchgrund für diesen Task.

- [ ] **Step 6: Regressionsnachweis**

Ein Test in `reprise-gnome`, der für jede der vier Funktionen den gerenderten
`String` gegen den heutigen Wortlaut prüft — vor dem Umbau geschrieben, damit
er die Gleichheit belegt und nicht nur das Ergebnis nachzeichnet. Dazu ein
Test in `reprise-view`, der die `Message`-Werte selbst prüft (msgid, Plural,
Argumente) und ohne jede Übersetzung auskommt.

- [ ] **Step 7: Untergrenze anheben, volle Gates, Commit**

```bash
cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings \
  && cargo test --workspace && cargo audit \
  && bash scripts/check-architecture.sh && bash scripts/check-frontend-thinness.sh
git add crates scripts po
git commit -m "feat(view): describe translatable text as a value

reprise-view carries the msgid, its plural form and the named arguments;
each surface renders them — GTK through gettext, Android through
strings.xml. The frontend keeps a one-file adapter, so no call site moves."
```

---

## Task 4: Die string-gekoppelte Datei

`playlist_io_names.rs` konnte vorher nicht ziehen, weil es
`strings::text(strings::IMPORTED_PLAYLIST_FALLBACK_NAME)` ruft. Nach Task 3
ist der Weg frei — und der Task beweist, dass das Format trägt.

**Files:**
- Modify: `crates/reprise-view/src/playlists.rs`
- Modify: `crates/reprise-view/src/strings/scan.rs` oder neu:
  `crates/reprise-view/src/strings/playlists.rs`
- Modify: `crates/reprise-gnome/src/ui/playlists/mod.rs`
- Delete: `crates/reprise-gnome/src/ui/playlists/playlist_io_names.rs`
- Modify: `po/POTFILES.in`

- [ ] **Step 1: Die msgid mitnehmen**

`IMPORTED_PLAYLIST_FALLBACK_NAME` lebt heute in `ui/strings.rs` — dem Modul,
das diese Welle ausdrücklich **nicht** bewegt. Nur diese eine Konstante zieht
nach `reprise-view`; `ui/strings.rs` re-exportiert sie zurück, damit seine
übrigen Aufrufstellen unberührt bleiben. Das ist der Präzedenzfall für jede
spätere Welle, die eine einzelne msgid aus `strings.rs` herauslöst.

- [ ] **Step 2: `playlist_name_from_file` und `display_name` umziehen**

`display_name` ist reine Formatierung ohne Übersetzung und zieht unverändert.
`playlist_name_from_file` gibt seinen Rückfalltext ab jetzt als `Message`
heraus — oder, falls der Aufrufer zwingend einen `String` braucht, nimmt es
den bereits gerenderten Rückfalltext als Parameter entgegen. **Welche der
beiden Formen es wird, entscheidet die Aufrufstelle, nicht dieser Plan** —
Task 4 beginnt damit, sie zu lesen.

- [ ] **Step 3: Untergrenze anheben, volle Gates, Commit**

---

## Task 5: Das Muster festschreiben

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Die vier Regeln in den Wellenplan**

§4 um das ergänzen, was diese Welle bewiesen hat:

1. Sichtbarkeiten werden beim Crate-Wechsel `pub`, nie enger.
2. `reprise-gnome` behält je Bereich **eine** Adapterdatei; Aufrufstellen im
   `ui`-Baum ändern sich nicht.
3. Übersetzbarer Text überquert die Crate-Grenze als `Message`, nie als
   fertiger `String` (V1 / Spec-O2).
4. Jede Welle hebt `view_floor` im selben Commit an.

- [ ] **Step 2: Spec-Punkt O2 schließen**

In `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md` §8
den Punkt O2 von „noch nicht entschieden" auf die getroffene Entscheidung
umschreiben, mit Verweis auf den Messbefund in
`docs/research/android-spike-2026-08.md` §Frage 6.

- [ ] **Step 3: Ledger-Eintrag**

Eine Zeile im Hausformat an `.superpowers/sdd/progress.md`: Commits, Basis,
was bewiesen wurde (Gate erst rot, dann grün; msgids zeichengleich; Testzahl
gegen Baseline; `view_floor` von N auf M).

- [ ] **Step 4: Volle Gates und Commit**

---

## Nach dieser Welle

Welle 2 (`lyrics`, 8 Dateien, ~1.560 LOC) ist die erste mit echter
Zustandslogik. Sie erbt aus dieser Welle die Adapterdatei-Naht, die
`pub`-Regel und das `Message`-Format — und muss keine davon neu verhandeln.

`strings.rs` selbst bleibt liegen. Mit 1.731 Aufrufstellen gehört es zu den
größten Einzelposten von P1a und braucht eine eigene Welle, deren Zuschnitt
sich auf die hier bewiesene Adapterdatei stützen kann.
