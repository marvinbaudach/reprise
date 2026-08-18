---
slug: youtube-channel-size-and-sorting
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-16
---
# TODO: „Add Channel" soll die Größe des Kanals zeigen — und danach sortieren können

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„hier wäre ne info wie groß der kanal ist noch spannend. vllt auch eine
Sortierung danach"* — belegt durch einen Screenshot des Dialogs **Add Channel**
mit der Suche `viking music`. Alle vier Treffer (Bjorth, Nordheim Melodies,
Draugr Balled, NordicPulse) tragen nur `N matching video(s) · audio only`.

## Ist-Zustand: die Anzeige ist gebaut, sie bekommt nur nie Daten

Die Abonnentenzahl ist als `SRC-9` bereits vollständig verdrahtet:

- `crates/reprise-core/src/podcasts/ytdlp_search.rs:17` — `YtDlpChannel.follower_count: Option<u64>`
- `ytdlp_search.rs:94-98` — liest `channel_follower_count`, macht aus Fehlendem
  bewusst nie eine Null
- `crates/reprise-gnome/src/ui/podcasts/add_dialog_results.rs:20-29` —
  `youtube_subtitle()` hängt die Zahl an, **wenn** sie da ist
- `crates/reprise-gnome/src/ui/strings_podcasts.rs:206,564-566` — `{count} subscribers`

Der Untertitel im Screenshot bleibt trotzdem nackt, weil die Suche das Feld gar
nicht anfordert. Gemessen am 16.08.2026 gegen das echte yt-dlp:

```
$ yt-dlp --no-warnings --flat-playlist -J 'ytsearch5:viking music'
entries 5
has_follower 0
kanalnahe Felder pro Eintrag: channel, channel_id, channel_is_verified,
                              channel_url, view_count
```

`--flat-playlist` ist genau die Sparflamme, die `channel_follower_count`
weglässt — und `search_channels()` fährt sie
(`crates/reprise-core/src/podcasts/ytdlp.rs:178-190`, `ytsearch20:`).
Ergebnis: `follower_count` ist im Dialog immer `None`, der `SRC-9`-Zweig ist in
der Praxis toter Code. Nur `reprise-mcp` (`discovery_actions.rs:182`) reicht das
Feld ebenfalls durch und ist genauso blind.

## Was ein zweiter Aufruf kostet — gemessen, nicht geschätzt

Ein Metadatenabruf pro Kanal ohne Einträge liefert beides, Abonnenten **und**
Videoanzahl, in unter einer Sekunde:

```
$ time yt-dlp --no-warnings --flat-playlist -I 0 -J \
    https://www.youtube.com/channel/UClDzr-KM5H2-bsO3xIC32mg
0,66 s real
{'channel': 'Bjorth', 'channel_follower_count': 73300, 'playlist_count': 2}
```

Das ist derselbe Bjorth aus dem Screenshot. `-I 0` unterdrückt die Einträge, die
Antwort ist nur der Playlist-Kopf. Bei den ~4–8 Kanälen, die `ytsearch20:` nach
der Zusammenfassung übrig lässt, sind das parallel wenige hundert Millisekunden
Zusatzlast — aber es sind N zusätzliche Netzabrufe, die unter `NET-1a`
(Einwilligung) und die bestehenden yt-dlp-Zeitlimits gehören.

## Zu klären, bevor daraus ein Plan wird

1. ~~**Welche Größe meint der Nutzer?**~~ **Beantwortet am 16.08.2026:
   „ja abozahlen"** — gemeint ist `channel_follower_count`, also genau das
   Feld, das `SRC-9` schon vorsieht. `playlist_count` fällt beim selben Aufruf
   mit ab, ist aber nicht das Gewünschte; es bliebe optionale Zugabe und darf
   den Untertitel nicht überladen, in dem heute schon
   `N matching videos · audio only` steht.
2. **Anreichern oder billig schätzen?** Alternative ohne Zweitaufruf: die
   `view_count`-Werte der Treffer-Videos, die die flache Suche schon liefert.
   Das misst aber die Reichweite *dieser Videos*, nicht die des Kanals — als
   „Größe" wäre das eine stillschweigende Ersetzung, gegen die `SRC-9` bewusst
   angeschrieben ist („never a substituted zero"). Mit der Antwort aus Punkt 1
   ist das entschieden: **die Abonnentenzahl gibt es nur über den Zweitaufruf**,
   die flache Suche liefert sie nachweislich nie.
3. **Sortierung wonach und wo?** Heute ist die Reihenfolge die Relevanzordnung
   von yt-dlp, unsortiert durchgereicht (`ytdlp_search.rs` sortiert nichts,
   `add_dialog_results.rs:159` projiziert nur). Eine Sortierung nach Größe
   braucht eine Entscheidung: fester Zweitschlüssel, Sortier-Chip im Dialog,
   oder Spaltenkopf. Und sie braucht eine Antwort für Kanäle **ohne** Zahl —
   die dürfen nicht als „0 Abonnenten" ans Ende rutschen.
4. **Teilweise Daten sind der Normalfall.** Kanäle können die Abonnentenzahl
   verbergen. Eine Sortierung, die dann die Hälfte der Treffer verliert oder
   ans Ende schiebt, ist schlechter als keine.

## Berührte Stellen

| Datei | Rolle |
| --- | --- |
| `crates/reprise-core/src/podcasts/ytdlp.rs:178-190` | `search_channels()` — hier müsste die Anreicherung ansetzen |
| `crates/reprise-core/src/podcasts/ytdlp_search.rs` | `YtDlpChannel`, Zusammenfassung der Videos zu Kanälen |
| `crates/reprise-gnome/src/ui/podcasts/add_dialog_results.rs:20-29,159` | Untertitel und Projektion |
| `crates/reprise-gnome/src/ui/strings_podcasts.rs:206,564-566` | `{count} subscribers` |
| `crates/reprise-mcp/src/discovery_actions.rs:182` | reicht `subscriber_count` weiter, heute immer leer |
