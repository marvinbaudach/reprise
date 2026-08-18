---
slug: youtube-channel-tile-shows-an-episode-thumbnail
worktree: /home/marvin/Projects/reprise-youtube-channel-tile-shows-an-episode-thumbnail
branch: feature/youtube-channel-tile-shows-an-episode-thumbnail
phase: planned
codex_session:
created: 2026-08-18
---
# Der YouTube-Kanal trägt sein eigenes Bild

Ersetzt den Befund gleichen Slugs vom 18.08.2026. Zwei Messungen an derselben
Installation (18.08.2026, 22:41–22:50) haben ihn in einem Punkt **widerlegt**
und die eigentliche Quelle des Fehlers freigelegt — siehe „Was die Datenbank
sagt".

## Was der Nutzer sieht

Die Kachel eines abonnierten Kanals in der YouTube-Liste trägt das Vorschaubild
eines **Videos** statt des Kanal-Avatars. Die Detailansicht desselben Kanals
zeigt dagegen gar kein Bild, nur `video-x-generic-symbolic`.

## Was die Datenbank sagt (die Korrektur am Befund)

`sqlite3 "file:~/.local/share/reprise/reprise.db?mode=ro"` über
`podcast_subscriptions`, 18.08.2026:

```
2  youtube  HOLLOW FALLEN        <NULL>
4  youtube  VOID PREACHER        <NULL>
12 youtube  Heldom               https://i.ytimg.com/vi/ksu_4tR47F0/hq720.jpg…
13 youtube  Danheim              https://i.ytimg.com/vi/Le6-gZNCvNE/hqdefault…
14 youtube  Bjorth               https://i.ytimg.com/vi/9WXsdApQIY4/hq720.jpg…
15 youtube  Nordheim Melodies    https://i.ytimg.com/vi/4pO4TbR4Sw0/hq720.jpg…
16 youtube  NordicPulse          https://i.ytimg.com/vi/ktgzAtgQC2E/hq720.jpg…
17 youtube  Mystical Nordic Amb  https://i.ytimg.com/vi/TRTzhoLwUBI/hq720.jpg…
18 youtube  Jacob Lizotte        https://i.ytimg.com/vi/S_MZC2jHngw/hq720.jpg…
```

Der Befund behauptete, die Spalte bleibe für YouTube-Abos **dauerhaft NULL** und
die GUI maskiere das mit dem Episodencover. Für zwei der neun Abos stimmt das.
Für die anderen sieben steht ein **Video-Thumbnail fest in der Datenbank**, und
es kommt nicht aus dem GUI-Rückfall, sondern aus dem Anlege-Weg: `subscribe`
(`crates/reprise-gnome/src/ui/podcasts/add_dialog_subscription.rs:57`) schreibt
`candidate.image_url`, und für YouTube ist das entweder das Vorschaubild aus
`parse_search_channels` (`ytdlp_search.rs:39-40,61` — ein Video-Thumbnail, weil
Suchtreffer Videos sind) oder `listing.image_url` aus der URL-Vorschau
(`add_dialog.rs:598`).

Gemessen: `yt-dlp --no-warnings --flat-playlist -J "ytsearch3:…"` liefert pro
Treffer **kein** Kanalbild — nur `thumbnails` mit 360×202/720×404 aus
`i.ytimg.com/vi/…`. Die Suche *kann* den Avatar nicht kennen, ohne pro Treffer
einen weiteren yt-dlp-Aufruf zu fahren.

Ein Fix, der nur die drei Stufen des Befunds repariert, lässt diese sieben
Zeilen also unberührt stehen und schreibt bei jedem neuen Abo denselben Fehler
erneut.

## Die Kette (belegt, unverändert gültig)

1. **`YoutubeListing` hat kein Bildfeld.** `YtDlpPlaylist.image_url` existiert
   (`crates/reprise-core/src/podcasts/ytdlp.rs:77`) und wird beim Parsen befüllt
   (`ytdlp.rs:746`), aber `YoutubeListing` (`youtube.rs:14-18`) trägt keins, also
   lässt `project_playlist` (`youtube.rs:33`) den Wert fallen.
