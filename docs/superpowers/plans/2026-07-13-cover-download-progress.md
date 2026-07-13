# Sichtbarer Cover-Download — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI und deutsche interne Dokumentation; keine realen
Nutzerdaten oder Netzabrufe in QA; Core-Reinheit; jedes bearbeitete Sourcefile unter
800 Zeilen; vollständige Gates vor jedem Commit; niemals pushen.

## Aufgabe 1 — Typisierte Worker-Ergebnisse

`cover_download_worker.rs` erhält `DownloadOutcome` und eine Request-Option zum
Überspringen bereits vorhandener Cover. Zuerst Tests für lokale Cover, fehlende
Tags und Album-Deduplizierung rot sehen; dann den vorhandenen seriellen Worker
minimal erweitern. `cover_loader.rs` konsumiert den neuen Typ ohne Verhaltensänderung.

Commit: `refactor: report cover download outcomes`.

## Aufgabe 2 — Batchzustand und Orchestrierung

Neues `cover_download_batch.rs`: reine `BatchProgress`-Übergangstests sowie DB-Test
für aktive Pfade zuerst rot. Danach Controller mit Generation, Start/Stop,
sequentieller Worker-Nutzung, Fortschrittscallback und Cover-Refresh implementieren.
Der Controller darf keinen Netzwerkcode duplizieren.

Commit: `feat: download missing covers with progress`.

## Aufgabe 3 — Native Preferences-Fortschrittsanzeige

Neues `preference_cover_download.rs`: GTK-Test für Statuszeile und Progressbar rot,
dann dynamische Zeile unter dem Cover-Schalter bauen. `primary_menu.rs` benachrichtigt
den Batch-Controller bei beiden Schalteroberflächen; `window.rs` enthält nur die
Composition-Root-Verdrahtung und bleibt unter 800 Zeilen. Englische und deutsche
Texte vollständig aktualisieren.

Commit: `feat: show cover download progress`.

## Aufgabe 4 — Abschluss

Isolierten Offline-Smoke mit privatem D-Bus/Xvfb/XDG und fakesink ausführen,
Screenshots und kritische Logs prüfen, vollständigen Release-Checker ausführen,
adversarial reviewen, MANUAL-QA/Ledger/STATUS aktualisieren und Lock freigeben.

Commit: `docs: record cover download progress QA`.
