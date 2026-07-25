# Scrobbling-Provider in den Plugins vereinfachen

Stand: 2026-07-25

## Kurzantwort

Ja. ListenBrainz und Last.fm sollten in der Oberfläche als **ein Plugin
„Scrobbling“** erscheinen. Darin bleiben zwei unabhängig verbindbare Ziele:
ListenBrainz, Last.fm oder beide gleichzeitig.

Die gemeinsame Oberfläche sollte nicht versuchen, die Anmeldung zu
vereinheitlichen. ListenBrainz verwendet einen vom Benutzer kopierten Token;
Last.fm verwendet die Browser-Autorisierung einer registrierten Anwendung.
Gemeinsam sind dagegen Wiedergabeereignis, Scrobble-Schwelle, Statussprache und
Versandpipeline.

## Empfohlene Oberfläche

Eine einzige `Scrobbling`-Expander-Zeile auf der Plugins-Seite:

```text
Scrobbling
Send completed plays to your listening-history services

  ListenBrainz   Connected as alice                 [on]  >
  Last.fm        Not connected                 [Connect]

  Tracks count after 50% or 4 minutes.
```

- Kein äußerer dritter Ein-/Aus-Schalter. Die beiden Zielzeilen bleiben
  unabhängig; so sind ListenBrainz, Last.fm oder beide möglich.
- Im Normalzustand zeigt jede Zeile nur Anbieter, Kontostatus und Aktion.
- Der vorhandene Anbieter-Schalter bleibt nach erfolgreicher Verbindung die
  nicht-destruktive Pause. `Disconnect` bleibt in der aufgeklappten
  Kontoverwaltung und entfernt die Verbindung.
- Zugangsfelder erscheinen nur während Einrichtung oder Kontoverwaltung, nicht
  dauerhaft in der Plugin-Liste.
- „Now Playing“, Batchgrößen, Wiederholungen und Warteschlangen sind
  Transportdetails und keine Benutzeroptionen.

Damit wird aus zwei großen, technisch geprägten Plugin-Expandern ein fachlicher
Bereich mit zwei kompakten Zielen. Die Formulierung „Scrobbling“ erklärt auch,
warum diese Anbieter zusammengehören, ohne vorzugeben, sie seien dasselbe
Konto.

## Was die APIs gemeinsam haben

