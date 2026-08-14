---
slug: shellcheck
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-14
---
# shellcheck als Gate ohne Ausnahmenliste

> Gemessen mit shellcheck 0.11.0 gegen `origin/dev` im Worktree
> `/home/marvin/Projects/reprise-shellcheck`, Zweig `feature/shellcheck`.
> Zwei Läufe: `shellcheck -x -f gcc` (152 Meldungen) und derselbe Lauf zusätzlich
> mit `-P SCRIPTDIR` (116 Meldungen, davon 54 auf Stufe `warning`).

## Problem und Ursache

Das Repo prüft Rust mit Clippy auf `-D warnings`, Architektur, Barrierefreiheit, Motion-Tokens, AppStream und Flatpak mit eigenen Gates — aber die 98 getrackten Shell-Skripte, die genau diese Gates ausführen, prüft niemand. Das ist die falsche Reihenfolge: die Wächter sind ungeprüft, das Bewachte ist geprüft. shellcheck 0.11.0 ist jetzt lokal vorhanden, damit fällt der einzige Grund weg, das so zu lassen.

Die Messung zeigt kein Trümmerfeld: 66 der 98 Skripte sind bereits vollständig sauber, es gibt **null** Meldungen der Stufe `error`. Die 152 Meldungen verteilen sich auf 63 `warning` und 89 `note`. Das Repo hat also keine Shell-Qualitätskrise, sondern eine Prüflücke.

