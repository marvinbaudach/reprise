---
slug: showroom-figures-derive-themselves
worktree: /home/marvin/Projects/reprise-showroom-figures-derive-themselves
branch: feature/showroom-figures-derive-themselves
phase: shipped
codex_session:
created: 2026-08-19
---

# Der Showroom leitet seine Zahlen ab — und bekommt eine Tempo-Timeline

> Erhoben gegen `origin/dev` @ `299ac24533` (19.08.2026).
>
> **Setzt auf `showroom-survives-vite-8` auf.** Jener Branch bringt
> `build.cssTarget` und die sieben entschärften Testdateien. Dieser Worktree
> trägt den Fix vorerst als `bdda74095e`; **vor dem ersten Handgriff auf das
> gelandete `origin/dev` rebasen** — der Rebase wirft den doppelten Commit weg.

**Ziel.** Der Footer sagt heute selbst: „This build does not measure them yet —
the figures are typed into one module and the measurement runs in CI next."
Danach stimmt das nicht mehr. Jede Zahl, die die Seite ausspricht, kommt aus dem
Repository: gezählt zur Bauzeit oder aus einem eingecheckten Messprotokoll
zitiert. Und die vier Statistikzeilen der Tempo-Spur weichen einer Timeline, die
die fünf Wochen benennt, die es gebraucht hat.

**Was heute abgeleitet ist:** genau eine Zahl — die Gate-Anzahl, über das
Vite-Plugin `reprise-derived-facts` und `virtual:merge-gates`. SHOW-10 hält den
Rahmen bereits: die Zahl darf in keiner `.tsx` und nicht in `measurements.ts`
als Literal neben den Wörtern stehen, die sie behauptet. **Dieses Muster trägt
alles, was hier dazukommt.**

## Beschlüsse des Nutzers (19.08., vor dem Plan)

1. **Timeline-Quelle:** eine bewusst eingecheckte Datei, nicht das
   Plan-Frontmatter und nicht die Git-Historie. Begründung unter §1.
2. **Umfang der Ableitung:** Zeilenzahlen zählt der Build aus dem Baum;
   Performance-Werte kommen als eingechecktes Messprotokoll ins Repo.
3. **Gate für die Skript-Selbsttests:** eine `gate`-Zeile in
   `scripts/check-merge-readiness.sh`, nicht der CI-Job `base-contracts`.
   Nebenwirkung ist erwünscht: die Gate-Wand im Showroom zählt die neue
   Prüfung automatisch mit.

## §1 Die Tempo-Timeline

### Warum eine eingecheckte Quelle

`docs/plans/README.md` schreibt vor, dass Pläne mit `phase: shipped` und alle
`*.HANDOFF*.md` gelöscht werden, sobald die Arbeit gelandet ist. Eine zur
Bauzeit gezählte Plananzahl **schrumpft also mit jedem Aufräumen** — sie wäre
eine Zahl, die lügt, sobald das Repository gepflegt wird. Dazu kommt: von 89
Plänen tragen nur 58 ein `created:`. Die Git-Historie wüsste es genau, steht in
CI mit `fetch-depth: 1` aber nicht zur Verfügung.

Also eine Quelle, die genau das ist, was sie behauptet: ein festgeschriebenes
Protokoll der fünf Wochen.

### Die Quelle

Neu: `docs/showroom/timeline.md`. Eine Tabelle, nach dem Vorbild von
`docs/agents/pipeline.md`, das der Build heute schon liest.

**Die Spanne ist fünf Wochen, nicht vier** (Beschluss des Nutzers, 19.08.). Der
Entwurf begann bei `18–24 Jul` und ließ damit die Woche aus, in der das Projekt
entstand: der erste Commit ist Samstag, **11.07.2026**, `docs: design document
for Musikbox (Rhythmbox successor)` — die Idee selbst. Die Wochen laufen Sa→Fr
ab diesem Tag; die Überschrift lautet „Idea to alpha · 5 weeks", und die Zahl
kommt aus `TIMELINE.length`.

| Week | Span | Theme | Anker in der Historie |
|---|---|---|---|
| 1 | 2026-07-11 … 2026-07-17 | CORE | `docs/ux-rules.md` geboren 17.07. |
| 2 | 2026-07-18 … 2026-07-24 | SURFACES | `reprise-cli` + `reprise-mcp` geboren 21.07. |
| 3 | 2026-07-25 … 2026-07-31 | DEPTH | kein neues Frontend; Arbeit nach innen |
| 4 | 2026-08-01 … 2026-08-07 | ANDROID | `android/` geboren 03.08. |
| 5 | 2026-08-08 … 2026-08-14 | SIGNATURE | `showroom/` geboren 14.08. |

