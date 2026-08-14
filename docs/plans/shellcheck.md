---
slug: shellcheck
worktree: /home/marvin/Projects/reprise-shellcheck
branch: feature/shellcheck
phase: planned
codex_session:
created: 2026-08-14
---
# shellcheck als Gate ohne Ausnahmenliste

> Alle Zahlen mit shellcheck 0.11.0 gegen `origin/dev` @ `a6a0d11604` erhoben,
> im Worktree `/home/marvin/Projects/reprise-shellcheck`. Referenzlauf:
> `shellcheck -x -P SCRIPTDIR -f gcc $(git ls-files '*.sh' '.githooks/*')` —
> **99 Dateien, 116 Meldungen: 54 `warning`, 60 `info`, 2 `style`**, kein
> einziger `error`. Laufzeit 8 s.

## Problem und Ursache

Das Repo prüft Rust mit Clippy auf `-D warnings`, dazu Architektur,
Barrierefreiheit, Motion-Tokens, AppStream und Flatpak mit eigenen Gates — aber
die 99 getrackten Shell-Skripte, die genau diese Gates ausführen, prüft niemand.
Das ist die falsche Reihenfolge: die Wächter sind ungeprüft, das Bewachte ist
geprüft. shellcheck 0.11.0 ist jetzt lokal vorhanden, damit fällt der einzige
Grund weg, das so zu lassen.

Die Messung zeigt kein Trümmerfeld: 66 der Skripte sind vollständig sauber, es
gibt **null** Meldungen der Stufe `error`. Das Repo hat keine Shell-Qualitäts­krise,
sondern eine Prüflücke.

