---
slug: library-doctor-out-of-date-rows-are-unreadable
worktree: /home/marvin/Projects/reprise-library-doctor-out-of-date-rows-are-unreadable
branch: feature/library-doctor-out-of-date-rows-are-unreadable
phase: shipped
codex_session:
created: 2026-08-20
---
# Der Doctor veraltet seine eigenen Zeilen

Der Library Doctor zeigt „out of date" / „Stale" an drei Stellen, ohne dass ein
Nutzer daraus ableiten kann, was zu tun ist. Beschluss: **die veralteten Zeilen
gar nicht mehr zeigen und im Hintergrund nachziehen.**

Der Grill hat drei Punkte festgezurrt, die den Zuschnitt bestimmen:

- **Verbergen, nicht erklären.** Zweimal bestätigt, ausdrücklich auch für den
  Fall, dass nach der Wurzelkorrektur kaum etwas übrig bleibt.
- **„Verbergen" heißt ganz raus aus der Sitzung** — nicht eine eigene Kategorie,
  nicht bloß ein Banner. Die Zeilen werden **gar nicht erst** aufgenommen.
- **Die Reihenfolge ist hart:** die Schnappschuss-Korrektur landet **vor** dem
  Verbergen. Andersherum schluckt der Doctor bei jedem Apply stillschweigend
  eigene Restarbeit — die selbstverschuldet veralteten Zeilen verschwänden
  einfach, statt sichtbar falsch zu sein. Das ist keine Stilfrage.

