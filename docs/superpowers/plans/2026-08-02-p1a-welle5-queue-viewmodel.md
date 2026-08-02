---
slug: p1a-welle5-queue-viewmodel
worktree: /home/marvin/Projects/reprise-p1a-welle5
branch: feature/p1a-welle5-queue-viewmodel
phase: planned
codex_session:
created: 2026-08-02
---
# P1a Welle 5 — `QueueViewModel`, die erste Extraktion

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `QueueViewModel` samt `QueueSection`, `VirtualContext` und dem
`ContextWindow`-Trait verlässt `queue_sections.rs` und zieht nach
`reprise-view`. Dazu `ColumnId` aus `column_layout.rs`.

**Warum ausgerechnet dieser Typ:** §2.6 des Wellenplans hat das gesamte
Frontend auf Funktionsebene vermessen und gegnerisch gegenprüfen lassen.
`QueueViewModel` ist **der einzige nachweislich tragende Fall** — reine Daten,
`&self`-Methoden, keine Widgets. Der Doc-Kommentar des Moduls benennt ihn
selbst als den Typ, den eine Android-Oberfläche über UniFFI erreicht. Er wurde
in Welle 0 mit dieser Absicht gebaut.

**Basis:** `dev` (P0 und die Wellen 0 bis 4 sind gelandet).