Der größte einzelne Block ist gar kein Codeproblem, sondern ein Aufrufproblem:
29 × SC1091 („Not following: lib.sh") entstehen, weil shellcheck `source`-Pfade
relativ zum Arbeitsverzeichnis auflöst, nicht relativ zum Skript. Mit
`-P SCRIPTDIR` verschwinden sie restlos, und zusätzlich lösen sich 9 SC2034 von
selbst auf, weil shellcheck nun sieht, dass die gesourcte Bibliothek die Variable
tatsächlich liest: von 152 Meldungen bleiben 116.

Der zweite Block ist echte Substanz und der eigentliche Anlass. Sechs Stellen
beginnen eine Zeile mit `!`:

```
scripts/tests/worktree-gc.sh:103,185,217,313,574
scripts/tests/worktree-gc-schedule.sh:19
```

Beide Dateien laufen unter `set -euo pipefail` und verlassen sich darauf, dass
eine fehlgeschlagene Zusicherung das Skript abbricht — genau so funktionieren die
Nachbarzeilen `[[ ! -d $stale_worktree ]]` und `rg -Fq "removed …"`. Für eine mit
`!` negierte Kommandoliste schaltet bash `errexit` jedoch ausdrücklich ab.
Existiert der Zweig `test/stale` also weiterhin, obwohl die GC ihn hätte löschen
müssen, liefert die Zeile Status 1, `errexit` greift nicht, das Skript läuft
weiter, der Test endet grün. **Sechs Zusicherungen, die nicht rot werden können,
in ausgerechnet den Testskripten, die die Löschlogik für Worktrees und Zweige
absichern.**

Eine davon ist namentlich die Wache gegen den teuersten bekannten Betriebsfehler
dieses Repos: `worktree-gc-schedule.sh:19` verbietet, dass der Timer wieder auf
`/home/marvin/Projects/reprise/scripts/reprise-worktree-gc.sh` zeigt — der Pfad,
der monatelang ins Leere lief, bis `.worktrees` auf 162 GB gewachsen war.

Dazu kommen zwei Befunde, die beim Nachmessen für diesen Plan aufgefallen sind
und die den Zustand erklären:

- **`scripts/tests/worktree-gc-schedule.sh` ist heute rot — auf `origin/dev`,
  ohne unser Zutun.** Zeile 18 erwartet `--scope /home/marvin/Projects/reprise`,
  die Unit `docs/automation/reprise-worktree-gc.service` sagt seit #455
  `--scope ~/Projects/reprise`. Der Lauf endet nach 0 s mit Status 1 und sagt
  nicht, warum. Der Defekt dahinter ist echt: **systemd expandiert `~` in
  `ExecStart` nicht**, nur `%h` und Umgebungsvariablen — die Unit sweept einen
  Pfad, den es nicht gibt.
- **Diese Tests ruft nichts automatisch auf.** `check-merge-readiness.sh` fährt
  ausschließlich `check-*.sh`; `qa-linters.sh` prüft für `worktree-gc*.sh` nur
  `require_executable`; kein Workflow unter `.github/workflows/` ruft
  `scripts/tests/`. Deshalb fiel weder das Rot noch die Blindheit je jemandem auf.

Der Rest zerfällt in Meldungen, die aus der Bauweise der Harnesse folgen
(Konstantenbibliotheken, Prozessgrenzen, Trap-Handler) und in 30 × SC2016 plus
7 × SC1003 — einfache Anführungszeichen und Backslashes in SQL-, awk- und
Heredoc-Text, wo Expansion gerade nicht erwünscht ist. Sie liegen sämtlich unter
`warning` und brauchen ein Urteil, kein Stummschalten.

## Entscheidungen mit Begründung

**E1 — `.shellcheckrc` im Wurzelverzeichnis statt 29 Dateiannotationen.** Eine
Datei mit `external-sources=true` und `source-path=SCRIPTDIR` beseitigt alle 29
SC1091. **Nachgemessen: die rc-Datei allein liefert exakt dasselbe Ergebnis wie
die expliziten Schalter — 116 = 116.** shellcheck sucht sie im Verzeichnis des
geprüften Skripts und in allen Elternverzeichnissen, eine Datei an der Wurzel
deckt also `scripts/`, `scripts/ptr-e2e/`, `build-aux/`, `acceptance/` und
`docs/evidence/` gleichermaßen ab. Entscheidend ist der Nebeneffekt: Editor, ein
blankes `shellcheck datei.sh` von Hand und das Gate sehen danach dasselbe.
`external-sources` lässt sich aus Sicherheitsgründen ohnehin nur über die
rc-Datei setzen. Das Gate übergibt `-x -P SCRIPTDIR` zusätzlich explizit, damit
es auch aus einem fremden Arbeitsverzeichnis deterministisch bleibt.

**E2 — Das Gate fährt zwei Läufe: `-S warning` global, dazu eine Einschlussliste
auf Note-Ebene.** Ein Gate allein auf `-S warning` hätte einen Konstruktionsfehler,
den die Messung sichtbar macht: **alle elf echten Defekte, die dieser Plan
gefunden hat, liegen unter `warning`.** Ein Gate, das die eigenen Funde nicht
fängt, erlaubt sie ab dem Tag nach der Reparatur wieder. Deshalb:

```
shellcheck -x -P SCRIPTDIR -S warning -f gcc -- "${files[@]}"
shellcheck -x -P SCRIPTDIR -S style -i SC2251,SC2004,SC2181 -f gcc -- "${files[@]}"
```

Der zweite Lauf **muss `-S style`** fahren, nicht `-S info`: SC2004 und SC2181
liegen auf `style` und wären sonst stumm (nachgemessen). Beide Läufe zusammen
kosten ~16 s; JSON-Filterei im Gate wäre mehr Fläche als Nutzen.

Die drei Regeln der Liste stehen nach Welle 2 repoweit auf null und haben keine
legitime Gegenform, die im Repo vorkommt. **Eine Einschlussliste ist die
Umkehrung einer Baseline:** sie kann nur strenger machen, nie etwas verstecken,
und wenn sie verrottet, ist das Gate genau so streng wie heute, nicht laxer.
Ausdrücklich **nicht** auf der Liste: SC2015 und SC2086 — beide haben legitime
Verwendungen und würden künftig `disable`-Kommentare erzwingen, also genau die
Kommentarschicht, die dieses Vorhaben vermeidet.

**E3 — Kein `disable` für SC2329, SC2094, SC2153, SC2016, SC1003.** Diese
Meldungen liegen unter der Gate-Schwelle und stehen nicht auf der Einschlussliste.
Sie werden gelesen und entschieden; das Ergebnis ist ein Satz in diesem Plan, nie
ein Kommentar, der eine Meldung stummschaltet, die ohnehin niemanden aufhält.

**E4 — Die sechs SC2251 sind ein Defekt und werden repariert, mit Mutationsprobe.**
Die Reparatur ersetzt `! kommando …` durch eine kleine Hilfsfunktion in beiden
Dateien:

```bash
refute() { if "$@"; then printf 'refute failed: %s\n' "$*" >&2; exit 1; fi; }
```

Die zwei Aufrufe mit Here-String (`! rg -Fq … <<<"$report"`) funktionieren damit
unverändert: die Umleitung hängt am `refute`-Aufruf, `rg` erbt sie als stdin.
Gegenüber shellchecks eigenem Vorschlag `kommando && exit 1` hat das einen
Nebengewinn — die gerissene Zusicherung sagt beim Abbruch, welche es war. Heute
sagt sie gar nichts.

**E5 — Die zwei SC2329 sind Falschmeldungen und bleiben unangetastet.**
`cleanup_private` in `scripts/cua-explore/run.sh:103` wird vier Zeilen später
durch `trap cleanup_private EXIT` installiert, `cleanup_app` in
`scripts/playback-history-smoke.sh:313` fünf Zeilen später durch
`trap cleanup_app EXIT`. Beide sind innerhalb einer anderen Funktion bzw. eines
`case`-Zweigs definiert, was shellchecks Erreichbarkeitsanalyse nicht durchdringt;
die Meldung nennt diesen Fall selbst. Kein toter Code, keine Änderung, kein
`disable`.

**E6 — Die zwei SC2094 sind eine bewusste Selbstinspektion und bleiben
unangetastet.** In `scripts/check-display-tests.sh` leitet der Arbeiterblock seine
gesamte Ausgabe nach `"$results_dir/$index.log"` um (Zeile 218), und Zeile 208
liest dieselbe Datei mit `grep -q "Failed to initialize GTK"`, um zu entscheiden,
ob ein Testlauf wegen eines nie hochgekommenen Displays wiederholt wird. Es ist
keine Pipeline, sondern eine Blockumleitung; der geprüfte Kindprozess ist beim
`grep` bereits beendet. Ein Umbau brächte kein Verhalten und ein Risiko.

**E7 — Nur was das Gate hält, gehört in diesen Plan.** Damit fallen zwei
Reparaturen heraus, die der Entwurf noch vorsah: die sechs SC2015 in
`scripts/check-logo-artwork.sh` (`[[ … ]] && ok "…" || bad "…"`) und die zwei
SC2086 in `scripts/performance-runtime-baseline.sh:462` und
`build-aux/meson-cargo-build.sh:30`. Sie sind reale, wenn auch unwahrscheinliche
Schwächen — aber sie stehen weder über der Gate-Schwelle noch auf der
Einschlussliste, also gäbe es hinter ihrer Reparatur keinen Wächter, und morgen
könnte sie jemand zurückbauen, ohne dass etwas rot wird. Arbeit ohne Wächter
gehört in ein eigenes Vorhaben, nicht in dieses. `check-logo-artwork.sh` behält
deshalb nur seine SC2034-Reparatur, `meson-cargo-build.sh` nur seine beiden
toten Zuweisungen; `performance-runtime-baseline.sh` fällt ganz aus dem Plan.
**An Logo, Artwork oder Marke ändert dieses Vorhaben nichts.**

**E8 — Die fünf SC2153 sind Prozess- und Indirektionsgrenzen, keine Tippfehler.**
`SCRATCH` in `scripts/cua-e2e/filter_clear_matrix.sh:97` wird in Zeile 360
derselben Datei als `SCRATCH="$scratch"` an ein `bash "${BASH_SOURCE[0]}"`
weitergereicht — das Skript ruft sich selbst als Kindprozess auf. `APP_LOG` und
`WINDOW_ID` werden in `scripts/cua-e2e/run.sh:171/190` und
`scripts/cua-e2e/scrobbling.sh:94/114` gesetzt und von den Szenariodateien
gelesen. `TRACK_ID_X`/`TRACK_ID_Y` in `scripts/ptr-e2e/run.sh:685` stammen aus
indirekter Zuweisung über `db_scalar_into` in Zeile 670/673. Keine Änderung,
kein `disable`.

**E9 — SC2034 wird je Variable geprüft, nicht je Regel; `export` ist als
Ausweichweg ausdrücklich verworfen.** In `scripts/ptr-e2e/geometry.sh` haben 26
der 28 gemeldeten Konstanten nachweislich Konsumenten in `scripts/ptr-e2e/*.sh`;
`PRIMARY_MENU_FROM_RIGHT` und `SEARCH_TOGGLE_FROM_RIGHT` haben repoweit keinen,
sind aber bewusst geführt (der Kommentar darüber sagt „record every slot
explicitly so future pointer recalibration cannot accidentally target the
revealed second top bar"). Die Datei bekommt genau **ein** `disable=SC2034` auf
Dateiebene mit dreiteiliger Begründung.

Es gibt einen Weg ganz ohne Kommentar — `export` schaltet SC2034 ab
(nachgemessen: `export Y=2` meldet nichts, `readonly Z=3` sehr wohl). Er wird
**nicht** genommen: die Konsumenten *sourcen* die Datei, sie lesen sie nicht aus
der Umgebung; `export` behauptete das Gegenteil, schöbe 28 Koordinaten­variablen
in die Umgebung jedes Kindprozesses (xdotool, die App selbst) und wäre eine
Unwahrheit, die nur den Linter besänftigt. Eine begründete Ausnahme ist ehrlicher
als ein Trick, der nicht wie eine Ausnahme aussieht.

`APP_ID` in `scripts/cua-e2e/run.sh:26` und `scripts/ptr-e2e/run.sh:113` hat
dagegen keinen Konsumenten — geprüft: die repoweiten Treffer auf `APP_ID` sind
ausschließlich gleichnamige Rust-Konstanten in `crates/`. Die zwei Zuweisungen
werden gelöscht, nicht stummgeschaltet.

**E10 — Das Gate erzwingt die Begründungspflicht selbst.** Präzedenzfall im Repo:
`scripts/check-ai-hygiene.sh` prüft unter GP-20, dass jedes `#[allow(dead_code)]`
einen Kommentar in derselben oder der vorhergehenden Zeile trägt.
`scripts/check-shell.sh` bekommt dieselbe Prüfung für `# shellcheck disable=`.
Beide Formen sind gemessen zulässig — shellcheck akzeptiert den Grund hinter der
Direktive und in der Zeile darüber. Bei drei Ausnahmen wirkt das
überdimensioniert; es ist aber der einzige Mechanismus, der „keine
Ausnahmenliste" über die Zeit trägt. Ohne ihn stehen in einem Jahr zwanzig nackte
`disable`s im Repo und niemand merkt es.

**E11 — Die Dateiliste ist `git ls-files -z '*.sh' '.githooks/*'` — heute 99
Dateien.** `.githooks/pre-push` ist ein von Hand geschriebenes bash-Skript ohne
Endung, ruft `check-merge-readiness.sh` auf und ist damit Teil der Gate-Kette; es
fiele durch ein reines `*.sh`-Raster und ist nachgemessen auf allen Stufen sauber.
Die einzige weitere getrackte Datei mit Shell-Shebang ohne `.sh` ist
`android/gradlew`, von Gradle generiert und nicht unser Code; sie fällt durch
dieselbe Regel heraus, ohne dass ein Name auf einer Ausnahmenliste steht. Die
Liste ist eine Regel, keine Aufzählung: neue Skripte werden automatisch
mitgeprüft, ein Gradle-Upgrade kann das Gate nicht wegen fremden Codes rot färben.

**E12 — Das Gate hängt in `scripts/check-merge-readiness.sh`, nicht direkt in
`scripts/ci-quality.sh`.** `ci-quality.sh` ist ein 31-zeiliger CI-Vorspann, der
den Basiszweig ermittelt, die Promotionsregel für `main` prüft und dann
`check-merge-readiness.sh --no-fetch` ruft; sämtliche inhaltlichen Gates liegen
dort. Über die Kette `ci-quality.sh → check-merge-readiness.sh → check-shell.sh`
läuft das Gate in CI **und** über `.githooks/pre-push` lokal. Position: unmittelbar
nach `git diff --check` und vor `check-architecture.sh` — damit ein Shell-Fehler
in Sekunden auffällt und auch ein abgebrochener lokaler Lauf ihn noch gesehen hat.

**E13 — Fehlendes shellcheck überspringt das Gate, aber CI installiert es.**
`scripts/lib/rulebook.sh` bringt `skip_gate_if_tool_missing` mit;
`check-appstream.sh` benutzt es dreimal. `check-shell.sh` sourct dieselbe
Bibliothek und ruft `skip_gate_if_tool_missing shellcheck`. Gegengewicht: ein
Gate, das überall übersprungen wird, ist kein Gate. Deshalb wird `shellcheck` in
`.github/workflows/ci.yml` in die `pacman -Syu`-Liste des Quality-Jobs aufgenommen
(alphabetisch zwischen `ripgrep` und `rust`), und `scripts/tests/qa-linters.sh`
bekommt ein `require_pattern 'shellcheck' .github/workflows/ci.yml`, damit niemand
das Paket entfernt und das Gate lautlos zum Dauerskip wird.

**E14 — Die reparierten Wächter werden verdrahtet.** `scripts/tests/worktree-gc.sh`
und `scripts/tests/worktree-gc-schedule.sh` werden in `check-merge-readiness.sh`
aufgenommen. Beide sind hermetisch: `worktree-gc-schedule.sh` schiebt ein
gefälschtes `systemctl` unter, `worktree-gc.sh` baut seine Fixtures in
`mktemp -d`, `ripgrep` steht bereits in der pacman-Liste. Kosten: **+77 s** in
einem CI-Lauf, der ohnehin Clippy, die Workspace-Tests und die gesamte
Display-Suite fährt. Nutzen: `scripts/reprise-worktree-gc.sh` ist ein
641-Zeilen-Werkzeug, das Worktrees und Zweige **löscht**. Sechs Zusicherungen
rot-fähig zu machen und sie weiter niemanden fragen zu lassen, wäre dieselbe
Lücke eine Etage höher.

**E15 — Der Unit-Fix gehört in dieses Vorhaben, die Scope-Rework der GC nicht.**
`docs/automation/reprise-worktree-gc.service` bekommt
`--scope %h/Projects/reprise` statt `~/Projects/reprise`, die Erwartung in
`worktree-gc-schedule.sh:18` zieht mit. Zwei Zeilen, ein eigener Commit, **vor**
der SC2251-Reparatur — ohne ihn stirbt das Skript in Zeile 18 und die
Mutationsprobe an Zeile 19 ist gar nicht durchführbar. Nur das Orakel
anzugleichen wäre falsch: das hieße, den Test an die kaputte Realität anzupassen.
Ausdrücklich **nicht** Gegenstand bleibt die tiefere Scope-Schwäche der GC
(Geschwister-Worktrees gelten als `outside_scope`, `dirty` blockt die
target-Löschung) — offener Punkt seit dem 11.08., eigenes Vorhaben.

**E16 — SC1112 in `readme-showcase.sh` braucht kein `disable`.** Die Zeile
`reject_fixed '## Roadmap: … today’s player' "$english"` meldet das `’` (U+2019),
weil sie einfach gequotet ist. In **doppelten** Anführungszeichen schweigt
shellcheck (nachgemessen, Exit 0), und der String ist byteidentisch — er enthält
kein `$`, keinen Backtick, keinen Backslash, es gibt also keinen
Expansionsunterschied. Ein Zeichenwechsel statt eines Kommentars; die Zusicherung
gegen die zurückgezogene README-Überschrift bleibt exakt dieselbe. Das
`disable`-Inventar sinkt damit auf **drei**.

**E17 — Erst säubern, dann scharfschalten, in getrennten Commits.** Die
Säuberungswellen enthalten keine Zeile Gate-Code, die Gate-Welle keine Zeile
Skriptsäuberung. Ein Gate, das beim Einbau schon rot ist, wird abgeschaltet statt
repariert — und ein vermischter Commit macht unlesbar, ob eine Verhaltensänderung
an einem Testskript gewollt oder vom Linter erzwungen war.

## Umsetzung in Wellen

Ein Strang, sequenziell in `feature/shellcheck`, vier Wellen, jede ein eigener
Commit (Welle 1 darf sich nach Familien aufteilen).

### Welle 0 — gemeinsame Messgrundlage

`.shellcheckrc` im Wurzelverzeichnis:

```
external-sources=true
source-path=SCRIPTDIR
```

Dazu ein kurzer Kommentar, warum sie existiert. Sonst nichts.

**Nachweis:** `shellcheck -f gcc $(git ls-files '*.sh' '.githooks/*')` liefert
danach **ohne weitere Schalter** 116 Meldungen und **keine einzige SC1091**.

### Welle 1 — die 54 Warnungen, 17 Dateien

*Harnesse und Abnahme:*

- `scripts/ptr-e2e/geometry.sh` (Zeilen 5–44) — 28 × SC2034. Ein
  `# shellcheck disable=SC2034` auf Dateiebene direkt unter dem Kopfkommentar,
  mit einer Begründung darüber, die (a) die Datei als Konstantenbibliothek für
  `scripts/ptr-e2e/*.sh` benennt, (b) festhält, dass `PRIMARY_MENU_FROM_RIGHT`
  und `SEARCH_TOGGLE_FROM_RIGHT` heute keinen Konsumenten haben und das Raster
  bewusst vollständig halten, und (c) die Regel nennt: eine Konstante wird
  gelöscht, wenn ihr letzter Konsument geht.
- `scripts/ptr-e2e/preferences.sh:34,86,89` — 3 × SC2034, aber **zwei gezielte**
  `disable`-Kommentare statt eines auf Dateiebene, damit eine künftige echte
  Leiche in dieser Datei weiter auffällt: einer über
  `WINDOW_ID="$(xdotool getactivewindow …)"` mit der Begründung, dass fünf andere
  `ptr-e2e`-Dateien die Variable lesen (`column-reorder.sh`, `compact-seek.sh`,
  `run.sh`, `search-chip.sh`, `window-helpers.sh`), einer über der
  `local ROW_*`-Leiter (deckt 86 und 89) mit der Begründung, dass die Zeilenleiter
  vollständig geführt wird, damit eine Neuvermessung die benutzten Zeilen nicht
  stillschweigend verschiebt.
- `scripts/cua-e2e/run.sh:26` und `scripts/ptr-e2e/run.sh:113` — 2 × SC2034 auf
  `APP_ID`. Vor dem Löschen noch einmal gegen `*.py`, `meson.build`, `*.yml` und
  `*.rs` gegenprüfen (Stand heute: nur gleichnamige Rust-Konstanten), Ergebnis in
  die Commit-Beschreibung. Zuweisung löschen.
- `scripts/cua-e2e/scrobbling.sh:43` — SC2154 auf `repo_root`. Kein `disable`,
  sondern die Bibliothek selbsttragend machen:
  `repo_root=${repo_root:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}`
  nahe dem Dateikopf. Beim Sourcen aus `run.sh` verhaltensgleich, standalone
  korrekt.
- `scripts/cua-e2e/selection_anchor.sh:109` — SC2034 auf `b`.
  `read -r r g b` → `read -r r g _`; das letzte Feld schluckt den Rest wie zuvor,
  und `_` löst SC2034 nachweislich nicht aus.
- `scripts/cua-e2e/source_content.sh:136,145` — 2 × SC1007. `LANGUAGE= \` →
  `LANGUAGE='' \`. Identische Semantik, shellchecks eigener Vorschlag.
- `scripts/cua-common/session.sh:190,192` — SC2097 + SC2098. Den zusammengesetzten
  Wert einmal in eine lokale Variable heben
  (`local stub_data_dirs="$stub_root:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"`)
  und sowohl in der Präfixzuweisung vor `dbus-run-session` als auch im inneren
  `env` diese Variable benutzen. Beweisbar verhaltensgleich — beide Ausdrücke
  expandieren heute schon im Elternprozess zum selben String —, aber die Absicht
  ist danach lesbar statt geraten.
- `scripts/playback-history-smoke.sh:212` — SC2034 auf `second`. Zuweisung
  entfernen, Aufruf als `wait_for_title '' "$first" >/dev/null` stehen lassen;
  die Wartewirkung ist der Zweck. Form wie zwei Zeilen darunter.
- `scripts/ptr-e2e/compact-seek.sh:59` — SC2034 auf `attempt`.
  `for attempt in $(seq 1 4)` → `for _ in $(seq 1 4)` (dieselbe Datei benutzt vier
  Zeilen tiefer bereits diese Form), `attempt` aus der `local`-Zeile streichen.
- `acceptance/deezer-placeholder-portraits/run-accept.sh:637` — SC2034 auf
  `total`. `while IFS='|' read -r rank name total` → `read -r rank name _`. Die
  dritte Position muss bleiben, sonst zieht `name` das dritte Feld an sich.

*Build-, Gate- und Werkzeugskripte:*

- `build-aux/meson-cargo-build.sh:8,10` — 2 × SC2034 auf `stem_backend` und
  `worker_path`. Beide werden aus `$5`/`$7` gelesen und nirgends benutzt; der
  Kommentar ab Zeile 18 erklärt, warum. `prefix` (`$6`) existiert ausschließlich
  als Default für `worker_path` und fällt mit. Vor dem Löschen die Aufrufstelle in
  `meson.build` lesen und im Kommentar festhalten, dass die Argumente 5 bis 7
  weiterhin übergeben und bewusst ignoriert werden — **die Aufrufsignatur bleibt
  unangetastet**, nur die toten Zuweisungen gehen.
- `scripts/check-logo-artwork.sh:351` — SC2034 auf `dimensions`. Aus der
  `local`-Zeile streichen. Sonst nichts an dieser Datei (E7).
- `scripts/reprise-worktree-gc.sh:323,416` — 6 × SC1007.
  `local path= head= branch= locked=false line` →
  `local path='' head='' branch='' locked=false line`, an beiden Stellen.
  Semantisch identisch.
- `scripts/verify-radio-favicons.sh:43` — SC2174. `mkdir -p -m 0700 "$out_dir"` →
  `mkdir -p "$out_dir"`; das `chmod 0700 "$out_dir"` fünf Zeilen darunter setzt
  die Rechte ohnehin unbedingt und ist die eigentliche Zusicherung, während `-m`
  bei `-p` nur das tiefste Verzeichnis erreicht.
- `docs/evidence/bounded-daemon-stop/probe-stop-daemon.sh:6` — SC1090. Kein
  `disable`, sondern die Direktive, die die Meldung selbst verlangt:
  `# shellcheck source=../../../scripts/cua-common/session.sh` über dem
  `source "$1"`. Das schweigt nicht nur, es liefert shellcheck die fehlende
  Analyse.

*Wächter:*

- `scripts/tests/readme-showcase.sh:104` — SC1112 nach E16: einfache durch
  doppelte Anführungszeichen ersetzen, Zeichen unverändert lassen.

**Nachweis Welle 1:** `shellcheck -x -P SCRIPTDIR -S warning` über die volle
Dateiliste ist leer; `bash -n` über jede angefasste Datei;
`scripts/ptr-e2e/harness-self-test.sh` endet mit 0.

### Welle 2 — die Wächter, die nicht rot werden können

**Zuerst der Unit-Fix (E15), sonst ist der Rest nicht nachweisbar:**
`docs/automation/reprise-worktree-gc.service` auf `--scope %h/Projects/reprise`,
`scripts/tests/worktree-gc-schedule.sh:18` auf dieselbe Erwartung. Danach endet
`bash scripts/tests/worktree-gc-schedule.sh` mit 0 — heute endet es mit 1.

Dann die Reparatur: `refute`-Hilfe nach E4 in `scripts/tests/worktree-gc.sh` und
`scripts/tests/worktree-gc-schedule.sh`, sechs Stellen umgestellt
(`worktree-gc.sh:103,185,217,313,574`, `worktree-gc-schedule.sh:19`). Dazu die
zwei Befunde der Einschlussliste:

- `scripts/reprise-worktree-gc.sh:588` — SC2004:
  `excluded_paths[$excluded_index]` → `excluded_paths[excluded_index]`.
- `scripts/cua-e2e/filter_clear_matrix.sh:364` — SC2181:
  `[[ $? -eq 0 ]] || failed+=("$case_name")` prüft den Status des vorangehenden
  mehrzeiligen `env … bash …`-Aufrufs. Die Datei läuft unter `set -uo pipefail`
  ohne `-e`, das Konstrukt funktioniert heute — es ist eine Kette, die beim
  nächsten Einschub dazwischen zerbricht. Umbau auf
  `if ! env … ; then failed+=("$case_name"); fi`.

**Das Mutationsprotokoll — acht Läufe, nicht achtzehn.** `worktree-gc.sh` braucht
77 s pro Lauf, `worktree-gc-schedule.sh` ist instantan. Der Dreischritt je Stelle
wäre 20 Minuten reine Wartezeit; derselbe Beweis geht so:

1. **Ein** Lauf auf dem Stand *vor* der `refute`-Reparatur (aber *nach* dem
   Unit-Fix), mit **allen sechs Stellen gleichzeitig verfälscht** → endet
   **grün**. Das ist der Defektbeweis für alle sechs auf einmal: sechs gerissene
   Zusicherungen, und der Test merkt nichts.
2. Reparieren.
3. **Sechs** Läufe, je genau eine Stelle verfälscht → je Exit ≠ 0, und die
   `refute`-Meldung nennt die Stelle. Einzeln nötig, weil `errexit` nach der
   Reparatur beim ersten Treffer abbricht und die übrigen fünf sonst nie erreicht
   würden.
4. **Ein** Abschlusslauf ohne Verfälschung → grün.

Jede Verfälschung wird im Protokoll benannt, damit das Review sie nachvollziehen
kann — etwa `refs/heads/test/stale` → `refs/heads/main` (existiert im Fixture
garantiert), oder das `rg`-Muster auf eine Zeile ändern, die im Bericht
nachweislich steht. Das Protokoll ist eine Tabelle mit acht Zeilen und gehört in
die Commit-Beschreibung.

**Keine Änderung, dokumentiertes Urteil** (E3, E5, E6, E8): SC2329 in
`cua-explore/run.sh:103` und `playback-history-smoke.sh:313`; SC2094 in
`check-display-tests.sh:208,218`; SC2153 in `filter_clear_matrix.sh`,
`filter_clear_playback.sh`, `selection_anchor.sh`, `ptr-e2e/run.sh:685`; 30 ×
SC2016 und 7 × SC1003 in `cua-e2e.sh`, `cua-explore.sh`, `github-flow.sh`,
`qa-linters.sh`, `motion-tokens.sh`, `harness-self-test.sh`.

### Welle 3 — das Gate

Erst wenn Welle 1 und 2 stehen und beide Läufe leer sind.

`scripts/check-shell.sh`, ausführbar, nach dem Muster der bestehenden Gates:
`#!/usr/bin/env bash`, `set -euo pipefail`,
`cd "$(git rev-parse --show-toplevel)"`, `source scripts/lib/rulebook.sh`,
`skip_gate_if_tool_missing shellcheck`. Dateiliste über
`git ls-files -z '*.sh' '.githooks/*'` in ein Array
(`mapfile -d '' files < <(…)`), damit Pfade mit Sonderzeichen nicht zerfallen.
Dann die zwei Läufe aus E2 und die Begründungsprüfung aus E10 über dieselbe
Liste. Die erste Ausgabezeile nennt die gefundene shellcheck-Version und die Zahl
der geprüften Dateien, damit ein Skip und ein Versionssprung im CI-Protokoll
sichtbar sind statt still. **Kein `--report`-Schalter** — durch die
`.shellcheckrc` zeigt ein blankes `shellcheck datei.sh` von Hand ohnehin alles,
und jede zusätzliche Schalterfläche im Gate kann selbst kaputtgehen.

Verdrahtung, alles in einem Commit:

- Aufruf `scripts/check-shell.sh` in `scripts/check-merge-readiness.sh`
  unmittelbar nach `git diff --check` (E12).
- `scripts/tests/worktree-gc.sh` und `scripts/tests/worktree-gc-schedule.sh`
  ebendort aufnehmen (E14).
- `shellcheck` in die pacman-Liste in `.github/workflows/ci.yml` (E13).
- In `scripts/tests/qa-linters.sh`: `require_executable scripts/check-shell.sh`,
  `require_pattern 'check-shell.sh' scripts/check-merge-readiness.sh`,
  `require_pattern 'shellcheck' .github/workflows/ci.yml` sowie
  `require_pattern 'worktree-gc.sh' scripts/check-merge-readiness.sh`.
- In `TESTING.md` unter „Required merge gates" ein Absatz, der das Gate, seine
  zwei Schwellen, sein Skip-Verhalten und die neu verdrahteten gc-Tests benennt.

Ohne diese Ergänzungen wäre der neue Wächter selbst der einzige ungeprüfte,
undokumentierte Wächter im Repo — genau der Zustand, den dieser Plan beseitigt.

## Abnahmekriterien

1. `shellcheck -x -P SCRIPTDIR -S warning -f gcc $(git ls-files '*.sh' '.githooks/*')`
   gibt nichts aus und endet mit 0. Referenz: heute 54 Meldungen aus 17 Dateien.
2. `shellcheck -x -P SCRIPTDIR -S style -i SC2251,SC2004,SC2181 -f gcc` über
   dieselbe Liste gibt nichts aus. Referenz: heute 8 Meldungen.
3. `scripts/check-shell.sh` endet mit 0 und meldet Version und 99 geprüfte
   Dateien.
4. `PATH` ohne shellcheck: `scripts/check-shell.sh` schreibt
   `SKIPPED: shellcheck is not installed; this gate did not run` nach stderr und
   endet mit 0.
5. Ein absichtlich eingebauter Verstoß über der Schwelle (etwa eine unbenutzte
   Variable) färbt `scripts/check-shell.sh` rot. Danach zurückdrehen.
6. **Ein absichtlich eingebautes `! kommando` in einem Testskript färbt
   `scripts/check-shell.sh` rot.** Das ist der Selbstbeweis der Einschlussliste;
   ohne ihn ist Kriterium 2 wertlos. Danach zurückdrehen.
7. Ein `# shellcheck disable=SC2034` ohne Begründung, testweise eingefügt, färbt
   `scripts/check-shell.sh` rot. Danach zurückdrehen.
8. Das achtzeilige Mutationsprotokoll aus Welle 2 liegt vor: ein grüner Lauf mit
   sechs Verfälschungen vor der Reparatur, sechs rote Einzelläufe danach, ein
   grüner Abschlusslauf.
9. `bash scripts/tests/worktree-gc.sh` und
   `bash scripts/tests/worktree-gc-schedule.sh` enden mit 0 (letzteres endet
   heute mit 1).
10. `bash scripts/tests/qa-linters.sh` endet mit 0.
11. `bash -n` über alle 99 Dateien endet fehlerfrei.
12. `scripts/ptr-e2e/harness-self-test.sh` endet mit 0.
13. `git grep -n 'shellcheck disable' -- '*.sh' '.githooks/*'` liefert **genau
    drei** Treffer — `geometry.sh` (1) und `preferences.sh` (2) —, jeder mit
    Begründung in derselben oder der vorhergehenden Zeile, keiner für SC2251,
    SC2329, SC2094, SC2015, SC2153, SC2016 oder SC1003. Weicht die Zahl ab,
    gehört die Abweichung in die Commit-Beschreibung; sie ist der Griff, an dem
    das Review die Ausnahmenliste kontrolliert, die es nicht geben soll.

## Risiken

**Die SC2251-Reparatur ändert Kontrollfluss in den zwei Testskripten, die
Worktree-Löschung absichern.** Wenn eine der sechs Zusicherungen heute in
Wahrheit fehlschlägt und das nur wegen des defekten `errexit` niemandem auffällt,
wird der Test nach der Reparatur rot — zu Recht, aber überraschend. Das ist kein
Rückschlag, sondern das Ergebnis, für das die Reparatur gemacht wurde: der Befund
gehört dann untersucht und als eigener Punkt behoben, nicht durch Zurückdrehen
der Reparatur erledigt.

**Die neu verdrahteten gc-Tests können den Merge-Gate-Lauf röten, ohne dass der
PR etwas damit zu tun hat.** Sie waren jahrelang ungefragt; der erste
verdrahtete Lauf ist zugleich ihr erster ehrlicher. Gegenmaßnahme: beide laufen
in Welle 2 vollständig und grün, bevor Welle 3 sie einhängt.

**Das Auflösen der `source`-Pfade legt neue Befunde frei.** Bereits gemessen:
SC2153 steigt von 3 auf 5, weil shellcheck unter `-P SCRIPTDIR` weiter sieht.
Beide neuen Meldungen sind Notes und harmlos (E8), aber das Muster wiederholt
sich: wer künftig eine `source`-Zeile hinzufügt, kann Meldungen in einer Datei
auslösen, die er nicht angefasst hat. Das ist der Preis für echte Analyse statt
Blindflug und wird durch die Gate-Schwelle gedämpft.

**Löschen scheinbar toter Variablen kann eine Übergabe kappen.** Betrifft
`APP_ID`, `stem_backend`, `worker_path`, `prefix`. Eine exportierte
Shell-Variable kann von Python, meson oder einem Rust-Test gelesen werden, ohne
in einer `*.sh` aufzutauchen. Gegenmaßnahme: vor jedem Löschen ein repoweites
`git grep -F` über alle Dateitypen, Ergebnis in die Commit-Beschreibung; für
`meson-cargo-build.sh` zusätzlich die Aufrufstelle in `meson.build` lesen.

**Versionsdrift von shellcheck.** Lokal 0.11.0, im CI-Container das jeweils
aktuelle Arch-Paket. Eine neue Version kann Regeln hinzufügen und einen
unbeteiligten Zweig rot färben. Ein Pin wäre eine Baseline unter anderem Namen
und würde genauso verrotten; die Antwort ist, dass `check-shell.sh` seine Version
protokolliert, damit ein solcher Fall in fünf Sekunden als Versionssprung
erkennbar ist statt als Rätsel.

**Der Datei-`disable` in `geometry.sh` kann eine künftige echte Leiche
verbergen.** Bewusst in Kauf genommen: die Datei besteht auf 44 Zeilen zu 100 %
aus Konstanten und Kommentaren, der Schaden einer vergessenen Koordinate ist eine
ungenutzte Zahl. In `preferences.sh` — wo neben Konstanten auch normaler Code
steht — wird deshalb ausdrücklich *nicht* auf Dateiebene stummgeschaltet.

**`skip_gate_if_tool_missing` macht das Gate auf jedem Rechner ohne shellcheck
lautlos folgenlos.** Deshalb ist die pacman-Ergänzung in `ci.yml` kein Beiwerk,
sondern trägt die halbe Wirkung des Vorhabens, und deshalb sichert
`qa-linters.sh` sie mit einem `require_pattern` ab.

## Ausdrücklich nicht Gegenstand

Keine Baseline-, Ausnahme- oder Unterdrückungsdatei, in keiner Form — das ist die
Zielsetzung, nicht eine Abwägung. Kein Absenken der globalen Schwelle unter
`warning`; die einzige Ausnahme ist die benannte Dreierliste SC2251/SC2004/SC2181,
und sie macht ausschließlich strenger. Keine Reparatur ohne Wächter dahinter (E7):
SC2015 in `check-logo-artwork.sh`, SC2086 in `meson-cargo-build.sh` und
`performance-runtime-baseline.sh` bleiben, wie sie sind. **Keine Änderung an Logo,
Artwork, Marke oder deren Erzeugung.** Kein `shfmt`, keine Formatierungs-,
Umbenennungs- oder Strukturarbeit über konkrete Befunde hinaus. Kein Umbau der
cua-/ptr-Harnesse, insbesondere nicht der Prozessgrenzen aus E8 oder der
Selbstinspektion aus E6. Keine Scope-Rework der Worktree-GC (E15).
`android/gradlew` und alles andere Fremdgenerierte bleibt außen vor. Keine Linter
für die Python-Skripte unter `scripts/` — eigenes Vorhaben. Keine Änderung an der
Promotionslogik in `ci-quality.sh` und keine Umsortierung der bestehenden Gates in
`check-merge-readiness.sh` außer den neuen Zeilen.

## Parallelität

**Bewusst ein Strang.** Der Schnitt wurde geprüft und verworfen. Die Arbeit
zerfällt zwar in zwei Hälften mit verschiedenen Nachweisen — „der Linter
schweigt" gegen „der Test kann rot werden" —, aber die beiden Hälften sind
gekoppelt: Welle 1 ändert `scripts/reprise-worktree-gc.sh` (sechs SC1007), Welle 2
ändert und verdrahtet die Tests, die genau dieses Skript ausführen. Getrennte
Stränge hätten die Mutationsproben gegen einen Runner erbracht, den es nach dem
Merge so nicht mehr gibt, und hätten diese Kopplung als Nachmerge-Kreuzprüfung
nachgereicht. Sequenziell entfällt sie ersatzlos. Der Preis ist Wanduhr, nicht
Qualität: die Arbeit besteht überwiegend aus Einzeilern, und die 77-Sekunden-Läufe
der Mutationsproben lassen sich ohnehin nicht parallelisieren, weil sie
aufeinander aufbauen.
