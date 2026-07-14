# Sidebar-Problemquellen aufräumen — Designspezifikation

## Ziel

Die nur bei Bedarf sichtbaren Quellen **Import errors** und **Missing files**
lassen sich direkt in der Sidebar vollständig aufräumen. Dadurch verschwindet
der gesamte Abschnitt **ISSUES**, sobald keine offenen Einträge mehr vorhanden
sind, ohne zusätzliche dauerhafte Buttons oder Zeilen in der Navigation.

Die bestehende volle Fortschrittszeile für einen laufenden Cover-Scan bleibt
unverändert. Sie ist ein temporärer Status einer aktiven Arbeit und gehört
nicht zu dieser Bereinigung.

## Interaktion

- Ein Rechtsklick auf **Import errors** zeigt genau **Dismiss all import
  errors**. Die Aktion entfernt nur gespeicherte Fehlerdiagnosen und kann
  deshalb unmittelbar ausgeführt werden.
- Ein Rechtsklick auf **Missing files** zeigt genau **Remove all missing
  entries…**. Vor der dauerhaften Datenbankänderung bestätigt ein nativer
  destruktiver `AdwAlertDialog`, dass Mediendateien niemals gelöscht werden.
- Nach Erfolg baut sich die Sidebar neu auf. Eine nun leere Problemquelle
  verschwindet; war sie ausgewählt, greift der bestehende Rückfall auf
  **Music**.
- Ein kurzer Toast nennt die Zahl der bereinigten Einträge. Fehler bleiben
  sichtbar als Toast und werden geloggt.

## Daten- und Wiedergabesicherheit

Die Bulk-Aktion für fehlende Titel benutzt denselben transaktionalen,
datenbankexklusiven Pfad wie die vorhandene Einzelauswahl. Sie löscht nur
Zeilen mit `missing = 1`, kompaktiert betroffene Playlist-Positionen und gibt
die tatsächlich entfernten IDs zurück. Die GTK-Komposition entfernt genau
diese IDs zusätzlich aus Wiedergabekontext und Up-next-Queue. Es finden keine
Dateisystemoperationen statt.

Das Verwerfen von Importfehlern löscht ausschließlich Zeilen aus
`import_errors`; Bibliothekseinträge und Dateien bleiben unangetastet.

## Architektur

- `reprise-core::queries` stellt zwei kleine Bulk-Verträge bereit:
  `delete_all_import_errors` und `remove_all_missing_tracks`.
- Ein neues fokussiertes GNOME-Modul besitzt Kontextmenü, Bestätigung,
  Mutation, Toast und Sidebar-Neuaufbau.
- Ein eigener String-Baustein hält die bereits große zentrale Stringdatei
  unter der 800-Zeilen-Grenze.
- `Sidebar` bietet nur einen Callback für tatsächlich entfernte Missing-IDs;
  `window.rs` verbindet ihn mit dem bestehenden Queue-Purge-Pfad.

## Verifikation

- Core-Regressionen beweisen Tabellenisolation, Live-Row-Schutz,
  transaktionale Missing-Bereinigung und lückenlose Playlist-Positionen.
- Ein reiner UI-Test fixiert Quellen-zu-Aktionen und die erforderliche
  Bestätigung.
- Ein vollständig isolierter GTK-Test prüft sekundäre Klickgesten,
  verschwundene leere Problemzeilen, exakte entfernte IDs und den Rückfall
  auf **Music**.
- Vollständige Projekt-Gates, Core-Purity, gettext und Dateigrößenprüfung.

## Nicht enthalten

- keine Änderung an Cover-Scan- oder Library-Scan-Status;
- keine automatische Löschung realer Musikdateien;
- keine neue dauerhafte Sidebar-Aktion oder zusätzliche Statuszeile;
- keine Änderung der vorhandenen Einzelaktionen in den Problemansichten.
