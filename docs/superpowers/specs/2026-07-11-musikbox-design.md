# Musikbox — Design-Dokument

**Datum:** 2026-07-11
**Status:** Entwurf — wartet auf finale Nutzer-Review

## Zusammenfassung

Ein Musikplayer für Linux (GNOME/Wayland). Die Bibliotheksansicht orientiert
sich an Rhythmbox (sortierbare Spaltenliste für große Bibliotheken), das
visuelle Design folgt Variante **2a** aus `Musikplayer.pdf` (dunkles Schema,
Playerleiste unten im Spotify-Stil, Blur-/Glow-Optik) und ist per
Einstellungen auf die anderen PDF-Varianten umschaltbar.

**Plattform-Entscheidung:** Linux-only-Fokus. Eine macOS-Portierung ist kein
Ziel mehr (Nutzer-Entscheidung: unwahrscheinlich, dass portiert wird) — der
Tauri-Stack hält sie theoretisch offen, aber sie beeinflusst keine
Entscheidungen. Konsequenz: GStreamer als Audio-Engine statt Rodio/Symphonia.

## Positionierung: Rhythmbox-Nachfolger

Rhythmbox ist faktisch im Wartungsmodus (GTK3; der GTK4-Port ist trotz
mehrerer Anläufe gescheitert). Musikbox positioniert sich als moderner
Nachfolger:

- **Freie Software, GPL-3.0** — passt zur Rhythmbox-Herkunft und zur
  GNOME-Community.
- **Migrationspfad im MVP:** Import von `rhythmdb.xml` (Bewertungen,
  Play Counts, zuletzt gespielt, hinzugefügt am) und `playlists.xml`
  (statische Playlists). Ein Nachfolger, der die Altdaten nicht übernimmt,
  wird nicht angenommen.
- **Verteilung als Flatpak** (Flathub) als primärer Kanal; GStreamer-
  Plugins kommen über die Freedesktop-Runtime mit.
- Vertraute Rhythmbox-Konzepte bleiben erhalten (Spaltenbibliothek,
  intelligente Playlists, Warteschlange, Missing Files, Modul-Schalter),
  in modernem Gewand.

## Ziele und Nicht-Ziele

### MVP-Umfang

- Lokale Musikbibliothek: Ordner scannen, Tags lesen, durchsuchen, abspielen
- Spaltenansicht mit virtuellem Scrolling (flüssig bei 100k+ Titeln)
- Wiedergabe-Warteschlange, Shuffle, Repeat
- 5-Sterne-Bewertungen, Play Count, „zuletzt gespielt"
- Manuelle Playlists
- Intelligente Playlists: „Zuletzt gespielt", „Top bewertet", „Zuletzt hinzugefügt"
- Suche über alle Felder
- Automatische Ordner-Überwachung: neue/geänderte/gelöschte Dateien werden
  erkannt (notify-Crate), zusätzlich manueller Rescan per Button
- Titel löschen: aus der Bibliothek entfernen oder Datei in den Papierkorb
  verschieben (trash-Crate), jeweils mit Bestätigung
- Tag-Editor: Metadaten (Titel, Interpret, Album, Album-Interpret, Jahr,
  Track-Nr., Genre) bearbeiten und via lofty in die Datei schreiben
- MPRIS-Integration (GNOME-Mediensteuerung, Medientasten)
- Layout-Einstellungen: Position der Playerleiste (unten / oben in der
  Headerbar / schwebende Insel — die Varianten 2a, 1a, 1c aus dem PDF),
  Farbschema (dunkel / hell), sichtbare Spalten und Spaltenbreiten
- 10-Band-Equalizer mit Presets (GStreamer `equalizer-10bands`),
  ein-/ausschaltbar, Einstellungen werden gespeichert
- ReplayGain-Lautstärkeangleichung (GStreamer `rgvolume`): liest
  ReplayGain-Tags aus den Dateien, Modus Titel / Album / Aus
