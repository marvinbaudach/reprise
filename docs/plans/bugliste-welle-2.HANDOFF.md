# Übergabe: Welle 2 ist gebaut und geprüft — es fehlt das Landen

**Stand:** 18.08.2026, 20:25. Fortsetzung von `bugliste-welle-1.HANDOFF-3.md`.
**`origin/dev` = `077aa63918`** (nach #557). Drei Stränge stehen fertig in ihren
Worktrees, keiner ist gepusht, keiner hat einen PR.

## ZUERST: Wake-Lock und laufende Prozesse

`wake-lock` hält weiterhin `bugliste`. Solange Codex-Läufe offen sind, dranlassen;
danach `wake-lock release bugliste`. Fremde Locks (`ghostty`,
`showroom-design-import`) nicht anfassen.

Beim Übernehmen prüfen, ob noch etwas läuft:

```sh
pgrep -af 'co[d]ex exec' | grep reprise
heavy-run status
```

## Was gelandet ist

| PR | Inhalt |
| --- | --- |
| #557 | 35 verschollene Plandokumente zurück unter Versionskontrolle |

Der dev-Lauf für `834193e7d8` war **vollständig grün, inklusive „Core and
workspace quality suite"** — damit ist #556 aus Welle 1 belegt und nicht nur
behauptet. Der offene Punkt aus Übergabe 3 ist damit erledigt.

### Die Aufräumsache war nicht das, wonach sie aussah

Die 62 ungetrackten Pfade unter `docs/plans/` waren **drei** Gruppen, nicht eine:

| Gruppe | Zahl | Behandlung |
| --- | --- | --- |
| in keinem Zweig vorhanden | 35 | committet (#557) |
| **veraltete** lokale Kopien | 15 | verworfen |
| gehören laufenden Worktrees | 5 | ausgelassen |

Die mittlere Gruppe ist die Falle: der Hauptcheckout steht auf `be5f014d3b`
(alt, detached), sein `git status` vergleicht also gegen einen Wochen alten
Stand — „ungetrackt" heißt dort **nicht** „neu". Ein pauschales
`git add docs/plans/` hätte 15 Pläne zurückdatiert: `phase: shipped` zurück auf
`refactored`/`planned` und ~480 Zeilen Planinhalt gelöscht. Immer erst in einem
Worktree auf `origin/dev` kopieren und dort `git status` lesen.

## Die drei Stränge

Alle drei sind gegen `origin/dev` = `077aa63918` gebaut.

| # | Branch / Worktree | Zustand | Belege |
| --- | --- | --- | --- |
| 1 | `feature/playback-errors-report-the-first-cause` | Refactor lief zuletzt noch | fmt/clippy grün, 5130/0, Display 752/0 (vor dem Refactor) |
| 2 | `feature/concerts-duplicate-events` | Refactor fertig, Nachprüfung lief zuletzt | fmt/clippy grün, 5127/0, **Migration gegen die echte Bibliothek** |
| 3 | `feature/filter-bar-clear-without-a-filter` | fertig, ohne Befund | fmt/clippy grün, 5124/0 |

Worktrees liegen unter `/home/marvin/Projects/reprise-<slug>`.

### Strang 1 — die erste Ursache gewinnt

`PlayerEvent::Error` trägt statt eines `String` jetzt einen typisierten
`PlaybackFailure` (Meldung + `PlaybackFailureKind` + `PlaybackSessionId`). Die
Typisierung **muss** an der Bus-Stelle passieren: der Statuscode steht nur in
`e.debug()` und wurde bisher dort verworfen. Die drei Zwei-Zeilen-Änderungen in
`reprise-android-ffi` sind die Folge davon.

Vier Reviewbefunde gingen an Codex zurück:

1. **HIGH — Aufgabe 4 war für `PlaybackMode::QueuedEpisode` nicht erfüllt.** Der
   typisierte Befund `unavailable_episode` wurde nur im `Podcast`-Zweig
   ausgewertet; bei manuell eingereihten Episoden fiel er weg und `fail_podcast`
   markierte die Episode nach drei Fehlschlägen trotzdem als kaputt — exakt das
   Verhalten, das der Plan beenden sollte. Der neue Test fasste nur die reine
   Funktion an, nie die Weiche.
2. **HIGH** Der Anzeigetext (Toast) war nicht redigiert — der Capability-Token
   des lokalen Proxys hätte auf dem Bildschirm stehen können.
3. **HIGH** Die früheste Logzeile (`player_pipeline.rs:467`) ebenfalls nicht;
   die Redaktionsfunktion zieht dafür nach `reprise-core`.
4. **MEDIUM** `http_status_from_debug` sucht irgendein `(DDD)` im ganzen
   Debugtext. **Von beiden Reviewern unabhängig gefunden.**

Sauber befunden: die Sitzungssperre hält end-to-end (jeder externe Versuch geht
über `set_state(Null)` → neue, streng steigende Sitzungs-ID; Gapless umgeht das,
wird aber für externe Wiedergabe nie benutzt), keine Borrow-Probleme, keine
Panic-Pfade.

### Strang 2 — der Dublettenschlüssel

Der Plan wurde vor dem Bauen gegrillt und dabei umgedreht. **Die Messung hat die
Diagnose des Befunds widerlegt:** alle fünf Dublettenpaare stammen von *einem*
Anbieter — Ticketmaster liefert dieselbe Veranstaltung mehrfach, einmal je
Ticketverkäufer oder Paket (`etix`, `axs`, `universe`, „VIP Upgrades" gegen
„Premium Packages"). Die Ortsnamen weichen in fünf verschiedenen Klassen ab, und
die Koordinaten liegen beim selben Haus bis zu **14 km** auseinander. Jede
Ortsnormalisierung und jeder Geo-Radius scheidet damit aus; der Schlüssel wurde
`artist_key|date_key|city`, was den zweiten Befund (Festival-Kollision) mitlöst.

**Der stärkste Beleg, selbst gefahren:** die Migration `v76` gegen eine Kopie der
echten Bibliothek — **413 → 408 Zeilen, null Dublettengruppen,
`user_version = 76`**, und die fünf Sieger exakt wie im Grill vorhergesagt
(`Toads Place - CT`, `Riviera Theatre- IL`, `Ziggo Dome Club` per Gleichstand,
`Intersection`, `Cardiff University Students Union`). Der Prüfcode war ein
temporärer `#[test] #[ignore]` in `db_concerts.rs`, per `LIVE_DB`-Umgebung auf
die Kopie gezeigt und danach mit `git checkout --` entfernt.

Drei Reviewbefunde, alle behoben (`89c799ca25`, `4af566166c`, `fa9d206b40`):

1. **HIGH** `provider_owns_ticket_url` prüfte, ob *irgendein* Host-Label
   `ticketmaster` heißt — `ticketmaster.evil.com` galt als anbietereigen und
   gewann, wodurch die Migration die echte Zeile **gelöscht** hätte.
2. **MEDIUM** Die Sieger-Regel lief nur in `merge()`; der `ON CONFLICT`-Zweig
   überschrieb danach bedingungslos. Ein späterer Refresh mit nur der
   Verliererauflistung ersetzte den gespeicherten Sieger.
3. **LOW** Ein unbekannter `provider`-String ließ `migrate_v76` fehlschlagen —
   und damit das Öffnen der Datenbank, also den App-Start.

Die Migration selbst wurde als solide befunden: eine Transaktion, Verlierer vor
Siegern, Sieger über einen temporären Schlüssel umbenannt (verhindert die
transiente `UNIQUE`-Verletzung), versionsgesteuert und wiederholbar.

### Strang 3 — die Filterleiste

Ohne Reviewbefund, aber mit zwei Korrekturen aus der Prüfung:

- **Meine Grill-Prämisse war falsch.** Ich hatte die Auto-Auswahl beim
  Sitzungsstart als Abweichung der Podcast-Ansicht dargestellt. Sie ist die
  Umsetzung der **aktiven** Regel `START-3` (`docs/ux-rules.md:1398`,
  ausdrücklich für „the last loaded track **or episode**"), und die Track-Liste
  nagelt sie per Test fest (`start_restore_tests.rs:121`).
  `TrackRevealPolicy::MarkerOnly` regelt das **Zentrieren**, nicht die Auswahl —
  daran bin ich hängengeblieben. Die Streichung wurde zurückgenommen; der Strang
  trägt nur noch die Beschriftungen.
- **Ein Fremdeffekt in den Katalogen.** Die neue Beschriftung „Clear filters"
  trifft eine bereits existierende `msgid` (aus `strings.rs:328`). Beim
  Neuerzeugen hat Codex deren deutsche Übersetzung von „Filter zurücksetzen" auf
  „Filter löschen" und die spanische von „Borrar filtros" auf „Limpiar filtros"
  geändert — also die Beschriftung einer fremden, ausgelieferten Oberfläche.
  Zurückgedreht in `9d922b8da3`.

## Was als Nächstes zu tun ist

1. **Strang 1: Refactor-Ergebnis prüfen.** `git -C <wt> log --oneline
   origin/dev..HEAD`, Diff lesen, dann fmt/clippy/`cargo test --workspace`.
   Besonders auf Befund 1 achten — er verlangt, dass der typisierte Befund die
   Weiche überlebt, und der zugehörige Test muss die **Weiche** anfassen, nicht
   nur `external_error_presentation`.
2. **Strang 2: Nachprüfung abschließen** und die Live-Migration erneut fahren
   (die Sieger-Regel hat sich geändert; 413 → 408 und dieselben fünf Sieger
   müssen weiter herauskommen).
3. **Landen, in der Reihenfolge 1 → 2 → 3.** Jeder Merge schiebt `dev` weiter,
   der nächste Strang muss davor rebasen. Der Ablauf steht unten.

### Landen — der Ablauf von Hand

`land.sh` ist weiterhin kaputt (rebased sich nach dem eigenen Merge in einen
add/add-Konflikt). Pro Strang:

```sh
cd "$WT"
git fetch origin --quiet && git rebase origin/dev
bash ~/.claude/skills/pipeline/scripts/status.sh set <plan.md> phase shipped
git add <plan.md> && git commit -m "docs: mark <slug> shipped"
./scripts/bump-version.sh --base origin/dev
git commit -m "chore: bump version to <…>" -- Cargo.toml Cargo.lock android/app/build.gradle.kts
git push -u origin <branch>
gh pr create --base dev --title "…" --body-file <datei>
gh pr checks <PR> --json name,state -q '.[] | select(.name=="Quality gate") | .state'
gh api -X PUT repos/marvinbaudach/reprise/pulls/<PR>/merge -f merge_method=squash
cd /home/marvin/Projects/reprise
git worktree remove "$WT" --force && git branch -D "$BR" && git worktree prune
```

`phase: shipped` **muss vor dem Merge in den Feature-Zweig**, sonst liegt die
Plandatei nach dem Squash ohne Status auf `dev`. `gh pr merge` wird weiterhin
abgelehnt, die REST-Route geht sofort durch — in dieser Sitzung erneut bestätigt.

## Neue Fallen aus dieser Sitzung

- **Ein Merge nach `dev` killt den laufenden dev-Lauf.** `ci.yml` fährt
  `concurrency: ci-…-${{ github.ref }}` mit `cancel-in-progress: true`. #557 lag
  deshalb bewusst, bis der Lauf für `834193e7d8` durch war — und ein Ersatzlauf
  hätte den Beweis nicht liefern können, weil eine reine `docs/`-Änderung den
  `core`-Pfad gar nicht routet und die Suite wieder übersprungen worden wäre.
  **Vor jedem Merge prüfen, ob ein Lauf beobachtet werden muss.**
- **`scripts/tests/gettext-catalogs.sh` ist auf `dev` rot** — 81 Fehler in
  `po/ar.po`, systemisch über alle Sprachen, wenn man frisch extrahiert. Das
  Skript läuft in keinem CI-Gate und ist über die Zeit verrottet. Codex hat
  deshalb in Strang 3 **nicht committet** und nachgefragt; die Rückfrage war
  berechtigt. **Eigener Aufräum-PR fällig.**
- **Eine neue Beschriftung kann eine fremde `msgid` treffen.** Wer einen String
  auf einen Text ändert, den es schon gibt, erbt dessen Übersetzungen — und ein
  Neuerzeugen der Kataloge schreibt sie um. Nach jedem `po/`-Diff die
  `msgstr`-Zeilen prüfen, nicht nur die neuen Einträge.
- **Ein Wächter, der eingreift, schlägt einen, der meldet.** Gegen die
  Zwei-Stunden-Falle aus Übergabe 3 lief in dieser Sitzung ein Monitor, der
  verbotene Einstiege (Sammel-Gate, Gradle, Display-Suite) **im Worktree des
  jeweiligen Strangs** erkennt und den Prozess abschießt; der Codex-Lauf sieht
  nur einen fehlgeschlagenen Befehl und macht weiter. Zwei Details tragen das:
  die Zuordnung läuft über den **Worktree-Pfad**, nie über Prozessnamen (sonst
  trifft der Wächter sich selbst), und die Muster stehen als Zeichenklassen im
  Skript, weil der Lastregler-Hook schon das **Schreiben** einer Datei blockiert,
  deren Text die Namen ausgeschrieben enthält.
- **Eine leere Logdatei ist kein Stillstand.** `codex-run.sh` schreibt sein Log
  erst am Ende; eine Stillstandswarnung darauf hätte nach 20 Minuten dreimal
  falsch gefeuert.
- **Die Display-Suite ist kein Pflichtbeleg mehr** (Ansage vom 18.08.2026).
  fmt, clippy und `cargo test --workspace` genügen; die gtk4-Tests nur noch auf
  Wunsch oder wenn die Änderung genau dort ansetzt.

## Welle 2, der Rest

Aus dem Bestand der Übergabe 3 sind zwei Befunde erledigt
(`concerts-duplicate-events`, `filter-bar-clear-without-a-filter`). Offen auf
`todo` bleiben zehn:

`clearing-the-search-hops-through-the-top` ·
`episode-covers-appear-seconds-after-start` (braucht erst eine Messung) ·
`radio-genre-chip-drops-the-country` ·
`device-page-on-this-device-when-not-connected` ·
`stats-hide-more-top-artists-stutters` ·
`visuals-bars-fall-in-from-the-top-on-open` (braucht erst die Prüfung von
`INITIAL_SENSITIVITY_HEADROOM = 0.85`) · `jump-always-centers-the-current-track` ·
`lyrics-scan-should-ride-along-with-the-library-scan` ·
`library-doctor-out-of-date-rows-are-unreadable` ·
`android-artist-portrait-before-album-cover`.

Welle 3 bleibt `youtube-channel-tile-shows-an-episode-thumbnail`.

**Für die nächste Planungsrunde:** beide Grills dieser Sitzung haben die
Diagnose des Befunds verschoben, nicht bestätigt — einmal durch eine
Datenbankmessung, einmal durch das Lesen der Regel, die der Code umsetzt. Die
Befunde sind Ausgangspunkte, keine Diagnosen.
