# Eingebettete Rhythmbox-Importnavigation — Design

## Ziel

Die Auswahl des dauerhaften Rhythmbox-Imports soll nicht als Dialog über dem
Einstellungsfenster erscheinen. Einstellungen → Bibliothek öffnet stattdessen
eine zweite Navigationsebene im bereits vorhandenen Preferences-Fenster. Die
native Zurück-Navigation führt unverändert zur Bibliotheksseite zurück.

## Verhalten

- Die nur bei gefundener `rhythmdb.xml` sichtbare Rhythmbox-Zeile wird zu einer
  aktivierbaren Navigationszeile mit Vorwärtspfeil.
- Aktivieren der Zeile pusht genau eine Detailseite `Import from Rhythmbox` in
  den vorhandenen `AdwNavigationView`; kein zusätzliches Auswahlfenster entsteht.
- Die Detailseite zeigt die bestehende read-only-Erklärung und alle sechs
  Optionen mit unveränderten Defaults: Column layout aus; Ratings, Play counts,
  Date added, Last played und Playlists an.
- Eine hervorgehobene Header-Aktion `Import` startet erst nach expliziter
  Betätigung den unveränderten Hintergrund-/Mergepfad und ist währenddessen
  deaktiviert.
- Erfolg, Teilerfolg oder Fehler bleiben kurze Adwaita-Ergebnisdialoge über dem
  Preferences-Fenster. Sie sind keine weitere Konfigurationsebene.
- Zurück verwirft lediglich noch nicht gestartete Schalterauswahlen. Bereits
  abgeschlossene Importe bleiben wie bisher konservativ und wiederholbar.

## Architektur

`preference_rhythmbox.rs` ersetzt den `AdwAlertDialog` für die Auswahl durch
einen Builder für `AdwNavigationPage`. Die Seite enthält `AdwToolbarView`,
`AdwHeaderBar`, `AdwPreferencesPage`, die vorhandenen `AdwSwitchRow`s und die
Import-Schaltfläche.

`PreferencesContext` verwendet den bereits für den Spalteneditor gespeicherten
schwachen `AdwNavigationView`. Die Rhythmbox-Zeile ruft einen kleinen
`open_rhythmbox_import`-Pfad auf, der die Detailseite baut, verdrahtet und pusht.
Der bestehende `start_rhythmbox_import`-Pfad bleibt für Parsing, SQLite-Merge,
Spaltenlayout, Reload, Sidebar-Aktualisierung und Smoke-Tests zuständig.

## Fehlerbehandlung

- Fehlt die Preferences-Navigation wider Erwarten, wird gewarnt und kein
  separates Ersatzfenster geöffnet.
- Ein während des Imports verschwindender `rhythmdb.xml`-Pfad verwendet weiter
  den bestehenden verständlichen Fehlerpfad und verändert keine Daten teilweise.
- Das Schließen oder Zurücknavigieren vor `Import` löst keinerlei Import aus.

## Tests und QA

- Ein isolierter GTK-Test verlangt eine aktivierbare Navigationszeile mit
  Vorwärtspfeil statt Import-Schaltfläche.
- Ein isolierter GTK-Test verlangt eine poppbare `AdwNavigationPage` mit sechs
  Optionen, unveränderten Defaults und sichtbarer `Import`-Headeraktion.
- Ein Preferences-Navigationstest pusht die Rhythmbox-Seite in dasselbe Fenster
  und beweist die Zurück-Navigation.
- Der bestehende Scratch-Rhythmbox-Smoke beweist weiterhin den tatsächlichen
  Import ohne echte Rhythmbox-, Musik- oder Reprise-Daten.
- Vollständige fmt-, Clippy-, Workspace-Test-, Audit-, Core-Purity-, gettext-
  und Dateigrößen-Gates bleiben verpflichtend. Die tatsächliche Darstellung und
  Pointer-Navigation unter GNOME bleiben als manueller Check dokumentiert.

## Explizit nicht Teil

- Keine Änderung an Parsern, Konfliktregeln, Datenbanktransaktionen oder
  importierten Datentypen.
- Kein automatischer Import und kein Schreiben nach Rhythmbox oder Audiodateien.
- Kein erneuter First-run-Import und keine Änderung anderer Preferences-Seiten.
- Keine Umwandlung der kurzen Ergebnis-/Fehlerbestätigung in eine dauerhafte
  dritte Navigationsebene.
