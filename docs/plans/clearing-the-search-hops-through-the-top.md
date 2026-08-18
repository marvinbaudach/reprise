---
slug: clearing-the-search-hops-through-the-top
worktree:
branch:
phase: todo
codex_session:
created: 2026-08-17
---
# TODO: Nach „Clear" springt die Ansicht erst an den Tabellenanfang und dann zum Song

**Beobachtung des Nutzers, kein Plan. Nicht nachgestellt, nur aus dem Code
hergeleitet.** Gemeldet am 17.08.2026:

> *„wenn ich suche nutze und einen song aus den results abspiele, dann clear
> drücke, dann springt die ansicht kurz zum anfang der tabelle und dann erst
> wieder zurück zu meinem Song. er sollte nicht hüpfen wenn möglich"*

Der **Endzustand ist richtig** — die Ansicht landet beim laufenden Titel. Das
ist genau, was SEARCH-16 verlangt. Beanstandet ist allein der **sichtbare
Zwischenzustand**: ein Bild am Tabellenanfang, bevor die Zentrierung greift.

## Das ist der Rest des schon behobenen Fehlers, nicht seine Rückkehr

`docs/plans/jump-to-playing-track-drops-the-filter.md` (`phase: shipped`, PR
**#489**, `8188bcf29a`) hat denselben Handgriff behandelt: suchen → aus den
Treffern abspielen → „Clear all". Damals **blieb** die Ansicht oben stehen.
Seitdem gilt SEARCH-16 (`docs/ux-rules.md`), und die Ansicht kommt zurück:

> *„Emptying the query … restores the pre-search anchor, unless the user
> started playback during that query …, in which case the loaded track is
> centred."*

Der Sprung, den der Nutzer jetzt sieht, ist der **Weg** dorthin. Dieselbe
Erscheinungsform ist im Repo schon einmal als eigener Fehler geführt worden:
`docs/plans/navback-scroll-jump-to-top.md` (`phase: reviewed`) — *„erscheint
für einen Moment an ihrer Spitze und rückt dann an die richtige Stelle. Der
Endzustand stimmt; sichtbar ist der Ruckler dorthin."* Wortgleiches Symptom,
anderer Auslöser (Zurück-Navigation statt Suche-leeren).

## Wo der Zwischenzustand herkommt (Herleitung, unbewiesen)

Alle Zeilenangaben gegen `origin/dev` = `216890a548`. Der lokale Hauptcheckout
stand beim Schreiben auf `be5f014d3b` (13.08.), also **vor #489** — wer dort
nachliest, bekommt den alten Code und damit die alte Diagnose: dort gibt es
`ReloadViewport::CenterPlayingElsePreSearch` noch nicht, `shared.pre_search`
heißt noch `pre_search_anchor`, und das Leeren führt immer über
`RestorePreSearch` mit leerem Anker. Genau so ist es einem Suchlauf für dieses
TODO ergangen. Also `git show origin/dev:<pfad>` lesen, nicht die Arbeitskopie.

**1. Nach dem Modelltausch setzt GTKs eigene Allokation die Position.** Das
steht im Repo schon als Kommentar, an `schedule_top_scroll_restore`
(`track_list_reload.rs:296-306`):

> *„the allocation pass that follows restores GTK's own scroll position — the
> pre-filter value, clamped to the new and usually much shorter list."*

Beim Leeren der Suche läuft es andersherum: die Liste wird **länger**, der alte
Wert stammt aus einem kurzen Trefferset und ist entsprechend klein — also
faktisch der Tabellenanfang. Genau das Bild, das der Nutzer beschreibt.

**2. Die Zentrierung kommt erst danach.**
`centered_scroll_restore::schedule` (`centered_scroll_restore.rs:11-59`)
versucht `apply()` sofort; scheitert das an noch nicht messbarer Geometrie —
der Normalfall unmittelbar nach dem Tausch —, registriert es zwei
**nachgelagerte** Korrekturen (`after_changed_once` und `idle_add_local_once`)
und ruft am Ende GTKs `column_view.scroll_to(position, …, ListScrollFlags::NONE, …)`.
Es gibt in diesem Pfad **keinen Schreibvorgang vor dem ersten Bild**:
`prepaint_position` wird hier nur benutzt, um die Zeilennummer für `scroll_to`
zu bestimmen, nicht um das Adjustment vorab zu setzen.

**3. Der Puffer, der genau das verdecken würde, wird vorher weggeworfen.** Der
Anker-Pfad `reload_anchor_scroll::schedule` bekommt einen `AdjustmentHold`
durchgereicht (250 ms, `reload_anchor_scroll.rs:15`), der fremde Schreiber
während des Einschwingens verdrängt. Der Zentrier-Pfad bekommt ihn nicht — er
wird unmittelbar davor **freigegeben** (`track_list_reload.rs:249-260`):

```rust
if matches!(viewport, ReloadViewport::CenterPlayingElsePreSearch) {
    shared.pre_search.take();
    if let Some(hold) = hold {
        hold.release_now();          // <— der Halt fällt weg …
    }
}
super::centered_scroll_restore::schedule(shared, playing_track_id, current_ids);
```

Ob die Freigabe die Ursache oder nur eine Begleiterscheinung ist, ist **offen**
— sie kann sehr wohl nötig sein, damit der Zentrier-Schreiber selbst nicht
verdrängt wird. Aber sie ist die Stelle, an der der eine Pfad einen Schutz hat,
den der andere nicht hat.

## Lösungsrichtungen (offen, nicht abgewogen)

1. **Vor dem ersten Bild schreiben.** Wenn beim Leeren der Suche die
   Zeilenhöhe der *alten* Liste bekannt ist (sie ändert sich beim
   Listenwechsel nicht — genau der Befund aus `navback-scroll-jump-to-top`),
   lässt sich das Ziel schon vor der Allokation rechnen und setzen. Der
   Zwischenzustand entsteht dann gar nicht erst.
2. **Den Halt über die Zentrierung ziehen** statt ihn davor freizugeben — mit
   dem Zentrier-Schreiber als erlaubtem Schreiber.
3. **Erst zeichnen, wenn die Position steht.** Grundsätzlich sauber, aber der
   teuerste und riskanteste Weg (blockierte Frames, Flackern anderswo).

Richtung 1 ist die naheliegendste, weil dieselbe Mechanik den verwandten Fehler
bereits gelöst hat.

## Was zu klären ist

- **Welches „Clear" der Nutzer meint, ist egal — es ist derselbe Code.** Alle
  Wege (Chip-×, Escape über `shortcuts.rs:149`, Feld von Hand leeren über
  `section_search.rs:131`, „Clear all" über `window_runtime_wiring.rs:472`)
  laufen durch `SectionSearch::clear_active_query` (`section_search.rs:349`)
  und enden in `TrackList::set_filter("")`. Ein Fix trifft alle fünf; ein
  Nachstellen braucht keine Variantensuche.
- **Hängt es an der Listenlänge?** Kurzes Trefferset → lange Bibliothek ist der
  gemeldete Fall. Bei ähnlich langen Listen wäre der geklemmte Wert nicht am
  Anfang, der Sprung also kleiner oder unsichtbar.
- **Gilt es genauso für den Anker-Pfad ohne Wiedergabe** (Clear ohne dass
  etwas abgespielt wurde, `RestorePreSearch`)? Dort greift der `AdjustmentHold`
  — wenn es dort *nicht* hüpft, ist das der Beleg für Richtung 2.
- **Ist der Sprung eine Regelverletzung?** `docs/ux-rules.md` verbietet den
  sichtbaren Zwischenzustand für die Trackliste bisher **nicht**; SEARCH-16
  beschreibt nur das Ergebnis. Ein Fix braucht deshalb entweder eine SEARCH-16-
  Revision oder eine eigene Regel („die Wiederherstellung ist nicht als
  Zwischenposition sichtbar").

## Anknüpfungspunkte

- `crates/reprise-gnome/src/ui/track_list/centered_scroll_restore.rs`
- `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:208-290`
  (`restore_reload_anchor`), `:296-320` (`schedule_top_scroll_restore`)
- `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`,
  `crates/reprise-gnome/src/ui/adjustment_hold.rs`
- **Der Endzustand ist bereits getestet — der Weg dorthin nicht.**
  `track_list/search_viewport_display_tests.rs:290`
  `search_16_clearing_after_a_play_centers_the_loaded_track` stellt genau
  diesen Handgriff nach und prüft, wo die Ansicht **landet**. Daneben
  `:322` `search_16_clearing_without_a_play_returns_to_the_pre_search_place`,
  `:115` `search_16_a_result_set_that_fits_still_centers_after_clear_all`,
  `:26` `typed_search_reads_from_the_top_and_clearing_comes_back`. Alle vier
  sind `#[ignore]` und brauchen `xvfb-run`. Kein Test sieht die
  Zwischenposition — dort ist die Lücke.
- `reload_restore.rs` (`fil_9_filter_change_centers_playing_track_in_new_results`)
- Verwandte offene Aufgabe im selben Modul:
  `docs/plans/jump-always-centers-the-current-track.md` (`phase: todo`) — dort
  geht es um *wo* die Zeile landet, hier um *wie sichtbar* der Weg dorthin ist.
  Beide fassen `centered_scroll_restore.rs` an; wer zuerst baut, sollte das
  wissen.

## Messen, bevor gebaut wird

Ein Zwischenbild ist per Screenshot kaum zu fassen. Der Repo-eigene Weg ist
`crate::ui::scroll_probe::probe(...)` — `centered_scroll_restore.rs:89` schreibt
bereits `"centered_refinement"`, `reload_anchor_scroll.rs:26-51` hat je einen
Namen pro Pfad. Ein Display-Test, der die **Folge** der geschriebenen Werte
mitschreibt, weist den Sprung als Wertfolge nach (klein → Ziel) statt als Bild.
