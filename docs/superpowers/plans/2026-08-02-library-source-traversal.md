---
slug: library-source-traversal
worktree: ~/Projects/reprise-library-source-traversal
branch: feature/library-source-traversal
phase: planned
codex_session:
created: 2026-08-02
---
# Storage-Abstraktion, Paket 2 — die Traversierung

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das Ablaufen eines Bibliotheksbaums läuft über `LibrarySource`
statt über `walkdir` direkt — und der Doppellauf des Scanners verschwindet,
weil er unter SAF nicht bezahlbar ist.

**Basis:** `dev` (`78c729257f`, Paket 1 gemergt).

**Belege:** `docs/research/android-spike-2026-08.md`, Abschnitt „Paket 1
umgesetzt". Die dortige Cluster-Tabelle listet Dateien; die Zahlen unten sind
am 2026-08-02 auf `dev` nachgemessen.

## Warum die Traversierung und nicht die Präsenzprüfungen

Der Rohbestand des Spike-Clusters „Baum und Präsenz" ist **53 Fundstellen in
22 Dateien** (`.exists()`, `fs::metadata`, `symlink_metadata`, `is_file`,
`is_dir`, `read_dir`, `WalkDir::new`; ohne Testdateien). Das ist mehr als das
Doppelte der „rund zwanzig", die der Spike schätzte.

Die Zahl ist aber irreführend, und zwar systematisch: **ein großer Teil der
Treffer betrifft app-private Pfade**, die laut Spike gar nicht in die
Abstraktion gehören. `cover.rs` ist der Musterfall — fünf Treffer, davon
genau **einer** bibliotheksbezogen (`read_dir` im Albumordner, Zeile 82); die
vier `out.exists()` prüfen Thumbnails im XDG-Cache. Wer den Cluster als Block
umbaut, zieht app-privaten Speicher in ein Trait, das die Musikquelle
beschreibt.

Die Traversierung dagegen ist klein und scharf umrissen:

| | |
| --- | --- |
| `WalkDir::new` in `reprise-core` | **4** |
| davon über den Bibliotheksbaum | `scanner.rs:261`, `scanner_progress.rs:15`, `relink.rs:166`, `relink.rs:268` |

Vier Stellen — dieselbe Größenordnung wie Paket 1 mit seinen dreien. Und sie
tragen die Entwurfsfrage, die die ~44 Präsenzprüfungen nicht tragen.

## Die Entwurfsfrage: der Doppellauf

`scan_folder_with_progress` läuft **zweimal über denselben Baum**:

- `scanner.rs:133` ruft `scan_progress::count_audio_files(root)` — ein
  vollständiger `WalkDir`-Lauf, nur um den Nenner des Fortschrittsbalkens zu
  kennen.
- `scanner.rs:261` läuft danach denselben Baum erneut, diesmal richtig.

Unter Linux kostet der Vorlauf fast nichts: der zweite Lauf trifft den
Page-Cache. **Unter SAF ist jede Verzeichnisauflistung ein Binder-IPC an den
DocumentsProvider.** Der Vorlauf verdoppelt damit die teuerste Operation des
gesamten Scans, auf genau der Plattform, auf der der Scan ohnehin am
langsamsten ist.

Ein `walk`, das `walkdir` bloß einpackt, verewigt dieses Muster. Deshalb ist
die Frage dieses Pakets nicht „wie sieht `walk` aus", sondern:

**Wie meldet der Scanner Fortschritt, ohne den Baum vorher zu zählen?**

Ein Teil der Antwort steht schon im Code: `ScanProgressReporter` hebt seinen
`total` bereits an, wenn der Lauf mehr Dateien findet als die Vorzählung sah
(`scanner_progress.rs`, Doc-Kommentar). Die Maschinerie für einen unbekannten
Nenner ist also halb vorhanden. Ob daraus ein unbestimmter Fortschritt, eine
gespeicherte Schätzung aus dem letzten Scan oder etwas Drittes wird,
entscheidest du — aber **begründet und in der Commit-Nachricht**.

**Was dieses Paket nicht anfasst:** Präsenz- und Metadatenprüfungen, Handles,
Ein-/Ausgabe, `watcher.rs`. Und keinen app-privaten Pfad.

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
- **Baseline:** 3957 passed, 0 failed, 413 ignored, 56 Suiten.
- **Auf Linux darf sich am Ergebnis nichts ändern.** Ein Scan muss dieselben
  Zeilen schreiben wie heute. Der *Fortschrittsverlauf* darf sich ändern —
  das ist Teil der Aufgabe —, das *Ergebnis* nicht.
