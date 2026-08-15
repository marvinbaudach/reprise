---
slug: worktree-gc-root-probe
worktree: /home/marvin/Projects/reprise-worktree-gc-root-probe
branch: feature/worktree-gc-root-probe
phase: reviewed
codex_session:
created: 2026-08-15
---
# Der Löschfehler-Fall muss gemessen werden, nicht angenommen

> Ausgelöst vom roten `dev`-Lauf
> [31844403016](https://github.com/marvinbaudach/reprise/actions/runs/31844403016)
> nach PR #497 (`59267425a9`). Alle Belege unten selbst erhoben, nicht aus einem
> Bericht übernommen.

## Problem

`scripts/tests/worktree-gc.sh` konstruiert einen Fall, in dem die
Artefakt-Löschung **fehlschlagen muss**: ein Unterverzeichnis
`…/delete-failure/target/restricted`, das per `chmod 555` (Zeile 508)
schreibgeschützt wird. Daran hängen vier Zusicherungen — der Runner soll das
`target` behalten, `keep artifact_delete_failed` melden, `removable` trotzdem
räumen und danach **weiterarbeiten** statt abzubrechen.

Die Annahme dahinter gilt nur für unprivilegierte Nutzer. Der Quality-gate-Job
läuft laut `.github/workflows/ci.yml:55-56` in `container: archlinux:latest`,
und GitHub-Actions-Container laufen ohne `options: --user` als **root**. Root
umgeht die Rechteprüfung (CAP_DAC_OVERRIDE), `find -xdev -depth -delete` im
Runner (`scripts/reprise-worktree-gc.sh:241`) räumt das `target` also vollständig
weg. Der Test stirbt dann an der nächstbesten Stelle:

```
du: cannot access '/tmp/reprise-worktree-gc.y3UiMb/artifact-scope/.worktrees/delete-failure/target': No such file or directory
```

Das ist `worktree-gc.sh:535` (`failing_target_kib_after=$(du -sk …)`) unter
`set -euo pipefail`. Die drei Zusicherungen dahinter (`:588`, `:604`, `:605`)
hätten danach ebenfalls gerissen.

**Warum das erst jetzt auffällt:** Der Test lief bis PR #497 in *keinem* Gate.
Der Zweig hat ihn in `check-merge-readiness.sh` eingehängt — damit ist eine seit
jeher unhaltbare Annahme zum ersten Mal in einer root-Umgebung gelaufen. Es ist
keine Regression des Tests, sondern eine nie geprüfte Voraussetzung.

### Belege

| Zusicherung | Ergebnis |
|---|---|
| CI-Fehlerzeile | `du: cannot access …/delete-failure/target`, exit 1 |
| Reihenfolge im CI-Log | `check-shell.sh` grün (21:54:48) → `worktree-gc.sh` rot (21:56:18) |
| CI-Umgebung | `.github/workflows/ci.yml:55` → `container: archlinux:latest`, kein `--user` |
| rechteabhängige Stellen im Test | genau **eine**: `:508`; dazu das Aufräumen in `:41` |
| Kontrolle als normaler Nutzer, lokal | `rm -rf` scheitert mit `Permission denied`, `restricted` überlebt — der Fall ist hier herstellbar |
| Löschpfad des Runners | `find "$artifact" -xdev -depth -delete` → sonst `keep artifact_delete_failed` |

## Was **nicht** geht

- **Sperre als root herstellen.** `chattr +i` braucht CAP_LINUX_IMMUTABLE und
  wird von overlayfs/tmpfs nicht getragen; ein Mountpoint als Sperre braucht
  CAP_SYS_ADMIN. Der GHA-Container hat beides nicht zugesichert. Eine Reparatur
  auf diesem Weg wäre eine Wette auf die Container-Innereien.
- **Den Test als unprivilegierter Nutzer fahren.** Derselbe Job installiert per
  `pacman` Pakete und braucht dafür root; ein abgesenkter Teillauf zieht
  Repo-Ownership (`dubious ownership`), `HOME` und git-Konfiguration nach sich.
  Viel Fläche für einen Fall.
- **Den Test wieder aus dem Gate hängen.** Das wäre die Rücknahme dessen, was
  #497 gerade richtig gemacht hat.

## Lösung

Die Fähigkeit **messen statt annehmen**, und in beiden Fällen etwas Echtes
prüfen — kein stiller Skip, keine Ausnahmenliste.

1. **Sondierung mit genau dem Kommando des Runners**, bevor das Fixture bewertet
   wird: ein Wegwerf-Verzeichnis nach demselben Muster anlegen (Unterverzeichnis
   mit Datei, `chmod 555` darauf) und `find … -xdev -depth -delete` darauf
   loslassen. Gelingt es, gibt diese Umgebung keine Löschsperre her.

   **Zwingend außerhalb von `$failing_artifact_worktree/target`.** Der Runner
   entscheidet Frische über `find … -newermt "$max_age_days days ago" -print -quit`
   (`reprise-worktree-gc.sh:230`): *eine einzige* neue mtime unterhalb `target`
   macht das ganze Artefakt frisch und verschiebt den Testfall nach `keep fresh`.
   Eine Sondierung im Fixture würde also einen zweiten, subtileren Fehlschlag
   erzeugen. Das Wegwerf-Verzeichnis gehört in `$fixture` und muss danach restlos
   verschwinden (im Nicht-Root-Fall braucht das Aufräumen `chmod u+w`).

2. **Sperre herstellbar** (lokal, unprivilegiert): Fixture, Ablauf und alle vier
   Zusicherungen bleiben **wortgleich wie heute**. Dieser Zweig darf die
   bestehende Abdeckung nicht anfassen.

3. **Sperre nicht herstellbar** (CI als root): denselben Sweep fahren, aber den
   Erfolgsfall zusichern statt des Fehlerfalls — `target` verschwindet
   vollständig, der Report meldet es als geräumt, und `du` wird gar nicht erst
   auf einen verschwundenen Pfad losgelassen (der Beitrag zur
   `reclaimed_kib`-Rechnung ist dann der volle Vorher-Wert). Die Zusicherungen,
   die **nicht** am Fehlerfall hängen — dass nach diesem Worktree weitergeräumt
   wird (`cleaned stale_android_build …`, `cleaned stale_target
   $after_failure_worktree/target`) und die Gesamtsumme `reclaimed_kib` — müssen
   in **beiden** Fällen laufen.

4. **Der übersprungene Fall muss sich melden.** Eine Zeile auf stdout, die sagt,
   welche Zusicherung in dieser Umgebung nicht lief und warum (Muster:
   `SKIPPED: …; this gate did not run` aus `check-shell.sh`). Ein Fall, der
   stillschweigend zum Erfolgsfall umschaltet, ist genau die Sorte Loch, die
   #497 zugemacht hat.

## Zusicherungen für die Abnahme

Codex misst selbst und legt die Ausgaben vor — keine Behauptungen:

1. `bash scripts/tests/worktree-gc.sh` lokal (unprivilegiert): Exit 0, und die
   Ausgabe belegt, dass der **Fehlerfall gelaufen ist**, nicht der Ersatzpfad.
2. Rot-Probe A — im Fehlerfall-Zweig eine Zusicherung verfälschen (etwa
   `keep artifact_delete_failed` → ein Muster, das nicht im Report steht): der
   Test muss rot werden. Danach zurückdrehen (`git checkout --`, **nicht** `cp`,
   das ist interaktiv aliasiert).
3. Rot-Probe B — die Sondierung künstlich auf „keine Sperre herstellbar" zwingen
   (Rückgabewert umdrehen) und den Test fahren: er muss den Ersatzpfad nehmen,
   die Skip-Zeile ausgeben und **Exit 0** liefern. Das ist die Probe, die den
   CI-Fall ohne CI misst. Danach zurückdrehen.
4. Rot-Probe C — im Ersatzpfad eine Zusicherung verfälschen: rot. Sonst ist der
   Ersatzpfad nur Dekoration.
5. `./scripts/check-shell.sh`: Exit 0 — der neue Code steht selbst unter dem
   Gate aus #497, inklusive Begründungspflicht für jedes `shellcheck disable`.
6. `bash scripts/tests/worktree-gc-schedule.sh`: Exit 0 (unberührt, aber in
   derselben Gate-Kette).

## Nicht Gegenstand

- Die tiefere Scope-Schwäche der Worktree-GC (Geschwister-Worktrees als
  `outside_scope`, `dirty` blockt die target-Löschung) — offener Punkt seit dem
  11.08., eigenes Vorhaben.
- Dass `qa-linters.sh` mit seinen `require_*`-Wächtern in keinem PR läuft —
  bekannt aus #497, eigenes Vorhaben.
- Jede Änderung an `scripts/reprise-worktree-gc.sh`. Der Runner verhält sich
  korrekt; falsch ist die Annahme im Test. Fällt beim Arbeiten doch ein echter
  Defekt im Runner auf, wird er berichtet, nicht nebenbei behoben.
