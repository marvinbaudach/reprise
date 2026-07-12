# GUI-A: Cover-Pipeline & Player-Leiste — Design

**Datum:** 2026-07-12
**Etappe:** GUI (Stage 5), Unter-Etappe **A** — die erste von vier
(A Cover+Leiste · B Tag-Editor+Löschen · C Browse-Leiste+Rhythmbox-Import ·
D First-Run+Session-Restore). Reihenfolge vom Nutzer bestätigt (2026-07-12):
A zuerst, weil die Cover-Pipeline fast jede andere Oberfläche speist.

**Baut auf:** dem abgeschlossenen Refactoring (Stage 4) — Drei-Crate-Workspace
`reprise-core` (dependency-pure) + `reprise-platform-linux` + `reprise-gnome`,
typisierte Settings-Façade, Modul-Registry. Master-Spec:
`2026-07-11-reprise-design.md` (Cover-Pipeline Z. 111–112, Player-Leisten-
Position Z. 353–354 — Vorschaukarten „Oben/Unten"; „schwebend" 2026-07-12
verworfen, ersetzt durch die Now-Playing-Vollansicht, Master-Spec Z. 659).

## Ziel

Album-Cover in Trackliste, Player-Leiste und der GNOME-Titelwechsel-
Benachrichtigung anzeigen, gespeist aus einer portablen, plattenschonenden
Thumbnail-Pipeline; die Player-Leiste zu einer polierten Leiste mit
umschaltbarer Position (oben / unten) ausbauen; und eine Now-Playing-
Vollansicht (Amberol-Stil) als GNOME-nativen Blickfang ergänzen.

## Scope

**Enthalten:**
- Cover-Auflösung: eingebettetes Bild (lofty) → sonst `cover.*`/`folder.*` im
  Albumordner → sonst keins.
- Thumbnail-Erzeugung (Decode + Resize) und ein On-Disk-Cache im
  XDG-Cache-Verzeichnis, geschlüsselt per Content-Hash der Quell-Bildbytes.
- Lazy, off-thread Cover-Laden und -Anzeigen in der Trackliste (48 px),
  der Player-Leiste (96 px) und der Benachrichtigung (96 px).
- Player-Leisten-Position als persistierte Einstellung (oben/unten, Default
  unten), sofort wirksam, über einen neuen Enum-Accessor der Settings-Façade.
- „Now Playing"-Vollansicht (Amberol-Stil): per Klick auf die Player-Leiste
  öffnet sich eine großflächige Wiedergabe-Ansicht mit großem Cover (1024 px),
  prominentem Titel/Interpret/Album, Seekbar und Transport — der GNOME-native
  Blickfang, der die schwebende Leiste ersetzt (Nutzer-Entscheidung 2026-07-12).

**Nicht enthalten (spätere Etappen/Module):**
- Online-Cover-Download — **eingeplant als direkt nächste Unter-Etappe GUI-A2**
  (Nutzer-Entscheidung 2026-07-12): ein *opt-in* gated Modul, das für Tracks
  ohne (oder mit nur winzigem) lokalem Cover ein hochauflösendes Album-Cover
  von **Cover Art Archive** (kein API-Key, MusicBrainz-basiert) lädt und im
  selben Cache ablegt. Standardmäßig **aus** (Netzwerk + Privatsphäre, vgl. die
  abgelehnte Genius-Telemetrie); korrekter User-Agent + 1 Anfrage/s. Nur
  Album-Cover; Interpreten-Bilder (fanart.tv/Wikidata) bleiben späteres Optional.
  **Architektur-Haken in GUI-A:** `resolve_source` ist die eine Stelle, an der
  GUI-A2 eine Netzwerk-Fallback-Stufe einhängt (nach eingebettet/Ordnerbild) —
  GUI-A verbaut sich dadurch nicht; der Rest der Pipeline (Hash-Cache,
  Thumbnailing, Loader) bleibt unverändert.
- Album-Grid-/Cover-Wand-Ansicht (Master-Spec Z. 657, spätere GUI-Etappe).
- Cover-Bearbeitung/-Einbettung (gehört zum Tag-Editor, GUI-B).
- Lyrics-Panel und ambienter Cover-Farb-Glow der Now-Playing-Ansicht (spätere
  Erweiterungen dieser Ansicht).
- Schwebende Player-Leiste (verworfen 2026-07-12; durch die Now-Playing-
  Vollansicht ersetzt).
- Alle weiteren GUI-Features (Tag-Editor, Löschen, Browse-Leiste, Import,
  First-Run, Session-Restore) — eigene Unter-Etappen.

## Kernversprechen (unverhandelbar)

