---
slug: artwork-load-path-preexisting-weaknesses
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-13
---

# Vorbestehende Schwächen im Bild-Ladepfad

Drei Befunde aus der Review von `feature/source-artwork-never-reloads`
(13.08.2026). Alle drei sind **älter als dieser Branch** und wurden dort bewusst
nicht angefasst, damit der PR am Auftrag bleibt. Keiner ist ein Sicherheitsleck:
alle fallen zu, nicht auf.

Fundstellen gelten für den Stand `f8871fa502` (Merge-Basis des Branches); vor
dem Anpacken gegen `origin/dev` gegenprüfen, nicht gegen den lokalen Stand.

## 1. Radio-Favicons bleiben beim Kaltstart aus

`crates/reprise-gnome/src/ui/radio/radio_columns.rs:299-304` bindet Bildmaterial
über `source_image::gate_open()` — den zuletzt veröffentlichten Wert des
prozessweiten `GATE_OPEN`-Atomics (`ui/podcasts/source_image.rs:66`, startet auf
`false`). Gefüllt wird das Atomic nur von einem `SourceImage::load_texture`
irgendwo sonst in der App oder vom Besuch der Einstellungen
(`ui/preferences/preferences.rs:473`, einziger Aufrufer von `recompute_gate`).

Ist die erste gerenderte `SourceImage` in einer Sitzung eine Radiozeile, liefert
`gate_open()` also `false` — auch für einen Nutzer, der voll zugestimmt hat. Die
Favicons bleiben aus, bis eine andere Ansicht das Atomic nebenbei füllt.

**Warum es zählt:** reine Reihenfolgeabhängigkeit, für den Nutzer nicht
nachvollziehbar. Das Verhalten fällt zu (kein Abruf ohne Zustimmung), verletzt
also `NET-1a`/`SRC-11` nicht — es ist ein Funktions-, kein Zustimmungsfehler.

**Richtung:** das Gate einmal beim Fenster-/App-Start berechnen, so wie es
`preferences.rs:473` bereits tut, damit kalte Radiozeilen nicht von der
zufälligen Reihenfolge anderer Ansichten abhängen. Alternativ berechnet
`radio_columns` den Wert selbst statt das Atomic zu lesen — dann verschwindet
die verdeckte Kopplung ganz.

**Beweis, den der Fix schuldet:** isoliertes Profil, leerer Bild-Cache,
zugestimmter Nutzer, App startet **direkt** in der Radio-Ansicht → Favicons
sind da. Ohne diesen Lauf ist nichts gezeigt: grüne Tests beweisen hier nichts.

## 2. Die Cache-Obergrenze wird nicht atomar durchgesetzt

`crates/reprise-core/src/remote_image/cache.rs:128-149`: `enforce_bound` liest
das Verzeichnis, rechnet sich seine Räumliste aus und löscht — ohne Sperre über
Lesen und Räumen. Alle Worker rufen `store_image` gleichzeitig. Schreiben zwei
Worker fast zeitgleich, arbeitet jeder auf einer Momentaufnahme ohne den
Schreibvorgang des anderen; die Obergrenze kann dadurch kurzzeitig um bis zu
`ARTWORK_WORKERS - 1` Einträge überschritten werden.

Das Muster ist alt, aber der Branch verschärft es: 8 statt 4 Worker verdoppeln
die Überschreitung im schlechtesten Fall. Selbstheilend — der nächste
`store_image` korrigiert; nichts wächst unbegrenzt, nichts wird beschädigt.

**Warum es trotzdem notiert ist:** der Doc-Kommentar verspricht eine härtere
Zusage („enforces a scope's cap"), als der Code einlöst. Entweder den Kommentar
auf die weiche Zusage ziehen oder Lesen und Räumen unter eine Sperre stellen.

**Messbar machen:** 8 Worker gleichzeitig auf denselben Bereich schreiben lassen
und den Höchststand der Einträge mitschreiben — sonst bleibt die Aussage
theoretisch.

## 3. Bild-URLs im Debug-Log

`ui/podcasts/source_image.rs:419` und `ui/podcasts/source_artwork_queue.rs:176-181`
loggen bei Decode-/Abruffehlern die vollständige Bild-URL. Für Podcasts,
YouTube-Kanäle und Radiosender ist diese URL faktisch eine Kennung dessen, was
der Nutzer abonniert hat.

Im Normalbetrieb ist nichts sichtbar: `main.rs:92-93` setzt den Filter auf
`info,lofty=error`, die Zeilen erscheinen nur mit `REPRISE_LOG=debug`. Genau
dieser Schalter wird aber für die headless-Diagnose regelmäßig gesetzt.

**Richtung:** erst relevant, wenn es einen „Debug-Log einsammeln"-Weg für Nutzer
gibt. Dann `url=`/`path=` in den Bildzeilen kürzen oder weglassen. Vorher nichts
tun — die Felder sind bei der Fehlersuche nützlich.

## Reihenfolge

1 ist der einzige Punkt mit sichtbarer Nutzerwirkung und sollte zuerst kommen.
2 ist eine Kommentar- oder Sperrfrage, 3 wartet auf einen Anlass.