2. **Die Projektion setzt das Feld hart auf `None`.** `project_youtube_feed`
   (`pipeline.rs:156`) schreibt `image_url: None` als Konstante.
3. **Die GUI maskiert die Lücke.** `group_image_url`
   (`crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs:337-345`) fällt nur
   für `PodcastKind::Youtube` auf `episodes.first().image_url` zurück (sortiert
   `published_at DESC`, `query.rs:56` — das neueste Video). Die Detailansicht
   (`youtube_channel_detail.rs:490`) liest `rendered.group.image_url` **ohne**
   diesen Rückfall; daher der Widerspruch zwischen Kachel und Detail.

## Was der Kanaldump wirklich enthält

`yt-dlp --no-warnings --flat-playlist --playlist-items 1 -J
"https://www.youtube.com/@kurzgesagt/videos"` (18.08.2026, 22:41), derselbe
Modus wie `ytdlp.rs:154`:

| `id` | `width`×`height` | URL |
|------|------------------|-----|
| `0`…`5` | 1060×175 … 2560×424 | Banner-Zuschnitte |
| `banner_uncropped` | **fehlt** | `…=s0` |
| `7` | 900×900 | `…/ytc/AIdro_…=s900-c-k-…` |
| `avatar_uncropped` | **fehlt** | `…/ytc/AIdro_…=s0` |

Ein flaches `thumbnail` gibt es auf Kanalebene nicht (`'thumbnail' in d` →
`False`). Zwei Eigenschaften bestimmen die Auswahlregel:

- `avatar_uncropped` trägt **keine Maße** (`keys == [id, preference, url]`) — er
  ist nur über seine `id` auffindbar, nicht über „quadratisch".
- `avatar_uncropped` ist `=s0`, das ungedeckelte Original. Die Kachel ist 40 px
  (`podcasts_groups.rs:283`), und `remote_image` kennt **keine** Obergrenze für
  die Abrufgröße. `=s900` aus `id: "7"` ist die richtige Größenordnung.

`entry_image_url` (`ytdlp_search.rs:112-120`) nimmt `thumbnail`, sonst
`thumbnails[0]` — auf Kanalebene ist das das **Banner 1060×175**. Wer die
Stufen 1 und 2 mit dieser Funktion repariert, tauscht das Episodencover gegen
einen 6:1-Streifen in einer quadratischen Kachel.

## Entscheidungen (im Grill festgelegt)

1. **Auswahlregel:** größter Eintrag mit `width == height` (beide vorhanden,
   > 0) → sonst `id == "avatar_uncropped"` → sonst `None`. **Niemals** das
   Banner; ein Kanal ohne Avatar bekommt das Fallback-Icon.
2. **Der Episoden-Rückfall wird ersatzlos gestrichen.** Kein Bild ist besser als
   ein falsches: der Rückfall versteckt jeden künftigen Rückschritt derselben
   Art. Damit lesen Kachel und Detailansicht **strukturell** dieselbe Quelle.
3. **Der Anlege-Weg speichert für YouTube kein Bild mehr.** Das Suchergebnis
   bleibt reine Vorschau; das Abo startet ohne Bild und bekommt es vom ersten
   Refresh. Ohne diesen Punkt schreibt jedes neue Abo den Fehler sofort zurück
   in die Datenbank.
4. **Die Trefferliste der Suche behält ihr Videobild** — dort ist ein Bild aus
   dem Kanal besser als ein graues Quadrat, und ein Avatar ist dort ohne
   zusätzlichen Netzaufruf gar nicht zu haben.
5. **Einmalige Migration statt Aussitzen.** Der Refresh ist alle
   `DEFAULT_REFRESH_HOURS = 6` h plus bis zu 1 h Jitter fällig
   (`config.rs:47`, `refresh.rs:102-119`) — ohne Migration bliebe die Liste bis
   zu sieben Stunden im falschen Zustand.

