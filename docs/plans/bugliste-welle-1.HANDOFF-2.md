# Übergabe 2: Bugliste Welle 1 — zwei Stränge fertig, einer läuft noch

**Stand:** 18.08.2026, ~14:45. Fortsetzung von `bugliste-welle-1.HANDOFF.md`.
**Basis aller drei Zweige:** `origin/dev` = `bf546d6cc8`.
**Nichts gelandet, nichts gepusht.** Die Freigabe zum Landen steht aus — so
vereinbart.

## ACHTUNG ZUERST: der Wake-Lock muss bleiben

`wake-lock` hält `bugliste` **noch**, und das ist Absicht: Strang 1 läuft weiter.

```sh
wake-lock status                 # 'bugliste' muss auftauchen, solange Codex läuft
wake-lock release bugliste       # ERST wenn Strang 1 durch ist
```

Die vorige Übergabe hat den Lock am Sitzungsende freigegeben — das war dort
richtig (nichts lief), hier wäre es falsch.

## Wo die drei Stränge stehen

| Strang | Worktree `~/Projects/reprise-<slug>` | Phase | Beleglage |
| --- | --- | --- | --- |
| `compact-player-misses-external-artwork` | `-compact-player-misses-external-artwork` | **`refactored`** | vollständig, siehe unten |
| `podcast-resume-pill-survives-finishing-the-episode` | `-podcast-resume-pill-…` | **`refactored`** | vollständig, siehe unten |
| `youtube-streaming-internal-data-stream-error` | `-youtube-streaming-…` | `planned` | **Codex läuft noch** |

Alle drei Zweige heißen `feature/<slug>` und sitzen auf `bf546d6cc8`. In jedem
Worktree ist die zugehörige Plandatei als erster Commit **eingecheckt** — nötig,
damit `land.sh` den Zweig über `^branch: <BR>$` findet, und zugleich der Schutz
gegen das bekannte Verschwinden ungetrackter Pläne.

### Strang 2 — compact-player, fertig

Commits: `ec14e2a363` (Plan), `64aafda448` (Fix), `30c3a0fff9` (Test).

- Review (rust-reviewer): Produktionscode **korrekt**. Der Wettlauf ist über zwei
  getrennte `Rc<Cell<u64>>`-Zähler sauber gelöst; die Flacker-Sicherung hängt an
  der GTK-Eigenschaft, dass `Image::paintable()` nach `set_icon_name` `None`
  liefert — der Reviewer hat das gegen die **installierte** GTK4-Bibliothek per
  `python3-gi` nachgemessen, nicht angenommen.
- Ein Befund (HIGH): der beigelegte Test war ein reiner Quelltext-Grep. Angenommen
  und von Codex behoben — der neue Test ist display-abhängig und misst Verhalten.
- **Mutationsnachweis, von mir unabhängig nachgefahren** (nicht nur von Codex
  behauptet):
  - Kontrollarm unmutiert: **grün** (diese Kontrolle fehlte in Codex' Bericht;
    ohne sie sind beide roten Läufe wertlos).
  - `is_none()` → `is_some()`: **rot**, `…:250 the compact cover fell back to the
    placeholder while the first snapshot was loading`.
  - gemeinsamer Generationszähler: **rot**, `…:279 both player artwork targets
    must end with the second snapshot; bar=None, compact=Some([199, 199, 199])`.
  - Beide vollständig zurückgenommen, Worktree sauber, Produktionsteil
    byte-identisch zum Stand vor dem Refactor.
- Codex' Gate-Liste (fmt, clippy, `cargo test --workspace`, audit, UX-Traceability
  384 Regeln) ist **nicht** von mir nachgemessen. Die Mutationsläufe belegen den
  Test, nicht die Suite.

### Strang 3 — podcast-resume-pill, fertig

Commits: `791b98efad` (Plan), `e21b2c6b8c` (Fix).

- Review (rust-reviewer): **freigegeben, kein CRITICAL/HIGH.** Zwei Zweifel
  ausgeräumt: die Abweichung von der Plan-Dateiliste ist korrekt
  (`podcasts_view_marker.rs` ist der Ort des Schwestermusters, das der Plan selbst
  als Vorbild zitiert), und „schwach verdrahtet" stimmt — Sidebar und beide Views
  nur als `Weak`, kein Zyklus.
- **Von mir nachgemessen:** `cargo fmt --check` grün, `cargo clippy --all-targets
  --workspace -- -D warnings` grün, `cargo test --workspace` **5354 passed,
  0 failed, 781 ignored**. Codex' Zahl für GNOME (`1908 passed, 752 ignored`)
  deckte sich exakt.
