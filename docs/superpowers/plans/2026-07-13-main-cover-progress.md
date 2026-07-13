# Cover-Download-Fortschritt im Hauptfenster — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI und deutsche interne Dokumentation; keine realen
Nutzerdaten oder Netzabrufe in QA; alle App-Läufe mit privatem D-Bus, Xvfb,
XDG-Daten/XDG-Cache und `fakesink`; Core-Reinheit; jedes neue oder wesentlich
bearbeitete Sourcefile unter 800 Zeilen; vollständige Gates vor jedem Commit;
niemals pushen.

## Aufgabe 1 — Fortschritt an mehrere Anzeigen publizieren

Dateien: `crates/reprise-gnome/src/ui/cover_download_batch.rs`,
`preference_cover_download.rs`.

1. RED: Reine Tests beweisen, dass zwei Subscriber sofort denselben aktuellen
   Zustand und alle weiteren Updates erhalten und ein `false` zurückliefernder
   Subscriber aus der Liste entfernt wird.
2. Ersetze den einzelnen überschreibenden Callback durch eine re-entranzsichere
   Subscriberliste. `subscribe_progress` liefert den aktuellen Zustand sofort;
   Updates klonen Callbacks vor dem Aufruf und entfernen tote IDs danach.
3. Stelle die Preferences-Zeile auf einen schwachen Subscriber um, ohne ihr
   bestehendes persistentes Verhalten zu ändern.
4. Gezielte Tests, vollständige Gates und adversarial Review.

Commit: `refactor: broadcast cover download progress`.

## Aufgabe 2 — Native Hauptfensterzeile und Scan-Folge

Dateien: neues `crates/reprise-gnome/src/ui/main_cover_download_progress.rs`,
`ui/mod.rs`, `ui/scan_flow.rs`, minimale Composition-Root-Änderung in
`ui/window.rs`; vorhandene Strings werden wiederverwendet.

1. RED: Reine Tests für Idle/Running/Complete/Stopped/Failed-Präsentation und
   geklemmten Bruchteil; optionaler Displaytest prüft Revealer, Label und Balken.
2. Baue die kompakte Hauptfensterzeile. Running bleibt sichtbar; terminale
   Ergebnisse werden angezeigt und nach drei Sekunden generationengesichert
   ausgeblendet; Idle ist verborgen.
3. Eine Wiring-Funktion hängt die View an `ToolbarView`, registriert sie am
   bestehenden Batch und verbindet einen erfolgreichen Scanabschluss mit
   `start_if_enabled`. `window.rs` erhält nur den Composition-Aufruf.
4. Gezielte Tests, vollständig isolierter GTK-Test, vollständige Gates und
   adversarial Review.

Commit: `feat: show cover download progress in main window`.

## Aufgabe 3 — Abschluss

Isolierten Offline-App-Smoke mit kopierten Fixtures und lokalem Sidecar-Cover
ausführen; Running/Complete-Logs und saubere Anwendungslogs prüfen; vollständige
Gates, Audit, Core-Purity, gettext, Dateigrößen und Whole-Diff-Review ausführen;
Ledger, STATUS und MANUAL-QA aktualisieren und den Lock freigeben.

Commit: `docs: record main cover progress QA`.
