# Übergabe: Bugliste, Welle 1 — Planung fertig, Codex noch nicht gestartet

**Stand:** 18.08.2026, ~13:00. Sitzung endet hier auf Wunsch (`/clear`).
**Basis:** `origin/dev` = `bf546d6cc8` (gefetcht 18.08.2026, 12:35).
**Nichts committet, nichts gebaut, kein Worktree angelegt, kein Codex gelaufen.**

## Auftrag

> „arbeite bei unseren TODOs die bugliste ab"

Die TODOs sind die Plandateien mit `phase: todo` in `docs/plans/`. Zum
Sitzungsbeginn waren es **24**, davon **15 echte Bugs** (der Rest Wünsche:
Changelog, Downloads, Dependabot, Repo-Tidy, Tooltips, Podcast-Chip,
YouTube-Kanalgröße, Plugins-Hierarchie).

Im Chat abgestimmt (18.08.2026):

| Frage | Antwort des Nutzers |
| --- | --- |
| Vorgehen | **Batchweise, 3–4 parallel** durch die Pipeline, je eigener Worktree |
| Umfang | **Alle 15** Bugs, inklusive der drei UX-Befunde |
| Autonomie | **Grilling ja, Landen erst nach Freigabe** — implementieren und reviewen autonom, Merge nach `dev` nur mit ausdrücklicher Freigabe |

## Was in dieser Sitzung passiert ist

1. Die Bugliste aus den 24 TODOs herausgeschnitten und mit dem Nutzer bestätigt.
2. **Den YouTube-Streamfehler diagnostiziert** — er war der einzige Befund ohne
   Diagnose, und die Diagnose war der eigentliche Ertrag der Sitzung (unten).
