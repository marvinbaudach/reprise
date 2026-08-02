---
slug: library-source-probe
worktree: /home/marvin/Projects/reprise-library-source-inventory
branch: feature/library-source-inventory
phase: planned
codex_session:
created: 2026-08-02
---
# Storage-Abstraktion, Paket 3 — eine Frage statt dreier

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Präsenz- und Metadatenfragen an eine Bibliotheksdatei laufen über
`LibrarySource` — und der Scanner stellt sie **einmal** pro Datei statt zweimal.

**Basis:** `dev` (`6fdd622dec`, Paket 2 gemergt).

**Belege:** `docs/research/android-spike-2026-08.md`, Abschnitt „Die
Klassifikation, die Paket 3 freigibt". Die dort als **Klasse A** geführten
21 Stellen sind der vollständige Umfang dieses Pakets. Keine Stelle aus B, C
oder E wird angefasst.

## Der Fund, der den Schnitt bestimmt

`scan_folder_inner` fragt jede Audiodatei **zweimal** nach denselben
Metadaten:

```
324:  let mtime = file_mtime(path);   // std::fs::metadata(path)
327:  let stat  = file_stat(path);    // std::fs::metadata(path)
```

Beide rufen `std::fs::metadata` auf demselben Pfad. Unter Linux trifft der
zweite Aufruf den Inode-Cache und kostet praktisch nichts — dieselbe Ökonomie,
die den Scanner-Doppellauf in Paket 2 so lange unauffällig ließ. **Unter SAF
ist jeder Aufruf ein eigener `DocumentsContract.query` über Binder**, und der
Scanner macht das für jede Datei der Bibliothek.

Es kommt schlimmer: der Walk hat diese Datei gerade erst geliefert. Ein
SAF-Cursor führt Größe, Änderungszeit und Dokument-ID **in derselben Zeile**
mit, die den Eintrag überhaupt erst gemeldet hat. Der dritte Rundlauf ist also
ebenfalls vermeidbar — aber nur, wenn der Eintrag mehr tragen darf als heute.

