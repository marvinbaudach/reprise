# Lastregler für schwere Läufe (`heavy-run`)

Datum: 2026-08-11
Status: **scharf geschaltet** am 2026-08-11 — siehe „Umsetzungsstand" am Ende

## Problem

Auf diesem Host (8 Kerne, 31 GB RAM, 64 GB Swap) laufen regelmäßig mehrere
Agenten-Sessions parallel — Claude-Sessions in eigenen Ghostty-Fenstern,
Codex-Läufe in Worktrees, der Nightly-Build per Timer, dazu Emulator, headless
Chromium und Xvfb-Testläufe. Keiner dieser Läufe weiß von den anderen. Jeder
nimmt sich die volle Maschine.

Belegte Folgen:

- **2026-08-03:** Zwei gleichzeitige `cargo test --workspace` aus zwei
  Codex-Worktrees. 624 % von 800 % belegt, **630.000 Kontextwechsel/s**, 57 %
  Systemzeit, Load 22, Swap von 0 auf 14 GB. Der Durchsatz war dabei nicht
  höher als bei einem Lauf allein — die Differenz war reine Verlustleistung.
- **2026-08-04:** Load 13,7 bei nur ~277 % echter CPU-Nutzung. Der Rest war
  I/O-Wait aus Speicherdruck: Android-Emulator (4 h 52 min idle, 1,8 GB) plus
  vier Java-Daemons (3,5 GB) drückten 13 GB in den Swap. Der Speicherdruck
  killte nebenbei den `episodic-memory`-Sync mit SIGBUS.

Beide Male war die Ursache dieselbe: **niemand kennt die Gesamtlast.** Ein
Werkzeug zur Zuordnung existiert (cgroup-Zähler), aber keines zur Regelung.

## Ziel

Ein maschinenweiter Lastregler, der drei Dinge sicherstellt:

1. Zwei schwere Läufe überbuchen die Kerne nicht mehr, sondern teilen sie.
2. Die Maschine bleibt jederzeit interaktiv bedienbar.
3. Auch das, was sich nie anmeldet (vergessener Emulator, Runaway-Browser,
   fremder Build), kann den Host nicht in den Swap drücken.

## Nicht-Ziele

- Kein Leerlauf-Wächter für Prozessleichen. Dafür gibt es `xvfb-orphan-gc`,
  und das Sicherheitsnetz deckelt sie ohnehin.
- Keine Fairness-Warteschlange, keine Prioritäten, kein Broker-Daemon.
- Keine Regelung der interaktiven Desktop-Arbeit des Nutzers. Die hat Vorrang,
  wird aber nicht selbst begrenzt.

## Entschiedene Weichenstellungen

| Frage | Entscheidung |
|---|---|
| Sperrmodus | Kern-Budget (zählendes Semaphor), nicht strikte Exklusivität |
| Durchsetzung | Kooperative Absprache **plus** cgroup-Netz für Nicht-Kooperierende |
| Auslöser | Automatisch (PATH-Shim + Hook), nicht per Disziplin |
| Bei Vollast | Bis 90 s warten, dann schmal starten statt blockieren |
| Zielgröße | 6 von 8 Kernen für Agentenarbeit, Desktop hat Vorrang |
| Ablage | Skripte in `~/.local/bin`, Spec im Reprise-Repo |

## Architektur

Vier Teile, die unabhängig voneinander funktionieren. Fällt einer aus, bleiben
die anderen wirksam.

```
  Shim (cargo/gradle/…)  ──┐
  PreToolUse-Hook        ──┼──▶  heavy-run  ──▶  flock-Semaphor (6 Slots)
  expliziter Aufruf      ──┘                          │
                                                      ▼
                                       systemd-run --scope --slice=agents.slice
                                                      │
                                                      ▼
                                     CPUQuota 600 %, CPUWeight 50, MemoryHigh 18G
```

### 1. `heavy-run` — der Zähler

Ein Bash-Skript in `~/.local/bin/heavy-run`.

```
heavy-run [KLASSE] -- KOMMANDO [ARGS…]
heavy-run status
```

**Slot-Mechanik.** Sechs Dateien `~/.local/state/heavy-run/slot.1` … `slot.6`.
Ein Bewerber greift sich *k* davon per `flock -n` und hält sie an offenen
Dateideskriptoren für die Lebensdauer des Kommandos.

Der Kernel gibt Locks beim Prozessende automatisch frei — auch bei SIGKILL,
Absturz oder abgeschnittenem Terminal. Es gibt deshalb kein `release`, keinen
Aufräumlauf und keine verwaisten Slots.

