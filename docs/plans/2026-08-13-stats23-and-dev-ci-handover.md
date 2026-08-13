# Handover — STATS-23 gelandet, dev-CI repariert

Stand: 2026-08-13, 21:35 Uhr
Zwei Stränge, beide gelandet. Offen ist nur die Bestätigung des letzten CI-Laufs.

> Nicht zu verwechseln mit `docs/plans/2026-08-13-session-handover.md` — der
> gehört einer anderen, parallel laufenden Sitzung. Nicht überschreiben.

## Strang A — My Stats: Künstlerbilder und Interpretenrangliste

**Gelandet als `d329ab208c`** (PR #457, Squash nach `dev`). Vollständiger
Fachhandover mit allen Details:
`docs/superpowers/plans/2026-08-13-stats-artist-images-and-ranking.HANDOFF.md`

Kurzfassung: Der Refactor-Lauf setzte alle sieben Review-Befunde um, ein
unabhängiger Reviewer bestätigte sie gegen den Diff (nicht gegen die
Commit-Texte). Die visuelle Abnahme gegen die echte Bibliothek ist bestanden —
alle drei Stufen der Bildkette (Porträt → Album-Cover → Initialen) sind auf dem
Bildschirm belegt. Plan steht auf `phase: shipped`, Worktree und Branch sind
weg, Wake-Lock `stats-artist-images` freigegeben.

Belege: `.tmp/stats23-visual/` (Screenshots + `run-accept.sh`).

## Strang B — dev-CI war rot

**Gelandet als `8043050e57`** (PR #458).

Der Workflow **CI** war auf `dev` seit `ca85fedf` (#452, 13:41 Uhr) rot, mit
genau einem Verstoß: `frontend thinness: rusqlite grew from 110 to 111`. Die
111. Fundstelle war ein Wrapper in `artwork_consent_banner.rs`, der nichts tat,
als `settings::get_bool` durchzureichen — und dabei `rusqlite::Error` in eine
Frontend-Signatur holte. Codex hat ihn ersatzlos gestrichen; die eine
Aufrufstelle matcht jetzt direkt. Verhalten und Warn-Zeile unverändert.

Unabhängig nachgemessen, nicht aus dem Codex-Bericht übernommen:
`check-frontend-thinness.sh` gibt `rusqlite: 110 (at budget)`, exit 0, und der
Diff gegen `scripts/check-frontend-thinness.sh` ist leer — die Baseline wurde
**nicht** angehoben. Das war die Falle: Das Gate sagt ausdrücklich „must get
thinner, not thicker".

**Offen:** Der dev-Lauf `31736000672` zu `8043050e` lief um 21:35 noch (seit
~20:40, also ungewöhnlich lang). Ein Monitor hängt dran. Falls er rot endet:
gegen den Vorlauf vergleichen, bevor man ihn diesem Zweig zuschreibt.

```bash
gh run view 31736000672 --json status,conclusion
gh run view 31736000672 --log-failed | sed 's/.*Z //' | grep -vE 'at budget|passed|^=='
```

## Zwei Folgeaufgaben, beide noch nicht angelegt

### 1. Deezer liefert Platzhalter statt Bandbildern

In *My Stats* tragen Rang 3 (The Devil Wears Prada) und Rang 10 (Oceano) ein
graues Personen-Icon. Zwei verschiedene Ursachen, beide gemessen:

- **Der Code nimmt den falschen Treffer.** Deezer liefert mehrere exakte
  Namenstreffer; der erste trägt oft die Bildkennung
  `d41d8cd98f00b204e9800998ecf8427e` — den MD5 des *leeren Strings* — und das
  echte Bild steckt im zweiten. `is_placeholder_url`
  (`crates/reprise-core/src/artist_portrait/deezer.rs:54`) prüft nur auf
  `/artist//`, einen *fehlenden* Pfadabschnitt, und läuft daran vorbei. Selbst
  wenn es griffe, macht `parse_best_artist` beim ersten Treffer
  `return Some(...)` (`:49`). Bei „ONI" reproduziert.
- **Der Zwischenspeicher ist vergiftet.** Acht Einträge in
  `~/.cache/reprise/artist-portraits` (alle vom 18.07.2026) halten einen
  Platzhalter fest. Für Oceano und Wake Me liefert Deezer heute längst ein
  echtes Bild — ein Code-Fix allein ändert für die nichts.

Erkennung **nicht** über Datei-Hashes bauen: Deezer hat den Platzhalter
zwischenzeitlich neu gezeichnet, im Cache liegen zwei byte-verschiedene
Varianten. Stabil ist allein die Kennung in der URL.

### 2. `reprise-core` leakt `rusqlite::Error`

`settings_api::get_bool` (`crates/reprise-core/src/library/settings_api.rs:39`)
gibt `Result<bool, rusqlite::Error>` zurück. Genau deshalb braucht das Frontend
den Typ überhaupt — und genau deshalb wird der nächste Durchreicher das
Thinness-Gate wieder reißen. Ein kapselnder Fehlertyp in `reprise-core` würde
diese Verstoßklasse dauerhaft beenden, betrifft aber viele Aufrufer. Bewusst
nicht in den CI-Notfallfix gepackt.

## Was diese Sitzung über die Werkzeuge gelernt hat

- **`scripts/land.sh` gibt es nicht.** Das Skript liegt unter
  `~/.claude/skills/pipeline/scripts/land.sh` und will
  `land.sh <pr-nummer> [worktree]` — es braucht einen fertigen PR und verlangt
  einen sauberen Worktree, also vorher `.pipeline-*.md` wegräumen
  (`.pipeline-codex.md` ist **getrackt**, nur `git restore`).
- **GitHubs Konflikt-Verdict ist ein Cache.** Der erste Merge-Versuch bei #457
  wurde als „not mergeable" abgelehnt, obwohl git den Merge sauber fand; der
  zweite Versuch ging durch. `land.sh` fängt das selbst ab.
- **Hintergrund-Bash-Läufe über ~10 Minuten werden abgeräumt.** Zwei Watcher auf
  den CI-Lauf wurden gekillt. Das `Monitor`-Tool überlebt — für lange Wartezeiten
  das richtige Werkzeug.
- **Der `repair-dev`-Wake-Lock von 09:34 ist verwaist.** Wer prüft, ob schon
  jemand an den dev-Gates arbeitet, darf sich nicht auf den Lock verlassen —
  die laufenden Codex-Prozesse per `/proc/<pid>/cwd` gegen den Worktree prüfen.

Neu abgelegte Notizen: `reprise-deezer-portrait-placeholders`,
`reprise-artwork-module-supersedes-portraits-switch`,
`proving-an-image-fallback-chain`.

## Aufräumen, falls die Sitzung endet

```bash
wake-lock release dev-ci-thinness     # noch gehalten
```

Die Testinstanzen und Xvfb-Displays der visuellen Abnahme sind bereits beendet;
die echte App des Nutzers lief durchgehend unangetastet weiter.
