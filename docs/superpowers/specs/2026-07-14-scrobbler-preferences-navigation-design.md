# Eingebettete Scrobbler-Einrichtung — Design

## Ziel

ListenBrainz und Last.fm sollen beim Aktivieren kein separates Dialogfenster
unterhalb der Einstellungen öffnen. Die Kontoeinrichtung erscheint jeweils als
zweite Navigationsebene im vorhandenen Preferences-Fenster. Gleichzeitig bleibt
der vom Nutzer eingeschaltete Schalter während Prüfung und Einrichtung stabil,
statt sichtbar von Aus zu An und sofort wieder zu Aus zu springen.

## Verhalten

- Einschalten eines noch nicht konfigurierten Dienstes hält den Schalter sichtbar
  auf An und deaktiviert die Zeile nur während der kurzen Keyring-Prüfung.
- Fehlen gültige Zugangsdaten, pusht Reprise eine native Detailseite in denselben
  `AdwNavigationView`: ListenBrainz mit verdecktem Tokenfeld, Last.fm mit verdecktem
  API-Key und Shared Secret.
- Die Detailseite besitzt die native Zurück-Navigation und eine hervorgehobene
  Headeraktion (`Connect` beziehungsweise `Open Browser`). Die Aktion bleibt bis
  zu vollständig ausgefüllten Pflichtfeldern deaktiviert.
- Zurück oder Schließen der Einstellungen ohne erfolgreiche Verbindung setzt den
  weiterhin nur angeforderten Schalter auf Aus zurück. Das Modul bleibt während
  der Einrichtung unpersistiert und sendet keine Scrobbles.
- Erfolgreich gespeicherte Zugangsdaten aktivieren und persistieren das Modul wie
  bisher. Eine bereits aktive Verbindung bleibt beim Öffnen und Zurücknavigieren
  stabil an.
- Die vorhandene Configure-Aktion öffnet dieselbe Detailseite. Aktive Dienste
  zeigen dort weiterhin eine destruktive Disconnect-Aktion.
- Last.fm öffnet den externen Browser erst nach `Open Browser`. Die anschließende
  Bestätigung sowie Fehlerhinweise bleiben kurze Alerts, werden aber über dem
  Preferences-Fenster statt über dem Hauptfenster präsentiert.

## Architektur

`preference_listenbrainz.rs` und `preference_lastfm.rs` ersetzen ihre initialen
`AdwAlertDialog`-Builder durch je einen `NavigationPage`-Surface-Builder aus
`AdwToolbarView`, `AdwHeaderBar` und `AdwPreferencesPage`. Die bestehenden
Validierungs-, Netzwerk-, Keyring-, Persistenz-, Runtime- und Disconnect-Pfade
bleiben unverändert.

`PreferencesContext` stellt den bereits gespeicherten schwachen
`AdwNavigationView` als sichere Upgrade-Methode bereit. Ein gemeinsamer kleiner
Preferences-Helfer hält SwitchRows während asynchroner Aktivierungsprüfung aktiv
und vorübergehend insensitive. Der `hiding`-Callback der Detailseite stellt ohne
aktive Runtime den wahrheitsgemäßen Aus-Zustand wieder her.

Kurze Alerts verwenden einen gemeinsamen Preferences-Parent mit Fallback auf das
Hauptfenster, falls die Einstellungen während einer asynchronen Operation bereits
geschlossen wurden.

## Fehler- und Sicherheitsverhalten

- Ein Keyring-, Netzwerk- oder Workerfehler beendet den Pending-Zustand, setzt den
  nicht aktivierten Schalter auf Aus und zeigt den bestehenden übersetzten Fehler.
- Keine Credentials werden in Datenbank, Logs, Environment, Screenshots oder
  Tests geschrieben. Tests verwenden ausschließlich leere/maskierte Widgets und
  bestehende Fake-Transporte.
- Zurück vor einer expliziten Connect-/Browser-Aktion löst keinen Netzwerkzugriff
  und keine Persistenz aus.
- ListenBrainz- und Last.fm-Queues, Tokens und Runtimes bleiben strikt getrennt.

## Tests und QA

- Ein GTK-Helfertest verlangt, dass ein angeforderter Switch während Pending an
  bleibt, insensitive wird und danach wieder bedienbar ist.
- Je ein isolierter GTK-Test verlangt eine poppbare zweite Navigationsebene,
  verdeckte Pflichtfelder und korrekt gegatete Headeraktion.
- Bestehende Policy-, Status-, Fake-Transport-, Keyring- und Scrobble-Tests bleiben
  grün; vollständige fmt-, Clippy-, Workspace-, Audit-, Core-Purity-, gettext- und
  Dateigrößen-Gates sind verpflichtend.
- Reales Keyring-, Browser- und GNOME-Pointerverhalten bleibt manueller QA mit
  Wegwerfkonten; niemals echte Secrets in Agenten- oder Testbefehle übernehmen.

## Explizit nicht Teil

- Keine Änderung an Scrobble-Schwellen, Payloads, Warteschlangen oder Retryregeln.
- Kein automatisches Aktivieren ohne erfolgreich gespeicherte Credentials.
- Kein eingebetteter Browser und keine Speicherung von Last.fm-App-Secrets durch
  Reprise außerhalb des bestehenden System-Keyrings.
- Keine Umgestaltung anderer Pluginzeilen oder ihrer Modulsemantik.
