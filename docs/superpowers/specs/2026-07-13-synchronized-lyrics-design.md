# Synchronisierte Songtexte im Informationspanel — Design

## Ziel

Reprise lädt Songtexte für den tatsächlich abgespielten Titel automatisch und
zeigt sie in der bestehenden rechten Informationsspalte. Enthält der Provider
Zeitmarken, wird die aktuelle Zeile passend zur Wiedergabeposition
hervorgehoben und die Ansicht ruhig dorthin nachgeführt. Liegt nur normaler
Text vor, bleibt er vollständig lesbar, aber ohne vorgetäuschte Synchronität.

Die Funktion ersetzt keine Musikansicht und führt insbesondere keine neue
große Cover- oder Now-Playing-Seite ein. Sie ergänzt das bestehende
Informationspanel um den oben erreichbaren Bereich **Lyrics**.

## Provider und Datenschutz

LRCLIB ist der einzige Provider dieser Etappe. Reprise verwendet den exakten
Lookup `GET /api/get` mit Titel, Interpret, Album und Dauer. Diese vier
Metadaten verlassen das Gerät erst dann, wenn ein Titel tatsächlich zum
Abspielen geladen wurde. Pfad, Datenbank-ID, Hörhistorie, Bewertung,
Bibliotheksgröße und Gerätekennung werden nie übertragen.

- Songtextabruf ist immer aktiv und besitzt keinen Schalter in Preferences,
  Plugins, Onboarding oder Kontextmenüs.
- Es gibt keinen Massenabruf für die Bibliothek und kein periodisches Polling.
- Ein frischer Cachetreffer erzeugt keinen Netzwerkzugriff.
- Reprise identifiziert sich mit `Reprise/<version>` und der bereits
  verwendeten erreichbaren Maintainer-URL.
- Vollständige Songtexte werden weder geloggt noch in SQLite, Musikdateien
  oder eingebettete Tags geschrieben.
- Der Cache liegt ausschließlich unter
  `$XDG_CACHE_HOME/reprise/lyrics` und darf jederzeit gelöscht werden.

LRCLIB liefert `syncedLyrics`, `plainLyrics` und den Zustand `instrumental`.
Synchronisierter Text wird bevorzugt. Fehlt er, verwendet Reprise den
normalen Text. Eine Instrumentalantwort wird nicht als Fehler behandelt.

## Track-Abgleich und Cache

Eine Anfrage besteht aus getrimmtem Titel, Interpret, Album sowie der auf
Sekunden gerundeten Dauer. Titel und Interpret müssen nicht leer sein; ohne
diese Felder erfolgt kein Netzwerkzugriff. Die Dauer bleibt Teil von Anfrage
und Cacheidentität, damit ähnlich benannte Versionen nicht vertauscht werden.

Der Cache ist ein versioniertes JSON-Dokument pro normalisiertem
`artist + title + album + duration`. Der Dateiname verwendet den vorhandenen
stabilen Reprise-Hash. Das Dokument enthält die Anfrageidentität, Abrufzeit
und genau einen der Zustände:

- synchronisierte Zeilen;
- normaler Text;
- instrumental;
- nicht gefunden.

Positive Ergebnisse bleiben ohne automatische Ablaufzeit verwendbar. Ein
„nicht gefunden“-Eintrag wird nach sieben Tagen erneut geprüft, weil der
Provider später ergänzt werden kann. Cachedateien werden atomar über eine
eindeutige temporäre Geschwisterdatei veröffentlicht. Beschädigte oder
unpassende Dokumente gelten als Cachemiss und blockieren Playback nie.

HTTP 404 ist ein sauberer, negativ gecachter Miss. Timeout, Offline, 429, 5xx,
unlesbarer Body und ungültiges JSON sind vorübergehende Fehler und werden
nicht negativ gecacht. Existiert bereits ein positiver Cache, bleibt er auch
bei einem späteren Netzfehler sichtbar.

## LRC-Verarbeitung und Positionslogik

Reprise parst die üblichen LRC-Zeitmarken `[mm:ss]`, `[mm:ss.x]`,
`[mm:ss.xx]` und `[mm:ss.xxx]`. Mehrere Zeitmarken vor derselben Textzeile
erzeugen mehrere Zeilenereignisse. Metadatenfelder wie `[ar:…]`, `[ti:…]`
und `[offset:…]` werden in dieser Etappe ignoriert; ein Offset wird nicht
geraten. Unbekannte oder fehlerhafte Zeilen werden übersprungen, nicht als
normaler Text in die synchronisierte Liste gemischt.

Nach dem Parsen werden Ereignisse stabil nach Startzeit sortiert. Die aktive
Zeile ist das letzte Ereignis mit `start_ms <= position_ms`; vor der ersten
Zeitmarke ist keine Zeile aktiv. Gleiche Zeitmarken bleiben in
Providerreihenfolge. Leere synchronisierte Nutzdaten fallen auf
`plainLyrics` zurück.

Die vorhandenen `PlayerEvent::Position`-Ereignisse im 500-ms-Takt sind die
einzige Zeitquelle. Es entsteht kein zweiter GStreamer-Ticker. Auch ein Seek
landet beim nächsten vorhandenen Positionsereignis auf der korrekten Zeile.

