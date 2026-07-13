# ListenBrainz-Scrobbling — Designspezifikation

## Ziel

Reprise erhält seine erste kontobasierte Integration: optionales, standardmäßig
deaktiviertes Scrobbling zu ListenBrainz. Nach ausdrücklicher Aktivierung meldet
Reprise den aktuellen Titel als `playing_now` und überträgt einen dauerhaften Listen,
sobald die von ListenBrainz dokumentierte Schwelle erreicht wurde: die Hälfte des
Titels oder vier Minuten, je nachdem, was zuerst eintritt.

Netz- und Dienstfehler dürfen Wiedergabe, Bibliothek und Start niemals blockieren.
Fertige Listens bleiben bis zur bestätigten Übertragung in einer lokalen FIFO-
Warteschlange. Das Zugangstoken liegt ausschließlich im systemeigenen Secret Service
bzw. im Flatpak-Secret-Portal, nie in SQLite, Logs oder Umgebungs-Diagnosen.

## Umfang

### Enthalten

- neuer standardmäßig deaktivierter Plugin-Deskriptor `listenbrainz`;
- Token-Eingabe, Validierung über `/1/validate-token`, Kontoname und explizites
  Trennen des Kontos in den Einstellungen;
- sichere Tokenablage mit `oo7::Keyring`, einschließlich des verschlüsselten
  Flatpak-Dateibackends über das Secret-Portal;
- `playing_now` beim erfolgreichen Start eines neuen Wiedergabevorgangs;
- genau ein permanenter Listen pro Wiedergabesitzung nach Erreichen der Schwelle;
- persistente, geordnete SQLite-Warteschlange und erneute Zustellung nach Neustart,
  beim nächsten Titel sowie mit begrenztem Backoff;
- verständlicher Plugin-Status: nicht verbunden, wird verbunden, verbunden als,
  offline mit ausstehenden Listens oder Token abgelehnt;
- isolierte Tests mit Fake-Transport bzw. lokalem HTTP-Server, niemals mit einem
  echten Konto oder dem produktiven ListenBrainz-Dienst;
- vollständige englische Quelltexte und deutsche gettext-Übersetzungen;
- Datenschutzhinweis in README und AppStream.

### Nicht enthalten

- Last.fm und Libre.fm; die gemeinsame Schnittstelle wird dafür vorbereitet, aber
  nur das ListenBrainz-Backend implementiert;
- OAuth, Browser/WebView oder automatisches Auslesen fremder Zugangsdaten;
- Liebe/Hass-Feedback, Empfehlungen, ListenBrainz-Playlisten oder Historienimport;
- MusicBrainz-Metadaten-Nachschlagen nur für Scrobbling;
- Scrobbling externer MPRIS-Player oder von Dateien außerhalb der Reprise-
  Wiedergabe;
- periodische Telemetrie, Absturzberichte oder andere Hintergrundübertragungen;
- Löschen oder Verändern von Musikdateien.

## Architektur

### Core: Vertrag, JSON/HTTP und Offline-Warteschlange

`reprise-core::scrobbling` enthält GUI- und plattformfreie Datentypen:

```rust
pub struct TrackMetadata {
    pub artist_name: String,
    pub track_name: String,
    pub release_name: Option<String>,
    pub duration_ms: i64,
}

pub struct Listen {
    pub id: Option<i64>,
    pub listened_at: i64,
    pub track: TrackMetadata,
}

pub trait ScrobblerTransport: Send + 'static {
    fn validate_token(&self, token: &str) -> Result<String, TransportError>;
    fn playing_now(&self, token: &str, track: &TrackMetadata)
        -> Result<(), TransportError>;
    fn submit(&self, token: &str, listens: &[Listen])
        -> Result<(), TransportError>;
}
```

Der konkrete `ListenBrainzClient` verwendet den vorhandenen blockierenden
Rustls-Client `ureq`, feste HTTPS-Produktionsendpunkte, kurze Timeouts und
`Authorization: Token ...`. Der Token wird nie in einem Fehlerwert oder Logfeld
ausgegeben. JSON entsteht mit `serde`, nicht per Stringkonkatenation. Leere Titel
oder Interpreten werden vor dem Einreihen abgewiesen, weil die API beide Felder
verlangt.

Eine Schema-Migration fügt `listenbrainz_queue` hinzu. Gespeichert werden nur die
zum späteren Senden notwendigen Titelmetadaten und der Startzeitpunkt; niemals der
Token. Ein erfolgreicher Batch bis maximal 1000 Einträge wird in einer Transaktion
gelöscht. Bei Netzwerk-, Rate-Limit- und 5xx-Fehlern bleiben alle Zeilen erhalten.
401 markiert die Verbindung als abgelehnt und stoppt automatische Versuche, bis
ein neues Token gespeichert wurde. Ein explizites „Konto trennen“ löscht Token
und lokale ListenBrainz-Warteschlange; bloßes Ausschalten behält bereits
eingereihte Listens lokal und sendet nichts.

### GNOME: Secret Store und Worker

`ui::listenbrainz_secret` kapselt ausschließlich die `oo7`-Aufrufe. Lookup-
Attribute enthalten App-ID und Dienstname, aber keine geheimen Daten. Die API ist
asynchron und läuft über den GLib-Main-Context; Keyring-Fehler werden als Status
angezeigt, nicht durch unsichere Ersatzspeicherung umgangen.

