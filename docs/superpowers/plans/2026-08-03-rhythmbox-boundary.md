---
slug: rhythmbox-boundary
worktree: /home/marvin/Projects/reprise-rhythmbox
branch: feature/rhythmbox-boundary
phase: in-progress
created: 2026-08-03
---
# Multi-Surface P3 — Rhythmbox als ausdrueckliche Quellenfaehigkeit

**Ziel:** Der Rhythmbox-Import ist eine ausdruecklich optionale Faehigkeit
einer `LibrarySource`. Eine Pfadquelle unterstuetzt diese Faehigkeit, ein
DocumentsProvider-Baum nicht. Eine Surface bietet den Einstieg nur an, wenn
ihre Quelle die Faehigkeit besitzt; ein unbekanntes Sondierungsergebnis wird
nicht als bestaetigte Abwesenheit ausgegeben.

**Basis:** `dev` (`5fff82d152`).

## Verifizierte Messung

1. `reprise-gnome::default_rhythmdb_path()` leitet den Pfad aus
   `glib::user_data_dir()` beziehungsweise dem vorhandenen Test-Override ab.
   Diese Entscheidung liegt bereits bei der Surface und bleibt dort.
2. `rhythmbox_data_available()` verwendet `Path::is_file()` und verdichtet
   bestaetigte Abwesenheit, fehlende Berechtigung und jeden anderen
   Sondierungsfehler auf `false`. `LibraryPathPresence` ist die bereits
   vorhandene verlustfreie Antwort.
3. `rhythmbox_import.rs` oeffnet `rhythmdb.xml` in `parse_rhythmdb` und
   `prescan_rhythmdb_with_source` sowie `playlists.xml` in `parse_playlists`
   direkt mit `File::open`; der Prescan liest den Aenderungszeitpunkt direkt
   mit `std::fs::metadata`.
4. Die Ausgangsmessung war um eine Stelle zu klein:
   `prescan_rhythmdb_with_source` fragt `playlists_path.is_file()` ebenfalls
   direkt. Diese Sondierung gehoert zum selben Schnitt und wird mitgenommen.
5. `prescan_rhythmdb_with_source` ist heute nur fuer die im XML genannten
   Musikdateien quellenbewusst. Die beiden Rhythmbox-Dateien selbst laufen an
   seinem `source`-Parameter vorbei; sein Name beschreibt deshalb nur einen
   Teil der Implementierung.

## Vertrag und Naht

`LibrarySource` erhaelt die verpflichtende Methode
`rhythmbox_import_capability() -> RhythmboxImportCapability`. Der benannte
Enum hat genau zwei Werte:

- `Supported`: Die Quelle hat eine Vorstellung von einer importierbaren
  Rhythmbox-Sammlung und kann deren von der Surface gewaehlte Pfade mit ihren
  bestehenden Lese- und Sondierungsoperationen beantworten.
- `Unsupported`: Diese Speicherwelt besitzt keine solche Sammlung. Ein
  DocumentsProvider-Baum antwortet so; die Surface darf den Einstieg nicht
  anbieten.

Es gibt keine Vorgabeimplementierung. Jeder Adapter muss die Frage bewusst
beantworten und scheitert sonst beim Kompilieren. Der Unix-Pfadadapter ist
`Supported`, der SAF-Adapter `Unsupported`.

Die Faehigkeit ist nicht die Anwesenheit einer konkreten Datei. Fuer diese
zweite Frage bleibt `LibrarySource::probe` zustaendig:

- `Present` mit einer regulaeren Datei bestaetigt den Einstieg.
- `Absent` widerlegt ihn.
- `Present` ohne regulaere Datei widerlegt genau die erwartete Dateiform.
- `Unknown` bleibt unbekannt und darf nicht in `Absent` umgedeutet werden.

