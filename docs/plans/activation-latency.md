# Ein Track soll sofort spielen

Das Abspielen ist das Hauptfeature dieser Anwendung. Ein Doppelklick auf eine
Zeile ist der direkteste Ausdruck von „ich will das jetzt hören" — er muss sich
sofort anfühlen, nicht nur schneller als heute.

## Was gemessen wurde (2026-08-08)

Release-Build, isolierte Instanz auf einer Kopie der echten Bibliothek
(2340 Tracks, 241 MB DB), Xvfb, eigener D-Bus, eigener PulseAudio-Null-Sink.
Klickzeitpunkt per `xdotool`, Zeitachse aus den `tracing`-Logs, Hauptthread per
`eu-stack` gesampelt.

| Messgröße | Wert |
| --- | --- |
| `activate track` → `playback started` (Doppelklick) | **Median 92 ms** (66–150), 14 Läufe |
| Klick → `playback started` (Weiter-Knopf) | 34–65 ms, 3 Läufe |
| ID-Abfrage der ganzen Ansicht, kalt / warm | **66 ms / 2–4 ms** |
| Musikdatei lesen (35–50 MB FLAC), kalt / warm | 24 ms / 16 ms |

Aus dem Journal der echten Nutzersitzung, dasselbe Muster: der erste Doppelklick
nach dem Start kostet 216 ms, danach fallend über 118, 81, 51 auf 37 ms — die
Signatur eines Datenbank-Caches, der warmläuft.

**Zwei Verdächtige wurden ausgeschlossen, bevor Arbeit hineinfloss:**

- *Datei-I/O.* Eine 50-MB-FLAC liest sich kalt in 24 ms, warm in 16 ms. Acht
  Millisekunden Unterschied können keine wahrnehmbare Verzögerung erklären.
- *Vorpuffern.* Es lag nahe, dass der Weiter-Knopf nur deshalb schneller ist,
  weil er den gapless vorbereiteten Stream übernimmt. Tut er nicht: nur
  `advance_gaplessly` (der automatische Übergang am Trackende) nutzt
  `StartPlayback::No`; der manuelle Weiter-Knopf geht über `advance_playback`
  mit `StartPlayback::Yes` und baut die Pipeline genauso neu auf. Der Vergleich
  der beiden Wege ist damit fair.

## Wo die Zeit hingeht

### B1 — Der Seitenleisten-Rebuild läuft bei jedem Trackwechsel

Das ist der Befund mit der breitesten Wirkung: er betrifft **jeden Song**, nicht
nur den Doppelklick.

```
play_from_view                      (Doppelklick)
 → queue.set_tracks(ids, start_index)
 → play_track_id(id)                ← hier wird "playback started" geloggt
 → notify_queue_changed()
     → queue_changed-Callbacks (window.rs:281)
         → sidebar_rebuild::rebuild             19 synchrone Abfragen
             → count_releases_view
                 → query_complete_history_in
                     → artist_news_query::local_library_index
                        ← Index über die ganze Bibliothek, für eine Zahl
     → feed_next()
```

`advance_common` (Weiter-Knopf) und der automatische Übergang rufen
`notify_queue_changed` ebenfalls. Alle drei Wege zahlen diesen Preis. Im
Stack-Sampling erschien `sidebar_rebuild` → `count_releases_view` →
`artist_news` in 2 von 25 arbeitenden Samples.

Keiner dieser Zähler ändert sich dadurch, dass ein Track zu spielen beginnt.

### B2 — Die volle ID-Abfrage bei jeder Aktivierung

`track_list_activation::queue_ids_for_activation` holt über
`queries::query_track_ids_browsed` die vollständige, sortierte, gefilterte
ID-Liste der Ansicht — bei **jedem** Doppelklick neu. Direkt gemessen: 66 ms
kalt, 2–4 ms warm. Das ist der Anteil, den der Weiter-Knopf nicht hat, und er
erklärt das Warmlauf-Muster oben.

Die Liste hängt nur von Quelle, Sortierung, Filter und Browse-Facetten ab. Zwei
Doppelklicks in derselben Ansicht liefern dieselbe Liste; nur der Startindex
unterscheidet sich.

### B3 — Die Reihenfolge im Klickpfad

`play_from_view` setzt erst die vollständige Warteschlange, startet dann die
Wiedergabe, und aktualisiert danach die Zähler. Der angeklickte Track ist aber
von der ersten Zeile an bekannt. Die Warteschlange wird frühestens am Ende des
laufenden Tracks gebraucht, die Zähler nie dringend.

### Offen: die Aufteilung der 92 ms

Bei warmem Cache kostet B2 nur 2–4 ms, die Spanne `activate → started` bleibt
aber bei ~92 ms im Median. Der Rest ist **nicht zugeordnet** — `eu-stack`
braucht ~290 ms pro Sample und trifft ein 92-ms-Fenster nur zufällig; über 14
Aktivierungen fielen 2 Samples hinein.

**Das ist bewusst offen gelassen und muss beim Implementieren geklärt werden.**
Kandidaten im Pfad: `play_origin::resolve` (lädt für Playlist-/Smart-Quellen die
Playlisten), `queue.set_tracks` (kopiert bis zu `QUEUE_LIMIT` IDs), und
`play_track_id` selbst. Wer hier ohne Messung optimiert, optimiert womöglich die
falsche Stelle — genau das ist bei der Löschsache beinahe passiert, wo der
vermutete Kostenträger (die Abfrage) sich als billig erwies und der echte
(ein X11-Roundtrip pro Tooltip) erst im Sampling auftauchte.

