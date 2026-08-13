---
slug: library-source-residence
worktree: ~/Projects/reprise-libsource
branch: feature/library-source-residence
phase: planned
codex_session:
created: 2026-08-02
---
# Storage-Abstraktion, Paket 1 — Aufenthalt und Erreichbarkeit

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Feststellung „Laufwerk weg" gegen „Datei gelöscht" läuft über ein
Trait statt über POSIX-`st_dev` direkt. Damit ist die erste Naht der
`LibrarySource`-Abstraktion gelegt, an der kleinsten Stelle, die die Form
beweist.

**Dies ist keine P1a-Welle.** Der Umzugsvorrat von P1a ist mit Welle 6
geschlossen. Das hier ist Paket 1 der Storage-Abstraktion, die `reprise-core`
für Android SAF öffnet — Arbeit am Kern, nicht am Frontend.

**Basis:** `dev`.

**Belege:** `docs/research/android-spike-2026-08.md` §Frage 7 und ihre
Korrektur, sowie §Frage 8 (bereits umgesetzt, #230).

## Warum dieser Cluster zuerst

§Frage 7 hat 27 Stellen gezählt, die über die Abstraktion müssen. Die
Korrektur dazu hat gezeigt, dass die gefürchtete Entwurfsfrage kleiner ist als
gedacht: `classify_missing` ist **zehn Zeilen und bereits plattformneutral** —
es fragt nur, ob ein gespeichertes Merkmal mit dem aktuellen übereinstimmt.
Plattformabhängig ist genau **eine** Funktion, `nearest_existing_ancestor_dev`.

Gemessen am 2026-08-02:

| | |
| --- | --- |
| `classify_missing` Aufrufstellen | **2** (`scanner_vanish.rs:148`, `queries/maintenance.rs:365`) |
| `nearest_existing_ancestor_dev` externe Aufrufer | **1** (`scanner_vanish.rs:121`) |
| `library/mounts.rs` | 288 Zeilen |

Drei Aufrufstellen. Das ist der kleinste Schnitt, an dem sich die Trait-Form
erproben lässt, bevor die rund zwanzig mechanischen Stellen folgen.

## Die Form, nicht erfunden sondern abgeschaut

`reprise-core` hat bereits vier Traits dieser Art — `FingerprintBackend: Send +
Sync`, `PlaybackBackend`, `EventProvider: Send`, `LyricsProvider`. **Das neue
Trait folgt deren Hausform**, nicht der Skizze in der Spec, wo beide
auseinandergehen. Die Spec-Skizze ist ein Entwurf, kein Vertrag.

Für dieses Paket braucht es nur zwei Methoden:

```rust
fn residence_token(&self, at: &Path) -> Option<i64>;
fn reachability(&self, at: &Path, stored: Option<i64>) -> MissingReason;
```

Die zweite hat eine Vorgabe-Implementierung: sie **ist** `classify_missing`,
ausgedrückt über die erste. Eine Quelle, die ein Merkmal liefern kann, bekommt
die Klassifikation geschenkt.

**Was dieses Paket nicht anfasst:** `walk`, `open`, `open_rw`, `watch` und alle
Datei-Ein-/Ausgabe. Die kommen in eigenen Paketen. Wer sie hier mitnimmt,
macht den ersten Schnitt unprüfbar.

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
- **Baseline:** 3955 passed, 0 failed, 413 ignored, 56 Suiten.
- **Auf Linux darf sich nichts ändern.** Jede heutige Klassifikation muss
  nachher identisch ausfallen. Das ist die Hauptanforderung.
- **Kein `#[allow(…)]`** gegen eine Warnung des eigenen Umbaus.
- **Keine neue Abhängigkeit**, kein Schema-Wechsel.

---

## Task 1: Das Trait und die Unix-Quelle

**Files:**
- Create: `crates/reprise-core/src/library/source.rs`
- Modify: `crates/reprise-core/src/library.rs` oder `mod.rs`
- Modify: `crates/reprise-core/src/library/mounts.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Das Trait, nach Hausform**

Lies zuerst `playback.rs`s `PlaybackBackend` und `fingerprint.rs`s
`FingerprintBackend` — **wie dort dokumentiert und benannt wird, wird auch hier
dokumentiert und benannt.** Insbesondere: `PlaybackBackend` führt vor, wie
dieses Repo eine Fähigkeit beschreibt, die eine Implementierung nicht hat
(„documented degradation, never a failure"). Diese Haltung gilt hier ebenso —
eine Quelle ohne stabiles Merkmal antwortet `None`, sie täuscht keins vor.

`reachability` bekommt eine Vorgabe-Implementierung, die `classify_missing`s
heutige Logik unverändert wiedergibt.

- [ ] **Step 3: Die Unix-Implementierung**

Sie kapselt `nearest_existing_ancestor_dev`. Der Doc-Kommentar dieser Funktion
erklärt ausführlich, warum `lstat` und nicht `stat` — **dieser Text zieht mit,
er ist der Grund, warum die Funktion korrekt ist.**

- [ ] **Step 4: Volle Gates und Commit**

Noch ohne Aufrufstellen-Umbau. Das Trait existiert, niemand benutzt es.

---

## Task 2: Die drei Aufrufstellen umlegen

**Files:**
- Modify: `crates/reprise-core/src/library/scanner_vanish.rs`
- Modify: `crates/reprise-core/src/queries/maintenance.rs`
- Modify: `crates/reprise-core/src/library/mounts.rs`

- [ ] **Step 1: Wie kommt die Quelle an die Aufrufstelle?**

Das ist die eigentliche Entwurfsfrage dieses Pakets, und sie ist nicht
vorentschieden. Drei Wege stehen offen, und dieses Repo führt alle drei
irgendwo vor:

- ein Parameter, wie `PlaybackBackend` es tut
- ein injizierter Closure, wie `trash_tracks.rs` es für die Löschung tut
- ein `Effect`-Enum, das eine Plattformschicht ausführt, wie
  `device_sync/machine.rs` es tut

**Lies alle drei, entscheide begründet, und schreib die Begründung in die
Commit-Nachricht.** Maßgeblich ist, was an *diesen* Aufrufstellen am wenigsten
Reibung erzeugt — `scanner_vanish` läuft im Scan, `maintenance` in einer
Query.

- [ ] **Step 2: Umlegen**

`classify_missing` bleibt als Funktion bestehen oder verschwindet in die
Vorgabe-Implementierung — auch das entscheidest du. Bestehende Tests von
`mounts.rs` müssen unverändert grün bleiben; sie sind der Beweis für
Verhaltensgleichheit.

- [ ] **Step 3: Der Beweis, dass es trägt**

Ein Test mit einer **zweiten** Implementierung des Traits — einer, die ein
Merkmal aus etwas anderem als `st_dev` ableitet. Sie muss dieselbe
Dreiteilung liefern. Das ist der eigentliche Zweck des Pakets: zu zeigen, dass
die Klassifikation nicht an POSIX hängt.

Diese Testquelle gehört **nicht** hinter `#[cfg(test)]` versteckt, wenn sie
später als SAF-Vorlage dient — aber das entscheidet sich erst, wenn es eine
SAF-Quelle gibt. Für jetzt reicht `#[cfg(test)]`.

- [ ] **Step 4: Volle Gates und Commit**

---

## Task 3: Festhalten

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: §Frage 7 nachziehen**

Paket 1 ist umgesetzt; die verbleibenden Stellen der 27 benennen, damit das
nächste Paket nicht neu messen muss. Halte fest, **welche Zuführungsform** in
Task 2 gewonnen hat — daran richten sich die Folgepakete aus.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach diesem Paket

Paket 2 wären die rund zwanzig mechanischen Stellen — `.exists()`,
`metadata()`, die vier `walkdir`-Läufe über denselben Baum. Paket 3 die
Ein-/Ausgabe über Handles, wo `tag_mutation.rs`s eine Naht die gesamte
Tag-Schreib-Oberfläche abdeckt.

Beide erst, wenn die Zuführungsform aus Task 2 sich an drei Aufrufstellen
bewährt hat.