`LibraryEntry` trägt derzeit `{ path, is_file }`. Das war in Paket 2 richtig
(„genau das, was die Verbraucher lesen"). Jetzt lesen sie mehr.

## Die Entwurfsfrage

**Trägt der Walk die Metadaten mit, oder gibt es eine eigene Abfrage — und wer
zahlt dafür?**

Die Spannung ist echt und geht in beide Richtungen:

- Würde `LibraryEntry` Größe und Änderungszeit **immer** tragen, müsste der
  Unix-Adapter jeden Eintrag während des Laufs staten — auch jede Nicht-Audio-
  Datei, die der Scanner danach wegwirft. Das macht Linux *langsamer*, um
  Android schneller zu machen.
- Gäbe es nur eine getrennte Abfrage, bliebe SAF bei einem zusätzlichen
  Rundlauf pro Datei, obwohl es die Antwort schon in der Hand hatte.

Die Hausform kennt eine Antwort auf genau diese Sorte Frage: **ehrliche
Degradation.** `residence_token` liefert `None`, wenn eine Quelle kein
stabiles Merkmal hat. Ein Eintrag könnte ebenso Fakten tragen, *die die Quelle
ohnehin schon hatte*, und die Abfrage füllt nach, wenn nicht. Ob das die
richtige Form ist, entscheidest du — aber entscheide es begründet, und schreib
die Begründung in die Commit-Nachricht.

Maßgeblich ist: **auf Linux darf kein zusätzlicher `stat` entstehen, und auf
SAF kein zusätzlicher Rundlauf.** Eine Lösung, die eine Seite auf Kosten der
anderen bevorzugt, ist keine.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`,
  `bash scripts/tests/gettext-catalogs.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe. Testbilanz nach
  **Schlüsselwort** summieren, nicht nach Feldposition.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Baseline:** 3962 passed, 0 failed, 414 ignored, 56 Suiten.
- **Das Scan-Ergebnis darf sich auf Linux nicht ändern.** Dieselben Zeilen,
  dieselben Zähler, dieselben Verdikte.
- **Nur Klasse A.** Keine app-private, keine fremde, keine Adapter-Stelle.
- **Kein `#[allow(…)]`**, keine neue Abhängigkeit, kein Schema-Wechsel.

---

## Task 1: Die Doppelabfrage im Scanner

**Files:**
- Modify: `crates/reprise-core/src/library/scanner.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Eine Abfrage statt zweier**

`file_mtime` und `file_stat` beantworten Fragen an dasselbe `stat`. Führe sie
zusammen, **bevor** ein Trait im Spiel ist — dieser Schritt ist reine
Aufräumarbeit und muss für sich grün sein.

`file_stat`s Doc-Kommentar trägt die Begründung aus #230 („**A platform arm
must never fabricate an identity**"). Sie zieht mit, ohne Kürzung.

Achte auf den Unterschied: `file_mtime` liefert heute `0`, wenn `stat`
fehlschlägt; `file_stat` liefert `None`. Das ist **kein** Zufall — `0` landet
als `file_mtime` in der Datenbank und bedeutet dort „unbekannt, immer neu
lesen". Was immer du zusammenführst, muss beide Fälle unverändert erhalten.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 2: Die Abfrage auf dem Trait

**Files:**
- Modify: `crates/reprise-core/src/library/source.rs`
- Modify: die Klasse-A-Stellen laut Spike

- [ ] **Step 1: Die Entwurfsfrage entscheiden**

Siehe oben. Lies zuerst, was die 21 Stellen tatsächlich wissen wollen — mehrere
fragen nur „liegt da eine Datei", andere brauchen Größe, Änderungszeit oder
Identität. Die Signatur trägt genau das, was gebraucht wird, und die Antwort
auf eine Frage, die eine Quelle nicht beantworten kann, ist `None` und nicht
ein erfundener Wert.

- [ ] **Step 2: Umlegen**

Die Zuführungsform steht seit Paket 1 fest: ein `&dyn LibrarySource`-Parameter
mit schmalen Vorgabe-Wrappern an der öffentlichen Grenze. **Nicht neu
entscheiden.**

Drei der 21 Stellen sind `read_dir` auf einem Albumordner
(`cover.rs:82`, `cover_writeback.rs:50`, `writeback_publish.rs:198`) — eine
*flache* Auflistung, kein rekursiver Lauf. Ob `walk` dafür eine Tiefengrenze
bekommt oder eine eigene Operation danebentritt, entscheidest du; beides ist
vertretbar, aber begründe es.

- [ ] **Step 3: Der Beweis**

Ein Test, der eine Klasse-A-Stelle über eine Quelle fährt, die kein
Dateisystem anfasst — wie `scanner_source_tests.rs` es für die Traversierung
tut. Und ein Test, der zeigt, dass pro Datei **nur noch eine** Abfrage
stattfindet: eine zählende Testquelle genügt. Ohne diesen Zähler ist die
Hauptaussage des Pakets unbelegt.

- [ ] **Step 4: Volle Gates und Commit**

---

## Task 3: Festhalten

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Klasse A abhaken**

Welche der 21 Stellen umgestellt sind und welche — mit Grund — nicht. Halte
fest, ob der Walk am Ende Metadaten mitträgt, denn davon hängt ab, wie teuer
ein SAF-Scan wirklich wird.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach diesem Paket

Klasse C braucht keine Abstraktion, sondern eine **Plattformgrenze**: der
Rhythmbox-Import ist eine Desktop-Funktion und hat unter Android kein
Gegenstück. Danach bleiben die Schreib-Handles — `tag_mutation.rs` als die
eine produktive Lofty-Speichernaht —, und `watcher.rs` als ausdrücklich
optionale Fähigkeit.
