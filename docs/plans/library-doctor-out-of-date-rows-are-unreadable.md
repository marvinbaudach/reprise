---
slug: library-doctor-out-of-date-rows-are-unreadable
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: „out of date" / „Stale" im Library Doctor versteht niemand

**Nur ein Befund, kein Plan.** Festgehalten am 16.08.2026, 08:05, gemeldet vom
Nutzer („das mit dem outdated versteht keiner … können wir das nicht
rausnehmen, wenn man es nicht fixen kann"), belegt durch einen Screenshot des
Library Doctor (laufender Build: 0.1.13, gebaut 15.08.2026 23:00, entspricht dem `dev`-Kopf `95b4b30016`).

## Symptom

Der Library Doctor zeigt drei Varianten derselben Aussage, alle unerklärt:

- Banner unter der Kopfzeile: **„139 fixes are out of date — these files
  changed after the scan."** daneben **Scan again**
- Album-Kopfzeile: **„30 changes · out of date"** (Album `GRAVESIDE
  CONFESSIONS`, Carnifex, 15 Tracks)
- Spalte *Source* in jeder Zeile: **„MusicBrainz · 70 % · Stale"**

Für den Nutzer ist weder erkennbar, **was** veraltet ist (der Vorschlag? die
Datei? der Scan?), noch **was er tun soll**. Der Zustand wirkt wie ein Fehler,
ist aber ein normaler Nebeneffekt jeder Bibliotheksänderung nach dem Scan.

Widerspruch im selben Screenshot, ungeklärt: die Kopfzeile sagt „408 fixes
ready · everything is preselected", die Fußzeile „408 of 408 selected", während
gleichzeitig 139 Zeilen „out of date" sind — und veraltete Zeilen werden im
Code **nie** vorausgewählt (`starts_selected`, siehe unten). Ob 408 und 139
disjunkte Mengen sind (also 547 Zeilen insgesamt) oder sich überlappen, ist
**nicht** geprüft.

## Was der Zustand technisch bedeutet

`stale_flags()` in
`crates/reprise-core/src/library/library_doctor/store.rs:380-418` vergleicht
den zum Scan gespeicherten Datei-Fingerabdruck (`path`, `file_mtime`,
`file_size`, `device`, `inode`) mit dem heutigen Stand der `tracks`-Tabelle.
Weicht irgendetwas davon ab — oder ist der Track weg —, gilt die Zeile als
veraltet (`current.is_none_or(|current| current != snapshot)`, `:414`). Fehlt
ein Snapshot ganz, zählt sie vorsichtshalber ebenfalls als veraltet
(`review.rs:260`, `unwrap_or(true)`).

Folge im Modell:

- `DoctorReviewRowState::Stale` (`review.rs:266-270`, `:326-330`)
- nie vorausgewählt: `starts_selected()` verlangt `state == Ready`
  (`review.rs:97-101`)
- fällt aus der stillen Auto-Apply-Stufe heraus: `is_auto_applied()`
  (`review.rs:73-84`), Kommentar dazu in `store.rs:515-516`

**Es ist also fixbar:** ein erneuter Scan derselben Dateien erneuert den
Fingerabdruck und macht die Zeilen wieder anwendbar — genau das tut die
Schaltfläche **Scan again**. Die Aussage „man kann es nicht fixen" trifft die
Lage nicht; was fehlt, ist, dass die Oberfläche das erklärt oder es gleich
selbst tut.

## Code-Verortung der Texte

- `crates/reprise-gnome/src/ui/strings_library_doctor.rs:380-381`
  (`doctor_stale_notice`, Singular/Plural), Tests `:665-670`
- `crates/reprise-gnome/src/ui/strings_library_doctor.rs:84`
  (`DOCTOR_STATUS_STALE` = „Stale")
- Banner-Berechnung: `crates/reprise-gnome/src/ui/library_doctor/review_page.rs:311-319`
- Spaltentext: `crates/reprise-gnome/src/ui/library_doctor/review_model.rs:369`
- Zeilen-Übergang: `review_page.rs:386` (`DoctorWriteRowState::Unavailable`)

## Entscheidungsoptionen (offen, für den Nutzer)

1. **Verbergen.** Veraltete Zeilen gar nicht listen, stattdessen eine Zeile
   „139 Änderungen brauchen einen neuen Scan" mit *Scan again*. Kleinster
   Eingriff, deckt sich mit dem Wunsch des Nutzers.
2. **Selbst nachziehen.** Beim Öffnen der Ansicht die betroffenen Dateien im
   Hintergrund neu einlesen, sodass der Zustand normalerweise gar nicht
   auftritt; „Stale" bliebe nur für echte Ausreißer (Datei gelöscht/verschoben).
3. **Nur die Sprache reparieren.** „out of date"/„Stale" durch etwas
   Handlungsleitendes ersetzen („Datei hat sich seit dem Scan geändert —
   erneut scannen"). Billigste Variante, löst aber das Grundproblem nicht,
   dass 139 unbrauchbare Zeilen die Liste füllen.

Meine Empfehlung: **1 + 2** — verbergen und im Hintergrund nachziehen; die
Zeilen sind für den Nutzer wertlos, solange sie veraltet sind.

## Offene Fragen

- Zählt „408 fixes ready" die veralteten mit? (Widerspruch oben klären, bevor
  irgendwas verborgen wird — sonst ändert sich die Kopfzahl unerwartet.)
- Warum sind hier **139 von** vermutlich mehreren hundert veraltet? Wurde
  zwischen Scan und Ansicht geschrieben (eigene Apply-Läufe des Doctors
  ändern `file_mtime` und machen andere Vorschläge desselben Albums damit
  veraltet — siehe `store.rs:515-516`)? Das wäre selbstverschuldet.
- Betrifft ein Apply-Lauf auf ein Album danach systematisch die restlichen
  Zeilen desselben Albums? Wenn ja, ist das die eigentliche Ursache.
