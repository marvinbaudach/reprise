# Sichtbarer Bibliotheks-Scanfortschritt — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI und deutsche interne Dokumentation; niemals reale
Musik oder die echte Datenbank für QA verwenden; alle App-Läufe vollständig mit
privatem D-Bus, Xvfb, XDG-Daten/XDG-Cache und `fakesink` isolieren; Core-Reinheit;
jede neue oder wesentlich bearbeitete Datei unter 800 Zeilen; vollständige Gates
vor jedem Commit; niemals pushen.

## Aufgabe 1 — Fortschrittsvertrag im Core

Dateien: `crates/reprise-core/src/library/scanner.rs`, neue
`scanner_progress_tests.rs` (die bestehende legacy `scanner_tests.rs` bleibt
unangetastet und wächst nicht weiter).

1. RED: Test mit zwei Audiofiles plus Nicht-Audiodatei, der zuerst `Discovering`,
   dann exakt zwei monotone `Scanning`-Ereignisse mit `total = 2` verlangt und
   weiterhin den unveränderten `ScanReport` prüft.
2. Implementiere `ScanProgress` und `scan_folder_with_progress`; `scan_folder`
   delegiert mit No-op-Callback. Nutze einen speicherkonstanten Zähldurchlauf und
   behalte die bestehende transaktionale Scanlogik im zweiten Durchlauf.
3. Gezielte Tests, vollständige Gates, Core-Purity und adversarial Review.

Commit: `feat: report library scan progress`.

## Aufgabe 2 — Native Fortschrittszeile und Verdrahtung

Dateien: neues `crates/reprise-gnome/src/ui/scan_progress.rs`,
`ui/scan_flow.rs`, minimale Composition-Root-Erweiterung in `ui/window.rs`,
`ui/mod.rs`, `ui/strings.rs`, `po/reprise.pot`, `po/de.po`.

1. RED: reine Tests für Discovering/Scanning-View-State, geklemmte Bruchteile und
   leere Bibliothek; optionaler Displaytest prüft Revealer, Label und ProgressBar.
2. Baue `ScanProgressView`: pulsierender unbekannter Zustand, bestimmter Zustand
   mit `processed / total`, ellipsierter Dateiname, Hide/Timer-Abbruch bei Ende.
3. `spawn_scan` startet die Anzeige sofort. Der Worker nutzt
   `scan_folder_with_progress` und einen `bounded(1)`-Fortschrittskanal mit
   `try_send`; eine lokale Drain-Future aktualisiert nur GTK. Der bestehende
   Resultatkanal und alle Erfolgs-/Fehlerpfade bleiben verantwortlich für Finish.
4. Übergib dieselbe View an Header-Scan, Rescan und Smoke. Der Setup-Wizard nutzt
   sie automatisch über den bestehenden Button-Click.
5. Aktualisiere englische/deutsche gettext-Kataloge vollständig.

Commit: `feat: show library scan progress`.

## Aufgabe 3 — Abschluss

Isolierten Mehrdatei-Smoke ausführen und exakte Phasenlogs sowie saubere App-Logs
prüfen; vollständige Gates, `cargo audit`, Core-Purity, Dateigrößen und
adversarial Whole-Diff-Review ausführen. Ledger/STATUS/MANUAL-QA nur soweit nötig
aktualisieren und Lock freigeben.

Commit: `docs: record library scan progress QA`.
