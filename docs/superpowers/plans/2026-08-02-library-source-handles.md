---
slug: library-source-handles
worktree: /home/marvin/Projects/reprise-library-source-handles
branch: feature/library-source-handles
phase: planned
codex_session:
created: 2026-08-02
---
# Storage-Abstraktion, Paket 4 — der Lese-Griff

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `reprise-core` öffnet eine Bibliotheksdatei **zum Lesen** über
`LibrarySource` statt über `std::fs::File`. Damit ist beantwortet, ob ein
Griff die Abstraktion überhaupt überqueren kann.

**Basis:** `dev` (`73c7b11b34`, Paket 3 gemergt).

**Belege:** `docs/research/android-spike-2026-08.md`, Abschnitt „Die
Klassifikation, die Paket 3 freigibt" und dessen Korrektur.

## Der Bestand, gemessen am 2026-08-02

84 Produktiv-E/A-Stellen in `reprise-core` (`File::open`, `File::create`,
`OpenOptions`, `fs::read`, `fs::write`, `fs::rename`, `fs::remove_file`,
`fs::copy`, …; Testdateien und `#[cfg(test)]`-Blöcke ausgeschlossen). Davon
sind **55 app-privat** — Thumbnail-Cache, Portrait-Cache, Podcast-Downloads,
HTTP-Fixtures, die Datenbankdatei selbst.

Klasse A, also Bibliotheksdateien, sind **18**:

| Datei | Zeilen | Was |
| --- | --- | --- |
| `writeback_publish.rs` | 154, 166, 186, 247, 270 | Cover in den Albumordner veröffentlichen |
| `library/scanner_repair.rs` | 48, 61, 69, 70 | Temp-Datei, dann Rename auf den Track |
| `library/tag_mutation.rs` | 285, 286, 365, 366 | Track ganz lesen, verändert ganz schreiben |
| `lyrics/local.rs` | 39, 82 | `.lrc`-Sidecar, eingebettete Lyrics |
| `provenance.rs` | 194, 221 | Track zum Hashen öffnen |
| `cover.rs` | 272 | Ordnerbild lesen |

Sechs Stellen der Rhythmbox-Dateien sind **Klasse C** und bleiben unangetastet;
`db.rs:49` und die Cache-Schreibpfade in `cover.rs` sind **Klasse B**.

## Warum nur die Lese-Seite

Die 18 zerfallen in zwei Sorten, und sie zu vermischen wäre derselbe Fehler,
den die Wellen-Pakete gemacht hätten:

**Lesen** ist ein Griff. „Gib mir etwas, aus dem ich lesen kann."

**Schreiben ist hier kein Griff, sondern eine Zusicherung.**
`writeback_publish.rs:154` benutzt `OpenOptions::create_new` — das ist nicht
„öffne eine Datei", sondern „beanspruche diesen Namen unteilbar **oder
scheitere**", und das ganze Modul existiert, um ein vorhandenes Cover niemals
zu überschreiben. `scanner_repair.rs:69` benutzt `rename` — nicht „verschiebe",
sondern „ersetze unteilbar". **Unter SAF gibt es beides so nicht:**
`DocumentsContract.createDocument` scheitert bei einem Namenskonflikt nicht,
sondern legt stillschweigend `cover (1).png` an. Genau die Zusicherung, auf der
das Modul beruht, kippt dort ins Gegenteil.

Das ist eine eigene Frage und bekommt ein eigenes Paket. **Dieses Paket fasst
keine schreibende Stelle an.**

## Die Entwurfsfrage

**Kann ein Griff die Abstraktion überqueren — und wie?**

`Box<dyn Read + Seek>` löst es in Rust und **nicht** über UniFFI: der Spike
hält fest, dass weder Closures noch fremde Trait-Objekte die Brücke
überqueren. Auf Android liefert `ContentResolver.openFileDescriptor` aber
einen echten POSIX-Dateideskriptor, den Rust mit `File::from_raw_fd`
übernehmen kann. Ein Griff *kann* also ankommen — die Frage ist, was im
Vertrag steht, damit das möglich bleibt.

