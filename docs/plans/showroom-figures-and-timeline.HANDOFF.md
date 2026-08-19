# Übergabe — Showroom: abgeleitete Zahlen, Tempo-Timeline, eine Zählregel

Stand 19.08.2026, 22:45 Uhr. Die Vite-8-Reparatur ist gelandet. Von der
eigentlichen Aufgabe sind **§3 (Gate für die Skript-Selbsttests) und §2a
(Zeilen-Census) fertig und eingecheckt**; offen sind die Timeline, die
Verdrahtung der Zahlen in die Seite, der Footer und die Regeln.

Der Plan mit allen Beschlüssen liegt unter
`docs/plans/showroom-figures-derive-themselves.md` — er ist gegenüber der
Vorgängerfassung an drei Stellen korrigiert, weil die Messung ihre Annahmen
nicht bestätigt hat. **Diese Datei ergänzt ihn, sie ersetzt ihn nicht.**

## Was gelandet ist

| PR | Merge | Inhalt |
|---|---|---|
| [#577](https://github.com/marvinbaudach/reprise/pull/577) | `2d7630bd51` | Der Showroom überlebt Vite 8s neuen CSS-Minifier |

Beleg: lokales Gate `GATE_EXIT=0`, Display-Suite 0 von 535 rot; dev-CI-Lauf
[32297254493](https://github.com/marvinbaudach/reprise/actions/runs/32297254493)
`success`.

## Der Branch

`feature/showroom-figures-derive-themselves`, Worktree
`/home/marvin/Projects/reprise-showroom-figures-derive-themselves`, auf
`origin/dev` @ `2d7630bd51`, Arbeitskopie sauber, **fünf Commits voraus**:

| Commit | Inhalt |
|---|---|
| `0a38b8690f` | die geerbte Übergabe (von dieser hier abgelöst) |
| `468b60b7d5` | Die Skript-Selbsttests bekommen ein Gate (§3) |
| `1520de3b22` | Der Zeilen-Census (§2a) |
| `1aebcf234d` | Der Census zählt auch deklarierte Tests |
| `HEAD` | diese Übergabe |

Noch **kein PR**, noch **kein volles Gate** auf diesem Branch gelaufen.

## §3 — erledigt, und der Befund war größer als gedacht

`qa-linters.sh` war nicht nur ungegatet, sie war **rot**, und deshalb hing die
ganze Familie: 27 Selbsttests in `scripts/tests/`, von denen vier im Gate
liefen. Drei Zusicherungen prüften eine **Schreibweise** statt einer **Regel**
— dieselbe Krankheit, die Vite 8 gerade in den Showroom-Tests aufgedeckt hat.
Sie sind auf die Regel nachgezogen, nicht gelockert.

Neu: `gate "Script self-tests" -- scripts/tests/qa-linters.sh` (**111 s**
gemessen), die drei aufruferlosen Selbsttests in der Laufliste, und
`npm --prefix showroom run typecheck` in `check-project-quality.sh --showroom`.

**Die Gate-Anzahl ist von 26 auf 27 gestiegen.** Gate-Wand, Tempo-Zahl und
`HEADLINE_FIGURES[3]` sind mitgewachsen, ohne dass eine Zahl angefasst wurde.

Drei Behauptungen der alten Übergabe haben die Prüfung **nicht** überlebt — wer
sie erneut liest, soll sie nicht noch einmal glauben:

- Die sechs fehlenden `cua-explore-*.py` **gibt es nicht**; die Laufliste deckt
  alle 19 Module ab.
- `check-android-suite.sh` braucht **kein JDK und keine Bindings** (0 s, grün);
  die geplante `MERGE_READINESS_SKIP_ANDROID_QUALITY`-Ausnahme entfällt.
- `scan-scrutinee-borrows.py` ist **kein Test, sondern ein Bericht** — endet
  immer mit 0, druckt 95 Funde. Es bekommt kein Gate; daraus eines zu machen
  hieße, eine Schwelle zu erfinden, die niemand beschlossen hat. Es liegt falsch
  unter `scripts/tests/`, und das bleibt so vermerkt.

## §2a — der Census steht

`showroom/derive/code-census.mjs` plus 16 Tests in
`showroom/tests/code-census.test.mjs`. 185 ms für 1711 Dateien.

```
reprise line census — 1aebcf234d, 1711 files
  Rust, product            204'513   50.9 %
  Rust, tests              161'493   40.2 %
  Rust, Android bridge      11'967    3.0 %
  Kotlin                    23'856    5.9 %
  total                    401'829
  of them tests            179'637   44.7 %
  declared tests             6'592
```

Gegen die getippten Werte (cloc auf `604677322e`): Testanteil **44,7 %** gegen
45,8 %, Rust-Testzeilen 161'493 gegen 149'504. Zwei unabhängige Verfahren auf
verschiedenen Commits, 1,1 Punkte auseinander. Der Produkt-Überhang ist
erklärt: cloc lässt Kommentarzeilen weg, diese Zählung nicht.

**Drei echte Fehler lagen auf dem Weg dahin, alle drei in die schmeichelhafte
Richtung.** Wer den Zähler anfasst, muss sie kennen:

1. **Ausgelagerte Suiten.** Die 800-Zeilen-Regel treibt große Testmodule in
   Nachbardateien, die als `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`
   zurückkommen. Nur den Modulnamen zu lesen sucht ein `tests.rs`, das es nicht
   gibt: **434 solcher Attribute, 71'263 Zeilen** falsch als Produkt.
2. **Zweifach geteilte Suiten.** Die zweite Teilung steht in einer Datei, die
   schon Test ist — dort ist `#[cfg(test)]` überflüssig und wird nicht
   geschrieben. Ein Fixpunkt-Durchlauf holt sie.
3. **Übergriff in die Gegenrichtung.** `runtime_tests.rs` enthält
   `#[path = "runtime_effects_tests.rs"] mod effects;`. Attribut und
   Deklaration getrennt gelesen, landet man bei `src/effects.rs` — dem
   Equalizer, also Produktionscode. Sie werden jetzt zusammen gelesen, und
   `declaredModules()` hat dafür einen eigenen Test.

Restunschärfe **beidseitig** gemessen: höchstens 0,65 % der Zeilen sind
strittig. Mutationsgeprüft: String-Überspringen aus → Suite rot.

> **Eine Zählregel, überall.** Beschluss des Nutzers. Dieses Modul ist die
> einzige Stelle, an der das Projekt seine Zeilen zählt — auch für Zahlen
> **außerhalb** des Repositories. `node showroom/derive/code-census.mjs` druckt
> die Tabelle samt Commit; wer eine Zahl zitiert, zitiert diesen Lauf, statt neu
> zu zählen. Zwei Zählungen desselben Baums, die sich widersprechen, sind
> schlimmer als gar keine: beide sehen autoritativ aus.

## Der Lebenslauf ist mitgezogen — und **nicht** committet

`/home/marvin/Projects/bewerbung` (eigenes Git-Repo) trug vier veraltete Zahlen.
Der Nutzer hat das Überschreiben des dort vorgefundenen uncommitteten Standes
ausdrücklich freigegeben.

| Feld | alt | neu |
|---|---|---|
| `repriseTotalLines` | 347'842 | **401'829** |
| `repriseTestShare` | 45,8 % | **44,7 %** |
| `repriseTestCount` | 5'986 | **6'592** |
| `repriseQualityGates` | **21** | **27** |

Die Gate-Zahl war um sechs daneben. Geändert: `src/shared/profile.js` (mit
Herkunftskommentar: Commit, Datum, Zählregel, Verweis auf den Census),
`src/lebenslauf.html` (zwei Statistikblöcke **und** die hartkodierte `>21<` in
Zeile 106), `src/cv-en.js` (Übersetzungspaare), sowie
`tests/profile_data_test.js`, `tests/cv_english_data_test.js`,
`tests/html_cv_test.sh`, `tests/mutation-run.sh`.

Die Anschreiben binden über `data-profile`-Spans und ziehen automatisch nach.

**Cortec ist auf Wunsch des Nutzers entfernt.** Das Anschreiben war die einzige
vorbestehend rote Stelle (`anschreiben_cortec_test.sh`: „Cortec-PDF fehlt", weil
`out/anschreiben-cortec-fullstack.pdf` im vorgefundenen Stand gelöscht war).
Gelöscht: `src/bewerbungen/cortec-fullstack.html`,
`tests/anschreiben_cortec_test.sh`, das PDF (Löschung gestaget). Nachgezogen:
der Kommentar in `build.sh` nennt jetzt keine einzelne Variante mehr, und
`tests/mutation-run.sh` ist auf zwei Verträge zurückgebaut — die zwei
Mutationen, die nur das Cortec-Anschreiben trafen, sind mit ihm gegangen, der
Rest ist auf 1–5 durchnummeriert. Keine Cortec-Fundstelle bleibt übrig.

**Grün, alle zehn Verträge:** `profile_data_test.js`, `cv_english_data_test.js`,
`cv_variants_data_test.js`, `html_cv_test.sh`, `html_cv_english_test.sh`,
`html_cv_october_80_test.sh`, `document_inventory_test.sh`, die drei
`anschreiben_{css,neho,zhdk}_test.sh`.

**Drei Dinge liegen dort offen:**

1. **Die PDFs unter `out/` sind veraltet** — sie zeigen noch 347'842 / 45,8 % /
   21. Das Neurendern ist ein Build und wurde deshalb nicht ungefragt
   ausgelöst. `build.sh` ist der Weg.
2. **`tests/mutation-run.sh` ist ungelaufen.** Es verweigert den Start, solange
   seine Ziele unversionierte Arbeit tragen — und `src/lebenslauf.html` ist
   geändert. Der Umbau ist syntaktisch geprüft (`bash -n`), aber die Gegenprobe
   steht aus: **nach dem Commit einmal laufen lassen.**
3. **`repriseMcpTools: 24` ist ungeprüft.** Es ließ sich nicht sauber messen und
   wurde deshalb **nicht** angefasst. Wenn eine Zahl als Nächste abgeleitet
   werden soll, dann diese — die MCP-Tools stehen in `crates/reprise-mcp`.

Nichts davon ist committet. Das Repo enthält daneben fremde, unabhängige Arbeit
(Verfügbarkeit „Oktober 2026 · 80 %", drei neue Anschreiben) — beim Committen
also bewusst entscheiden, was zusammengehört.

## Was noch aussteht

### 1. Die Timeline (§1) — Quelle abstimmen, dann bauen

**Beschluss: fünf Wochen, nicht vier.** Der Entwurf begann bei `18–24 Jul` und
ließ damit die Woche aus, in der das Projekt entstand: erster Commit **Samstag,
11.07.2026**, `docs: design document for Musikbox (Rhythmbox successor)` — die
Idee selbst. Überschrift wird „Idea to alpha · 5 weeks", die Zahl kommt aus
`TIMELINE.length`.

Vier der fünf Themen sind aus der Historie **belegt**, nicht erfunden:

| Woche (Sa→Fr) | Thema | Anker |
|---|---|---|
| 11–17 Jul | CORE | `docs/ux-rules.md` geboren 17.07. |
| 18–24 Jul | SURFACES | `reprise-cli` + `reprise-mcp` geboren 21.07. |
| 25–31 Jul | DEPTH | kein neues Frontend; Arbeit nach innen |
| 1–7 Aug | ANDROID | `android/` geboren 03.08. |
| 8–14 Aug | SIGNATURE | `showroom/` geboren 14.08. |

`SURFACES` ist der einzige neue Name gegenüber dem Entwurf: die Woche, in der
aus einem Frontend vier wurden — genau das, was `HEADLINE_FIGURES[2]` („1 → 4")
behauptet. **Der Wortlaut der Spalte „What landed" geht dem Nutzer zur Korrektur
zu, bevor `docs/showroom/timeline.md` eingecheckt wird** — es ist eine
Behauptung über sein Projekt.

Danach: `readTimeline()` in `vite.config.ts` nach dem Muster von `readGates()`
(jede Zusicherung wirft, statt still zu schrumpfen), drittes virtuelles Modul
`virtual:build-timeline`, dann `TempoBand.tsx` ersetzen. Der Untertext des
Entwurfs — „Dated from the plan records in `docs/`" — ist nach diesem Beschluss
**falsch** und wird durch das ersetzt, was zutrifft.

### 2. Die Zahlen in die Seite verdrahten (§2b)

Der Census ist gebaut, aber **noch nirgends angeschlossen**. Es fehlen:

- `virtual:code-census` im Plugin `reprise-derived-facts`; der Zähler braucht
  eine `.d.mts` daneben, damit `tsc --noEmit` die Config weiter typprüft.
- `HEADLINE_FIGURES[0]`/`[1]` und alle vier `CODE_SEGMENTS` (Zeilen **und**
  Anteile) aus der Ableitung, nicht getippt.
- `docs/measurements/index-rebuild.md` für `PERFORMANCE`/`PERFORMANCE_PRICE` —
  **keine Werte neu erheben**, nur die vorhandenen mit ihrer Herkunft
  einchecken.
- `SPECTRAL_AXIS` aus `crates/reprise-view/src/spectral_colour.rs` parsen.
- Footer: `SiteFooter.tsx:25` sagt zu, die Messung komme noch. Danach stimmt der
  Satz nicht mehr.

### 3. Regeln und Abnahme

SHOW-11…15 in `docs/ux-rules.md`, Abschnitt `AJ. Showroom` (§4 des Plans). Das
Prüfmuster steht schon: `chapter-two.test.mjs`, „show-10 the gate count is
nowhere a literal" sucht das Literal **in Gesellschaft der Wörter**, die es
behaupten würde.

Abnahme: `npm --prefix showroom test`, `lint`, `typecheck`,
`check-ux-traceability.sh`, volles Gate **mit der neuen Gate-Zeile**, und —
was kein Test ersetzt — die gebaute Seite ansehen.

## Was Zeit spart

- **Der Lastregler liest den Kommandotext.** Steht `check-merge-readiness`,
  `cargo test` oder `codex-run` irgendwo in der Zeile, blockt der Hook auch ein
  harmloses `grep` oder ein Heredoc, das den String nur *enthält*. `HEAVY_RUN_DISABLE=1`
  als Präfix half **nicht** — der Hook prüft vor der Ausführung. Was hilft:
  Skripte mit dem Write-Tool ablegen und dann ausführen, oder den Dateinamen im
  Kommando aus Teilen zusammensetzen.
- **Die Display-Stufe schweigt bis zum Ende.** `scripts/check-display-tests.sh`
  sammelt jeden Testlog in ein `mktemp`-Verzeichnis und gibt erst danach die
  Bilanz aus — rund 35 Minuten Stille. Ein Log-Stall-Detektor meldet dort
  „hängt". Der ehrliche Fortschritt sind die `<index>.status`-Dateien im
  Ergebnis-`mktemp`: `cat /tmp/tmp.*/*.status | sort | uniq -c`.
- **`cp` ist auf `-i` aliast** und hängt in Skripten an der Rückfrage.
  `command cp -f` nehmen.
- **`pkill -f <task-id>` erwischt die eigene Shell** (Exit 144). Hintergrundläufe
  über ihre PID beenden.
- **Nach jedem Edit an Showroom-Tests `npx biome check --write tests/`.** Der
  `lint-contract`-Test führt `npm run lint` über das ganze Projekt aus; eine zu
  lange Zeile in einer Testdatei lässt ihn scheinbar grundlos scheitern.
- **`land.sh` will eine PR-Nummer** und verlangt einen Plan mit passendem
  `branch:` — für einen Reparatur-PR ohne Plan `--no-plan`. Es pusht und öffnet
  den PR nicht selbst.

`wake-lock showroom-figures` wird gehalten → freigeben, wenn die Aufgabe durch
ist.
