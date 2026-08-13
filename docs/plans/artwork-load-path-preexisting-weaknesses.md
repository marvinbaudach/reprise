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

## Gegenprobe gegen `origin/dev` (13.08.2026, `4912275130`)

Alle vier Punkte sind dort **offen**. Verifizierte Fundstellen, die im Text oben
noch auf die Merge-Basis zeigen:

| Sache | Stelle auf `origin/dev` |
| --- | --- |
| `GATE_OPEN`, startet `false` | `ui/podcasts/source_image.rs:66` |
| `recompute_gate()` | `source_image.rs:161-166` |
| `gate_open()` | `source_image.rs:170-172` |
| Schreibstelle in `load_texture` | `source_image.rs:374` |
| einziger `recompute_gate`-Aufrufer | `ui/preferences/preferences.rs:473` |
| Leser des Atomics | `ui/radio/radio_columns.rs:303` |
| `enforce_bound` | `reprise-core/src/remote_image/cache.rs:143` |
| `ARTWORK_WORKERS = 8` | `ui/podcasts/source_artwork_queue.rs:10` |
| `AssertUnwindSafe` + `catch_unwind`, ohne Kommentar | `source_artwork_queue.rs:204-206` |
| Panik-Test, Panik im `fetch`-Closure | `source_artwork_queue.rs:425`, Panik in `:437` |

Kein Aufruf von `recompute_gate` beim App- oder Fensterstart — Punkt 1 gilt
unverändert. Ein zweiter Panik-Test für `decode_pixels` existiert nicht.

**Zwei Korrekturen am Text oben:**

- **Punkt 3 stimmt so nicht.** Neben den beiden Debug-Zeilen
  (`source_image.rs:386`, `source_artwork_queue.rs:176-180`) loggt
  `source_artwork_queue.rs:208` die volle URL auf **`warn`** — und `warn` liegt
  über dem Default-Filter `info,lofty=error` (`main.rs:93`). Die Aussage „Im
  Normalbetrieb ist nichts sichtbar" trägt also nur für die Debug-Zeilen. Ob das
  den Punkt aus dem Wartestand holt, ist eine eigene Entscheidung; hier ist nur
  festgehalten, dass die Prämisse nicht hält.
- **Punkt 2, Doc-Kommentar.** Der oben zitierte Wortlaut („enforces a scope's
  cap") wurde über `enforce_bound` nicht gefunden. Bevor jemand „den Kommentar
  auf die weiche Zusage ziehen" als Lösung wählt: erst nachsehen, ob der Satz
  noch existiert und wo — sonst greift dieser halbe Vorschlag ins Leere.

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

## 4. Zwei Punkte, die es nicht mehr in #452 geschafft haben

Aus der Review der fünf Nachbesserungs-Commits, beide klein und beide bewusst
nicht mehr eingebaut, weil der PR schon gelandet war:

**a) `AssertUnwindSafe` ohne Begründung.** In `run_worker`
(`ui/podcasts/source_artwork_queue.rs`) wickelt `catch_unwind` den ganzen
`process_job`-Aufruf ein. Die Zusicherung trägt nur, weil der konkrete
`fetch`-Closure zustandslos ist — er umschließt eine freie Funktion. Der Typ
erzwingt das nicht, und derselbe `&mut dyn FnMut` wird für jeden weiteren Auftrag
in der Schleife wiederverwendet. Gibt ihm jemand später einen eigenen Zustand
(Wiederholungszähler, kleiner Verbindungs-Cache), bricht die Annahme still. Eine
Zeile Kommentar in der Art der `// SAFETY:`-Disziplin des Projekts, die genau
diese Annahme benennt.

**b) Der Panik-Test deckt den unwahrscheinlicheren Fall ab.**
`src_11_panicking_job_finishes_and_the_worker_accepts_the_same_url_again` löst
die Panik im `fetch`-Closure aus — also **vor** `std::mem::take(waiters)`. Der
wahrscheinlichere Ort ist `decode_pixels`: es läuft danach.

> **Korrektur 13.08.2026:** `decode_pixels`
> (`ui/podcasts/source_image_texture.rs:33-52`) nimmt einen **`&Path`**, keine
> Bytes — die Formulierung „schickt geladene Bytes durch die pixbuf-FFI" stimmt
> nicht. Es ruft `Pixbuf::from_file_at_scale(path, width*2, height*2, true)`.

