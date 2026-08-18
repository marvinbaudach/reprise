---
slug: filter-bar-clear-without-a-filter
worktree: /home/marvin/Projects/reprise-filter-bar-clear-without-a-filter
branch: feature/filter-bar-clear-without-a-filter
phase: planned
codex_session:
created: 2026-08-18
---
# Plan: Die Filterleiste zeigt keine Aktion mehr für einen Filter, den es nicht gibt

Aus dem Befund vom 16.08.2026 (*„es sollte kein Clear angezeigt werden, wenn gar
kein Filter gesetzt ist"*). Der Befund hatte bereits nachgewiesen, dass die
sichtbare Schaltfläche gar nicht die Filter-Schaltfläche ist, sondern
`clear_selection`. Gegrillt am 18.08.2026; die Messung hat zwei Ursachen
getrennt, die der Befund noch zusammen gesehen hat.

## Was gemessen wurde

1. **Ein einfacher Klick auf eine Zeile ist bereits eine Auswahl.**
   `podcasts_row_interaction.rs::pointer_intent`: ein Druck →
   `Select(SelectMode::Only)`, zwei Drücke → `Play`. Der erste Druck eines
   Doppelklicks wählt also mit aus — ausdrücklich so gewollt („that is what
   `ColumnView` does"). Wer eine Episode zum Abspielen anklickt, hinterlässt
   eine Auswahl.
2. **Die Sitzungswiederherstellung wählt von selbst aus.**
   `podcasts_view_marker.rs:173-177`: bei `LoadedItemChange::SessionRestore` und
   `RevealRequest::Episode` ruft die Ansicht `select_row(episode_id,
   SelectMode::Only)`. Nach jedem Start steht damit „1 selected" in der Leiste,
   ohne dass jemand etwas ausgewählt hätte — und die Zeile liegt oft in einem
   eingeklappten Kanal, ist also nirgends zu sehen. **Das ist das Bild aus dem
   Screenshot.**
3. **Die Track-Liste macht es anders.** `current_track_selection.rs:38-44`:
   `CurrentTrackChange::SessionRestore` ergibt `TrackRevealPolicy::MarkerOnly` —
   Marker, keine Auswahl. Die Podcast-Ansicht weicht ohne erkennbaren Grund von
   der eigenen Hausregel ab.
4. **Beide Oberflächen teilen sich die Leiste.** Die gruppierte
   Bibliotheksansicht und die YouTube-Kanaldetailansicht speisen dieselbe
   `podcasts_filter_bar.rs` (`podcasts_view_selection.rs:44`,
   `youtube_channel_detail.rs:577-580`). Eine Umbenennung trifft beide. Die
   Auto-Auswahl aus Punkt 2 gibt es **nur** in der Bibliotheksansicht.
5. **Zwei Sprachkataloge müssen vollständig bleiben.**
   `scripts/tests/gettext-catalogs.sh` führt `complete_locales=(de es)`. Neue
   `msgid`s ohne Übersetzung machen diesen Lauf rot.

## Die Entscheidungen

1. **Beide Schaltflächen werden explizit benannt.** „Clear all" → „Clear
   filters", „Clear" → „Clear selection". Danach kann keine der beiden mehr für
   die allgemeinere gehalten werden. Der Kommentar über
   `PODCAST_CLEAR_SELECTION` verlangt das bereits („the two must not read
   alike") — er wird endlich eingelöst.
2. **Die Auto-Auswahl beim Sitzungsstart bleibt.** Im Grill war sie als
   Abweichung der Podcast-Ansicht eingeordnet worden — das war falsch. Sie ist
   die Umsetzung der **aktiven** Regel `START-3`
   (`docs/ux-rules.md:1398`: „that row becomes the sole selection and is
   centered", ausdrücklich für „the last loaded track **or episode**"). Die
   Track-Liste tut dasselbe und nagelt es fest
   (`start_restore_tests.rs:121`: „START-3 gives the restored loaded track the
   sole selection"); `TrackRevealPolicy::MarkerOnly` regelt das Zentrieren, nicht
   die Auswahl. Eine Streichung wäre eine Regeländerung mit eigenem Radius —
   auch `docs/ux-rules.md:3264` argumentiert mit ihr. Entschieden am 18.08.2026:
   nicht im Vorbeigehen, und wenn, dann für Track-Liste und Episodenliste
   gemeinsam.
3. **Die Sichtbarkeitsregel bleibt, wie sie ist.** Sobald eine Auswahl besteht,
   bleibt der Weg sie aufzulösen sichtbar — auch bei eingeklapptem Kanal. Eine
   Aktion zu verstecken, deren Zustand („1 selected") weiter angezeigt wird,
   wäre die schlechtere Falle.

## Aufgaben

1. **Beschriftungen** in `crates/reprise-gnome/src/ui/strings_podcasts.rs`:
   `PODCAST_CLEAR_ALL` von `"Clear all"` auf `"Clear filters"`,
   `PODCAST_CLEAR_SELECTION` von `"Clear"` auf `"Clear selection"`. Der
   erklärende Kommentar darüber bleibt und wird an den neuen Stand angepasst.
2. **Kataloge nachziehen.** `po/reprise.pot` neu erzeugen (der Aufruf steht in
   `scripts/tests/gettext-catalogs.sh`) und die beiden vollständigen Sprachen
   übersetzen:
   - de: „Clear filters" → **„Filter löschen"**, „Clear selection" →
     **„Auswahl löschen"** (bestehende Linie: „Clear all" = „Alle löschen",
     „Selection" = „Auswahl").
   - es: „Clear filters" → **„Limpiar filtros"**, „Clear selection" →
     **„Limpiar selección"** (bestehende Linie: „Clear all" = „Limpiar todo").
   Die übrigen Sprachen (ar, bn, fr, hi, zh_CN) sind nicht als vollständig
   geführt und bleiben unübersetzt.
3. **`podcasts_view_marker.rs` bleibt unangetastet.** Siehe Entscheidung 2.

## Tests

1. `start_3_restored_episode_uses_the_selection_reveal_path` bleibt unverändert
   grün — der Test, der `START-3` in dieser Ansicht festhält.
2. Ein einfacher Klick auf eine Zeile wählt weiterhin aus, die Schaltfläche
   erscheint (das bestehende Verhalten darf nicht mitverschwinden).
3. Die Beschriftungen: die Filter-Schaltfläche trägt „Clear filters", die
   Auswahl-Schaltfläche „Clear selection" — als Zeichenketten-Test, damit die
   beiden nicht wieder aneinander angleichen.
4. `bash scripts/tests/gettext-catalogs.sh` läuft grün (de und es vollständig).

## Nachweis

1. App starten, ohne etwas anzuklicken: die wiederhergestellte Episode ist
   weiterhin ausgewählt (`START-3`), die Schaltfläche daneben heißt aber
   **„Clear selection"** und ist damit nicht mehr mit dem Filter zu
   verwechseln — das war der gemeldete Fehler.
2. Eine Episode einfach anklicken: „1 selected" und **„Clear selection"**
   erscheinen; die Schaltfläche räumt die Auswahl, nicht den Filter.
3. Einen Filter setzen: rechts steht **„Clear filters"**, und zwar auch dann,
   wenn gleichzeitig eine Auswahl besteht — beide Schaltflächen sind dann
   nebeneinander unterscheidbar.

## Parallelität

**Nicht teilbar.** Nach dem Wegfall von Aufgabe 3 bleiben zwei Aufgaben, und
Aufgabe 2 hängt unmittelbar an den Zeichenketten aus Aufgabe 1 (dieselben
`msgid`s).