Zur Klarstellung, weil es beim Planen einmal falsch dastand: `image_url =
COALESCE(?7, image_url)` (`store.rs:360`) **ersetzt** einen vorhandenen Wert,
sobald der neue nicht NULL ist; es schützt nur gegen ein fehlendes neues Bild.
Der Refresh heilt die sieben Zeilen nach dem Fix also von selbst — die Migration
kürzt lediglich die Wartezeit ab.

## Aufgaben

**T1 — `channel_avatar_url` in `crates/reprise-core/src/podcasts/ytdlp_search.rs`.**
Neue `pub(super) fn channel_avatar_url(value: &Value) -> Option<String>` nach
Entscheidung 1. `entry_image_url` bleibt **unverändert** — auf Videoebene ist es
richtig und wird von `ytdlp.rs:732` und der Kanalsuche weiterverwendet.
Unit-Tests, jeder gegen die oben gemessene Struktur:
- die neun Einträge des Kanaldumps → die `=s900`-URL; nicht `=s0`, nicht Banner;
- nur Banner-Einträge (0…5 plus `banner_uncropped`) → `None`;
- kein quadratischer Eintrag, aber `avatar_uncropped` → dessen URL;
- `thumbnails` fehlt / ist leer / ist kein Array → `None`;
- zwei quadratische (88×88 und 900×900) → das größere;
- ein Eintrag mit `width == height == 0` zählt nicht als quadratisch.

**T2 — `ytdlp.rs:746`:** `YtDlpPlaylist.image_url` kommt aus
`channel_avatar_url(&value)` statt aus `entry_image_url(&value)`.

**T3 — `youtube.rs`:** `YoutubeListing` bekommt `image_url: Option<String>`;
`project_playlist` reicht `playlist.image_url` durch.

**T4 — `pipeline.rs:156`:** `project_youtube_feed` setzt
`image_url: listing.image_url` statt der `None`-Konstante.
Test in `pipeline_youtube_tests.rs`: ein Refresh mit einem Kanaldump, der Avatar
**und** Banner trägt, hinterlässt in `podcast_subscriptions.image_url` die
Avatar-URL — je einmal ausgehend von `NULL` und ausgehend von einem
`i.ytimg.com/vi/…`-Videobild. Der zweite Fall ist der Beleg dafür, dass die
sieben Altzeilen wirklich heilen.

**T5 — `add_dialog_subscription.rs:57`:** `subscribe` schreibt für
`PodcastKind::Youtube` `image_url: None` in die `NewSubscription`; für
`PodcastKind::Rss` bleibt alles wie bisher (dort ist das Vorschaubild
`itunes:image`, also echt). Die Vorschauzeile selbst behält ihr Bild
(Entscheidung 4) — also nur der Persistenzpfad ändert sich, nicht
`candidate_row`.

**T6 — `podcasts_groups.rs`:** `group_image_url` ersatzlos entfernen; die
Aufrufstelle (`:274`) liest `group.image_url` direkt. Danach gibt es im
GNOME-Frontend keinen YouTube-Sonderweg für Bilder mehr, und die Detailansicht
(`youtube_channel_detail.rs:490`) liest nachweislich dieselbe Quelle wie die
Kachel.

**T7 — `SCHEMA_V77` in `crates/reprise-core/src/db.rs`** (`SUPPORTED_SCHEMA_VERSION`
steht auf 76, `db.rs:26`). Die Migration räumt den Altbestand:

```sql
UPDATE podcast_subscriptions
   SET image_url = NULL,
       last_fetch_at = NULL
 WHERE kind = 'youtube'
   AND (image_url IS NULL OR image_url LIKE '%i.ytimg.com/vi/%');
```

