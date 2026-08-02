---
slug: p1a-welle6-freigeschaltet
worktree: /home/marvin/Projects/reprise-p1a-welle6
branch: feature/p1a-welle6-unblocked
phase: planned
codex_session:
created: 2026-08-02
---
# P1a Welle 6 — Was Welle 5 freigeschaltet hat

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `column_widths.rs` und `queue_row_mapping.rs` ziehen nach
`reprise-view`. Beide waren blockiert, weil ihre einzige Abhängigkeit in einer
toolkit-schweren Wirtsdatei saß; Welle 5 hat `ColumnId` und `QueueSection`
bewegt und damit die Blockade aufgehoben.

**Basis:** `dev` (Wellen 0 bis 5 sind gelandet, `view_floor` steht bei 806).

**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md` §2.5, §2.6, §4

## Der Zuschnitt, gemessen (2026-08-02, nach Welle 5)

| Datei | LOC | Toolkit | `Rc`/`Cell`/Thread | externe Aufrufstellen |
| --- | --- | --- | --- | --- |
| `track_list/queue_row_mapping.rs` | 244 | 0 | 0 | **51** |
| `track_list/column_widths.rs` | 92 | 0 | 0 | 2 |

Beide importieren heute nur einen Typ, und zwar jeweils über die
Adapter-Naht aus Welle 5:

- `column_widths.rs` → `crate::ui::column_layout::ColumnId`, ein Re-Export von
  `reprise_view::columns::ColumnId`
- `queue_row_mapping.rs` → `super::queue_sections::{QueueSection,
  QueueSectionKind}`, Re-Exporte von `reprise_view::queue`

**Das ist das Welle-1-Muster, nicht das Welle-5-Muster.** Regel 13 (erst
herauslösen, dann bewegen) gilt hier nicht: Es gibt nichts herauszulösen, die
Dateien sind bereits ganz. Sie ziehen als Ganzes um und tauschen dabei ihren
Import von der Naht auf die Crate.

**Damit ist der in §2.6 gemessene Vorrat erschöpft.** Nach dieser Welle gibt es
keinen weiteren Kandidaten, den eine gegnerische Prüfung überlebt hat.

## Eine Frage, die diese Welle beantworten muss

§2.6 hat ein drittes Kriterium eingeführt: **würden alle drei Oberflächen es
wollen?** Bei `queue_row_mapping.rs` ist die Antwort klar — Umsortier-Operationen
auf einer Warteschlange sind universell.

Bei `column_widths.rs` ist sie es nicht. Es serialisiert `id:width`-Paare für
**Tabellenspalten**, und eine Compose-Oberfläche auf einem Telefon hat
vermutlich gar keine breitenverstellbaren Spalten. Ein Tauri-Desktop dagegen
hätte sie.

**Task 2 entscheidet das, statt es anzunehmen.** Beide Antworten sind
vertretbar: „zieht mit, weil der Desktop-Zuschnitt es braucht" ebenso wie
„bleibt, weil es GTK-Tabellenmechanik ist". Was nicht vertretbar ist, ist die
Frage nicht zu stellen — Welle 3 hat `result_count_markup` genau daran
aufgeteilt.

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
- **Baseline:** 3955 passed, 0 failed, 413 ignored, 56 Suiten (`dev` nach #230).
- **Keine der 51 + 2 Aufrufstellen ändert sich.**
- **Kein `#[allow(…)]`** gegen eine Warnung des eigenen Umbaus.
- **`view_floor` steigt in jedem Commit, der Code bewegt.**

---

## Task 1: `queue_row_mapping.rs`

244 Zeilen, 51 Aufrufstellen — die meisten, die eine Datei dieser Reihe je
hatte. Die Naht muss sie alle tragen.

**Files:**
- Create: `crates/reprise-view/src/queue_rows.rs` (oder als Modul unter `queue`)
- Modify: `crates/reprise-view/src/lib.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/mod.rs`
- Delete: `crates/reprise-gnome/src/ui/track_list/queue_row_mapping.rs`
- Modify: `scripts/check-frontend-thinness.sh`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Umziehen**

Der Import wechselt von `super::queue_sections::{…}` auf
`crate::queue::{…}` — innerhalb von `reprise-view` ist das kein Re-Export
mehr, sondern das Original. Sichtbarkeiten werden `pub`.

**Ob die Datei ein eigenes Modul wird oder ein Untermodul von `queue`,
entscheidest du an ihrem Inhalt.** Sie arbeitet auf `QueueSection`; wenn sie
dort hineingehört, gehört sie hinein.

- [ ] **Step 3: Die Naht**

Re-Export in `track_list/mod.rs` nach Welle-1-Muster, sodass alle 51
Aufrufstellen unverändert auflösen.

- [ ] **Step 4: `view_floor` anheben, volle Gates, Commit**

---

## Task 2: `column_widths.rs` — erst entscheiden, dann bewegen

- [ ] **Step 1: Die Frage aus §2.6 beantworten**

Lies, was die Datei tut und wer sie ruft (zwei Stellen). Entscheide, ob eine
Compose- und eine Tauri-Oberfläche dieselbe Serialisierung wollen. Schreib die
Entscheidung samt Begründung in die Commit-Nachricht — **auch wenn sie „nein"
lautet.** Ein begründetes Nein ist hier ein vollwertiges Ergebnis und beendet
Task 2.

- [ ] **Step 2: Falls ja — umziehen, Naht legen, `view_floor` anheben**

Der Import wechselt von `crate::ui::column_layout::ColumnId` auf
`crate::columns::ColumnId`.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 3: Den Vorrat für erschöpft erklären

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: §3 abschließen**

Nach dieser Welle ist kein Kandidat mehr übrig, den die gegnerische Prüfung
aus §2.6 überlebt hat. Das gehört dort hingeschrieben, mit dem erreichten
`view_floor` — nicht als Scheitern, sondern als Ergebnis einer Messung.

Die Frage, die danach ansteht, ist keine Welle: jede Oberfläche trägt ihre
Logik selbst, oder die Logik wird für die geteilte Schicht neu geschrieben.
**Diese Entscheidung gehört dem Eigentümer, nicht diesem Plan.** Schreib sie
als offene Frage hin, nicht als Empfehlung.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**