- **Kein `#[allow(…)]`** gegen eine Warnung des eigenen Umbaus.
- **Keine neue Abhängigkeit**, kein Schema-Wechsel.
- **Kein app-privater Pfad** wandert in `LibrarySource`.

---

## Task 1: Den Doppellauf auflösen

**Files:**
- Modify: `crates/reprise-core/src/library/scanner_progress.rs`
- Modify: `crates/reprise-core/src/library/scanner.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Entscheiden, wie Fortschritt ohne Vorzählung entsteht**

Lies `ScanProgressReporter` und seine Tests in `scanner_progress_tests.rs`
zuerst — sie sagen dir, welche Zusicherungen der Fortschritt heute gibt.
Danach entscheide. Schreib die Begründung in die Commit-Nachricht, und nenne
darin ausdrücklich, **was der Nutzer auf dem Bildschirm anders sieht**.

- [ ] **Step 3: Umbauen, mit den Fortschritts-Tests als Beweis**

Die bestehenden Tests von `scanner_progress` sind der Maßstab. Wo eine
Zusicherung bewusst fällt, ändere den Test **in einem sichtbaren Schritt** und
begründe ihn im Commit — nie stillschweigend anpassen.

- [ ] **Step 4: Volle Gates und Commit**

Noch ohne Trait-Erweiterung. Nach diesem Commit läuft der Scanner einmal.

---

## Task 2: `walk` auf dem Trait

**Files:**
- Modify: `crates/reprise-core/src/library/source.rs`
- Modify: `crates/reprise-core/src/library/scanner.rs`
- Modify: `crates/reprise-core/src/library/relink.rs`

- [ ] **Step 1: Die Form**

`walkdir::DirEntry` darf **nicht** in der Trait-Signatur auftauchen — das
wäre die Abhängigkeit, die das Paket loswerden soll. Was die drei
Aufrufstellen tatsächlich aus einem Eintrag lesen (Pfad, Dateityp, mehr?),
misst du zuerst; die Signatur trägt genau das und nichts darüber hinaus.

Beachte: UniFFI kann keine Closures und keine anonymen Tupel tragen (siehe
Spike). Ein `Iterator` als Rückgabetyp macht das Trait außerdem nicht mehr
objekt-sicher — `LibrarySource` wird heute als `&dyn` gereicht. Löse das,
statt daran vorbeizubauen.

- [ ] **Step 2: Die Unix-Implementierung**

Sie kapselt `walkdir` mit `follow_links(false)`. Dass Symlinks nicht verfolgt
werden, ist **keine Beiläufigkeit** — `source.rs`s
`nearest_existing_ancestor` erklärt ausführlich, warum diese Codebasis
Symlinks nicht folgt. Der Zusammenhang gehört dokumentiert.

- [ ] **Step 3: Die drei Aufrufstellen umlegen**

Die Zuführungsform steht aus Paket 1 fest: ein `&dyn LibrarySource`-Parameter,
Vorgabe-Wrapper an der öffentlichen Grenze. **Nicht neu entscheiden.**

- [ ] **Step 4: Der Beweis**

Ein Test mit einer zweiten Quelle, deren Baum **kein Dateisystem** ist — ein
im Test aufgebauter Eintragsbaum genügt. Er muss dieselbe Reihenfolge- und
Filterzusicherung liefern wie der Unix-Lauf. Ohne diesen Test hat das Paket
`walkdir` nur umbenannt.

- [ ] **Step 5: Volle Gates und Commit**

---

## Task 3: Festhalten

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Die Cluster-Tabelle korrigieren**

Die 53 gemessenen Stellen und der Grund, warum die Zahl irreführt (app-private
Pfade im selben Cluster, `cover.rs` als Musterfall). Paket 3 braucht eine
**stellenweise klassifizierte** Liste, keine Dateiliste — halte fest, dass die
Klassifikation noch aussteht.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach diesem Paket

Die Präsenz- und Metadatenprüfungen, aber erst nach der Klassifikation aus
Task 3 — sonst wandert app-privater Speicher in ein Trait, das die Musikquelle
beschreibt. Danach die Handles, wo `tag_mutation.rs`s eine Naht die gesamte
Tag-Schreib-Oberfläche abdeckt.
