# Reprise — Design-Dokument

**Datum:** 2026-07-11
**Status:** Vom Nutzer freigegeben (2026-07-11)

## Zusammenfassung

Ein Musikplayer für Linux (GNOME/Wayland) als **native GTK4/libadwaita-App**.
**Frontend-Pivot 2026-07-11:** Web-Technologie (Tauri + React) verworfen —
GNOME-Puristen bevorzugen natives GTK; damit entfallen auch Glasoptik und
die PDF-Layout-Varianten (`Musikplayer.pdf` ist als Designreferenz obsolet).
Die Bibliotheksansicht orientiert sich an Rhythmbox (sortierbare
Spaltenliste via `GtkColumnView` für große Bibliotheken), das visuelle
Design folgt der **GNOME HIG** mit Adwaita-Widgets — Richtung gemäß dem
GTK4-Mockup des Nutzers (`docs/design/2026-07-11-designmock-gtk4.pdf`,
ausdrücklich „grobe Richtung, keine genaue Vorgabe"): dunkles, flaches
Layout, Navigations-Seitenleiste, Playerleiste standardmäßig unten
(Position per Einstellung: oben / unten — Nutzer-Entscheidung 2026-07-11,
ersetzt die zwischenzeitliche HIG-Verschlankung; „schwebend" 2026-07-12
verworfen zugunsten der Now-Playing-Vollansicht), Farbschema
Dunkel / Hell / System via `AdwStyleManager`.

**Plattform-Entscheidung:** Linux-only, endgültig (Nutzer: „werde es eh nie
portieren"). GStreamer als Audio-Engine — dieselbe Engine wie Rhythmbox.

## Positionierung: Rhythmbox-Nachfolger

Rhythmbox ist faktisch im Wartungsmodus (GTK3; der GTK4-Port ist trotz
mehrerer Anläufe gescheitert). Reprise positioniert sich als moderner
Nachfolger:

- **Open-Core-Lizenz (2026-07-13):** Engine (Core + platform-linux) MIT, Linux-GUI (reprise-gnome) GPL-3.0-or-later, künftige Mac/Windows/Mobile-Frontends proprietär (separate Repos) — passt zur Rhythmbox-Herkunft und zur
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
- **Grundsatz „rührt deine Dateien nicht an":** Reprise verschiebt,
  benennt oder verändert Musikdateien niemals ungefragt. Schreibzugriffe
  passieren ausschließlich auf explizite Nutzeraktion (Tag-Editor,
  Papierkorb-Löschen) — nie automatisch.
- **Performance als Versprechen:** flüssig bei 100k+ Titeln, schneller
  Start, Suche tippt sich verzögerungsfrei — die Hauptkritik an
  Rhythmbox (träge bei großen Bibliotheken) ist der Maßstab.

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
- Layout-Einstellungen (zurückgeholt per Nutzer-Entscheidung 2026-07-11,
  Mockup 7b): Position der Playerleiste (oben / unten, über visuelle
  Vorschau-Karten), Seitenleiste und Statusleiste ein-/ausblendbar,
  Listendichte (Komfortabel / Standard / Kompakt), sichtbare Spalten und
  Spaltenbreiten (Spalten-Popover am Listenkopf); Farbschema Dunkel /
  Hell / System (`AdwStyleManager`). Nur die Glasoptik bleibt gestrichen.
- Sortiereinstellungen: Sortierung (Spalte + Richtung) wird pro Ansicht
  gespeichert; sinnvolle Sekundär-Sortierung (Album → Track-Nr.,
  Interpret → Album → Track-Nr.)
- Spalten-Popover am Listenkopf (Design-Screens des Nutzers): Checkbox-
  Liste „Spalten anzeigen" mit Drag-Griffen zum Umsortieren; „Titel" ist
  fixiert. Verfügbare Spalten: Titel, Interpret, Album, Jahr, Länge,
  Bewertung (Standard an) sowie Genre, Wiedergaben, Bitrate (Standard aus)
- 10-Band-Equalizer mit Presets (GStreamer `equalizer-10bands`),
  ein-/ausschaltbar, Einstellungen werden gespeichert
- ReplayGain-Lautstärkeangleichung (GStreamer `rgvolume`): liest
  ReplayGain-Tags aus den Dateien, Modus Titel / Album / Aus
- Internes Modulsystem mit An/Aus-Liste in den Einstellungen (wie
  Rhythmbox' Plugins-Tab); Equalizer und ReplayGain sind die ersten Module
- Rhythmbox-Import: `rhythmdb.xml` (Bewertungen, Play Counts, zuletzt
  gespielt, hinzugefügt am) und `playlists.xml`. Dreistufiger Abgleich:
  1. exakter Pfad-Treffer (URI-dekodiert) gegen die gescannte Bibliothek;
  2. Dateien außerhalb der Bibliotheksordner werden einzeln aufgenommen
     (Rhythmbox erlaubt Import von überall), häufige Fremd-Ordner werden
     als neue Bibliotheksordner vorgeschlagen;
  3. tote Pfade werden per Fuzzy-Abgleich (Titel + Interpret + Album +
     Dauer ±2 s) gerettet, falls die Datei woanders wieder auftaucht.
  Import-Bericht: übernommen / einzeln importiert / gerettet / nicht
  auffindbar. Radio-Streams (`iradio`-Einträge) werden aufgehoben und dem
  späteren Radio-Modul bereitgestellt.
  - *Optionaler Bonus (Nutzer-Idee 2026-07-11, niedrige Priorität):*
    Neben den Bibliotheksdaten liest der Import auch Rhythmbox' UI-
    Einstellungen aus GSettings (`org.gnome.rhythmbox.*`, u. a.
    `visible-columns` und die Sortierung) und überträgt sie per Best-
    Effort-Mapping auf unsere Spaltenkonfiguration — nur die überlappenden
    Spalten (Interpret, Album, Genre, Jahr, Länge, Wiedergaben, Bewertung,
    Bitrate), Rest wird verworfen. Damit fühlt sich die Liste sofort
    vertraut an. Ist dconf/das Schema nicht lesbar, passiert einfach nichts
    (kein Fehler). Bewusst zweitrangig gegenüber dem Daten-Import — das
    Spaltenlayout ist in Sekunden manuell gesetzt, die Bewertungen/Play
    Counts nicht.
- Browse-Leiste wie in Rhythmbox: einblendbare Filterspalten
  Genre / Interpret / Album über der Titelliste, kombinierbar mit der Suche
- Cover-Art-Pipeline: eingebettete Cover (lofty) und Bilddateien im
  Albumordner (`cover.*`, `folder.*`), Thumbnail-Cache auf Platte
- Drag & Drop: Titel in Playlists ziehen, Warteschlange und Playlists
  per Drag umsortieren
- Tastatur-Shortcuts: u. a. Leertaste Play/Pause, Strg+F Suche,
  Entf Löschen-Dialog, Pfeiltasten-Navigation in der Liste
- GNOME-Benachrichtigung bei Titelwechsel (Cover, Titel, Interpret)
- Datei-Assoziation + Single-Instance: „Öffnen mit Reprise" aus dem
  Dateimanager; ein zweiter Start reicht die Dateien an die laufende
  Instanz weiter
- Erster-Start-Assistent: Musikordner wählen; vorhandene Rhythmbox-Daten
  werden automatisch erkannt („Rhythmbox gefunden — Bibliothek, Bewertungen
  und Playlists jetzt übernehmen?") und per Ein-Klick importiert
- Sitzung wiederherstellen: Warteschlange, aktueller Titel und Position
  überleben einen Neustart
- M3U-Playlist-Import und -Export (Interop mit anderen Playern)
- Hintergrund-Wiedergabe: optional spielt Musik beim Schließen des
  Fensters weiter (Einstellung; Steuerung dann über MPRIS/Medientasten)

### Bewusst NICHT im MVP

- Scrobbling (ListenBrainz, Last.fm, Libre.fm) — aber **priorisiert als
  erstes Modul direkt nach dem MVP** (der Nutzer scrobbelt aktiv; die Lücke
  soll kurz bleiben). **ListenBrainz ist der bevorzugte, empfohlene Dienst**
  (offen, privatsphäre-freundlich, MusicBrainz-nah — passt zur GPL-/Keine-
  Telemetrie-Linie); Last.fm/Libre.fm als etablierte Alternativen daneben.
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
| UI | GTK4 + libadwaita (gtk4-rs) | Native GNOME-App, HIG-konform; `GtkColumnView` ist genau das Widget für 100k+-Spaltenlisten (System: GTK 4.22, adw 1.9) |
| Backend | Rust | Scanner, Datenbank, Audio, MPRIS — ein Prozess, eine Sprache |
| Audio | GStreamer (gstreamer-rs, playbin3) | Auf Linux triviale Systemabhängigkeit; jedes Format, Gapless und ReplayGain eingebaut — dieselbe Engine wie Rhythmbox |
| Tags | lofty | Einheitliches Tag-Lesen und -Schreiben über alle Formate |
| Watcher | notify | Ordner-Überwachung (inotify) |
| Löschen | trash | Dateien in den Papierkorb statt endgültig löschen |
| Datenbank | SQLite (rusqlite) | Bewertungen und Statistiken liegen in der App-Datenbank (Entscheidung: keine Tag-Schreibzugriffe) |
| MPRIS | zbus | D-Bus-Anbindung für GNOME-Mediensteuerung und Medientasten |
| i18n | gettext (Quellsprache Englisch) | GNOME-Standard; Community-Übersetzungen als `.po`-Dateien |

Verworfene Alternativen: **Tauri 2 + React** (ursprüngliche Wahl — einziger
Grund war die Designfreiheit für die PDF-Glasoptik; mit dem Verzicht auf
Glasoptik und Portierbarkeit blieb kein Vorteil gegenüber nativem GTK.
Pivot 2026-07-11, rechtzeitig vor Frontend-Beginn — der Rust-Backend-Code
aus der Tauri-Phase wurde übernommen); **Electron** (RAM-Overhead, Node
statt Rust); **Rodio/Symphonia** (GStreamer bringt Gapless, ReplayGain,
Equalizer und alle Formate frei Haus).

## Architektur

```text
reprise/                        eine Rust-Crate (kein npm, kein Webview)
├── src/
│   ├── main.rs                 AdwApplication, Fenster, App-ID org.reprise.Reprise
│   ├── db.rs / models.rs       SQLite + Migrationen (aus der Tauri-Phase übernommen)
│   ├── queries.rs              Track-Windowing (Sortier-Whitelist, Filter, Stats)
│   ├── library/                Scanner (walkdir + lofty), Watcher (notify),
│   │                           Tag-Schreiben, Löschen (trash), Rhythmbox-Import,
│   │                           Cover-Extraktion + Thumbnail-Cache
│   ├── player.rs               GStreamer playbin3: Play/Pause/Seek, Gapless,
│   │                           audio-filter-Kette rgvolume → equalizer-10bands;
│   │                           Events als glib-Signale/Callbacks
│   ├── queue/                  Warteschlange, Shuffle, Repeat
│   ├── playlists/              Manuell + intelligent (Regeln → SQL)
│   ├── mpris/                  zbus: GNOME-Mediensteuerung, Medientasten
│   └── ui/                     GTK-Widgets: Hauptfenster, Sidebar
│                               (AdwNavigationSplitView), Track-Liste
│                               (GtkColumnView + eigenes GListModel),
│                               Playerleiste (GtkActionBar), Browse-Leiste,
│                               Dialoge (AdwDialog/AdwAlertDialog),
│                               Einstellungen (AdwPreferencesDialog)
└── data/                       .desktop, Icons, GSettings-Schema, po/ (gettext)
```

### Modulsystem (Rhythmbox-Plugin-Gedanke, intern)

Die App ist von Anfang an intern modular — Vorbild ist Rhythmbox' Plugin-
Liste, aber ohne Fremd-Plugin-API:

- Jedes optionale Feature oder jede externe Integration (später: Scrobbling,
  Podcasts, Lyrics, Interpreten-Infos) ist ein abgeschlossenes **Modul** mit
  eigenem Backend-Teil (Rust-Trait `Module`: init/shutdown, eigene
  Commands/Events) und optionalem Frontend-Teil.
- **Erweiterungspunkte** sind definiert und werden vom Kern bereitgestellt:
  Sidebar-Einträge, Kontextmenü-Aktionen, Einstellungs-Seiten,
  Detail-Panel-Tabs, Audio-Pipeline-Elemente.
- In den Einstellungen gibt es eine **Modul-Liste mit An/Aus-Schaltern**
  (UI-Name: „Plugins", wie Rhythmbox); Zustand in der `settings`-Tabelle.
  MPRIS und der Online-Coverabruf sind die ersten Module. Equalizer und ReplayGain
  sind feste Kernfunktionen unter „Wiedergabe"; MTP-/iPod-Geräte-Support ist eine
  feste Funktion unter „Synchronisation" und kein Plugin.
- Eine echte **Fremd-Plugin-API** (zur Laufzeit ladbar, z. B. WASM) bleibt
  spätere Ausbaustufe und setzt auf denselben Erweiterungspunkten auf.

### Datenfluss (ein Prozess statt IPC)

- **UI → Backend:** direkte Rust-Aufrufe (gleicher Prozess). Lange
  Operationen (Scan, Import) laufen in Worker-Threads; Ergebnisse und
  Fortschritt kommen über `glib::MainContext`-Channels zurück auf den
  GTK-Main-Thread.
- **Player → UI:** GStreamer-Bus-Watch läuft im GLib-MainLoop der App;
  Zustands-/Positions-/EOS-Ereignisse erreichen die Widgets als
  Callbacks/Signale. Der GTK-Main-Thread blockiert nie auf Audio.

Die UI hält nie die ganze Bibliothek im Speicher: Ein eigenes `GListModel`
hinter dem `GtkColumnView` fordert über die Query-Schicht (`queries.rs`)
nur Fenster an; Sortieren, Filtern und Suche laufen als SQL (Sortierfelder
per Whitelist, alles parametrisiert). Damit skaliert die Liste auf 100k+
Titel.

## Datenmodell (SQLite)

```sql
tracks(
  id, path UNIQUE, title, artist, album, album_artist,
  year, track_no, genre, duration_ms, bitrate_kbps,
  rating,            -- 0–5, 0 = unbewertet
  play_count, last_played_at, added_at,
  file_mtime,        -- für inkrementellen Rescan
  file_size, device, inode,  -- Move-Detection (Schema v2)
  missing            -- Datei verschwunden: markieren, nicht löschen
)
playlists(id, name, position)
playlist_tracks(playlist_id, track_id, position)
smart_playlists(id, name, rules_json, sort_field, sort_dir, limit_count)
import_errors(id, path, reason, occurred_at)  -- Sidebar-Quelle „Importfehler"
settings(key PRIMARY KEY, value)   -- Layout, Farbschema, Spalten, Ordner,
                                   -- EQ, ReplayGain, Modul-Schalter
```

- Smart-Playlist-Regeln: JSON aus Bedingungen (Feld / Operator / Wert),
  im Backend zu parametrisiertem SQL übersetzt. Die drei vordefinierten
  Playlists sind normale Einträge dieses Systems — kein Sonderfall.
- Play Count/`last_played_at` werden erhöht, wenn ein Titel überwiegend
  abgespielt wurde (Schwelle: >50 % gehört).
- Inkrementeller Rescan: nur Dateien mit geänderter `file_mtime` neu lesen.
- **Move-Detection (Nutzer-Anforderung 2026-07-11):** Verschobene oder
  umbenannte Dateien/Alben werden beim Rescan **wiedererkannt statt als neu
  behandelt** — Bewertungen, Play Counts, `added_at` und `last_played_at`
  bleiben erhalten, nur der Pfad (und ggf. Tags/mtime) wird aktualisiert.
  Abgleich zweistufig, Kandidaten sind ausschließlich Zeilen, deren alter
  Pfad nicht mehr existiert (oder die `missing` sind):
  1. **(device, inode)-Treffer** — exakt für `mv`/Umbenennen innerhalb
     eines Dateisystems (Inode bleibt beim Verschieben erhalten);
  2. **Fingerprint-Treffer** — Titel + Interpret + Album + Dauer (±2 s) +
     Dateigröße, für Verschiebungen über Dateisystemgrenzen (kopieren +
     löschen). Nur bei **genau einem** Kandidaten; bei Mehrdeutigkeit
     (identische Duplikate) wird konservativ neu angelegt und die
     Ambiguität geloggt — niemals raten.
  Der `ScanReport` weist verschobene Titel separat aus (`moved`). Die
  Echtzeit-Erkennung über den Watcher (notify) nutzt später dieselbe
  Logik.
- Ordner-Überwachung: notify-Events (angelegt/geändert/gelöscht/umbenannt)
  werden entprellt (Debounce) und in dieselbe Rescan-Logik gespeist wie der
  manuelle Scan. Schreibt der eigene Tag-Editor eine Datei, wird das
  resultierende Watcher-Event ignoriert (Pfad kurzzeitig auf Ignorierliste),
  damit kein doppelter Rescan entsteht.

## UI — GNOME HIG, Adwaita

Natives libadwaita-Design (der frühere 2a-/Glasoptik-Ansatz ist mit dem
GTK4-Pivot entfallen). **Designreferenz:** GTK4-Mockup des Nutzers
(`docs/design/2026-07-11-designmock-gtk4.pdf`, „grobe Richtung"):

- *Hauptfenster (7a):* flache Headerbar (Ansichtstitel mittig, Suche und
  Menü rechts); Navigations-Seitenleiste mit Zählern/Badges; Spaltenliste
  mit Sterne-Bewertungen; laufender Titel als Akzentzeile mit
  Wiedergabe-Glyphe; schlanke Statuszeile unten rechts über der
  Playerleiste („1.704 Titel · 4 Tage, 6 Std. 28 Min. · 43,4 GB" —
  Mittelpunkt-Trenner); Playerleiste: links Titel (fett) + „Interpret —
  Album · Jahr", mittig Shuffle/Zurück/**runder Akzent-Play-Button**/
  Weiter/Repeat mit Seekbar in voller Breite darunter, rechts
  Warteschlangen-Knopf + Lautstärke.
- *Sidebar-Eintrag „Neuigkeiten"* mit Zähler-Badge (im Mockup): das ist
  der UI-Name des Radar-Moduls in der Seitenleiste.
- *Einstellungen (7b/7c):* AdwPreferences-Stil — View-Switcher oben,
  Boxed Lists mit Schaltern, Karten-Auswahl für die Playerleisten-Position.

Drei Zonen im Standard-Layout:

1. **Sidebar links** — Abschnitte BIBLIOTHEK (Musik, Warteschlange),
   PLAYLISTEN, INTELLIGENT; jeweils mit Track-Zähler. „Neue Playlist"-Aktion.
   Dazu zwei **Problem-Quellen wie in Rhythmbox**, die nur erscheinen,
   wenn sie Einträge haben (mit Zähler-Badge):
   - *Importfehler:* Dateien, die Scanner oder Rhythmbox-Import nicht
     verarbeiten konnten — als Liste mit Pfad und verständlichem Grund
     (defekt, Format nicht unterstützt, keine Leserechte …). Aktionen:
     erneut versuchen, im Dateimanager zeigen, Eintrag verwerfen.
   - *Fehlende Dateien:* als `missing` markierte Titel (Datei verschwunden,
     Statistiken bleiben erhalten). Aktionen: erneut suchen, aus
     Bibliothek entfernen.
2. **Hauptbereich** — Spaltenansicht: Titel / Interpret / Album / Jahr /
   Länge / Bewertung. Klick auf Spaltenkopf sortiert. Suchfeld
   „Alle Felder durchsuchen" oben. Einblendbare **Browse-Leiste**
   (Rhythmbox' „Browse"): drei Filterspalten Genre / Interpret / Album
   über der Titelliste, Auswahl filtert kaskadierend und kombiniert sich
   mit der Suche. **Statusleiste** unten mit den Zahlen der *aktuellen
   Ansicht* — Bibliothek, Playlist oder Suchergebnis: Titelanzahl,
   Gesamtdauer, Speichergröße („1.704 Titel, 4 Tage, 6 Std. 28 Min.,
   43,4 GB"; bei aktivem Filter: „42 von 1.704 Titeln").
   Laufender Titel ist farblich hervorgehoben (Akzentzeile).
3. **Playerleiste unten** (Spotify-Stil) — Cover + Titel/Interpret links,
   Transport (Zurück/Play/Weiter) + Seekbar mit Zeitanzeige mittig,
   Shuffle/Repeat + Lautstärke rechts.

**Kontextmenü** (Rechtsklick auf Zeile, mit Mehrfachauswahl): Abspielen,
zur Warteschlange, zu Playlist hinzufügen, Tags bearbeiten, aus Bibliothek
entfernen, Datei in den Papierkorb. **Tag-Editor** als modaler Dialog. **Mehrfachbearbeitung mehrerer
selektierter Titel ist Pflicht** (Nutzer-Anforderung 2026-07-12): Bei
Mehrfachauswahl zeigt der Dialog gemeinsame Werte normal an; Felder, die
sich zwischen den Titeln unterscheiden, erscheinen mit einem
Platzhalter „(mehrere Werte)" und bleiben unangetastet. **Nur Felder,
die der Nutzer aktiv ändert, werden geschrieben** — ein unverändert
gelassenes „(mehrere Werte)"-Feld darf niemals alle Titel mit einem Wert
überschreiben (sonst gingen pro-Titel-Werte wie individuelle Titelnamen
verloren). Das schützt bestehende Metadaten und passt zum Grundsatz
„rührt deine Dateien nicht ungefragt an". Geschrieben wird pro Datei
einzeln (lofty), Fehler pro Datei gemeldet; die DB spiegelt danach die
neuen Tags. Löschen immer mit Bestätigungsdialog, der klar
unterscheidet: „nur aus Bibliothek" vs. „Datei in den Papierkorb".

### Einstellungen

**`AdwPreferencesDialog`** mit Seiten (View-Switcher oben, Mockup 7b/7c) —
Wiedergabe · Darstellung · Layout · Bibliothek · Plugins
(· Synchronisation, erst mit dem Sync-Modul). Jede Änderung wirkt sofort
ohne Neustart.

- **Wiedergabe:** Equalizer (10 Bänder als Slider, Presets Flat/Rock/Pop/…,
  Ein/Aus), ReplayGain (Modus Titel / Album / Aus, Fallback-Verstärkung),
  Verhalten am Titelende.
- **Darstellung:** Farbschema Dunkel / Hell / System (`AdwStyleManager`).
- **Layout** (Mockup 7b): Playerleisten-Position über zwei visuelle
  Vorschau-Karten „Oben / Unten" (technisch `ToolbarView`-Top-/Bottom-Bar).
  **Standard bleibt „Unten", angedockt** — die GNOME-HIG-konforme, native
  Variante (wie Amberol/Decibels); die dichte Spaltenliste verträgt keine
  überlagernde Leiste, die Zeilen verdeckt. Die zwischenzeitlich erwogene
  Position „Schwebend" ist 2026-07-12 verworfen — sie bräuchte einen
  halbtransparenten Hintergrund über den Listenzeilen (nahe der abgelehnten
  Glasoptik); ihre Rolle als nativer Blickfang übernimmt die
  Now-Playing-Vollansicht (GUI-A), die den Platz füllt statt zu überlagern. Seitenleiste anzeigen (an/aus),
  Statusleiste anzeigen (an/aus); Listendichte Komfortabel / Standard /
  Kompakt; „Sichtbare Spalten" mit Bearbeiten-Aktion (öffnet dasselbe
  Spalten-Popover wie am Listenkopf).
- **Bibliothek:** Ordner verwalten (hinzufügen/entfernen), Rescan,
  Rhythmbox-Import.
- **Plugins** (so heißt das Modulsystem im UI): Liste optionaler Funktionen und
  externer Integrationen mit
  Name, Kurzbeschreibung, An/Aus-Schalter und — wo vorhanden —
  „Konfigurieren…"-Button für die moduleigene Einstellungsseite
  (z. B. Scrobbler mit Konto-Status „angemeldet als …"). Equalizer und ReplayGain
  erscheinen ausschließlich unter „Wiedergabe".
  Der Bereich „Plugin installieren…" (`~/.config/reprise/plugins`) aus
  dem Mockup ist für die spätere Fremd-Plugin-API reserviert und im MVP
  noch nicht sichtbar.
- **Synchronisation:** feste Produktseite für Geräte-Support. Sie erscheint, sobald
  MTP-/iPod- oder WLAN-Synchronisation implementiert ist, und wird nicht als Plugin
  ein- oder ausgeschaltet.

Das **Spalten-Popover am Listenkopf** (Mockup 1): Button rechts außen in
der Kopfzeile öffnet „Spalten anzeigen" — Checkboxen mit Drag-Griffen zum
Umsortieren, „Titel" fixiert; Standard-Spalten an, Genre/Wiedergaben/
Bitrate zuschaltbar.

Alle Einstellungen persistiert in einer `settings`-Tabelle (Key-Value) in
der SQLite-DB; beim Start geladen, Änderungen wirken sofort ohne Neustart.
Spaltenkonfiguration (Sichtbarkeit, Reihenfolge, Breiten) ebenso.

Optik kommt von Adwaita; eigenes CSS nur minimal und HIG-verträglich.
Bewertung als klickbares 5-Sterne-Widget direkt in der Tabellenzeile. **UI-Sprache Englisch** (Quellsprache — Community-Projekt; Entscheidung
2026-07-11, ersetzt die frühere Festlegung auf Deutsch). **Mehrsprachigkeit
ist fest eingeplant:** via gettext (GNOME-Standard), Übersetzungen als
`.po`-Dateien, zuerst Deutsch; die Sprache folgt dem System-Locale. Bis
zur gettext-Einführung liegen alle UI-Strings zentral in einem
Strings-Modul (englisch). Auch Commits, Code-Kommentare und
Log-/Fehlermeldungen sind englisch. Deutsche UI-Zitate in diesem Dokument
sind illustrativ — implementiert wird englisch.

## Sicherheit

Reprise verarbeitet nicht vertrauenswürdige Eingaben (fremde Audiodateien,
Tags, XML) und soll als Flatpak in die Welt — Sicherheit ist Teil des
Designs, nicht Nachrüstung:

- **Kein Webview:** native GTK-App ohne Browser-Engine — die gesamte
  Webview-Angriffsfläche (CSP, Remote-Content, JS-Injection) existiert
  nicht mehr (die Tauri-Härtung der ursprünglichen Architektur ist damit
  gegenstandslos).
- **Eingaben validieren:** alle UI→Backend-Aufrufe prüfen Parameter —
  Sortierfelder nur per Whitelist, SQL ausschließlich parametrisiert
  (nie String-Konkatenation), Limits gedeckelt.
- **Pfad-Disziplin:** Der Player öffnet nur Dateien, die aus der eigenen
  Datenbank stammen; gescannt wird nur, was der Nutzer explizit als
  Bibliotheksordner gewählt hat. Keine rohen Frontend-Pfade ins
  Dateisystem ohne Abgleich gegen die DB.
- **Unsichere Parser isoliert:** Tag-Parsing (lofty) und XML-Import
  (rhythmdb) sind memory-safe Rust; Parserfehler werden als Importfehler
  behandelt, nie als Absturz. Beim XML-Import keine externen Entities.
- **Flatpak-Sandbox:** minimale Berechtigungen (`xdg-music`, weitere
  Ordner nur via XDG-Portal), kein Netzwerkzugriff im MVP — erst Module
  wie Scrobbling/Radar fordern ihn an und machen das im Modul-UI
  transparent.
- **Keine Telemetrie.** Reprise sendet nichts nach Hause.
- **Dependency-Hygiene:** `cargo audit` und `npm audit` im Release-Prozess
  (Etappe 6), Abhängigkeits-Updates vor jedem Release.

## GNOME-Integration (Dock/Taskleiste)

Reprise soll sich wie eine native GNOME-App anfühlen:

- **Desktop-Eintrag + Icon:** `.desktop`-Datei und App-Icon (hicolor, inkl.
  symbolischem Icon); die Wayland-App-ID entspricht exakt dem
  Desktop-Dateinamen (`org.reprise.Reprise`), damit GNOME das Fenster im
  Dock korrekt gruppiert, das richtige Icon zeigt und „Zu Favoriten
  hinzufügen" funktioniert.
- **Mediensteuerung in der Shell:** über MPRIS erscheint Reprise im
  GNOME-Schnellmenü und auf dem Sperrbildschirm mit Cover, Titel und
  Transport-Tasten; Medientasten wirken global. GNOME hat bewusst keine
  Tray-Icons — MPRIS *ist* dort die Taskleisten-Integration für Player;
  die optionale Hintergrund-Wiedergabe stützt sich darauf.
- **Benachrichtigungen** bei Titelwechsel über XDG-Notification-Portal
  (bereits im MVP).
- **Portale statt Direktzugriff:** Datei-/Ordnerdialoge über
  XDG-Desktop-Portale — funktioniert sauber in der Flatpak-Sandbox.
- **Farbschema** folgt via libadwaita automatisch der GNOME-Einstellung
  (dark/light) — keine eigene Logik nötig.
- **Dateimanager:** MIME-Zuordnungen der Audioformate in der Desktop-Datei
  („Öffnen mit Reprise", bereits im MVP).

## Fehlerbehandlung

**Grundsatz Fehlertoleranz:** Zustände außerhalb der App — Dateisystem,
Tags, GStreamer, D-Bus — dürfen Reprise niemals zum Absturz bringen.
Die Bibliothek ist eine *Sicht* auf das Dateisystem, keine Garantie:
Dateien können jederzeit verschwinden, umbenannt oder unlesbar werden,
auch zwischen Klick und Ausführung. Jede externe Operation liefert ein
`Result` und wird behandelt und gemeldet; `unwrap()`/`expect()` sind
außerhalb von Tests und Startup tabu.

- Scanner überspringt defekte/unlesbare Dateien und protokolliert sie in
  einer `import_errors`-Tabelle (Pfad, Grund, Zeitpunkt); sichtbar als
  Sidebar-Quelle „Importfehler", Scan-Ergebnis meldet die Anzahl.
- Verschwundene Dateien werden als `missing` markiert, nicht gelöscht
  (Bewertungen/Statistiken bleiben erhalten; analog Rhythmbox „Missing Files").
- **Datei beim Abspielversuch weg** (gerade physisch gelöscht oder
  umbenannt): kein Absturz — Titel sofort als `missing` markieren,
  Toast („Datei nicht gefunden"), automatisch weiter mit dem nächsten
  Titel der Warteschlange; der Watcher bestätigt die Änderung asynchron.
- **Datei verschwindet während der Wiedergabe:** der bereits geöffnete
  Stream spielt unter Linux in der Regel bis zum Ende (offener
  File-Handle); meldet GStreamer stattdessen einen Bus-Fehler, greift
  dieselbe Skip-Logik. Die `missing`-Markierung kommt über den Watcher.
- `missing`-Titel werden im UI ausgegraut mit Hinweis-Symbol gezeigt,
  nicht versteckt; Abspielen/Tag-Bearbeiten sind deaktiviert,
  „erneut suchen" und „aus Bibliothek entfernen" bleiben verfügbar.
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

## Logging & Diagnose

Reprise soll sich aus der Konsole heraus vollständig beobachten lassen —
wer die App im Terminal startet, bekommt nützliche Diagnose-Ausgaben:

- **Framework:** `tracing` + `tracing-subscriber` (EnvFilter). Die Konsole
  (stderr) zeigt standardmäßig `INFO`; Feinsteuerung pro Modul über die
  Umgebungsvariable `REPRISE_LOG` (z. B.
  `REPRISE_LOG=reprise::library=debug,reprise::player=trace`), dazu das
  CLI-Flag `--verbose`/`-v` als Abkürzung für Debug-Level.
- **Start-Banner:** Version + Git-Commit, Pfade (DB, Einstellungen, Logs),
  GStreamer- und WebKitGTK-Version, Session-Typ (Wayland/X11) — die
  Infos, die in jedem Bug-Report als Erstes fehlen.
- **Was geloggt wird:** jeder IPC-Command mit Dauer (Debug-Level,
  Parameter gekürzt), Scanner-Fortschritt und Importfehler,
  Watcher-Events, Player-Zustandswechsel und GStreamer-Bus-Fehler,
  DB-Migrationen. Fehler immer mit Kontext (`WARN`/`ERROR`) — passend
  zum Grundsatz „Fehler nie verschlucken".
- **Logdatei:** zusätzlich zur Konsole nach `~/.local/state/reprise/logs/`
  mit täglicher Rotation und begrenzter Aufbewahrung — auch bei Start
  über das Dock ist nachträglich alles nachlesbar.
- **Panic-Hook:** Panics landen mit Backtrace in der Logdatei, nicht nur
  auf stderr.
- **UI-Diagnose:** GTK Inspector (`GTK_DEBUG=interactive`) für
  Widget-Debugging; `G_MESSAGES_DEBUG=all` schaltet GLib-/GTK-interne
  Meldungen zu. Da alles ein Prozess ist, landet ohnehin alles in
  einer Zeitachse im tracing-Log.
- **GStreamer-Diagnose:** `GST_DEBUG` wirkt unverändert durch
  (System-GStreamer), z. B. `GST_DEBUG=playbin:5` für Pipeline-Analyse.
- **Grenzen:** Logs bleiben lokal (keine Telemetrie, siehe „Sicherheit");
  keine sensiblen Inhalte über lokale Dateipfade hinaus.

## Testing

- **Rust-Unit-Tests:** Scanner (Tag-Parsing, inkrementeller Rescan,
  defekte Dateien), Watcher-Event-Verarbeitung (Debounce, Ignorierliste),
  Tag-Schreiben (Roundtrip lesen→schreiben→lesen), Smart-Playlist-Übersetzung
  Regeln→SQL, Queue-Logik (Shuffle/Repeat/Next), Play-Count-Schwelle,
  Lösch-Pfade (nur DB vs. Papierkorb), Rhythmbox-Import
  (XML-Parsing, Pfad-Abgleich, Rating-Übernahme), Browse-Facetten-Queries
  (kaskadierende Filter + Suche kombiniert).
- **UI-Schicht:** Logik lebt testbar im Backend (Query-Schicht,
  Format-Helfer, Player-Zustandsmaschine — alles Rust-Unit-Tests);
  GTK-Widget-Verhalten wird schlank gehalten und headless (Xvfb)
  verifiziert. E2E-Automatisierung erst nach dem MVP.

## Spätere Ausbaustufen

- **Interpreten-/Album-Infos:** Detailansicht zu einem Interpreten mit
  dessen Alben (Discografie) und Zusatzinfos, Daten z. B. via
  MusicBrainz/Last.fm-API. Architektonisch vorbereitet durch ein
  `MetadataProvider`-Trait im Backend und ein aufklappbares Detail-Panel
  im Frontend — im MVP existiert nur die lokale Implementierung
  (Alben des Interpreten aus der eigenen Bibliothek).

  *Design des Interpreten-Panels (Referenz-Mockup vom 2026-07-11; mit dem
  GTK4-Pivot als Adwaita-Interpretation — gleiche Inhalte und Aktionen,
  native Widgets/`AdwPreferencesGroup`-artige Karten statt Glas-Look):*
  Das Panel öffnet rechts neben der Titelliste (schließbar):
  - **Kopf:** Interpreten-Bild (Quelle über den `MetadataProvider`,
    z. B. Fanart.tv/Wikimedia via MusicBrainz-Relationen; lokal
    gecacht, Fallback: Platzhalter aus Initialen), Interpreten-Name,
    Status-Badges „Aktiv" (grüner Punkt) und „Auf Tour"; Meta-Zeile
    „142 Titel in deiner Bibliothek · Deathcore · seit 2018" —
    Bibliotheks-Zähler lokal, Genre und Gründungsjahr via MusicBrainz.
  - **Neue Veröffentlichung:** Karte mit Albumtitel und Erscheinungsdatum
    („Album · erscheint 26. Sep 2026"), Aktion „Vormerken" — speist die
    Vormerk-Liste des Radar-Moduls.
  - **Tour <Jahr>:** Terminliste (Datum, Stadt, Venue); Termine in der
    Nähe werden hervorgehoben („In deiner Nähe · 23 km", Ort aus den
    Radar-Einstellungen) und bieten einen „Tickets"-Button (externer
    Link zum Anbieter).
  - **Fußzeile:** Quellen und Cache-Alter („Quellen: MusicBrainz ·
    Bandsintown · aktualisiert vor 3 Std.").

  Das Panel kombiniert lokale Bibliotheksdaten mit Radar-Daten:
  Interpreten-Infos und Musik-Radar teilen sich Provider-Trait und
  lokalen Cache — das Panel ist neben dem Sidebar-Eintrag „Radar" die
  zweite Oberfläche derselben Daten.
- **Lyrics:** Anzeige von Songtexten im Detail-Panel; Quellen: eingebettete
  Tags (USLT/Vorbis `LYRICS`, von lofty lesbar), `.lrc`-Dateien neben der
  Musikdatei, später Online-Quellen über dasselbe Provider-Trait.
- **Begleit-App (Android) + Geräte-Synchronisation** (aktualisiert nach
  Mockup 7c und Nutzer-Idee 2026-07-11, „grobe Richtung"): Der Nutzer
  plant eine **eigene Android-Musik-App als Begleit-App**. Sie kann
  zweierlei:
  1. **Remote-Steuerung:** Reprise auf dem PC vom Handy aus steuern
     (Play/Pause/Next/Seek/Lautstärke, aktueller Titel + Cover,
     Warteschlange). Architektonisch günstig: Alle Transportbefehle
     laufen bereits durch eine zentrale Controller-/MPRIS-Schicht — ein
     Remote-Modul exponiert dieselben Kommandos über ein
     LAN-Protokoll (gepaart, lokal, kein Cloud-Dienst).
  2. **Synchronisation über zwei gleichwertige Wege** (Nutzer-Anforderung
     2026-07-11 — Kabel ist ausdrücklich kein bloßer Fallback):
     - **Kabel / USB (MTP/iPod, primär):** Gerät per USB anstecken → über
       MTP (via `libmtp`/gvfs, wie Rhythmbox) erkennen und synchronisieren;
       für Geräte im Massenspeicher-Modus alternativ direkter
       Dateisystem-Zugriff. Klassische iPods erhalten einen eigenen Adapter auf
       derselben Synchronisationsschicht. Funktioniert ohne Begleit-App, mit jedem
       Android-Gerät und vielen Musikplayern — der zuverlässige,
       netzwerkunabhängige Standardweg.
     - **Kabellos über WLAN (Begleit-App):** Kopplung per QR-Code,
       „nur im WLAN"; für Nutzer, die die Begleit-App installiert haben.
     Beide Wege teilen dieselben **Sync-Regeln**: ausgewählte Playlists,
     Bewertungen & Wiedergabezähler in beide Richtungen, optionales
     Transkodieren (z. B. FLAC → Opus 128 kbit/s, via GStreamer),
     „nur Neues übertragen / Verwaistes optional entfernen".
     iOS-Unterstützung der Begleit-App bleibt angedacht (Mockup 7c);
     iOS-Geräte sind über MTP allerdings nicht klassisch ansteuerbar —
     dort führt nur der WLAN-Weg.
  3. **Geteilter Rust-Core für die mobilen Apps** (Nutzer-Bestätigung
     2026-07-11 — „halte auch Android/iOS mit im Blick"): Die Begleit-Apps
     sind keine dünnen Remote-Clients, sondern konsumieren denselben
     `reprise-core` wie die Desktop-Frontends — über **dieselbe
     UniFFI-Brücke, die Kotlin (Android) *und* Swift (iOS) bedient**
     (ein FFI-Layer, zwei generierte Binding-Sätze; bewährter Pfad, vgl.
     Mozilla-Komponenten auf iOS + Android). Wiederverwendbar sind
     Datenmodell, SQLite-Queries, Queue-Engine, Smart-Playlist-Regeln,
     Move-Detection, M3U und Tag-Lesen (lofty) — allesamt plattform-
     unabhängiges Rust. **Entscheidend: Das Sync-Protokoll + die
     Merge-/Reconciliation-Logik leben im geteilten Core.** So sprechen
     Desktop-Server und Handy-Client ein aus *demselben* Code erzeugtes
     Wire-Format → **kein Protokoll-Drift** zwischen den Enden (der
     klassische Companion-App-Bug fällt strukturell weg). Nativ bleiben
     nur die plattformspezifischen Nähte: Audio (ExoPlayer/Media3 auf
     Android, AVFoundation auf iOS — via `PlaybackBackend`-Trait),
     Mediensteuerung (Android `MediaSession` / iOS `MPNowPlayingInfo` —
     via `NowPlaying`-Trait) sowie der Dateizugriff (Androids Scoped
     Storage / iOS-Sandbox brauchen einen plattformspezifischen
     Scanner-Pfad) und die UI (Jetpack Compose / SwiftUI). Layout:
     `frontends/android/` bzw. `frontends/ios/` als weitere Konsumenten
     von `crates/reprise-ffi` — dasselbe Monorepo-Schema wie Desktop.
  Sicherheit: WLAN-Kopplung explizit (QR + Bestätigung), Kommunikation nur
  im lokalen Netz, verschlüsselt; das Modul fordert die Netzwerk-
  Berechtigung im Flatpak erst an, wenn der WLAN-Weg aktiviert wird — der
  Kabel-Weg braucht kein Netzwerk, nur USB-/MTP-Zugriff (Flatpak-Portal).
  Angeschlossene und gekoppelte Geräte erscheinen als Liste im Tab
  „Synchronisation" mit „Jetzt synchronisieren"/
  „Entfernen"; eigener Sidebar-Eintrag „Geräte" (erkennt USB-Geräte über
  udev/gvfs automatisch).
  Die Android-App selbst ist ein eigenes Projekt (eigenes Repo/eigene
  Planung) — Reprise-seitig zählt hier das Protokoll + die Module.
- **Scrobbling (ListenBrainz bevorzugt; Last.fm, Libre.fm)** — erstes
  Modul nach dem MVP (priorisiert), damit die Hörhistorie beim Umstieg
  schnell weiterläuft. Ein gemeinsames `Scrobbler`-Trait mit je einem
  Backend pro Dienst (ListenBrainz: „listens submit" + „now playing" API,
  Token-Auth; Last.fm/Libre.fm: klassische AudioScrobbler-API,
  Session-Key); mehrere Dienste gleichzeitig aktivierbar. Offline-Queue:
  Scrobbles bei fehlender Verbindung lokal puffern und später nachreichen
  (Play-Schwelle >50 % wie bei Play Count). Konto-Status je Dienst im
  Modul-UI („angemeldet als …").
- **Musik-Radar** — zweites Modul nach dem MVP (priorisiert):
  1. *Fehlende Alben:* Abgleich der Bibliotheks-Interpreten gegen deren
     Discografie via MusicBrainz (Release-Groups); meldet Alben, die in
     der Bibliothek fehlen — als Nachkauf-Liste.
  2. *Kommende Veröffentlichungen:* angekündigte Alben mit Datum in der
     Zukunft, als Vormerk-Liste.
  3. *Konzerte & Touren:* Termine von Bibliotheks-Interpreten in der Nähe
     (Anbieter z. B. Bandsintown); der Ort wird manuell in den
     Einstellungen gesetzt — keine Standortabfrage.
  Eigener Sidebar-Eintrag „Radar" mit Benachrichtigungs-Badge; Ergebnisse
  werden lokal gecacht, API-Zugriffe rate-limitiert im Hintergrund.
  Interpreten einzeln stummschaltbar. Dieselben Daten erscheinen
  zusätzlich im Interpreten-Detail-Panel (siehe „Interpreten-/Album-
  Infos" oben: Neue Veröffentlichung mit „Vormerken", Tourtermine mit
  Nähe-Hinweis).
- Podcasts, Internetradio — jeweils als eigenes Modul über die
  bestehenden Erweiterungspunkte
- Crossfade (GStreamer), Online-Cover-Suche (über `MetadataProvider`)
- Discord Rich Presence (aktuellen Titel im Discord-Status zeigen) —
  kleines Modul, aus den Design-Screens des Nutzers übernommen
- Fremd-Plugin-API: zur Laufzeit ladbare Plugins von Dritten (z. B. WASM)
  auf Basis derselben Erweiterungspunkte
- Alben-Grid-Ansicht: Cover-Wand als zweite Ansicht neben der
  Spaltenliste („Musik als Kunstform", nicht nur Tabelle)
- **„Now Playing"-Vollansicht** (Nutzer-Idee 2026-07-11, statt schwebender
  Leiste als GNOME-nativer Blickfang; **eingeplant für GUI-A, 2026-07-12** —
  Basis-Ansicht ohne Lyrics/Farb-Glow): per Klick auf die Playerleiste öffnet
  sich eine großflächige Wiedergabe-Ansicht im Amberol-Stil — großes Cover,
  Titel/Interpret/Album prominent, Seekbar, Transport; nutzt den Platz statt
  die Liste zu verdecken. Später erweiterbar um Lyrics-Panel und ambienten
  Farb-Glow aus dem Cover (der frühere Blur-Wunsch, hier HIG-verträglich,
  weil er eine eigene Ansicht füllt statt über der Liste zu schweben).
- Import aus Clementine/Strawberry — erweitert die Umsteiger-Zielgruppe
  über Rhythmbox hinaus
- Mehrsprachige Oberfläche (i18n): gettext-Übersetzungen als `.po`-Dateien
  (GNOME-Standard, community-freundlich), zuerst Deutsch; Sprachwahl folgt
  dem System-Locale

### Aus der iTunes-Feature-Studie (freigegeben 2026-07-12)

Sieben Kandidaten, geprüft in `docs/research/itunes-feature-study.md` und
vom Nutzer für die Roadmap freigegeben. Reihenfolge = empfohlene
Sequenzierung; abgelehnte Features (Auto-Move/„Keep organized", Copy-to-
library, Apple-Genius mit Telemetrie, Visualizer, Mehrfach-Bibliotheken)
sind in der Studie mit Begründung dokumentiert und bleiben draußen.

1. **Skip Count** — Übersprungen-Zähler pro Titel (`skip_count`-Spalte),
   analog zur bestehenden Play-Count-Schwellenlogik; billig, ein „Skip"
   ist ein Signal (u. a. fürs Scrobbling und den späteren Auto-DJ). Kann
   früh mitlaufen, sobald der Tag-Editor/das Statistik-Modul dran ist.
2. **Smart-Playlist-Regeleditor (UI)** — das generische Regelsystem
   (Feld/Operator/Wert → SQL) existiert schon seit Etappe 3; hier fehlt
   nur der Dialog. Niedrige Grenzkosten → Studie empfiehlt, ihn früher
   zu ziehen (MVP-Ausläufer statt vage „später").
3. **Duplikat-Finder** — nutzt die Fingerprint-Logik der Move-Detection
   (Titel+Interpret+Album+Dauer ±2 s+Größe) fast unverändert; neu ist nur
   die Report-Ansicht + Papierkorb-Aktion (Lösch-Infrastruktur existiert).
4. **Compilation-/Album-Interpret-Korrektheit (VA-Alben)** — `album_artist`
   ist bereits durchgängig (Scanner/Schema/Browse-Queries); es fehlt nur
   die compilation-bewusste Gruppierung (ein „Various Artists"-Eimer in
   der Interpret-Filterspalte statt Streuung über alle Gastkünstler).
5. **Klassik-Felder** — `composer`/`work`/`movement`/`movement_name`
   (Schema + lofty-Lesen + Tag-Editor-Oberfläche) plus optionale Regel
   „innerhalb eines Werks nicht mischen".
6. **Lokaler Auto-DJ** — seed-basierter Smart-Shuffle (Genre/Tags/Bewertung),
   komplett in-process, keine Telemetrie/kein Vendor-Graph; nach dem
   Regeleditor (teilt dessen Matching-Engine als Kandidatenquelle).
7. **LAN-Bibliotheks-Sharing** — erweitert das geplante Companion-App-
   Protokoll, sodass eine zweite Reprise-Instanz die Bibliothek einer
   anderen im LAN durchstöbern/streamen kann (gepaart, kein Cloud);
   nach der Begleit-App selbst (Streaming-Protokoll, größerer Aufwand).

- Bewertungen optional in Dateitags exportieren
- Bewertungen optional in Dateitags exportieren
- (macOS-Port: endgültig gestrichen — Nutzer-Entscheidung 2026-07-11,
  „werde es eh nie portieren"; GTK4-Pivot besiegelt das)
