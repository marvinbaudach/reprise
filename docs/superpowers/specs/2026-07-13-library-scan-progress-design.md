# Sichtbarer Bibliotheks-Scanfortschritt — Design

## Ziel

Ein erster Bibliotheksaufbau oder manueller Rescan darf nicht wie ein hängender
Button wirken. Reprise zeigt während der gesamten Arbeit sichtbar, in welcher
Phase der Scan ist, wie viele Audiodateien bereits bearbeitet wurden und wann der
Lauf beendet ist.

## Verhalten

- Nach der Ordnerauswahl erscheint direkt unter der Headerbar eine schmale native
  Fortschrittszeile.
- Phase 1 lautet „Musikdateien werden ermittelt …“. Der Balken pulsiert, weil die
  Gesamtzahl während der Verzeichnisermittlung noch unbekannt ist.
- Phase 2 zeigt einen bestimmten Balken mit „{processed} von {total} Dateien
  gescannt“ und den gekürzten Namen der aktuell bearbeiteten Datei.
- Der Balken verschwindet nach Erfolg oder Fehler. Das bestehende Fehler-Toast,
  die Button-Sperre, der Tracklisten-Reload und der Watcher-Neustart bleiben
  unverändert.
- Das Verhalten gilt für den Wizard „Bibliothek einrichten“, den Header-Button,
  „Bibliothek neu scannen“ und den vorhandenen Post-Launch-Smoke, weil alle vier
  bereits denselben `spawn_scan`-Pfad verwenden.
- Der reine `REPRISE_SCAN_DIR`-Entwicklerhook vor dem Fenster zeigt bewusst keine
  UI; er existiert nur für isolierte Fixtures und ist kein Nutzerpfad.

## Architektur

`reprise-core::library::scanner` erhält `ScanProgress` mit den Phasen
`Discovering` und `Scanning { processed, total, current_path }` sowie
`scan_folder_with_progress`. Das bestehende `scan_folder` bleibt als kompatibler
No-op-Callback-Wrapper für Watcher, Tag-Reconciliation und Tests bestehen.

Die Ermittlung läuft in einem ersten, read-only `WalkDir`-Durchlauf mit konstantem
Speicher. Der zweite Durchlauf behält die bestehende transaktionale Scanlogik und
meldet nach jeder berücksichtigten Audiodatei einen Fortschrittswert. Dateien, die
zwischen beiden Durchläufen erscheinen, dürfen den gemeldeten Nenner nach oben
erweitern; Bruchteile werden immer auf `0..=1` begrenzt. Verzeichnisfehler werden
wie bisher ausschließlich im eigentlichen Scanlauf in `import_errors` erfasst.

`ui/scan_progress.rs` besitzt ausschließlich die GTK-Darstellung und eine
Generation für den Puls-Timer. `scan_flow.rs` überträgt Worker-Fortschritt über
einen kapazitätsbegrenzten Kanal mit `try_send`: die UI darf Zwischenstände
zusammenfassen, der Worker wird weder gebremst noch entsteht eine unbeschränkte
Eventqueue. Das Endergebnis bleibt auf dem bestehenden separaten One-shot-Kanal.

## Sicherheit und Skalierung

Es gibt keinen Schreibzugriff auf Musikdateien. Beide Traversierungen folgen
keinen Symlinks. Der erste Durchlauf speichert keine Pfadliste und bleibt damit
auch für sehr große Bibliotheken speicherkonstant. Tag-/DB-Arbeit läuft weiterhin
im Worker, nie auf dem GTK-Main-Thread. Kein `RefCell`- oder SQLite-Borrow wird über
einen GTK-Callback gehalten.

Eine Restzeit wird nicht erfunden: Dateigröße, Netzwerk-Mount-Latenz und Tagformat
machen eine frühe ETA unzuverlässig. Der bestimmte Dateizähler beantwortet
stattdessen nachvollziehbar, wie weit der Lauf ist.

## Tests

- Core-Tests beweisen die exakte Phasenfolge, monotone Zähler, Audiofilterung und
  unveränderten `ScanReport`.
- Reine UI-Zustandstests prüfen Text und Bruch einschließlich leerer Bibliothek.
- Ein isolierter Xvfb/D-Bus/XDG-Smoke scannt mehrere Fixture-Dateien über
  `REPRISE_SMOKE_RESCAN`, protokolliert Discovering/Scanning/Complete und lehnt
  GTK-/GLib-Criticals, Panics und `RefCell`-Fehler ab.
- Vollständige Gates, Core-Purity und Dateigrößenprüfung bleiben verpflichtend.

## Explizit nicht Teil

Keine künstliche ETA, kein Abbrechen/Pausieren, kein paralleler Tagscan, keine
Fortschrittsanzeige für einzelne Watcher-Ereignisse und kein Speichern sämtlicher
Dateipfade im RAM.
