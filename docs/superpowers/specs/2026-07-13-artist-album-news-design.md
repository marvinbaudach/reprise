# Interpreten- und Albumneuigkeiten im Informationspanel — Design

## Ziel

Reprise erhält ein standardmäßig deaktiviertes Plugin **Artist & Album News**.
Wählt der Nutzer einen Titel aus, zeigt die rechte Informationsspalte neue und
angekündigte Alben beziehungsweise EPs des Interpreten. Grundlage sind die
Interpreten und Alben der lokalen Bibliothek; Reprise lädt keinen allgemeinen
redaktionellen Feed und erstellt kein Nutzerprofil.

Die erste Ausbaustufe beantwortet bewusst nur eine klare Frage: „Welche neuen
oder kommenden regulären Veröffentlichungen dieses Interpreten fehlen in
meiner Bibliothek?“ Tourdaten, Meldungstexte, Konten und Empfehlungen sind
spätere eigenständige Etappen.

## Provider und Datenschutz

MusicBrainz ist der einzige Provider dieser Etappe. Die offizielle API erlaubt
die Suche eines Interpreten nach Namen und anschließend das Browsen seiner
Release-Groups nach MBID. Reprise fragt ausschließlich `artist` sowie
`release-group` ab und sendet keine Titelpfade, Hörhistorie, Bewertungen,
Bibliotheksgröße, Kennung oder Telemetrie.

- Das Plugin ist standardmäßig **aus**.
- Die Aktivierung erklärt: Interpretennamen werden an MusicBrainz gesendet.
- Erst ein ausgewählter beziehungsweise aktuell geladener Interpret löst eine
  Abfrage aus. Öffnen des Panels ohne Kontext erzeugt keinen Netzwerkzugriff.
- MusicBrainz-Anfragen laufen seriell und global mit mindestens einer Sekunde
  Abstand. Coverdownload und News teilen denselben Rate-Limiter.
- Der User-Agent lautet `Reprise/<version>
  ( https://github.com/marvinbaudach )`. Die URL ist der bereits konfigurierte
  Maintainer-Account und ersetzt den nicht erreichbaren Projektplatzhalter.
- Alle Antworten werden unter `$XDG_CACHE_HOME/reprise/artist-news` gecacht.
  Musikdateien und die reale Bibliotheksdatenbank werden nicht verändert.

## Interpretenauflösung

Die Tracktabelle besitzt derzeit keine MusicBrainz-Artist-ID. Deshalb erfolgt
vor dem Browse eine konservative Suche:

1. Der sichtbare Interpret wird getrimmt und whitespace-/case-normalisiert.
2. `GET /ws/2/artist/?query=artist:"…"&fmt=json&limit=5` liefert Kandidaten.
3. Nur Kandidaten mit Score mindestens 95 und exakt normalisiertem Namen sind
   zulässig.
4. Gibt es mehr als einen gleichwertigen exakten Kandidaten, wird kein MBID
   geraten. Das Panel zeigt „Artist could not be matched unambiguously“.
5. Ein eindeutiger MBID wird im Cache gespeichert und für spätere Aktualisierung
   wiederverwendet.

Damit erhält ein gleichnamiger Interpret niemals still die Diskografie eines
anderen. Eine spätere Scanner-Etappe kann eingebettete Artist-MBIDs direkt
persistieren; sie ist nicht Voraussetzung dieser Etappe.

## Veröffentlichungen

Nach eindeutiger Auflösung verwendet Reprise den offiziellen Browse-Endpunkt:

`/ws/2/release-group?artist=<MBID>&type=album|ep&release-group-status=website-default&limit=100&fmt=json`

Es werden maximal die ersten 100 Album-/EP-Gruppen betrachtet. Die erste
Etappe lädt keine weiteren Seiten: Das genügt für Neuigkeiten und verhindert
große Hintergrundläufe bei umfangreichen Diskografien.