`ui::listenbrainz_runtime` besitzt einen dedizierten Worker-Thread. Nur Plain-
Rust-Daten, Token und Befehle überschreiten die Threadgrenze; keine GTK-Objekte und
keine `Rc<RefCell<Connection>>`. Der Worker öffnet eine eigene Verbindung zur
isolationsfähigen `reprise_core::db::default_path()`, validiert das Token, sendet
`playing_now` best-effort und leert die FIFO-Warteschlange. Statusmeldungen laufen
per Channel auf den Main-Context und verwenden eine Generation, damit Ergebnisse
eines alten Tokens nach Neuverbinden/Trennen verworfen werden.

Backoff beginnt bei fünf Sekunden und wächst bis fünf Minuten. Neue Arbeit weckt
den Worker sofort. Stoppen oder Neukonfigurieren blockiert den GTK-Thread nicht auf
einen laufenden HTTP-Timeout.

### Wiedergabeintegration

Der bestehende `PlayerController` bleibt einzige Quelle für Wiedergabesitzungen.
Nach erfolgreichem `player.play` erhält die Runtime Metadaten und Startzeitpunkt für
`playing_now`. Beim bereits vorhandenen Abschlussweg `evaluate_play_tracking` wird
zusätzlich `should_scrobble` geprüft und genau einmal in SQLite eingereiht. Damit
decken Titelwechsel, natürliches Ende und Fehler denselben idempotenten Weg ab.

Die lokale Play-Count-Regel bleibt unverändert bei 50 Prozent. ListenBrainz nutzt
separat seine dokumentierte Schwelle `min(duration / 2, 4 min)`. Es zählt wie die
bestehende Play-Count-Logik die höchste gemeldete Position; Pause allein erhöht sie
nicht. Metadaten werden vor jedem Callback aus `RefCell`s herauskopiert.

### Einstellungen

Die Plugins-Seite zeigt eine live wirkende ListenBrainz-Zeile sowie eine
Kontokonfiguration. Aktivieren ohne Token öffnet die Konfiguration und bleibt bis
zu einem erfolgreich gespeicherten Token aus. Das Tokenfeld ist verdeckt und zeigt
nie den bestehenden Wert. Speichern startet Validierung/Worker ohne den Dialog zu
blockieren. Trennen ist destruktiv nur für das gespeicherte Token und die lokale
Scrobble-Warteschlange, niemals für Musikdateien oder die allgemeine Bibliothek.

## Fehler- und Datenschutzverhalten

- Standard ist aus; ohne aktives Modul und Token findet kein ListenBrainz-Netzwerk-
  zugriff statt.
- Logs enthalten Statuscode, Kategorie und Warteschlangenlänge, niemals Token oder
  kompletten Authorization-Header.
- Serverantworten werden größenbegrenzt gelesen; Fehlermeldungen werden nicht blind
  als UI-Markup verwendet.
- Ein Keyring-Ausfall deaktiviert die Integration nicht dauerhaft, speichert das
  Token aber auch nicht unsicher. Der Nutzer kann nach Beheben erneut verbinden.
- Ein Dienstfehler beeinflusst weder Wiedergabestatus noch Queue-Auto-Advance noch
  lokale Play Counts.
- Tests setzen eigene temporäre Datenpfade und Fake-Tokens ein. Kein Test nutzt
  echte Musikdateien, das reale Reprise-DB-Profil oder das Produktionskonto.

## Teststrategie

- pure Schwellen-, Metadatenvalidierungs- und JSON-Snapshot-Tests;
- Migration, FIFO-Reihenfolge, Batchlöschung, Persistenz nach erneutem Öffnen und
  explizites Leeren der Offline-Warteschlange;
- Fake-Transport-Tests für Erfolg, Offline-Erhalt, 401-Stopp, Backoff und neue
  Arbeit als sofortigen Wake-up;
- lokaler TCP-Server für exakte URL-, Header- und Payload-Prüfung ohne Internet;
- Controller-Tests für genau ein Einreihen pro abgeschlossener Sitzung und kein
  Einreihen unter der Schwelle/bei deaktiviertem Modul;
- Preferences-/Status-Tests sowie ein ignorierter echter GTK-Displaytest;
- vollständige Projekt-Gates, Core-Reinheitsbeweis, gettext-, Flatpak-Quellen- und
  Dateigrößenprüfung;
- isolierter Headless-Smoke mit lokalem Fake-ListenBrainz-Endpunkt und privatem
  XDG-Daten-/Cachepfad, eigener D-Bus-Session, Xvfb und `fakesink`.

## Manuelle Abnahme

- Token im nativen GNOME-Keyring bzw. installierten Flatpak speichern, App neu
  starten und „verbunden als …“ prüfen;
- einen kurzen Titel bis zur Hälfte und einen langen Titel bis vier Minuten hören;
- Netzwerk trennen, Titel abschließen, neu starten, Netzwerk aktivieren und
  einmalige Nachlieferung im echten ListenBrainz-Profil prüfen;
- Plugin deaktivieren und bestätigen, dass weder `playing_now` noch Listens gesendet
  werden; danach Konto trennen und Keyring-Eintrag entfernen.

## Explizit nicht tun

Keine echten Tokens in Tests, Screenshots, Ledger oder Commits. Kein Fallback auf
Klartextdatei/SQLite. Keine neue Fremd-Plugin-ABI. Keine Musikdateioperation. Keine
Produktionsanfrage in automatisierten Gates.
