---
slug: episode-covers-appear-seconds-after-start
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-18
---
# TODO: Episoden-Cover erscheinen erst Sekunden nach dem App-Start

**Befund, kein Plan.** Gemeldet am 18.08.2026: *„wenn ich reprise starte sehe
ich bei den episoden erstmal kein cover. das kommt erst nach ein paar
sekunden."* Belegt durch
[assets/episode-covers-appear-seconds-after-start.png](assets/episode-covers-appear-seconds-after-start.png).

## Symptom

Direkt nach dem Start zeigen die Kanal- und Episodenzeilen der Podcast-/
YouTube-Ansicht das graue Kamera-Fallback-Icon; auch die Warteschlange rechts
ist voll davon. Erst nach einigen Sekunden füllen sich die Zellen mit den
echten Bildern. Die Bilder sind lokal vorhanden — es ist kein Download,
sondern eine Anzeigeverzögerung. Zwei Kanäle (`Heldom`, `HOLLOW FALLEN`) haben
ihr Bild im Screenshot schon, der Rest noch nicht: die Verzögerung staffelt
sich, passt also zu einer Warteschlange, nicht zu einem einzelnen Startsignal.

## Verdachtsspur (noch nicht bewiesen)

Fundstellen auf dem lokalen Stand, vor dem Anpacken gegen `origin/dev`
gegenprüfen:

| Sache | Stelle |
| --- | --- |
| Speicher-Cache-Treffer, der synchron zeichnen würde | `ui/podcasts/source_image.rs:364` (`cached_texture`) |
| jeder Fehltreffer geht in die Worker-Queue | `source_image.rs:380` (`source_artwork_queue::queue`) |
| Startarbeit hängt zusätzlich am Quiet-Gate | `source_image.rs:400-403` (`StartupTiming::AfterQuiet`) |
| Zeilen registrieren sich als `AfterQuiet` | `ui/podcasts/podcasts_row_interaction.rs:25`, `ui/podcasts/source_image_fallback.rs:83` |
| acht Worker teilen sich Disk-Lesen + Dekodieren + Skalieren | `ui/podcasts/source_artwork_queue.rs:10` (`ARTWORK_WORKERS = 8`) |
| Quiet-Gate: erster gemalter Frame + 100 ms, Priority::LOW | `ui/startup_quiet.rs:14`, `:104-119` |

Der Texturspeicher überlebt den Prozess nicht. Beim Start ist er also leer,
und **jede** sichtbare Zelle muss den vollen Weg gehen: Quiet-Gate abwarten →
Queue → Datei lesen → dekodieren → skalieren → zurück auf den GTK-Thread. Das
Gate selbst erklärt keine Sekunden (100 ms), die Sammelarbeit über acht Worker
im Wettbewerb mit dem Startscan schon eher. Zu prüfen ist außerdem, ob die
sichtbaren Zeilen überhaupt Vorrang vor den nicht sichtbaren bekommen.

## Fragen fürs Planen

1. Warum liegt die Verzögerung im Sekundenbereich, obwohl die Bilder auf der
   Platte liegen? Erst messen (Zeit von `queue()` bis `on_ready` je Zeile),
   dann bauen.
2. Lohnt ein sitzungsübergreifender Cache der bereits skalierten Pixel, oder
   reicht Priorisierung der sichtbaren Zeilen?
3. Kollidiert der Bildpfad beim Start mit dem Bibliotheks-/Cover-Scan um
   dieselben Kerne? Siehe auch
   [artwork-load-path-preexisting-weaknesses.md](artwork-load-path-preexisting-weaknesses.md).

Verwandt: [compact-player-misses-external-artwork.md](compact-player-misses-external-artwork.md)
(dasselbe Bildmaterial, andere Oberfläche).
