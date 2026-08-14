# Handover: shellcheck als Gate ohne Ausnahmenliste

Stand: **2026-08-14**. Der Plan `docs/plans/shellcheck.md` ist vollständig
umgesetzt, reviewt und nachgebessert. `phase: refactored`. Es fehlt nur noch das
Landen.

- **Zweig:** `feature/shellcheck`
- **Worktree:** `/home/marvin/Projects/reprise-shellcheck`
- **Basis:** `origin/dev` @ `7360373ac5` — am 14.08. daraufhin rebast,
  konfliktfrei, Gate danach erneut grün (siehe „Vor dem Landen")
- **Umfang:** 28 Dateien, +125/−43, kein Push

## Was der Zweig tut

Das Repo prüfte Rust mit Clippy auf `-D warnings`, dazu Architektur,
Barrierefreiheit, Motion-Tokens, AppStream und Flatpak mit eigenen Gates — aber
die inzwischen 100 getrackten Shell-Skripte, die genau diese Gates *ausführen*,
prüfte niemand. Der Zweig schließt diese Lücke, ohne eine einzige Baseline- oder
Ausnahmedatei einzuführen.

Sechs Commits, in dieser Reihenfolge:

| Commit | Welle | Inhalt |
|---|---|---|
| `ac6ac366b8` | 0 | `.shellcheckrc` (`external-sources=true`, `source-path=SCRIPTDIR`) — löst 29 × SC1091 und 9 × SC2034 auf einen Schlag |
| `53fdfc9a42` | 1 | die 54 Warnungen aus 17 Dateien, überwiegend Einzeiler |
| `e2b2540634` | 2a | Unit-Fix: `--scope ~/…` → `%h/…` (systemd expandiert `~` nicht) |
| `e921d1bfef` | 2b | die sechs blinden Zusicherungen, `refute`-Helfer, **Mutationsprotokoll in der Commit-Beschreibung** |
| `1f1b7da1c3` | 3 | `scripts/check-shell.sh` plus Verdrahtung |
| `c6e09fe901` | — | die drei angenommenen Review-Befunde |

### Der eigentliche Anlass

Sechs Zeilen in `scripts/tests/worktree-gc.sh` und
`scripts/tests/worktree-gc-schedule.sh` begannen mit `! kommando`. Unter
`set -euo pipefail` schaltet bash `errexit` für eine mit `!` negierte
Kommandoliste ausdrücklich ab — **diese sechs Zusicherungen konnten nicht rot
werden**, ausgerechnet in den Testskripten, die die Lösch-Logik für Worktrees und
Zweige absichern. Eine davon verbietet namentlich, dass der GC-Timer wieder auf
den Pfad zeigt, der monatelang ins Leere lief, bis `.worktrees` auf 162 GB
gewachsen war.

Ersetzt durch:

```bash
refute() { if "$@"; then printf 'refute failed: %s\n' "$*" >&2; exit 1; fi; }
```

Nebengewinn gegenüber shellchecks eigenem Vorschlag `kommando && exit 1`: die
gerissene Zusicherung sagt beim Abbruch, welche es war. Vorher sagte sie nichts.

### Das Gate

`scripts/check-shell.sh` fährt zwei Läufe über `git ls-files -z '*.sh' '.githooks/*'`:

```
shellcheck -x -P SCRIPTDIR -S warning -f gcc -- "${files[@]}"
shellcheck -x -P SCRIPTDIR -S style -i SC2251,SC2004,SC2181 -f gcc -- "${files[@]}"
```

Der zweite **muss** `-S style` fahren, nicht `-S info` — SC2004 und SC2181 liegen
auf `style` und wären sonst stumm. Grund für den zweiten Lauf überhaupt: *alle*
elf echten Defekte dieses Vorhabens lagen unter `warning`; ein Gate nur auf
`-S warning` hätte die eigenen Funde ab dem Tag nach der Reparatur wieder
erlaubt. Die Dreierliste ist eine **Einschluss**liste — die Umkehrung einer
Baseline: sie kann nur strenger machen, und wenn sie verrottet, ist das Gate so
streng wie heute, nicht laxer.

Dritter Teil: die Begründungspflicht. Jedes `# shellcheck disable=` braucht einen
Grund in derselben oder der vorhergehenden Zeile, sonst rot. Präzedenzfall im
Repo ist GP-20 in `check-ai-hygiene.sh` für `#[allow(dead_code)]`. Bei heute drei
Ausnahmen wirkt das überdimensioniert; es ist aber der einzige Mechanismus, der
„keine Ausnahmenliste" über die Zeit trägt.

Verdrahtet in `scripts/check-merge-readiness.sh` direkt nach `git diff --check`
— läuft damit in CI über `ci-quality.sh` **und** lokal über `.githooks/pre-push`.
Dort neu eingehängt sind außerdem `scripts/tests/worktree-gc.sh` und
`scripts/tests/worktree-gc-schedule.sh` (+77 s), die vorher in **keinem** Gate
liefen. `shellcheck` steht jetzt in der pacman-Liste von `.github/workflows/ci.yml`.

## Nachweislage

Alles hier ist **selbst nachgemessen**, nicht aus Codex' Berichten übernommen —
Codex hat seine Berichte zweimal am Ende auf eine Zusammenfassung gekürzt, die
auf die Datei verweist, die sie selbst ist. Der einzige vollständige Codex-Beleg
ist das Mutationsprotokoll in der Beschreibung von `e921d1bfef`.

| Zusicherung | Ergebnis |
|---|---|
| `-S warning` über 100 Dateien | leer, Exit 0 |
| `-S style -i SC2251,SC2004,SC2181` | leer, Exit 0 |
| `scripts/check-shell.sh` | Exit 0, `ShellCheck 0.11.0: checking 100 tracked shell files` |
| ohne shellcheck im `PATH` | `SKIPPED: shellcheck is not installed; this gate did not run`, Exit 0 |
| `bash scripts/tests/worktree-gc.sh` | Exit 0 (voller 77-s-Lauf) |
| `bash scripts/tests/worktree-gc-schedule.sh` | Exit 0 — **war auf `dev` rot** |
| `bash -n` über alle 100 | fehlerfrei |
| `scripts/ptr-e2e/harness-self-test.sh` | Exit 0 |
| `git grep 'shellcheck disable'` | genau 3, jeder begründet |
| `systemd-analyze verify` auf die Unit | leer — vorher `path is not absolute, ignoring` |

### Die Rot-Proben

Ein Gate, das nur grün war, ist nicht bewiesen. Jede Probe einbauen, rot sehen,
zurückdrehen:

| Mutation | Ergebnis |
|---|---|
| unbenutzte Variable | Exit 1, SC2034 |
| `! true` in einem Testskript | Exit 1, SC2251 — **der Selbstbeweis der Einschlussliste** |
| `disable` ohne Begründung | Exit 1, Datei:Zeile genannt |
| `./-probe.sh` mit unbegründetem `disable` | Exit 1 (Befund M1, siehe unten) |
| `%h` → `~` in `ConditionPathIsDirectory` | Exit 1, der neue Wächter reißt |
| Schedule-Zeile aus der Gate-Kette gelöscht | Exit 1, `must contain policy pattern` |

### Die Kernprobe, unabhängig nachgestellt

Dieselbe Verfälschung an `worktree-gc-schedule.sh:19` (Zusicherung auf
`Type=oneshot`, das im Unit steht):

- alte `!`-Form → **Exit 0, „Worktree GC schedule: OK"** — die gerissene
  Zusicherung geht durch
- neue `refute`-Form → **Exit 1, `refute failed: rg -Fq Type=oneshot …service`**

Das ist der Kernbefund des ganzen Vorhabens, in beide Richtungen gemessen.

## Der Review und was er gefunden hat

Vier Reviewer parallel (drei generisch Sonnet/high, einer `security-reviewer`),
je auf einen eigenen Ausschnitt der 28 Dateien. Zwei Befunde wurden von je zwei
Reviewern unabhängig gefunden.

**Angenommen und in `c6e09fe901` behoben:**

- **M1** — `check-shell.sh:33`, `grep` ohne `--`. Eine getrackte Datei mit
  führendem Bindestrich im Wurzelverzeichnis (`-x.sh`) wird als Optionsbündel
  gelesen: `-x -. -s -h`; `-s` schluckt die Fehlermeldung, `-h` den Dateinamen,
  grep liest stdin. Exit 1 **ohne jede Ausgabe** → `unexplained=0` → beliebig
  viele unbegründete `disable` passieren. Genau die Kontrolle, die das Gate
  einführt, fiel aus. Fix: `grep -n --`, dazu `sed -n … --`.
- **M2** — `reprise-worktree-gc.service:3,4`. Dieselbe `~`-Ursache, die Welle 2
  in `ExecStart` behebt, stand eine Zeile darüber unverändert in
  `ConditionPathIsDirectory`. systemd **verwirft** die Bedingung („path is not
  absolute, ignoring"), statt sie als nie erfüllt zu werten — der Dienst lief
  ungegated. Fix: beide auf `%h`, plus eine Zusicherung darauf in
  `worktree-gc-schedule.sh`, die die Zeile vorher gar nicht prüfte.
- **M3** — `qa-linters.sh:90`. `require_pattern 'worktree-gc.sh'` deckt
  `worktree-gc-schedule.sh` nicht ab, der Teilstring bricht an `-schedule`. Die
  zweite neu verdrahtete Gate-Zeile war unbewacht. Fix: zweites
  `require_pattern`.

**Bewusst abgelehnt — nicht übersehen, sondern entschieden:**

- `require_pattern 'shellcheck' .github/workflows/ci.yml` besteht auch gegen eine
  bloße Kommentarzeile mit dem Wort. Plankonform umgesetzt; die Schwäche steht im
  Plan (E13), nicht in der Ausführung.
- `verify-radio-favicons.sh:43` — zwischen `mkdir -p` und dem `chmod 0700` fünf
  Zeilen später existiert das Verzeichnis kurz mit umask-Rechten. Endzustand
  identisch, Verzeichnis in dem Fenster noch leer.
- `scripts/ptr-e2e/harness-self-test.sh` 644 → 755 **bleibt**. Ungeplant von
  Codex eingebracht, aber stimmig (`run.sh` ist ebenfalls 755, die übrigen
  ptr-e2e-Dateien sind gesourcte Bibliotheken). Kein Konsument hängt daran.
- Die Begründungs-Regex verlangt eine eigenständige Kommentarzeile mit Leerzeichen
  nach `#`. Fail-safe-Richtung, für die drei realen Fälle irrelevant.

## Offene Punkte

**Die `require_*`-Wächter laufen in keinem PR.** `scripts/tests/qa-linters.sh`
wird ausschließlich von `scripts/check-release.sh` aufgerufen, und das ist laut
`RELEASING.md`/`TESTING.md` der manuelle Release-Kandidaten-Lauf — nicht in CI,
nicht im Pre-Push-Hook, nicht in `check-merge-readiness.sh`. E13 begründet sein
`require_pattern` damit, „dass niemand das Paket entfernt und das Gate lautlos
zum Dauerskip wird"; diese Absicherung feuert erst zum Release. Als **eigenes
Vorhaben** entschieden, nicht in diesem Zweig — die Wächter sollen richtig sein,
unabhängig davon, wann sie laufen. Gehört in die PR-Beschreibung.

**`qa-linters.sh` endet mit 1**, wegen `README.md must remain a concise developer
entry point` (195 Zeilen gegen Limit 170). Geprüft: `README.md` ist byteidentisch
mit `origin/dev` und mit der Merge-Basis. Vorbestehend, nicht von diesem Zweig.
Berührt das Landen nicht, weil `qa-linters.sh` in der Gate-Kette gar nicht hängt.

**Nicht Gegenstand geblieben** (bewusst, nach E7 „keine Reparatur ohne Wächter
dahinter"): SC2015 in `check-logo-artwork.sh`, SC2086 in `meson-cargo-build.sh`
und `performance-runtime-baseline.sh`. Ebenso die tiefere Scope-Schwäche der
Worktree-GC (Geschwister-Worktrees gelten als `outside_scope`, `dirty` blockt die
target-Löschung) — offener Punkt seit dem 11.08., eigenes Vorhaben. Und keine
Linter für die Python-Skripte unter `scripts/`.

## Vor dem Landen

**Erledigt am 14.08.:** Der Zweig ist auf `origin/dev` @ `7360373ac5` rebast —
neun Commits, konfliktfrei, keine Dateiüberschneidung. Danach erneut gemessen und
grün: 100 Dateien, beide Schwellen leer, `check-shell.sh` Exit 0, beide gc-Tests
Exit 0, `bash -n` fehlerfrei, `harness-self-test.sh` Exit 0, drei `disable`,
Arbeitsbaum sauber.

Das war die eine echte Landerisiko-Frage, und sie ist beantwortet: dieser Zweig
führt ein Gate ein, das **alle** getrackten Shell-Dateien prüft — auch die, die er
nie angefasst hat. Ein fremder Commit kann es also röten, ohne mit ihm etwas zu tun
zu haben. `dev` hatte in diesem Fenster genau eine Shell-Datei angefasst
(`scripts/check-frontend-thinness.sh`, nur geändert), und der Showroom-Commit
`f2ba57cbd7` brachte keine neuen Skripte mit.

**Bewegt sich `dev` erneut, bevor gelandet wird, gilt dasselbe von vorn:**

```bash
cd /home/marvin/Projects/reprise-shellcheck
mapfile -d '' files < <(git ls-files -z '*.sh' '.githooks/*')   # bash, nicht zsh
shellcheck -x -P SCRIPTDIR -S warning -f gcc -- "${files[@]}"
shellcheck -x -P SCRIPTDIR -S style -i SC2251,SC2004,SC2181 -f gcc -- "${files[@]}"
./scripts/check-shell.sh
```

Das ist der Preis für echte Analyse statt Blindflug und im Plan unter „Risiken"
so vorgesehen.

Landen dann wie üblich: PR eröffnen, `scripts/land.sh <pr>` — **nicht** auf CI
warten, `dev` bewegt sich schneller als der 45-Minuten-Lauf und GitHub verweigert
den Merge dann aus einem veralteten Mergeability-Cache. `land.sh` findet über
`^branch: feature/shellcheck$` selbst den Plan, setzt `phase: shipped` und
committet ihn in den Feature-PR. Danach den `dev`-Lauf beobachten und
vorwärtsreparieren, falls er rot wird.

Diese Handover-Datei trägt bewusst **keinen** Statusblock — sonst fände `land.sh`
zwei Pläne für einen Zweig und verweigerte den Dienst.

## Fallen, die diese Runde gekostet haben

- **`heavy-run medium`, nicht `heavy`.** Der Lastregler-Hook liest den
  Kommandotext und blockt `codex-run.sh` als schweren Einstiegspunkt; die
  `heavy`-Klasse verhungert an fremden Läufen. Außerdem frisst `heavy-run` die
  stderr des Kindes — Codex' Fortschritt landet nirgends, nur der Ergebnispfad.
- **`codex-run.sh` abkoppeln** (`setsid nohup`), sonst schneidet der
  Harness-Timeout den Lauf ab. Ein Monitor auf `kill -0 $PID` ist der Wecker.
- **`.pipeline-codex.md` ist in diesem Repo nicht von `.gitignore` gedeckt** —
  Codex muss ausdrücklich angewiesen werden, sie nicht zu committen.
- **`mapfile` gibt es in zsh nicht.** Jeder Gate-Nachbau von Hand braucht
  `bash -c '…'`.
- **`cp` ist interaktiv aliasiert** — in Skripten `command cp` oder `git checkout --`
  benutzen, sonst hängt die Wiederherstellung nach einer Rot-Probe und die
  nächste Probe misst den verfälschten Stand mit.
- **`pkill -f '<muster>'` matcht sich selbst**, wenn das Muster im eigenen
  Kommandotext steht.
- **Codex kürzt seinen Bericht am Ende** und verweist auf „den vollständigen
  Bericht" in genau der Datei, die er gerade gekürzt hat. Nachweise selbst
  erheben, nicht aus dem Bericht abschreiben.