## Aufgaben

Nach **jeder** Aufgabe messen, mit Gegenprobe bei deaktivierter Änderung.

### T0 — Erst die 92 ms aufteilen

Bevor irgendetwas geändert wird: den Abschnitt `activate track` →
`playback started` instrumentieren (temporäre `tracing`-Spans oder Zeitstempel
genügen) und die Anteile von `queue_ids_for_activation`, `play_origin::resolve`,
`queue.set_tracks` und `play_track_id` einzeln ausweisen. Ergebnis in diesen
Plan schreiben.

Die folgenden Aufgaben sind nach heutigem Wissen priorisiert; wenn T0 ein
anderes Bild ergibt, hat T0 recht und die Reihenfolge wird angepasst. Das
ausdrücklich im Ergebnis vermerken, nicht stillschweigend umsortieren.

### T1 — Die Zähler nicht bei jedem Trackwechsel neu berechnen

Ein Trackwechsel ändert keinen einzigen Seitenleisten-Zähler. Der Rebuild aus
`notify_queue_changed` heraus ist damit reine Verschwendung — dreimal pro Song
(Start, Wechsel, automatischer Übergang).

Zu klären ist, was der Queue-Callback wirklich braucht: vermutlich nur die
Queue-Länge, nicht die 19 Abfragen für Musik, Missing, Library Doctor,
Playlisten, Podcasts, YouTube, Radio, Releases und Concerts.

**Falle, die zu prüfen ist:** die Zähler dürfen nicht veralten. Jeder Weg, der
eine Zahl tatsächlich ändert, braucht weiterhin seinen eigenen Refresh — das ist
dieselbe Falle wie beim Watcher-Gate im Löschplan, und dort hat sich gezeigt,
dass die anderen Aufrufstellen ihn schon haben. Nachweisen, nicht annehmen. Ein
Test soll es festhalten.

**Zusätzlich:** `count_releases_view` baut über `local_library_index` einen Index
über die ganze Bibliothek auf, nur um eine Zahl zu bilden. Selbst wenn der
Rebuild seltener wird, gehört das nicht synchron in den UI-Thread.

### T2 — Die ID-Liste der Ansicht nicht bei jedem Doppelklick neu abfragen

Die Liste ist eine reine Funktion aus Quelle, Sortierung, Filter und
Browse-Facetten. Sie kann behalten werden, solange sich davon nichts ändert; ein
zweiter Doppelklick in derselben Ansicht braucht dann nur noch den Startindex.

**Fallen:** Die Bibliothek verändert sich unter der Ansicht (Scan, Löschen,
Tag-Änderung, Watcher). Eine behaltene Liste, die auf gelöschte oder verschobene
Tracks zeigt, ist schlimmer als eine langsame Abfrage — sie spielt dann das
Falsche oder nichts. Die vorhandene `TrackListModel::generation` ist genau dafür
da, dass etwas erkennt, wann das Modell sich geändert hat; sie ist der
naheliegende Schlüssel. `QUEUE_LIMIT` und die Kappungswarnung müssen erhalten
bleiben.

### T3 — Ton zuerst, Rest danach

Der angeklickte Track ist sofort bekannt. Die Wiedergabe kann starten, bevor
Warteschlange und Zähler stehen — beides wird erst später gebraucht.

**Fallen:** `feed_next` (das gapless Vorbereiten des nächsten Tracks) braucht die
fertige Warteschlange; es darf nicht auf eine halb gefüllte treffen. Ein
Doppelklick, der schnell auf einen zweiten folgt, darf die Reihenfolge nicht
verdrehen — der zuletzt angeklickte Track gewinnt. Und die Warteschlange muss
stehen, bevor der laufende Track endet, sonst bricht die Wiedergabe am
Trackende ab statt weiterzulaufen.

Diese Aufgabe erst angehen, wenn T0 zeigt, dass sich damit noch etwas holen
lässt — nach T1 und T2 könnte der Pfad bereits kurz genug sein.

## Verifikation

- Nur per Timer messen. Frame-Sampling liefert hier null Samples und sieht dann
  fälschlich grün aus.
- Zu jeder Messung die Gegenprobe mit deaktivierter Änderung.
- Der Endpunkt, der zählt, ist der **Toneinsatz**, nicht eine Logzeile. Ein
  PulseAudio-Null-Sink plus `parec` mit RMS in 5-ms-Blöcken misst ihn.
  Achtung: unter Systemlast (parallele Builds) verfälscht der Audio-Stack diese
  Messung erheblich — Lastzustand mit protokollieren.

Zielwerte gegen die Ausgangslage:

- Doppelklick `activate track` → `playback started`: deutlich unter 92 ms Median
- Der Abstand zum Weiter-Knopf (34–65 ms) soll weitgehend verschwinden
- Kein Seitenleisten-Rebuild mehr im Trackwechsel-Pfad
- Seitenleisten-Zähler bleiben in jedem Fall korrekt (die Falle aus T1)
- Wiedergabe läuft am Trackende normal weiter (die Falle aus T3)

## Nicht Teil dieser Arbeit

- Datei-I/O (gemessen: 24 ms kalt, keine relevante Größe).
- Das gapless Vorpuffern selbst — es funktioniert und ist nicht die Ursache.
