---
slug: library-source-tags
worktree: ~/Projects/reprise-library-source-tags
branch: feature/library-source-tags
phase: planned
codex_session:
created: 2026-08-02
---
# Storage-Abstraktion, Paket 5 — das Loch, das `lofty` verdeckt hat

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Tag-Lesungen laufen über `LibrarySource::open_read` statt über
`lofty::read_from_path`. Danach kann ein Scan über eine fremde Quelle nicht
mehr an jedem einzelnen Track scheitern.

**Basis:** `dev` (`1850384607`, Paket 4 gemergt).

**Belege:** `docs/research/android-spike-2026-08.md`, Abschnitt „Alle
Inventuren zusammengeführt — und das Loch, das keine davon sah".

## Warum dieses Paket vor der Schreibseite kommt

`lofty::read_from_path(path)` und `lofty::probe::Probe::open(path)` rufen
intern `std::fs::File::open`. **Kein Muster über `fs::`, `File::open` oder
`.exists()` findet sie**, weshalb sie in drei aufeinanderfolgenden Messungen
unsichtbar blieben. Es sind 13 Stellen im Kern.

Die folgenschwerste ist `scanner_meta::read_meta`: sie liest die Tags **jedes
Tracks, den der Scanner importiert**, und geht am Trait vorbei. Heute liefe ein
SAF-gestützter Scan korrekt durch Traversierung, Präsenzprüfung und
Klassifikation — und scheiterte dann an jedem einzelnen Track.

Das ist kein Schreibproblem und gehört deshalb nicht in das Schreibpaket. Es
ist eine Lesestelle, die nur ein Messfehler aus Paket 4 herausgehalten hat.

## Umfang

Reine Lesungen auf Bibliotheksdateien — **fünf Stellen**:

| Datei | Zeile | Funktion |
| --- | --- | --- |
| `library/scanner_meta.rs` | 132 | `read_meta` — Pass 1, jeder importierte Track |
| `library/scanner_meta.rs` | 184 | `read_meta_relaxed` — Pass 2 |
| `library/tag_edit.rs` | 125 | `read_editable_tags` |
| `library/tag_mutation_guarded.rs` | 114 | `read_tag_field_values` |
| `library/library_doctor/remote/metadata.rs` | 121 | prüfen und, falls Bibliothek, mitnehmen |

**Nicht in diesem Paket:**

- `scanner_meta.rs:148` — `read_meta_content_based` liest die **Temp-Datei**
  aus `scanner_repair`, nicht die Bibliothek. Siehe unten.
- `tag_mutation.rs:199, 289, 376, 486`, `tag_mutation_guarded.rs:201`,
  `provenance.rs:212` — an eine Schreiboperation gekoppelt.
- `podcasts/episode_tags.rs:106` — app-privat.

## Die Falle, und ihre Ausnahme sechzehn Zeilen weiter

`Probe::open(path)` setzt den Dateityp **aus der Endung** vor.
`Probe::new(reader)` kennt keinen Pfad und setzt nichts. In Paket 4 hat genau
das eine stille Regression erzeugt: `guess_file_type` ist `sniffed.or(seed)`,
also wurde aus einer gescheiterten Header-Erkennung „unbekanntes Format", wo
vorher die Endung rettete.

**Aber `read_meta_content_based` will genau das Gegenteil.** Es liest eine
Temp-Datei, der `scanner_repair` absichtlich eine Nicht-Audio-Endung gibt,
damit der Walk sie nicht als Track einsammelt — der Parser *muss* dort nach
Inhalt gewählt werden. Die Regel lautet also nicht „immer die Endung
vorsetzen", sondern: **jede Stelle behält die Typwahl, die sie vorher hatte.**

Für die fünf Stellen oben heißt das:

- `read_from_path(path)` ist `Probe::open(path)?.read()` — Typ **nur aus der
  Endung**, kein Schnüffeln. Die Ersetzung muss genau das tun, nicht mehr.
- Wenn die Endung unbekannt ist, ergibt `Probe::open` heute einen
  Lofty-Fehler aus `read()`. Ein `FileType::from_path(path)?`, das vorher mit
  einem *anderen* Fehler abbricht, ist **nicht** verhaltensgleich — die
  Fehlerklassifikation in `import_errors::classify_lofty` hängt daran.

## Die zweite Frage: der Scanner fasst jede Datei jetzt zweimal an