> Das ist bewusst **anders als `wake-lock`**. Dort musste der Lock den
> aufrufenden Prozess überleben, weil das Agenten-Harness Hintergrundprozesse
> reapt — daher systemd-Units. Hier ist die Anforderung genau umgekehrt: der
> Slot soll exakt so lange leben wie der Lauf und keine Sekunde länger.

**Klassen.** Drei feste Größen statt freier Zahlen:

| Klasse | Slots | wofür |
|---|---|---|
| `heavy` | 4 | Testsuite (`--workspace`), Codex-Lauf, Nightly, Merge-Gate |
| `medium` | 2 | Einzelcrate-Build, Display-Test-Batch — **Default** |
| `light` | 1 | Einzeltest, einzelner Xvfb-Lauf |

Bei 6 Slots passen ein `heavy` und ein `medium` nebeneinander. Zwei `heavy`
passen nicht — der zweite bekommt, was übrig ist.

**Zuteilung.** Slots werden in fester Reihenfolge 1…6 non-blocking probiert.
Reichen die gefundenen nicht für *k*, gibt der Bewerber die bereits genommenen
sofort wieder frei, wartet 3 s und versucht es erneut. Das Freigeben zwischen
den Versuchen ist wesentlich: zwei Bewerber, die sich gegenseitig Teilmengen
halten, würden sich sonst blockieren.

Nach 90 s ohne Erfolg startet der Lauf mit den Slots, die in diesem Moment frei
sind — auch mit keinem. Die zugeteilte Parallelität ist in jedem Fall
mindestens 1, ein Lauf mit null Slots fährt also einspurig und wird allein
durch die Slice gedeckelt. Es wird **nie** dauerhaft blockiert; ein Lauf, der
im Vordergrund hängt, würde den 10-Minuten-Timeout des Harness reißen.

**Weitergabe an das Kommando.** Das zugeteilte *k* wird als Umgebung gesetzt:

```
CARGO_BUILD_JOBS=k
RUST_TEST_THREADS=k
MAKEFLAGS=-jk
GRADLE_OPTS="$GRADLE_OPTS -Dorg.gradle.workers.max=k"
HEAVY_RUN_SLOTS=k
```

**Vererbung.** Ist `HEAVY_RUN_SLOTS` beim Start bereits gesetzt, nimmt der
Aufruf **keine neuen Slots**, sondern übernimmt das Kontingent des Elternteils
und setzt nur die Parallelitäts-Variablen. Ohne das würde ein `cargo`-Aufruf
innerhalb eines Codex-Laufs gegen den Codex-Lauf selbst konkurrieren und der
Governor sich selbst verklemmen.

**Metadaten.** Nach erfolgreichem Lock schreibt der Halter neben den Slot eine
Datei `slot.N.meta` mit PID, Klasse, Kommando und Startzeit. Sie wird nicht
gelöscht; `status` behandelt die Meta-Datei eines lockbaren (= freien) Slots
als veraltet.

**Exit-Code.** `heavy-run` reicht den Exit-Code des Kommandos unverändert
durch. Signale (SIGINT, SIGTERM) werden an das Kommando weitergeleitet.

### 2. `agents.slice` — das Sicherheitsnetz

`~/.config/systemd/user/agents.slice`:

```ini
[Slice]
CPUQuota=600%
CPUWeight=50
MemoryHigh=18G
IOWeight=50
TasksMax=8192
```

`heavy-run` startet das Kommando per
`systemd-run --user --scope --slice=agents.slice`. Verifiziert am 2026-08-11:
die Controller `cpu`, `memory` und `pids` sind an `user@1000.service`
delegiert, alle Werte sind also ohne root setzbar.

`MemoryHigh` statt `MemoryMax`: das drosselt bei Überschreitung, tötet aber
nicht. Ein Testlauf soll langsamer werden, nicht sterben.

Die Quota gilt für die **Summe** aller Läufe, nicht pro Lauf. Zwei Läufe mit je
3 Slots teilen sich dieselben 600 %.

Zusätzlich sollen die bekannten Dauerfresser in dieselbe Slice: Android-
Emulator, Gradle-Daemon, headless Chromium. Damit greift das Netz auch bei den
Prozessen, die nie einen Slot nehmen — dem Fall vom 2026-08-04.

### 3. Auslöser: PATH-Shim und Hook

**Warum nicht nur ein Hook.** Ein `PreToolUse`-Hook in der Claude-Konfiguration
sieht nur Kommandos, die Claude Code selbst absetzt. **Codex läuft nicht unter
Claude-Hooks.** Der Hook sähe `codex-run.sh` als Ganzes, aber keinen der
Dutzenden `cargo`-Aufrufe darin — und genau die waren am 2026-08-03 die
Verursacher.

**Shims.** `~/.local/bin/agent-shims/` mit Wrappern für `cargo`, `gradle`,
`ninja`, `make`. Jeder Shim:

1. bestimmt die Klasse aus den Argumenten: enthält der Aufruf eines der
   Unterkommandos `test`, `build`, `clippy` oder `nextest` **und** zusätzlich
   `--workspace` oder `--all`, gilt er als `heavy`; jeder andere Aufruf als
   `medium`,
2. löst das echte Binary auf, indem er das Shim-Verzeichnis aus dem PATH
   entfernt und neu sucht (`cargo` liegt hier unter `/usr/bin/cargo`),
3. ruft `heavy-run <klasse> -- <echtes Binary> "$@"`.

**Fail-open, ausnahmslos.** Schlägt irgendetwas im Governor fehl — State-Dir
nicht schreibbar, `systemd-run` nicht verfügbar, `flock` fehlt —, führt der
Shim das echte Binary ungebremst aus und schreibt eine Warnung nach stderr.
Ein Lastregler darf niemals Arbeit verhindern.

**PATH-Einbindung.** In `~/.zshenv` (wird auch von nicht-interaktiven Shells
gelesen). Kindprozesse erben den PATH, also erreicht der Shim auch Codex,
Subagenten und alles, was aus einer Session heraus startet. Systemd-Timer
erben ihn *nicht* — `reprise-nightly-build` und der Worktree-GC rufen
`heavy-run` deshalb explizit auf.

**Hook.** Zusätzlicher `PreToolUse`-Matcher für `Bash`, additiv zu den zwei
vorhandenen. Er klassifiziert das, was kein Binary mit Shim ist:
`codex-run.sh`, `check-merge-readiness`, `reprise-nightly-build`, `xvfb-run`
→ `heavy`. Kein Treffer heißt durchlassen.

### 4. `heavy-run status`

Zeigt pro Slot, ob er frei ist, und bei belegten Slots PID, Klasse, Kommando
und Haltedauer aus der Meta-Datei. Darunter die Ist-Werte der Slice
(CPU-Nutzung, Speicher, gedrosselte Zeit) aus `cpu.stat` und `memory.current`.

Ohne diese Ansicht ist eine Blockade nicht diagnostizierbar — dieselbe
Begründung wie bei `wake-lock status`.

## Fehlerverhalten

| Fall | Verhalten |
|---|---|
| State-Dir fehlt oder ist nicht schreibbar | Warnung, Kommando läuft ungebremst |
| `systemd-run` schlägt fehl | Warnung, Kommando läuft ohne Slice, Slots gelten trotzdem |
| Kein Slot in 90 s frei | Start mit den freien Slots, mindestens 1 |
| Prozess stirbt (auch SIGKILL) | Slots fallen durch den Kernel zurück |
| Verschachtelter Aufruf | Erbt `HEAVY_RUN_SLOTS`, nimmt keine neuen Slots |

## Testbarkeit

`heavy-run` liest drei Variablen, die Tests ohne systemd und ohne echte
Kernzahl ermöglichen:

- `HEAVY_RUN_STATE_DIR` — Slot-Verzeichnis (Test: tmp-Verzeichnis)
- `HEAVY_RUN_TOTAL` — Slot-Anzahl (Test: 2)
- `HEAVY_RUN_WAIT_SECONDS` — Warte-Deadline (Test: 2 statt 90)
- `HEAVY_RUN_NO_SLICE=1` — ohne `systemd-run` starten

Testfälle:

1. `medium` bei 2 freien Slots nimmt beide.
2. Zweiter `medium` bekommt nach Ablauf der Deadline 0 Slots und läuft
   einspurig weiter statt zu hängen.
3. `kill -9` auf den Halter gibt die Slots sofort frei.
4. Verschachtelter Aufruf mit gesetztem `HEAVY_RUN_SLOTS` nimmt keine Slots.
5. Nicht schreibbares State-Dir: Kommando läuft, Exit-Code stimmt, Warnung
   erscheint.
6. Exit-Code und Signalweitergabe sind transparent.

## Abnahmekriterien

Der Fall vom 2026-08-03 wird nachgestellt: zwei `cargo test --workspace`
gleichzeitig aus zwei Worktrees, einmal ohne und einmal mit Regler. Gemessen
werden Kontextwechsel/s, Load, Systemzeit-Anteil und Swap-Nutzung — nach der
cgroup-Methode, nicht mit `ps`/`top`.

Abnahme gilt als bestanden, wenn mit Regler:

- die Kontextwechsel/s deutlich unter dem Referenzwert liegen,
- die Swap-Nutzung nicht ansteigt,
- die Gesamtlaufzeit beider Läufe zusammen **nicht** länger ist als ohne
  Regler (die Verlustleistung finanziert die Staffelung),
