---
slug: youtube-channel-tile-shows-an-episode-thumbnail
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-18
---
# TODO: Der YouTube-Kanal zeigt ein Episodencover statt des Kanalbildes

**Befund mit vollständig belegter Ursache, kein Plan.** Gemeldet am 18.08.2026:
*„und der yt kanal sollte das kanalbild nutzen und nicht ein episodencover"*.

Die Ursache ist am Code und an der yt-dlp-Antwort nachgemessen (18.08.2026), und
sie ist **nicht** die naheliegende: das Kanalbild fehlt nicht, weil YouTube es
nicht liefert. Es geht auf drei Stufen verloren, und die dritte Stufe ist die
teure — sie kippt still ein Banner in eine quadratische Kachel, sobald man die
ersten beiden repariert.

## Was der Nutzer sieht

Die Kachel eines abonnierten Kanals in der Podcasts-Liste trägt das Vorschaubild
des **neuesten Videos**. Es wechselt entsprechend, sobald der Kanal etwas
Neues veröffentlicht.

Die **Detailansicht desselben Kanals** zeigt dagegen gar kein Bild, nur das
Rückfall-Icon `video-x-generic-symbolic`
(`crates/reprise-gnome/src/ui/podcasts/youtube_channel_detail.rs:490`, `:497` —
sie liest `rendered.group.image_url` direkt, ohne den Episoden-Rückfall). Kachel
und Detailansicht widersprechen einander also; beide sind falsch, auf zwei
verschiedene Arten.

## Die Kette, Stufe für Stufe

**Stufe 1 — yt-dlp liefert das Kanalbild, die App wirft es weg.**

Gemessen mit demselben `--flat-playlist`-Modus, den die App fährt
(`crates/reprise-core/src/podcasts/ytdlp.rs:154`):

```sh
yt-dlp --no-warnings --flat-playlist --playlist-items 1 -J \
  "https://www.youtube.com/@Kurzgesagt/videos"
```

Der Kanal-Dump trägt **neun** `thumbnails`-Einträge auf oberster Ebene: sechs
Banner-Zuschnitte (1060×175 bis 2560×424), `banner_uncropped`, ein
quadratisches Avatar 900×900 (`id: "7"`) und `avatar_uncropped`. Ein flaches
`thumbnail`-Feld gibt es auf Kanalebene **nicht** (geprüft: `"thumbnail" in d`
→ `False`).

`YtDlpPlaylist` hat sehr wohl ein `image_url` (`ytdlp.rs:77`) und es wird beim
Parsen befüllt (`ytdlp.rs:746`). Verloren geht es erst eine Stufe später:
`YoutubeListing` (`crates/reprise-core/src/podcasts/youtube.rs:14-18`) hat
**kein** Bildfeld, also lässt `project_playlist` (`youtube.rs:33`) den Wert
fallen.

**Stufe 2 — die Projektion setzt das Feld hart auf `None`.**

`project_youtube_feed` (`crates/reprise-core/src/podcasts/pipeline.rs:156`)
schreibt `image_url: None` als Konstante in den `ParsedFeed`. Selbst wenn
Stufe 1 den Wert durchreichte, käme er hier nicht heraus.

Folge in der Ablage: `store.rs:82` schreibt
`image_url = COALESCE(excluded.image_url, podcast_subscriptions.image_url)`.
Da beim Upsert immer `None` ankommt und nie ein früherer Wert existierte, bleibt
die Spalte für YouTube-Abos **dauerhaft NULL**. Kein Aktualisierungslauf kann
das von selbst heilen.

**Stufe 3 — die GUI maskiert die Lücke mit dem Episodencover.**

`group_image_url`
(`crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs:337-345`):

```rust
group.image_url.as_deref().or_else(|| match group.kind {
    PodcastKind::Rss => None,
    PodcastKind::Youtube => group.episodes.first()
        .and_then(|episode| episode.image_url.as_deref()),
})
```

`episodes_for_subscription_in` sortiert `published_at DESC`
(`crates/reprise-core/src/podcasts/query.rs:56`), `first()` ist also das
**neueste** Video. Weil `group.image_url` nach Stufe 2 immer `None` ist, greift
dieser Zweig **immer** — das ist genau das gemeldete Verhalten. Für
`PodcastKind::Rss` ist derselbe Zweig `None`: RSS-Podcasts bekommen ihr Bild aus
`itunes:image` und fallen nie auf eine Episode zurück. Der Rückfall ist ein
reiner YouTube-Sonderweg.

## Die Falle, die den Fix teuer macht

`entry_image_url` (`crates/reprise-core/src/podcasts/ytdlp_search.rs:113-120`)
nimmt `thumbnail`, sonst den **ersten** Eintrag aus `thumbnails`. Auf
Videoebene ist das richtig. Auf **Kanalebene** ist `thumbnails[0]` das
Banner 1060×175 — ein Streifen im Verhältnis 6:1. Wer nur die Stufen 1 und 2
repariert, tauscht das Episodencover gegen ein Banner, das in einer
quadratischen Kachel entweder zu einem Ausschnitt in der Bannermitte
zusammengeschnitten oder zu einem Balken gequetscht wird — und das sieht
schlimmer aus als der jetzige Zustand, weil Bannermitte selten das Logo trägt.

Gewollt ist der **Avatar**: `id == "avatar_uncropped"`, ersatzweise der
quadratische Eintrag (`width == height`, hier 900×900). Die Auswahlregel für
Kanalbilder muss also eine andere sein als die für Videobilder —
`entry_image_url` darf dafür nicht einfach wiederverwendet werden.

## Was ein Plan entscheiden muss

1. **Auswahlregel für das Kanalbild.** `avatar_uncropped` zuerst, dann der
   größte quadratische Eintrag, dann nichts? Und was passiert bei einem Kanal
   ohne Avatar — kein Bild, oder doch das Banner?
2. **Bleibt der Episoden-Rückfall in `group_image_url`?** Wenn das Kanalbild ab
   Stufe 2 echt ankommt, ist er nur noch für Altbestände nötig. Er versteckt
   aber jeden künftigen Rückschritt derselben Art — der Nutzer sieht *irgendein*
   Bild und meldet nichts. Argument, ihn ersatzlos zu streichen.
3. **Altbestand.** Bestehende Abos haben `image_url = NULL` in der DB, und
   `COALESCE` heilt nur, wenn der neue Wert nicht `NULL` ist — das ist nach dem
   Fix erfüllt, aber erst beim nächsten Aktualisierungslauf des Abos. Reicht
   das, oder braucht es einen einmaligen Nachziehlauf?
4. **Die Detailansicht** (`youtube_channel_detail.rs:490`) wird vom selben Fix
   mitgeheilt. Nachweisen, nicht annehmen.

## Abgrenzung

- Betrifft nur `PodcastKind::Youtube`. RSS-Podcasts sind an keiner der drei
  Stufen beteiligt.
- Nicht verwandt mit `youtube-channel-size-and-sorting.md` (Größe und
  Sortierung der Kacheln, nicht ihr Inhalt).
- Die Android-App hat keinen YouTube-Weg und ist nicht betroffen.

## Nachweis, den ein Fix erbringen muss

- Ein frisch angelegtes Kanal-Abo trägt in `podcast_subscriptions.image_url`
  eine `yt3.googleusercontent.com`-URL, und zwar die **quadratische**.
- Kachel und Detailansicht desselben Kanals zeigen dasselbe Bild.
- Das Bild ändert sich **nicht**, wenn der Kanal ein neues Video veröffentlicht
  — das ist der eigentliche Regressionstest gegen Stufe 3.
