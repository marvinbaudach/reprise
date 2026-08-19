# Übergabe — Showroom-Welle: Galerie-Hover + Kapitel Zwei

Stand 19.08.2026, abends. **Beide Pläne sind implementiert, verifiziert und
fertig zum Landen.** Offen ist nur noch das Landen selbst — das bleibt beim
Nutzer.

## Zustand

| | Gallery | Kapitel Zwei |
|---|---|---|
| Plan | `docs/plans/gallery-hover-holds-the-frame-still.md` | `docs/plans/chapter-two-two-figures.md` |
| Branch | `feature/gallery-hover-holds-the-frame-still` | `feature/chapter-two-two-figures` |
| Worktree | `/home/marvin/Projects/reprise-gallery-hover-holds-the-frame-still` | `/home/marvin/Projects/reprise-chapter-two-two-figures` |
| Basis | `origin/dev` @ `7bb1a3c433` | **der Gallery-Branch**, nicht dev |
| Commits | 8 (inkl. Refactor) | 5 darüber |
| `phase` | `reviewed` | `planned` (Statuszeile noch nicht nachgezogen) |
| Gate | **grün** (`GATE_EXIT=0`) | **muss neu laufen** — siehe unten |

`wake-lock gallery-hover` wird noch gehalten → `wake-lock release gallery-hover`,
wenn die Welle abgeschlossen ist.

## Das Kapitel-Zwei-Gate steht noch aus

