# Übergabe — shellcheck als Gate

Stand: 2026-08-14, 18:35 CEST. Entwurf: `docs/plans/shellcheck.draft.md`
(`phase: planned`, Statusblock steht). **Der Grill hat noch nicht stattgefunden —
genau dort steigt die nächste Sitzung ein.**

## Wo die Arbeit steht

- **Worktree** `/home/marvin/Projects/reprise-shellcheck`, Zweig
  `feature/shellcheck`, aufgesetzt auf `origin/dev` = `a6a0d11604`.
- **Nichts committet außer dieser Übergabe und dem Entwurf.** Kein Codex-Lauf
  gestartet, kein PR, kein Gate gebaut.
- Wake-Lock `shellcheck-pipeline` ist **freigegeben** — es läuft nichts mehr.
  Wer weitermacht, nimmt vor dem ersten Codex-Lauf einen neuen
  (`wake-lock acquire shellcheck-pipeline "…"`), einen für den ganzen Lauf, nicht
  einen je Strang.
- Messprotokolle liegen im Scratchpad der alten Sitzung und sind **flüchtig** —
  bei Bedarf neu erzeugen, der Befehl steht unten.

## Wie es hierher kam

Vorlauf war #484 (Porträt-Platzhalter per Fingerabdruck, gelandet als
`a6a0d11604`). Am Ende jenes Laufs fiel auf, dass `shellcheck` weder installiert
war noch irgendwo im Repo aufgerufen wurde. Der Nutzer hat es daraufhin
installiert (0.11.0) und auf Rückfrage die **große** Variante gewählt: alle
Skripte säubern und ein Gate bei `warning` scharfschalten, **ohne Baseline-Datei**.
Die kleine Variante (nur der Abnahme-Prüfstand plus ein Selbsttest) wurde
ausdrücklich verworfen.

## Der Beschluss, der schon feststeht

Nicht mehr zur Disposition, das war die Entscheidung des Nutzers:

- Alle getrackten Shell-Skripte werden shellcheck-sauber.
- `scripts/check-shell.sh` scheitert repoweit bei Schweregrad `warning`.
- **Keine Baseline-, Ausnahme- oder Unterdrückungsdatei.** Sie verrottet.
- Jedes `# shellcheck disable=` trägt eine Begründung.
- Kein `disable` für die Regeln, die echtes Urteil brauchen (siehe unten).
- Verhaltensändernde Reparaturen an Testskripten brauchen eine **Mutationsprobe**,
  keine Behauptung.

## Die Messung — nachgeprüft, nicht übernommen

Der Planungsagent hat die Zahlen geliefert; ich habe die tragenden selbst
nachgemessen und sie stimmen. Reproduzieren:

```
cd /home/marvin/Projects/reprise-shellcheck
mapfile -t s < <(git ls-files '*.sh')          # in bash, nicht zsh!
shellcheck -x -f gcc "${s[@]}"                 # 152 Meldungen (63 warning, 89 note)
shellcheck -x -P SCRIPTDIR -f gcc "${s[@]}"    # 116 Meldungen (54 warning, 62 note)
```

| Befund | Wert |
|---|---|
| getrackte `*.sh` | 98, davon **66 vollständig sauber** |
| Schweregrade | **0 `error`**, ohne rc-Datei 63 `warning` / 89 `note` |
| mit `-P SCRIPTDIR` | 116 Meldungen, **SC1091 auf 0**, 54 `warning` / 62 `note` |
| `warning`-Regeln danach | SC2034 (40), SC1007 (8), SC2174/SC2154/SC2098/SC2097/SC1112/SC1090 (je 1) |

**Der Angelpunkt des ganzen Plans, selbst nachgemessen:** die sieben Regeln, für
die der Auftrag ein `disable` verbietet — SC2251, SC2329, SC2094, SC2015, SC2153,
SC2016, SC1003 — liegen **ausnahmslos** auf `note` (`warning=0` für jede
einzelne). Ein Gate bei `warning` liest sie also gar nicht. Sie brauchen ein
Urteil, nie ein Stummschalten. Das ist der Grund, warum der Entwurf ohne
Kommentarschicht auskommt, und die Zahl, an der der Grill zuerst rütteln sollte.

## Der schärfste inhaltliche Befund

Sechs Stellen in `scripts/tests/worktree-gc.sh` (103, 185, 217, 313, 574) und
`scripts/tests/worktree-gc-schedule.sh:19` beginnen eine Zeile mit `!`. Bash
schaltet für eine negierte Kommandoliste `errexit` **ab**. Beide Dateien laufen
unter `set -euo pipefail` und verlassen sich genau darauf, dass eine gerissene
Zusicherung abbricht. Ergebnis: sechs Zusicherungen, die nicht rot werden können
— ausgerechnet in dem Test, der die Löschlogik für Worktrees und Zweige
absichert. Der Entwurf repariert das über eine `refute`-Hilfsfunktion und
verlangt je Stelle eine Mutationsprobe **mit Gegenprobe auf dem heutigen Stand**
(dieselbe Verfälschung vor der Reparatur muss grün bleiben — das belegt den
Defekt, statt ihn zu behaupten).

## Was der Entwurf sonst entscheidet (E1–E14, Kurzform)