Was er festnageln soll: Nach einer Panik im Dekodieren wird der `pending`-Eintrag
trotzdem entfernt, eine spätere Anfrage für dieselbe URL startet einen frischen
Auftrag und läuft durch — die URL strandet nie. Dass die Wartenden der
abgebrochenen Charge dabei einen geschlossenen Kanal statt `Ok(None)` sehen, ist
in Ordnung und soll so bleiben: `source_image.rs` behandelt beides gleich.
`process_job` deswegen **nicht** umbauen — der Test hält fest, was wirklich gilt.

Nachgeprüft am 13.08.2026 gegen `origin/dev`: beide Punkte sind dort offen.

## Reihenfolge

1 ist der einzige Punkt mit sichtbarer Nutzerwirkung und sollte zuerst kommen.
2 ist eine Kommentar- oder Sperrfrage, 3 wartet auf einen Anlass.

## Stand 13.08.2026

**Punkt 1, 4a und 4b sind geplant** in `docs/plans/radio-favicons-cold-start.md`.
Punkt 1 wird dort über Richtung (b) gelöst — die Radio-Tabelle rechnet ihre
Bild-Erlaubnis selbst, `gate_open()` verliert damit seinen einzigen Aufrufer und
wird gelöscht. Ausschlaggebend war ein Befund, der beim Planen dazukam: `queue()`
hat genau einen Aufrufer, und der schreibt `GATE_OPEN` sechs Zeilen vorher. Der
Worker liest den Startwert `false` also strukturell nie — nur die UI tut das. Ein
`recompute_gate` beim App-Start (Richtung (a)) wäre nach dem Umbau toter Code.

**Punkt 2 und 3 bleiben offen** und sind bewusst nicht Teil dieses Plans.

### Vier Sachen, die beim Planen abgefallen sind

1. **Für Quellen-Artwork gibt es keine Fixture-Route.** Das ist eine echte Lücke,
   keine Marginalie: `validate_remote_url` und `PublicOnlyResolver`
   (`reprise-core/src/podcasts/source_artwork.rs:69-96` bzw. `:32-45`) sperren
   jede lokale Gegenstelle hart, ohne `#[cfg(test)]`-Ausweg. Podcasts, Radio,
   Concerts, Lyrics (zweimal) und MusicBrainz haben je ein eigenes
   Fixture-System — Artwork nicht. Eine gemeinsame Schicht existiert nirgends.
   Folge: **Artwork lässt sich headless nicht offline abnehmen.** Der
   Kaltstart-Beweis in `radio-favicons-cold-start.md` weicht deshalb auf echte,
   öffentliche URLs aus dem eigenen Repo aus. Das trägt für einen einmaligen
   Abnahmelauf, aber nicht für einen wiederholbaren Test — der nächste, der
   Artwork headless prüfen will, steht wieder davor. Aufwand für eine eigene
   Route: ~60–80 Zeilen nach dem Vorbild von `concerts/http.rs`, feature-gegatet
   hinter `test-fixtures` (existiert, `reprise-core/Cargo.toml:12`, von
   `reprise-gnome/Cargo.toml:18` durchgereicht).
2. **`track_list_smoke.rs:52-58` listet die akzeptierten `REPRISE_SMOKE_SOURCE`-
   Werte unvollständig** — `radio`, `podcasts`, `youtube`, `concerts` und
   `releases` fehlen, obwohl `parse_smoke_source` (`:125-148`) sie annimmt. Reiner
   Doku-Fehler, eine Zeile, vorbestehend.
3. **`network_allowed(...).unwrap_or(false)` vs. `network_allowed_or_off(...)`.**
   Die zweite Form ist laut ihrem Doc-Kommentar (`online_sources.rs:104-112`) die
   für Frontends vorgesehene, loggt aber bei jedem Lesefehler eine Warnung — bei
   einem Aufruf pro Zeilen-Bind ist das zu laut. `radio/add_dialog.rs:35` benutzt
   die erste Form, der neue Radio-Pfad erbt sie. Wenn das Projekt die
   konsolidierte Form erzwingen will, ist es ein Einzeiler an beiden Stellen.
4. **Ein warmer Bild-Cache umgeht die Gate-Abfrage.** `process_job` fährt das
   cache-only-`resolve` (`source_artwork_queue.rs:123-137`) **vor** der
   Gate-Abfrage (`:138`). Jede künftige Abnahme in diesem Ladepfad muss mit
   leerem `XDG_CACHE_HOME` laufen, sonst erscheinen die Bilder auch mit dem
   Fehler und der Lauf beweist nichts. Der Bild-Cache liegt unter
   `$XDG_CACHE_HOME/reprise/covers/remote-images-{persistent,transient}/`
   (`cover::cache_dir()` → `dirs::cache_dir()`).