Der Cover-Cache liegt ausschließlich unter `$XDG_CACHE_HOME/reprise/covers/`
(Fallback `~/.cache/reprise/covers/`). Reprise schreibt **niemals** ein
Thumbnail, eine Datei oder sonst etwas in die Musiksammlung des Nutzers. Cover
werden nur **gelesen** (eingebettet oder Ordnerbild). Der Cache ist jederzeit
gefahrlos löschbar und wird bei Bedarf neu erzeugt.

## Architektur

### 1. `reprise-core::covers` (pure, cross-platform)

Bleibt frei von gtk4/libadwaita/gstreamer/zbus (die Purität ist per
`cargo tree -p reprise-core` erzwungen). Thumbnailing gehört in den Core, weil
auch die geplanten Android/iOS-Frontends Thumbnails brauchen und denselben
Cache-Contract nutzen sollen.

**Neue Abhängigkeit:** das `image`-Crate (reines Rust, cross-platform) fürs
Dekodieren und Skalieren. Es verletzt die Dependency-Purität nicht (kein
gtk/gst/zbus). Bewusst hinzugefügt und dokumentiert; Feature-Set auf die real
vorkommenden Formate begrenzt (JPEG, PNG, WebP, GIF, BMP) statt der vollen
Default-Feature-Menge, um die Build-Fläche klein zu halten.

**Öffentliche Oberfläche (abgeleitet, minimal):**

```rust
/// Woher ein Cover für einen Track stammt.
pub enum CoverSource {
    /// In die Audiodatei eingebettetes Bild (via lofty).
    Embedded(Vec<u8>),
    /// Eine Bilddatei im Albumordner (cover.*, folder.*).
    FolderImage(PathBuf),
}

/// Löst die beste verfügbare Cover-Quelle für einen Track auf: zuerst das
/// eingebettete Bild, sonst das erste passende Ordnerbild, sonst None. Reine
/// Lese-Operation.
pub fn resolve_source(track_path: &Path) -> Option<CoverSource>;

/// Liefert den Cache-Pfad zu einem Thumbnail der gegebenen Kantenlänge und
/// erzeugt es bei Bedarf: Quellbytes hashen -> Cache treffen -> sonst
/// dekodieren, auf `size` skalieren (Seitenverhältnis erhalten, quadratisch
/// eingepasst), als PNG atomar (Temp-Datei + rename) schreiben. In-Flight-
/// Dedup identischer Keys, damit parallele Anfragen dieselbe Quelle nicht
/// mehrfach dekodieren.
pub fn thumbnail(source: &CoverSource, size: ThumbnailSize)
    -> Result<PathBuf, CoverError>;

/// Die drei gecachten Kantenlängen. Genau drei Konsumenten, genau drei Größen
/// (YAGNI): 48 px für Tracklisten-Zeilen, 96 px für Player-Leiste und
/// Benachrichtigung, 1024 px für die Now-Playing-Vollansicht (großzügig, damit
/// das Cover auch vergrößert / auf HiDPI nicht pixelig wird — es wird nur je
/// herunterskaliert, nie hochskaliert).
pub enum ThumbnailSize { List, Bar, Full } // 48 / 96 / 1024 px
```

**Cache-Layout:** `<cache>/reprise/covers/<hex-hash>-<size>.png`. Der Hash
ist ein schneller nicht-kryptografischer Content-Hash über die **Quell-
Bildbytes** (bei `FolderImage` über den Dateiinhalt, bei `Embedded` über die
eingebetteten Bytes), hex-kodiert. Konkret genügt `std`s `DefaultHasher`
(`std::hash`) — der Schlüssel muss nur auf einer Maschine deterministisch und
kollisionsarm genug für einen Cache sein; keine kryptografische Eigenschaft
nötig, daher keine neue Hash-Abhängigkeit. Folgen des Content-Keyings: robust gegen Verschieben/
Umbenennen von Alben (passt zur Move-Detection), automatische Deduplizierung
identischer Cover über Alben hinweg, keine Kollision gleichnamiger Alben
verschiedener Interpreten, und ein geändertes Cover erzeugt automatisch einen
neuen Cache-Eintrag.

**Fehlertoleranz:** Ein defektes/ungültiges Bild führt zu `CoverError`, nie zu
einem Panic; der Aufrufer behandelt das wie „kein Cover" (Platzhalter). Ein
nicht beschreibbarer Cache-Ordner wird einmal geloggt und der Pfad wird
weiterhin versucht — Cover-Fehlen darf die App nie funktionsunfähig machen.

### 2. `reprise-gnome::cover_loader` (GTK-Seite)

Ein Loader, der Cover **lazy** (nur für sichtbare Zeilen) und **off-thread**
(Decode/Resize blockieren nie den Main-Loop) beschafft:

- Anfrage `(track_path, size)` geht an einen Worker (bestehendes Muster:
  `async-channel` + `glib::MainContext::spawn_local` für das Ergebnis, wie
  bei der lazy SQL-Fensterung).
- Der Worker ruft `covers::thumbnail(...)`, bekommt einen PNG-Pfad zurück,
  lädt ihn in eine `gdk::Texture` und liefert sie zurück.
- Das Ergebnis wird pro Track-Pfad in einem kleinen LRU im Speicher gehalten,
  damit Scrollen back-and-forth nicht neu dekodiert.
- Fehlt ein Cover (oder Fehler), wird ein **Platzhalter** gesetzt (ein
  gemeinsames symbolisches Musiknoten-`gtk::Image`, kein Decode nötig).
- Recycling-fest: Beim Wiederverwenden einer Tracklisten-Zeile wird die
  laufende Cover-Anfrage der alten Zeile verworfen (Generation-Token pro
  Zelle), damit ein spät eintreffendes Cover nie in der falschen Zeile landet.

### 3. Anzeige-Oberflächen

- **Trackliste:** eine schmale, führende Cover-Spalte (48 px), lazy im selben
  Fenster-Muster wie die SQL-Windows befüllt.
- **Player-Leiste:** 96 px-Cover links neben Titel (fett) und
  „Interpret — Album".
- **GNOME-Benachrichtigung:** nutzt denselben 96 px-Thumbnail-Pfad beim
  Titelwechsel (die Notification existiert bereits; sie bekommt nur das
  Cover-Bild ergänzt).

### 4. Player-Leiste + Positions-Einstellung

- Zwei Positionen über `adw::ToolbarView`: **oben** (`add_top_bar`) und
  **unten** (`add_bottom_bar`, Default). Der frühere „schwebend"-Vorschlag ist
  bewusst verworfen (Nutzer-Entscheidung 2026-07-12) — ersetzt durch die
  Now-Playing-Vollansicht (§5), den GNOME-nativen Blickfang, der den Platz
  nutzt statt über der Liste zu schweben.
- **Persistenz:** neuer typisierter Accessor in der Settings-Façade
  (`reprise-core::library::settings`):

  ```rust
  pub fn get_player_bar_position(conn: &Connection) -> PlayerBarPosition;
  pub fn set_player_bar_position(conn: &Connection, pos: PlayerBarPosition)
      -> Result<(), rusqlite::Error>;
  pub enum PlayerBarPosition { Top, Bottom } // Default: Bottom
  ```

  Speicherung als kanonischer String (`"top"`/`"bottom"`) über
  die bestehenden `get_setting`/`set_setting`; ein unbekannter/handeditierter
  Wert fällt tolerant auf `Bottom` zurück (mit `tracing::warn!`) — dieselbe
  Toleranz-Haltung wie `get_bool` (Task 7). Damit hat der in Task 7 bewusst
  zurückgestellte Enum-Accessor jetzt seinen ersten realen Konsumenten (YAGNI
  erfüllt).
- **Umschalten sofort wirksam:** Position wechseln löst die Leiste aus ihrer
  aktuellen Aufhängung und hängt sie in die neue — kein Neustart. (Die
  Auswahl-UI selbst gehört zum Einstellungs-Dialog einer späteren Etappe;
  GUI-A liefert den persistierten Zustand, beide Aufhängungen und einen
  headless-Schalthebel zum Verifizieren.)

### 5. „Now Playing"-Vollansicht (Amberol-Stil)

Ein Klick auf die Player-Leiste öffnet eine großflächige Wiedergabe-Ansicht,
die den Fensterinhalt ersetzt (kein Overlay über der Liste) — der GNOME-native
Blickfang, der den Platz nutzt statt zu verdecken.

- **Navigation:** eigene Seite im bestehenden `adw`-Navigations-Stack; Klick
  auf die Leiste pusht sie, Zurück-Geste/Escape kehrt zur Liste zurück. Die
  Minimal-Leiste bleibt der Ausgangs-Kontext.
- **Inhalt:** großes Cover (1024 px aus dem Cache, in gedeckelter Anzeigegröße
  → nie hochskaliert, daher gestochen scharf auch auf HiDPI), Titel/Interpret/Album
  prominent (klare Typo-Hierarchie), Seekbar mit Positions-/Dauer-Labels,
  Transport (Vorheriger/Play-Pause/Nächster), Shuffle/Repeat.
- **Ein Zustand, kein zweiter Pfad:** alle Bedienelemente binden an denselben
  `PlayerController` und dieselben Aktionen wie die Leiste — kein duplizierter
  Wiedergabe-/Seek-Zustand (sonst driften Leiste und Vollansicht auseinander,
  dieselbe Disziplin wie bei der MPRIS-Spiegelung).