Die Wochen tragen ISO-Daten; die Anzeigeform (`11–17 Jul`) rechnet der Build
daraus aus, damit die Datumszeile nicht zweimal gepflegt wird.

**Vier der fünf Themen sind belegt, nicht erfunden** — die Anker oben stammen
aus `git log --diff-filter=A` im lokalen Klon, wo die Historie vollständig ist.
`SURFACES` ist der einzige neue Name gegenüber dem Entwurf: es ist die Woche, in
der aus einem Frontend vier wurden, also genau die Woche, die
`HEADLINE_FIGURES[2]` („1 → 4") behauptet. Nur `DEPTH` trägt keinen
Geburtstermin, sondern eine Abwesenheit: die einzige Woche ohne neue Oberfläche.
**Der Wortlaut ist abgestimmt** (Nutzer, 19.08., 22:52 Uhr). Die Spalte „What
landed" ist eine Behauptung über sein Projekt und ging ihm deshalb vor dem
Einchecken zur Korrektur zu; er hat sie unverändert freigegeben. Das ist der
Inhalt, der nach `docs/showroom/timeline.md` geht — Wortlaut nicht neu erfinden:

| Week | Span | Theme | What landed |
|---|---|---|---|
| 1 | 2026-07-11 … 2026-07-17 | CORE | The idea, the workspace split into `reprise-core` and a Linux platform layer, and the UX rulebook that has governed every change since. |
| 2 | 2026-07-18 … 2026-07-24 | SURFACES | One frontend became four: `reprise-cli`, `reprise-mcp` and `reprise-stems` joined the GNOME app. |
| 3 | 2026-07-25 … 2026-07-31 | DEPTH | No new surface. The single-owner runtime, its versioned protocol and its client went in underneath the ones that already existed. |
| 4 | 2026-08-01 … 2026-08-07 | ANDROID | The shared presentation layer, then the FFI bridge and the Android app on top of it — the library running on a phone. |
| 5 | 2026-08-08 … 2026-08-14 | SIGNATURE | The GNOME conformance rulebook, and the showroom itself: a prerendered page that reads its own numbers out of the tree. |

Die Spanne der Wochen ist **gegen `origin/dev` mit Autor-Daten** nachgemessen,
nicht mit Committer-Daten: Rebases und Squash-Merges schreiben letztere um, ein
`git log --since` darauf zählt Wochen falsch zu. Commits je Woche (ohne Merges):
955 · 735 · 409 · 142 · 246. Zusätzliche Anker über den Entwurf hinaus:
`reprise-runtime-protocol` und `reprise-runtime` geboren 28.07.,
`reprise-runtime-client` 29.07. (Woche 3), `reprise-view` 02.08. (Woche 4, die
Voraussetzung für Android), das GNOME-Regelwerk 12.08. (Woche 5).

### Der Leser

`readTimeline()` in `showroom/vite.config.ts`, drittes virtuelles Modul
`virtual:build-timeline`, exakt nach dem Muster von `readGates()`:

- Zeilen der Tabelle über einen Ausdruck einsammeln, Kopf- und Trennzeile
  auslassen.
- **Jede Zusicherung wirft, statt still zu schrumpfen.** Leere Ableitung → Fehler
  (`readGates()` begründet das: „the one failure that would look like success").
  Dazu: Datum nicht ISO-parsbar → Fehler; Wochen nicht aufsteigend → Fehler;
  Lücke oder Überlappung zwischen zwei Wochen → Fehler.
- `configureServer` nimmt die Datei in den Watcher auf — sie liegt nicht unter
  der Vite-Wurzel.

### Die Komponente

`TempoBand.tsx` wird **ersetzt**, nicht ergänzt: statt der vier Statistikzeilen
eine Timeline aus fünf Wochenkarten mit Punkt-Linie und Mono-Datumszeile.
Überschrift „Idea to alpha" bleibt, die Zahl darunter kommt aus
`TIMELINE.length` — die „5" ist damit selbst abgeleitet und kann nicht mehr von
der Quelle abweichen.

- Der Screenshot des Nutzers ist **Richtungsgeber, nicht pixelgenaue Vorlage**.
  Rhythmus, Farbe, Hairlines und Reveal-Verhalten aus den vorhandenen Tokens und
  den Kapitelmustern ableiten; die Spur bleibt auf ihrem eigenen Grund
  (`data-ground`) und zwischen zwei Hairlines, damit sie Zäsur bleibt.
- `prefers-reduced-motion: reduce` muss die Punkt-Linie genauso stilllegen wie
  SHOW-9 es für Marken und Zellen verlangt.
- Der Untertext des Entwurfs — „Dated from the plan records in `docs/`" — ist
  nach diesem Beschluss **falsch** und wird ersetzt durch das, was zutrifft:
  die Timeline zitiert `docs/showroom/timeline.md`, verlinkt über `permalink()`.

**Was dabei nicht verloren geht:** die vier Statistikzeilen doppeln
`HEADLINE_FIGURES` aus Kapitel Eins. Die Zahlen bleiben der Seite also erhalten;
die Tempo-Spur hört nur auf, sie ein zweites Mal zu sagen.

## §2 Die Zahlen zählen sich selbst

### Zeilen: `showroom/derive/code-census.ts`

Nicht in `vite.config.ts`: der Zähler bekommt ein eigenes Modul, weil er das
einzige Stück hier ist, das eine Sprache lesen muss.

- **Rust:** alle `crates/**/*.rs`. Produkt gegen Test trennen über Dateien unter
  einem `tests/`-Verzeichnis (ganz Test) und `#[cfg(test)] mod … { … }`-Blöcke
  innerhalb einer Datei (nur der Block).
- **Der Klammernzähler ist die Risikostelle.** Ein naives Zählen von `{` und `}`
  verzählt sich an Klammern in String-Literalen, Zeichen-Literalen,
  Zeilenkommentaren, Blockkommentaren (in Rust **schachtelbar**) und
  Raw-Strings (`r#"…"#`). Der Scanner überspringt diese fünf Fälle, und **er
  bekommt eigene Tests mit genau diesen Fixtures** — sonst verschiebt eine
  geschweifte Klammer in einem Fehlertext lautlos 20'000 Zeilen von Test nach
  Produkt.
- **Android-Brücke:** `crates/reprise-android-ffi` bleibt ein eigenes Segment.
- **Kotlin:** `android/**/*.kt` und `*.kts`.
- **Gezählt werden nicht-leere Zeilen.** Das ist die einfachste Regel, die sich
  in einem Satz belegen lässt; sie steht im Doc-Kommentar des Moduls und in der
  `detail`-Zeile der Figur.

**Die Zahlen werden sich ändern.** Die heutigen Werte stammen aus `cloc 2.08`
auf `604677322e` und lassen Kommentarzeilen weg; diese Zählung tut das nicht.
Das ist kein Fehler, sondern der Punkt: ab jetzt sagt die Seite, was der Build
im Baum vorgefunden hat. `HEADLINE_FIGURES[0]`, `[1]` und alle vier
`CODE_SEGMENTS` (Zeilen **und** Anteile) folgen daraus; die Anteile rechnet der
Build, sie werden nicht getippt.

`HEADLINE_FIGURES[2]` „1 → 4" folgt aus den vorhandenen Frontends und bleibt
eine Aussage über Architektur, keine Messung — sie bleibt getippt, mit dem
Beleg in `detail`.

### Performance: ein eingechecktes Messprotokoll

`PERFORMANCE` und `PERFORMANCE_PRICE` sind **nicht ableitbar** — es sind
Vorher/Nachher-Werte eines einmaligen Umbaus, kein Zustand des Baums. Sie
verlassen trotzdem die `.ts`-Datei:

- Neu: `docs/measurements/index-rebuild.md` — die vier Messreihen plus den
  Preis-Absatz, jede Zeile mit **Commit, Datum und Messverfahren**.
- Der Build liest sie über dasselbe Plugin (`virtual:measurements`).
- Es werden **keine Werte neu erhoben**. Was ins Protokoll geht, sind exakt die
  Werte, die heute in `measurements.ts` stehen, mit der Herkunft, die ihnen
  bisher fehlte. Wer sie später nachmisst, ändert eine Datei — nicht eine
  Komponente.

### `SPECTRAL_AXIS`

Die beiden Hex-Werte sind von Hand aus
`crates/reprise-view/src/spectral_colour.rs` kopiert und damit driftfähig. Der
Build parst die Datei und leitet sie ab — die billigste Ableitung im ganzen
Plan, weil die Quelle schon verlinkt ist.

### Der Footer

`SiteFooter.tsx:25` sagt der Leserschaft heute zu, die Messung komme noch. Nach
diesem Plan stimmt der Satz nicht mehr und wird durch das ersetzt, was gilt:
welche Zahlen der Build zählt, welche er aus einem Protokoll zitiert, und welche
eine Aussage statt einer Messung sind. `BASELINE.commit` verliert seine Rolle
als Herkunft der Zeilenzahlen und behält sie nur noch für die Permalinks.

## §3 Die Skript-Selbsttests bekommen ein Gate — **erledigt, gemessen**

**Der Befund war schlimmer als die Übergabe wusste:** `qa-linters.sh` war nicht
nur ungegatet, sie war **rot**. Zwei ihrer Policy-Muster verlangten noch die
alte Aufrufform des Merge-Gates (`^scripts/check-project-quality.sh$` und
`^scripts/check-display-tests.sh --rule-named$`), das inzwischen auf die Form
`gate "<Name>" -- <Befehl>` umgebaut ist. Ein drittes, in `cua-explore.sh`,
verlangte die Schreibweise `XDG_DATA_DIRS="$stub_root:`, während `session.sh`
den Wert längst über eine Zwischenvariable setzt. **Alle drei prüften eine
Schreibweise statt einer Regel** — dieselbe Krankheit, die Vite 8 in den
Showroom-Tests aufgedeckt hat. Sie sind auf die Regel nachgezogen, nicht
gelockert.

**Was geschnitten wurde:**

1. Neue Zeile `gate "Script self-tests" -- scripts/tests/qa-linters.sh`. Damit
   laufen ihre Laufliste und ihre Policy-Muster bei jedem Merge.
2. Die drei aufruferlosen Selbsttests kamen in die Laufliste:
   `architecture-size-limits.sh`, `cua-explore.sh`, `check-android-suite.sh`.
3. `npm --prefix showroom run typecheck` kam in
   `scripts/check-project-quality.sh --showroom`. Der Selbsttest
   `scripts/tests/project-quality.sh` hat die Änderung sofort gemeldet — seine
   erwartete Befehlsfolge ist mitgezogen. Genau so soll es sein.

**Drei Annahmen der Übergabe halten nicht — gemessen, nicht geglaubt:**

- **Die sechs fehlenden `cua-explore-*.py` gibt es nicht.** Die Laufliste von
  `cua-explore.sh` deckt alle 19 Module ab; der Abgleich Verzeichnis gegen
  Laufliste ist leer.
- **`check-android-suite.sh` braucht kein JDK und keine Bindings.** Es ist ein
  Parser-Selbsttest auf `mktemp`-Fixtures, 0 s, grün. Die geplante Ausnahme
  `MERGE_READINESS_SKIP_ANDROID_QUALITY` ist unnötig.
- **`scan-scrutinee-borrows.py` ist kein Test, sondern ein Bericht.** Es endet
  immer mit 0 und druckt 95 Funde. Es bekommt **keinen** Gate-Aufruf: ein Gate
  daraus zu machen hieße, eine Schwelle zu erfinden, die niemand beschlossen
  hat. Es liegt falsch — unter `scripts/tests/`, obwohl es ein Werkzeug ist —
  und das bleibt hier so vermerkt statt stillschweigend behoben.

**Laufzeit, gemessen statt geschätzt:** `qa-linters.sh` braucht **111 s**
(84 s vor den drei Ergänzungen). Gegen ein Gate von ~40 Minuten ist das
bezahlbar; nichts wurde ausgelassen, nichts stillschweigend gedeckelt.

**Doppelt läuft:** `worktree-gc.sh`, `worktree-gc-schedule.sh` und
`check-architecture.sh` haben eigene `gate`-Zeilen **und** stehen in der
Laufliste von `qa-linters.sh`. Das bleibt so: `check-release.sh` erreicht sie
nur über die Laufliste, und die eigenen Zeilen sind von `qa-linters.sh` selbst
als Muster zugesichert. Der Preis sind wenige Sekunden.

Die sieben verwaisten `scripts/*.sh` (`check-spectrogram-smoke.sh`,
`playback-history-smoke.sh`, `render-showroom-seek-track.sh`,
`render-showroom-visualizer.sh`, `repro-track-list-fresh-start.sh`,
`shoot-updates-popover.sh`, `verify-radio-favicons.sh`) sind **Werkzeuge, keine
Tests** — sie bekommen keinen Gate-Aufruf. `scripts/check-shell.sh` lintet sie
bereits; das ist die richtige Tiefe.

**Nebenwirkung, gewollt und eingetreten:** die Gate-Anzahl ist von **26 auf 27**
gestiegen. Gate-Wand, Tempo-Zahl und `HEADLINE_FIGURES[3]` sind mitgewachsen,
ohne dass eine Zahl angefasst wurde — alle drei lesen dieselbe Ableitung.

**Beleg:** `scripts/tests/qa-linters.sh` → exit 0, 111 s ·
`npm --prefix showroom run typecheck` → 0 ·
`npm --prefix showroom test` → 63 von 63 grün.

## §4 Regeln

Neu in `docs/ux-rules.md`, Abschnitt `AJ. Showroom`, jede mit Test:

- **SHOW-11** [web] — Die Tempo-Timeline benennt die Wochen aus einem
  eingecheckten Protokoll. Weder ein Wochenname, noch eine Datumsspanne, noch
  ihre Anzahl steht als Literal in einer `.tsx`.
- **SHOW-12** [web] — Keine Zeilenzahl und kein Anteil steht als Literal neben
  den Wörtern, die sie behaupten; die Seite liest sie aus der Zählung des Baums.
- **SHOW-13** [web] — Die Performance-Werte zitieren ein eingechecktes
  Messprotokoll mit Commit und Datum; keiner steht als Literal in einer `.tsx`.
- **SHOW-14** [web] — Der Footer benennt für jede Zahlengruppe, ob sie gezählt,
  zitiert oder behauptet ist. Er behauptet nicht mehr, die Messung stehe aus.
- **SHOW-15** [web] — Unter `prefers-reduced-motion: reduce` steht die
  Punkt-Linie der Timeline still.

Das Prüfmuster steht schon: `chapter-two.test.mjs` „show-10 the gate count is
nowhere a literal" sucht das Literal **in Gesellschaft der Wörter**, die es
behaupten würde, und prüft, dass die anzeigenden Dateien die Ableitung
importieren. SHOW-11 bis SHOW-13 erben es unverändert.

## §5 Abnahme

- `npm --prefix showroom test` — die 63 bestehenden Tests plus die neuen, alle
  grün. **Nach jedem Edit an Testdateien `npx biome check --write tests/`**, sonst
  fällt `lint-contract` scheinbar grundlos.
- `npm --prefix showroom run lint` → 0, `run typecheck` → 0.
- `scripts/check-ux-traceability.sh` — SHOW-11…15 abgedeckt.
- Das volle Merge-Gate grün, **mit der neuen Gate-Zeile darin**.
- **Ein Beleg, den kein Test ersetzt:** die gebaute Seite ansehen. Die
  Timeline ist eine Entwurfsarbeit; dass sie grün ist, sagt nichts darüber, ob
  sie trägt.

> **Die Display-Stufe schweigt bis zum Ende.** `scripts/check-display-tests.sh`
> sammelt jeden Testlog in ein `mktemp`-Verzeichnis und gibt erst danach die
> Bilanz aus — 531 Tests seriell, rund 35 Minuten Stille. Ein Log-Stall-Detektor
> meldet dort „hängt". Der ehrliche Fortschritt sind die `<index>.status`-Dateien
> im Ergebnis-`mktemp`.

## §6 Reihenfolge

**Stand 19.08., 22:52 Uhr — Schritte 1 bis 3a sind erledigt und committet.**
Was der Branch schon trägt, steht in
`docs/plans/showroom-figures-and-timeline.HANDOFF.md`; dort stehen auch drei
Annahmen, die die Messung widerlegt hat. **Nichts davon neu bauen.**

1. ~~Rebase auf das gelandete `origin/dev`~~ — erledigt.
2. ~~§3 Gate für die Skript-Selbsttests~~ — erledigt, `468b60b7d5`. Die
   Gate-Anzahl steht jetzt bei 27.
3. **§2 Zeilen-Census** — der Zähler selbst ist erledigt (`1520de3b22`,
   `1aebcf234d`): `showroom/derive/code-census.mjs` plus 16 Tests. **Offen ist
   die Verdrahtung** (`virtual:code-census` samt `.d.mts`,
   `HEADLINE_FIGURES[0]`/`[1]`, die vier `CODE_SEGMENTS`), danach
   `docs/measurements/index-rebuild.md`, `SPECTRAL_AXIS`, dann der Footer.
4. **§1 Timeline** — die Quelle ist belegt und ihr Wortlaut ist abgestimmt (die
   Tabelle in §1). Also: `docs/showroom/timeline.md` einchecken, dann
   `readTimeline()` und `virtual:build-timeline`, dann `TempoBand.tsx` ersetzen.
5. Regeln und Abnahme.
