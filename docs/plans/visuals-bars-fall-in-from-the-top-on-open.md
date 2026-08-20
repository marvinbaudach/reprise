---
slug: visuals-bars-fall-in-from-the-top-on-open
worktree: /home/marvin/Projects/reprise-visuals-bars-fall-in-from-the-top-on-open
branch: feature/visuals-bars-fall-in-from-the-top-on-open
phase: planned
codex_session:
created: 2026-08-20
---
# Die Peak-Kappen klingen nur ab, wenn jemand hinsieht

Beim Öffnen der Visuals-Ansicht fallen die Balken von oben herein, statt sofort
zur Musik zu spielen. Ursache gemessen am 20.08.2026: die weißen Peak-Kappen
klingen **ausschließlich** im Tick-Loop ab, und der läuft nur bei sichtbarem
Tab — während sie durchgehend angehoben werden, solange Musik spielt.

**Alle Zeilenangaben gegen `51e9c6c9bb`.** Basis dieses Worktrees ist
`origin/dev` = `40655644fc`; die zwei Commits dazwischen (#583, #584) fassen nur
`.github/workflows/ci.yml` und `scripts/tests/cua-*.sh` an, keine Quelldatei
dieses Plans. Die Zeilennummern gelten unverändert — trotzdem vor dem Ändern
kurz gegenlesen.

## Die Vermutung des Befunds war falsch

Der ursprüngliche Befund verdächtigte `protect_initial_output` in
`crates/reprise-core/src/playback/cava/smoothing.rs` — die Normierung auf 0.85
der Leinwandhöhe während der Aufwärmphase.

**Gemessen, mit Kontrollarm, und widerlegt.** Der Glättungspfad lässt die
Balken von *unten* hochlaufen:

```
auto=1 chunk=1024 level=0.10   erste Frames: 0.10 0.19 0.26 0.34 … 0.81 0.85 0.85 …
                               peak=0.988  steady=0.976  Plateau@0.85 = 24 Frames
KONTROLLARM auto=0             peak=0.436  steady=0.386  Plateau@0.85 =  0 Frames
```

`protect_initial_output` erzeugt ein **Plateau**, kein Herunterfallen. Der
Kontrollarm belegt, dass das Plateau tatsächlich von dieser Stelle stammt.

Zwei Strukturfunde entkräften die Vermutung zusätzlich:

- **`Smoother::reset()` hat keinen einzigen Aufrufer** (`smoothing.rs:138`) —
  die Aufwärmphase läuft genau einmal.
- Der Analysator entsteht beim Bau der Playback-Pipeline
  (`reprise-platform-linux/src/player_pipeline.rs:308`), **nicht** beim Öffnen
  der Ansicht. Beim Öffnen ist er längst eingeschwungen.

Damit ist der Glättungspfad als Ursache ausgeschlossen. Er bleibt trotzdem ein
eigener Befund wert (das Plateau und der tote `reset()`) — aber nicht hier.

## Was wirklich passiert

`crates/reprise-core/src/visuals/engine.rs`:

- `ingest()` (`:219-227`) hebt die Kappen nur an: `*peak = peak.max(*current)`.
  **Kein Abklingen.**
- `advance_ticks()` (`:273-280`) ist die **einzige** Stelle, die sie senkt:
  `*peak = (*peak - PEAK_DECAY * elapsed_ticks).max(floor)`, mit
  `PEAK_DECAY = 0.018` (`:16`) bei `SIMULATION_TICKS_PER_SECOND = 60.0` (`:23`).

Der Tick-Loop läuft nur bei sichtbarem und aktivem Visual-Tab
(`ui/now_playing/song_visualizer.rs:166-219`, `ensure_tick`/`stop_tick`, gesteuert
über `set_active` aus `now_playing_light.rs:80-86`). `ingest()` hängt dagegen
**nicht** an der Tab-Auswahl, sondern nur an „Musik spielt"
(`now_playing.rs:543-556`, `song_visualizer.rs:119-126`).

Bei geschlossenem Tab rasten die Kappen also nach oben, ohne je zu fallen.

### Die Messung

`VisualEngine` ist portabel und ohne GTK fahrbar. Gemessen wurde mit
synthetischer, pro Band phasenverschobener Musik (0,05–0,9), Ingest 60 Hz,
Tick-Schritt fest 16 667 µs wie in der GUI:

| Arm | Kappen (Median) | laufende Musik (Median) |
| --- | --- | --- |
| Tab war 10 s zu, kein Tick | **0,8917** (max 0,9000) | 0,3907 (max 0,7291) |
| Kontrollarm: Tab durchgehend offen | Abstand `max(peak − current)` über den ganzen Lauf: **0,0476** | — |

Nach dem Öffnen erreichen die Kappen das Niveau des Kontrollarms erst nach
**46 Ticks ≈ 0,767 s** (gehalten über 10 Ticks, Toleranz 0,01). Im Moment des
Öffnens klaffen 0,874 gegen 0,409.

Gemessen wurde an `bands_peaks` bzw. `ModeCtx.peaks` — genau den Werten, die
`modes/bars.rs:87-161` als Kappenhöhe zeichnet. Nicht an Pixelkoordinaten: die
Konstanten in `modes/bars.rs` sind modulprivat. Die Pixel-Y-Position ist eine
affine Funktion von `peak`, die Aussage ändert sich dadurch nicht — der
Vollständigkeit halber genannt.

## Es ist ein Regelbruch, keine offene Frage

`docs/ux-rules.md`, **AC-23** [active] sagt über den pausierten Zustand:

> …while the last CAVA values remain intact and the peak caps keep their normal
> **independent** decay.

Genau diese Unabhängigkeit fehlt. Das Abklingen ist an die Sichtbarkeit des Tabs
gekoppelt. Der Plan stellt die zugesagte Eigenschaft her; er erfindet keine neue.

## Aufgaben

### Task 1 — Der Kontrollarm wandert in die Suite

Der Messaufbau oben wird zum dauerhaften Test in `reprise-core` (kein GTK
nötig): Engine bauen, 10 s ingesten ohne Tick, dann Tick starten und gegen einen
zweiten Engine halten, der durchgehend getickt hat.

Zusage: der Abstand der Kappen zwischen beiden Armen ist **beim ersten Tick nach
dem Öffnen** bereits innerhalb der Toleranz, die der Kontrollarm über seinen
ganzen Lauf einhält (gemessen 0,0476).

Der Test ist zunächst **rot** und nennt den Betrag: heute klaffen dort 0,874
gegen 0,409.

Name mit Präfix `ac_23_`, damit `scripts/check-ux-traceability.sh` die Kennung
wiederfindet.

**Akzeptanz:** Ein Test, der den Aufstau als Zahl nennt und aus dem
Fehlerprotokoll ablesbar macht, wie weit die Kappen über der Musik stehen.

### Task 2 — Das Abklingen wandert nach `ingest()`

**Die Richtung ist im Grill entschieden und steht nicht mehr zur Wahl:** die
Kappen klingen künftig **in `ingest()`** mit ab. Sie werden damit zu dem, was
AC-23 verspricht — eine zeitabklingende Größe, unabhängig davon, ob jemand
hinsieht.

`ingest()` kennt die verstrichene Zeit heute nicht; sie muss hineingereicht
werden. Das ist der eigentliche Umbau, und er ist der Grund für diese Wahl: es
ist die einzige Variante, nach der der Fehler strukturell nicht wiederkommen
kann. Ein zweiter Weg, der `advance_ticks` umgeht, hätte sonst dasselbe Problem
erneut.

**Was unverändert bleibt — beides ausdrücklich:**

- der Bodenwert `.max(floor)`;
- das Verhalten im pausierten Zustand (AC-27, von AC-23 referenziert). Ein
  pausierter Titel behält seine Kappen. Das ist zugesagt und kein Aufstau — und
  weil `ingest()` bei Pause nicht läuft, fällt es von selbst richtig aus.
  Trotzdem gehört ein Test dazu, der es festhält.

**Achtung bei der Zeitquelle:** `ingest()` darf sich das Delta nicht selbst aus
einer Uhr holen, sonst ist der Test aus Task 1 nicht mehr deterministisch. Die
verstrichene Zeit kommt vom Aufrufer, so wie `advance_ticks` sie heute
bekommt.

**Akzeptanz:** Der Test aus Task 1 wird grün, ohne dass eine Toleranz gelockert
wird. Bestehende Visuals-Tests bleiben grün, und ein Test hält das pausierte
Verhalten fest.

### Task 3 — AC-23 sagt es ausdrücklich

Die Regel verspricht das Verhalten bereits, aber so knapp, dass der Code jahrelang
danebenliegen konnte. Ergänzt wird der Satz um: das Abklingen läuft **auch
während die Ansicht nicht sichtbar ist**; ein wiederaufgenommener Tick-Loop darf
keinen Rückstand abzuarbeiten haben.

Keine neue Kennung — AC-23 ist aktiv und trägt die Zusage schon.

**Akzeptanz:** Traceability-Gate grün; die Regel nennt den unsichtbaren Fall.

### Task 4 — Gegenmessung

Mutationsprobe: die Abklingstelle aus Task 2 wieder entfernen — **genau ein
Vorkommen** — und belegen, dass der Test aus Task 1 rot wird. Erst committen,
dann mutieren; `git checkout --` verschluckt Uncommittetes wortlos.

## Nicht in diesem Plan

- **„Beim Aktivieren aufholen"** (`set_active(true)` senkt die Kappen auf das
  Niveau, das sie bei durchgehendem Tick hätten). Im Grill verworfen: klein und
  zielgenau, behandelt aber den Öffnen-Moment statt der Kopplung.
