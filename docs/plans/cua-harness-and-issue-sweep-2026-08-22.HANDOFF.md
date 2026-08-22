# Handover — PR-Durchgang, Issue-Triage und CUA-Harness, 22.08.2026

Stand ~13:00. Diese Datei ist **ungetrackt** und gehört in den nächsten
Doku-Commit. Vorgänger: `docs/plans/pr-and-issue-sweep-2026-08-22.HANDOFF.md`.

## Das Einzige, was jetzt Aufmerksamkeit braucht

**Zwei PRs warten auf den Merge: #623 und #624.** Beide sind reine Skript-/CI-
Änderungen, `bump-version.sh` meldet für beide `no desktop or Android app
changes`.

**PR #623 ist `CLEAN` und wartet nur auf den Merge.** `Quality gate` grün, alle
Rust-Stufen `skipping` (reine Skriptänderung, Route-Filter greift korrekt).

```bash
gh pr merge 623 --squash --delete-branch \
  --subject "fix(cua-e2e): make the responsive-window scenario runnable again"
```

Der Merge-Readiness-Gate wurde **nicht** gefahren — der Branch fasst nur
`scripts/cua-e2e/` an, `bump-version.sh --base origin/dev` meldet
`no desktop or Android app changes`. Wer sichergehen will, fährt ihn trotzdem.

## Was fertig ist

| | |
|---|---|
| #601, #621 | gemergt (`a3cdd68dae`, `41aca1beeb`) |
| #588 | geschlossen — von #608 überholt, dev's Optik bleibt |
| #79 | geschlossen — aktuelle Platzierung akzeptiert (Owner-Entscheidung) |
| #108 | **geschlossen** — zwei grüne Läufe, gegen die Evidenz geprüft |
| #622 | neu — Last.fm-Credentials an keinen Build angeschlossen |
| #623 | neu — vier Harness-Defekte repariert |

Alle 16 damals offenen Issues tragen jetzt ihren Stand. **Achtung:** der
Vorgänger-Handoff behauptete „10 kommentiert" — auf GitHub stand null. Die 13
Schließungen waren echt, die Kommentare auf den *offen gebliebenen* Issues
fehlten vollständig und wurden hier nachgeholt.

## Die vier Harness-Defekte (Inhalt von #623)

Das `responsive-window`-Szenario konnte gar nicht laufen. Jeder Defekt gemessen,
nicht gefolgert:

1. **Die Readiness-Schleife brach an ihrer eigenen Wartebedingung ab.** Die
   AT-SPI-Brücke registriert sich Sekunden nach dem Fenster (~1,3 s Fenster,
   ~2,8 s nutzbarer Baum), frühe Snapshots sind `degraded`. `cua_snapshot`
   lehnt die zu Recht ab — unter `set -e` killte die Ablehnung aber
   `wait_for_label` im Versuch 1 von 24. Beleg: genau eine
   `responsive-ready-1.json`, `total_element_count: 1`.
2. **`snapshot_id` fehlte in jedem Element-Payload.** cua-driver verlangt ihn
   seit 0.17, unverändert auf 0.21.0. Vier Payload-Stellen betroffen.
3. **`role == "label"` ist unentscheidbar.** Der Treiber liefert nur
   interaktive und strukturelle Rollen; `label` kommt in einem ganzen Lauf
   kein einziges Mal vor, und `get_window_state` hat keinen Schalter dafür.
4. **Der Pixelklick auf den Toast wirkte nicht.** Punkt (510,710) liegt auf der
   Schaltfläche, der Klick wird zugestellt, nach 24 Abfragen steht der Toast
   unverändert und die Restzeit ging nur 1:51 → 1:50 (normale Wiedergabe) —
   er traf also auch die Seek-Leiste darunter nicht.

Defekt 1 verdeckte alle anderen.

