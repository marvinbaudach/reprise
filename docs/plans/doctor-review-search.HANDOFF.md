# Handover — `doctor-review-search`

Stand: 2026-08-15, Abend. Der Plan ist `docs/plans/doctor-review-search.md`,
`phase: shipped`.

## Was gelandet ist

| PR | Inhalt | Merge |
| --- | --- | --- |
| #520 | CSS-Extraktion aus `library_doctor/mod.rs` in `review_css.rs` (Task 0) | `2a4083e5b6` |
| #524 | Die Doctor-Review-Suche, Tasks 1–13 | `95b4b30016`, Version 0.1.13 |

`main` wurde per Fast-Forward auf `95b4b30016` promotet (16 Commits, `dev` und
`main` sind identisch, `dev` wurde nicht gelöscht).

Die installierte Nightly (`~/.local/bin/reprise`) ist auf diesem Stand und
verifiziert: `last-built.sha` = `95b4b30016`, dreimal `Compiling reprise-core`
(einmal je `custom_target`), alle drei Binaries byteidentisch mit dem Build, und
`"Clear Filters"` — ein String, den es vor diesem Merge nirgends in `dev` gab —
steckt nachweislich in der Binary.

**Task 0 lief nicht wie geplant.** Der Plan verlangte in §3, die CSS-Extraktion
zuerst allein nach `dev` zu mergen und den Feature-Branch vom Merge-Commit
abzuzweigen. Tatsächlich war sie der erste Commit des Feature-Branches. Das wurde
beim Landen nachgeholt: Prereq-Branch als #520 zuerst gelandet, Feature-Branch
darauf rebast. Wer die Historie liest, findet die Extraktion deshalb korrekt als
eigenen Commit in `dev`, nicht im Feature-PR.

## Was noch offen ist: der Abnahmelauf aus Plan §7

Das ist das Einzige, was aussteht. Vier Punkte, alle brauchen die **echte**
Bibliothek und jemanden an der GUI — die Instrumentierung existiert im
Testprozess gar nicht.

- **A-1** — Review öffnen, ein fünfstelliges Präfix Zeichen für Zeichen tippen,
  mit Escape löschen, fünfmal wiederholen. Median der `path="search"`-Zeilen
  lesen. **Dieser Punkt entscheidet die Debounce-Frage** gegen das Entwurfsziel
  von 16,7 ms aus §2: darüber kommt der Debounce nach (durch Wiederverwendung
  von `SEARCH_DEBOUNCE_MS` aus `view_session.rs:25`, **nicht** mit einer neuen
  Konstante), darunter wird die Zahl in §7 eingetragen und die Zusicherung in
  `review_search_wall_clock_probe` ergänzt
  (`review_page_perf_tests.rs:214`).
- **A-2** — Album-Kopf-Checkbox je fünfmal mit und ohne aktive Query umschalten.
  `path="selection"`-Medianen müssen dem Fix-Arm von Strang B entsprechen
  (≈ 13,6 ms), `touched`-Zahlen unverändert.
- **A-3** — Mit aktiver Query so scrollen, dass ein Albumkopf mittig steht,
  Screenshot, eine Zeile umschalten, Screenshot. Keine Verschiebung.
- **A-4** — Query ohne Treffer: der `"no-match"`-Zweig muss erscheinen (Lupe,
  Query im Titel, versteckte Anzahl in der Beschreibung), **nicht** die grüne
  Seite „No Changes to Review". Clear-Knopf klicken, vollständige Liste und
  leeres Eingabefeld müssen in einem Schritt zurückkommen.

**16,7 ms sind ein Entwurfsziel, keine Messung.** Nichts in diesem Baum hat
diesen Pfad je gemessen.

## Drei Fallen, die ich beim Vorbereiten gefunden habe

Der Plan sagt in §7, der Harness „exists and needs no rebuilding". Das stimmt
nicht mehr. Wer die Abnahme startet, läuft sonst in drei Wände:

1. **Der Harness zeigt auf einen gelöschten Worktree.**
   `~/.cache/reprise-doctor-b0-harness/doctor-b0-run.sh:30` setzt
   `WT=/home/marvin/Projects/reprise-doctor-review-selection-and-refresh-b`.
   Dieser Worktree existiert nicht mehr — `land.sh` entfernt ihn beim Landen von
   Strang B. Das `cargo build` in Zeile 72 scheitert also. **Fix:** `WT` auf
   einen aktuellen Checkout von `origin/dev` zeigen lassen, oder den
   Build-Schritt überspringen und `SRC_BIN` direkt auf die verifizierte Nightly
   (`~/.local/bin/reprise`) setzen — die trägt bereits `95b4b30016`.

