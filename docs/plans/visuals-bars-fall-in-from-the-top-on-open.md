---
slug: visuals-bars-fall-in-from-the-top-on-open
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Beim Öffnen der Visuals fallen die Balken erst von ganz oben herunter

**Nur ein Befund, keine Messung.** Gemeldet am 16.08.2026:

> „wenn ich beim neuen Song, der gerade gespielt wird, auf Visuals klicke, dann
> sehe ich eine seltsame Visualisierung. Die Bars fallen von sehr weit oben
> erstmal runter, bevor ich die Visualisierung zur Musik erlebe."

Laufender Build: 0.1.13 (= `dev`-Kopf `95b4b30016`).

## Was bekannt ist

Die Balkenhöhen glättet die Engine **nicht** — `VisualEngine::ingest`
(`crates/reprise-core/src/visuals/engine.rs:219-233`) übernimmt die CAVA-Werte
direkt; nur Spitzenkappen, Stop-Fade und Glow haben eigene Konstanten. Ein
Anfangszustand „ganz oben" kann also nicht aus der Engine kommen: `new()`
(`:99-113`) und `note_track_changed()` (`:202-208`) setzen alle Bänder und
Spitzen auf `0.0`.

Damit bleibt die **CAVA-Auto-Sensitivität** als Quelle
(`crates/reprise-core/src/playback/cava/smoothing.rs`).

## Hauptverdacht — der Schutz gegen genau dieses Problem erzeugt das Bild

`smoothing.rs:61-65` hält fest, was verhindert werden sollte:

> *„A cold analyzer can otherwise draw every band at 1.0 while autosensitivity
> backs down from its first overshoot."*

Der Schutz (`protect_initial_output`) skaliert die Ausgabe deshalb, solange
`sensitivity_initializing || sensitivity_settling` gilt, auf
`INITIAL_SENSITIVITY_HEADROOM / max_internal` (`:124-133`). Die Konstante ist
**0.85** (`:8`). Das heißt: in der Aufwärmphase wird der lauteste Balken auf
**85 % der Leinwandhöhe normiert** — und alles, was intern nahe am Maximum
liegt, steht mit ihm oben. Erst wenn die Sensitivität eingeschwungen ist, fällt
der Schutz weg und die Balken sinken auf ihren echten Pegel.

Das ist exakt die beschriebene Bewegung: **hoch starten, herunterfallen, dann
zur Musik spielen.** Der Schutz verhindert das *Clipping* bei 1.0, nicht den
Absturz von 0.85 — genau das sieht der Nutzer.

**Ungeprüft.** Drei Alternativen, die dasselbe Bild erzeugen könnten:

1. `autosensitivity == 0` in der Konfiguration — dann greift der Schutz gar
   nicht und die Balken stehen wirklich bei 1.0.
2. Die Einschwingphase endet zu früh: `sensitivity_settling` wird beim ersten
   Frame ohne Overshoot zurückgesetzt (`:113-116`), während die Sensitivität
   noch weit zu hoch ist.
3. `reset()` (`:139-145`) läuft beim Öffnen der Ansicht bzw. beim Trackwechsel
   nicht zum richtigen Zeitpunkt, sodass die Aufwärmphase erst beginnt, wenn
   der Nutzer schon zusieht.

## Wie das zu messen ist

Die Bandwerte der ersten ein bis zwei Sekunden nach dem Öffnen mitschreiben
(`REPRISE_LOG`, nicht `RUST_LOG`) und gegen den Verlauf von `sensitivity`
halten: Startet der höchste Balken bei ~0.85 und fällt monoton, ist es der
Schutz; startet er bei 1.0, ist `autosensitivity` aus. Vorher klären, ob die
Aufwärmphase beim **Trackwechsel** oder beim **Öffnen der Ansicht** beginnt —
davon hängt ab, ob der Nutzer sie überhaupt sehen darf.

## Lösungsrichtungen (offen)

1. **Die Aufwärmphase nicht zeigen.** Solange `sensitivity_initializing ||
   sensitivity_settling` gilt, die Balken bei 0 halten oder aus der Ruhewelle
   heraufwachsen lassen, statt normierte Kalibrierframes auszuliefern.
2. **Weicher einblenden.** Die Ausgabe während der Aufwärmphase mit einer
   ansteigenden Hüllkurve multiplizieren — nichts fällt, alles wächst.
3. **Aufwärmen, bevor jemand hinsieht.** Den Analyzer schon beim Trackstart
   laufen lassen, nicht erst beim Öffnen der Visuals — dann ist er
   eingeschwungen, sobald die Ansicht erscheint.

Richtung 1 oder 3; Balken, die von oben fallen, lesen sich als Fehler, Balken,
die aus der Ruhelage wachsen, als Einsatz.