**Struktureller Fund:** `run.sh` definiert `wait_for_label` /
`wait_for_label_absent` neu, **nachdem** es `lib.sh` mit
`cua_wait_for_label` / `cua_wait_for_label_absent` geladen hat. Die Szenarien
rufen die `run.sh`-Fassung. Wer die Bibliothek repariert, repariert nichts —
das hat hier einen ganzen Lauf gekostet. Beide Kopien sind gefixt, das
Zusammenlegen steht aus.

**Neu:** `scripts/cua-e2e/atspi_probe.py` liest den a11y-Baum direkt. Es
existiert wegen Defekt 3: Aussagen über statischen Text sind über den Treiber
nicht mehr entscheidbar, und eine Zusage auf das umzuschreiben, was der Treiber
zufällig liefert, senkt die Latte still. Die Sonde legt **immer** den gesehenen
Baum ab. Erstes Ergebnis: die App exponiert **beide** Spielzeit-Anzeigen (2
bzw. 4 Treffer) — kein Barrierefreiheits-Fehler, nur ein blinder Treiber.

## Drei Befunde, die Pläne widerlegen

### 1. #620 entsteht nicht durch einen Schreiberkonflikt

Der Sweep-Plan (Aufgabe 3) fragt: Restore- oder Ankerpfad? **Keiner von beiden.**

```
set_on_title_click → reveal_playing_track → NavigationIntent::RevealTrack
  → metadata_navigation::navigate → history.navigate_from
  → library_shell::route_to_place → track_list.restore_browser_place
  → view_state_memory::restore → restore_scroll_when_ready
  → apply_restored_scroll → reload_restore::scroll_target
```

`centered_scroll_restore::schedule` kommt darin nicht vor. Und
`navigation.rs:419` setzt `TrackAnchor::new(track_id, 0.0)`, was
`reload_restore.rs:149` als `layout.row_top(position) + 0.0` rechnet — Zeilen-
oberkante = Viewportoberkante. **Das Obenlanden ist spezifiziert**, nicht
erkämpft. Der geplante Preseed (Aufgabe 4) sitzt auf einem Pfad, den dieser
Klick nicht nimmt, und repariert #620 daher nicht. Aufgabe 13 (#475) hängt an
Aufgabe 4 und erbt die Frage.

### 2. Der #406-Befehl im Sweep-Plan existiert nicht

`hover-affordance-sweep` und `pointer-layout-reachability` sind **keine**
Szenariengruppen. Die Liste in `run.sh:662-694` ist fest kodiert und enthält
beide nicht; `CUA_E2E_ONLY` hätte mit `unknown CUA_E2E_ONLY scenario`
abgebrochen. Es gibt ein separates Verzeichnis `scripts/ptr-e2e`. **#406 hängt
an einem anderen Prüfstand als angenommen** — das ist ungeprüft.

### 3. #97 wird vom grünen Lauf nicht entschieden

Ich hatte im Triage-Kommentar geschrieben, `responsive-window` entscheide es.
Falsch: das Szenario prüft Seitenpanel-Verhalten an Breakpoints
(`responsive_window.sh:176-193`), aktiviert aber nie Podcasts/YouTube/Radio und
prüft nie die Nichtüberlappung bei breitem Fenster. Die Korrektur steht im
Issue, samt Vorschlag, welche zwei Zusagen es tatsächlich messen würden.

Aus dem Code steht weiter: Hälfte 1 ist erledigt (`activate_sidebar_route`
schließt nur bei `collapsed`, gepinnt durch
`doc_6b_sidebar_activation_routes_to_library_while_the_job_keeps_running`).
Hälfte 2 ist ungemessen.

## #622 — Last.fm-Credentials

Owner-Entscheidung 22.08.: **Repository-Secrets**, Injektion zur Bauzeit.

Betrifft **nur Last.fm**. ListenBrainz braucht keinen App-Key —
`listenbrainz_secret.rs` speichert nur einen Nutzer-Token im Schlüsselbund.

Keys: <https://www.last.fm/api/account/create>, verwalten unter
<https://www.last.fm/api/accounts>. Die Callback-URL ist Pflichtfeld, wird aber
nicht benutzt (Desktop-Flow `auth.getToken` → Browser → `auth.getSession`,
`lastfm.rs:97,120`; Signatur schließt `callback` aus, `lastfm.rs:189`).

