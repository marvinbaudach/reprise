---
slug: playback-errors-report-the-first-cause
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-18
---
# Plan: Die Wiedergabe meldet die erste Ursache, nicht die letzte Folge

Herausgelöst aus dem Befund `youtube-streaming-internal-data-stream-error.md` am
18.08.2026. Dort wurde gemessen, dass die Oberfläche die **dritte** Meldung
einer Fehlerkette zeigt. Dieser Plan behebt nur das; der Transportfehler selbst
hat seinen eigenen Plan und seinen eigenen Branch.

**Für Welle 2 vorgesehen**, nicht Welle 1: unabhängig wertvoll, aber niemand
wartet darauf.

## Der Befund

Journal des Laufs vom 16.08.2026, 08:00:55, PID 12694 — drei Busfehler in
40 Millisekunden, jeder überschreibt die Anzeige des vorigen:

```
06:00:55.028705 ERROR player_pipeline: error=Forbidden
                       … Forbidden (403), URL: https://…googlevideo.com/…
06:00:55.061550 ERROR player_pipeline: error=Internal data stream error.
                       … streaming stopped, reason error (-5)
06:00:55.061594 ERROR player_pipeline: error=Stream doesn't contain enough data.
                       … Can't typefind stream
```

Der Nutzer sieht die letzte. Sie benennt nichts. Die erste — `Forbidden (403)` —
war die Wahrheit und hätte die Diagnose um zwei Tage verkürzt.

Der Wert bleibt auch nach dem Transportfix: der nächste Serverwechsel kommt
wieder als Statuscode an, und dann entscheidet sich an dieser Stelle, ob er
sichtbar ist oder unter Folgerauschen verschwindet.

## Aufgaben

1. **Die erste Busfehlermeldung einer Wiedergabesitzung gewinnt.** Nach dem
   ersten Fehler werden weitere Meldungen derselben Sitzung protokolliert, aber
   nicht mehr angezeigt. Der Zähler wird beim Start der nächsten Sitzung
   zurückgesetzt. Betrifft
   `crates/reprise-gnome/src/ui/playback/player_event_handling.rs` und
   `crates/reprise-platform-linux/src/player_pipeline.rs`.
2. **HTTP-Status typisieren.** Der Statuscode steht wörtlich im
   GStreamer-Debugtext (`Forbidden (403), URL: …`). Ihn dort herausziehen und in
   eine typisierte Fehlerart überführen. **Kein Substring-Matching auf der
   Anzeigemeldung** — `podcasts.rs:375` hält als Test fest, dass die Projektion
   über typisierte Arten läuft und nicht über Textstücke
   (`yt_dlp_projection_uses_the_typed_kind_not_message_substrings`). Dieselbe
   Regel gilt hier.
3. **Eigene, handlungsfähige Meldung für 403 auf einer aufgelösten
   YouTube-URL.** Vorbild ist `YOUTUBE_BROWSER_RECOVERY_MESSAGE` in
   `reprise-core/src/podcasts.rs`.
4. **„Unavailable now" bleibt Episodenfehlern vorbehalten.** Entschieden im
   Grilling am 18.08.2026: Ein Transportfehler ist eine Aussage über die
   Verbindung, nicht über die Episode. Heute markiert jeder Wiedergabefehler die
   Zeile über `context.unavailable_episode`
   (`podcasts_groups.rs:233` → `podcasts_download_presentation.rs:45-52` →
   `strings_podcasts.rs:156`). Künftig nur noch bei 404/410 und
   Extraktorfehlern; bei 403 und Transportfehlern behält die Zeile ihren
   Download-Button und bleibt bedienbar. Beim aktuellen Fehler wären sonst
   reihum alle Episoden als kaputt markiert worden, obwohl keine es war.

## Tests

1. Eine Busfehlerkette `403 → Internal data stream error → Stream doesn't
   contain enough data` erzeugt genau **eine** Anzeige, und zwar die des 403.
2. Die Kette in umgekehrter Reihenfolge erzeugt entsprechend die erste davon —
   die Regel ist „erste gewinnt", nicht „403 gewinnt".
3. Eine neue Wiedergabesitzung zeigt wieder einen Fehler an; die Sperre ist
   sitzungsgebunden und nicht dauerhaft.
4. Ein 403 setzt `unavailable_episode` **nicht**; ein 404 setzt es.
5. Die Statusermittlung läuft über die typisierte Art: ein Debugtext, der das
   Wort „Forbidden" in anderer Bedeutung enthält, ohne Statuscode, wird nicht
   als 403 gewertet.

## Nachweis

1. Mit künstlich verfälschter Stream-URL erscheint die 403-Meldung, nicht
   „Internal data stream error".
2. Die Zeile behält dabei ihren Download-Button.
3. Journal: die Folgefehler stehen weiterhin im Log — unterdrückt wird die
   Anzeige, nicht die Aufzeichnung.

## Parallelität

**Nicht teilbar.** Vier Aufgaben auf denselben zwei Fehlerpfaden; die
Typisierung aus Aufgabe 2 ist Voraussetzung für 3 und 4.

## Verhältnis zum Transportplan

Unabhängig baubar und unabhängig landbar. Läuft
`youtube-streaming-internal-data-stream-error` zuerst, ist die
Nachweis-Verfälschung aus Punkt 1 durch den Proxy zu führen statt direkt — das
ist der einzige Berührungspunkt, und er betrifft die Abnahme, nicht den Code.