- **Display-Suite vollständig gefahren:** `scripts/check-display-tests.sh` →
  `passed: 752, failed: 0 of 752`. Das ist die ganze ignorierte Suite, nicht die
  vier Einzeltests aus Codex' Bericht.
- Der Test belegt die Kernzusage über **Pointer-Identität** der Zeile
  (`before.as_ptr() == after.as_ptr()`) plus unverändertes `expanded_sources` —
  echter Verhaltensnachweis für „chirurgisch, kein Neuaufbau".

### Strang 1 — YouTube-Range-Proxy, läuft noch

Seit ~1 h 47 min. Der Implementierungs-Commit steht bereits:

- `7ec57ae194` (Plan), `66dadba086` `fix(podcasts): proxy YouTube streams through
  bounded ranges`. Arbeitskopie sauber.

Codex ist danach **weiter aktiv** (vermutlich Verifikation/Gates), deshalb steht
die Phase noch auf `planned`. Nicht anfassen, solange der Prozess lebt.

```sh
heavy-run status | head -3      # hält er noch zwei Slots?
cat ~/Projects/reprise-youtube-streaming-internal-data-stream-error/.pipeline-codex.md
git -C ~/Projects/reprise-youtube-streaming-internal-data-stream-error log --oneline bf546d6cc8..HEAD
```

Die Hintergrund-Benachrichtigung dieser Sitzung ist mit ihr weg — der Zustand
steht in `.pipeline-codex.md` im Worktree (überlebt die Sitzung; die
Scratchpad-Logs unter `/tmp/claude-1000/…` nicht zwingend).

## Der nächste Schritt

1. Warten, bis Strang 1 seine Slots freigibt. Dann `.pipeline-codex.md` lesen,
   Diff gegen `bf546d6cc8` ansehen, `status.sh set … phase coded`.
2. `/check` für Strang 1 — **`rust-reviewer` UND `security-reviewer`**. Der zweite
   ist hier Pflicht und war bei den anderen beiden entbehrlich: der Strang baut
   einen HTTP-Server auf `127.0.0.1` mit Token-Schutz. Zu prüfen sind mindestens
   Token-Vergleich in konstanter Zeit, Bindung wirklich nur auf Loopback,
   Fenstergrenze ≤ 1 000 000 Bytes, und ob eine fremde lokale Anwendung den Proxy
   als offenen Weiterleiter missbrauchen kann.
3. Danach die Gates für Strang 1 **selbst** nachmessen (siehe unten), nicht Codex'
   Liste übernehmen.
4. Erst dann alle drei Zweige dem Nutzer zur Freigabe vorlegen. **Landen nur nach
   ausdrücklichem Wort.**

### Merge-Reihenfolge, wenn freigegeben

Strang 1 und Strang 3 fassen **beide** `external_media.rs` an (Strang 1 als
Anschlusspunkt des Proxys, Strang 3 für die `notify_episode_played(id)`-Signatur).
Die Prüfung Strang 2 gegen Strang 3 war disjunkt; Strang 1 konnte noch nicht
geprüft werden.

Vorschlag: **Strang 2 zuerst** (berührt keinen der anderen), dann **Strang 1**,
dann **Strang 3** darauf rebasen. Vor dem Landen die tatsächliche Überschneidung
messen, nicht raten:

```sh
comm -12 <(git -C ~/Projects/reprise-youtube-streaming-internal-data-stream-error \
            diff --name-only bf546d6cc8..HEAD | grep -v '^docs/plans/' | sort) \
         <(git -C ~/Projects/reprise-podcast-resume-pill-survives-finishing-the-episode \
            diff --name-only bf546d6cc8..HEAD | grep -v '^docs/plans/' | sort)
```

## Neu in dieser Sitzung: ein weiterer Bug

`docs/plans/youtube-channel-tile-shows-an-episode-thumbnail.md`, `phase: todo`,
**ungetrackt** — gehört früh committet.

Gemeldet: *„der yt kanal sollte das kanalbild nutzen und nicht ein episodencover"*.
Ursache vollständig belegt, drei Stufen:

1. yt-dlp **liefert** das Kanalbild (am Objekt gemessen: neun `thumbnails`, sechs
   Bannerzuschnitte, ein quadratisches Avatar 900×900, dazu `avatar_uncropped`),
   aber `YoutubeListing` hat kein Bildfeld, also fällt der Wert bei
   `project_playlist` heraus.