**Die Falle, gegen die Toolchain geprüft:** `flatpak-builder` hat **kein**
`--env`. `flatpak build` hat es, und `build-options.build-args` reicht dorthin
durch — aber das ist Manifest-Inhalt. Ein Secret im Job-Env erreicht cargo im
Sandkasten also nicht. CI muss `io.github.marvinbaudach.Reprise.yml` auf dem
Runner patchen (Werte in die vorhandene `build-options.env`), das committete
Manifest bleibt sauber. `build-options.env` statt `build-args`, weil ein
`--env=`-Argument in der Prozessliste steht. Manifest nicht ins Log echoen.
Nebeneffekt: flatpak-builder cached über das Manifest, Patchen invalidiert.

**Offen und von der Entscheidung nicht gelöst:** Flathub baut aus dem
öffentlichen Manifest, nicht aus unserem Workflow — dieser Kanal bleibt bei
BYO, solange die Werte nicht öffentlich im Repo stehen.

**Erledigt (22.08. nachmittags):** Der Owner hat die Anwendung registriert und
beide Secrets angelegt (`gh secret list` bestätigt sie, 11:01/11:02 UTC). Die
Verdrahtung liegt als **PR #624**:

- `scripts/inject-build-credentials.py` patcht das ausgecheckte Manifest
  unmittelbar vor `flatpak-builder`; das committete bleibt sauber.
- `release.yml` reicht die Secrets nur an diesen einen Schritt.
- `RELEASING.md` bekommt den Abschnitt „Bundled Last.fm credential".

Drei Arme lokal geprüft: ohne Secrets bleibt das Manifest byte-identisch; mit
Secrets (Werte mit `"` und `#`) parst es und beide Werte kommen exakt zurück;
ohne Anker bricht es mit Exit 1 ab, statt still ein Release ohne Credential zu
bauen.

**Nicht angefasst, aber dieselbe Lücke:** `REPRISE_TICKETMASTER_APIKEY` ist
ebenfalls in **keinem** Workflow verdrahtet — nur in `RELEASING.md`
beschrieben. Das Secret existiert seit 26.07. Ein Einzeiler in
`inject-build-credentials.py` (`VARIABLES`) würde es mitnehmen; bewusst nicht
getan, weil es das Concerts-Verhalten ohne Auftrag ändert.

## Aufräumen (nicht gemacht, bewusst)

**Wake-Locks:** `wake-lock release cua-sweep` und `wake-lock release
pr-merge-sweep` — Letzterer stammt aus der Vorsession und ist nach dem
#601-Merge fällig. Die übrigen (`ghostty`, `hub-api-spec`, `krypto-paket3`,
`release-0144`) gehören anderen Sessions, **nicht anfassen**.

**Worktrees:** `.worktrees/pr601` und `.worktrees/issue-sweep` sind erledigt und
können mit `scripts/close-worktree.sh` weg; die lokalen Branches `merge/pr601`
und `docs/open-issue-sweep` brauchen `git branch -D` (nach Squash-Merge wird
kein Branch als merged gemeldet). `.worktrees/cua-sweep` bleibt bis #623 durch
ist.

## Was ich nicht geprüft habe

- Der Sweep lief nur `responsive-window`, und nur im **debug**-Profil. Der Plan
  nennt `CUA_E2E_PROFILE=release`. Ob release dasselbe Ergebnis liefert, ist
  offen.
- `scrobbling.sh` und `lib.sh` wurden an vier Payload-Stellen geändert, aber
  nur `responsive-window` ist gefahren. Die anderen Szenariengruppen sind
  ungetestet — sie waren vorher aus demselben Grund kaputt und sollten jetzt
  besser laufen, das ist aber unbelegt.
- #406 (falscher Prüfstand), #97 Hälfte 2, und alle übrigen offenen Issues
  bleiben auf dem Stand ihres Triage-Kommentars.
