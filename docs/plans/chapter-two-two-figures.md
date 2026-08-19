---
slug: chapter-two-two-figures
worktree: /home/marvin/Projects/reprise-chapter-two-two-figures
branch: feature/chapter-two-two-figures
phase: shipped
codex_session:
created: 2026-08-19
---

# Kapitel Zwei: zwei Figuren statt fünf Blöcken

> Erhoben gegen `origin/dev` @ `bdd339e6ad` (19.08.2026).
>
> **Setzt auf `gallery-hover-holds-the-frame-still` auf.** Jener Branch bringt
> die Ebene `[web]`, die `.mjs`-Quelle in `check-ux-traceability.sh`, den
> Abschnitt `AJ. Showroom` und `npm --prefix showroom test` im Gate. Dieser
> Plan erbt das alles und ergänzt nur. Vor dem Codex-Lauf auf das gelandete
> `origin/dev` rebasen (§8).

**Ziel.** Kapitel Zwei ist heute ein Inventar aus fünf Blöcken. Danach trägt es
zwei Figuren und einen Schlussabsatz: eine **Swimlane**, die jeden der sieben
Schritte dem Akteur zuordnet, der ihn ausführt, und eine **Gate-Wand**, die die
Prüfungen zeigt, die `check-merge-readiness.sh` wirklich fährt — anklickbar,
damit ein Besucher die Fail-Closed-Regel testet statt sie zu glauben.

---

## 1. Was heute dasteht

`showroom/src/components/chapters/ChapterTwo.tsx` (80 Zeilen):

| Zeilen | Block | Schicksal |
|--------|-------|-----------|
| 16–21 | Eyebrow „CH.02" + Titel | bleibt |
| 23 | `<AgentWorkflow />` — sieben Schritte als Kartenraster | → Swimlane (§3) |
| 25–32 | Vorspann der Beweisleiter | **weg** |
| 34–55 | `<ol className="rungs">` — fünf Sprossen | **weg** |
| 57–62 | Überleitung „The top two rungs are not scripts…" | **weg** |
| 64 | `<ExplorationLoop />` | **weg** |
| 66–68 | Zwischenüberschrift „The rulebook, and how much…" | **weg** |
| 70 | `<FigureGrid figures={RULEBOOK_FIGURES} …>` — Stat-Raster | **weg** |
| 72–76 | Schlussabsatz Traceability | bleibt, ergänzt (§6) |
| — | neu: Gate-Wand | §4 |

`showroom/src/components/process/AgentWorkflow.tsx` (62 Zeilen) hält die sieben
Schritte als Inline-Array `WORKFLOW` (Zeile 4–16), rendert sie als
`<ol className="agent-workflow__path">` und darunter einen Block
`agent-workflow__independence` mit drei Sätzen („The writer never reviews.",
„The reviewer never writes.", „The skeptic cannot apply findings.").
`ExplorationLoop.tsx` (66 Zeilen) und die gemeinsame `process.css` gehören
ebenfalls dazu.

---

## 2. Drei Befunde, die den Auftrag ändern

### 2.1 Die zu kodierende Invariante steht andersherum

Der Auftrag verlangt: *„Plan und Implement teilen sich einen Akteur, Refactor
nicht"* und *„der Akteur, der einen Befund anwendet, ist nie der, der den Code
schrieb."*

Die Pipeline sagt das Gegenteil. `~/.claude/skills/pipeline/SKILL.md`,
Phase *refactor*, Schritt 2:

> „Take the accepted findings … and hand them back to **Codex** in the same
> worktree, via `codex-run.sh <worktree> <prompt-file> "<the findings>"` …
> Codex implements; Opus plans, reviews and verifies."

Also: **Codex fährt 03 Implement und 06 Refactor**, im selben Worktree, über
dieselbe `codex_session`. **Opus fährt 01 Plan** — Plan und Implement teilen
sich gerade *keinen* Akteur.

Die Trennung, die wirklich hält, verläuft nicht zwischen Schreiber und
Nachbesserer, sondern zwischen **Schreiben und Urteilen**: kein Akteur, der
schreibt, urteilt je; kein Akteur, der urteilt, schreibt je. Genau das zeigt
die Swimlane (§3), und genau das prüft der geforderte Test — er geht
unverändert durch.

