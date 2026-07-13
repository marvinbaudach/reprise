# Plugin-Grenzen — Implementierungsplan

## Globale Randbedingungen

TDD RED→GREEN; englischer Code/UI, deutsche interne Dokumentation; keine realen
Nutzerdaten; Core-Reinheit; jede bearbeitete Datei unter 800 Zeilen; vollständige
Gates vor dem Commit; niemals pushen.

## Aufgabe 1 — Registry und Preferences korrigieren

Tests zuerst so ändern, dass `ALL_MODULES` keine Wiedergabe-Kernfunktionen enthält
und nur der Online-Coverabruf als live umschaltbares Plugin gilt. Die Tests rot
sehen. Danach Equalizer-/ReplayGain-Deskriptoren sowie ihre doppelten Plugin-Zeilen
und Synchronisationslogik entfernen. Die Playback-Seite und die Audio-Pipeline
bleiben unverändert. Veraltete Übersetzungsquellen entfernen.

Commit: `fix: keep core playback features out of plugins`.

## Aufgabe 2 — Architektur und QA nachführen

Master-Design, aktuelle Preferences-Spezifikation, Release-Anleitung und manuelle
QA auf die neue Grenze korrigieren. MTP/iPod fest unter Synchronisation verorten
und den späteren Plugin-Backlog dokumentieren. Vollständige Gates und Core-Purity
ausführen, Ledger/STATUS aktualisieren und Lock freigeben.

Commit: `docs: define plugin and core feature boundaries`.
