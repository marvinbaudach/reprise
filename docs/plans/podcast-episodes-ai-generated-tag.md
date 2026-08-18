---
slug: podcast-episodes-ai-generated-tag
worktree:
branch:
phase: dropped
codex_session:
created: 2026-08-16
---
# Verworfen: Episoden kennzeichnen, ob sie KI-generiert sind

> **Entscheidung vom 16.08.2026 — nicht umsetzen.** Der Nutzer wollte das Tag
> **aus YouTube ziehen**: *„ok wollte das tag aus yt ziehen. wenn das nicht
> geht können wir nichts machen."* Genau dieser Weg ist versperrt (§1), und
> ohne ihn stand kein Ersatz zur Debatte. Das Dokument bleibt als Beleg
> liegen, damit die Frage nicht in einem Jahr erneut untersucht wird —
> **der eine Punkt, der sich ändern kann, steht in §1.3.**

**Wunsch des Nutzers, kein Plan.** Festgehalten am 16.08.2026:
*„kannst du bei den episoden noch taggen ob sie KI generiert sind?"* — im
Kontext der YouTube-Kanäle aus der Podcast-Ansicht (Viking-Musik: Bjorth,
Danheim, Heldom).

**Kurzantwort: die Pille anzubringen ist trivial, sie wahrheitsgemäß zu füllen
ist es nicht.** Es gibt derzeit keine belastbare Quelle für die Aussage.

## Gemessen am 16.08.2026

### 1. YouTube gibt die Kennzeichnung nicht heraus — jedenfalls nicht über yt-dlp

Voller Metadatenabzug eines Videos aus genau diesen Kanälen
(`yt-dlp --no-warnings -J`, Bjorth `9WXsdApQIY4`):

```
total keys: 77
Felder mit synth|altered|ai_|_ai|artific|disclos|generat|label: []
```

YouTube fragt Hochladende seit 2024 nach „altered or synthetic content" ab und
zeigt den Hinweis in der Oberfläche — in den 77 Feldern, die yt-dlp liefert,
kommt davon nichts an. Es gibt also kein strukturiertes Signal, das die App
einfach durchreichen könnte.

**1.3 Der Einzeltest allein trüge diese Aussage nicht — der Extraktor tut es.**
Das geprüfte Video erklärt sich selbst als *nicht* KI-generiert. Bei so einem
Video ist ein fehlendes Offenlegungsfeld erwartbar und beweist nichts: es gäbe
schlicht nichts zu melden. Ohne Gegenprobe wäre der Befund wertlos. Die
Gegenprobe läuft deshalb nicht über ein zweites Video, sondern über den
Extraktor selbst — yt-dlp **2026.07.04**, vollständiger YouTube-Extraktor:

```
$ grep -ric "synthetic\|altered\|disclosure" \
    /usr/lib/python3.14/site-packages/yt_dlp/extractor/youtube/
(kein Treffer in keiner Datei)
```

Der Extraktor kennt das Konzept überhaupt nicht. Damit hängt der Negativbefund
nicht mehr an der Auswahl des Testvideos: yt-dlp gäbe die Kennzeichnung bei
**keinem** Video aus, auch bei einem gekennzeichneten nicht.

**Das ist zugleich die einzige Stelle, die sich ändern kann.** Wenn ein
künftiges yt-dlp die Offenlegung exportiert, wird aus diesem verworfenen
Dokument ein kleines Arbeitspaket: Spalte, Projektion, Pille. Ein Grep nach
denselben drei Wörtern im Extraktor beantwortet das in einer Sekunde.

### 2. Die naheliegende Textheuristik produziert sofort ein falsches Etikett

Dasselbe Video enthält in der Beschreibung die Zeichenkette `AI generated`.
Im Satz steht sie so:

> „This is real music, **not AI generated** !
> Composed and Produced by Bjoern Raymond Hoppen"

Eine Suche nach „AI generated" hätte damit ausgerechnet den Kanal aus dem
Screenshot als KI-Produktion markiert, der ausdrücklich das Gegenteil erklärt —
und das Etikett wäre eine öffentliche Behauptung über einen namentlich
genannten Komponisten und vier namentlich genannte Sängerinnen und Sänger.
Das ist kein Randfall, den man mit einer Negationsregel wegpatcht: der erste
untersuchte Treffer war bereits der Fehlschlag.

