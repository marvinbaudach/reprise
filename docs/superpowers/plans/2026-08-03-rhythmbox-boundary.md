---
slug: rhythmbox-boundary
worktree: ~/Projects/reprise-rhythmbox
branch: feature/rhythmbox-boundary
phase: complete
created: 2026-08-03
---
# Multi-Surface P3 — Rhythmbox ueber generische Quellenprimitive lesen

**Ziel:** Der Rhythmbox-Import greift in `reprise-core` nicht direkt auf das
Dateisystem zu. Die GNOME-Surface entscheidet ueber ihren eigenen Einstieg und
behandelt ein unbekanntes Sondierungsergebnis nicht als bestaetigte
Abwesenheit. Der universelle `LibrarySource`-Vertrag traegt keine Frage ueber
eine einzelne fremde GNOME-Anwendung.

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

Die GNOME-Surface besitzt Pfadwahl und Angebotsregel. Fuer die konkrete Datei
bleibt `LibrarySource::probe` zustaendig:

- `Present` mit einer regulaeren Datei bestaetigt den Einstieg.
- `Absent` widerlegt ihn.
- `Present` ohne regulaere Datei widerlegt genau die erwartete Dateiform.
- `Unknown` bleibt unbekannt und darf nicht in `Absent` umgedeutet werden.

Damit bleibt das heutige Desktop-Verhalten fuer vorhandene Datei, fehlenden
Pfad und Verzeichnis unveraendert; `Unknown` wird nicht mehr als Beweis fuer
"kein Rhythmbox" behandelt.

Core liest `rhythmdb.xml` und `playlists.xml` ueber `open_read`, gewinnt den
Aenderungszeitpunkt aus `probe` und versucht bei `Unknown` weiterhin den
eigentlichen Lesevorgang. Nur `Absent` beziehungsweise eine bestaetigte
Nicht-Datei unterdrueckt den optionalen Playlist-Import. Die bestehenden
pfadbasierten Funktionen bleiben als schmale Unix-Adapter erhalten, damit
Desktop-Aufrufer und ihre Tests unveraendert bleiben.

Die quellengestuetzten Importfunktionen behalten `&dyn LibrarySource` als
Leser. Das ist bereits die echte Speichernaht fuer `open_read` und `probe` und
hat sowohl Produktions- als auch in-memory Adapter. Ein kleiner
Rhythmbox-spezifischer Trait daneben wuerde dieselben Primitive nur duplizieren.
Ein konkreter `UnixLibrarySource`-Parameter waere enger, wuerde aber den
quellenreinen Test ohne Dateisystem verwerfen und Core wieder an eine konkrete
Speicherimplementierung binden.

Nichts im Typ verhindert eine kuenftige nicht-GNOME-Surface daran, diese
Importfunktionen bewusst aufzurufen. Dafuer gibt es heute keinen Aufrufer und
keine Anforderung: der gesamte Produktionsfluss liegt in
`preference_rhythmbox.rs`, waehrend CLI, MCP und Android keine Verdrahtung
besitzen. Ein Abwehr-Guard gegen diesen nicht existierenden Aufrufer waere
keine Sicherheitsgrenze.

## Korrektur nach Owner-Review

Die ersten vier Commits dieses Plans versuchten eine verpflichtende
`RhythmboxImportCapability` auf `LibrarySource`. Die anschliessende Messung
widerlegte diese Form: Zwoelf Implementierungen mussten die Methode tragen,
aber es gab nur zwei Produktionsaufrufe. Der einzige Oberflaechenaufruf fragte
einen konkreten `UnixLibrarySource`; der zweite war der Core-Guard selbst.

Die Korrektur entfernt deshalb die Trait-Methode, den Enum, alle zwoelf
Implementierungen, `require_rhythmbox_import` und `UnsupportedSource`. Sie
behaelt die unabhaengig richtigen Teile: alle fuenf direkten
Dateisystemzugriffe bleiben entfernt, XML wird weiter ueber `LibrarySource`
gelesen, und nur `Absent` beziehungsweise eine bestaetigte Nicht-Datei darf das
GNOME-Angebot unterdruecken.

## Vertikale Umsetzung

### Commit 1 — Entwurf

Nur dieses Dokument. Keine Verhaltensaenderung.

Commit: `docs(library): design optional Rhythmbox capability`

### Commit 2 — Core-Vertrag und Import (historisch, korrigiert)

- Die spaeter zurueckgezogene `RhythmboxImportCapability` einfuehren.
- Quellenbewusste Parser fuer Datenbank und Playlisten einfuehren; die
  bestehenden Pfadfunktionen bleiben unveraenderte Unix-Wrapper.
- Prescan-Metadaten, -Lesen und optionale Playlisten durch dieselbe Quelle
  fuehren.
- Eine in-memory Quelle beweist Lesen ohne Dateisystem. Der spaeter entfernte
  Unsupported-Test belegte nur den spekulativen Guard.

Commit: `refactor(library): make Rhythmbox import a source capability`

### Commit 3 — GNOME-Angebotsregel (historisch, korrigiert)

- Die vorhandene Pfadentscheidung beibehalten.
- Angebotsregel zunaechst aus Quellenfaehigkeit und verlustfreier Praesenz
  bilden; die Korrektur reduziert sie auf Praesenz allein.
- `Unknown` wird nicht als `Absent` behandelt.
- Die vorhandenen Desktop-Tests bleiben bytegleich und gruen. Neue
  Policy-Tests werden vor der Implementierung rot beobachtet.

Commit: `refactor(gnome): gate Rhythmbox import on source capability`

### Commit 4 — Nachweis

- Android-Spike um die korrigierte Messung und den gelandeten Vertrag
  ergaenzen.
- Fortschrittsledger mit Commits, Gates, Testbilanz, Annahmen und
  verbleibenden manuellen Pruefungen aktualisieren.

Commit: `docs: record the optional Rhythmbox boundary`

### Commit 5 — Scoped correction

- Die spekulative Faehigkeit vom universellen Vertrag und allen Adaptern
  entfernen.
- Den nicht belegten Core-Guard samt Fehlerfall entfernen.
- Quellenbasiertes Lesen und den `Unknown`-Waechter behalten.
- Messung, Nahtentscheidung und fehlende nicht-GNOME-Sperre dokumentieren.

Commit: `refactor(library): withdraw the Rhythmbox source capability`

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