Beide Dienste erhalten Künstler, Titel und den UTC-Zeitpunkt des
Wiedergabestarts. Album, Dauer, MusicBrainz-IDs und weitere Felder sind
optionale Anreicherungen
([ListenBrainz JSON](https://listenbrainz.readthedocs.io/en/latest/users/json.html#payload-json-details),
[Last.fm `track.scrobble`](https://www.last.fm/api/show/track.scrobble)).

Beide verwenden grundsätzlich dieselbe Scrobble-Schwelle: mindestens die
Hälfte des Titels oder vier Minuten, je nachdem, was früher erreicht wird
([ListenBrainz Submit API](https://listenbrainz.readthedocs.io/en/latest/users/api/core.html#post-1-submit-listens),
[Last.fm Scrobbling](https://www.last.fm/api/scrobbling)). Last.fm fordert
zusätzlich, dass der Titel **länger als 30 Sekunden** ist
([Last.fm Scrobbling](https://www.last.fm/api/scrobbling)).

Beide kennen eine optionale „Now Playing“-Meldung am Titelanfang und eine
dauerhafte Meldung nach Erreichen der Schwelle. ListenBrainz speichert
`playing_now` nur temporär und erwartet später nochmals `single` oder `import`;
bei Last.fm beeinflusst „Now Playing“ die Charts nicht
([ListenBrainz Submission JSON](https://listenbrainz.readthedocs.io/en/latest/users/json.html#submission-json),
[Last.fm Scrobbling](https://www.last.fm/api/scrobbling)). Das rechtfertigt
einen gemeinsamen Playback-Lebenszyklus, aber keinen sichtbaren Schalter für
„Now Playing“.

## Wo die Provider getrennt bleiben müssen

### ListenBrainz

Der Benutzer kopiert seinen persönlichen Token aus den
ListenBrainz-Einstellungen. Reprise sendet ihn als
`Authorization: Token …`; `/1/validate-token` bestätigt Token und Benutzername
([ListenBrainz Submit API](https://listenbrainz.readthedocs.io/en/latest/users/api/core.html#post-1-submit-listens),
[ListenBrainz Token Validation](https://listenbrainz.readthedocs.io/en/latest/users/api/core.html#get-1-validate-token)).
Die knappe Einrichtung lautet daher:

1. `Get token` öffnet die ListenBrainz-Einstellungen.
2. Ein maskiertes Token-Feld und `Connect`.
3. Danach nur noch `Connected as …`, Pause, Test und Disconnect.

### Last.fm

Last.fm-Desktop-Anmeldung beginnt mit API-Key und Shared Secret des
**Anwendungs-/API-Kontos**. Die Anwendung holt einen höchstens 60 Minuten
gültigen Request-Token, öffnet die Freigabe im Browser und tauscht den einmal
verwendbaren Token gegen einen Session-Key. Session-Keys gelten standardmäßig
unbegrenzt, sollen sicher gespeichert werden und können vom Benutzer widerrufen
werden
([Last.fm Desktop Authentication](https://www.last.fm/api/desktopauth)).

Für eine veröffentlichte Reprise-Version mit registrierten
Anwendungszugangsdaten sollte die normale Oberfläche deshalb nur
`Connect Last.fm` zeigen. API-Key und Shared Secret gehören höchstens in einen
klar bezeichneten erweiterten BYO-Abschnitt. Ohne registrierte
Reprise-Anwendung bleibt BYO technisch notwendig; dann kann Last.fm nicht
ehrlich als Ein-Klick-Verbindung angeboten werden.

Vor einer öffentlichen Last.fm-Integration ist außerdem der API-Account samt
Nutzungsrahmen zu klären: Die veröffentlichten Bedingungen beschränken die
Standardgenehmigung auf nicht-kommerzielle Nutzung und verlangen für
kommerzielle oder Forschungsnutzung vorherige Kontaktaufnahme
([Last.fm API Terms](https://www.last.fm/api/tos)).

### Versand und Fehler

Die Zielzustände müssen unabhängig bleiben, weil ein Dienst offline oder
abgemeldet sein kann, während der andere erfolgreich sendet. Auch die
Batchgrenzen unterscheiden sich: Last.fm erlaubt höchstens 50 Scrobbles pro
Request und empfiehlt eine geordnete, Neustarts überlebende Retry-Ablage
([Last.fm Scrobbling](https://www.last.fm/api/scrobbling)); ListenBrainz
erlaubt für `import` mehrere gespeicherte Listens und derzeit höchstens 1000 pro
Request, zusätzlich zu Byte-Limits
([ListenBrainz Submission JSON](https://listenbrainz.readthedocs.io/en/latest/users/json.html#submission-json),
[ListenBrainz Constants](https://listenbrainz.readthedocs.io/en/latest/users/api/core.html#constants)).

Die UI darf den Status unter „Scrobbling“ zusammenfassen, muss Fehler und
Warteschlangen aber pro Provider anzeigen.

## Abgleich mit dem aktuellen Reprise-Code

Die fachliche Zusammenführung ist bereits weitgehend vorhanden:

- [`play_tracking.rs`](../../crates/reprise-gnome/src/ui/playback/play_tracking.rs)
  beginnt genau eine Scrobble-Session, meldet „Playing Now“ an beide aktiven
  Laufzeiten und verteilt den fertigen Listen-Eintrag an alle aktiven Ziele.
- [`scrobbling.rs`](../../crates/reprise-core/src/scrobbling.rs) definiert mit
  `ScrobblerTransport` bereits einen gemeinsamen Vertrag für Validierung,
  „Playing Now“ und Submit.
- [`queue.rs`](../../crates/reprise-core/src/scrobbling/queue.rs) hält
  provider-spezifische, dauerhafte Queues und die unterschiedlichen
  Batchgrenzen getrennt.
- [`preference_plugins.rs`](../../crates/reprise-gnome/src/ui/preferences/preference_plugins.rs)
  macht nur in der Darstellung zwei Sonderfälle daraus; die Zusammenführung
  kann deshalb zunächst eine UI-Komposition bleiben, ohne Credentials oder
  Queue-Daten zu migrieren.
- [`preference_listenbrainz.rs`](../../crates/reprise-gnome/src/ui/preferences/preference_listenbrainz.rs)
  und
  [`preference_lastfm.rs`](../../crates/reprise-gnome/src/ui/preferences/preference_lastfm.rs)
  können ihre provider-spezifischen Verbindungsabläufe behalten.

Ein API-relevanter Korrekturpunkt ist vor der oder zusammen mit der UI-Arbeit
nötig: `should_scrobble` verwendet derzeit für beide Anbieter nur „Hälfte oder
vier Minuten“. Für Last.fm muss zusätzlich `duration > 30 s` gelten. Das sollte
provider-spezifisch modelliert werden, statt ListenBrainz ohne API-Grund
ebenfalls kurzes Audio zu verweigern.

## Entscheidung

**Kombinieren:** Plugin-Zeile, Begriff, Erklärung, Playback-Lebenszyklus und
kompakte Statusdarstellung.

**Getrennt lassen:** Konten, Aktivierung/Pause, Credentials,
Verbindungsfehler, dauerhafter Lieferstatus und Provider-Adapter.

Ein Provider-Dropdown oder ein „universelles“ Zugangsdatenformular wäre
irreführend. Ein gemeinsamer Bereich mit zwei unabhängigen Konten ist dagegen
API-treu und deutlich ruhiger als die aktuelle Darstellung.