### 3. Die Datenquelle liegt lokal gar nicht vor

`podcast_episodes` hat **keine** Beschreibungsspalte
(`crates/reprise-core/src/db_podcasts_radio.rs:22-42` plus alle späteren
`ALTER TABLE`-Migrationen — dazugekommen sind `removed_at`,
`downloaded_bytes`, `image_url`, `media_category`, nie eine Beschreibung).
Und die Kanallistung fährt `--flat-playlist`
(`crates/reprise-core/src/podcasts/ytdlp.rs:150-160`), die Beschreibungen
ebenso wenig liefert wie die Abonnentenzahl aus
`docs/plans/youtube-channel-size-and-sorting.md`. Für jede Textheuristik
müsste also erst pro Episode ein voller Metadatenabruf gefahren und eine neue
Spalte migriert werden — Aufwand, der auf einem Signal aufsetzt, das nach
Punkt 2 nicht trägt.

## Was gangbar gewesen wäre — nicht verfolgt

*Die folgenden drei Wege standen zur Wahl. Der Nutzer wollte ausdrücklich das
Tag aus YouTube und hat die manuellen Varianten damit abgelehnt; sie stehen
hier nur, damit eine spätere Runde sie nicht neu erfinden muss.*

1. **Manuell, vom Nutzer gesetzt.** Ein Tag, das *du* pro Kanal oder pro
   Episode vergibst. Ehrlich, offline, ohne Fehlurteil — die App behauptet
   nichts, sie merkt sich deine Einordnung. Anknüpfpunkt für die Spalte ist
   dieselbe Anreicherungsstelle wie `media_category`
   (`crates/reprise-core/src/podcasts/store_metadata.rs:11-28`).
2. **Auf Kanalebene statt pro Episode.** „Dieser Kanal produziert mit KI" ist
   eine Aussage, die man einmal trifft; pro Episode ist sie fast nie
   unterschiedlich. Das reduziert den Pflegeaufwand auf einen Bruchteil.
3. **Automatisch — nur mit einem Signal, das es heute nicht gibt.** Falls
   yt-dlp die YouTube-Offenlegung künftig exportiert, ist der Rest klein:
   Spalte, Projektion, Pille. Das gehört als Beobachtungspunkt notiert, nicht
   als Arbeitspaket.

## Wo die Pille hinginge

Der Anzeigeteil ist der einfache Teil und schon vorgezeichnet:

| Datei | Rolle |
| --- | --- |
| `crates/reprise-gnome/src/ui/podcasts/podcasts_presentation.rs:267-301` | `source_pill()` / `status_pill()` — die bestehenden Pillen einer Episodenzeile |
| `crates/reprise-gnome/src/ui/podcasts/css.rs:35-42` | Pillen-Stile, inkl. der Rahmenvariante |
| `crates/reprise-core/src/podcasts/store_metadata.rs:11-28` | Anreicherung pro Episode (`media_category`) — Vorbild für eine neue Spalte |
| `crates/reprise-core/src/db_podcasts_radio.rs` | Schema und Migrationen der Episodentabelle |

## Was offen bliebe, falls die Quelle je auftaucht

1. **Was heißt „KI-generiert"?** Komplett generierte Musik, KI-Vocals über
   echter Komposition, nur ein KI-Titelbild? YouTubes Offenlegung ist eine
   Selbstauskunft der Hochladenden und trennt das nicht — die Pille erbt
   also deren Unschärfe und darf nicht mehr behaupten, als dort steht.
2. **Ein Tag, das nur die Ausnahme markiert.** `docs/plans/feed-tags-mark-the-exception.md`
   hat für die Concerts-Pillen genau diese Frage schon entschieden: eine Pille
   trägt nur, wer von der Erwartung abweicht. Dasselbe Prinzip hier hieße —
   nur KI-Produktionen bekommen ein Zeichen, alles andere bleibt nackt.