**Alle Zeilenangaben gegen `51e9c6c9bb`.** Basis dieses Worktrees ist
`origin/dev` = `40655644fc`; die zwei Commits dazwischen (#583, #584) fassen nur
`.github/workflows/ci.yml` und `scripts/tests/cua-*.sh` an, keine Quelldatei
dieses Plans. Die Zeilennummern gelten unverändert — trotzdem vor dem Ändern
kurz gegenlesen.

## Zwei Befunde, die den Auftrag verändern

Die Quellenlage weicht in zwei Punkten von der Annahme des ursprünglichen
Befunds ab. Beide sind belegt, und beide gehören vor die Aufgaben, weil sie
bestimmen, was „nachziehen" überhaupt heißen kann.

### 1. Der Zählwiderspruch ist keiner

Der Befund hielt „408 ready" gegen „139 stale" und vermutete einen Zählfehler.
Es ist keiner: **die beiden Mengen sind disjunkt.** Beide Zahlenwege filtern
hart auf `Ready`:

- `review_model.rs:197-205` baut `selectable_row_ids` mit
  `row.state == DoctorReviewRowState::Ready`;
- `review_snapshot.rs:82-99` summiert `totals.changes` aus genau diesen IDs;
- `review_snapshot.rs:226-232` (`review_ready_count`) filtert `session.rows()`
  ebenfalls auf `Ready`.

Einzige Ausnahme ist die Album-Kopfzeile im blockierten Zustand
(`review_snapshot.rs:91`, `AlbumCounts.changes` über `row.row_ids.len()`) — das
ist der Text „N changes · out of date" und zählt bewusst alle Zeilen.

**Folge für den Plan:** Es gibt nichts zu reparieren an der Arithmetik. Und weil
die veralteten Zeilen nach Task 4 gar nicht mehr in der Sitzung stehen,
verschwindet der scheinbare Widerspruch von selbst — es bleibt nur noch eine
Zahl übrig.

### 2. Der Doctor erzeugt seine veralteten Zeilen selbst

Das ist der eigentliche Befund, und er ist größer als „unlesbar".

`stale_flags()` (`store.rs:373-418`) vergleicht einen Fingerabdruck **pro
Track** — `path, file_mtime, file_size, device, inode` — gegen den Schnappschuss
aus `library_doctor_scan_tracks`:

```rust
let changed = current.is_none_or(|current| current != snapshot);
```

Der Doctor schreibt beim Anwenden in die Datei. Das bewegt `file_mtime`. Der
Kommentar bei `store.rs:510-516` weiß das ausdrücklich:

> our own write moves the file's mtime and a moved mtime reads as "changed under
> us", i.e. as stale, and stale rows fall out of the quiet tier into review.

Dort wird nur das *bereits geschriebene* `(track_id, field)`-Paar herausgefiltert.
Der Fingerabdruck hängt aber **am Track, nicht am Feld**. Also veraltet ein
Apply auf ein Feld **jede weitere offene Zeile desselben Tracks**.

Und: **es gibt im gesamten `library_doctor`-Verzeichnis kein
`UPDATE library_doctor_scan_tracks`.** Der Schnappschuss ist auf den
Scan-Zeitpunkt eingefroren. Die Veraltung ist damit **dauerhaft**, bis jemand
„Scan again" drückt.

**Folge für den Plan:** Genau deshalb steht die Wurzelkorrektur vor dem
Verbergen. Würde nur verborgen, wären die selbstverschuldeten Zeilen still
weg — der Doctor würde bei jedem Apply eigene Restarbeit unterschlagen, ohne
dass es irgendwo auffiele.

Wieviel davon selbstverschuldet ist, ist noch **nicht gemessen**. Task 1 misst es.

### 3. Die Veraltung hat zwei Implementierungen

Derselbe Fingerabdruck wird an zwei Stellen unabhängig berechnet:

| Stelle | speist |
| --- | --- |
| `load_tracks()` / `current_identity()` (`store.rs:302-342`) | die Review-Sitzung (die Liste) |
| `stale_flags()` (`store.rs:373-418`) | `queries/doctor.rs:31` → das **Badge** in der Seitenleiste |

Eine Entscheidung, zwei Orte. Solange das so bleibt, meldet die Seitenleiste
nach Task 4 Befunde, die die Liste gar nicht mehr zeigt. Deshalb ist die
Zusammenführung eine **eigene Aufgabe** (Task 3) und keine Aufräumarbeit
nebenbei.

## Aufgaben

### Task 1 — Die Herkunft der veralteten Zeilen messen

Bevor etwas verborgen oder nachgezogen wird: **wieviele der veralteten Zeilen
stammen aus eigenen Apply-Läufen, und wieviele aus echten Fremdänderungen?**

Ein Test in `reprise-core` auf einer synthetischen Doctor-Sitzung:
Scan anlegen, mehrere Proposals auf **denselben** Track, eines anwenden, dann
`stale_flags()` erneut lesen. Erwartung, die es zu belegen gilt: die übrigen
Zeilen desselben Tracks stehen danach auf `stale`, obwohl niemand von außen
etwas angefasst hat.

Der Test ist zunächst der **Kontrollarm** und benennt die Zahl: wieviele Zeilen
veralten pro angewandter Zeile.

**Akzeptanz:** Aus dem Fehlerprotokoll ist ablesbar, dass ein einziger Apply
weitere Zeilen desselben Tracks veraltet — als Zahl, nicht als Behauptung.

### Task 2 — Der Schnappschuss zieht nach, was der Doctor selbst geschrieben hat

**Diese Aufgabe muss vor Task 4 landen. Ohne Ausnahme.**

Nach einem erfolgreichen Schreibvorgang wird der Fingerabdruck des betroffenen
Tracks in `library_doctor_scan_tracks` auf den neuen Dateizustand gehoben.

Das ist die enge, belegbare Form von „im Hintergrund nachziehen": sie betrifft
**nur** Tracks, die der Doctor selbst angefasst hat, und sie behauptet nichts
über Fremdänderungen. Eine Fremdänderung bleibt `stale` — zu Recht, denn dort
weiß der Doctor wirklich nicht mehr, was in der Datei steht.

**Zu beachten:** Der Schnappschuss darf erst nach dem *erfolgreichen* Schreiben
gehoben werden, und mit dem tatsächlich gelesenen neuen Zustand — nicht mit einem
vorausberechneten. Sonst behauptet er einen Dateizustand, den niemand geprüft
hat, und das ist schlimmer als eine veraltete Zeile.

**Akzeptanz:** Der Test aus Task 1 kippt: nach dem Apply stehen die übrigen
Zeilen desselben Tracks weiter auf `Ready`.

### Task 3 — Eine Quelle für die Veraltung

Die beiden Implementierungen aus Befund 3 werden zusammengeführt:
`load_tracks()` / `current_identity()` (`store.rs:302-342`) und `stale_flags()`
(`store.rs:373-418`) teilen sich künftig **eine** Funktion, die den
Fingerabdruck bildet und vergleicht. Beide Aufrufer — Sitzung und
`queries/doctor.rs:31` (Badge) — hängen danach an derselben Entscheidung.

Das ist kein Aufräumen: ohne diesen Schritt zeigt das Badge in der Seitenleiste
nach Task 4 eine Zahl an, zu der die Liste keine einzige Zeile hat. Die
Zusammenführung ist die Voraussetzung dafür, dass „verbergen" auch im Badge
gilt.

**Akzeptanz:** Es gibt genau eine Stelle im Verzeichnis, die den
Track-Fingerabdruck gegen den Schnappschuss hält. Ein Test belegt, dass Badge
und Sitzung dieselbe Menge veralteter Tracks sehen.

### Task 4 — Die veralteten Zeilen kommen gar nicht erst in die Sitzung

Erst jetzt, und erst nachdem Task 2 und Task 3 stehen.

Zeilen, deren Track laut Fingerabdruck veraltet ist, werden **beim Aufbau der
Review-Sitzung ausgelassen** — nicht in eine eigene Kategorie sortiert, nicht
ausgegraut, nicht mit Banner erklärt. Sie sind nicht da. Damit entfallen auch
die drei Anzeigeorte, die den Zustand heute benennen:

| Ort | Stelle | Zeichenkette |
| --- | --- | --- |
| Banner | `review_summary.rs:31-39` → `review_page.rs:92,112-114` | `strings_library_doctor.rs:381-388` (`doctor_stale_notice`) |
| Album-Kopfzeile | `review_header.rs:82-108` (Zeile 99-101) | `strings_library_doctor.rs:433-440` |
| Source-Spalte | `review_row.rs:264-265` → `review_model.rs:368-374` | `strings_library_doctor.rs:84` (`DOCTOR_STATUS_STALE`) |

**Der Zustand `Stale` selbst bleibt bestehen.** Er hat einen zweiten,
**manuellen** Pfad (`review.rs:636` `mark_state`, GUI-Aufrufer
`review_page.rs:314-336` und `:412-435`), der von dieser Änderung nicht berührt
wird. Was entfällt, ist ausschließlich der **fingerabdruck-getriebene** Weg in
die Sitzung. Zeichenketten und Anzeigeorte, die der manuelle Pfad noch braucht,
bleiben; die anderen fallen mitsamt ihren msgids.

Der Weg zurück für eine echt fremdgeänderte Datei bleibt „Scan again" — der
liest den Ordner neu und legt einen frischen Schnappschuss an. Für die heute
bereits bestehenden veralteten Zeilen ist das auch der einzige Weg: Task 2 wirkt
nur nach vorn, sie heilt keinen alten Schnappschuss. Ein einmaliger
Nachzieh-Lauf ist ausdrücklich **nicht** Teil dieses Plans.

**Akzeptanz:** Eine Sitzung über einen Bestand mit veralteten Tracks enthält
keine einzige Zeile für diese Tracks, und das Badge zählt sie ebenfalls nicht.

### Task 5 — Die Regeln nachziehen

`docs/ux-rules.md`:

- **DOC-9b** (`:4665-4711`) trägt heute die Norm für genau diese drei
  Anzeigeorte, einschließlich des Satzes, dass eine abgelehnte Zeile *„performs
  no refresh"*. Die Regel muss auf den neuen Stand: der eigene Schreibvorgang
  hebt den Schnappschuss (Task 2), und fingerabdruck-veraltete Zeilen erscheinen
  nicht mehr in der Sitzung (Task 4).
- **DOC-8b** (`:4565-4587`, „never when its track is stale") **braucht einen
  Zusatz.** Bisher beschreibt die Regel, dass eine veraltete Zeile aus dem
  stillen Tier in die Review fällt. Nach Task 4 fällt sie aus **beiden** Tiers —
  weder still angewandt noch angezeigt. Das deckt der heutige Text nicht ab, und
  ohne den Zusatz verspricht die Regel etwas, das die Oberfläche nicht mehr
  einlöst.
- **DOC-3a** (`:4264-4280`) bleibt unverändert gültig — sie beschreibt den
  manuellen Umgang mit veralteten Zeilen, und den ändert dieser Plan nicht.
  Zusammen mit dem manuellen `mark_state`-Pfad verwaist hier nichts.

**Akzeptanz:** Traceability-Gate grün; DOC-9b nennt den Selbstveraltungsfall,
DOC-8b nennt den Fall „aus beiden Tiers".

### Task 6 — Gegenmessung

Zwei Proben, beide **genau ein Vorkommen**, beide erst nach dem Commit
(`git checkout --` verschluckt Uncommittetes wortlos):

1. Die Schnappschuss-Aktualisierung aus Task 2 wieder entfernen → der Test aus
   Task 1 wird rot.
2. Den Ausschluss aus Task 4 wieder entfernen → der Test aus Task 4 wird rot.

## Nicht in diesem Plan

- **Den Watcher andocken.** `crates/reprise-core/src/library/watcher.rs` ist ein
  inotify-Watcher, der bei Dateiänderungen `scanner::scan_folder` nachzieht und
  dabei genau die Felder aktualisiert, die der Fingerabdruck vergleicht — er
  rührt den Doctor-Schnappschuss aber nicht an und stößt keinen Doctor-Rescan
  an. Ihn anzudocken wäre das vollständige „im Hintergrund nachziehen", ist aber
  deutlich mehr Fläche und berührt einen eigenen Thread mit eigener
  DB-Verbindung. Der Plan bleibt bei der engen Form.
- **Ein einmaliger Nachzieh-Lauf für die heute bestehenden 139 Zeilen.** Sie
  verschwinden durch Task 4 aus der Anzeige; geheilt werden sie durch „Scan
  again".
- **`crates/reprise-mcp`** (`doctor_dto.rs`, `doctor_tools.rs`) — der Agent-Pfad
  transportiert „stale" möglicherweise ebenfalls; nicht geprüft, eigener Befund.
- **Die Vorauswahl-Regeln.** `starts_selected` (`review.rs:97-101`) und
  `is_auto_applied` (`:73-84`) schließen `Stale` hart aus. Das ist richtig und
  bleibt.

## Belege

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`
- `scripts/check-ux-traceability.sh`
- die betroffenen Display-Tests **einzeln**: `review_page_tests.rs:148`
  (`doc_9b_stale_notice_follows_category_filter_and_is_hidden_at_zero`),
  `review_row_contract_tests.rs:304`
  (`doc_9b_a_stale_row_names_its_reason_where_the_click_happens`) — beide
  beschreiben Anzeigeorte, die Task 4 entfernt oder umdeutet; sie werden
  umgeschrieben, nicht gelöscht, solange der manuelle Pfad sie noch trägt
- die beiden Mutationsproben aus Task 6

## Parallelität

**Ein Strang.** Die Reihenfolge ist inhaltlich erzwungen, nicht bloß bequem:
Task 1 ist der Kontrollarm, Task 2 die Wurzelkorrektur, Task 3 macht das
Verbergen im Badge überhaupt wirksam, und erst Task 4 verbirgt.

**Reihenfolge:** 1 → 2 → 3 → 4 → 5 → 6. **Task 2 vor Task 4 ist verbindlich.**

**Dateibesitz dieses Strangs:**

```
crates/reprise-core/src/library/library_doctor/store.rs
crates/reprise-core/src/library/library_doctor/review.rs
crates/reprise-core/src/library/library_doctor/review_tests.rs
crates/reprise-core/src/library/queries/doctor.rs
crates/reprise-gnome/src/ui/library_doctor/review_summary.rs
crates/reprise-gnome/src/ui/library_doctor/review_header.rs
crates/reprise-gnome/src/ui/library_doctor/review_model.rs
crates/reprise-gnome/src/ui/library_doctor/review_row.rs
crates/reprise-gnome/src/ui/library_doctor/review_page.rs
crates/reprise-gnome/src/ui/library_doctor/jobs.rs
crates/reprise-gnome/src/ui/strings_library_doctor.rs
crates/reprise-gnome/src/ui/library_doctor/review_page_tests.rs
crates/reprise-gnome/src/ui/library_doctor/review_row_contract_tests.rs
docs/ux-rules.md
po/*
```

**Achtung, geteilte Dateien:** `docs/ux-rules.md` und `po/*` gehören auch den
vier Geschwisterplänen dieser Welle und dem Strang
`queue-centering-ignores-section-headers`. Verschiedene Regeln, dieselbe Datei —
der Konflikt wird **beim Landen** aufgeräumt, nicht vorher vermieden.

**Post-Merge-Querprüfungen:** keine.