3. Die anderen zwei Befunde der Welle am Code gegengeprüft; dabei fielen **alle**
   ihre offenen Fragen weg (siehe „Am Code geklärt" in den Plänen).
4. Das Grilling gefahren — fünf Entscheidungen, alle vom Nutzer beantwortet.
5. Vier finale Pläne geschrieben, `phase: planned`.

## Der Kern: warum YouTube-Streaming scheitert

Reproduzierbar, gemessen am 18.08.2026. Vollständig in
`docs/plans/youtube-streaming-internal-data-stream-error.md`; hier die Kurzform,
weil sie das teuerste Wissen der Sitzung ist.

- **Die angezeigte Meldung ist nicht der Fehler.** „Internal data stream error."
  ist die **dritte** Meldung einer Kette. Die erste steht nur im Journal:
  `Forbidden (403)`.
- **googlevideo bedient nur noch begrenzte Bereichsanfragen unter ~1 MiB.**
  `curl -r 0-999999` → 206. `Range: bytes=0-` oder ganz ohne Range → 403.
  `souphttpsrc` sendet **kein** `Range` (mitgeschnitten) — also 403, jedes Mal.
- **Einzeln ausgeschlossen:** User-Agent, `compress`, `keep-alive`,
  `icy-metadata`, HTTP/1.1 vs. HTTP/2, DLNA-Header, Ablauf (URL war 1 s alt),
  IP-Bindung (gleiche IP), yt-dlp-Version.
- **Kontrolliert:** frische URL, umgekehrte Testreihenfolge, identisches
  Ergebnis. Die Grenze ist nicht durch die eigenen Vorabfragen entstanden.
- **Nicht betroffen:** heruntergeladene Episoden, Podcast-Enclosures, Radio,
  und die Android-App (sie hat keinen YouTube-Weg).

Der Reproduktionsbefehl für die nächste Sitzung:

```sh
yt-dlp --no-warnings -f bestaudio -j "https://www.youtube.com/watch?v=yXB_llqHXJU" \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['url'])" > /tmp/u.txt
curl -s -o /dev/null -w "%{http_code}\n" -r 0-1000        "$(cat /tmp/u.txt)"   # 206
curl -s -o /dev/null -w "%{http_code}\n" --max-time 8     "$(cat /tmp/u.txt)"   # 403
gst-launch-1.0 -q souphttpsrc location="$(cat /tmp/u.txt)" ! fakesink           # 403
```

## Die fünf Grilling-Entscheidungen

1. **Transport: lokaler Range-Proxy.** Prozessintern auf `127.0.0.1`,
   token-geschützt, Fenster ≤ 1 000 000 Bytes über `&range=`. Gegen
   „yt-dlp als Transport" und gegen „Streamen heißt Herunterladen".
2. **Wirkungsbereich: nur yt-dlp-aufgelöste URLs.** Podcast und Radio bleiben
   unberührt.
3. **Ort: `reprise-core`** — `ureq = "3"` liegt dort, `reprise-gnome` hat keinen
   HTTP-Client, ohne GTK ist der Proxy testbar. (Faktenlage, keine Entscheidung.)
4. **Resume-Pille: chirurgisch.** `notify_episode_played(id)` reicht die ID
   durch, die Ansicht patcht genau eine Zeile. Kein `refresh()`, kein Neuaufbau.
5. **Transportfehler markieren die Zeile nicht mehr als „Unavailable now".**
   Das bleibt Episodenfehlern (404/410/Extraktor) vorbehalten.

## Die vier Pläne

| Plan | Phase | Welle |
| --- | --- | --- |
| `youtube-streaming-internal-data-stream-error.md` | `planned` | **1** |
| `compact-player-misses-external-artwork.md` | `planned` | **1** |
| `podcast-resume-pill-survives-finishing-the-episode.md` | `planned` | **1** |
| `playback-errors-report-the-first-cause.md` | `planned` | 2 |

Der vierte ist in dieser Sitzung **neu entstanden**: der ursprüngliche
YouTube-Befund zerfiel in zwei unabhängige Fehler (Transport / Meldung). Der
alte TODO-Text ist durch den Transportplan ersetzt.

Alle vier sind ungetrackt. **Vorsicht:** ungetrackte Pläne verschwinden
regelmäßig aus dem geteilten Hauptcheckout — sie gehören früh committet.

## Der nächste Schritt

Die Pläne sind fertig, das Grilling ist durch. Es fehlt nur noch die Ausführung:

```sh
WT=~/.claude/skills/pipeline/scripts/worktree.sh
CR=~/.claude/skills/pipeline/scripts/codex-run.sh
ST=~/.claude/skills/pipeline/scripts/status.sh

# 1. alle drei Worktrees VOR dem ersten Codex-Lauf
for s in youtube-streaming-internal-data-stream-error \
         compact-player-misses-external-artwork \
         podcast-resume-pill-survives-finishing-the-episode; do
  p=$($WT ensure "$s")
  $ST set docs/plans/$s.md worktree "$p"
  $ST set docs/plans/$s.md branch "feature/$s"
done

# 2. je Worktree .pipeline-task.md schreiben (Planrumpf + Datei-Ownership +
#    „Implement this plan. Make focused commits. Touch only files this
#     strand owns. Do not touch files outside this worktree.")
# 3. drei codex-run.sh im Hintergrund, EIN Wake-Lock für den ganzen Lauf
```

Danach: `/check` je Worktree (rust-reviewer, Sonnet/high), `/refactor` durch
Codex, dann **stopp** — die drei Branches dem Nutzer zur Freigabe vorlegen.
Erst danach `land.sh`.

**Betriebshinweise für den Lauf:**
- Die Pipeline-Skripte liegen **nicht** in `scripts/` des Repos, sondern in
  `~/.claude/skills/pipeline/scripts/`. Immer mit vollem Pfad aufrufen.
- Der Lastregler-Hook blockt jedes Kommando, dessen *Text* `codex-run` enthält —
  auch ein harmloses `ls`. Für Nicht-Läufe `HEAVY_RUN_DISABLE=1`, für echte
  Läufe `heavy-run medium -- …` (`heavy` verhungert an fremden Läufen).
- Maschine: 8 Kerne, 6 freie Lastregler-Slots. **Drei** gleichzeitige Kaltbuilds
  sind die Obergrenze; vier bremsen sich gegenseitig aus. Genau deshalb ist der
  vierte Plan in Welle 2 gewandert.
- Wake-Lock `bugliste` war gesetzt und ist beim Sitzungsende **freigegeben** —
  vor dem nächsten Lauf neu nehmen.

## Die zwölf restlichen Bugs (Wellen 2+)

Reihenfolge nach Schwere, wie mit dem Nutzer besprochen. Noch kein Plan, nur der
Befund:

1. `playback-errors-report-the-first-cause` *(geplant, wartet auf Welle 2)*
2. `concerts-duplicate-events`
3. `clearing-the-search-hops-through-the-top`
4. `episode-covers-appear-seconds-after-start` — braucht **erst eine Messung**
   (Zeit von `queue()` bis `on_ready` je Zeile), dann einen Plan
5. `filter-bar-clear-without-a-filter`
6. `radio-genre-chip-drops-the-country`
7. `device-page-on-this-device-when-not-connected`
8. `stats-hide-more-top-artists-stutters`
9. `visuals-bars-fall-in-from-the-top-on-open` — Hauptverdacht steht im Befund
   (`INITIAL_SENSITIVITY_HEADROOM = 0.85`), ist aber **ungeprüft**
10. `jump-always-centers-the-current-track`
11. `lyrics-scan-should-ride-along-with-the-library-scan`
12. `library-doctor-out-of-date-rows-are-unreadable`
13. `android-artist-portrait-before-album-cover`

## Was ich nicht getan habe

- **Kein Code geändert, kein Build gefahren, kein Worktree angelegt.** Der
  Zustand des Checkouts ist unverändert bis auf vier Plandateien und eine
  gelöschte Draft-Runde.
- Die zwölf restlichen Bugs sind **nicht** geplant — nur eingeordnet.
- Die Messung zu `episode-covers-appear-seconds-after-start` steht aus. Der
  Befund nennt sie ausdrücklich als Vorbedingung („Erst messen, dann bauen"),
  und ich habe sie nicht ersetzt durch eine Vermutung.