### 2.2 Die Prozessdefinition liegt nicht im Repo

Kein Dokument im Repository beschreibt diese Rollen.
`grep -ciE 'skeptic|refute|reviewer agent|author agent|refactor agent'
docs/ux-rules.md` → **0**. `AGENTS.md:99` sagt nur:

> „Recommended: after each task, do (or dispatch) an adversarial review of the
> diff before moving on"

— optional, und „do (or dispatch)" erlaubt ausdrücklich, dass derselbe Agent
sich selbst reviewt. Die heutige Seite behauptet dagegen „A fresh agent reviews
each language" und „The writer never reviews" (`AgentWorkflow.tsx:12,47-49`).
Die Behauptung ist richtig — aber ihre Quelle liegt in einem unversionierten
Skill außerhalb des Repos, während die Seite sonst alles mit Repo-Artefakten
belegt.

Deshalb entsteht **`docs/agents/pipeline.md`** (§3.1): die Phasentabelle als
zitierbare Quelle, aus der die Swimlane ihre Lanes ableitet.

### 2.3 Drei der vier Stat-Zahlen sind falsch

| `RULEBOOK_FIGURES` behauptet | tatsächlich |
|---|---|
| `571` active UX rules | **384** (`grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[active\]'`) |
| `897` files holding those tests | **527** |
| `250` files under docs/ | **359** |
| `100 %` of enforceable rules have a test | **stimmt** — erzwungen von `check-ux-traceability.sh` |

Das Raster fällt ohnehin weg. Der Befund entscheidet nur, was in den
Schlussabsatz darf: die einzige Aussage, die ein Gate erzwingt (§6). Die drei
Handzählungen kommen nicht mit — sie sind schon einmal verrottet.

---

## 3. Die Swimlane

Ersetzt `AgentWorkflow.tsx` durch `AgentSwimlane.tsx`. Zeilen = Akteure,
Spalten = die sieben Schritte, eine Marke je Schritt:

```
                 01   02   03   04   05   06   07
Human             ·    ■    ·    ·    ·    ·    ·     challenges
Opus              ■    ·    ·    ·    ·    ·    ·     drafts
Codex             ·    ·    ■    ·    ·    ■    ·     writes · applies
Reviewer          ·    ·    ·    ■    ·    ·    ·     reviews
Skeptic           ·    ·    ·    ·    ■    ·    ·     refutes
Gates             ·    ·    ·    ·    ·    ·    ■     decides
```

Die leeren Zellen sind der Inhalt: keine Zeile deckt einen schreibenden **und**
einen urteilenden Schritt ab, und die menschliche Zeile trägt genau eine Marke
— vor dem ersten Code.

Marken erscheinen beim Hereinscrollen in Schrittreihenfolge, damit die Übergaben
links nach rechts lesbar werden (`transition-delay` nach Spaltenindex, kein
Timer).

**Layout.** Unterhalb ihrer natürlichen Breite scrollt das Raster waagerecht
statt umzubrechen: `overflow-x: auto` am Wickel, `min-width: max-content` am
Raster. Die Beschriftungsspalte bleibt stehen — `position: sticky; left: 0` auf
den Zellen der ersten Spalte, mit eigenem Hintergrund, damit Marken darunter
durchlaufen.

**Semantik.** Die Swimlane ist eine Tabelle und wird als solche ausgezeichnet:
`<table>` mit `<caption>`, Akteur als `<th scope="row">`, Schritt als
`<th scope="col">`. Marken tragen Text (den Verb), keine reinen Symbole; die
leeren Zellen bleiben leer. Damit liest ein Screenreader die Zuordnung, statt
ein Raster aus `<div>`s zu buchstabieren.

Die drei Unabhängigkeitssätze aus `agent-workflow__independence` bleiben als
Bildunterschrift der Figur — sie sind die Behauptung, die das Raster belegt.

### 3.1 `docs/agents/pipeline.md` als Quelle

Neues Dokument mit genau einer maschinenlesbaren Tabelle:

```markdown
| Step | Phase      | Actor    | Writes | Judges |
|------|------------|----------|--------|--------|
| 01   | Plan       | Opus     | no     | no     |
| 02   | Checkpoint | Human    | no     | yes    |
| 03   | Implement  | Codex    | yes    | no     |
| 04   | Review     | Reviewer | no     | yes    |
| 05   | Refute     | Skeptic  | no     | yes    |
| 06   | Refactor   | Codex    | yes    | no     |
| 07   | Gate       | Gates    | no     | yes    |
```

Dazu die Prosa, die §2.1 festhält: dass Codex 03 und 06 fährt, dass die
Trennung zwischen Schreiben und Urteilen verläuft, und dass der Skill in
`~/.claude/skills/pipeline/` die ausführende Fassung ist.

Die Swimlane liest diese Tabelle zur Bauzeit (§4.2, zweites virtuelles Modul),
sodass eine Änderung an der Pipeline die Seite ändert — oder den Test rot
macht.

---

## 4. Die Gate-Wand

### 4.1 Das Gate-Skript bekommt einen `gate`-Helfer

`scripts/check-merge-readiness.sh` mischt heute `scripts/*.sh`,
`cargo`-Aufrufe, ein `env … cargo test`, ein `dbus-run-session …`, ein
`git diff --check` und einen `case` mit zwei `check-project-quality.sh`-Zweigen,
die zusammen **einen** Check bilden. Ein Regex darüber muss raten. Stattdessen:

```bash
gate() {
  local name=$1
  shift
  [[ ${1:-} == -- ]] && shift
  echo "== $name =="
  "$@"
}
```

Jeder Prüfschritt wird ein `gate "<Name>" -- <Befehl>`. Die zwei Fälle, die kein
einfacher Befehl sind, bekommen je eine Funktion davor:

```bash
quality_cmd=(scripts/check-project-quality.sh)
case "${MERGE_READINESS_SKIP_ANDROID_QUALITY:-}" in
  1 | true)
    echo "Skipping the Android area here; it runs in the android-unit-suite job."
    quality_cmd=(scripts/check-project-quality.sh --project --showroom)
    ;;
esac

run_audit() {
  if ! cargo audit; then
    echo "live advisory refresh unavailable; checking the cached database" >&2
    cargo audit --no-fetch
  fi
}
```

Die 26 Namen, in Skriptreihenfolge:

```
Branch diff · Shell · Project quality · Worktree GC · Worktree GC schedule ·
Gettext catalogues · Architecture · Device-sync GStreamer ·
Accessibility semantics · Input parity · Runtime service install ·
Frontend thinness · UX traceability · AppStream · Flatpak manifest ·
GNOME idioms · AI hygiene · Motion tokens · Rust formatting · Rust lint ·
Rust documentation · Workspace tests · Linux platform tests ·
Rule-owned display tests · Runtime service bus tests · Dependency audit
```

Die Vorbereitungsschritte (Basis-Ref auffrischen, sauberer Worktree,
Stale-Branch-Prüfung, `mktemp`) bleiben ohne `gate` — sie sind Vorbedingungen,
keine Prüfungen. Genau deshalb ergibt die Ableitung 26 und nicht mehr.

`scripts/check-shell.sh` fährt shellcheck über die Skripte: die umgebaute Datei
muss dort sauber bleiben (`"$@"` statt `$*`, Array für `quality_cmd`).

### 4.2 Ableitung zur Bauzeit

`showroom/vite.config.ts` bekommt ein Plugin mit zwei virtuellen Modulen:

```ts
// virtual:merge-gates   → export const GATES: readonly string[]
[...text.matchAll(/^gate "([^"]+)"/gm)].map((m) => m[1])

// virtual:agent-pipeline → export const PIPELINE: readonly Step[]
// (die eine Tabelle aus docs/agents/pipeline.md, Zeile für Zeile)
```

Beide werfen beim Build, wenn die Ableitung leer bleibt — eine stumme leere
Liste wäre die eine Fehlerart, die als „alles grün" durchginge.

