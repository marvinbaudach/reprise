---
slug: changelog
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: Das Projekt braucht ein Changelog

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026: *„füge ein
Changelog hinzu"*, ausdrücklich als Vormerkung für später.

## Ist-Zustand (geprüft gegen `origin/dev` `216890a548`)

- **Kein `CHANGELOG.md`, kein `NEWS`** — weder im Wurzelverzeichnis noch
  sonstwo im Baum.
- Die einzige gepflegte Änderungshistorie ist die AppStream-Datei
  `data/io.github.marvinbaudach.Reprise.metainfo.xml:113-126`. Sie kennt genau
  **zwei** `<release>`-Einträge: 0.1.0 (12.07.2026) und 0.1.1 (25.07.2026),
  jeweils ein Satz auf Englisch und Deutsch.
- Die App steht auf `origin/dev` inzwischen bei **0.1.13**. Die
  AppStream-Historie hängt also zwölf Versionen zurück.
- `RELEASING.md` beschreibt die Freigabe-Prüfungen ausführlich, erwähnt aber
  weder ein Changelog noch das Nachtragen von Release-Notes.

## Die Ausgangsfrage: Was zählt als Eintrag?

Der Knackpunkt ist die Versionsmechanik. **Jeder Merge nach `dev` hebt die
Patch-Version** (`scripts/bump-version.sh`, aufgerufen aus `land.sh`) — bei
Version 0.1.13 nach rund zwei Wochen sind das mehrere Einträge pro Tag. Genau
deshalb bleibt `metainfo.xml` bewusst außen vor: ein `<release>` pro Merge wäre
eine Release-Historie über nichts.

Ein Changelog erbt dieses Problem. Vor der Umsetzung ist zu entscheiden:

1. **Was ist die Einheit?** Die gebumpte Patch-Version (dicht, automatisch
   ableitbar aus den PR-Titeln) oder die veröffentlichte Version (dünn, aber
   erst sinnvoll, sobald es Releases über 0.1.1 hinaus gibt).
2. **Wer schreibt?** Von Hand pro PR gepflegt, aus den Commit-/PR-Titeln
   generiert, oder beim Landen automatisch angehängt (dann gehört die Mechanik
   zu `land.sh` — und die liegt außerhalb des Repos, siehe
   `version-bumps-on-every-merge-into-dev`).
3. **Wo ist die Wahrheit?** Wenn `CHANGELOG.md` und `metainfo.xml` beide
   Release-Notes tragen, driften sie. Entweder speist eines das andere, oder
   die Trennung wird explizit (Changelog = Entwicklungshistorie, AppStream =
   Nutzer-Release-Notes) und `RELEASING.md` hält sie fest.
4. **Zweisprachig?** AppStream trägt `xml:lang="de"`. Ein `CHANGELOG.md` in
   zwei Sprachen ist doppelte Pflege — die Repo-Sprache ist Englisch
   (`README.de.md` ist die dokumentierte Ausnahme).

## Empfehlung fürs spätere Planen

Der kleinste ehrliche Schnitt: ein englisches `CHANGELOG.md` im Format *Keep a
Changelog*, Einträge pro **veröffentlichter** Version, plus ein
`## Unreleased`-Abschnitt, in den nennenswerte PRs einsortiert werden.
Die Patch-Bumps bleiben draußen — die Historie steht ohnehin in `git log`.
Dazu ein Satz in `RELEASING.md`, der das Nachziehen von `CHANGELOG.md` und
`metainfo.xml` vor einer Freigabe verlangt, und beim Aufräumen die zwölf
fehlenden AppStream-Einträge klären (nachtragen oder bewusst überspringen).

Offen bleibt, ob dieses Aufräumen zu
[`repo-tidy-before-going-public`](repo-tidy-before-going-public.md) gehört —
dort steht der Rest der Vorbereitung auf ein öffentliches Repository.