**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md`, §2.6 und §3

## Der Zuschnitt, gemessen (2026-08-02)

`track_list/queue_sections.rs` hat 676 Zeilen und 42 Top-Level-Items. Davon
berühren **drei** das Toolkit:

| Item | Zeilen | Urteil |
| --- | --- | --- |
| `apply_queue_header_factory` | 71 | **bleibt** — `gtk4::prelude`, baut die Spaltenkopf-Factory |
| `episode_context_skip_is_one_leading_removal…` | 22 | bleibt — Test, benutzt GTK |
| `RecordingContextWindow` (Test-Helfer) | 4 | bleibt — hält `Rc<Cell<usize>>` |

Alles andere ist rein: `QueueSectionKind`, `QueueSection`, `QueueViewModel`,
`VirtualContext`, `VirtualContextIdentity`, das `ContextWindow`-Trait,
`SliceContextWindow`, und die Methoden `upcoming`, `sidebar_count`,
`total_len`, `items_window`, `all_items`, `leading_removal_change_from`,
`compose`, `compose_virtual`, `section_ranges` — plus der Großteil der Tests.

**Zwei Stellen liegen auf der Übersetzungsgrenze**, und für beide gibt es
bereits eine Regel:

- `header_title` (Zeile 430) wählt anhand der Abschnittsart zwischen drei
  msgids, und einer davon nimmt Argumente (`queue_context_tail(source_label,
  len)`). Das ist **wörtlich der `result_count`-Fall aus Welle 3**: die Wahl
  des Textes ist Logik und zieht mit, gerendert wird in `reprise-gnome`.
- `compose_virtual` (Zeile 283) benutzt `strings::text(strings::SIDEBAR_MUSIC)`
  als Rückfall-Label. Das ist der `playlist_name_from_file`-Fall aus Welle 1:
  entweder als `Message` heraus oder den gerenderten Rückfalltext hinein — das
  entscheidet die Aufrufstelle, nicht dieser Plan.

**Blast-Radius:** `queue_sections::` hat **71** externe Aufrufstellen
(`now_playing`, `up_next_panel`, `playback/queue_context_window`), `ColumnId`
hat **163**. Keine davon darf sich ändern; dafür ist die Adapter-Naht aus
Welle 1 da.

## Warum Form und Ort getrennt werden

Welle 0 hat vorgeführt, dass ein Fehlschlag nicht mehr zuzuordnen ist, wenn
eine Änderung gleichzeitig die Form und den Ort ändert. Hier ist die Gefahr
größer als damals: 71 Aufrufstellen, eine Übersetzungsgrenze und ein
Crate-Wechsel in einem Schritt wären nicht mehr trennbar.

Deshalb: **Task 1 löst heraus, ohne die Crate zu wechseln.** Erst Task 3
bewegt. Nach Task 1 muss der Workspace grün sein, ohne dass `reprise-view`
auch nur angefasst wurde.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`.
- **Exit-Codes einzeln erfassen.** Nie durch eine Pipe lesen — sie meldet
  ihren eigenen Erfolg. Und beim Auswerten der Testbilanz **nach
  Schlüsselwort** summieren, nicht nach Feldposition; eine Positions-Summe hat
  in Welle 4 einen Testfehler erfunden, den es nicht gab.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Baseline:** 3941 passed, 0 failed, 413 ignored, 55 Suiten (`dev` nach #227).
- **Keine der 71 + 163 Aufrufstellen ändert sich.**
- **Kein `#[allow(…)]`** gegen eine Warnung des eigenen Umbaus.
- **`view_floor` steigt nur in Task 3** — Task 1, 2 und 4 bewegen ihn nicht,
  und das ist kein Mangel, sondern der Punkt einer Extraktionswelle.
- **Bekannt rot, nicht von dieser Welle:** `scripts/tests/gettext-catalogs.sh`
  scheitert am fehlenden `po/ar.po`-Eintrag für `"Play this track"`. Gegen die
  Basis gegenprüfen, dass es derselbe eine Fehler ist.

---

## Task 1: Die reinen Typen aus der Wirtsdatei lösen — ohne Crate-Wechsel

**Files:**
- Create: `crates/reprise-gnome/src/ui/track_list/queue_model.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_sections.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/mod.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die reinen Items verschieben**

Alle oben als rein aufgeführten Typen und Methoden wandern nach
`queue_model.rs`. In `queue_sections.rs` bleiben `apply_queue_header_factory`,
`header_title` (vorerst unverändert) und die zwei GTK-gebundenen Testteile.

`queue_sections.rs` re-exportiert die bewegten Namen, sodass **alle 71
Aufrufstellen unverändert auflösen**. Das ist die Naht aus Welle 1, hier zum
ersten Mal innerhalb derselben Crate.

- [ ] **Step 3: Die `Send + Sync`-Zusicherung — und nur, wenn sie greift**

`QueueViewModel` hält `Vec<QueueItem>` und `Vec<QueueSection>`. Trägt es
damit `Send + Sync`, bekommt es die `const`-Zusicherung nach Welle-2-Muster,
**mit Mutationsbeweis**: ein `Rc<()>`-Feld einführen, den Compile-Fehler sehen,
zurücknehmen, beides mit den gesehenen Meldungen belegen.

Das `ContextWindow`-Trait ist hier der interessante Teil: Welle 0 hat es
eingeführt, damit **kein Closure** im ViewModel landet. Prüfe, ob die
Zusicherung das auch nach dem Umzug noch erzwingt — wenn nicht, ist der Schutz
aus Welle 0 beim Verschieben verlorengegangen, und das wäre ein Befund.

- [ ] **Step 4: Volle Gates und Commit**

Der Workspace muss grün sein, ohne dass `reprise-view` angefasst wurde.
`view_floor` bleibt unverändert.

---

## Task 2: Die Übersetzungsgrenze ziehen

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_model.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_sections.rs`

- [ ] **Step 1: `header_title` gibt `Message` zurück**

Die Wahl zwischen den drei msgids zieht mit — sie hängt an der Abschnittsart,
also an Daten, und muss auf Android dieselbe sein. `queue_sections.rs` behält
eine Hülle gleichen Namens, die rendert, damit die Aufrufstellen unverändert
bleiben. Vorbild: `result_count`/`result_count_state` aus Welle 3.

- [ ] **Step 2: Das Rückfall-Label in `compose_virtual`**

`strings::text(strings::SIDEBAR_MUSIC)` verlässt das Modell. **Lies zuerst die
Aufrufstelle**, dann entscheide zwischen „als `Message` heraus" und „gerendert
hinein" — beide Formen sind vertreten, Welle 1 hat die zweite gewählt, Welle 3
die erste.

- [ ] **Step 3: Verhaltensgleichheit belegen**

Für jede der drei Abschnittsarten der gerenderte Wortlaut vor und nach dem
Umbau, plus der Fall mit Argumenten (`queue_context_tail`). Die msgids müssen
zeichengleich bleiben, sonst verwaisen die Katalogeinträge.

- [ ] **Step 4: Volle Gates und Commit**

---

## Task 3: Der Crate-Wechsel

**Files:**
- Create: `crates/reprise-view/src/queue.rs`
- Modify: `crates/reprise-view/src/lib.rs`
- Delete: `crates/reprise-gnome/src/ui/track_list/queue_model.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_sections.rs`
- Modify: `scripts/check-frontend-thinness.sh`

- [ ] **Step 1: `queue_model.rs` wird `reprise-view/src/queue.rs`**

Sichtbarkeiten werden `pub` (Welle-1-Regel). Die Re-Export-Naht in
`queue_sections.rs` zeigt jetzt auf `reprise_view::queue` statt auf das lokale
Modul — eine Zeile, und die 71 Aufrufstellen merken nichts davon.

- [ ] **Step 2: Gate-Gegenprobe**

`check-architecture.sh` muss grün bleiben. Einmal mutieren: probeweise eine
verbotene Kante in `crates/reprise-view/Cargo.toml` eintragen, Gate rot sehen,
zurücknehmen — mit den gesehenen Meldungen im Commit.

- [ ] **Step 3: `view_floor` anheben, volle Gates, Commit**

---

## Task 4: `ColumnId` aus `column_layout.rs`

163 Aufrufstellen, aber der Typ selbst ist ein `Copy`-Enum aus elf
Varianten ohne Felder.

**Files:**
- Modify: `crates/reprise-view/src/lib.rs` oder ein neues Modul
- Modify: `crates/reprise-gnome/src/ui/track_list/column_layout.rs`

- [ ] **Step 1: Prüfen, ob er allein ziehen kann**

`column_widths.rs` wartet darauf (§2.5). Aber `column_layout.rs` benutzt
`ColumnId` als Schlüssel gegen Widget-Typen — **prüfe, ob am Enum selbst
etwas hängt, das nicht mitkann**, bevor du ihn bewegst. Findet sich so etwas,
ist das ein Befund und Task 4 endet mit einer Notiz statt einem Umzug.

- [ ] **Step 2: Umziehen, Naht legen, `view_floor` anheben, Gates, Commit**

---

## Task 5: Festschreiben

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Die Extraktionsregel in §4**

Was diese Welle gelernt hat, gilt für jede weitere Extraktion: erst herauslösen
ohne Crate-Wechsel, dann bewegen; die Übersetzungsgrenze wird beim Herauslösen
gezogen, nicht beim Bewegen.

- [ ] **Step 2: Ledger-Eintrag mit den Zahlen**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach dieser Welle

`column_widths.rs` und `queue_row_mapping.rs` sind dann freigeschaltet (§2.5) —
zusammen rund 300 Zeilen, die als ganze Dateien ziehen können. Danach ist der
in §2.6 gemessene Vorrat erschöpft, und die Frage aus §2.6 steht an: jede
Oberfläche trägt ihre Logik selbst, oder sie wird für die geteilte Schicht neu
geschrieben. Das ist eine Produktentscheidung und gehört nicht in einen
Wellenplan.
