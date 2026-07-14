# Nativer About-Dialog — Design

## Ziel

Reprise bietet im Hauptmenü einen nativen libadwaita-About-Dialog. Er nennt
Marvin Baudach als Entwickler und Copyright-Inhaber, zeigt die laufende
Programmversion und macht die Lizenzaufteilung des ausgelieferten Linux-Clients
sichtbar.

## Verhalten

- Der Eintrag `About` steht am Ende des bestehenden Hauptmenüs.
- Aktivierung öffnet einen `AdwAboutDialog` relativ zum Hauptfenster.
- Der Dialog zeigt App-Name, App-Icon, Cargo-Paketversion, Entwickler und
  Copyright.
- Die primäre Lizenz ist `GPL-3.0-or-later`, passend zum ausgelieferten
  `reprise`-Binary und zur GNOME-Frontend-Crate.
- Eine zusätzliche Rechtssektion nennt Reprise Engine und Linux Platform als
  MIT-lizenzierte Komponenten, wie in `LICENSING.md` gefordert.
- Der Menütext und die zusätzliche Rechtssektion sind vollständig in Deutsch
  übersetzt.

## Architektur

Ein kleines UI-Modul `about.rs` besitzt ausschließlich die unveränderlichen
Metadaten sowie Aufbau und Präsentation des Dialogs. `primary_menu.rs` fügt die
Fensteraktion `win.about` hinzu und hält nur eine schwache Referenz auf das
Hauptfenster. Core und Persistenz bleiben unberührt.

## Tests und QA

- Ein reiner Menüvertrag beweist, dass `win.about` dauerhaft angeboten wird.
- Ein isolierter Displaytest prüft App-Name, Version, Entwickler, Copyright und
  GPL-Lizenz des tatsächlich gebauten Dialogs.
- Gettext-Vollständigkeit, Workspace-Gates, Release-Checker, Core-Purity und
  Dateigrößen bleiben grün.
- Die tatsächliche Darstellung und Navigation zur Lizenzseite bleiben ein
  manueller GNOME-Check.

## Explizit nicht Teil

- Keine neue Einstellungsseite und keine persistente Option.
- Keine Website-, Support- oder Issue-Links ohne bestätigte öffentliche Ziele.
- Keine Änderung der bestehenden Lizenzaufteilung.
