# Desktop-Dateizuordnungen — Design

## Ziel

Reprise soll in GNOME als geeignete Anwendung für die bereits unterstützten lokalen
Audioformate sowie M3U/M3U8-Wiedergabelisten erscheinen. Ein Doppelklick oder „Öffnen mit
Reprise“ muss sowohl beim ersten Start als auch bei einer bereits laufenden Instanz denselben
Anwendungspfad auslösen.

## Verhalten

- Die Desktop-Datei deklariert die Audio-MIME-Typen für MP3, FLAC, Ogg/Vorbis, Opus, M4A,
  AAC und WAV sowie M3U/M3U8. `Exec` übergibt mehrere lokale Dateien an Reprise.
- `GtkApplication` verwendet `HANDLES_OPEN`; Sekundärprozesse leiten Open-Anfragen über die
  bestehende Single-Instance-Verbindung an das Primärfenster weiter.
- Lokale Audiodateien werden kanonisch gegen die vorhandene Bibliothek aufgelöst. Gefundene
  Titel werden in der übergebenen Reihenfolge als ein Wiedergabekontext gestartet.
- Nicht in der Bibliothek vorhandene Audiodateien werden nicht automatisch importiert, nicht
  gescannt und nicht in die Datenbank geschrieben. Ein übersetzter Toast erklärt das Ergebnis;
  vorhandene Titel derselben Anfrage werden trotzdem abgespielt.
- M3U/M3U8-Dateien verwenden unverändert `playlist_io::import_playlist` und denselben UI-
  Ergebnispfad wie der Import in der Seitenleiste. Damit gelten dieselben Regeln: Nur bereits
  bekannte Tracks werden übernommen und bei null Treffern entsteht keine leere Playlist.
- Nichtlokale URIs, Verzeichnisse und nicht deklarierte Dateitypen werden ignoriert und sichtbar
  als nicht unterstützt gemeldet. Das aktuelle Playback bleibt unverändert, wenn nichts
  Abspielbares aufgelöst wurde.
- Jede Open-Anfrage präsentiert das vorhandene Fenster; sie erzeugt niemals ein zweites Fenster,
  einen zweiten Player oder eine zweite Datenbankverbindung.

## Architektur

- `main.rs` besitzt einen kleinen, wiederverwendbaren Open-Handler. `connect_activate` baut wie
  bisher das Fenster; `connect_open` baut es bei Bedarf einmal und übergibt die Dateien danach an
  denselben Handler. Ein `RefCell`-Borrow wird nie über Fensterbau oder Callback-Aufruf gehalten.
- `ui/file_open.rs` kapselt Klassifikation, Bibliothekspfadauflösung und die GTK-Komposition aus
  Player, Playlist-Import, Sidebar, Toast-Overlay und Fenster.
- `window::build` gibt den vollständig verdrahteten Handler zurück. Die große Window-Komposition
  erhält keine zweite Öffnungslogik.
- `playlist_io::apply_import_result` wird nur innerhalb `ui` sichtbar gemacht, damit Dialog,
  Smoke-Hook und Dateizuordnung exakt denselben UI-Abschluss teilen.
- AppStream nennt dieselben Medientypen wie die Desktop-Datei; Reprise setzt sich nicht selbst als
  Standardanwendung, sondern bietet nur die Zuordnung an.

## Fehlerbehandlung

- Nichtlokale oder unbekannte Dateien erzeugen einen Toast, keinen Panic.
- Datenbank-Lookup-Fehler werden geloggt und wie ein nicht aufgelöster Titel behandelt.
- Ist GStreamer nicht verfügbar, bleibt Playlist-Import möglich; Audio-Open zeigt einen
  übersetzten Playback-Fehler.
- Ein fehlerhafter Playlist-Import nutzt unverändert den vorhandenen Fehler-Toast.

## Tests und QA

- Reine Unit-Tests prüfen Groß-/Kleinschreibung der Endungen, lokale Klassifikation,
  kanonische Bibliotheksauflösung, Reihenfolge und Teiltreffer.
- Ein Metadatenvertrag prüft `Exec`, `MimeType` und AppStream-`mediatype` auf Konsistenz.
- Main-/Application-Tests prüfen `HANDLES_OPEN` sowie den einen Handler für Aktivierung und Open.
- Vollständig isolierte Anwendungssmokes öffnen einen gescannten FLAC-Titel und eine M3U-Datei
  über echte Kommandozeilenargumente. Sie verwenden Scratch-XDG, eigenen D-Bus, Xvfb und
  `fakesink`.
- Der tatsächliche „Öffnen mit“-Eintrag und Doppelklick unter GNOME bleiben manuelle Checks nach
  einer Meson-Installation.

## Explizit nicht Teil

- Kein automatisches Setzen von Reprise als Standardanwendung.
- Kein stiller Import und keine temporäre Wiedergabe bibliotheksfremder Audio-Dateien.
- Keine Netzwerk-Streams oder entfernten URI-Schemata.
- Keine neuen Playlistformate neben M3U/M3U8.
- Keine Änderung an Musikdateien, Scanner-Wurzeln oder realen Benutzerdaten.