Dazu `showroom/src/virtual-modules.d.ts` mit den Moduldeklarationen, sonst
scheitert `npm run typecheck`. Die Pfade zeigen relativ auf `../scripts/` und
`../docs/`; beide liegen im Checkout, lokal wie in CI.

Im Dev-Server werden beide Quelldateien per `server.watcher.add` beobachtet,
damit eine Änderung am Gate-Skript die Seite neu lädt.

### 4.3 Die Wand selbst

Ein Raster aus 26 Zellen, benannt und in Skriptreihenfolge, die beim
Hereinscrollen der Reihe nach auf grün springen; darunter ein Readout.

**Zellen sind Buttons.** Ein Klick lässt die Prüfung fehlschlagen:
`aria-pressed` spiegelt den Fehlerzustand, der Name bleibt lesbar, die Farbe
wechselt. Das Readout darunter ist `aria-live="polite"` und `role="status"`:

- keine rote Zelle → „Ready to merge · 26 checks green"
- mindestens eine → „Merge blocked · 3 of 26 failing"

Ein zweiter Klick räumt die Zelle wieder frei; das Readout kehrt zurück. Das ist
der Beweis der Sektion: Fail-Closed wird etwas, das ein Besucher auslöst.

**Die Logik ist rein und liegt neben der Komponente.** `showroom/src/lib/mergeGates.ts`:

```ts
export function readout(failed: ReadonlySet<string>, total: number): Readout
```

Der Prüfstand hat kein DOM (alle Tests sind statische Analyse plus
Unit-Tests gegen `src/lib/*.ts`). Das Klickverhalten ist deshalb nur dann
prüfbar, wenn der Zustandsübergang als reine Funktion existiert — die Komponente
bleibt eine dünne Hülle darum. Dasselbe Muster fahren `seekClock.ts` und
`reveal.ts` bereits.

---

## 5. Die eine Zahl

Heute steht die Gate-Zahl an drei Stellen, jede für sich getippt:

| Stelle | heute |
|--------|-------|
| `showroom/src/data/measurements.ts:55` | `value: '21'` (Chapter One über `HEADLINE_FIGURES`) |
| `showroom/src/components/chapters/TempoBand.tsx:39` | JSX-Literal `21` |
| `showroom/src/components/process/AgentWorkflow.tsx:15` | `'21 checks decide whether it can land'` |

Alle drei ziehen künftig aus `GATES.length`. Der Text von Schritt 07 wird
`` `${GATES.length} checks decide whether it can land` ``, die Tempo-Band-Zahl
und die Kennzahl in `measurements.ts` ebenso — letztere wird damit vom Literal
zum abgeleiteten Feld.

Damit kann sich die Seite nicht mehr selbst widersprechen, und ein neues Gate im
Skript ändert alle vier Anzeigen zugleich.

---

## 6. Der Schlussabsatz

Bleibt, ergänzt um die eine Aussage aus dem gestrichenen Raster, die ein Gate
erzwingt — als Nebensatz, nicht als Zahl:

> A rule ID leads to a test, the test to a commit, the commit to the decision.
> The traceability is itself a merge gate: every enforceable rule has a test of
> the same name — not as a count anyone tallied, but because a rule without one
> fails the build, and so does a test pointing at a rule that no longer exists.

`571`, `897` und `250` kommen nicht mit (§2.3). `RULEBOOK_FIGURES` und
`VERIFICATION_RUNGS` fallen aus `measurements.ts` heraus, wenn kein anderer
Aufrufer sie hält — vor dem Löschen greppen.

---

## 7. Die neuen UX-Regeln

Angehängt an den Abschnitt **`AJ. Showroom (public site)`**, den der
Gallery-Branch anlegt — gleiches Präfix gehört in einen Abschnitt, IDs sind
append-only:

```markdown
- **SHOW-6** [active] [web] — Die Gate-Wand nennt die Prüfungen, die das
  Gate-Skript wirklich fährt, in Skriptreihenfolge; Liste und angezeigte Zahl
  stammen aus derselben Ableitung aus dem Skript.
- **SHOW-7** [active] [web] — Keine Bahn der Pipeline-Figur trägt Marken in
  einem schreibenden und einem urteilenden Schritt; die menschliche Bahn trägt
  genau eine Marke.
- **SHOW-8** [active] [web] — Eine fehlgeschlagene Gate-Zelle sperrt das
  Readout und nennt die Zahl der roten Prüfungen; sind alle frei, steht es
  wieder auf bereit.
- **SHOW-9** [active] [web] — Bei `prefers-reduced-motion: reduce` erscheinen
  Marken und Gate-Zellen im Endzustand, ohne Sequenz.
- **SHOW-10** [active] [web] — Die Gate-Zahl steht an keiner Stelle als
  Literal; jede Anzeige stammt aus der Ableitung.
```

### Die Tests

Neue Datei `showroom/tests/chapter-two.test.mjs`, Namen führen die ID.

- **`show-6`** — die Namen aus `scripts/check-merge-readiness.sh` unabhängig neu
  ableiten (derselbe `^gate "…"`-Ausdruck), dann: jeder Name steht im
  gebauten HTML, die Reihenfolge im HTML entspricht der Skriptreihenfolge, und
  die angezeigte Zahl gleicht der Länge der Ableitung. Fügt jemand ein Gate
  hinzu, ohne die Seite zu bauen, wird der Test rot — das ist sein Zweck.
- **`show-7`** — die Tabelle aus `docs/agents/pipeline.md` lesen; für jeden
  Akteur prüfen, dass er nicht in einem `Writes: yes`- und einem
  `Judges: yes`-Schritt steht, und dass `Human` genau einen Schritt hat.
  Zusätzlich: jede Bahn und jede Marke des Dokuments erscheint im HTML.
- **`show-8`** — Unit-Test gegen `src/lib/mergeGates.ts`: leeres Fehlerset →
  bereit; ein Eintrag → blockiert, Zahl 1; drei Einträge → Zahl 3; alles
  entfernt → wieder bereit.
- **`show-9`** — im Block hinter `prefers-reduced-motion:reduce` tragen Marken
  und Gate-Zellen weder `transition` noch `transition-delay`, und ihr
  Endzustand ist unbedingt gesetzt.
- **`show-10`** — kein Literal der Gate-Zahl unter `showroom/src/`: die drei
  Aufrufstellen importieren aus dem abgeleiteten Modul.

### Bestehende Tests, die nachziehen

- **`agent-process.test.mjs`** — beide Tests beziehen sich auf die alten
  Figuren. Der Workflow-Test wird auf die Swimlane umgeschrieben (die
  `data-role`-Prüfungen wandern auf die Bahnen), der ExplorationLoop-Test
  fällt ersatzlos weg.
- **`chapter-design.test.mjs`** — der Chapter-Two-Test prüft die fünf
  Sprossenzahlen und die fünf `RULEBOOK_FIGURES`-Werte; alle zehn
  Behauptungen fallen weg. Der Chapter-One-Teil prüft `21` als
  `data-counter` — wird auf die abgeleitete Zahl umgestellt.
- **`page-contract.test.mjs`** — nennt `ch-02` nur als Anker, bleibt.

---

## 8. Abhängigkeit und Reihenfolge

Dieser Branch **setzt auf `gallery-hover-holds-the-frame-still` auf**. Beide
fassen `docs/ux-rules.md` am Dateiende und `check-ux-traceability.sh` an;
parallel gäbe das einen sicheren Konflikt und zwei konkurrierende Fassungen
derselben Skriptänderung.

1. Gallery landet (`land.sh`) — bringt `[web]`, die `.mjs`-Quelle, Abschnitt AJ
   und `npm --prefix showroom test` im Gate.
2. Diesen Worktree auf das neue `origin/dev` rebasen.
3. Erst dann Codex starten.

Wird vorher gestartet, fehlt die `[web]`-Ebene und `check-ux-traceability.sh`
kann SHOW-6…10 gar nicht sehen — das Gate wäre rot, ohne dass etwas falsch wäre.

**Arbeitsreihenfolge im Branch:**

1. `scripts/check-merge-readiness.sh` auf `gate` umstellen (§4.1), shellcheck grün.
2. `docs/agents/pipeline.md` schreiben (§3.1).
3. Vite-Plugin + `virtual-modules.d.ts` (§4.2).
4. `src/lib/mergeGates.ts` (§4.3).
5. `AgentSwimlane.tsx` + `GateWall.tsx` + CSS; `AgentWorkflow.tsx` und
   `ExplorationLoop.tsx` löschen.