2. `project_youtube_feed` (`pipeline.rs:156`) setzt `image_url: None` als
   Konstante → die DB-Spalte bleibt für YouTube-Abos dauerhaft NULL.
3. Die GUI maskiert das (`podcasts_groups.rs:337-345`) mit dem Cover der
   **neuesten** Episode. Für RSS ist derselbe Zweig `None`.

**Die Falle für den künftigen Plan:** `entry_image_url` nimmt den *ersten*
`thumbnails`-Eintrag. Auf Kanalebene ist das das Banner 1060×175 — ein 6:1-Streifen
in einer quadratischen Kachel. Wer nur Stufe 1+2 repariert, macht es schlimmer.
Gebraucht wird eine **eigene** Auswahlregel (`avatar_uncropped`, sonst der größte
quadratische Eintrag).

Nebenbefund: die Detailansicht (`youtube_channel_detail.rs:490`) liest ohne den
Rückfall und zeigt gar kein Bild — Kachel und Detail widersprechen sich.

Einordnung: **Welle 3.** Er fasst `podcasts_groups.rs` und
`youtube_channel_detail.rs` an — dieselben Dateien wie Strang 3 —, muss also nach
dessen Landung geplant/rebased werden.

## Gedächtnis-Korrektur (wichtig)

`reprise-known-red-display-tests-on-dev` ist als **überholt** markiert. Die dort
gelisteten sechs „auf dev bekannt roten" Display-Tests (Stand 05.–13.08.) sind
**alle grün**: gemessen 18.08.2026, volle Suite `752/752`. Plausibel, weil der
Gate seit #463 die ganze ignorierte Suite fährt statt dreier Namensfilter.

**Konsequenz: ein rotes Display-Ergebnis ist wieder verdächtig.** Die alte Liste
als Freibrief zu benutzen, ließe jetzt genau die Regression durch, gegen die sie
einmal geschrieben wurde. Die Gegenprobe-Methode im Eintrag bleibt gültig.

## Betriebsfallen, die diese Sitzung gekostet haben

- **Der `cp`-Alias fragt interaktiv nach** und hängt jedes nicht-interaktive
  Skript auf. Rücknahme von Mutationen über `git checkout -- <pfad>`, nie `cp`.
- **Rücknahme gehört in ein `trap`, nicht ans Skriptende.** Mein erster
  Mutationslauf lief in den 2-Minuten-Deckel des Werkzeugs und ließ die Mutation
  im Baum stehen.
- **Eine halbe Mutation belegt nichts und sieht aus wie ein Freispruch.**
  `compact_cover_generation` kommt dreimal vor; mein `sed` traf nur die eine mit
  `&self.`-Präfix. Der Test blieb zu Recht grün — der Code teilte sich nie einen
  Zähler. Erst mit allen drei Vorkommen wurde er rot. Immer die Zahl der
  verbliebenen Vorkommen ausgeben.
- **Der Lastregler-Hook liest den Kommandotext**, nicht den Prozess. Wörter wie
  `codex-run`, `check-merge-readiness` oder `xvfb-run` blockieren selbst ein
  harmloses `sed`/`ls`. Umgehung: Glob (`codex?run.sh`) oder Zeichenklasse
  (`xv.b`); echte Läufe mit `heavy-run medium -- …`.
- **Drei `medium`-Läufe belegen alle sechs Slots.** Das ist die Obergrenze, wie
  die erste Übergabe sagte — bestätigt.

## Wie die Gates nachzumessen sind (nicht Codex glauben)

```sh
WT=~/Projects/reprise-<slug>
cd "$WT" && heavy-run medium -- bash -c \
  'cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace' \
  > /tmp/gates.log 2>&1
grep -E '^test result:' /tmp/gates.log | awk '{p+=$4; f+=$6} END {print "passed="p, "failed="f}'

cd "$WT" && heavy-run medium -- ./scripts/check-display-tests.sh > /tmp/display.log 2>&1
tail -3 /tmp/display.log        # 'failed: 0 of 752'
```

`cargo test --workspace` fährt die Display-Tests **nicht** (sie sind die 752
ignorierten) — beide Läufe sind nötig. Ganze Logs nie zurücklesen, nur greppen.

## Die restlichen Bugs

Unverändert die Liste aus `bugliste-welle-1.HANDOFF.md`, Wellen 2+, plus der neue
Kanalbild-Befund. `playback-errors-report-the-first-cause` steht weiter auf
`planned` und ist der nächste Kandidat für Welle 2 — er ist geplant und
gegrillt, es fehlt nur die Ausführung.