- ein interaktives Kommando im Terminal währenddessen ohne spürbare
  Verzögerung antwortet.

## Umsetzungsstand (2026-08-11)

Gebaut und aktiv. Die Dateien liegen außerhalb dieses Repos, weil das Werkzeug
maschinenweit gilt und nicht zu einem Projekt gehört:

| Datei | Rolle |
|---|---|
| `~/.local/bin/heavy-run` | Zähler, Warteschleife, Vererbung, `status` |
| `~/.local/bin/heavy-run-test` | Testsuite, 9 Fälle |
| `~/.local/bin/agent-shims/_shim` | Shim, verlinkt als `cargo`/`gradle`/`make`/`ninja` |
| `~/.config/systemd/user/agents.slice` | das Netz |
| `~/.claude/hooks/heavy-run-gate.sh` | Gate für schwere Einstiegspunkte |
| `~/.zshenv`, `~/.bashrc` | PATH-Einbindung, idempotent |
| `reprise-nightly.service.d/heavy-run.conf` | Slice + PATH für den Timer |
| `reprise-worktree-gc.service.d/heavy-run.conf` | Slice für den GC-Lauf |

Sicherung vor dem Eingriff: `~/.claude/settings.json.bak-pre-heavy-run`.

### Gemessen

- **Testsuite:** 9 von 9 grün, inklusive Slot-Freigabe nach `SIGKILL`,
  Vererbung, Fail-open bei nicht schreibbarem State-Dir und Exit-Code-Transparenz.
- **Hook:** 10 von 10 grün, inklusive kaputtem JSON und leerer Eingabe — beide
  lassen durch.
- **Deckelung wirkt:** Gegenprobe mit temporär auf 200 % gesetzter Quota und
  acht Rechenschleifen ergab **201 % gemessene CPU bei 60 Drosselungsereignissen**.
  Beim regulären Wert von 600 % kam derselbe Test nur auf 380 % und wurde nie
  gedrosselt — die schon laufende Fremdlast bremste ihn vorher aus, was genau
  die beabsichtigte Wirkung von `CPUWeight=50` ist.
- **Staffelung wirkt:** Zwei `heavy`-Läufe (4 + 4 Slots auf 6) überbuchten
  nicht. Der zweite wartete und bekam seine vier Slots, sobald der erste endete.
- **Klassifikation:** `cargo --version`, `metadata`, `fmt` laufen ungebremst
  durch; `check` → `light`, `build` → `medium`, `test --workspace`,
  `clippy --all`, `nextest run --workspace` → `heavy`.

### Offene Punkte

1. **Laufende Sessions sind noch ungeregelt.** Der PATH wird beim Start einer
   Shell gesetzt; Sessions und Codex-Läufe, die vor dem Einbau gestartet
   wurden, sehen die Shims nicht. Sie greifen ab dem nächsten Terminalstart.
2. **Die Abnahme mit zwei echten `cargo test --workspace` steht aus.** Sie
   wurde nicht gefahren, weil zum Zeitpunkt der Umsetzung bereits zwei
   Codex-Läufe die Maschine belegten und das Ergebnis verfälscht hätten. Die
   Messmethode steht oben unter „Abnahmekriterien".
3. **`./gradlew` wird mit Pfad aufgerufen** und trifft den PATH-Shim daher
   nicht. Für die Android-Arbeit greift nur die Slice. Ein Shim direkt im
   Projekt wäre möglich, ist aber nicht gebaut.
4. **`IOWeight=50` ist derzeit wirkungslos.** Gemessen am 2026-08-11: der
   `io`-Controller existiert auf der Wurzel-cgroup, ist aber nicht nach unten
   delegiert — `cgroup.subtree_control` führt in `/`, `user.slice` und
   `user-1000.slice` nur `cpu memory pids`. Die Zeile schadet nicht, tut aber
   nichts. Sie zu aktivieren erfordert root:
   `echo +io > /sys/fs/cgroup/cgroup.subtree_control` und dasselbe kaskadierend
   für `user.slice` und `user-1000.slice`, dauerhaft per Drop-in auf
   `user@.service`. Das ist relevant, weil am Messtag der I/O-Druck (`full
   avg10=57,7`) und nicht die CPU der eigentliche Engpass war.

## Verwandte Notizen

- `parallel-cargo-test-oversubscribes-cores` — der Vorfall vom 2026-08-03
- `agent-run-cpu-leftovers` — die Prozessleichen vom 2026-08-04
- `host-cpu-attribution-via-cgroups` — die Messmethode für die Abnahme
- `detach-long-runs-from-harness-timeout` — Grund für die 90-s-Grenze
- `delegation-needs-a-trigger` — Grund gegen ein reines Disziplin-Modell