Die GNOME-Surface behaelt die Pfadwahl. Ihre Angebotsregel kombiniert die
Faehigkeit der aktiven Quelle mit deren `LibraryPathPresence`. Bei
`Unsupported` bleibt die Zeile verborgen. Bei `Supported` bleibt das heutige
Desktop-Verhalten fuer vorhandene Datei, fehlenden Pfad und Verzeichnis
unveraendert; `Unknown` wird nicht mehr als Beweis fuer "kein Rhythmbox"
behandelt.

Core liest `rhythmdb.xml` und `playlists.xml` ueber `open_read`, gewinnt den
Aenderungszeitpunkt aus `probe` und versucht bei `Unknown` weiterhin den
eigentlichen Lesevorgang. Nur `Absent` beziehungsweise eine bestaetigte
Nicht-Datei unterdrueckt den optionalen Playlist-Import. Die bestehenden
pfadbasierten Funktionen bleiben als schmale Unix-Adapter erhalten, damit
Desktop-Aufrufer und ihre Tests unveraendert bleiben.

## Vertikale Umsetzung

### Commit 1 — Entwurf

Nur dieses Dokument. Keine Verhaltensaenderung.

Commit: `docs(library): design optional Rhythmbox capability`

### Commit 2 — Core-Vertrag und Import

- `RhythmboxImportCapability` und die verpflichtende Trait-Methode einfuehren.
- Unix- und SAF-Adapter sowie alle Testadapter bewusst klassifizieren.
- Quellenbewusste Parser fuer Datenbank und Playlisten einfuehren; die
  bestehenden Pfadfunktionen bleiben unveraenderte Unix-Wrapper.
- Prescan-Metadaten, -Lesen und optionale Playlisten durch dieselbe Quelle
  fuehren.
- Eine in-memory Quelle beweist Lesen ohne Dateisystem; eine nicht
  unterstuetzende Quelle beweist den expliziten Fehler. Beide neuen
  Verhaltenswaechter werden vor der Implementierung rot beobachtet.

Commit: `refactor(library): make Rhythmbox import a source capability`

### Commit 3 — GNOME-Angebotsregel

- Die vorhandene Pfadentscheidung beibehalten.
- Angebotsregel aus Quellenfaehigkeit und verlustfreier Praesenz bilden.
- `Unsupported` verbirgt die Zeile; `Unknown` wird nicht als `Absent`
  behandelt.
- Die vorhandenen Desktop-Tests bleiben bytegleich und gruen. Neue
  Policy-Tests werden vor der Implementierung rot beobachtet.

Commit: `refactor(gnome): gate Rhythmbox import on source capability`

### Commit 4 — Nachweis

- Android-Spike um die korrigierte Messung und den gelandeten Vertrag
  ergaenzen.
- Fortschrittsledger mit Commits, Gates, Testbilanz, Annahmen und
  verbleibenden manuellen Pruefungen aktualisieren.

Commit: `docs: record the optional Rhythmbox boundary`

## Gates vor jedem Commit

Jeder Befehl laeuft einzeln und sein Exit-Code wird einzeln erfasst:

- `cargo fmt --check`
- `cargo clippy --all-targets --workspace -- -D warnings`
- `cargo test --workspace`
- `cargo audit` (nur `RUSTSEC-2024-0436` akzeptiert)
- `bash scripts/check-architecture.sh`
- `bash scripts/check-frontend-thinness.sh`
- `bash scripts/tests/gettext-catalogs.sh`

Die Testbilanz wird aus dem vollstaendigen Log nach den Schluesselwoertern
`passed` und `failed` summiert. Zusaetzlich wird bei jedem Gate geprueft, ob
seine Ausgabe wirkliche Arbeit dieser Ausfuehrung belegt. Nach Core-Aenderungen
bleibt der Abhaengigkeitsbaum frei von GTK, libadwaita, GStreamer und zbus;
jede substanziell bearbeitete Rust-Datei bleibt unter 800 Zeilen.