6. `ChapterTwo.tsx` auf zwei Figuren + Schlussabsatz kürzen (§1, §6).
7. Die drei Zahl-Aufrufstellen auf `GATES.length` (§5).
8. `docs/ux-rules.md`: SHOW-6…10 an AJ anhängen (§7).
9. Tests: neu und nachgezogen (§7).
10. Grün: `npm --prefix showroom run lint`, `… typecheck`, `… test`,
    `scripts/check-shell.sh`, `scripts/check-ux-traceability.sh`.

Schritte 8 und 9 gehören in **einen** Commit — Regel und gleichnamiger Test
zusammen, wie das Regelbuch es in Zeile 13 verlangt.

---

## 9. Abnahme

**Gate.** `MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch`
grün vor dem PR — und dieser Lauf prüft sich hier selbst mit: das umgebaute
Skript ist Gegenstand der Änderung.

**Zahl.** Die Ableitung liefert 26; die Seite zeigt an allen vier Stellen 26.

**Sichtprüfung.** Swimlane in Ruhe und nach dem Hereinscrollen; Gate-Wand grün,
dann mit drei angeklickten roten Zellen samt Readout „Merge blocked". Dazu ein
Lauf mit erzwungener reduzierter Bewegung.

**Tastatur.** Durch die Gate-Wand tabben, eine Zelle mit Leertaste umschalten,
prüfen dass das Readout angesagt wird und `aria-pressed` mitgeht.

---

## 10. Fallen

- **Die Invariante des Auftrags ist falsch herum** — wer sie ungeprüft zeichnet,
  stellt eine unbelegte Behauptung auf eine Seite, deren Argument
  Nachprüfbarkeit ist (§2.1).
- **26 ist nicht „alle Zeilen mit `scripts/`"** — die Vorbedingungen zählen
  nicht mit, und der `case`-Zweig ist ein Check, nicht zwei (§4.1).
- **Eine leere Ableitung muss werfen.** Ein Tippfehler im Regex ergäbe sonst
  eine Wand aus null Zellen und die Zahl 0 — grün, still, falsch (§4.2).
- **Das Klickverhalten ist ohne reine Funktion nicht prüfbar** — der Prüfstand
  hat kein DOM (§4.3).
- **`RULEBOOK_FIGURES`/`VERIFICATION_RUNGS` vor dem Löschen greppen** — sie
  könnten anderswo hängen (§6).
- **Das Gate-Skript prüft sich selbst.** Ein Fehler im `gate`-Helfer bricht
  jeden weiteren Lauf; nach dem Umbau zuerst `bash -n` und
  `scripts/check-shell.sh`, bevor der volle Lauf gestartet wird.

---

## Parallelität

**Nicht geschnitten. Ein Strang.**

Der Schnitt scheiterte an derselben Stelle wie beim Gallery-Plan: Regel und
gleichnamiger Test müssen zusammen landen, und die Gate-Wand kann ohne den
`gate`-Umbau im Skript nicht ableiten. Die Kette

  Skript (§4.1) → Ableitung (§4.2) → Wand (§4.3) → Zahl (§5) → Test (§7)

ist eine Reihe von Vorbedingungen, keine unabhängigen Gruppen. Ein Strang für
die Swimlane wäre denkbar — er teilt sich mit der Wand aber `ChapterTwo.tsx`,
`process.css` und `docs/ux-rules.md`, also drei Dateien. Kein disjunkter Schnitt.

**Merge-Reihenfolge:** entfällt innerhalb des Plans. **Nach außen** gilt §8:
`gallery-hover-holds-the-frame-still` landet zuerst.
**Nach-Merge-Gegenprüfungen:** keine strangübergreifenden. Nach dem Rebase auf
das gelandete `origin/dev` einmal `scripts/check-ux-traceability.sh` fahren —
das ist der Punkt, an dem SHOW-1…5 und SHOW-6…10 sich erstmals im selben Baum
sehen.