Seit Paket 3 sondiert der Scanner jede Audiodatei einmal für Metadaten. Mit
diesem Paket öffnet er sie zusätzlich für die Tags. Auf Linux sind das zwei
billige Syscalls. Unter SAF sind es zwei Rundläufe — und `openFileDescriptor`
liefert ohnehin keinen Metadatensatz, die Abfrage ist eine eigene.

**Miss, ob sich das zusammenlegen lässt**, und wenn nicht, halte begründet
fest, warum. Nicht raten: die letzten drei Pakete haben je einen doppelten
Zugriff gefunden, der auf Linux gratis war.

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
- **Baseline:** 3969 passed, 0 failed, 415 ignored, 56 Suiten.
- **Die Fehlerklassifikation muss identisch bleiben.** `classify_lofty`
  unterscheidet `UnreadableTags`, `UnsupportedFormat`, `PermissionDenied`,
  `Io` — diese Verdikte landen in `import_errors` und in der Oberfläche.
  Jede heutige kaputte Datei muss nachher dasselbe Verdikt bekommen. Die
  Fixtures `broken-tags.mp3` und `broken-front-id3v2-damaged-ape.mp3` sind
  dafür da.
- **Keine Schreibstelle, keine app-private, keine Temp-Datei.**
- **Keine Vorgabe-Implementierung**, kein `#[allow(…)]`, keine neue
  Abhängigkeit, kein Schema-Wechsel.

---

## Task 1: Der Ersatz für `read_from_path`

**Files:**
- Modify: `crates/reprise-core/src/library/scanner_meta.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Die verhaltensgleiche Ersetzung finden**

Lies zuerst `lofty`s eigenen Quelltext für `read_from_path` und `Probe::open`
— welche Typwahl, welcher Fehler bei unbekannter Endung. Erst danach schreiben.

- [ ] **Step 3: `read_meta` und `read_meta_relaxed` umlegen**

`read_meta_content_based` bleibt unverändert. Wenn du versucht bist, es der
Einheitlichkeit halber mitzunehmen: lies seinen Doc-Kommentar.

- [ ] **Step 4: Volle Gates und Commit**

---

## Task 2: Die drei übrigen Lesestellen

**Files:**
- Modify: `crates/reprise-core/src/library/tag_edit.rs`
- Modify: `crates/reprise-core/src/library/tag_mutation_guarded.rs`
- Modify: `crates/reprise-core/src/library/library_doctor/remote/metadata.rs`

- [ ] **Step 1: Prüfen, ob `library_doctor/remote/metadata.rs:121` dazugehört**

Bibliothek oder nicht — entscheide anhand dessen, woher der Pfad kommt, und
sag es im Commit.

- [ ] **Step 2: Umlegen**

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 3: Der Beweis

**Files:**
- Modify: `crates/reprise-core/src/library/scanner_source_tests.rs`

- [ ] **Step 1: Ein Scan, dessen Tags nicht aus einer Datei kommen**

`scanner_source_tests.rs` hat schon eine skriptgesteuerte Quelle. Erweitere
sie um `open_read` aus einem `Vec<u8>` und fahre einen vollständigen Scan
damit — die Tracks müssen mit ihren echten Tags in der Datenbank landen, ohne
dass der Scanner eine Datei geöffnet hat.

**Das ist die Aussage des Pakets.** Ohne diesen Test bleibt „ein SAF-Scan
würde funktionieren" eine Behauptung.

- [ ] **Step 2: Die Fehlerverdikte absichern**

Ein Test, der eine kaputte Datei über die Quelle liest und dasselbe
`ImportErrorKind` bekommt wie heute über den Pfad.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 4: Festhalten

**Files:**
- Modify: `docs/research/android-spike-2026-08.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Die 13 lofty-Stellen abhaken**

Welche umgestellt, welche mit welchem Grund nicht. Und das Ergebnis der
Doppelzugriffs-Messung.

- [ ] **Step 2: Ledger-Eintrag**

- [ ] **Step 3: Volle Gates und Commit**

---

## Nach diesem Paket

Erst dann die Schreibseite — und die ist kein Umzug: `create_new` und `rename`
sind Unteilbarkeitszusicherungen, die SAF nicht gibt. Danach `mount_point_of`
(fünf Aufrufstellen ohne SAF-Entsprechung), die acht Bibliothekszugriffe in
`reprise-gnome`, die Plattformgrenze für Rhythmbox und `watcher.rs`.