Der größte einzelne Block ist gar kein Codeproblem, sondern ein Aufrufproblem: 29 × SC1091 („Not following: lib.sh") entstehen, weil shellcheck `source`-Pfade relativ zum Arbeitsverzeichnis auflöst, nicht relativ zum Skript. Ein zweiter Lauf mit `-P SCRIPTDIR` beweist das: die 29 SC1091 verschwinden restlos, und zusätzlich lösen sich 9 der 49 SC2034 von selbst auf, weil shellcheck nun sieht, dass die gesourcte Bibliothek die Variable tatsächlich liest. Von 152 Meldungen bleiben 116. Diese eine Zeile Konfiguration ist damit die wirksamste Einzelmaßnahme des ganzen Vorhabens — und sie gehört an genau eine Stelle, nicht als Annotation in 29 Dateien.

Der zweite Block ist echte Substanz. Sechs Stellen in `scripts/tests/worktree-gc.sh` und `scripts/tests/worktree-gc-schedule.sh` beginnen eine Zeile mit `!`:

```
scripts/tests/worktree-gc.sh:103:  ! git -C "$repo" show-ref --verify --quiet refs/heads/test/stale
scripts/tests/worktree-gc.sh:574:  ! rg -Fq "keep outside_scope $active_artifact_worktree" \
```

Beide Dateien laufen unter `set -euo pipefail`, und beide verlassen sich darauf, dass eine fehlgeschlagene Zusicherung das Skript abbricht — genau so funktionieren die Nachbarzeilen `[[ ! -d $stale_worktree ]]` und `rg -Fq "removed …"`. Für eine mit `!` negierte Kommandoliste schaltet bash `errexit` jedoch ausdrücklich ab. Existiert der Zweig `test/stale` also weiterhin, obwohl die GC ihn hätte löschen müssen, liefert die Zeile Exit-Status 1, `errexit` greift nicht, das Skript läuft weiter und der Test endet grün. Sechs Zusicherungen, die nicht rot werden können, in ausgerechnet dem Testskript, das die Löschlogik für Worktrees und Zweige absichert.

Der Rest zerfällt in drei Gruppen. Erstens ein knappes Dutzend kleiner, eindeutiger Defekte: tote Variablen, ein `mkdir -m` mit falscher Reichweite, eine Präfix-Zuweisung, die der folgende Ausdruck nicht sieht, `A && B || C` statt if-then-else. Zweitens Meldungen, die aus der Bauweise der Harnesse folgen und keine Defekte sind: Konstantenbibliotheken, deren Variablen der Aufrufer liest, Umgebungsvariablen über eine Prozessgrenze hinweg, Trap-Handler. Drittens 30 × SC2016 und 7 × SC1003 — einfache Anführungszeichen in SQL-, awk- und Heredoc-Text, wo Expansion gerade nicht erwünscht ist.

Für Gruppe zwei und drei gibt es einen Ausweg, der ohne Ausnahmenliste und ohne Kommentarmüll auskommt und den die Messung von selbst nahelegt: **jede einzelne dieser Meldungen liegt auf Stufe `note`.** Alle sechs SC2251, beide SC2329, beide SC2094, alle sechs SC2015, alle SC2153, alle SC2016, alle SC1003, alle SC1091 sind Notes. Alle 63 `warning`-Meldungen dagegen stammen aus SC2034, SC1007, SC1090, SC1112, SC2097, SC2098, SC2154 und SC2174. Ein Gate auf `-S warning` liest die Notes gar nicht. Damit braucht keine einzige der Meldungen, für die der Auftrag ein `disable` verbietet, jemals ein `disable` — sie brauchen ein Urteil, und wo das Urteil „Defekt" lautet, eine Reparatur.

## Entscheidungen mit Begründung

**E1 — `.shellcheckrc` im Wurzelverzeichnis statt 29 Dateiannotationen.** Eine Datei mit `external-sources=true` und `source-path=SCRIPTDIR` beseitigt alle 29 SC1091. shellcheck sucht die Datei im Verzeichnis des geprüften Skripts und in allen Elternverzeichnissen, eine Datei an der Wurzel deckt also Skripte in `scripts/`, `scripts/ptr-e2e/`, `build-aux/`, `acceptance/` und `docs/evidence/` gleichermaßen ab. Entscheidend ist der Nebeneffekt: Editor-Integration, ein blanker `shellcheck datei.sh` von Hand und das Gate sehen dann dasselbe Ergebnis. Ein `-P` nur im Gate-Skript hätte die Notes zwar aus dem Gate ferngehalten, aber jeden Entwickler weiter mit 29 Falschmeldungen versorgt. `external-sources` lässt sich aus Sicherheitsgründen nur über die rc-Datei setzen, nicht per Direktive im Skript — die rc-Datei ist also ohnehin der vorgesehene Ort. Das Gate übergibt `-x -P SCRIPTDIR` zusätzlich explizit, damit es auch dann deterministisch bleibt, wenn es einmal aus einem fremden Arbeitsverzeichnis heraus gerufen wird; beide Angaben müssen übereinstimmen und tun das nachweislich (gemessen: identische 116 Meldungen).

**E2 — Das Gate scheitert auf `-S warning`, und die verbleibenden Notes bleiben sichtbar statt stumm.** Die Alternative wäre, auch auf Note-Ebene auf null zu gehen. Das kostete rund 30 `disable`-Kommentare für SC2016 in `scripts/tests/cua-e2e.sh` und Nachbarn — also genau die Kommentarschicht, die der Auftrag vermeiden will, und zwar an Stellen, wo einfache Anführungszeichen offensichtlich Absicht sind. `-S warning` zieht die Grenze dort, wo shellcheck selbst sie zieht: `warning` heißt „das ist mit hoher Wahrscheinlichkeit falsch", `note` heißt „das ist möglicherweise unsauber". Das Gate wird damit zu einem Wächter mit Aussagekraft statt zu einem Stilrichter. Damit die Notes trotzdem nicht unsichtbar verrotten, bekommt `scripts/check-shell.sh` einen Schalter `--report`, der alle Stufen ausgibt und dennoch nur an `warning` scheitert. Der Bericht ist ein Werkzeug für Menschen, kein zweites Gate.

**E3 — Kein `disable` für SC2251, SC2329, SC2094, SC2015, SC2153, SC2016, SC1003.** Folgt aus E2: diese Meldungen liegen alle unter der Gate-Schwelle. Sie werden gelesen und entschieden, und das Ergebnis ist entweder eine Reparatur im Code oder ein Satz in diesem Plan — nie ein Kommentar, der eine Meldung stummschaltet, die ohnehin niemanden aufhält.

**E4 — Die sechs SC2251 sind ein Defekt und werden repariert, mit Mutationsprobe.** Begründung oben unter Problem und Ursache. Die Reparatur ersetzt `! kommando …` durch eine Form, in der der Fehlschlag tatsächlich abbricht. Statt sechs Mal `if kommando; then echo … >&2; exit 1; fi` auszuschreiben, bekommen beide Dateien eine kleine Hilfsfunktion in der Art `refute() { if "$@"; then printf 'refute failed: %s\n' "$*" >&2; exit 1; fi; }`. Die zwei Aufrufe mit Here-String (`! rg -Fq … <<<"$report"`) funktionieren damit unverändert, weil die Umleitung am Aufruf von `refute` hängt und `rg` sie als stdin erbt. Nebengewinn gegenüber shellchecks eigenem Vorschlag `kommando && exit 1`: die fehlgeschlagene Zusicherung sagt beim Abbruch, welche es war — heute sagt eine gerissene Zusicherung in diesen Dateien gar nichts.

**E5 — Die zwei SC2329 sind Falschmeldungen und bleiben unangetastet.** `cleanup_private` in `scripts/cua-explore/run.sh:103` wird vier Zeilen später durch `trap cleanup_private EXIT` installiert, `cleanup_app` in `scripts/playback-history-smoke.sh:313` fünf Zeilen später durch `trap cleanup_app EXIT`. Beide sind innerhalb einer anderen Funktion beziehungsweise eines `case`-Zweigs definiert, was shellchecks Erreichbarkeitsanalyse nicht durchdringt; die Meldung nennt diesen Fall selbst („or ignored if invoked indirectly"). Kein toter Code, keine fehlende Verdrahtung, keine Änderung, kein `disable`.

**E6 — Die zwei SC2094 sind eine bewusste Selbstinspektion und bleiben unangetastet.** In `scripts/check-display-tests.sh` leitet der Arbeiterblock seine gesamte Ausgabe nach `"$results_dir/$index.log"` um (Zeile 218), und Zeile 208 liest dieselbe Datei mit `grep -q "Failed to initialize GTK"`, um zu entscheiden, ob ein Testlauf wegen eines nie hochgekommenen Displays wiederholt wird. Es ist keine Pipeline, sondern eine Blockumleitung; der geprüfte Kindprozess ist beim `grep` bereits beendet und seine Ausgabe geflushed. Der umgebende Kommentar zeigt, dass die Konstruktion durchdacht ist. Ein Umbau auf eine Zwischendatei brächte kein Verhalten und ein Risiko.

**E7 — Die sechs SC2015 in `scripts/check-logo-artwork.sh` werden zu if-then-else, obwohl sie unter der Gate-Schwelle liegen.** `ok()` ist `printf …`, sein Rückgabewert ist der von `printf`. In `[[ bedingung ]] && ok "…" || bad "…"` läuft `bad` also nicht nur, wenn die Bedingung falsch ist, sondern auch, wenn `printf` scheitert — und `bad` setzt `fail=1`. Das ist ein Pfad zu einem falschen Rot in einem Gate, und ein falsches Rot ist teurer als ein falsches Grün, weil es das Vertrauen in alle anderen Aussagen des Skripts mitnimmt. Die Wahrscheinlichkeit ist klein, die Reparatur ist mechanisch, und das Repo hat diese Klasse bereits einmal explizit ausformuliert: `scripts/check-display-tests.sh:181` trägt den Kommentar „An explicit if: under `set -e` a bare `[[ … ]] && break` would abort the worker on the common case". Die if-Form ist hier die Hausform.

**E8 — Die fünf SC2153 sind Prozess- und Indirektionsgrenzen, keine Tippfehler.** Nachgesehen: `SCRATCH` in `scripts/cua-e2e/filter_clear_matrix.sh:97` wird in Zeile 360 derselben Datei als `SCRATCH="$scratch"` an ein `bash "${BASH_SOURCE[0]}"` weitergereicht — das Skript ruft sich selbst als Kindprozess auf, `$scratch` und `$SCRATCH` sind zwei Seiten derselben Übergabe. `APP_LOG` und `WINDOW_ID` werden in `scripts/cua-e2e/run.sh:171/190` und `scripts/cua-e2e/scrobbling.sh:94/114` gesetzt und von den Szenariodateien gelesen. Die zwei zusätzlichen Meldungen, die erst unter `-P SCRIPTDIR` auftauchen (`TRACK_ID_X`/`TRACK_ID_Y` in `scripts/ptr-e2e/run.sh:685`), stammen aus indirekter Zuweisung: `db_scalar_into TRACK_ID_X …` in Zeile 670 und 673 setzt die Variablen über ihren Namen. Keine Tippfehler, keine Änderung, kein `disable`.

**E9 — SC2034 wird je Variable geprüft, nicht je Regel.** Die Prüfung ist bereits gelaufen und hat sich gelohnt. In `scripts/ptr-e2e/geometry.sh` haben 26 der 28 gemeldeten Konstanten nachweislich Konsumenten in `scripts/ptr-e2e/*.sh`; `PRIMARY_MENU_FROM_RIGHT` und `SEARCH_TOGGLE_FROM_RIGHT` haben repoweit keinen. Sie sind aber nicht vergessen worden: der Kommentar direkt darüber sagt „record every slot explicitly so future pointer recalibration cannot accidentally target the revealed second top bar", und zwei Zeilen weiter erklärt die Datei sogar, welche Konstante sie bewusst *nicht* führt. Das ist absichtlich geführte Dokumentation, kein Rest. Ergebnis: `geometry.sh` bekommt genau ein `disable=SC2034` auf Dateiebene, dessen Begründung beide Tatsachen benennt — dass die Datei eine reine Konstantenbibliothek für ihre Aufrufer ist, und dass zwei Einträge das Header-Raster bewusst vollständig halten. `APP_ID` in `scripts/cua-e2e/run.sh:26` und `scripts/ptr-e2e/run.sh:113` hat dagegen repoweit keinen Konsumenten und keine solche Begründung — die zwei Zuweisungen werden gelöscht, nicht stummgeschaltet.

**E10 — Das Gate erzwingt die Begründungspflicht selbst, statt sie dem Review zu überlassen.** Das Repo hat dafür bereits einen Präzedenzfall: `scripts/check-ai-hygiene.sh` prüft unter GP-20, dass jedes `#[allow(dead_code)]` einen Kommentar in derselben oder der vorhergehenden Zeile trägt, und zählt ein nacktes `allow` als Verstoß. `scripts/check-shell.sh` bekommt dieselbe Prüfung für `# shellcheck disable=`, mit derselben Regel für die Position der Begründung. Beide Formen sind gemessen zulässig — shellcheck akzeptiert sowohl `# shellcheck disable=SC2034 # Grund` als auch den Grund in der Zeile darüber. Damit ist Auflage 1 nicht mehr Reviewdisziplin, sondern rot oder grün.

**E11 — Die Dateiliste ist `git ls-files -z '*.sh' '.githooks/*'`.** Die Messung benutzte `git ls-files '*.sh'`; das ist fast richtig und hat eine Lücke: `.githooks/pre-push` ist ein von Hand geschriebenes bash-Skript ohne Endung, es ruft `check-merge-readiness.sh` auf und ist damit Teil der Gate-Kette — es fiele durch das Raster. Nachgemessen ist es auf allen Stufen sauber, kostet also nichts. Die einzige weitere getrackte Datei mit Shell-Shebang ohne `.sh` ist `android/gradlew`, generiert von Gradle und nicht unser Code; sie fällt durch dieselbe Regel heraus, ohne dass irgendwo ein Name auf einer Ausnahmenliste stehen muss. Die Liste ist damit eine Regel, keine Aufzählung: neue Skripte werden automatisch mitgeprüft, ein Gradle-Upgrade kann das Gate nicht wegen fremden Codes rot färben. Ergebnis heute: 99 Dateien.

**E12 — Das Gate hängt in `scripts/check-merge-readiness.sh`, nicht direkt in `scripts/ci-quality.sh`.** `ci-quality.sh` ist ein 31-zeiliger CI-Vorspann: er ermittelt den Basiszweig, prüft die Promotionsregel für `main` und ruft dann `check-merge-readiness.sh --no-fetch`. Sämtliche inhaltlichen Gates — `check-architecture.sh`, `check-appstream.sh`, `check-motion-tokens.sh`, `check-display-tests.sh` und die übrigen — liegen in `check-merge-readiness.sh`. Dort einzuhängen erfüllt die Auflage („in ci-quality.sh hängen") über die Aufrufkette `ci-quality.sh → check-merge-readiness.sh → check-shell.sh` und hat den entscheidenden Zusatznutzen, dass `.githooks/pre-push` dasselbe Gate lokal vor jedem Push ausführt. Direkt in `ci-quality.sh` eingehängt liefe es nur auf GitHub, und ein Entwickler erführe von seinem Shell-Fehler erst nach dem Push. Position innerhalb der Datei: unmittelbar nach `git diff --check` und vor `check-architecture.sh`, damit ein Shell-Fehler in Sekunden auffällt statt nach `cargo test`.

**E13 — Fehlendes shellcheck überspringt das Gate, aber CI installiert es.** `scripts/lib/rulebook.sh` bringt bereits `skip_gate_if_tool_missing` mit; `scripts/check-appstream.sh` benutzt es dreimal, `scripts/check-flatpak-manifest.sh` benutzt das darunterliegende `skip_gate`. `check-shell.sh` sourct dieselbe Bibliothek und ruft `skip_gate_if_tool_missing shellcheck` — damit ist die Auflage nicht nur erfüllt, sondern in der Form erfüllt, die das Repo bereits kennt. Gegengewicht: ein Gate, das überall übersprungen wird, ist kein Gate. Deshalb wird `shellcheck` in `.github/workflows/ci.yml` in die `pacman -Syu`-Liste des Quality-Jobs aufgenommen (alphabetisch zwischen `rust` und `sqlite`), und `scripts/tests/qa-linters.sh` bekommt ein `require_pattern 'shellcheck' .github/workflows/ci.yml`, damit niemand das Paket später entfernt und das Gate lautlos zum Dauerskip wird.

**E14 — Erst säubern, dann scharfschalten, in getrennten Commits.** Die Säuberungswellen enthalten keine Zeile Gate-Code, die Gate-Welle enthält keine Zeile Skriptsäuberung. Ein Gate, das beim Einbau schon rot ist, wird abgeschaltet statt repariert — und ein vermischter Commit macht zudem unlesbar, ob eine Verhaltensänderung an einem Testskript vom Autor gewollt oder vom Linter erzwungen war.

## Umsetzung in Wellen

### Welle 0 — gemeinsame Messgrundlage

Eine neue Datei `.shellcheckrc` im Wurzelverzeichnis mit `external-sources=true`, `source-path=SCRIPTDIR` und einem kurzen Kommentar, warum sie existiert. Sonst nichts. Diese Welle geht zuerst in `feature/shellcheck`, weil alle drei Stränge danach identisch messen und weil eine getrennt gelesene Datei ohne Codeänderung eine Konfliktfläche von null hat.

Nachweis: `shellcheck -x -f gcc $(git ls-files '*.sh')` liefert danach ohne weitere Schalter dieselben 116 Meldungen wie der Referenzlauf mit `-P SCRIPTDIR`, und keine einzige SC1091 mehr.

### Welle 1 — die 54 Warnungen, in drei parallelen Strängen

Die vollständige Arbeitsliste, gemessen mit `shellcheck -x -P SCRIPTDIR -S warning`:

*E2E- und Harness-Skripte (Strang A):*

- `scripts/ptr-e2e/geometry.sh` — 28 × SC2034. Ein `# shellcheck disable=SC2034` auf Dateiebene, direkt unter dem Kopfkommentar, mit einer Begründung darüber, die (a) die Datei als Konstantenbibliothek für `scripts/ptr-e2e/*.sh` benennt, (b) festhält, dass `PRIMARY_MENU_FROM_RIGHT` und `SEARCH_TOGGLE_FROM_RIGHT` heute keinen Konsumenten haben und das Raster bewusst vollständig halten, und (c) die Regel nennt: eine Konstante wird gelöscht, wenn ihr letzter Konsument geht.
- `scripts/ptr-e2e/preferences.sh:34,86,89` — 3 × SC2034. Zwei gezielte `disable`-Kommentare statt eines auf Dateiebene, damit eine künftige echte Leiche in dieser Datei weiter auffällt: einer über `WINDOW_ID="$(xdotool getactivewindow …)"` mit der Begründung, dass fünf andere `ptr-e2e`-Dateien die Variable lesen (`column-reorder.sh`, `compact-seek.sh`, `run.sh`, `search-chip.sh`, `window-helpers.sh`), einer über der `local ROW_*`-Leiter mit der Begründung, dass die Zeilenleiter vollständig geführt wird, damit eine Neuvermessung die benutzten Zeilen nicht stillschweigend verschiebt.
- `scripts/cua-e2e/run.sh:26` und `scripts/ptr-e2e/run.sh:113` — 2 × SC2034 auf `APP_ID`. Repoweit ohne Konsumenten (vor dem Löschen noch einmal gegen `*.py`, `meson.build`, `*.yml` und `*.rs` gegenprüfen, weil eine exportierte Variable auch außerhalb der Shell gelesen werden könnte). Zuweisung löschen.
- `scripts/cua-e2e/scrobbling.sh:43` — SC2154 auf `repo_root`. Kein `disable`, sondern die Bibliothek selbsttragend machen: `repo_root=${repo_root:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}` nahe dem Dateikopf. Beim Sourcen aus `run.sh` verhaltensgleich, standalone korrekt.
- `scripts/cua-e2e/selection_anchor.sh:109` — SC2034 auf `b`. `read -r r g b` → `read -r r g _`; das letzte Feld schluckt den Rest wie zuvor, und `_` löst SC2034 nachweislich nicht aus.
- `scripts/cua-e2e/source_content.sh:136,145` — 2 × SC1007. `LANGUAGE= \` → `LANGUAGE='' \`. Identische Semantik (leere Umgebungsvariable für das folgende Kommando), shellchecks eigener Vorschlag.
- `scripts/cua-common/session.sh:190,192` — SC2097 + SC2098. Den zusammengesetzten Wert einmal in eine lokale Variable heben (`local stub_data_dirs="$stub_root:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"`) und sowohl in der Präfixzuweisung vor `dbus-run-session` als auch im inneren `env` diese Variable benutzen. Beweisbar verhaltensgleich — beide Ausdrücke expandieren heute schon im Elternprozess zum selben String —, aber die Absicht ist danach lesbar statt geraten.
- `scripts/playback-history-smoke.sh:212` — SC2034 auf `second`. Die Zuweisung entfernen, den Aufruf als `wait_for_title '' "$first" >/dev/null` stehen lassen; die Wartewirkung ist der Zweck, der Rückgabewert wird nirgends gelesen. Form wie zwei Zeilen darunter.
- `scripts/ptr-e2e/compact-seek.sh:59` — SC2034 auf `attempt`. `for attempt in $(seq 1 4)` → `for _ in $(seq 1 4)` (dieselbe Datei benutzt vier Zeilen tiefer bereits genau diese Form), `attempt` aus der `local`-Zeile streichen.
- `acceptance/deezer-placeholder-portraits/run-accept.sh:637` — SC2034 auf `total`. `while IFS='|' read -r rank name total` → `read -r rank name _`. Die dritte Position muss bleiben, sonst zieht `name` das dritte Feld an sich; `_` benennt sie als absichtlich verworfen.

*Build-, Gate- und Werkzeugskripte (Strang B):*

- `build-aux/meson-cargo-build.sh:8,10` — 2 × SC2034 auf `stem_backend` und `worker_path`. Beide werden aus `$5` und `$7` gelesen und nirgends benutzt; der Kommentar ab Zeile 18 erklärt, warum: die Option entscheidet inzwischen nur noch über ein anderes Ziel. `prefix` (`$6`) existiert ausschließlich als Default für `worker_path` und wird mit fallen. Vor dem Löschen die Aufrufstelle in `meson.build` lesen und im Kommentar festhalten, dass die Argumente 5 bis 7 weiterhin übergeben und bewusst ignoriert werden — die Aufrufsignatur bleibt unangetastet, nur die toten Zuweisungen gehen.
- `scripts/check-logo-artwork.sh:351` — SC2034 auf `dimensions`. Aus der `local`-Zeile streichen.
- `scripts/reprise-worktree-gc.sh:323,416` — 6 × SC1007. `local path= head= branch= locked=false line` → `local path='' head='' branch='' locked=false line`, an beiden Stellen. Semantisch identisch.
- `scripts/verify-radio-favicons.sh:43` — SC2174. `mkdir -p -m 0700 "$out_dir"` → `mkdir -p "$out_dir"`; das `chmod 0700 "$out_dir"` fünf Zeilen darunter setzt die Rechte ohnehin unbedingt und ist die eigentliche Zusicherung, während `-m` bei `-p` nur das tiefste Verzeichnis erreicht — also genau das, was die Meldung sagt. Falls die kurze Lücke zwischen `mkdir` und `chmod` als Risiko bewertet wird, ist die Alternative ein `disable=SC2174` mit ebendieser Begründung; die Entscheidung gehört in den Commit, nicht in dieses Dokument.
- `docs/evidence/bounded-daemon-stop/probe-stop-daemon.sh:6` — SC1090. Kein `disable`, sondern die Direktive, die die Meldung selbst verlangt: `# shellcheck source=../../../scripts/cua-common/session.sh` über dem `source "$1"`. Das schweigt nicht nur, es liefert shellcheck die Analyse, die ihm fehlte.

*Wächter (Strang C):*

- `scripts/tests/readme-showcase.sh:104` — SC1112. Das `’` (U+2019) in `'## Roadmap: the same core beyond today’s player'` ist der wörtliche Text einer zurückgezogenen README-Überschrift, den `reject_fixed` verbietet. Es durch ein ASCII-Apostroph zu ersetzen, würde die Prüfung stillschweigend entschärfen: sie träfe die alte Überschrift dann nicht mehr, falls sie zurückkäme. Also ein `disable=SC1112` mit genau dieser Begründung — eine der wenigen Stellen, an der Stummschalten die richtige Antwort ist, weil das Zeichen die Zusicherung *ist*.

### Welle 2 — die Note-Befunde, die echte Defekte sind

Getrennt von Welle 1, weil hier Verhalten geändert wird und der Nachweis anders aussieht.

- **Die sechs SC2251** (`scripts/tests/worktree-gc.sh:103,185,217,313,574`, `scripts/tests/worktree-gc-schedule.sh:19`) nach E4. **Mutationsprobe, je Stelle einzeln:** die Zusicherung so verfälschen, dass sie fehlschlagen *muss* — für `! git … show-ref refs/heads/test/stale` etwa auf `refs/heads/main` umbiegen, das im Fixture garantiert existiert; für `! rg -Fq "keep outside_scope …"` das Muster auf eine Zeile ändern, die im Bericht nachweislich steht. Dann das Testskript laufen lassen und einen Exit-Status ungleich null protokollieren. Anschließend zurückdrehen und den grünen Lauf protokollieren. Die Gegenprobe auf dem *heutigen* Stand — dieselbe Verfälschung vor der Reparatur, Testskript endet trotzdem grün — gehört in dieselbe Protokollzeile, weil sie den Defekt belegt, statt ihn zu behaupten. Sechs Stellen, sechs Zeilenpaare.
- **Die sechs SC2015** in `scripts/check-logo-artwork.sh:178,218,221,364,383,386` nach E7 auf `if/then/else` umstellen. Nachweis: `scripts/check-logo-artwork.sh` läuft weiterhin durch, und die Ausgabe (`ok`/`FAIL`-Zeilen) ist Zeile für Zeile identisch mit dem Lauf davor.
- **SC2181** in `scripts/cua-e2e/filter_clear_matrix.sh:364`: `[[ $? -eq 0 ]] || failed+=("$case_name")` prüft den Status des vorangehenden mehrzeiligen `env … bash …`-Aufrufs. Die Datei läuft unter `set -uo pipefail` ohne `-e`, das Konstrukt funktioniert also heute — es ist nur eine Kette, die beim nächsten Einschub dazwischen zerbricht. Umbau auf `if ! env … ; then failed+=("$case_name"); fi`.
- **SC2004** in `scripts/reprise-worktree-gc.sh:588`: `excluded_paths[$excluded_index]` → `excluded_paths[excluded_index]`.
- **SC2086 ×2**: `scripts/performance-runtime-baseline.sh:462` prüfen und quoten. `build-aux/meson-cargo-build.sh:30` ist ein `#!/bin/sh`-Skript, in dem `$cargo_profile_args $cargo_feature_args` bewusst wortgetrennt wird und Arrays nicht zur Verfügung stehen; die Datei baut ihre Kommandozeile ohnehin bereits über `set -- env …` auf, die Flags gehören in dieselbe Positionsliste. Damit fällt die Meldung ohne `disable` und ohne Semantikänderung.
- **SC1003 ×7** (`scripts/ptr-e2e/harness-self-test.sh`, `scripts/tests/motion-tokens.sh`) und **SC2016 ×30**: gelesen, keine Defekte — Backslashes in awk- und sed-Programmen, einfache Anführungszeichen um SQL, awk-Programme und Heredoc-Text, in denen Expansion gerade verhindert werden soll. Keine Änderung, kein `disable`, unter der Gate-Schwelle.

### Welle 3 — das Gate

Erst wenn Welle 1 und 2 vollständig zusammengeführt sind und der Vollprüflauf leer ist.

`scripts/check-shell.sh`, ausführbar, nach dem Muster der bestehenden Gates: `#!/usr/bin/env bash`, `set -euo pipefail`, `cd "$(git rev-parse --show-toplevel)"`, `source scripts/lib/rulebook.sh`, `skip_gate_if_tool_missing shellcheck`. Danach die Dateiliste über `git ls-files -z '*.sh' '.githooks/*'` in ein Array (`mapfile -d '' files < <(…)`), damit Pfade mit Sonderzeichen nicht zerfallen, und ein Lauf `shellcheck -x -P SCRIPTDIR -S warning -f gcc -- "${files[@]}"`. Zusätzlich die Begründungsprüfung nach E10 über dieselbe Liste. `--report` gibt denselben Lauf ohne `-S warning` aus, scheitert aber weiterhin nur an Warnungen. Das Skript nennt in seiner ersten Ausgabezeile die gefundene shellcheck-Version und die Zahl der geprüften Dateien, damit ein Skip und ein Versionssprung im CI-Protokoll sichtbar sind statt still.

Verdrahtung, alles in einem Commit: Aufruf in `scripts/check-merge-readiness.sh` nach `git diff --check` (E12); `shellcheck` in die pacman-Liste in `.github/workflows/ci.yml` (E13); in `scripts/tests/qa-linters.sh` ein `require_executable scripts/check-shell.sh`, ein `require_pattern 'check-shell.sh' scripts/check-merge-readiness.sh` und ein `require_pattern 'shellcheck' .github/workflows/ci.yml`; in `TESTING.md` unter „Required merge gates" ein Satz, der das Gate, seine Schwelle und sein Skip-Verhalten benennt. Ohne diese vier Ergänzungen wäre der neue Wächter selbst der einzige ungeprüfte, undokumentierte Wächter im Repo — genau der Zustand, den dieser Plan beseitigt.

## Abnahmekriterien

1. `shellcheck -x -P SCRIPTDIR -S warning -f gcc $(git ls-files '*.sh' '.githooks/*')` gibt nichts aus und endet mit 0. Referenz: heute 54 Meldungen aus 17 Dateien.
2. `scripts/check-shell.sh` endet mit 0 und meldet 99 geprüfte Dateien.
3. `PATH` ohne shellcheck: `scripts/check-shell.sh` schreibt `SKIPPED: shellcheck is not installed; this gate did not run` nach stderr und endet mit 0.
4. Ein absichtlich eingebauter Verstoß (etwa eine unbenutzte Variable in einem beliebigen Skript) färbt `scripts/check-shell.sh` rot — sonst ist Kriterium 2 wertlos. Danach zurückdrehen.
5. Ein `# shellcheck disable=SC2034` ohne Begründung, testweise eingefügt, färbt `scripts/check-shell.sh` rot. Danach zurückdrehen.
6. Die sechs Mutationsproben aus Welle 2 liegen protokolliert vor, je mit dem roten Lauf nach der Reparatur und dem grünen Lauf davor.
7. `bash scripts/tests/worktree-gc.sh` und `bash scripts/tests/worktree-gc-schedule.sh` enden mit 0.
8. `bash scripts/tests/qa-linters.sh` endet mit 0.
9. `bash -n` über alle 99 Dateien endet fehlerfrei.
10. `scripts/ptr-e2e/harness-self-test.sh` endet mit 0.
11. `scripts/check-logo-artwork.sh` erzeugt dieselbe `ok`/`FAIL`-Ausgabe wie vor Welle 2.
12. Die Gesamtzahl der `# shellcheck disable=`-Zeilen im Repo ist vier — `geometry.sh` (1), `preferences.sh` (2), `readme-showcase.sh` (1) —, jede mit Begründung. Weicht die Zahl ab, gehört die Abweichung in die Commit-Beschreibung; sie ist der Griff, an dem das Review die Ausnahmenliste kontrolliert, die es laut Zielsetzung nicht geben soll.
13. `git grep -c 'shellcheck disable' -- '*.sh'` enthält keine Datei aus `scripts/tests/` außer `readme-showcase.sh`, und keine für SC2251, SC2329, SC2094, SC2015, SC2153.

## Risiken

**Das Auflösen der `source`-Pfade legt neue Befunde frei.** Bereits gemessen: SC2153 steigt von 3 auf 5, weil shellcheck unter `-P SCRIPTDIR` weiter sieht. Beide neuen Meldungen sind Notes und harmlos (E8), aber das Muster wiederholt sich: wer künftig eine `source`-Zeile hinzufügt, kann Meldungen in einer Datei auslösen, die er nicht angefasst hat. Das ist der Preis für echte Analyse statt Blindflug und wird durch die Gate-Schwelle gedämpft — Notes halten niemanden auf.

**Die SC2251-Reparatur ändert Kontrollfluss in den zwei Testskripten, die Worktree-Löschung absichern.** Wenn eine der sechs Zusicherungen heute in Wahrheit fehlschlägt und das nur wegen des defekten `errexit` niemandem auffällt, wird der Test nach der Reparatur rot — zu Recht, aber überraschend. Der Plan behandelt das nicht als Rückschlag: ein rot gewordener Wächter ist das Ergebnis, für das die Reparatur gemacht wurde. Der Befund gehört dann untersucht und als eigener Punkt behoben, nicht durch Zurückdrehen der Reparatur erledigt.

**Löschen scheinbar toter Variablen kann eine Übergabe kappen.** Betrifft `APP_ID`, `stem_backend`, `worker_path`, `prefix`. Eine exportierte Shell-Variable kann von Python, meson oder einem Rust-Test gelesen werden, ohne dass sie in einer `*.sh` auftaucht. Gegenmaßnahme: vor jedem Löschen ein repoweites `git grep -F` über alle Dateitypen, Ergebnis in die Commit-Beschreibung. Für `meson-cargo-build.sh` zusätzlich die Aufrufstelle in `meson.build` lesen.

**Versionsdrift von shellcheck.** Lokal 0.11.0, im CI-Container das jeweils aktuelle Arch-Paket. Eine neue shellcheck-Version kann Regeln hinzufügen und einen unbeteiligten Zweig rot färben. Ein Pin wäre eine Baseline unter anderem Namen und würde genauso verrotten; die Antwort ist stattdessen, dass `check-shell.sh` seine Version protokolliert, damit ein solcher Fall in fünf Sekunden als Versionssprung erkennbar ist statt als Rätsel.

**Der Datei-`disable` in `geometry.sh` kann eine künftige echte Leiche verbergen.** Bewusst in Kauf genommen, weil die Datei zu 100 % aus Konstanten für andere Dateien besteht und 28 Einzelkommentare unlesbar wären. Die Begründung nennt die Regel, unter der eine Konstante zu löschen ist. In `preferences.sh` — wo neben den Konstanten auch normaler Code steht — wird deshalb ausdrücklich *nicht* auf Dateiebene stummgeschaltet.

**`skip_gate_if_tool_missing` macht das Gate auf jedem Rechner ohne shellcheck lautlos folgenlos.** Deshalb ist die pacman-Ergänzung in `ci.yml` kein Beiwerk, sondern trägt die halbe Wirkung des Vorhabens, und deshalb sichert `qa-linters.sh` sie mit einem `require_pattern` ab.

## Ausdrücklich nicht Gegenstand

Keine Baseline-, Ausnahme- oder Unterdrückungsdatei, in keiner Form — das ist die Zielsetzung, nicht eine Abwägung. Kein Absenken der Schwelle auf `style` oder `info` und keine Behandlung der nach Welle 2 verbleibenden Notes als Fehler; sie sind gelesen, entschieden und dokumentiert. Kein `shfmt`, keine Formatierungs-, Umbenennungs- oder Strukturarbeit an Skripten über das hinaus, was ein konkreter Befund verlangt. Kein Umbau der cua-/ptr-Harnesse, insbesondere nicht der Prozessgrenzen aus E8 oder der Selbstinspektion aus E6. `android/gradlew` und alles andere Fremdgenerierte bleibt außen vor. Keine Linter für die zahlreichen Python-Skripte unter `scripts/` und `scripts/tests/` — eigenes Vorhaben, eigene Entscheidungen. Keine Änderung an der Promotionslogik in `ci-quality.sh` und keine Umsortierung der bestehenden Gates in `check-merge-readiness.sh` außer dem Einfügen der einen neuen Zeile.

## Parallelität

Die Arbeit lässt sich schneiden, weil die 54 Warnungen über 17 Dateien in drei disjunkte Verzeichnisfamilien fallen und weil keine Reparatur eine andere braucht. Drei Stränge, danach eine Integrationsstufe, die keinem Strang gehört.

**Strang A — E2E- und Harness-Skripte.**
Zweck: die 40 Warnungen in den GUI-Testharnessen beseitigen, samt der Urteile zu SC2034 in den Konstanten- und Fragmentdateien (E9) und der Falschmeldungen aus E5 und E8, für die nichts zu tun ist außer der Feststellung im Commit.
Dateibesitz: `scripts/ptr-e2e/**`, `scripts/cua-e2e/**`, `scripts/cua-common/**`, `scripts/cua-explore/**`, `scripts/playback-history-smoke.sh`, `acceptance/**`.
Aufgaben: `geometry.sh` (28 × SC2034, Datei-`disable` mit dreiteiliger Begründung); `preferences.sh` (2 gezielte `disable`); `APP_ID` in beiden `run.sh` repoweit gegenprüfen und löschen; `scrobbling.sh` `repo_root` selbsttragend machen; `selection_anchor.sh` `read -r r g _`; `source_content.sh` 2 × `LANGUAGE=''`; `session.sh` `stub_data_dirs` heben; `playback-history-smoke.sh` `second` fallen lassen; `compact-seek.sh` `for _ in`; `run-accept.sh` `read -r rank name _`; `filter_clear_matrix.sh` SC2181 auf `if !` umbauen. Feststellungen ohne Codeänderung: SC2329 in `cua-explore/run.sh:103` und `playback-history-smoke.sh:313` (Trap-Handler), SC2153 in `filter_clear_matrix.sh`, `filter_clear_playback.sh`, `selection_anchor.sh`, `ptr-e2e/run.sh:685`.
Eigenprüfung vor dem Zusammenführen: `shellcheck -x -P SCRIPTDIR -S warning` über die eigenen Globs ist leer; `bash -n` über jede angefasste Datei; `scripts/ptr-e2e/harness-self-test.sh` endet mit 0.

**Strang B — Build-, Gate- und Werkzeugskripte.**
Zweck: die 11 Warnungen außerhalb der Harnesse beseitigen und die zwei Note-Defekte in den Gate-Skripten reparieren.
Dateibesitz: `build-aux/**`, `scripts/check-*.sh`, `scripts/reprise-worktree-gc.sh`, `scripts/verify-radio-favicons.sh`, `scripts/performance-runtime-baseline.sh`, `docs/evidence/**`.
Aufgaben: `meson-cargo-build.sh` (2 × SC2034 löschen nach Prüfung von `meson.build`, SC2086 über die bestehende `set --`-Liste auflösen); `check-logo-artwork.sh` (`dimensions` streichen, 6 × SC2015 auf `if/then/else`); `reprise-worktree-gc.sh` (6 × SC1007 auf `''`, SC2004 in Zeile 588); `verify-radio-favicons.sh` (SC2174); `probe-stop-daemon.sh` (`# shellcheck source=`-Direktive); `performance-runtime-baseline.sh` (SC2086). Feststellung ohne Codeänderung: SC2094 in `check-display-tests.sh:208,218`.
Eigenprüfung: eigene Globs warnungsfrei; `scripts/check-logo-artwork.sh` liefert dieselbe Ausgabe wie vorher; `meson setup` bzw. ein Build, der `build-aux/meson-cargo-build.sh` tatsächlich aufruft.

**Strang C — Wächter, die nicht rot werden können.**
Zweck: die sechs SC2251 reparieren und beweisen, dass die betroffenen Tests danach fehlschlagen können. Getrennter Strang, weil das der einzige Teil des Vorhabens ist, der Verhalten ändert, und weil er einen anderen Nachweis braucht als „shellcheck schweigt".
Dateibesitz: `scripts/tests/**`.
Aufgaben: `refute`-Hilfe in `worktree-gc.sh` und `worktree-gc-schedule.sh` einführen, sechs Stellen umstellen, sechs Mutationsproben mit Vorher/Nachher protokollieren; `readme-showcase.sh` SC1112 mit Begründung stummschalten. Feststellungen ohne Codeänderung: SC2016 in `cua-e2e.sh`, `cua-explore.sh`, `github-flow.sh`, `qa-linters.sh`; SC1003 in `motion-tokens.sh`.
Eigenprüfung: `scripts/tests/worktree-gc.sh`, `scripts/tests/worktree-gc-schedule.sh`, `scripts/tests/readme-showcase.sh`, `scripts/tests/motion-tokens.sh` enden je mit 0.

**Merge-Reihenfolge.** Welle 0 (`.shellcheckrc`) ist Vorbedingung für alle drei Stränge und geht zuerst; ohne sie messen die Stränge unterschiedlich und streiten über SC1091-Rauschen, das gar keins ist. A, B und C sind untereinander unabhängig und können in beliebiger Reihenfolge zusammengeführt werden — ihre Globs überschneiden sich nirgends. `scripts/check-shell.sh` und die Verdrahtung sind **kein Strang**: sie folgen als Welle 3 nach allen drei Merges. Ein Gate, das vor der letzten Säuberung eingebaut wird, ist beim Einbau rot, und ein rotes neues Gate wird abgeschaltet statt repariert.

**Nachmerge-Kreuzprüfungen.** Jede dieser Prüfungen liest Dateien, die keinem einzelnen Strang gehören, und kann deshalb erst nach dem Zusammenführen aussagekräftig laufen:

1. Vollständiger Lauf `shellcheck -x -P SCRIPTDIR -S warning -f gcc $(git ls-files '*.sh' '.githooks/*')` — leer. Liest alle drei Stränge; kein Strang kann das für sich belegen, weil das Gate global ist.
2. `scripts/check-shell.sh` grün, und derselbe Aufruf mit einem `PATH` ohne shellcheck endet mit 0 und meldet den Skip. Liest die gesamte Dateiliste plus `scripts/lib/rulebook.sh`, das keinem Strang gehört.
3. `scripts/tests/worktree-gc.sh` und `scripts/tests/worktree-gc-schedule.sh` gegen den zusammengeführten Baum. **Die schärfste Kreuzprüfung des Vorhabens:** Strang B ändert den Prüfling `scripts/reprise-worktree-gc.sh` (sechs SC1007, ein SC2004), Strang C ändert die Tests, die ihn ausführen — keiner der beiden sieht die Änderung des anderen vor dem Merge.
4. Mindestens eine Mutationsprobe aus Welle 2 wird nach dem Merge wiederholt, und zwar an einer Stelle in `worktree-gc.sh`, die den von Strang B geänderten Runner aufruft. Grund wie oben: der Nachweis „der Test kann rot werden" wurde gegen einen Runner erbracht, den es so nicht mehr gibt.
5. `bash scripts/tests/qa-linters.sh`. Liest `scripts/check-merge-readiness.sh` (Welle 3), `.github/workflows/ci.yml` (Welle 3), `TESTING.md` (Welle 3) und die Testskripte aus Strang C.
6. Inventar der `disable`-Kommentare: `git grep -n 'shellcheck disable' -- '*.sh' '.githooks/*'` liefert genau vier Treffer, jeder mit Begründung in derselben oder der vorhergehenden Zeile, keiner für eine der Regeln aus E3. Liest Strang A, B und C gemeinsam; die Zahl ist nur über alle drei hinweg prüfbar.
7. `bash -n` über alle 99 Dateien der Gate-Liste.
8. `scripts/ptr-e2e/harness-self-test.sh` und `scripts/check-logo-artwork.sh` erneut, weil beide gesourcte Dateien aus jeweils dem anderen Strang berühren beziehungsweise gemeinsame Muster teilen.