Das Zurücksetzen von `last_fetch_at` macht die betroffenen Abos beim nächsten
ohnehin laufenden Refresh fällig (`refresh_due` behandelt `None` als fällig,
`refresh.rs:108-110`); das Leeren von `image_url` sorgt dafür, dass bis dahin das
Fallback-Icon steht statt eines falschen Bildes. Der `kind`-Wert muss dem
entsprechen, was `kind_setting` schreibt (`store.rs`) — nachsehen, nicht raten.
Migrationstest analog zu den vorhandenen `db_*_migration_tests.rs`: eine DB auf
Version 76 mit je einer YouTube-Zeile (Videobild / NULL / bereits Avatar) und
einer RSS-Zeile; nach der Migration sind Videobild und NULL geräumt und fällig,
Avatar und RSS-Zeile unangetastet.

**T8 — Migrationsbeleg an einer Kopie der echten Datenbank.** Kopie von
`~/.local/share/reprise/reprise.db` (mit `-wal`/`-shm`) in den Worktree-Scratch,
die neun Zeilen vorher protokollieren, Migration fahren, nachher protokollieren.
Erwartet: die sieben `i.ytimg.com`-Zeilen und die beiden NULL-Zeilen haben
`image_url IS NULL` und `last_fetch_at IS NULL`, die vier RSS-Zeilen sind
unverändert. Vorher/Nachher gehört in den Abschlussbericht.

## Nachweis, den der Fix erbringen muss

- Ein frisch angelegtes Kanal-Abo trägt nach dem ersten Refresh eine
  `yt3.googleusercontent.com/ytc/…`-URL, nicht `i.ytimg.com/vi/…` und nicht die
  Banner-URL.
- Ein Abo, das mit einem `i.ytimg.com`-Videobild startet, trägt nach einem
  Refresh die Avatar-URL.
- Das gespeicherte Bild ändert sich **nicht**, wenn der Kanal ein neues Video
  veröffentlicht.
- Ein Kanal ohne Avatar im Dump führt zu `None` — Icon, niemals Banner.
- Kachel und Detailansicht lesen dieselbe Quelle (nach T6 strukturell erzwungen).

Gates lokal, jeweils grün und im Bericht mit der Ergebniszeile belegt:
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test -p reprise-core`, `cargo test -p reprise-gnome`.
Die Display-Suite ist hier **kein** Pflichtbeleg (kein Layout- oder CSS-Eingriff);
was dadurch ungeprüft bleibt — die tatsächliche Darstellung der Kachel — nennt
der Bericht ausdrücklich.

## Abgrenzung

- Nur `PodcastKind::Youtube`. RSS holt sein Bild aus `itunes:image`
  (`feed.rs:203-211`) und ist an keiner Stufe beteiligt.
- `entry_image_url` und die Trefferliste der Kanalsuche bleiben, wie sie sind
  (Entscheidung 4). Nur ihr Weg in die Datenbank wird gekappt (T5).
- Nicht verwandt mit `youtube-channel-size-and-sorting.md`.
- `episode-covers-appear-seconds-after-start.md` (Bilder erscheinen verzögert)
  ist ein Timing-, kein Auswahlproblem und bleibt offen.
- Die Android-App hat keinen YouTube-Weg.

## Parallelität

**Kein Cut — ein Strang.** T1→T2→T3→T4 sind eine einzige Datenkette durch vier
Dateien desselben Crates; jede Stufe ist die Vorbedingung der nächsten, und
keine zwei sind ohne die andere grün zu bekommen. T6 muss T4 nachfolgen: zieht
man den Rückfall heraus, bevor das echte Kanalbild ankommt, zeigen alle Kacheln
bis zum nächsten Refresh gar nichts. T7/T8 hängen an T4, weil die Migration nur
dann etwas Sinnvolles bewirkt, wenn der Refresh danach einen Avatar liefert.
T5 wäre für sich genommen unabhängig, ist aber ein Dreizeiler — ein zweiter
Worktree kostet mehr Rüstzeit, als er an Wanduhr spart.

Keine post-merge-Cross-Checks, weil es keine Strandgrenze gibt.
