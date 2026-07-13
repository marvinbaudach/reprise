# Last.fm-Scrobbling — Design

## Ziel

Reprise erhält neben ListenBrainz eine unabhängige, standardmäßig deaktivierte
Last.fm-Integration. Nach expliziter Aktivierung und browserbasierter
Desktop-Autorisierung meldet Reprise den aktuell laufenden Titel und überträgt
abgeschlossene Hörvorgänge. Offline-Hörvorgänge bleiben dauerhaft erhalten und
werden in zeitlicher Reihenfolge nachgereicht.

Last.fm veröffentlicht keine anwendungsweit nutzbaren Reprise-Zugangsdaten. Die
erste Version verwendet deshalb bewusst "Bring your own API credentials": Der
Nutzer trägt den API-Key und das Shared Secret eines eigenen Last.fm-API-Kontos
ein. Beide Werte, der später erhaltene Session-Key und der Benutzername liegen
ausschließlich im System-Keyring. Nichts davon wird in Git, SQLite, Logs oder
Fehlermeldungen geschrieben. Ein späteres offizielles Reprise-API-Konto kann
diese Eingabefelder durch Build-Konfiguration ersetzen, ohne Queue- oder
Sessiondaten zu migrieren.

## Offizieller Protokollvertrag

- Desktop-Authentifizierung: `auth.getToken`, Autorisierung im Browser,
  anschließend `auth.getSession`.
- Signatur: Parameter nach Namen sortieren, Namen und Werte konkatenieren,
  Shared Secret anhängen, MD5 als kleingeschriebenes Hex; `format` und
  `callback` werden nicht signiert.
- Now Playing: signierter POST `track.updateNowPlaying`.
- Scrobble: signierter POST `track.scrobble`, maximal 50 Einträge pro Request.
- Pflichtfelder: Artist, Track und Startzeit; Album und Dauer sind optional.
- Fehler 9/4 bedeuten ungültige Session, 8/11/16/29 sowie HTTP 408/429/5xx
  sind wiederholbar; ungültiger/suspendierter API-Key oder Signaturfehler sind
  permanente Konfigurationsfehler.

Quellen:

- https://www.last.fm/api/desktopauth
- https://www.last.fm/api/authspec
- https://www.last.fm/api/show/track.updateNowPlaying
- https://www.last.fm/api/show/track.scrobble

## Umfang

### Enthalten

- eigenes Plugin `lastfm`, standardmäßig aus und live aktivierbar;
- API-Key-/Shared-Secret-Eingabe, Browserautorisierung und Session-Austausch;
- sichere Speicherung und vollständiges Trennen im Secret Service;
- Now-Playing nach erfolgreichem Wiedergabestart;
- dieselbe Schwelle wie ListenBrainz: Hälfte oder vier Minuten;
- unabhängige dauerhafte FIFO mit maximal 50 Einträgen pro Last.fm-Request;
- Retry/Backoff, Cancellation und Generation Guards über den gemeinsamen
  Worker;
- deutscher gettext-Katalog, Datenschutztext und isolierter Fake-API-Smoke.

### Nicht enthalten

- eingebetteter persönlicher oder offizieller Reprise-API-Key;
- Passwortauthentifizierung oder `auth.getMobileSession`;
- Love/Unlove, Tags, Empfehlungen, Profilcharts oder Last.fm-Datenimport;
- Zusammenführen von ListenBrainz- und Last.fm-Queues;
- automatisches Öffnen eines Browsers beim App-Start;
- echte Konten, echte Musikdateien oder Produktionsnetzwerk in Agententests.

## Core-Architektur

`reprise-core::scrobbling` wird wegen der Dateigrößengrenze ein Verzeichnis:

- `mod.rs`: gemeinsame `TrackMetadata`, `Listen`, Schwelle, Transportvertrag,
  providerselektierte Queue-Operationen und Fehler;
- `listenbrainz.rs`: unveränderter JSON-/Token-Backendvertrag;
- `lastfm.rs`: Last.fm-Signatur, Desktop-Auth, Form-Requests und JSON-Antworten.

`ScrobbleProvider::{ListenBrainz, LastFm}` ist die einzige Quelle für
Tabellenauswahl und Batchlimit. Dynamische SQL-Namen stammen ausschließlich aus
diesem Enum. Schema v6 ergänzt `lastfm_queue`; die bestehende
`listenbrainz_queue` bleibt unberührt, damit vorhandene Offline-Hörvorgänge ohne
riskante Tabellenmigration erhalten bleiben.