Aus jeder Release-Group werden nur MBID, Titel, `first-release-date`, primärer
Typ und sekundäre Typen übernommen. Compilation, Live, Remix, Soundtrack,
Mixtape/Street und DJ-Mix werden ausgeblendet. Ein lokaler Vergleich
normalisiert Albumtitel wie die Interpretenauflösung; ein vorhandenes Album
wird nicht als Neuigkeit gezeigt.

Kategorien relativ zum lokalen Datum:

- **Upcoming:** Datum liegt heute bis maximal 365 Tage in der Zukunft.
- **New:** Datum liegt höchstens 365 Tage in der Vergangenheit.
- Ältere fehlende Alben gehören zur späteren vollständigen Radar-
  Nachkaufliste und erscheinen hier nicht.

Unvollständige Jahres- oder Monatsdaten werden konservativ an den Anfang des
bekannten Zeitraums gelegt. Ein komplett fehlendes Datum wird nicht als News
gezeigt. Sortierung: kommende Veröffentlichungen aufsteigend, danach neue
Veröffentlichungen absteigend; maximal fünf Karten.

## Cache und Fehlerfälle

Ein versioniertes JSON-Dokument pro normalisiertem Interpret enthält MBID,
Providerzeitpunkt und die gefilterten Karten. Die Cachedatei wird atomar über
eine temporäre Geschwisterdatei veröffentlicht.

- Frischer Cache: sieben Tage, keine Netzwerkabfrage.
- Manueller Refresh: umgeht nur die TTL, nicht Rate-Limit oder Privacy-Gate.
- Offline, Timeout, HTTP 429/503 oder Parsefehler: vorhandenen Cache als
  „Cached · updated …“ zeigen; ohne Cache eine nicht störende Fehlerkarte.
- Ambige/fehlende Interpretenauflösung: negative Cachezeit 24 Stunden, damit
  wiederholte Auswahl nicht erneut fragt.
- Ein Ergebnis einer alten Auswahl darf dank Generationstoken nie den neuen
  Interpreten überschreiben.
- Cachefehler blockieren weder Panel noch Playback und werden ohne Pfade oder
  Antwortinhalte geloggt.

## Rechtes Informationspanel

Das Panel setzt die bereits beschlossene rechte Spalte konkret um. Eine feste
horizontale GTK-Komposition sitzt innerhalb des Library-Content-Panes: Der
flexible Tabelleninhalt und die Informationsspalte sind direkte Geschwister.
Ein overlay-fähiger Container wird bewusst nicht verwendet:

- Sichtbar: feste 340 logische Pixel Breite; die Tabelle gibt entsprechend
  Breite ab, das Panel überlagert sie nie.
- Schmale Fenster: die linke Navigation bleibt unabhängig adaptiv, während die
  rechte Spalte weiterhin neben dem Tabelleninhalt liegt.
- Versteckt: der gespeicherte Sichtbarkeitswunsch gibt die volle Breite wieder an
  die Tabelle frei.
- Headerbar: „Information“, Refresh, Schließen.
- Lokaler Kopf: Cover, Titel, Interpret, Album des einzelnen ausgewählten
  Tracks. Bei keiner Auswahl wird der aktuell sichtbare/geladene Track
  verwendet; Mehrfachauswahl zeigt nur die Anzahl und startet keine News-
  Abfrage.
- Plugin aus: eine kompakte Karte erklärt den Opt-in und bietet denselben
  persistenten Schalter wie Preferences.
- Plugin an: Ladezustand, bis zu fünf Release-Karten und Quellen-/Cachezeile.

Jede Release-Karte zeigt Typ/Status, Titel und Datum. Ein externer
MusicBrainz-Knopf öffnet nach Nutzeraktivierung die Release-Group im
Standardbrowser über `GtkUriLauncher`; es gibt kein eingebettetes WebView.

## Gemeinsamer Laufzeitzustand