## Oberfläche

Das bestehende 340-Pixel-Informationspanel erhält oben einen nativen
`GtkStackSwitcher` mit **Information** und **Lyrics**. Der Header behält
Schließen, sichtbaren Requestfortschritt und die kontextabhängige
Aktualisieren-Aktion. Der Information-Bereich bleibt in Verhalten und
Auswahlbezug unverändert.

Der Lyrics-Bereich folgt ausschließlich dem laufenden Titel:

- kein geladener Titel: „Play a track to see its lyrics“;
- laufender Lookup: Spinner und Titel/Interpret;
- synchronisiert: eine vertikale Liste, aktuelle Zeile mit Accent und
  stärkerem Schriftgewicht, umliegende Zeilen normal;
- nur normaler Text: umbrochener, auswählbarer Text;
- instrumental: „Instrumental“;
- nicht gefunden: „No lyrics found“;
- vorübergehender Fehler ohne Cache: kompakte Offline-/Fehleranzeige mit
  manuellem Wiederholen.

Beim Wechsel der aktiven Zeile scrollt die Ansicht nur dann, wenn sich der
Index tatsächlich ändert. Das Ziel wird vertikal ungefähr in der Mitte des
sichtbaren Bereichs platziert und an Anfang/Ende geklemmt. Dadurch ist der
Übergang sichtbar, ohne bei jedem 500-ms-Tick zu zittern. Manuelles Scrollen
wird nicht dauerhaft gesperrt; der nächste echte Zeilenwechsel nimmt die
Nachführung wieder auf.

Eine alte Workerantwort darf dank Generationstoken nie Text für einen neueren
Titel anzeigen. Stoppen leert den Wiedergabekontext. Pausieren behält Text und
Markierung. Das Ausblenden des Informationspanels stoppt weder Playback noch
Cachearbeit.

## Architektur

### `reprise-core`

- `lyrics.rs`: Anfrage-/Ergebnistypen, LRCLIB-URL und HTTP-Klassifikation,
  JSON-/LRC-Parsing, aktive-Zeile-Entscheidung, Cache und blockierender
  `load_or_fetch`-Vertrag.
- `lyrics_tests.rs`: ausschließlich synthetische, kurze Fixturetexte; keine
  realen oder urheberrechtlich geschützten Songtexte.

Core bleibt frei von GTK, Adwaita, GStreamer und zbus.

### `reprise-gnome`

- `lyrics_worker.rs`: ein serieller dedizierter Thread; nur `Send`-Daten
  überqueren die Grenze, nie GTK-Objekte.
- `lyrics_view.rs`: eigener Widgetbaum, Zeilenrendering, Highlight und
  geklemmte Scrollentscheidung.
- `lyrics_strings.rs`: neue übersetzbare Texte, weil `strings.rs` bereits an
  der 800-Zeilen-Grenze liegt.
- `info_panel.rs`: bestehende Information als erste Stack-Seite, Lyrics als
  zweite Stack-Seite; keine Providerlogik.
- `player_lyrics.rs`: kleiner Controller-Fan-out für Trackwechsel,
  Positionsereignisse und Stoppen, damit `player_controller.rs` und
  `now_playing_wiring.rs` unter 800 Zeilen bleiben.

## Tests und Verifikation

- Core-Unit-Tests für URL-Encoding, Dauer, JSON-Priorität, Instrumental,
  alle unterstützten LRC-Zeitformen, Mehrfachmarken, Metadaten/Fehlzeilen,
  stabile Sortierung und aktive Zeile.
- Cachetests mit privatem Temp-Verzeichnis und injiziertem Fetcher: frischer
  Treffer ohne Netz, atomischer Roundtrip, siebentägiger Negativcache,
  beschädigte Datei, 404 gegen vorübergehende Fehler und stale-positive
  Rückfallebene.
- Worker-/Zustandstests für Serienfolge, Generation und Trackwechsel.
- GTK-Displaytests für Stack-Umschaltung, Plain-/Instrumentalzustand,
  Accentklasse der aktiven Zeile und geklemmte mittige Nachführung.
- Vollständig isolierter App-Smoke mit lokalem LRCLIB-Fixture: Titel starten,
  Lyrics laden, Position/Seek nachführen und schneller Trackwechsel ohne
  stale Text. Kein Test spricht den echten Provider an.
- Vollständige Projektgates, Core-Purity, gettext, Rustdoc,
  Releasechecker und Dateigrößenregel.

## Explizit nicht Teil

- Kein Bibliotheksweiter Vorabdownload und keine Hintergrundaktualisierung.
- Kein Lyrics-Schalter und kein Plugin-/Modulzustand.
- Keine Lyrics-Suche anhand von Dateipfaden, Fingerprints oder Konten.
- Kein Schreiben von `.lrc`-Dateien neben Musik und keine Tagänderung.
- Kein Lyrics-Editor, Offset-Editor, Karaoke-Wortmarken oder Übersetzungen.
- Kein zweiter Playback-Timer und keine neue große Coveransicht.
- Keine Weitergabe, Veröffentlichung oder Synchronisation gecachter Texte.