Beachte, was die Verbraucher wirklich brauchen: `provenance.rs` liest
streamend, `lyrics/local.rs:39` liest eine kleine Datei ganz,
`tag_mutation.rs:285` liest eine ganze Audiodatei in den Speicher, `lofty`
will `Read + Seek`. **Miss das zuerst**, und trage genau so viel im Vertrag,
wie gebraucht wird — ein Griff, der `Seek` verspricht, obwohl niemand es
braucht, verbaut jede Quelle, die nur streamen kann.

Entscheide begründet und schreib die Begründung in die Commit-Nachricht.

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`,
  `bash scripts/tests/gettext-catalogs.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe. Testbilanz nach
  **Schlüsselwort** summieren.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Baseline:** 3966 passed, 0 failed, 415 ignored, 56 Suiten.
- **Keine schreibende Stelle.** Nicht `writeback_publish.rs`, nicht
  `scanner_repair.rs`, nicht `tag_mutation.rs`s Schreibhälfte (Zeile 286 und
  366). Die Lesehälfte (285, 365) gehört dazu — aber nur, wenn sie sich ohne
  die Schreibhälfte umstellen lässt; wenn nicht, lass beide und sag es.
- **Keine Klasse B oder C.**
- **Nichts bekommt eine Vorgabe-Implementierung.** Paket 3 hat gezeigt, warum:
  eine Quelle, die eine Frage nicht beantworten kann, muss am Compiler
  scheitern, nicht mit einer Antwort, die „nicht da" bedeutet.
- **Kein `#[allow(…)]`**, keine neue Abhängigkeit, kein Schema-Wechsel.

---

## Task 1: Messen, was die Verbraucher brauchen

**Files:** keine Änderung — dieser Task erzeugt Erkenntnis, keinen Code.

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die Bedarfsanalyse**

Für jede der Klasse-A-Lesestellen: braucht sie `Read`, `Seek`, beides, oder
nur „gib mir den ganzen Inhalt"? Halte das Ergebnis fest — es bestimmt die
Signatur, und es ist die einzige Grundlage, auf der „so viel wie nötig, nicht
mehr" entschieden werden kann.

`lofty` ist der harte Fall: prüfe, was `tag_mutation.rs` ihm tatsächlich gibt
und ob das schon heute ein `Vec<u8>` ist statt eines Griffs.

---

## Task 2: Der Vertrag und die Unix-Quelle

**Files:**
- Modify: `crates/reprise-core/src/library/source.rs`

- [ ] **Step 1: Die Form**

Nach Hausform: benannt, objekt-sicher, ohne Vorgabe. `LibrarySource` wird als
`&dyn` gereicht — was du zurückgibst, muss das aushalten.

- [ ] **Step 2: Die Unix-Implementierung**

- [ ] **Step 3: Volle Gates und Commit**

Noch ohne Aufrufstellen.

---

## Task 3: Die Lesestellen umlegen

**Files:** die Klasse-A-Lesestellen aus der Tabelle oben

- [ ] **Step 1: Umlegen**

Zuführungsform steht seit Paket 1 fest: `&dyn LibrarySource`-Parameter mit
schmalen Vorgabe-Wrappern an der öffentlichen Grenze. Nicht neu entscheiden.

- [ ] **Step 2: Der Beweis**

Ein Test, der eine dieser Stellen über eine Quelle fährt, deren Inhalt **nicht
aus einer Datei** kommt — ein `Vec<u8>` im Test genügt. Ohne ihn ist der Griff
nur ein umbenanntes `File::open`.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 4: Festhalten

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Die E/A-Klassifikation aufnehmen**

84 Stellen, 55 app-privat, 18 Klasse A, davon welche erledigt. Und — wichtiger
als die Zahlen — **die Zusicherungsfrage benennen**, die Paket 5 tragen muss:
`create_new` und `rename` sind Unteilbarkeitsversprechen, die SAF nicht gibt.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach diesem Paket

Paket 5 ist die Schreibseite, und es ist kein Umzug: `create_new` und `rename`
sind Zusicherungen, die unter SAF neu erdacht werden müssen. Danach bleiben
`mount_point_of` (fünf Aufrufstellen, ohne SAF-Entsprechung), die acht
Bibliothekszugriffe in `reprise-gnome`, die Plattformgrenze für Rhythmbox und
`watcher.rs`.
