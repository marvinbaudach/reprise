---
slug: p1a-welle4-waveform
worktree: /home/marvin/Projects/reprise-p1a-welle4-waveform
branch: feature/p1a-welle4-waveform
phase: planned
codex_session:
created: 2026-08-02
---
# P1a Welle 4 — Der letzte freie Umzug

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `player_bar/waveform_shape.rs` zieht nach `reprise-view`. 125 Zeilen,
kein einziger Import.

**Warum das eine eigene Welle ist:** Es ist die **einzige** Datei in den
verbleibenden 47.943 gemessenen Zeilen, die ohne Vorarbeit umziehen kann. Die
Neuvermessung in §3 des Wellenplans hat acht Kandidaten geprüft und sieben
widerlegt. Ab der nächsten Welle ist die Arbeit Herauslösen statt Umziehen —
diese hier schließt die Umzugsphase ab.

**Basis:** `dev` (P0 und Wellen 0 bis 3 sind gelandet).

**Spec:** `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md` §3, neue Fassung

## Der Zuschnitt

| Datei | LOC | Imports | Aufrufstellen |
| --- | --- | --- | --- |
| `player_bar/waveform_shape.rs` | 125 | **keine** | 2, beide in `waveform_seek.rs` |

Es exportiert `aggregate_rms`, `smooth_neighbors`, `shape_display_peaks`,
`DisplayBar` und `SILENCE_DOT_HEIGHT`. Reine Signalformung über `f32`/`f64`:
RMS-Aggregation, Nachbarglättung, und die Regel, dass Buckets unter −50 dB
relativ zum eigenen Maximum als feste 2-px-Punkte statt als skalierte Balken
erscheinen.

**Nichts daran ist GTK.** Eine Compose-Oberfläche, die dieselbe Wellenform
zeichnet, braucht exakt dieselben Zahlen — und laut Spike-Befund trägt die
Wellenform-Spitzen ohnehin das Gerät, statt sie neu zu berechnen.

## Global Constraints

- **Gates vor dem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe lesen — eine Pipe meldet
  ihren eigenen Erfolg und hat in Welle 1 zweimal einen Clippy-Fehler verdeckt.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Baseline:** 3922 passed, 0 failed, 410 ignored, 56 Suiten (Welle 3 auf `dev`).
- **Keine Aufrufstelle ändert sich.** Beide liegen in `waveform_seek.rs` und
  müssen nach dem Umzug unverändert auflösen.
- **Kein `#[allow(…)]`** gegen eine Warnung des eigenen Umbaus.
- **Bekannt rot, nicht von dieser Welle:** `scripts/tests/gettext-catalogs.sh`
  scheitert auf jedem Branch am fehlenden `po/ar.po`-Eintrag für
  `"Play this track"`. Gegen die Basis gegenprüfen, dass es derselbe eine
  Fehler ist.

---

## Task 1: Der Umzug

**Files:**
- Create: `crates/reprise-view/src/waveform.rs`
- Modify: `crates/reprise-view/src/lib.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/mod.rs`
- Delete: `crates/reprise-gnome/src/ui/player_bar/waveform_shape.rs`
- Modify: `scripts/check-frontend-thinness.sh`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die Datei umziehen**

Unverändert, Sichtbarkeiten `pub` (Welle-1-Regel). Die Tests ziehen mit.

- [ ] **Step 3: Die `Send + Sync`-Zusicherung — nur wenn sie etwas sichert**

`DisplayBar` ist der einzige eigene Typ. Trägt er Daten, bekommt er die
Zusicherung nach Welle-2-Muster, **mit Mutationsbeweis**: ein `Rc<()>`-Feld
einführen, den Compile-Fehler sehen, zurücknehmen, beides mit den gesehenen
Zahlen in der Commit-Nachricht.

Ist `DisplayBar` dagegen ein reiner `Copy`-Wert aus Zahlen, dann **keine
Zusicherung schreiben**. Welle 3 hat gezeigt, wohin das führt: dort stand
`assert_send_sync::<String>()`, eine Tautologie, die nie fehlschlagen kann und
nichts gesichert hat. Eine Zusicherung, die nicht scheitern kann, ist keine.

- [ ] **Step 4: Die Naht**

Re-Export in `player_bar/mod.rs` nach Welle-1-Muster, sodass beide
`use super::waveform_shape::{…}`-Zeilen in `waveform_seek.rs` unverändert
auflösen.

- [ ] **Step 5: `view_floor` anheben, volle Gates, Commit**

---

## Task 2: Das Ende der Umzugsphase festhalten

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Den Übergang im Wellenplan markieren**

§3 sagt bereits voraus, dass dies der letzte freie Umzug ist. Nach dem Vollzug
gehört dort vermerkt, dass er stattgefunden hat und ab Welle 5 die
Extraktionsarbeit beginnt — mit dem erreichten `view_floor` als Zahl.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach dieser Welle

Welle 5 löst `QueueSection` aus `queue_sections.rs` und `ColumnId` aus
`column_layout.rs` heraus — beides reine Datentypen in toolkit-schweren
Wirtsdateien, die zusammen drei der vier `track_list`-Kandidaten blockieren.
Kein Crate-Wechsel, keine Bewegung von `view_floor`. Das ist die erste Welle,
deren Erfolg sich nicht am Boden ablesen lässt.
