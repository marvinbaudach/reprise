# GUI-A2: Online-Album-Cover-Download (opt-in) — Design

**Datum:** 2026-07-12
**Etappe:** GUI (Stage 5), Unter-Etappe **A2** — direkt nach GUI-A, vor GUI-B.
Nutzer-Entscheidung 2026-07-12 (nach dem Schärfe-/Pixeligkeits-Thema in GUI-A).

**Baut auf:** GUI-A (Cover-Pipeline `reprise-core::cover` mit `resolve_source` →
`thumbnail`, Cache in `$XDG_CACHE_HOME/reprise/covers/`, der `CoverLoader`,
Modul-Registry `reprise-core::modules`, Settings-Façade). GUI-A hat
`resolve_source` ausdrücklich als Einhängepunkt hinterlassen (Master-Spec
Z. 659; GUI-A-Spec „Nicht enthalten").

## Ziel

Für Tracks **ohne lokales Cover** (oder mit nur winzigem eingebettetem
Thumbnail) automatisch ein hochauflösendes Album-Cover von **Cover Art Archive**
laden — **opt-in, standardmäßig aus**. Das rettet genau die Fälle, in denen
GUI-As lokale Pipeline nichts Scharfes liefern kann.

## Scope

**Enthalten:**
- Ein gated Modul `cover_download` (Registry, `default_enabled: false`).
- Netz-Fetch (blockierend, off-thread): MusicBrainz-Release ermitteln →
  Cover Art Archive Front-Cover laden → in einen Download-Cache schreiben.
- `resolve_source` bekommt eine 3. Auflösungsstufe: Download-Cache (offline,
  rein — findet nur, was der async Fetch schon abgelegt hat).
- Ein Header-`MenuButton` (Haupt-Menü) mit Umschalter „Fehlende Album-Cover
  herunterladen".
- Rate-Limit (1 Anfrage/s an MusicBrainz), korrekter User-Agent, Negativ-Cache.

**Nicht enthalten (spätere Etappen/Optional):**
- Interpreten-/Künstlerbilder (fanart.tv/Wikidata) — späteres Optional.
- Aggressives Fuzzy-Matching (Falsch-Cover-Risiko) — bewusst verworfen.
- Herunterladenes Cover **in die Datei einbetten** (gehört zum Tag-Editor,
  GUI-B; A2 speichert nur in den Cache).
- Weitere Anbieter / eine `MetadataProvider`-Abstraktion (YAGNI — ein Anbieter).
- Einstellungs-Dialog (spätere Etappe); A2 liefert nur den einen Menü-Schalter.
- Manuelles „Cover neu suchen / auswählen" pro Album (späteres Optional).

## Kernversprechen & Privatsphäre (unverhandelbar)

- **Standardmäßig AUS.** Nichts geht ins Netz, bis der Nutzer das Modul aktiv
  einschaltet — konsequent zur abgelehnten Genius-Telemetrie.
- **Was gesendet wird, wenn an:** ausschließlich Album-Interpret + Albumname
  (bzw. eine eingebettete MusicBrainz-Release-ID) an MusicBrainz/Cover Art
  Archive — keine sonstige Telemetrie, keine Nutzer-ID, kein Tracking.
- **Wohin geschrieben wird:** heruntergeladene Cover landen **nur** unter
  `$XDG_CACHE_HOME/reprise/covers/downloaded/` — **niemals** in die
  Musiksammlung, niemals in die Audiodateien. Reine Lese-Beziehung zu den
  Dateien des Nutzers bleibt bestehen.

## Architektur

### 1. `reprise-core::cover_download` (neu, +Netzwerk-Dep)

**Neue Abhängigkeit:** `ureq` (blockierender, schlanker rustls-HTTP-Client;
kein async-Runtime — der Fetch läuft ohnehin off-thread) + das schon
vorhandene `serde_json` fürs MusicBrainz-JSON. `ureq` verletzt die
Dependency-Purität (kein gtk/gst/zbus) nicht; es lebt im Core, weil
Cover-Beschaffung geteilte Engine-Logik ist (Android/iOS nutzen sie später).
Bewusst hinzugefügt und dokumentiert; das Modul-Gate stellt sicher, dass der
Code zur Laufzeit nur bei eingeschaltetem Modul überhaupt Netz berührt.

**Öffentliche Oberfläche (abgeleitet, minimal):**

```rust
/// Versucht, ein Album-Cover zu beschaffen und im Download-Cache abzulegen.
/// Blockierend — NUR off-thread aufrufen. Gibt den Cache-Pfad des abgelegten
/// Bildes zurück, oder None (nicht gefunden / offline / Fehler). Legt bei
/// „nicht gefunden" einen Negativ-Marker an, damit nicht erneut angefragt wird.
pub fn fetch_and_cache(
    album_artist: &str,
    album: &str,
    mbid: Option<&str>,
) -> Option<PathBuf>;
```

**Ablauf innerhalb `fetch_and_cache`:**
1. **Album-Key** aus `normalize(album_artist) + "|" + normalize(album)` bilden
   (lowercase, trimmen, Mehrfach-Whitespace kollabieren); als Hash der
   Dateiname im Download-Cache. Ein Cover pro Album — alle Tracks teilen es.
2. **Schon da?** Download-Cache-Treffer oder Negativ-Marker → sofort
   zurück (kein Netz).
3. **Release ermitteln:** MBID bevorzugt. Liegt eine eingebettete
   MusicBrainz-Release-ID vor → direkt verwenden. Sonst **konservative Suche**
   `GET https://musicbrainz.org/ws/2/release?query=...&fmt=json&limit=5` und
   den besten Treffer nur nehmen, wenn Score hoch **und** Interpret/Album
   plausibel übereinstimmen (kein schwacher Fuzzy-Treffer → sonst falsches
   Cover). Findet nichts Sicheres → Negativ-Marker, Ende.
4. **Cover laden:** `GET https://coverartarchive.org/release/<mbid>/front`
   (Front-Cover, volle Auflösung; folgt der 302-Weiterleitung). 404 → nächster
   Release-Kandidat oder Negativ-Marker.
5. **Ablegen:** Bytes atomar (Temp + Rename) nach
   `<cache>/reprise/covers/downloaded/<album-key-hash>.<ext>` schreiben; Pfad
   zurück.

**Etikette & Robustheit:**
- **Rate-Limit:** global mindestens 1 s zwischen MusicBrainz-Anfragen
  (MusicBrainz-Vorgabe). Cover Art Archive ist toleranter, aber ebenfalls
  höflich behandeln.
- **User-Agent:** aussagekräftig, z. B. `Reprise/<version>
  ( https://github.com/…/reprise )` (MusicBrainz verlangt das).
- **Timeouts** auf jede Anfrage; offline / DNS-Fehler → still `None`, nie fatal.
- **Negativ-Cache:** ein `.notfound`-Marker pro Album-Key verhindert
  Dauer-Anfragen; (eine TTL ist später nachrüstbar, GUI-A2 nicht nötig).

### 2. `resolve_source` — 3. Auflösungsstufe (in `reprise-core::cover`)

`resolve_source` liest die Datei ohnehin einmal via lofty (für das eingebettete
Bild). Es extrahiert aus demselben `TaggedFile` zusätzlich Album-Interpret +
Album und, falls vorhanden, die eingebettete Release-MBID. Reihenfolge:

1. Eingebettetes Bild → `Embedded`.
2. Ordnerbild (`cover.*`/`folder.*`) → `FolderImage`.
3. **Download-Cache**: Album-Key bilden → `<downloaded>/<hash>.<ext>` da? →
   `FolderImage(pfad)`. (Rein offline — findet nur, was der async Fetch bereits
   ablegte.)
4. Sonst `None`.

`resolve_source` bleibt **synchron, offline, rein** — es löst **nie** selbst
einen Netz-Fetch aus. Kein Blockieren im Bind-Pfad.

### 3. Orchestrierung (Frontend, `CoverLoader`)

Wenn der Loader für einen Track ein Cover anfragt und `resolve_source` `None`
liefert **und** `modules::is_enabled(cover_download)`:
- Off-thread (bestehendes `gio::spawn_blocking`-Muster) die Track-Tags lesen
  (Album-Interpret/Album/MBID via lofty) und `cover_download::fetch_and_cache`
  aufrufen.
- Bei Erfolg denselben Generation-Guard-Pfad wie GUI-A nehmen: Thumbnail aus
  dem heruntergeladenen Bild erzeugen und die Zelle/Leiste aktualisieren —
  aber nur, wenn das Generation-Token noch aktuell ist (Zeile nicht recycelt).
- Der Fetch wird pro Album-Key **entdoppelt** (kein Sturm identischer Anfragen,
  wenn 12 Tracks eines Albums sichtbar werden) — in-flight-Set über den
  Album-Key.

### 4. Modul + Aktivierung

- **Registry:** `pub const COVER_DOWNLOAD_MODULE` (id `"cover_download"`,
  `default_enabled: false`) zu `ALL_MODULES` hinzufügen.
- **Aktivierung:** ein Header-`gtk4::MenuButton` (Haupt-Menü, neu — es gibt
  heute keins) mit einem Umschalt-Eintrag „Fehlende Album-Cover herunterladen",
  gebunden an `modules::set_enabled(cover_download, …)` /
  `is_enabled`. Beim Einschalten werden fehlende Cover für den sichtbaren
  Bereich nach und nach nachgeladen (kein Zwangs-Scan der ganzen Bibliothek —
  lazy, wie sonst auch). Das Haupt-Menü ist zugleich der künftige Ort für
  „Über" / „Einstellungen" (spätere Etappen).

## Datenfluss

1. Track sichtbar → Loader → `resolve_source`: eingebettet/Ordner/Download-Cache?
2. Treffer → Thumbnail → anzeigen (GUI-A-Pfad, unverändert).
3. `None` **und** Modul an → off-thread `fetch_and_cache` (rate-limited,
   album-key-entdoppelt) → schreibt in den Download-Cache.
4. Erfolg → `resolve_source` findet es beim nächsten Zugriff (bzw. die
   Orchestrierung aktualisiert die Zelle direkt, generation-guarded).
5. Nicht gefunden → Negativ-Marker → keine weiteren Anfragen für dieses Album.

## Fehlerbehandlung

- Offline / Timeout / DNS-Fehler → still `None`, kein Panic, keine ERROR-Zeile
  (höchstens `debug!`/`info!`).
- Nicht gefunden / 404 → Negativ-Marker, Platzhalter bleibt.
- Ungültige Bilddaten von CAA → wie „nicht gefunden" behandeln.
- Modul aus → der ganze Pfad wird nie betreten (kein Netz).

## Teststrategie

**Unit (pure, KEIN echtes Netz):**
- Album-Key-Normalisierung (Case/Whitespace/Trim; gleiche Alben → gleicher Key;
  verschiedene → verschieden).
- MusicBrainz-JSON-Parsing gegen **Fixtures** (gespeicherte Beispiel-Antworten):
  Release-MBID + Score korrekt extrahiert; schwacher Score → abgelehnt.
- Konservative Match-Logik: starker Treffer akzeptiert, schwacher verworfen.
- URL-Bau (Suche, direkte MBID, CAA-Front).
- Negativ-Cache: Marker verhindert erneute Anfrage.
- `resolve_source` 3. Stufe: Download-Cache-Datei wird gefunden; **Pfad liegt
  nachweislich unter dem Cache-Dir**, nie im Track-Ordner (Kernversprechen).
- Modul-Gate: bei `default_enabled: false` wird ohne explizites Einschalten
  nichts angefragt.

Der eigentliche HTTP-Aufruf wird in Tests **nicht** ausgeführt (die CI hat/darf
kein Netz). Falls ein End-to-End-Netz-Test gewünscht ist, dann als separater,
`#[ignore]`-markierter Integrationstest — nicht in der Standard-Suite.

**Headless-Smoke (isoliert, `XDG_DATA_HOME` + `XDG_CACHE_HOME` scratch — nie die
echte DB/den echten Cache):** Modul-Aus-Lauf → keine Netzanfrage, kein Fehler;
Menü-Umschalter togglet das persistierte Flag. (Ein echter Download wird nicht
headless in der CI geprüft — nur, dass der Aus-Zustand inert ist und das Gate
greift.)

## Explizit NICHT (YAGNI, bewusst verworfen)

- Kein aggressives Fuzzy-Matching (Falsch-Cover-Risiko).
- Keine Interpreten-/Künstlerbilder in A2.
- Kein Einbetten des heruntergeladenen Covers in die Audiodatei (GUI-B).
- Keine zweite Cover-Quelle / `MetadataProvider`-Abstraktion (ein Anbieter).
- Kein Einstellungs-Dialog (nur der eine Menü-Schalter); kein Pro-Album-
  „Cover auswählen"-UI.
- Kein Zwangs-Scan der ganzen Bibliothek beim Einschalten (lazy nachladen).
- Keine TTL/Größenbegrenzung des Download-Caches (nutzergefahrlos löschbar;
  später mit Messdaten).