- **Kein Cover:** derselbe Platzhalter wie sonst, nur groß.
- **Bewusst noch nicht:** Lyrics-Panel und ambienter Farb-Glow aus dem Cover
  (spätere Erweiterungen dieser Ansicht, nicht GUI-A).

## Datenfluss

1. Trackliste bindet eine sichtbare Zeile → fragt `cover_loader` nach
   `(track_path, List)`.
2. Loader: In-Memory-LRU-Treffer? → sofort setzen. Sonst Platzhalter setzen +
   Worker-Anfrage abschicken (mit Generation-Token der Zelle).
3. Worker: `covers::resolve_source` → `covers::thumbnail(List)` → PNG-Pfad →
   `gdk::Texture`.
4. Ergebnis zurück in den Main-Loop: Token noch aktuell? → `gtk::Image` setzen
   + LRU füllen. Token veraltet (Zeile recycelt)? → verwerfen.
5. Player-Leiste/Notification: analog mit `Bar`-Größe beim Titelwechsel.
6. Now-Playing-Vollansicht: beim Öffnen `Full`-Größe (1024 px) fürs große Cover;
   derselbe Loader, derselbe Platzhalter.

## Fehlerbehandlung

- Kein Cover / defektes Bild / nicht lesbare Datei → Platzhalter, kein Panic,
  keine ERROR-Zeile (höchstens `debug!`).
- Cache-Verzeichnis nicht anlegbar/beschreibbar → einmal `warn!`, App läuft
  ohne persistente Thumbnails weiter (dekodiert dann pro Sitzung neu).
- Unbekannter Positions-Wert in den Settings → `Bottom` + `warn!`.

## Teststrategie

**Unit (in `reprise-core`, pure — kein Display nötig):**
- `resolve_source`: eingebettetes Cover erkannt; Ordnerbild-Fallback
  (`cover.jpg`/`folder.png`, Groß-/Kleinschreibung); nichts vorhanden → None;
  Priorität eingebettet vor Ordnerbild.
- `thumbnail`: erzeugt PNG exakt der Zielkantenlänge; zweiter Aufruf trifft den
  Cache (kein erneuter Decode — z. B. per mtime/Aufruf-Zähler geprüft); Hash-
  Keying dedupliziert identische Quellbytes; defektes Bild → `CoverError`, kein
  Panic; **Cache-Pfad liegt nachweislich unter dem Cache-Dir, nie im Track-
  Ordner** (explizite Zusicherung des Kernversprechens).
- Enum-Accessor: Round-Trip beider Werte (Top/Bottom); unbekannter/hand-
  editierter Wert → `Bottom`.

**Headless-Smoke (in `reprise-gnome`, isoliert — nie ein Fenster auf dem
echten Desktop):** eine Smoke-Bibliothek mit eingebettetem Test-Cover scannen,
starten → keine ERROR-Zeilen, Cover-Spalte befüllt; einen Positions-Schalthebel
(Env-Hook) für oben/unten durchfahren → Exit 0, Leiste in der erwarteten
Aufhängung; einen Hook, der die Now-Playing-Vollansicht öffnet → Exit 0, großes
Cover gesetzt, Zurück kehrt zur Liste.

## Explizit NICHT (YAGNI, bewusst verworfen)

- Kein Online-Cover-Abruf in GUI-A selbst — das ist GUI-A2 (opt-in Modul, siehe
  „Nicht enthalten"). GUI-A liefert nur den `resolve_source`-Einhängepunkt.
- Keine vierte Thumbnail-Größe „auf Vorrat" — genau drei Konsumenten (Liste,
  Leiste, Vollansicht), genau drei Größen.
- Keine Cover-Wand/Grid-Ansicht (spätere GUI-Etappe).
- Kein Cover-Schreiben/-Einbetten (gehört in den Tag-Editor, GUI-B).
- Keine Cache-Größenbegrenzung/-Eviction-Policy in GUI-A (der Cache ist klein
  und nutzergefahrlos löschbar; eine Eviction lohnt erst mit Messdaten).
- Keine schwebende Player-Leiste (verworfen 2026-07-12; ersetzt durch die
  Now-Playing-Vollansicht).
- Kein Lyrics-Panel und kein ambienter Cover-Farb-Glow in der Now-Playing-
  Ansicht (spätere Erweiterungen).
- Keine Positions-Auswahl-UI mit Vorschaukarten in GUI-A (gehört in den
  Einstellungs-Dialog einer späteren Etappe) — nur der persistierte Zustand
  und die zwei funktionierenden Aufhängungen.
