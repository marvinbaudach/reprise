---
slug: one-centering-path-for-jump-and-clear
worktree: /home/marvin/Projects/reprise-one-centering-path-for-jump-and-clear
branch: feature/one-centering-path-for-jump-and-clear
phase: coded
codex_session:
created: 2026-08-18
---
# Ein Zentrierpfad für den Sprung und für das Leeren der Suche

Zieht `jump-always-centers-the-current-track` (16.08.) und
`clearing-the-search-hops-through-the-top` (17.08.) zu einem Vorhaben zusammen:
beide fassen `centered_scroll_restore.rs` an, beide verweisen aufeinander, und
beide beschreiben dieselbe Wurzel aus zwei Blickwinkeln — der eine fragt **wo**
die Zeile landet, der andere **wie sichtbar** der Weg dorthin ist. Dazu kommt
aus dem Grill eine dritte, bewusst beschlossene Änderung: der
Seitenleisten-Wechsel zentriert den laufenden Titel (Task 6).

**Alle Zeilenangaben gegen `origin/dev` = `9ac0aa425d` (nach #568).** Der
geteilte Hauptcheckout steht auf `be5f014d3b`, also vor #489 — wer dort
nachliest, bekommt einen Stand, den es nicht mehr gibt. `git show origin/dev:<pfad>`.

## Stand 19.08.2026, abends — alle Aufgaben sind auf dem Zweig

**Tasks 1, 2, 3, 5 und 6 sind umgesetzt. Task 4 ist ersatzlos entfallen.**
Der Weg dorthin lief über zwei Messungen: die aus Task 2, die die
Vorentscheidung dieses Plans widerlegt hat, und eine zweite beim
Wiederaufnehmen, die den Umbau in seine endgültige Form gebracht hat. Beide
stehen unten. Der ältere Abschnitt bleibt stehen, weil er erklärt, warum die
naheliegende Lösung nicht funktioniert.

### Was am Ende gebaut wurde

Ein Zug reicht nur, wenn zwei Dinge zusammenkommen — keines allein genügt:

1. **Der Bereich vor dem Wert.** `ListGeometry::configure` sät `upper` aus der
   gemerkten Zeilenhöhe und schreibt den Wert im selben Aufruf. Ohne das wird
   das Ziel in den *alten* Bereich geklemmt (714 bei 21 Treffern) und die
   Schreibung ist schlicht weg.
2. **Ein Wert, den GTKs Anker reproduziert.** Die zweite Messung hat gezeigt,
   dass `scroll_to` die Zeile **bedingungslos oben ausrichtet** — auch dann,
   wenn sie bereits zentriert im Blick steht (aus 2923.5 wie aus 2927.0 wurde
   `scroll_to(89)` → 3026.0 = 89 × 34). Die Werte, die ein einziger Zug halten
   kann, sind damit genau die Zeilenkanten. `centered_anchor` nimmt die Kante,
   die dem arithmetischen Mittelwert am nächsten liegt, und übergibt GTK die
   Zeile, die diesen Wert erklärt.

Der Preis ist höchstens eine halbe Zeile Versatz — in der gemessenen Geometrie
0,5 px von 239 px Viewport. Zwei Geschwistertests halten den Pfad deshalb auf
eine halbe Zeile statt auf einen halben Pixel.

**Task 4 entfällt.** Ein `AdjustmentHold` korrigiert aus einem Idle heraus und
ist damit immer ein zweiter sichtbarer Schritt — genau die Prügelei, die die
erste Messung protokolliert hat. Die Verankerung ersetzt ihn: es gibt nichts
mehr zu korrigieren. `hold.release_now()` vor der Zentrierung bleibt, wie es
war.

**Nachweis.** Der Kontrollarm aus Task 2 ist umgedreht — er hieß
`…_hops_through_an_intermediate_position` und behauptet jetzt als
`search_16_clearing_after_a_play_reaches_the_track_in_one_step` den einen Zug.
Zwei Mutationen töten ihn und sonst nichts Sichtbares:
`RevealMotion::Instant` → `Glide` reproduziert exakt die alten 6460.0, und das
Zurückdrehen der Kantenwahl auf den exakten Mittelwert macht wieder zwei
Schritte (2927.0 → 2924.0), während der Endwerttest daneben grün bleibt —
genau der blinde Fleck, für den der Kontrollarm gebaut wurde.

### Der Kontrollarm hat getragen

Zwei Anlässe, gemessen als Folge der tatsächlich eingenommenen Positionen mit
dem Schreiber je Schritt:

```
Suche leeren, während etwas läuft
  centered.scroll_to      3026.0     ← unser Edge-Snap
  centered.changed.apply  2923.5     ← die Zentrierung
App-Start (center_loaded_track)
  centered.initial.apply  4657.5     ← einstufig, kein Snap
```

Der Zweischritt existiert also, und er gehört uns — beim Leeren der Suche. Der
Start-Anlass ist **schon heute einstufig**: seine Geometrie steht bereits, der
sofortige `apply()` gelingt, und der Snap läuft gar nicht erst (Punkt 4 der
Belegliste). Die Erwartung „zweistufig für beide Anlässe" traf nur für einen zu.

### Die Frage (a) vs. (b) hat keine Antwort, sondern beide

Der Plan stellte den Edge-Snap **(a)** und GTKs eigene Allokationsschreibung
**(b)** als Alternativen gegenüber, von denen die Messung eine auswählt. Sie
sind keine Alternativen. Der Snap **verdeckte** (b):

Mit dem Snap entfernt und ansonsten genau dem Umbau aus Task 3 schreibt GTK
nach unserer sauberen Zentrierung ihren eigenen Wert darüber — gemessen
`centered.reveal.instant 2923.5` gefolgt von einem Schreiber, den keine Sonde
beansprucht, mit `0.0` im einen und `6460.0` im anderen Lauf. Der Endwert war
danach falsch (`landed at 6460 instead of 2923.5`).

Der Grund ist der, aus dem `AdjustmentHold` überhaupt existiert: `scroll_to`
gibt der Ansicht eine **eigene übernommene Position**. Solange wir sie vorher
anschnappen ließen, landete GTKs Allokationsschreibung auf der Zielzeile statt
auf dem Versatz, den die Ansicht sich gemerkt hatte. Der Snap war nicht nur
Sichtbarkeitszusage, er war die Verankerung.

### Warum Task 4 in seiner geplanten Form nicht reicht

Task 4 sollte den vorhandenen `AdjustmentHold` über die Zentrierung ziehen. Ein
Halt verteidigt einen **Wert**, und den zentrierten Wert gibt es erst, wenn die
Geometrie sich gesetzt hat. Bis dahin verteidigt er den Versatz, auf dem die
geleerte Liste zufällig steht — gemessen als Prügelei:

```
gtk                      6460.0
hold                      482.0
hold                     2923.5
centered.reveal.instant   482.0
hold                     2923.5
```

Der Endwert stimmte, der Weg dorthin war vierstufig statt einstufig, also
schlimmer als der Zustand, den der Plan beheben wollte.

### Die zweite Messung: was davon getragen hat

Der Vorsatz war, GTKs Allokationsschreibung **auf dem zentrierten Wert** landen
zu lassen statt sie hinterher zu korrigieren — mit der Behandlung des
Ankerpfads: Bereich vorsäen, Ziel vorhersagen statt nach dem Settle ableiten.
Die Hälfte davon trägt (Punkt 1 oben). Die andere Hälfte trägt **nicht**: mit
vorgesätem Bereich, aber ohne Anker, schrieb GTK weiterhin das Listenende
darüber (`6561.0 = upper − page`, zweimal). Erst der Anker auf einer
Zeilenkante hat den Zug auf einen reduziert.

Drei Sackgassen sind ausgeschlossen und müssen nicht erneut geprüft werden:

- **GTK kann nicht zentriert scrollen.** `gtk4::ScrollInfo` (0.11.4) kennt nur
  `set_enable_horizontal`/`set_enable_vertical`, keine Ausrichtung.
- **`scroll_to` ist kein „minimales Sichtbarmachen".** Es richtet die Zeile
  bedingungslos oben aus, auch wenn sie bereits vollständig und mittig im Blick
  steht. Ein Anker, der den exakten Mittelwert halten soll, existiert nicht.
- **Ein Halt kommt zu spät und ist selbst ein Schritt** — er korrigiert aus
  einem Idle, GTKs Schreibung liegt davor und ist dann bereits sichtbar.

### Der geparkte Umbau ist überholt

`wip/one-centering-path-rebuild` war die Vorlage: `RevealMotion`,
`ScrollGlide::jump_to`, der Notnagel hinter den Versuchen und das Abräumen der
toten Helfer (`live_row_height`, `centered_track_scroll_target` nur noch
`#[cfg(test)]`) sind daraus übernommen. Was daran rot war — der Halt und die
fehlende Verankerung — ist ersetzt. Der Zweig wird nicht mehr gebraucht.

### Task 6 ist gebaut

Der Seitenleisten-Wechsel zentriert den laufenden Titel über denselben Pfad
(NAV-19, neu in `docs/ux-rules.md`, `[active]` im selben Commit). Die
Unterscheidung zur Verlaufsnavigation liegt an der Aufrufstelle
(`TrackList::set_source`), nicht in `view_session::restore_browser_place` —
Vor/Zurück bleiben unter BROWSE-2 unverändert. Anders als START-3 fasst die
Zentrierung die Auswahl nicht an; `center_loaded_track` behält seinen eigenen
Auswahlschritt, `center_playing_track_in_view` trägt nur die Platzierung.

## Was am Code belegt ist

1. **Zwei Zentrierpfade, identische Mathematik.** Beide enden in
   `scroll_center::centered_scroll_value_with_height` (`scroll_center.rs:43-58`):
   der Sprungpfad über `centered_scroll_target` (`:19-30`), der
   Wiederherstellpfad über `reload_restore::centered_track_scroll_target`
   (`reload_restore.rs:194-211`).

2. **Identisches Geometrie-Tor.** Die Annahme des TODOs, der Wiederherstellpfad
   sei nachlässiger, trägt **nicht**: `live_row_height(n)` ist wörtlich
   `settled_row_height(adjustment.upper(), n)` (`list_geometry.rs:498-501`) —
   genau das, was der Sprungpfad benutzt.

3. **Der Unterschied ist das Verhalten im Fehlschlag.**
   `centered_scroll_restore::schedule` (`:11-60`) versucht `apply()` einmal
   sofort; scheitert das, registriert es **genau zwei** Nachbesserungen
   (`after_changed_once` :28-38, `idle_add_local_once` :42-53) und ruft dann
   GTKs `column_view.scroll_to(position, …, ListScrollFlags::NONE, …)` (:55-59)
   — das bringt die Zeile *minimal* in Sicht, also an den Rand. Greift danach
   keine Nachbesserung, bleibt der Edge-Snap stehen. `track_reveal::reveal_position`
   (`:156-177`) kennt kein `scroll_to`: es zentriert oder versucht es im
   nächsten Leerlauf erneut, bis `attempts` aufgebraucht ist.

4. **Der Edge-Snap läuft nur im Fehlschlag** (früher `return` bei :16-18). Genau
   die Zweiteilung, die der Nutzer als „mal zentriert, mal oben" sieht.

5. **Drei Anlässe hängen am Wiederherstellpfad**, zwei Aufrufstellen:
   - `track_list_reload.rs:259` — Suche leeren, während etwas läuft
     (`CenterPlayingElsePreSearch`, SEARCH-16) **und** Filter leeren ohne Suche
     (`CenterPlayingTrack`, FIL-9; `viewport_after_clearing` :87-96);
   - `track_list_reload.rs:352` `center_loaded_track` — **ausschließlich der
     App-Start** (einziger Aufrufer `window/window_runtime_wiring.rs:675`,
     Doc-Kommentar nennt START-3). Das ist **nicht** der Reiterwechsel; der
     läuft über `set_source` → `restore_browser_place` (`track_list.rs:459-466`)
     und stellt die gemerkte Position der Ansicht wieder her.

6. **Ein Pfad schreibt sofort, der andere gleitet.** `apply()` schreibt
   `adjustment.set_value` (`:98`), `reveal_position` übergibt an
   `shared.scroll_glide.glide_to` (`track_reveal.rs:164`). Nach einem
   Modelltausch ist sichtbare Bewegung genau der beanstandete Hüpfer, beim
   Titelwechsel ist sie erwünscht (`scroll_glide.rs:1-8`).

7. **Der Schutz wird vor der Zentrierung weggeworfen.** `track_list_reload.rs`
   ruft `hold.release_now()` unmittelbar vor `centered_scroll_restore::schedule`;
   der Ankerpfad bekommt denselben `AdjustmentHold` über 250 ms durchgereicht
   (`reload_anchor_scroll.rs:15`).

8. **Der Wiederherstellpfad ist der einzige ungenannte Schreiber.** Der
   Ankerpfad benennt jeden Schreiber (`RestorePath::*_probe`,
   `reload_anchor_scroll.rs:26-52`), `schedule_top_scroll_restore` meldet
   `"top_restore"`. Das `scroll_to` in `centered_scroll_restore.rs:55-59` meldet
   nichts, obwohl `scroll_probe::probe_scroll_to` dafür existiert
   (`scroll_probe.rs:21-33`).

9. **Das `scroll_to` wird für die Messung nicht gebraucht.** `measurement()`
   läuft den Widgetbaum ab und sammelt die Höhe **jeder** realisierten
   `ColumnViewRow` (`list_geometry.rs:319-339`) — nicht die der Zielzeile. Die
   ohnehin realisierten Zeilen genügen. Der Snap leistet nur noch eines: er
   garantiert, dass die Zeile am Ende **überhaupt sichtbar** ist.

10. **Bug 1 ist schon heute eine Regelverletzung.** NAV-10b `[active]`
    (`ux-rules.md:3276-3292`) verlangt das Zentrieren ausdrücklich. Für Bug 1
    braucht es also keine neue Regel, nur einen Test, der sie durchsetzt.

### Die eine offene Frage, die die Messung beantwortet

Der Hüpfer beim Leeren der Suche hat zwei mögliche Erzeuger:

- **(a) unser Edge-Snap** — dann ist Task 3 die ganze Ursache;
- **(b) GTKs eigene Allokation**, die nach dem Modelltausch den alten, auf die
  neue Listenlänge geklemmten Wert zurückschreibt (im Repo dokumentiert an
  `schedule_top_scroll_restore`, `track_list_reload.rs:296-306`). Bei kurzem
  Trefferset → langer Bibliothek ist das faktisch der Tabellenanfang — genau
  das Bild aus dem Nutzerbericht. Dagegen hilft Task 3 **nicht**; dann trägt
  Task 4.

Beide Mechaniken sind unten ausgeschrieben. Die Messung entscheidet nur, ob
Task 4 eingebaut bleibt oder als unnötig herausfällt — Codex hat in beiden
Fällen eine fertige Aufgabe und muss nicht anhalten.

## Beschlüsse aus dem Grill (bindend)

1. **Der Edge-Snap überlebt als Notnagel**, hinter den Zentrierversuchen statt
   vor ihnen. Die heutige Zusage „die Zeile ist danach sichtbar" bleibt.
2. **Nur das *Wo*, nicht das *Wann*.** `reveal_policy`
   (`current_track_selection.rs:38-48`) bleibt unangetastet: Doppelklick-Start
   und Sitzungswiederherstellung bewegen nichts, der automatische Titelwechsel
   hält sich 1,5 s zurück, während gescrollt wird.
3. **Beide Mechaniken vorab ausgeschrieben** (Task 3 und Task 4).
4. **SEARCH-16 wird um einen Satz erweitert**; keine neue übergreifende Regel,
   weil die Zurück-Navigation dasselbe Symptom hat und hier nicht behoben wird
   (`docs/plans/navback-scroll-jump-to-top.findings.md`).
5. **Der Seitenleisten-Wechsel zentriert** (Task 6) — Gleichzug mit SRC-13, das
   für Podcasts/YouTube/Radio bereits „revealed … row centered — on entering the
   view" verlangt. **Vor/Zurück bleiben unberührt**, BROWSE-2 („Back/Forward
   restores exactly") bleibt unverändert gültig. Und: **die Zentrierung
   navigiert nie selbst** — sie wirkt ausschließlich in der gerade sichtbaren
   Ansicht und wechselt niemals den Reiter, um etwas zeigen zu können.
6. **Ein Plan, ein Zweig, ein PR.**

## Aufgaben

### Task 1 [erledigt] — Die Sonde deckt den letzten blinden Schreiber ab

`probe_scroll_to("centered.scroll_to", …)` vor dem `scroll_to` in
`centered_scroll_restore.rs`, und ein Name pro Nachbesserung statt des einen
`"centered_refinement"` (`:93`): `centered.initial.apply`,
`centered.changed.apply`, `centered.idle.apply`. Reine Diagnostik hinter
`REPRISE_SCROLL_PROBE`, kein Verhalten ändert sich.

### Task 2 [erledigt, danach umgedreht] — Kontrollarm: der Zwischenzustand als Wertfolge

Ein `#[ignore]`-Display-Test in `track_list/search_viewport_display_tests.rs`,
neben `search_16_clearing_after_a_play_centers_the_loaded_track` (`:290`), der
denselben Handgriff fährt — suchen, aus den Treffern abspielen, leeren — und die
**Folge** der geschriebenen Adjustment-Werte mitschreibt statt nur den Endwert.
Zweiter Fall im selben Modul: der Start-Anlass über `center_loaded_track`
(vgl. `start_restore_tests.rs:96`).

**Erwartung vor dem Fix:** zwei Stufen — ein kleiner Wert, dann das Ziel.

**Dieser Test ist der Kontrollarm und muss vor dem Umbau zweistufig sein.**
Ist er es nicht, ist die Diagnose falsch und Task 3 baut ins Blaue; dann
stoppen und berichten, nicht weiterbauen.

Der Test protokolliert zusätzlich **welcher** Schreiber welche Stufe erzeugt
hat (die Namen aus Task 1). Das ist die Antwort auf (a) vs. (b).

### Task 3 [erledigt, mit Verankerung statt bloßem Umbau] — Ein Pfad: `reveal_position` bekommt eine Bewegungsart

`reveal_position(shared, position, attempts)` wird zu
`reveal_position(shared, position, attempts, motion)` mit
`enum RevealMotion { Glide, Instant }`:

- `Glide` = heutiges Verhalten (`scroll_glide.glide_to`) — Sprung und
  Titelwechsel;
- `Instant` = `adjustment.set_value` direkt — jeder Anlass nach einem
  Modelltausch, weil dort jede sichtbare Bewegung der beanstandete Hüpfer ist.

`centered_scroll_restore::schedule` wird zum dünnen Vorspann: Position auflösen
(`prepaint_position` bleibt), dann
`reveal_position(shared, position, attempts, RevealMotion::Instant)`. Die zwei
Sonderpfade `after_changed_once`/`idle_add_local_once` und das doppelte
Geometrie-Tor entfallen.

**Der Notnagel** (Beschluss 1): sind die `attempts` aufgebraucht **und** wurde
nie zentriert, dann — und nur dann — einmal `column_view.scroll_to(position, …)`,
mit `probe_scroll_to` benannt. Der Sprungpfad bekommt denselben Notnagel; er
gibt heute still auf (`track_reveal.rs:167-169`).

Zwei Dinge dürfen nicht verloren gehen:

- **Der Kurzschluss „Inhalt passt in den Viewport"** (`apply()` :66-81) liefert
  heute `true` **ohne zu schreiben** und beendet die Kette;
  `centered_scroll_value_with_height` lehnt denselben Fall mit `None` ab
  (`scroll_center.rs:50`), was im Sprungpfad einen Retry auslöst. Der Umbau muss
  diesen Fall als **Ende** behandeln, sonst dreht eine kurze Liste `attempts`
  Runden ins Leere und schlägt am Schluss den Notnagel an. Belegt durch
  `search_16_a_result_set_that_fits_still_centers_after_clear_all`.
- **`shared.track_reveal_pending`** setzt heute nur der Sprungpfad
  (`track_reveal.rs:43`, `:137`), gelesen wird es vom Reload. Führt der
  Reload-Anlass durch dieselbe Funktion, muss entschieden und im Code begründet
  sein, ob er die Marke setzen darf — ein Reload, der auf sich selbst wartet,
  wäre eine neue Schleife.

### Task 4 [entfallen] — Der Halt deckt die Zentrierung mit ab

Statt `hold.release_now()` vor `centered_scroll_restore::schedule`
(`track_list_reload.rs`, im Zweig `CenterPlayingElsePreSearch`) den
`AdjustmentHold` über die Zentrierung ziehen, mit dem Zentrier-Schreiber als
erlaubtem Schreiber — so wie ihn der Ankerpfad bekommt. Die Freigabe erfolgt
dann, wenn die Zentrierung ihr Ziel geschrieben hat oder endgültig aufgibt.

**Entschieden: entfallen.** Die Messung zeigte weder (a) noch (b) allein,
sondern beides — und ein Halt hilft gegen (b) nicht, weil er selbst aus einem
Idle korrigiert und damit ein zweiter sichtbarer Schritt ist. An seine Stelle
tritt die Verankerung auf einer Zeilenkante (siehe „Stand" oben). Der Rest
dieses Abschnitts steht als Protokoll dessen, was geprüft wurde.

**Ursprünglicher Vorbehalt:** zeigt die Messung Fall (a), ist Task 3
die ganze Ursache — dann entfällt Task 4 ersatzlos und der Plan vermerkt das.
Zeigt sie Fall (b), ist Task 4 zwingend. Beachten: zwei Holds auf einem
Adjustment sind eine Schlägerei, kein doppelter Schutz
(`adjustment_hold.rs:12-28`) — es darf nur der eine geben, der schon existiert.

### Task 5 [erledigt] — SEARCH-16 benennt den Zwischenzustand

`docs/ux-rules.md`, SEARCH-16 (`:3054`) bekommt einen Satz: die Wiederherstellung
ist nicht als Zwischenposition sichtbar — der Blick landet in einem Zug am Ziel.
Revisionsvermerk wie beim bestehenden „Revised 2026-08-14". Der Test aus Task 2
trägt danach den Namen `search_16_*`. Regel bleibt `[active]`, also muss der
Test im selben Commit grün sein.

### Task 6 [erledigt als NAV-19] — Der Seitenleisten-Wechsel zentriert den laufenden Titel

Beim Wechsel der Quelle über die Seitenleiste (`TrackList::set_source` →
`restore_browser_place`, `track_list.rs:459-466`) wird der laufende Titel
zentriert, sofern er in der neuen Ansicht vorkommt — mit `RevealMotion::Instant`
über denselben Pfad aus Task 3. Kommt er nicht vor, bleibt die gemerkte Position
dieser Ansicht unverändert.

**Grenzen, hart:**

- **Vor/Zurück nicht.** `restore_browser_place` bedient auch die
  Verlaufsnavigation (`window/library_shell.rs:343,350`). Dieser Weg bleibt
  unverändert, BROWSE-2 („Back/Forward restores exactly") bleibt gültig. Der
  Anlass muss also **an der Aufrufstelle** unterschieden werden, nicht in
  `view_session::restore_browser_place` geraten.
- **Nie navigieren.** Die Zentrierung wechselt niemals den Reiter oder die
  Ansicht, um etwas zeigen zu können. Sie wirkt nur in der Ansicht, die gerade
  sichtbar wird.
- **`reveal_policy` bleibt unangetastet** (Beschluss 2) — dies ist ein eigener
  Anlass, keine Änderung an den bestehenden.

**Regelarbeit:** eine neue Regel in `docs/ux-rules.md` neben NAV-10b, die den
Gleichzug mit SRC-13 für die Trackliste ausspricht und Vor/Zurück ausdrücklich
ausnimmt. IDs sind append-only. Sie wird `[active]` im selben Commit, der sie
umsetzt und mit einem `<id>_*`-Display-Test belegt.

### Nicht in diesem Plan

**Sektionshöhen im Zielwert.** `centered_track_scroll_target` rechnet reine
Zeilenmathematik, obwohl `apply()` die Sektionszahl kennt und der Queue-Anlass
sektioniert ist — Kopfzeilen gehen nicht in den Zielwert ein, anders als im
Ankerpfad (`list_geometry_layout::headers_above_in`). Eigener Fehler, eigenes
TODO: `docs/plans/queue-centering-ignores-section-headers.md` anlegen (phase
`todo`), nicht hier beheben. Falls Task 3 die Stelle ohnehin anfasst, im TODO
vermerken, dass der Umbau sie berührt hat.

## Akzeptanz

- **Kontrollarm zweistufig vor dem Fix** (Task 2), einstufig danach — für beide
  Anlässe (Suche leeren, App-Start).
- Grün bleiben: `search_16_clearing_after_a_play_centers_the_loaded_track`,
  `search_16_clearing_without_a_play_returns_to_the_pre_search_place`,
  `search_16_a_result_set_that_fits_still_centers_after_clear_all`,
  `typed_search_reads_from_the_top_and_clearing_comes_back`,
  `fil_9_filter_change_centers_playing_track_in_new_results`,
  `nav_10b_*`, `start_restore_tests`.
- Glide bleibt Glide: `current_track_selection_glide_tests.rs`,
  `glide_reload_display_tests.rs`, `delete_follow_display_tests.rs` grün.
- Vor/Zurück unverändert: die BROWSE-2-Tests grün, ohne Anpassung. Eine
  Anpassung dort ist ein Fehlschlag von Task 6, kein Ergebnis.
- **Mutationsnachweis, gelaufen 19.08.2026** (jeweils genau ein Vorkommen
  getauscht, Rücknahme über `git checkout --` im `trap` — **erst committen,
  dann mutieren**, sonst wirft der Trap uncommittete Arbeit weg):
  - `RevealMotion::Instant` → `Glide`: Kontrollarm rot mit
    `gtk 6460.0 / glide.instant 2923.5 / gtk 6460.0`, also exakt die alte
    Messung; der Endwerttest fällt mit.
  - Kantenwahl → exakter Mittelwert (`Some((anchor, centre))`): Kontrollarm rot
    mit zwei Schritten (2927.0 → 2924.0), **Endwerttest bleibt grün** — genau
    der blinde Fleck, für den der Kontrollarm existiert.
  - NAV-19: `center_playing_track_in_view` am Aufrufort entfernt — der positive
    Fall rot, der negative (Ansicht ohne den Titel) grün.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, Display-Suite (`scripts/check-display-tests.sh`).
  **Nicht** `scripts/check-merge-readiness.sh` — das Sammel-Gate läuft nie
  durch und hat in zwei Sitzungen je zwei Stunden gekostet. Ebenso keine
  Android-/Gradle-/uniffi-Läufe, die Änderung braucht sie nicht.

## Parallelität

**Ein Strang. Der Plan wird nicht geschnitten.**

Grund: Tasks 1–4 und 6 fassen alle `centered_scroll_restore.rs` und
`track_reveal.rs` an — es gibt keine disjunkte Dateigruppe. Task 2 ist
zusätzlich **Vorbedingung** für Task 3 und für die Entscheidung über Task 4;
Task 6 setzt den fertigen Pfad aus Task 3 voraus. Ein zweiter Strang wäre nur
`docs/ux-rules.md` — Text, dessen Regel-Kennung die Tests der anderen Tasks
tragen. Das ist keine Parallelität, sondern eine Abhängigkeit in zwei Dateien.

**Reihenfolge:** 1 → 2 (Kontrollarm) → 5 (Kennung) → 3 → ggf. 4 → 6.

**Dateibesitz dieses Strangs:**
`crates/reprise-gnome/src/ui/track_list/centered_scroll_restore.rs`,
`crates/reprise-gnome/src/ui/track_list/track_reveal.rs`,
`crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`,
`crates/reprise-gnome/src/ui/track_list/track_list.rs`,
`crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs`,
`crates/reprise-gnome/src/ui/scroll_probe.rs`,
`crates/reprise-gnome/src/ui/window/library_shell.rs` (nur die zwei
Aufrufstellen der Verlaufsnavigation, lesend/abgrenzend), `docs/ux-rules.md`.

**Der parallele Nachbar ist ein eigener Plan:**
`stats-hide-more-top-artists-stutters` fasst nur
`crates/reprise-gnome/src/ui/stats/stats_bands_card.rs` an — mit diesem Strang
disjunkt, eigener Worktree, beliebige Reihenfolge, kein gemeinsamer Merge-Zwang.

**Post-Merge-Querprüfungen:** keine — es gibt keinen zweiten Strang, dessen
Ergebnis dieser lesen müsste.