2. **Der Log-Filter blendet genau unsere Messung aus.**
   Zeile 63 setzt `info,reprise::ui::library_doctor::review_page=debug`. Unsere
   Suchmessung steht aber in `review_search.rs` und hat kein explizites
   `target:`, ihr Ziel ist also `reprise::ui::library_doctor::review_search`.
   Unter dem jetzigen Filter erscheint **keine einzige** `path="search"`-Zeile —
   und das sieht aus wie „nicht gemessen", nicht wie „falsch gefiltert".
   **Fix:** `REPRISE_LOG_OVERRIDE` setzen und `…::library_doctor=debug` nehmen,
   damit beide Module fallen.

3. **`ACCEPTANCE-strand-b.md` beschreibt eine andere Abnahme.**
   Das Dokument im selben Verzeichnis gehört zu Strang B (Selektionspfad, zwei
   Arme, Kontroll- gegen Fix-Arm) und **nicht** zu A-1…A-4. Unser Plan leiht sich
   nur die Skripte (`doctor-b0-run.sh` zum Starten, `doctor-b0-medians.sh` zum
   Auswerten), nicht die Prozedur. Wer das Dokument von oben abarbeitet, misst
   das Falsche.

Dazu die zwei Fallen, die der Plan selbst schon nennt und die weiter gelten:
Logfelder sind **auch in Dateien** ANSI-eingefärbt (`grep -F 'stage="search"'`
findet nichts — erst `sed 's/\x1b\[[0-9;]*m//g'`, dann auf den Meldungstext
`DOCTOR_REVIEW_REFRESH path` matchen), und `tracing::debug!` existiert in
Testläufen überhaupt nicht.

## Vorbedingungen für den Lauf

- **Eigene Reprise beenden.** Eine Kopie, die entsteht während die App die WAL
  hält, ist eine andere Datenbank. Prüfen mit `ps -C reprise` — `pgrep -f reprise`
  matcht sich selbst und lügt.
- Die Probe-Binary wird bewusst nach `~/.cache/doctor-b0/bin/dcheck` kopiert und
  mit `setsid` gestartet, damit ihr Kommandotext kein „reprise" enthält: fremde
  Aufräumläufe machen `pkill -f reprise`. Das nicht „reparieren".
- Lange Logs **nicht** ins Session-Scratchpad legen — `agent-tmp-gc` hat dort
  schon Logs mitten im Lauf gelöscht, eines davon offen gehalten.
- Wake-Lock nehmen (`wake-lock acquire …`), der Lauf ist unbeaufsichtigt lang.

## Kleinigkeiten, die liegen geblieben sind

Aus dem Review angenommen und umgesetzt wurden M1, M2 und L1. Nicht angenommen,
weiterhin offen:

- **Der Clear-Knopf ist doppelt verkabelt** (`review_search.rs:7-22`): er trägt
  `action_name("win.clear-all-filters")` **und** einen `connect_clicked`-Handler,
  der nur die Suche räumt. Beide feuern, das Ergebnis stimmt, aber `on_clear` ist
  toter Ballast und die Signatur von `no_match_page` behauptet eine Verkabelung,
  die nicht mehr die maßgebliche ist. Eine Zeile.
- **`.pipeline-codex.md` ist getrackt**, steht aber in `.gitignore` (Zeile 39) —
  deshalb greift die Ignore-Regel nicht und jeder Pipeline-PR konfligiert daran
  beim Rebase. Für diesen Branch habe ich die Datei netto-änderungsfrei gemacht;
  die Ursache steht weiter. Der saubere Fix wäre ein eigener PR mit
  `git rm --cached .pipeline-codex.md`.

## Nachlauf, den noch niemand angesehen hat

Der Quality-Gate-Lauf auf `95b4b30016`
([Run 31908053993](https://github.com/marvinbaudach/reprise/actions/runs/31908053993))
lief zum Zeitpunkt der Promotion noch. Die Android-JVM-Suite auf demselben
Commit war grün, ebenso **alle** Jobs des vorherigen vollständigen `dev`-Laufs
(`d0a2a35b18`). Sollte das Gate rot enden, betrifft das jetzt `main`: der Weg
wäre ein `hotfix/*` **von `dev` aus**, kein Direktcommit auf `main` — ein
`hotfix`, der direkt nach `main` gemergt wird, zerstört die
Fast-Forward-Eigenschaft unwiderruflich.