- Internes Modulsystem mit An/Aus-Liste in den Einstellungen (wie
  Rhythmbox' Plugins-Tab); Equalizer und ReplayGain sind die ersten Module
- Rhythmbox-Import: `rhythmdb.xml` (Bewertungen, Play Counts, zuletzt
  gespielt) und `playlists.xml`, mit Pfad-Abgleich gegen die gescannte
  Bibliothek und Import-Bericht (Anzahl übernommen / nicht zugeordnet)
- Browse-Leiste wie in Rhythmbox: einblendbare Filterspalten
  Genre / Interpret / Album über der Titelliste, kombinierbar mit der Suche
- Cover-Art-Pipeline: eingebettete Cover (lofty) und Bilddateien im
  Albumordner (`cover.*`, `folder.*`), Thumbnail-Cache auf Platte
- Drag & Drop: Titel in Playlists ziehen, Warteschlange und Playlists
  per Drag umsortieren
- Tastatur-Shortcuts: u. a. Leertaste Play/Pause, Strg+F Suche,
  Entf Löschen-Dialog, Pfeiltasten-Navigation in der Liste
- GNOME-Benachrichtigung bei Titelwechsel (Cover, Titel, Interpret)
- Datei-Assoziation + Single-Instance: „Öffnen mit Musikbox" aus dem
  Dateimanager; ein zweiter Start reicht die Dateien an die laufende
  Instanz weiter

### Bewusst NICHT im MVP

- Last.fm/Libre.fm-Scrobbling — aber **priorisiert als erstes Modul direkt
  nach dem MVP** (der Nutzer scrobbelt aktiv; die Lücke soll kurz bleiben)
- Podcasts, Internetradio
- Schreiben von *Bewertungen* in Dateitags (Ratings bleiben in der DB;
  der Tag-Editor schreibt nur die klassischen Metadaten)
- Regel-Editor für intelligente Playlists (die drei vordefinierten zuerst;
  das Regelsystem ist aber von Anfang an generisch)
- Rhythmbox-Altlasten, die bewusst nicht übernommen werden:
  CD rippen/brennen, DAAP-Sharing, FM-Radio, IM-Status
- Crossfade und Online-Cover-Suche: spätere Module

Nichts davon verbaut die Architektur — alle Punkte sind später ergänzbar.

## Technikstack

| Schicht | Wahl | Begründung |
|---|---|---|
| Shell | Tauri 2 | Natives Wayland-Fenster (WebKitGTK), Rust-Backend, volle Designfreiheit für den 2a-Look |
| Backend | Rust | Scanner, Datenbank, Audio, MPRIS |
| Audio | GStreamer (gstreamer-rs, playbin3) | Auf Linux triviale Systemabhängigkeit; jedes Format, Gapless und ReplayGain eingebaut — dieselbe Engine wie Rhythmbox |
| Tags | lofty | Einheitliches Tag-Lesen und -Schreiben über alle Formate |
| Watcher | notify | Plattformübergreifende Ordner-Überwachung (inotify/FSEvents) |
| Löschen | trash | Dateien in den Papierkorb statt endgültig löschen |
| Datenbank | SQLite (rusqlite) | Bewertungen und Statistiken liegen in der App-Datenbank (Entscheidung: keine Tag-Schreibzugriffe) |
| MPRIS | zbus | D-Bus-Anbindung für GNOME-Mediensteuerung und Medientasten |
| Frontend | React + TypeScript + Vite | Volle Designfreiheit für den 2a-Look |
| Listen | TanStack Virtual | Virtuelles Scrolling der Spaltenansicht |
| State | Zustand | Leichtgewichtig, passt zu IPC-Event-Modell |

Verworfene Alternativen: **GTK4/libadwaita** (beste GNOME-Integration und
natives GtkColumnView, aber das PDF-Design — Blur, Glow, Layout-Varianten —
ist in GTK-CSS nicht umsetzbar; es würde eine Adwaita-Interpretation);
**Electron** (RAM-Overhead, Node statt Rust); **Rodio/Symphonia** (war für
macOS-Portabilität gewählt; nach der Linux-only-Entscheidung schlägt
GStreamer es klar: Gapless, ReplayGain, Equalizer, alle Formate eingebaut).

## Architektur

```text
musikbox/
├── src-tauri/                  Rust-Backend
│   └── src/
│       ├── library/            Scanner (walkdir + lofty), SQLite-Zugriff,
│       │                       Watcher (notify), Tag-Schreiben, Löschen (trash),
│       │                       Rhythmbox-Import (rhythmdb.xml, playlists.xml),
│       │                       Cover-Extraktion + Thumbnail-Cache
│       ├── player/             GStreamer playbin3: Play/Pause/Seek, Gapless
│       │                       (about-to-finish), Lautstärke; audio-filter-
│       │                       Kette: rgvolume (ReplayGain) → equalizer-10bands
│       ├── queue/              Warteschlange, Shuffle, Repeat
│       ├── playlists/          Manuell + intelligent (Regeln → SQL)
│       ├── mpris/              zbus: GNOME-Mediensteuerung, Medientasten
│       └── ipc/                Tauri-Commands + Event-Definitionen
└── src/                        React-Frontend
    ├── components/
    │   ├── sidebar/            Bibliothek / Playlisten / Intelligent
    │   ├── browse-bar/         Filterspalten Genre / Interpret / Album
    │   ├── track-table/        Spaltenansicht, Sortierung, Sternebewertung
    │   ├── player-bar/         Infos, Transport, Lautstärke — rendert in den
    │   │                       Slot der gewählten Layout-Variante
    │   └── ui/                 Buttons, Slider, Stars, Toast
    ├── hooks/                  usePlayerEvents, useTrackWindow
    ├── lib/                    IPC-Wrapper, Formatierung, Farb-Extraktion
    └── styles/                 tokens.css, typography.css, global.css
```

### Modulsystem (Rhythmbox-Plugin-Gedanke, intern)

Die App ist von Anfang an intern modular — Vorbild ist Rhythmbox' Plugin-
Liste, aber ohne Fremd-Plugin-API:

- Jedes optionale Feature (später: Scrobbling, Podcasts, Lyrics,
  Interpreten-Infos, Android-Sync) ist ein abgeschlossenes **Modul** mit
  eigenem Backend-Teil (Rust-Trait `Module`: init/shutdown, eigene
  Commands/Events) und optionalem Frontend-Teil.
- **Erweiterungspunkte** sind definiert und werden vom Kern bereitgestellt:
  Sidebar-Einträge, Kontextmenü-Aktionen, Einstellungs-Seiten,
  Detail-Panel-Tabs, Audio-Pipeline-Elemente.
- In den Einstellungen gibt es eine **Modul-Liste mit An/Aus-Schaltern**
  (wie Rhythmbox' Plugins-Tab); Zustand in der `settings`-Tabelle.
  Equalizer und ReplayGain sind die ersten Module und beweisen das System
  im MVP.
- Eine echte **Fremd-Plugin-API** (zur Laufzeit ladbar, z. B. WASM) bleibt
  spätere Ausbaustufe und setzt auf denselben Erweiterungspunkten auf.

### Kommunikation (IPC)

- **Frontend → Backend:** Tauri-Commands, z. B. `scan_folder`, `get_track_window`
  (Sortierung + Filter + Offset/Limit), `play_track`, `seek`, `set_rating`,
  `create_playlist`, `queue_add`, `update_tags`, `delete_tracks`
  (`from_library` | `to_trash`), `set_eq_bands`, `set_replaygain_mode`,
  `get_settings` / `set_setting`, `import_rhythmbox`.
- **Backend → Frontend:** Tauri-Events, z. B. `player:position` (Tick),
  `player:track-changed`, `player:state`, `library:scan-progress`,
  `library:changed`.

Das Frontend hält nie die ganze Bibliothek im Speicher: Die Tabelle fordert
über `get_track_window` nur den sichtbaren Ausschnitt an; Sortieren, Filtern
und Suche laufen als SQL im Backend. Damit skaliert die Liste auf 100k+ Titel.

## Datenmodell (SQLite)

```sql
tracks(
  id, path UNIQUE, title, artist, album, album_artist,
  year, track_no, genre, duration_ms,
  rating,            -- 0–5, 0 = unbewertet
  play_count, last_played_at, added_at,
  file_mtime,        -- für inkrementellen Rescan
  missing            -- Datei verschwunden: markieren, nicht löschen
)
playlists(id, name, position)
playlist_tracks(playlist_id, track_id, position)
smart_playlists(id, name, rules_json, sort_field, sort_dir, limit_count)
settings(key PRIMARY KEY, value)   -- Layout, Farbschema, Spalten, Ordner,
                                   -- EQ, ReplayGain, Modul-Schalter
```

- Smart-Playlist-Regeln: JSON aus Bedingungen (Feld / Operator / Wert),
  im Backend zu parametrisiertem SQL übersetzt. Die drei vordefinierten
  Playlists sind normale Einträge dieses Systems — kein Sonderfall.
- Play Count/`last_played_at` werden erhöht, wenn ein Titel überwiegend
  abgespielt wurde (Schwelle: >50 % gehört).
- Inkrementeller Rescan: nur Dateien mit geänderter `file_mtime` neu lesen.
- Ordner-Überwachung: notify-Events (angelegt/geändert/gelöscht/umbenannt)
  werden entprellt (Debounce) und in dieselbe Rescan-Logik gespeist wie der
  manuelle Scan. Schreibt der eigene Tag-Editor eine Datei, wird das
  resultierende Watcher-Event ignoriert (Pfad kurzzeitig auf Ignorierliste),
  damit kein doppelter Rescan entsteht.

## UI — visuelle Vorlage 2a, konfigurierbar

Das PDF enthält vier Layout-Varianten; **2a ist der Standard**, die anderen
sind als Einstellungen wählbar. Drei Zonen im Standard-Layout:

1. **Sidebar links** — Abschnitte BIBLIOTHEK (Musik, Warteschlange),
   PLAYLISTEN, INTELLIGENT; jeweils mit Track-Zähler. „Neue Playlist"-Aktion.
2. **Hauptbereich** — Spaltenansicht: Titel / Interpret / Album / Jahr /
   Länge / Bewertung. Klick auf Spaltenkopf sortiert. Suchfeld
   „Alle Felder durchsuchen" oben. Einblendbare **Browse-Leiste**
   (Rhythmbox' „Browse"): drei Filterspalten Genre / Interpret / Album
   über der Titelliste, Auswahl filtert kaskadierend und kombiniert sich
   mit der Suche. Fußzeile mit Gesamtstatistik
   („1.704 Titel, 4 Tage, 6 Std. 28 Min., 43,4 GB").
   Laufender Titel ist farblich hervorgehoben (Akzentzeile).
3. **Playerleiste unten** (Spotify-Stil) — Cover + Titel/Interpret links,
   Transport (Zurück/Play/Weiter) + Seekbar mit Zeitanzeige mittig,
   Shuffle/Repeat + Lautstärke rechts.

**Kontextmenü** (Rechtsklick auf Zeile, mit Mehrfachauswahl): Abspielen,
zur Warteschlange, zu Playlist hinzufügen, Tags bearbeiten, aus Bibliothek
entfernen, Datei in den Papierkorb. **Tag-Editor** als modaler Dialog;
bei Mehrfachauswahl werden gemeinsame Felder (z. B. Album, Interpret)
gesammelt bearbeitet. Löschen immer mit Bestätigungsdialog, der klar
unterscheidet: „nur aus Bibliothek" vs. „Datei in den Papierkorb".

**Blur-Look:** ambienter Farb-Glow, extrahiert aus dem Cover des laufenden
Titels, hinter halbtransparenten Flächen mit `backdrop-filter`. Reines CSS —
läuft auf Wayland ohne Compositor-Abhängigkeit.

### Einstellungen

Ein Einstellungs-Dialog (Zahnrad in der Headerbar) mit:

- **Playerleiste:** unten (2a, Standard) / oben in der Headerbar (1a) /
  schwebende Insel (1c). Umgesetzt als eine `PlayerBar`-Komponente, die an
  drei Layout-Slots gerendert wird — keine drei Implementierungen.
- **Farbschema:** dunkel (Standard) / hell (1b) / System folgen. Beide
  Schemata sind vollständige Token-Sets in `tokens.css`; der Blur-Look
  funktioniert in beiden.
- **Spalten:** einzelne Spalten ein-/ausblenden (Rechtsklick auf den
  Spaltenkopf), Spaltenbreiten per Drag, beides wird gespeichert.
- **Bibliotheksordner** verwalten (hinzufügen/entfernen).
- **Equalizer:** 10 Bänder als Slider, Presets (Flat, Rock, Pop, …),
  Ein/Aus-Schalter; Werte werden gespeichert.
- **ReplayGain:** Modus Titel / Album / Aus; Fallback-Verstärkung für
  Dateien ohne ReplayGain-Tags.
- **Module:** Liste aller Module mit An/Aus-Schaltern und Kurzbeschreibung
  (wie Rhythmbox' Plugins-Tab).

Alle Einstellungen persistiert in einer `settings`-Tabelle (Key-Value) in
der SQLite-DB; beim Start geladen, Änderungen wirken sofort ohne Neustart.

Design-Tokens (Farben, Typo, Abstände, Radien, Blur-Stärken) zentral in
`tokens.css`. Bewertung als klickbare 5-Sterne-Komponente direkt in der
Tabellenzeile. UI-Sprache Deutsch; alle Strings zentral in einem
Strings-Modul abgelegt (i18n-fähig, ohne Framework-Overhead jetzt).

## Fehlerbehandlung

- Scanner überspringt defekte/unlesbare Dateien und protokolliert sie;
  Scan-Ergebnis meldet Anzahl übersprungener Dateien.
- Verschwundene Dateien werden als `missing` markiert, nicht gelöscht
  (Bewertungen/Statistiken bleiben erhalten; analog Rhythmbox „Missing Files").
- Typisierte Fehler (`thiserror`) über die IPC-Grenze als strukturierte
  Fehlerobjekte; im UI nutzerfreundlich als Toast, Details im Log.
- Abspielfehler (defekte Datei): Titel überspringen, Toast, weiter mit
  dem nächsten Titel der Warteschlange.
- Tag-Schreiben: schlägt das Schreiben fehl (Datei gesperrt, schreibgeschützt,
  Format nicht unterstützt), bleibt die DB unverändert und der Dialog zeigt
  den Fehler pro Datei an. Wird gerade der laufende Titel bearbeitet, wird
  die Wiedergabe nicht unterbrochen (Schreiben erfolgt nach Dateiende oder
  über eine temporäre Kopie).
- Löschen: Papierkorb-Fehler (z. B. Netzlaufwerk ohne Trash) werden gemeldet;
  es wird nie endgültig gelöscht, ohne dass der Nutzer das explizit wählt.

## Testing

- **Rust-Unit-Tests:** Scanner (Tag-Parsing, inkrementeller Rescan,
  defekte Dateien), Watcher-Event-Verarbeitung (Debounce, Ignorierliste),
  Tag-Schreiben (Roundtrip lesen→schreiben→lesen), Smart-Playlist-Übersetzung
  Regeln→SQL, Queue-Logik (Shuffle/Repeat/Next), Play-Count-Schwelle,
  Lösch-Pfade (nur DB vs. Papierkorb), Rhythmbox-Import
  (XML-Parsing, Pfad-Abgleich, Rating-Übernahme), Browse-Facetten-Queries
  (kaskadierende Filter + Suche kombiniert).
- **Vitest + React Testing Library:** Sternebewertung, Suche/Filter,
  Tabellen-Sortierung, Player-Bar-Zustände, Tag-Editor-Dialog
  (Einzel-/Mehrfachauswahl), Lösch-Bestätigung.
- E2E (WebDriver/tauri-driver) erst nach dem MVP.

## Spätere Ausbaustufen

- **Interpreten-/Album-Infos:** Detailansicht zu einem Interpreten mit
  dessen Alben (Discografie) und Zusatzinfos, Daten z. B. via
  MusicBrainz/Last.fm-API. Architektonisch vorbereitet durch ein
  `MetadataProvider`-Trait im Backend und ein aufklappbares Detail-Panel
  im Frontend — im MVP existiert nur die lokale Implementierung
  (Alben des Interpreten aus der eigenen Bibliothek).
- **Lyrics:** Anzeige von Songtexten im Detail-Panel; Quellen: eingebettete
  Tags (USLT/Vorbis `LYRICS`, von lofty lesbar), `.lrc`-Dateien neben der
  Musikdatei, später Online-Quellen über dasselbe Provider-Trait.
- **Android-Synchronisation** (wie in Rhythmbox): Gerät via MTP/gvfs oder
  als Massenspeicher erkennen, ausgewählte Playlists/Titel aufs Gerät
  kopieren, Playlist-Export als M3U, Abgleich (nur Neues übertragen,
  Verwaistes optional entfernen). Erkennt angeschlossene Geräte über
  udev/gvfs; erscheint als eigener Sidebar-Eintrag „Geräte".
- **Scrobbling (Last.fm/Libre.fm)** — erstes Modul nach dem MVP
  (priorisiert), damit die Hörhistorie beim Umstieg schnell weiterläuft
- Podcasts, Internetradio — jeweils als eigenes Modul über die
  bestehenden Erweiterungspunkte
- Crossfade (GStreamer), Online-Cover-Suche (über `MetadataProvider`)
- Fremd-Plugin-API: zur Laufzeit ladbare Plugins von Dritten (z. B. WASM)
  auf Basis derselben Erweiterungspunkte
- Regel-Editor für Smart Playlists
- Bewertungen optional in Dateitags exportieren
- macOS-Port bleibt durch den Tauri-Stack theoretisch möglich
  (GStreamer und MPRIS wären zu ersetzen), ist aber kein Ziel
