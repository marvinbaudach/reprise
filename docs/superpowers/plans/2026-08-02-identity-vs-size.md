---
slug: identity-vs-size
worktree: /home/marvin/Projects/reprise-identity-fix
branch: fix/identity-vs-size
phase: refactored
codex_session:
created: 2026-08-02
---
# Dateigröße von Dateisystem-Identität trennen

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Umzugserkennung soll ihre zweite Strategie — den Fingerabdruck
aus Titel, Interpret, Album, Dauer und Größe — auch dann benutzen können, wenn
`(device, inode)` fehlt. Heute entfallen beide Strategien gemeinsam.

**Warum das jetzt ansteht:** Zwei Gründe, und der erste ist ein Datenfehler.

1. **Windows.** `file_stat`s Nicht-Unix-Zweig liefert `Some((size, 0, 0))`, und
   sein Doc-Kommentar sagt dazu „never reached at runtime" — was gilt, solange
   die App nur auf Linux läuft. Ein Tauri-Desktop macht die Aussage falsch.
   Dann fragt Strategie 1 `WHERE device = 0 AND inode = 0` und trifft **jede**
   dort gescannte Zeile. Der Kandidatenfilter und die Mehrdeutigkeitssperre
   fangen fast alles ab; bleibt **genau eine** passende Zeile, hängt die
   Abspielhistorie eines fremden Titels am neuen. Selten, still, nicht
   rückgängig zu machen. Genau diesen Fehler beschreibt derselbe Kommentar als
   den, den Stage 3 Task 1 beseitigt hat — jener Fix greift aber nur, wenn
   `stat` **fehlschlägt**, und auf Windows schlägt es nicht fehl.
2. **Android/SAF.** Dort gibt es keine Inodes, wohl aber eine echte Dateigröße
   (der DocumentsProvider liefert sie als Spalte) und Tags über ein Handle.
   Strategie 2 wäre also voll funktionsfähig — sie ist nur verriegelt.

Beides ist derselbe Umbau. Belegt in
`docs/research/android-spike-2026-08.md` §Frage 8.

**Basis:** `dev`.

## Was heute im Weg steht

`crates/reprise-core/src/library/scanner.rs:441`:

```rust
match (device, inode) {
    (Some(device), Some(inode)) => move_detect::find_move_candidate(…),
    _ => None,
}
```

Der Kommentar darüber begründet das und hat für seinen Fall recht: schlägt
`stat` fehl, ist auch `file_size` ein Platzhalter-`0`, und der Fingerabdruck
vergliche gegen Müll. **Die Begründung bindet also an die Größe, nicht an die
Identität** — nur ist beides heute im selben `Option` verpackt.

`MoveLookup` (`scanner_move.rs:32`) trägt `device: i64` und `inode: i64` als
Pflichtfelder, und `find_move_candidate_inner` führt Strategie 1 immer aus,
bevor es zu Strategie 2 kommt.

## Der Umbau

**Größe und Identität werden zwei Dinge.** Die Identität wird optional, die
Größe bleibt Pflicht — und das Gate hängt an der Größe.

Kein Schema-Wechsel: `tracks.device` und `tracks.inode` sind bereits nullable,
und `classify_missing(None, _)` liefert bereits `Unknown`. Die Datenbank ist
auf diesen Fall vorbereitet, nur der Code ist es nicht.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe lesen. Testbilanz nach
  **Schlüsselwort** summieren, nicht nach Feldposition.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Auf Linux darf sich nichts ändern.** Jede heutige Erkennung muss nachher
  identisch ausfallen — das ist die Hauptanforderung, nicht eine Nebenbedingung.
- **Bekannt rot, nicht von dieser Änderung:** `scripts/tests/gettext-catalogs.sh`
  am fehlenden `po/ar.po`-Eintrag für `"Play this track"`.

---

## Task 1: Der Regressionstest, bevor irgendetwas umgebaut wird