- **E1** `.shellcheckrc` an der Wurzel (`external-sources=true`,
  `source-path=SCRIPTDIR`) statt 29 Dateiannotationen — wirksamste Einzelzeile.
- **E2** Gate bei `-S warning`, `--report`-Schalter zeigt trotzdem alles.
- **E5/E6/E8** SC2329 sind Trap-Handler, SC2094 ist bewusste Selbstinspektion,
  SC2153 sind Prozessgrenzen und indirekte Zuweisung — **Falschmeldungen**, keine
  Änderung, kein `disable`.
- **E9** SC2034 wird je Variable geprüft: `geometry.sh` bekommt einen
  Datei-`disable` mit dreiteiliger Begründung, `APP_ID` wird gelöscht.
- **E10** Das Gate erzwingt die Begründungspflicht selbst (Vorbild: GP-20 in
  `check-ai-hygiene.sh`).
- **E12** Gate hängt in `check-merge-readiness.sh`, nicht direkt in
  `ci-quality.sh` — dadurch läuft es auch im `.githooks/pre-push`.
- **E13** `skip_gate_if_tool_missing shellcheck` aus `scripts/lib/rulebook.sh`,
  dafür `shellcheck` in die pacman-Liste von `.github/workflows/ci.yml` und ein
  `require_pattern` in `qa-linters.sh`, damit das Gate kein Dauerskip wird.
- **E14** Erst säubern, dann scharfschalten, in getrennten Commits.

Drei Stränge (A: ptr-/cua-Harnesse · B: Build und Gates · C: `scripts/tests/`),
Welle 0 (`.shellcheckrc`) vorab, Gate als Nachmerge-Welle. Acht
Nachmerge-Kreuzprüfungen stehen im Entwurf.

## Offen: der Grill

Das ist der Checkpoint und der nächste Schritt. Vier Stellen, an denen ich
zuerst bohren würde:

1. **Ist `warning` die richtige Schwelle, oder macht sie das Gate zahnlos?**
   Alle sechs echten Defekte, die der Entwurf gefunden hat (SC2251), liegen auf
   `note` — das Gate hätte sie also **nicht** gefunden. Ein Gate, das die
   gefundenen Defekte selbst nicht fängt, verdient die Frage, wofür es da ist.
   Gegenposition des Entwurfs: es fängt *künftige* Warnungen, und die Notes sind
   einmalig gelesen und entschieden. Das ist eine ehrliche Antwort, aber sie
   sollte ausgesprochen im Plan stehen.
2. **Der Datei-`disable` in `geometry.sh`** deckt 28 Konstanten ab und kann eine
   künftige echte Leiche verbergen. Der Entwurf nimmt das bewusst in Kauf. Trägt
   die Begründung?
3. **Der Strangschnitt.** Strang B ändert `reprise-worktree-gc.sh`, Strang C
   ändert die Tests, die es ausführen — die Globs sind disjunkt, die *Wirkung*
   nicht. Der Entwurf schiebt das korrekt auf die Nachmerge-Liste (Punkt 3 und
   4). Reicht das, oder gehören B und C zusammen?
4. **Drei Stränge für 54 Warnungen** — ist das Verhältnis richtig, oder ist die
   Aufteilung teurer als die Arbeit?

## Danach

`/code docs/plans/shellcheck.md` (die finale Datei, nicht der Entwurf) — das
Pipeline-Skript fächert von dort in die Stränge auf. Vorher den Entwurf nach dem
Grill mit Opus/max zu `docs/plans/shellcheck.md` umschreiben, Statusblock
übernehmen, `.draft.md` löschen. Bei mehreren überlebenden Strängen zusätzlich
je eine `docs/plans/shellcheck-<n>.md` und `strands`/`merge_order` auf der
Mutterdatei setzen.

## Fallen aus dem Vorlauf, die hier wieder zuschlagen

- **`mapfile` gibt es in zsh nicht.** Die Standard-Shell dieser Umgebung ist zsh;
  jeder Sammelbefehl über `git ls-files` muss in `bash -c '…'` laufen, sonst
  meldet er lautlos „0 Skripte".
- **`.git` ist im Worktree eine Datei, kein Verzeichnis.** `[ -d .git/rebase-merge ]`
  ist dort immer falsch — Rebase-Zustand über
  `git rev-parse --git-path rebase-merge` erfragen. Hat mich im Vorlauf einen
  Durchlauf gekostet, der lautlos nichts tat und „fertig" meldete.
- **Der Lastregler blockt `grep -r` über das Repo.** `git grep` benutzen oder
  über `heavy-run medium -- …` fahren.
- **`land.sh` verweigert bei *ungetrackten* Dateien**, nicht nur bei
  schmutzigen getrackten. Vor dem Landen `.pipeline-findings.md` und Konsorten
  beiseitelegen.
- **`.pipeline-codex.md` ist getrackt** und kollidiert in jedem Rebase, der es
  anfasst — mit `--theirs` auflösen.
- **Der Planungsagent (`Plan`) kann keine Dateien schreiben.** Er läuft im
  Nur-Lese-Modus; sein Ergebnis muss der Hauptthread ablegen. Hat diesmal den
  vollständigen Plan nur im Kontext zurückgegeben.