Der bestehende `ScrobblerTransport` bleibt providerneutral. Beim Last.fm-Client
sind API-Key und Shared Secret Konstruktorzustand; der bereits bestehende
`token`-Parameter trägt den Last.fm-Session-Key. `validate_token` verwendet einen
signierten `user.getInfo`-Request und liefert den Kontonamen.

Eine erfolgreiche Last.fm-Antwort gilt als konsumiert, auch wenn Last.fm einen
Eintrag fachlich als `ignored` markiert: Ein erneuter Versand kann einen
permanenten Metadaten-/Zeitfehler nicht reparieren und würde die FIFO blockieren.
Transport- oder Protokollfehler bestätigen dagegen keine Zeile.

## Frontend-Architektur

Der bisherige `ListenBrainzRuntime` wird ohne Verhaltensänderung zu
`ScrobbleRuntime` verallgemeinert. Konstruktorparameter sind Datenbankpfad,
Provider und Dienstname; `configure` erhält den Session-Schlüssel und ein
providerkonkretes `Box<dyn ScrobblerTransport>`. Jeder Provider besitzt eine
eigene Runtime-Instanz, Generation, Cancellation-Flag, Drain-Lock und Queue.

`PlayerController` behält genau eine `ScrobbleSession` pro Titel. Beim Start
sendet er Now-Playing an alle aktiven Provider. Beim Abschluss erzeugt er genau
ein unveränderliches `Listen` und reiht dessen Clone unabhängig bei jedem dann
aktiven Provider ein. Erfolg oder Ausfall eines Dienstes beeinflusst den anderen
nicht.

`preference_lastfm.rs` besitzt den gesamten Dialog- und Aktivierungsfluss:

1. API-Key und Shared Secret erfassen.
2. Off-main `auth.getToken` aufrufen.
3. die Last.fm-Autorisierungs-URI durch GIO öffnen;
4. erst nach explizitem "Finish Connection" off-main `auth.getSession` aufrufen;
5. vollständige Credentials im Keyring speichern;
6. Pluginflag setzen und Runtime mit Last.fm-Client starten.

Abbruch oder Fehler lässt das Plugin deaktiviert. Trennen stoppt die Runtime,
löscht Credentials und leert ausschließlich die Last.fm-Queue nach expliziter
Bestätigung im bestehenden Kontobereich.

## Fehler- und Datenschutzverhalten

- Kein Secret implementiert `Debug` oder erscheint in strukturierten Logs.
- Keyring-Fehler haben keinen Klartext-Fallback.
- Fehler 9/4 setzen den sichtbaren Status auf nicht autorisiert und behalten die
  Queue für neue Credentials.
- Netzwerk- und temporäre Dienstfehler behalten die Queue und verwenden 5 s bis
  5 min Backoff; neue Arbeit weckt den Worker.
- permanente API-Konfigurationsfehler stoppen den Worker sichtbar.
- Deaktivieren verwirft eine laufende Titelsitzung für diesen Provider, aber
  löscht bestehende Offline-Zeilen erst beim expliziten Trennen.
- Übertragen werden Artist, Titel, optional Album, Dauer und Startzeit.

## Tests

- reine Signatur-/Parameter-/URL-/JSON-/Fehlerklassifikationstests;
- lokaler TCP-Server prüft POST, Form-Encoding, Signatur und Secret-Abwesenheit;
- Migration-v6-, Providerisolation-, FIFO-, Batchlimit-, Ack- und Reopen-Tests;
- bestehende Runtime-Suite läuft für providerselektierte Queues; zusätzlicher
  Dual-Provider-Test beweist unabhängiges Ack/Retry;
- Sitzungsregressionen für nur Last.fm, beide Provider und Deaktivierung;
- Secret-Attribut-, Credential-Roundtrip- und Dialogzustandstests;
- ein ignorierter GTK-Displaytest für maskierte Secret-Eingabe;
- vollständig isolierter App-Smoke mit Loopback-Fake-Endpunkt und synthetischen
  Credentials, privatem XDG-Daten-/Cachepfad, eigener D-Bus-Session, Xvfb, X11,
  leerem Wayland-Display und fakesink.

## Manuelle Abnahme

- eigenes Last.fm-API-Konto, Browserfreigabe und Keyring-Prompt;
- korrektes Now Playing und Scrobble im echten Profil;
- Offline/Online-Nachlieferung und widerrufene Session;
- native GNOME-Anordnung, Fokus, Maskierung und Fehlermeldungen.