Der Fehler ist heute nicht sichtbar, weil der Nicht-Unix-Zweig nicht läuft.
Ein Test muss ihn sichtbar machen, sonst ist nicht belegbar, dass der Umbau
etwas behebt.

**Files:**
- Modify: `crates/reprise-core/src/library/scanner_move.rs` (Testmodul) oder
  die passende `*_tests.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Den Fehler als Test schreiben**

Zwei Zeilen mit `device = 0`, `inode = 0` in die Testdatenbank, davon **eine**
als gültiger Kandidat (alter Pfad weg oder `missing_since` gesetzt), und ein
`MoveLookup` mit `device: 0, inode: 0`, dessen Tags **nicht** zu jener Zeile
passen. Heute liefert Strategie 1 sie trotzdem. Der Test muss **rot** sein —
beobachte das und halte die Meldung fest, sonst prüft er nichts.

- [ ] **Step 3: Commit mit dem roten Test**

Ausdrücklich als `#[ignore]` oder mit `#[should_panic]` — je nachdem, was hier
Hausbrauch ist — damit die Suite grün bleibt und der Befund trotzdem im Baum
steht. Die Commit-Nachricht nennt die beobachtete Fehlermeldung.

---

## Task 2: Identität optional machen

**Files:**
- Modify: `crates/reprise-core/src/library/scanner_move.rs`
- Modify: `crates/reprise-core/src/library/scanner.rs`
- Modify: `crates/reprise-core/src/library/relink.rs`

- [ ] **Step 1: `MoveLookup` bekommt ein Feld statt zwei**

`device: i64` und `inode: i64` werden zu **einem** `identity: Option<(i64, i64)>`.
Ein Feld, nicht zwei `Option`s — Welle 1 dieses Projekts hat vorgeführt, warum:
zwei lose `Option`s erlauben einen Zustand, den niemand meint, und niemand
merkt es.

- [ ] **Step 2: Strategie 1 nur noch mit Identität**

`find_move_candidate_inner` führt die `device`/`inode`-Abfrage nur aus, wenn
`identity` da ist. Strategie 2 läuft unverändert und **immer**. Die
Mehrdeutigkeitssperre und `valid_candidates` bleiben, wie sie sind.

- [ ] **Step 3: Das Gate von der Identität auf die Größe legen**

In `scanner.rs` entfällt das `match (device, inode)`. Die Umzugserkennung läuft,
sobald `file_stat` **überhaupt** etwas geliefert hat — dann ist die Größe echt.
Liefert es `None`, bleibt alles wie heute: keine Erkennung.

- [ ] **Step 4: Der Nicht-Unix-Zweig gibt keine Identität mehr vor**

`file_stat` liefert dort die echte Größe und **keine** Identität, statt
`(0, 0)` zu erfinden. Damit ist der Fehler aus Task 1 behoben — der Test aus
Task 1 wird grün und verliert sein `#[ignore]`.

- [ ] **Step 5: Verhaltensgleichheit auf Linux belegen**

Die vorhandenen Umzugstests müssen unverändert grün sein. Zusätzlich der
Nachweis, dass bei vorhandener Identität weiterhin **erst** Strategie 1
greift — sonst wäre die Reihenfolge still gekippt.

- [ ] **Step 6: Volle Gates und Commit**

---

## Task 3: Festhalten

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: §Frage 8 auf „umgesetzt" nachziehen**, mit dem Hinweis, was
  auf Android jetzt möglich ist (Strategie 2 ohne Identität) und was dort
  weiterhin fehlt (`rename`-Erkennung).

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Was diese Änderung ausdrücklich nicht tut

Sie führt **keine** `LibrarySource`-Abstraktion ein und fasst keinen der 27
Dateisystem-Aufrufe an, die §Frage 7 aufzählt. Sie macht nur die eine Stelle
plattformfähig, an der heute ein Datenfehler wartet — und schaltet nebenbei
frei, was Android später braucht.