`ArtistNewsRuntime` besitzt genau einen `enabled`-Wert, einen seriellen Worker
und schwache Statussubscriber. Informationspanel und Preferences verändern
denselben Zustand. Deaktivierung:

- persistiert `module.artist_news.enabled=false`;
- erhöht die Panelgeneration und verwirft sichtbare Onlinekarten;
- löscht keinen Cache;
- beeinflusst weder Coverdownload noch andere Module.

Die rechte Oberfläche verwendet den vorhandenen `TrackList`-`CoverLoader` für
den lokalen Kopf. Es entsteht keine zweite Coverpipeline.

## Architektur

### `reprise-core`

- `musicbrainz.rs`: gemeinsamer User-Agent, HTTP-Timeout, globaler
  Ein-Sekunden-Limiter und blockierender GET-Vertrag.
- `artist_news.rs`: URLs, JSON-Parsing, konservatives Matching,
  Datums-/Albumfilter, Cache und blockierender Refresh.
- `queries/artist_context.rs`: lokal vorhandene Alben eines Interpreten.
- `modules.rs`: `ARTIST_NEWS_MODULE`, default off.
- `library/settings.rs`: `ui.info_panel_visible`, default true.

Core bleibt frei von GTK, Adwaita, GStreamer und zbus.

### `reprise-gnome`

- `artist_news_worker.rs`: `ArtistNewsRuntime`, Request/Response-Kanal,
  serieller Thread und Live-Aktivierung.
- `info_panel_state.rs`: pure Kontext-, Breakpoint- und
  Generationentscheidungen.
- `info_panel.rs`: Widgets, Karten, geteilter CoverLoader und sichere URI-
  Aktivierung.
- `library_shell.rs`: extrahiert die Content-/Navigation-Komposition aus der
  bereits 799 Zeilen großen `window.rs` und setzt das rechte SplitView ein.
- `track_list.rs`: kleiner Auswahlcallback ohne Panelabhängigkeit.
- `preferences.rs`: generische Pluginzeile schaltet Artist News live.

Kein Core-Typ enthält GTK-Widgets; keine fremde Plugin-ABI entsteht.

## Tests und Verifikation

- Fixturetests: URL-Encoding, eindeutige/ambige Artist-Suche, Album-/EP-
  Parsing, Ausschluss sekundärer Typen, lokaler Albumabgleich,
  Datumsgrenzen, Sortierung und Fünferlimit.
- Cachetests: frischer Treffer ohne Netz, atomischer Roundtrip, stale fallback,
  negatives 24-Stunden-Ergebnis und korrupte Datei.
- Querytests: Artist und Album-Artist, fehlende Tracks ausgeschlossen,
  deduplizierte Albumtitel.
- Runtime-/Zustandstests: Default off, Aktivierung, Deaktivierung invalidiert
  Generation, stale Workerantwort wird verworfen, Mehrfachauswahl fragt nicht.
- Displaytest: Panel öffnen/schließen, Opt-in, Fixtureergebnis mit Upcoming/New,
  Accessible Names und schmale Overlaygeometrie.
- Vollständig isolierter App-Smoke mit Fixture-Provider: Track auswählen,
  News anzeigen, Auswahl wechseln und beweisen, dass keine alte Karte landet.
- Alle Projektgates, Core-Purity, gettext, Releasechecker und Dateigrößenregel.

## Explizit nicht Teil

- Keine allgemeinen Musiknachrichten, Artikeltexte, RSS-Feeds oder Bandcamp-
  Scraping.
- Keine Tourdaten, Standortverarbeitung, Ticketlinks oder Benachrichtigungen.
- Keine vollständige historische „fehlende Alben“-Nachkaufliste.
- Kein automatisches periodisches Bibliotheks-Polling und kein Startzeitplan.
- Kein Konto, keine MusicBrainz-Schreiboperation und keine Dateiänderung.
- Keine Artist-Bilder aus Drittquellen und kein eingebetteter Browser.
