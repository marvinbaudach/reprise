# Übergabe — Showroom: Vite-8-Reparatur, abgeleitete Zahlen, Tempo-Timeline

Stand 19.08.2026, 20:15 Uhr. Die Showroom-Welle (Galerie-Hover + Kapitel Zwei)
ist gelandet. Danach hat eine fremde Sitzung `dev` rot gefahren; die Reparatur
ist gebaut und steht im Gate. Die eigentliche Aufgabe — Zahlen ableiten und die
Skript-Selbsttests unter ein Gate bringen — ist **noch nicht begonnen**, dafür
liegt hier der vollständige Bestandsaufnahme-Befund.

## Was gelandet ist

| PR | Merge | Inhalt |
|---|---|---|
| [#574](https://github.com/marvinbaudach/reprise/pull/574) | `c213e2a02e` | Galerie-Hover hält den Rahmen still, SHOW-1…5 |
| [#575](https://github.com/marvinbaudach/reprise/pull/575) | `9bafa746e5` | Kapitel Zwei als zwei abgeleitete Figuren, SHOW-6…10 |

Beleg: lokales Gate `GATE_EXIT=0` (Display-Suite 0 von 531 rot), dev-CI-Lauf
[32280637778](https://github.com/marvinbaudach/reprise/actions/runs/32280637778)
`success`. `check-ux-traceability.sh` auf dem gemergten `dev`: 394 aktive Regeln
abgedeckt, SHOW-1…10 zusammen im selben Baum. `feature/showroom-plate-plays-the-visualizer`
war überholt (Inhalt byteidentisch auf `dev`, Plan mit #571 gepflückt) und ist
samt Branch entfernt.

## Der laufende Vorgang: dev war rot, die Reparatur steht im Gate

Direkt nach der Welle hat die Dependabot-Sitzung drei Major-Bumps nach `dev`
gebracht — **Vite 6.4.3 → 8.2.1, TypeScript 5.9.3 → 7.0.2, @vitejs/plugin-react
4.7.0 → 6.0.5** (#538/#539/#540). Danach waren **10 von 63 Showroom-Tests rot**,
dev-CI-Läufe `32281136452` und `32281407712` beide `failure`.

**Ursache:** Vite 8 minifiziert CSS nicht mehr mit esbuild, sondern mit
**Lightning CSS**. Keiner der zehn Tests hat eine kaputte Seite gefunden; alle
zehn haben den Minifier gefunden, der dieselben Regeln anders schreibt:

- `oklch()` wird gegen die „widely available"-Baseline **heruntergerechnet** —
  jede Farbe doppelt, einmal Hex-Näherung, einmal `lab()`.
- Deklarationen werden **innerhalb des Blocks sortiert**.
- `@media (hover:hover)` bekommt ein **Leerzeichen**, wo esbuild keines schrieb.
- `flex:1 1 320px` → `flex:320px`, `animation:rp-sweep 1.5s linear infinite` →
  `animation:1.5s linear infinite rp-sweep`, `animation:… ease both` verliert das
  `ease` (Initialwert), `transition-delay:0s` faltet sich in `transition:none`.

**Die Reparatur** (Commit `ce52a67e04`, Branch `feature/showroom-survives-vite-8`,
Worktree `/home/marvin/Projects/reprise-showroom-survives-vite-8`, auf `origin/dev`
@ `45e65480e9`):

1. `build.cssTarget` nennt die vier Browser, ab denen `oklch()` verfügbar ist —
   die Palette geht in dem Farbraum raus, in dem sie entworfen wurde.
2. Sieben Testdateien behaupten nicht länger eine **Schreibweise**, sondern die
   **Regel**: Blockzugehörigkeit statt Reihenfolge, optionales Leerzeichen im
   `@media`-Prelude, `(?:1 )?` im flex-Kurzformat, Bestandteile des
   `animation`-Kurzformats statt seiner Sequenz. SHOW-4/5/9 prüfen unverändert
   dasselbe. Bei `transition-delay` ist die Zusicherung **schärfer** geworden:
   `assert.doesNotMatch(guarded, /transition-delay:(?!0(?:ms|s)?[;}])/)` verbietet
   jede Verzögerung ungleich null, statt eine bestimmte Zeile zu erwarten.

**Belegt:** `npm --prefix showroom test` → 63/63 grün · `npm --prefix showroom run
lint` → 0 · `npm --prefix showroom run typecheck` → 0 (TypeScript 7 ist sauber).

**Offen — das Erste, was die nächste Sitzung tut:**

Das Gate läuft seit 19:54 Uhr im Reparatur-Worktree (PID 3723675, Log
`…/scratchpad/gate-vite8.log`, Runner `heavy-run medium`). Wenn es grün ist:

```
cd /home/marvin/Projects/reprise-showroom-survives-vite-8
git push -u origin feature/showroom-survives-vite-8
gh pr create --base dev --title "The showroom survives Vite 8's new CSS minifier" --body …
~/.claude/skills/pipeline/scripts/land.sh <nr> /home/marvin/Projects/reprise-showroom-survives-vite-8
```

`land.sh` braucht eine **PR-Nummer**; es pusht und öffnet nichts selbst.

> **Die Display-Stufe schweigt bis zum Ende.** `scripts/check-display-tests.sh`
> sammelt jeden Testlog in ein `mktemp`-Verzeichnis und gibt erst danach die
> Bilanz aus — 531 Tests seriell, rund 35 Minuten Stille. Ein Log-Stall-Detektor
> meldet dort „hängt"; genau so wurde der Lauf am 19.08. einmal abgeschossen.
> Der ehrliche Fortschritt sind die `<index>.status`-Dateien im Ergebnis-`mktemp`.

## Die eigentliche Aufgabe

### 1. Die Tempo-Timeline (neue Idee des Nutzers, per Screenshot)

`TempoBand` **ersetzen**: statt der vier Statistik-Zeilen (347'842 Zeilen /
45.8 % / Gates / 0) eine Timeline aus vier Wochenkarten — CORE, DEPTH, ANDROID,
SIGNATURE — mit Punkt-Linie und Mono-Datumszeile darunter (`18–24 Jul` …
`8–14 Aug`), Überschrift „Idea to alpha · 4 weeks" bleibt. Untertext im Entwurf:
„Dated from the plan records in `docs/`".

Entscheidungen des Nutzers: **ersetzen** (nicht ergänzen), und der Screenshot ist
**Richtungsgeber, nicht pixelgenaue Vorlage** — Details aus den bestehenden
Tokens und Kapitel-Mustern ableiten.

**Der Entwurf nennt Zahlen, die das Repository nicht deckt** (gemessen 19.08.):

| Entwurf | Wirklichkeit |
|---|---|
| 203 Pläne | **88** in `docs/plans/*.md`, davon **30 ohne `created:`** |
| 23 Design-Specs | **2** Einträge in `docs/design/` (`README.md`, `reprise-showroom.design.html`) |
| „dated from the plan records" | die Daten stehen im Frontmatter — aber nur bei 58 von 88 |

**Und die Falle darunter:** `docs/plans/README.md` schreibt vor, dass Pläne mit
`phase: shipped` und alle `*.HANDOFF*.md` **gelöscht werden**, sobald die Arbeit
gelandet ist (zuletzt #571). Eine zur Bauzeit gezählte Plananzahl **schrumpft
also mit der Zeit** — sie wäre eine Zahl, die lügt, sobald aufgeräumt wird. Wer
die Wochen wirklich datieren will, braucht entweder die Git-Historie
(`git log --diff-filter=A -- docs/plans/`) — die in CI mit `fetch-depth: 1`
**nicht** vorhanden ist — oder eine bewusst festgeschriebene Quelle. Das ist die
Entwurfsentscheidung, die vor dem ersten Handgriff fällt.

### 2. Zahlen ableiten statt zählen (`measurements.ts`)

Bestand, vollständig vermessen:

- **Abgeleitet ist genau eine Zahl:** die Gate-Anzahl, über das Vite-Plugin
  `reprise-derived-facts` (`showroom/vite.config.ts:78-113`) und die virtuellen
  Module `virtual:merge-gates` / `virtual:agent-pipeline`. Konsumenten:
  `GateWall.tsx`, `AgentSwimlane.tsx`, `TempoBand.tsx:40`, `measurements.ts:60`.
- **Kein Repo-Artefakt trägt** `HEADLINE_FIGURES[0]` (347'842 Zeilen),
  `[1]` (45.8 % Tests) oder die vier `CODE_SEGMENTS`-Zeilen. Der Doc-Kommentar in
  `measurements.ts:5-8` sagt es selbst: extern mit `cloc 2.08` plus manuellem
  AST-Durchgang auf Commit `604677322e` gezählt. Dieser Vorgang ist **nicht im
  Repository**. `SiteFooter.tsx:25` sagt es der Leserschaft sogar zu: „This build
  does not measure them yet — the figures are typed into one module and the
  measurement runs in CI next."
- **`PERFORMANCE` und `PERFORMANCE_PRICE`** (µs-Zeiten, ms/s CPU, DB-Bytes) sind
  einmalig gemessene Werte ohne Benchmark-Datei im Repo. Ableitbar sind sie
  nicht; ehrlich wären sie nur mit einer eingecheckten Messausgabe.
  `headless-ledger-footer.test.mjs:72-97` prüft nur, dass die Literale gerendert
  werden — ein Rundlauf, keine Wahrheitsprüfung.
- **`SPECTRAL_AXIS`** verlinkt `crates/reprise-view/src/spectral_colour.rs`, die
  Hex-Werte sind von Hand kopiert — ableitbar, wenn jemand die Datei parst.
- `HEADLINE_FIGURES[2]` „1 → 4" könnte aus `crates/` plus `android/` folgen.
- **SHOW-10 hält den Rahmen schon:** `chapter-two.test.mjs` (~183) verbietet die
  Gate-Zahl als Literal in `measurements.ts` und in jeder `.tsx` unter
  `src/components`. Dasselbe Muster trägt jede weitere abgeleitete Zahl.

### 3. Die Skript-Selbsttests laufen in keinem Gate

`scripts/tests/` enthält 27 Skripte plus 13 Python-Testmodule. Im **automatischen**
Gate laufen davon **vier**: `worktree-gc.sh`, `worktree-gc-schedule.sh`,
`gettext-catalogs.sh` (`check-merge-readiness.sh:85-87`) und `github-flow.sh`
(`ci.yml:95`).

Alles Übrige hängt an `scripts/tests/qa-linters.sh`, und die wird **nur** von
`scripts/check-release.sh:11` gerufen — einem Skript, das weder `ci.yml` noch
`check-merge-readiness.sh` je anfasst. Betroffen sind unter anderem
`project-quality.sh` (der ursprüngliche Befund), `cua-e2e.sh`, `motion-tokens.sh`,
die vier `performance-*.sh`, `readme-showcase.sh`, `accessibility-semantics.sh`,
`input-parity.sh`, `android-theme.sh`, `weekly-portfolio-sync.sh`, `msrv.sh`.

**Ganz ohne Aufrufer** — auch nicht über `qa-linters.sh`:
`architecture-size-limits.sh`, `check-android-suite.sh`, `cua-explore.sh`,
`scan-scrutinee-borrows.py` sowie sechs `cua-explore-*.py`, die in der Laufliste
von `cua-explore.sh:8-20` fehlen. Dazu sieben verwaiste `scripts/*.sh`
(`check-spectrogram-smoke.sh`, `playback-history-smoke.sh`,
`render-showroom-seek-track.sh`, `render-showroom-visualizer.sh`,
`repro-track-list-fresh-start.sh`, `shoot-updates-popover.sh`,
`verify-radio-favicons.sh`).

**Der natürliche Ort** ist der Job `base-contracts` in `.github/workflows/ci.yml`
(`:59-98`): er läuft bei jedem nicht übersprungenen Push ohne Pfad-Routing, hat
`ripgrep`/`shellcheck`/Node/`uv`, baut kein Cargo — und ruft mit `github-flow.sh`
bereits ad hoc ein `scripts/tests/*` auf. Zweiter Kandidat: eine `gate`-Zeile in
`check-merge-readiness.sh` (dann zählt die Gate-Wand im Showroom sie automatisch
mit — die Zahl steht an allen vier Stellen abgeleitet).

**Dritte Lücke, beim Reparieren gefunden:** `showroom/package.json` hat ein
`typecheck`-Skript (`tsc --noEmit`), das **kein Gate aufruft** —
`check-project-quality.sh --showroom` fährt `ci`, `lint`, `lint-contract`, `test`.
Der TypeScript-7-Bump wäre ungeprüft durchgelaufen.

## Zustand der Arbeitskopien

| | Reparatur | Aufgabe |
|---|---|---|
| Branch | `feature/showroom-survives-vite-8` | `feature/showroom-figures-derive-themselves` |
| Worktree | `…/reprise-showroom-survives-vite-8` | `…/reprise-showroom-figures-derive-themselves` |
| Basis | `origin/dev` @ `45e65480e9` | `origin/dev` @ `299ac24533` (älter) |
| Commits | 1 (`ce52a67e04`) | derselbe Fix als `bdda74095e`, sonst nichts |
| Gate | **läuft** | nicht gestartet |

Der Aufgaben-Worktree trägt den Fix nur, damit dort überhaupt grün gearbeitet
werden kann. **Sobald die Reparatur gelandet ist, auf das neue `origin/dev`
rebasen** — der Rebase wirft den doppelten Commit weg.

`wake-lock showroom-figures` wird gehalten → freigeben, wenn beides durch ist.

## Was Zeit spart

- **Der Lastregler liest den Kommandotext.** Steht `check-merge-readiness`,
  `cargo test` oder `codex-run` irgendwo in der Zeile, blockt der Hook auch ein
  harmloses `cat` oder ein Heredoc, das den String nur *enthält*. Dateien mit dem
  Write-Tool schreiben oder `HEAVY_RUN_DISABLE=1` voranstellen.
- **Der Scratchpad der Vorgängersitzung wird aufgeräumt.** Ein dort liegendes
  Gate-Log verschwindet mitten im Lauf. Läuft ein fremder Prozess weiter, hilft
  `tail -n +1 -f --pid=<pid> <log> > eigene-kopie` — `tail -f` folgt dem Inode
  und liest auch nach dem Löschen weiter.
- **Nach jedem Edit an Showroom-Tests `npx biome check --write tests/`.** Der
  `lint-contract`-Test führt `npm run lint` über das ganze Projekt aus; eine zu
  lange Zeile in einer Testdatei lässt ihn scheinbar grundlos scheitern.
- **Codex war am 19.08. ohne Kontingent** („try again at Aug 20th, 2026 7:26 AM").
  Der Nutzer hat entschieden, dass Opus im Hauptthread implementiert — das ist
  eine **Ausnahme**. Ab dem 20.08. 07:26 gilt wieder `/code`.
