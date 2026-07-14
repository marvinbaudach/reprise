# Native Offline-Hilfe — Design

## Ziel

Reprise erhält einen dauerhaft erreichbaren `Help`-Eintrag im Hauptmenü. Da
noch keine bestätigte öffentliche Dokumentations- oder Support-URL existiert,
öffnet er eine lokale native libadwaita-Hilfe mit den tatsächlich verfügbaren
Tastaturbefehlen.

## Verhalten

- `Help` steht unmittelbar vor `About` im Hauptmenü.
- Aktivierung öffnet einen `AdwShortcutsDialog` relativ zum Hauptfenster.
- F1 öffnet dieselbe Hilfe auch aus Compact View.
- Die Hilfe nennt nur implementierte Befehle: Wiedergabe/Pause, Suche,
  Library/Compact-Umschaltung, Suche leeren beziehungsweise zur Titelliste
  zurückkehren, ausgewählten Titel abspielen, Kontextmenü und Hilfe selbst.
- Titel, Abschnitte und Beschreibungen sind vollständig ins Deutsche übersetzt.

## Architektur

Ein fokussiertes Modul `help.rs` hält unveränderliche Shortcut-Spezifikationen,
baut daraus `AdwShortcutsSection` und `AdwShortcutsItem` und präsentiert einen
`AdwShortcutsDialog`. `primary_menu.rs` installiert die Fensteraktion
`win.help`; `window.rs` bindet F1 an diese bestehende Aktion. Es gibt weder
Persistenz noch Core- oder Netzwerkzugriff.

## Tests und QA

- Der reine Menüvertrag verlangt `win.help` direkt vor `win.about`.
- Ein reiner Shortcut-Vertrag verlangt ausschließlich die bekannten
  Accelerator-Tokens.
- Ein isolierter Displaytest prüft Titel, Abschnitte und sichtbare Items des
  gebauten nativen Dialogs.
- Workspace-Gates, gettext, Release-Checker, Core-Purity und Dateigrößen bleiben
  grün.
- Tatsächliche Darstellung, F1-Ereignisrouting und Tastaturnavigation bleiben
  ein manueller GNOME-Check.

## Explizit nicht Teil

- Keine erfundene Website-, Support- oder Dokumentations-URL.
- Kein Online-Handbuch, WebView oder Netzwerkzugriff.
- Keine neuen Wiedergabe- oder Navigationsbefehle.