Der Lauf vom 19.08. wurde nach 36 Stufen **von außen abgebrochen** — `SIGTERM`
mitten in den Linux-Platform-Tests, beim `gnome_conformance`-Binary
(`GATE_EXIT=143`, „Terminated"). Das ist **kein** fehlgeschlagener Test, und die
Ursache ist ungeklärt: weder ein Testfehler noch eine Meldung im Log, nur der
Abbruch. Alles davor war grün.

Vor dem PR also einmal sauber durchfahren:

```
export ANDROID_HOME=/home/marvin/.local/share/android-sdk
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
export MERGE_READINESS_SKIP_ANDROID_QUALITY=1
MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch
```

Detachen (`setsid nohup … > log 2>&1 &`) und **nachsehen, dass der Prozess
wirklich lebt** — ein Lauf, der gar nicht startet, sieht genauso aus wie einer,
der noch läuft. Wiederholt er den Abbruch an derselben Stelle, ist das ein
eigener Befund und nichts, was dieser Branch verursacht hat: er fasst kein
Rust-Crate an.

## Landen (Reihenfolge zwingend)

1. `land.sh` für **Gallery**.
2. Kapitel-Zwei-Worktree auf das neue `origin/dev` rebasen. Er sitzt aktuell auf
   dem Gallery-Branch, nicht auf dev — der Rebase wirft die dann gelandeten
   Gallery-Commits weg und behält die fünf eigenen.
3. `land.sh` für **Kapitel Zwei**.
4. Danach einmal `scripts/check-ux-traceability.sh` — das ist der Moment, in dem
   SHOW-1…5 und SHOW-6…10 sich erstmals im selben Baum sehen.

## Warum Codex nichts davon gebaut hat

Codex' Kontingent war am 19.08. erschöpft („You've hit your usage limit … try
again at Aug 20th, 2026 7:26 AM"). Der Nutzer hat entschieden, dass Opus im
Hauptthread implementiert. Das ist eine **Ausnahme**, keine neue Regel — siehe
`feedback-codex-does-the-coding`. Ab dem 20.08. 07:26 gilt wieder der normale
Weg über `/code`.

## Gallery — was drinsteht

Hover ist reines CSS: vier Custom Properties, `:hover` in `@media (hover: hover)`,
`:focus-visible` daneben, Bildzoom auf einem neuen `.shot-tile__picture`-Wickel
um Bild **und** `children` (sonst bliebe der Visualizer-Canvas der Hero-Platte
stehen). Sheen, Kippung, Beschreibung sind weg; der **Ladeschimmer `sweep`
bleibt**. Neu: Lupenzeichen (Phosphor, Lizenz in `LICENSES/PHOSPHOR-MIT.txt`).

Zusätzlich: SHOW-1…5 im Regelbuch, `[web]`-Ebene, `.mjs`-Quelle im
Traceability-Skript, `npm --prefix showroom test` im PR-Gate, Textkürzung der
Spectral-Seek-Karte.

**Abweichungen vom Plan (bewusst):** `showcase.css` kam dazu — dort lagen vier
weitere Regeln mit `var(--mx)`, ohne die SHOW-2 schlicht falsch wäre. Und die
Nachziehung von `shot-tile-lightbox.test.mjs` liegt im Bauteil-Commit, damit
jeder Commit einzeln grün ist.

**Messung** steht in §7 des Plans: 11/11 Platten halten Höhe und Breite unter
echtem Zeiger.

## Kapitel Zwei — was drinsteht

`gate "<Name>" -- <Befehl>` im Gate-Skript (26 Prüfungen, Vorbedingungen zählen
nicht mit) · `docs/agents/pipeline.md` als zitierbare Quelle · Vite-Plugin mit
zwei virtuellen Modulen · `AgentSwimlane` (echte `<table>`) · `GateWall` mit
klickbaren Zellen und `src/lib/mergeGates.ts` als reiner Logik · Gate-Zahl an
allen vier Stellen abgeleitet · SHOW-6…10 samt Tests.

`@types/node` ist neu in `showroom/package.json` — `vite.config.ts` liest jetzt
Dateien.

**Belegt:** Klick → `Merge blocked · 3 of 26 failing`, leeren → wieder bereit ·
Tastatur: Tab wandert, Leertaste kippt `aria-pressed`, Live-Region meldet ·
reduzierte Bewegung: Marken und Zellen sofort auf `matrix(1,0,0,1,0,0)`, Delay 0s.

## Was beim nächsten Mal Zeit spart

- **Headless-Chromium meldet `hover: none`.** `@media (hover: hover)` greift dann
  nie, und `CSS.forcePseudoState('hover')` bleibt wirkungslos. Zwei Messrunden
  gingen dafür drauf. Start braucht
  `--blink-settings=primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4`,
  und Hover per `Input.dispatchMouseEvent`. Immer einen **Kontrollarm** mitlesen.
  Als Erinnerung gesichert: `headless-chromium-reports-hover-none`.
- **Nicht scrollen, Layout verschieben** — sonst sind alle Schnappschüsse
  schwarz (`browser-screenshots-go-black-after-scrolling`).
- **Frischer Worktree schafft die Android-Gate-Stufe nicht.** Die
  UniFFI-Kotlin-Bindings sind generiert und gitignored; `android/local.properties`
  fehlt ebenfalls. Für Branches, die Android nicht anfassen:
  `MERGE_READINESS_SKIP_ANDROID_QUALITY=1` — und das im Bericht **sagen**.
  Erinnerung: `fresh-worktree-cannot-pass-the-android-gate-stage`.
- **`cp` ist auf `cp -i` aliased.** Restores in Mutationsproben laufen sonst in
  die Rückfrage und finden nicht statt — die Mutationen stapeln sich, und die
  Probe ist wertlos. `/bin/cp -f` nehmen.
- **Der Lastregler blockt am Kommando*text*.** Steht `cargo test`,
  `codex-run` oder `check-merge-readiness` drin, greift der Hook auch bei einem
  harmlosen `cat`. Dateien mit dem Write-Tool schreiben oder den String trennen.
- **Der Scratchpad wird während der Sitzung aufgeräumt.** Ein dort abgelegtes
  Gate-Skript war weg, bevor `heavy-run` es startete — der Lauf sah aus, als
  liefe er, und hat nie stattgefunden. Nach dem Start prüfen, dass der Prozess
  wirklich lebt.

## Offene Punkte, die keiner der Pläne abdeckt

- **`measurements.ts` ist weiterhin handgezählt.** Kapitel Zwei hat nur die
  Gate-Zahl abgeleitet. `HEADLINE_FIGURES`, `CODE_SEGMENTS` und `PERFORMANCE`
  bleiben ungeprüft — eigene Aufgabe wert. Drei der vier Zahlen im gestrichenen
  Stat-Raster waren falsch; dieselbe Sorte Zahl steht noch in Kapitel Eins.
- **`scripts/tests/project-quality.sh` läuft in keinem Gate.** Der Contract-Test
  war nach der Gate-Verdrahtung rot und ist im Gallery-Branch nachgezogen — aber
  gemerkt hat das ein Reviewer, kein Gate.
- **`feature/showroom-plate-plays-the-visualizer`** liegt noch als Worktree
  herum; Inhalt scheint bereits auf `dev`. Vor dem Aufräumen prüfen.
