# Reprise UX-Regelwerk

Kanonischer UX-Vertrag der App. Jede Regel hat eine stabile ID und einen Status:
`[aktiv]` = implementiert und durch Tests/Abnahme gedeckt · `[geplant]` = beschlossen,
Implementierung ausstehend. Statuswechsel `[geplant] → [aktiv]` passiert im
Implementierungs-Commit der Regel. Akzeptanztests referenzieren Regel-IDs.

> Herkunft: Die IDs NAV-1..8, PLAY-*, FB-* stammen aus dem mündlichen Regelwerk der
> Projektführung; dieses Dokument schreibt sie erstmals fest. Bestandsregeln, deren
> Wortlaut hier rekonstruiert ist, sind mit ⟲ markiert und bei Abweichung zu korrigieren.

## Navigation (NAV)

- **NAV-2 · History-Stack** `[aktiv]` — Navigationssprünge (Quellenwechsel über die
  Sidebar, explizite Sprünge wie NAV-9) pushen den verlassenen Ort auf einen globalen
  History-Stack. „Back" (Alt+←) kehrt zum obersten Eintrag zurück; Back selbst pusht nicht.
- **NAV-3 · Kontext-Klicks in der Player-Leiste** `[aktiv]` ⟲ — Der Artist-Klick in der
  Player-Leiste führt in die Artists-Ansicht mit selektiertem Künstler. Album-/Artist-Klicks
  behalten ihre Ziele auch nach Einführung von NAV-9 — nur Cover/Titel springen zum Track.
- **NAV-5 · View-State bleibt in der Session erhalten** `[aktiv]` ⟲ — Sidebar-/Ortswechsel
  erhält Scroll-Position und Selektion des verlassenen Modus innerhalb der Session. Beim
  Zurückkommen findet man die Liste wie verlassen vor. Präzisierung: kein Persistieren
  über App-Neustarts — der Zustand lebt nur in der laufenden Session.
- **NAV-9 · Jump to Now Playing** `[aktiv]` — Klick auf Cover oder Titel in der
  Player-Leiste navigiert zur Heimat des spielenden Tracks (Library-Modus Tracks bzw.
  Playlist, aus der er spielt), selektiert die Row und zentriert sie (Scroll so, dass die
  Row im mittleren Drittel liegt — kein scrollIntoView-Kantenkleben). Zusätzlich Shortcut
  Strg+L. Das ist die explizite „wo bin ich gerade"-Geste; sie pusht auf den History-Stack
  (NAV-2 global, Back kehrt zurück). Artist-/Album-Klick in der Leiste behalten ihre
  NAV-3-Ziele — nur Cover/Titel springen zum Track.

## Wiedergabe-Kontext (PLAY)

- **PLAY-1 · Wiedergabe hat immer einen Kontext** `[aktiv]` ⟲ — Ein Wiedergabestart erzeugt
  einen Kontext-Snapshot (Container-Play bzw. sichtbare Liste); Auto-Advance läuft durch
  diesen Snapshot, nie durch eine versteckte andere Reihenfolge.
- **PLAY-2 · Doppelklick = sichtbare Liste ab Position** `[aktiv]` ⟲ — Doppelklick auf eine
  Row übernimmt die aktuell sichtbare (sortierte, gefilterte) Liste als Snapshot und startet
  an der geklickten Position.
- **PLAY-3 · Filter zählt** `[aktiv]` ⟲ — Ist beim Start ein Filter aktiv, enthält der
  Snapshot nur die Treffer.

## Queue (QUE)

- **QUE-1 · Die Queue ist nie leer, solange etwas spielt** `[aktiv]` — Sie zeigt drei
  Abschnitte, in dieser Reihenfolge:
  1. **Now Playing** (1 Row, Akzent + EQ, wie überall)
  2. **Play Next** — manuell eingereihte Tracks („Play next"/„Add to queue"), nur wenn
     vorhanden, mit Sektionstitel
  3. **Up Next · aus <Quelle>** — der Rest des Playback-Snapshots (z. B. „Up Next · from
     Late Night" oder „· from Neverbloom"), inklusive Shuffle-Reihenfolge, falls Shuffle an
- **QUE-2 · Abspiellogik = Anzeigereihenfolge** `[aktiv]` — Erst Play-Next-Einträge
  (FIFO), dann der Snapshot ab aktueller Position. Keine versteckte Priorität — was die
  View zeigt, ist was passiert.
- **QUE-3 · Interaktion** `[aktiv]` — DnD-Reorder innerhalb „Play Next"; Up-Next-Rows per
  DnD nach „Play Next" ziehbar; Rechtsklick „Remove from queue" überall (entfernt aus dem
  Snapshot, nicht aus der Library); Doppelklick auf eine Queue-Row springt dorthin
  (Playhead, kein Neuaufbau). „Clear queue"-Button räumt nur „Play Next"; der Snapshot
  bleibt (er verschwindet erst mit Playback-Stop oder neuem Kontext).
- **QUE-4 · Leerzustand nur ohne Wiedergabe** `[aktiv]` — StatusPage „Nothing queued —
  play something" (FB-5: ein nächster Schritt, kein Grid an Vorschlägen).
- **QUE-5 · Sidebar-Zähler „Queue · N"** `[aktiv]` — N = Play Next + verbleibende
  Up-Next-Tracks (nicht Gesamt-Snapshot).

## Feedback & Leerzustände (FB)

- **FB-5 · Leerzustände bieten genau einen nächsten Schritt** `[aktiv]` ⟲ — Ein
  Leerzustand benennt, was fehlt, und bietet eine einzige naheliegende Aktion an — kein
  Grid an Vorschlägen, keine konkurrierenden Call-to-Actions.