- **„Beim Reaktivieren um die echte verstrichene Zeit vorspulen."** Der erste
  Tick nach `ensure_tick` rechnet heute mit einem festen Delta von 16 667 µs
  (`song_visualizer.rs:197-198`); es gibt keinen Nachholsprung. Ein echter
  Nachholsprung spult **auch Leerlaufwelle und Glow-Freigabe** vor, und was die
  dann tun, ist nicht gemessen. Im Grill ausdrücklich verworfen.
- **Das 0,85-Plateau im Glättungspfad** (`smoothing.rs`) und der tote
  `Smoother::reset()`. Beides ist echt und oben gemessen, aber es erklärt den
  gemeldeten Fehler nicht. Eigener Befund.
- **Der Ingest-Takt 20 Hz gegen 60 Hz.** Im Code konnte keine
  Interpolationsstufe belegt werden; falls die Pipeline wirklich langsamer
  liefert als gezeichnet wird, ist das ein getrenntes Ruckel-Thema.

## Belege

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`
- `scripts/check-ux-traceability.sh`
- die Mutationsprobe aus Task 4

Display-Tests sind nicht gefordert: der gesamte Mechanismus liegt in
`reprise-core` und ist ohne Oberfläche messbar — das ist der Grund, warum dieser
Befund überhaupt in einer Nacht belegbar war.

## Parallelität

**Ein Strang.** Task 1 ist der Kontrollarm für Task 2, Task 3 hängt an Task 1s
Testnamen.

**Reihenfolge:** 1 → 2 → 3 → 4.

**Dateibesitz dieses Strangs:**

```
crates/reprise-core/src/visuals/engine.rs
crates/reprise-core/src/visuals/engine/*_tests.rs        (neu)
crates/reprise-gnome/src/ui/now_playing/song_visualizer.rs
docs/ux-rules.md
```

**Achtung, geteilte Datei:** `docs/ux-rules.md` gehört auch den vier
Geschwisterplänen dieser Welle und dem Strang
`queue-centering-ignores-section-headers`. Verschiedene Regeln, dieselbe Datei —
der Konflikt wird **beim Landen** aufgeräumt, nicht vorher vermieden.

`crates/reprise-core/src/playback/cava/smoothing.rs` gehört diesem Strang
**nicht** — der Glättungspfad ist ausdrücklich nicht Teil des Plans.

**Post-Merge-Querprüfungen:** keine.
