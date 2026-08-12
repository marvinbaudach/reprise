---
slug: dev-green-display-tests
worktree: /home/marvin/Projects/reprise-dev-green-display-tests
branch: feature/dev-green-display-tests
phase: shipped
codex_session:
created: 2026-08-11
---
# origin/dev wieder grün: Display-Tests

Von 424 regelbenannten Display-Tests sind auf `origin/dev` (4f6dfc7cb2) 13 rot.
Einzeln nachgefahren (eigener Xvfb je Test, drei Versuche) bleiben **11** rot;
`stats_19_period_switch_tweens_bars_without_restarting_static_content` und
`fb_9_chip_end_inset_is_measured_from_the_header_title_buttons` sind einzeln
grün und waren Rudel-Effekte der Parallelität — die nicht anfassen.

Motion- und CSS-Display-Tests sowie die Runtime-Service-Bus-Tests sind grün.

Die elf zerfallen in vier unabhängige Baustellen. Jede ist einzeln
verifizierbar; Reihenfolge egal.

Einen einzelnen Display-Test fährt man so:

```
env GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  GIO_USE_VFS=local GTK_USE_PORTAL=0 \
  dbus-run-session -- xvfb-run --server-num=4990 \
  cargo test -p reprise-gnome <voller::test::pfad> -- --ignored --exact
```

`scripts/check-display-tests.sh` **nicht** für einzelne Tests benutzen — es
fährt die ganze Suite. `scripts/check-merge-readiness.sh` gar nicht starten.

## Baustelle 1: Accessible-Rolle `Link` fehlt an Metadaten-Links

Rot:
- `ui::now_playing::surface::tests::browse_4_now_playing_metadata_exposes_track_album_and_artist_links`
  — `assertion failed: gtk4::test_accessible_has_role(&surface, AccessibleRole::Link)`
- `ui::stats::stats_metadata_links::tests::stats_22_metadata_links_are_compact_keyboard_links`
  — `left: Label, right: Link`

Ursache (belegt): Commit `7bae41183a` „Deleting tracks feels immediate again
(#366)" hat `widget.set_accessible_role(gtk4::AccessibleRole::Link)` aus
`crates/reprise-gnome/src/ui/link_activation.rs`, Funktion `present()`,
entfernt. Der Grund dafür ist gut: GTK verweigert den Rollenwechsel an einem
bereits realisierten Widget, und in einer recycelnden `ColumnView` erzeugte der
Rollen-Swap pro Bind nur noch `Gtk-CRITICAL`-Spam. Ersatz sollte sein, dass
Widgets ihre Rolle einmalig im `class_init` deklarieren — nachgezogen wurde das
aber nur für `track_list::track_cover::TrackCover`.

`now_playing.rs` und `stats_metadata_links.rs` benutzen gar keine eigene
GObject-Subklasse, sondern schlichte `gtk4::Label`/`gtk4::Image`. Dort blieb
nach dem Umbau nichts übrig, das die Rolle setzt.

Die Tests haben recht: `docs/ux-rules.md` ACC-2 [active] verlangt „the matching
role" für jedes interaktive Element, BROWSE-4 [active] nennt Track/Album/Artist
im Now-Playing-Panel ausdrücklich als Navigationsziele, STATS-22 [active]
verlangt für die Songs-Card „its two labels lead into the library … link color
and underline on hover".

Reparatur — die Rolle bei der **Erzeugung** setzen, dort wird das Widget
genau einmal gebaut (kein recycelter Cell-Bind, das Realize-Problem greift
nicht):

- `crates/reprise-gnome/src/ui/stats/stats_metadata_links.rs`, in `link()`
  direkt nach dem `gtk4::Label::new(...)`
- `crates/reprise-gnome/src/ui/now_playing/now_playing.rs`, je einmal nach der
  Erzeugung von `cover`, `title`, `artist` und `album`

`present()`, `unpresent()` und `arm()` in `link_activation.rs` bleiben
**unangetastet** — sonst kehrt der `Gtk-CRITICAL`-Spam an `TrackCover` zurück.

Beide Stellen sind nötig; eine allein macht nur einen der zwei Tests grün.
Prüfe beim Durchgehen, ob es weitere Aufrufer von `arm`/`present` gibt, die
dieselbe verwaiste Stelle haben — der Umbau hat sie alle betroffen, nachgezogen
wurde nur einer.

## Baustelle 2: Podcast-Gruppenkopf — Tests hängen an der alten flachen Struktur

Rot:
- `ui::podcasts::podcasts_groups::tests::src_11_group_header_stays_on_the_fallback_when_images_are_not_allowed`
  — Panik `source image stack` in `podcasts_groups_tests.rs:638`
- `ui::podcasts::podcasts_groups::tests::src_4b_the_group_header_offers_no_second_unsubscribe_control`
  — `assertion failed: header.last_child().and_downcast::<gtk4::MenuButton>().is_some()`

Ursache (belegt): `group_header()` in
`crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs` baut den Kopf über
`crate::ui::source_row::skeleton()`. Der liefert drei Kinder — `media`,
`identity`, `trailing` — statt der früher flachen Anordnung. `first_child()`
ist heute die `media`-Box (der Artwork-`Stack` steckt im `CenterBox` darin),
`last_child()` ist die `trailing`-Box (die `facts`-Label **und** den
`MenuButton` enthält).

Der Umbau kam mit `b7a312792b` „Podcasts and YouTube episode rows share one
grammar (#285)". Dort wurde der Nachbartest `src_12a…` an die neue Struktur
angepasst, diese beiden aber vergessen — sie sind `#[ignore]`d und laufen nur
unter Xvfb, deshalb blieb es liegen.

Hier hat der **Produktionscode** recht: `docs/ux-rules.md` SRC-16 [active]
schreibt die Kapselung ausdrücklich vor („a fixed 64 × 40 media column, one
identity box and one trailing box").

Reparatur in `podcasts_groups_tests.rs`: statt direkt auf
`first_child()`/`last_child()` zu greifen, die im selben Modul bereits
vorhandene Hilfsfunktion `descendants()` benutzen und im Baum nach dem
gesuchten Widget suchen. Die Aussage der Tests bleibt exakt dieselbe — `src_4b`
prüft weiterhin, dass es **genau einen** Unsubscribe-Zugang gibt, nicht bloß
mindestens einen. Wenn `descendants()` das nicht hergibt, zähle die Treffer.

## Baustelle 3: Preferences-Gegenprobe misst an einer verkleinerten Karte

Rot:
- `ui::preferences::preferences_window::chrome_placement_tests::fb_9_counterprobe_legacy_toolbar_status_moves_the_content`
  — `the retired in-flow status path must reproduce its layout jump (measured 62 px, floor 80 px)`

Der Nachbartest `fb_9_chip_end_inset_is_measured_from_the_header_title_buttons`
ist einzeln grün und bleibt unangetastet.

Ursache (belegt): Die Gegenprobe misst den echten `.scan-card`-Stil. Dieser
wurde mit `78bcee1147` „The Library Doctor matches releases, not recordings
(#376)" bewusst verschlankt (u. a. `margin: 8px 4px 0 4px` → `margin: 0 4px`,
neue `JOB_CARD_HEIGHT_PX = 70`, konsistente Kartenfamilie). Die Karte ist
seither ~62 px statt ~88 px hoch. Der Floor
`RETIRED_TOP_BAR_MIN_JUMP_PX = 80.0` in
`crates/reprise-gnome/src/ui/preferences/preferences_chrome_placement_tests.rs`
stammt unverändert aus der Einführung des Tests (`32b5e66e23`).

Der Mechanismus ist intakt: die vorangehende Assertion `jump == card_height`
hält. Nur die hartkodierte Untergrenze passt nicht mehr.

Reparatur: `RETIRED_TOP_BAR_MIN_JUMP_PX` auf einen Wert unterhalb der heutigen
Kartenhöhe senken, der weiterhin deutlich über 0 liegt, und den Kommentar
darüber (er nennt noch „88 px") auf den heutigen Stand bringen — inklusive der
Angabe, warum die Karte kleiner wurde. Den Wert nicht raten: die gemessene
Höhe steht in der Fehlermeldung des Tests, und `jump == card_height` liefert
sie ohnehin.

Wichtig: die Gegenprobe darf nicht zur Tautologie werden. Sie existiert, um zu
beweisen, dass der Nachbartest überhaupt etwas misst. Ein Floor von 0 oder ein
Vergleich, der immer hält, entwertet beide Tests.

## Baustelle 4: die Zentrierung wird nie angewendet

Rot:
- `ui::track_list::current_track_selection::start_restore_tests::start_3_loaded_track_is_selected_centered_and_marked_paused`
  — `a normal start must center the loaded track: actual 2040, expected 1937.5`
- `ui::track_list::current_track_selection::tests::fil_9_filter_changes_center_the_visible_playing_track`
  — `filter change must center playing track 51: actual 680, expected 577.5`

Ursache (belegt, und die Rechnung geht exakt auf): Die „actual"-Werte sind
genau `position * 34`, also GTKs eigener `scroll_to`-Rohwert — Zeile oben
ausgerichtet, ohne die halbe Zeilenmitte und ohne `page_size / 2`. Aus beiden
„expected"-Werten folgt dieselbe `page_size = 239`. Die Differenz von 102,5 px
ist damit exakt der fehlende `page_size/2`-Term (119,5) minus die fehlende
halbe Zeile (17). Anders gesagt: **die Verfeinerung läuft nie**, die Formel in
`crates/reprise-gnome/src/ui/scroll_center.rs` ist unverändert korrekt.

Der Bruch kam heute mit `744f8d953b` „fix(gnome): keep scroll anchor writes out
of GTK's allocation pass". Der Commit ist inhaltlich richtig: ein synchrones
`adjustment.set_value(...)` **innerhalb** einer `changed`-Emission re-entriert
GTKs laufende Allocation, der `GtkListItemManager` behält dann die alten
gebundenen Zeilen und fordert nie eine neue Allocation an. Deshalb wurde
`schedule_scroll_restore` in
`crates/reprise-gnome/src/ui/track_list/track_list_reload.rs` sauber auf
`list_geometry_changed::after_changed_once` umgestellt, samt
`debug_assert!(!in_changed_emission())` in `apply_scroll_anchor_if_allocated`
und einem Generation-Guard gegen zwischenzeitliche Reloads.

Die strukturgleiche Schwesterfunktion `schedule_centered_scroll_restore` /
`apply_centered_scroll_refinement` — genau der Pfad beider roten Tests — wurde
dabei **nicht** mitgenommen. Am Diff sieht man es: dort wurde nur der Modulpfad
nachgezogen (`ui::list_geometry::on_changed_once` →
`ui::list_geometry_changed::on_changed_once`), der Aufruf blieb das synchrone
`on_changed_once`. `apply_centered_scroll_refinement` schreibt weiterhin ein
rohes `adjustment.set_value(value)` aus der Emission heraus und verliert die
Verfeinerung dadurch systematisch — sie landet zuverlässig auf genau dem
„stale upper"-Frame, den der Modulkommentar in `ui/list_geometry.rs` selbst als
bekannte Falle beschreibt.

Die beiden Testdateien wurden seit ihrer Einführung nicht angefasst; die
Erwartung ist unverändert. Hier hat der **Produktionscode** unrecht.

Reparatur: `schedule_centered_scroll_restore` auf `after_changed_once`
umstellen, exakt analog zu `schedule_scroll_restore` in derselben Datei — inkl.
des `debug_assert!(!in_changed_emission(), …)` in
`apply_centered_scroll_refinement`, das die Schwesterfunktion schon hat.

Eine Änderung macht beide Tests grün. Prüfe im selben Zug, ob es in dieser
Datei oder ihren Nachbarn **weitere** Aufrufer von `on_changed_once` gibt, die
aus dem Callback heraus `set_value` schreiben — `744f8d953b` hat einen von zwei
Schreibern migriert, es können mehr sein. Jeder gefundene Fall gehört mit
umgestellt und im Commit benannt. Eine dritte Fundstelle ist bereits bekannt:
`crates/reprise-gnome/src/ui/track_list/view_state_memory.rs`,
`restore_scroll_when_ready` ruft ebenfalls noch `on_changed_once` und schreibt
synchron. Sie hängt an keinem der roten Tests (sie betrifft BROWSE-2 /
Back-Forward), gehört aber zum selben Muster — mit umstellen.

## Baustelle 5: Anker-Drift zwischen Erfassen und Wiederherstellen

Rot:
- `ui::track_list::tag_mutation_refresh::display_tests::tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`
  — `the edited album moved out of the viewport after its year changed`
- `ui::track_list::track_list_reload::display_tests::tag_1_query_reloading_metadata_save_keeps_the_live_viewport`
  — `rating save moved the viewport: before=1835, after=1802`
- `ui::delete_tracks::large_block_display_tests::browse_11_large_block_delete_keeps_the_deep_viewport_off_the_top`
  — `expected=13395, row height=34; samples(n=73 first=Some(13395.0) min=13157 max=13395)`

Alle drei laufen über `capture_reload_anchor` → `reload_with_anchor(_and_viewport)`
→ `restore_reload_anchor` → `apply_scroll_anchor_if_allocated` in
`crates/reprise-gnome/src/ui/track_list/track_list_reload.rs` und lösen den
Anker über `ListGeometry` in `crates/reprise-gnome/src/ui/list_geometry.rs` auf.

Herkunft: derselbe Zusammenhang wie Baustelle 4. Der ganze Branch
`feat/list-geometry-service` (12 Commits, `2a719c1a6c` bis `744f8d953b`) ist
heute auf dev gelandet. Der Test für `browse_11` wurde eine Minute **vor** dem
ersten Branch-Commit angelegt, um genau dieses Verhalten zu charakterisieren —
sein eigener Kommentar nennt sich „counterprobe … after the geometry-service
track provides the switch". Der Branch bekennt sich also selbst zu einem noch
offenen Punkt.

**Die folgende Mechanik ist eine begründete Hypothese aus der Codelektüre, nicht
gemessen.** Behandle sie als Startpunkt, nicht als Befund: reproduziere zuerst,
bestätige oder verwirf sie, und richte die Reparatur nach dem, was du misst.

Hypothese: `ListGeometry::observed_row_height` liefert einen gecachten Wert, der
„Assumed" (CSS-Minimum) oder „Measured" (reale Höhe) sein kann und nur bis
`ROW_HEIGHT_AGREEMENT_EPSILON = 0.5` gegen `upper()/n_rows` abgeglichen wird.
Erfassung und Wiederherstellung des Ankers laden diesen Wert **unabhängig
voneinander** neu; dazwischen läuft die Query. Eine Restdifferenz von bis zu
0,5 px je Zeile summiert sich über die Scrolltiefe: bei `before=1835` sind das
~54 Zeilen, und die beobachtete Differenz beträgt 33 px ≈ eine Zeilenhöhe.
Bei `browse_11` sieht es anders aus — dort ist der Endwert richtig
(`first=13395`, `max=13395`), nur zwischendurch fällt er auf 13157. Das ist
ein transienter Einbruch während der Übernahme, kein falscher Endzustand:
`ListGeometry::configure` schreibt `adjustment.configure(target, …)`
unbedingt, auch solange die Höhe noch „Assumed" ist; erst der verzögerte
zweite Durchlauf korrigiert.

Zwei Ansatzpunkte, beide zuerst prüfen, dann erst umsetzen:
- In `ListGeometry::configure` den *Bereich* (upper) vorseeden, den *Wert*
  aber erst schreiben, wenn die Höhe für diesen Aufruf settled ist.
- Erfassung und Wiederherstellung denselben `RowHeight` durchreichen lassen,
  statt ihn auf beiden Seiten neu aus dem Cache zu laden — dann müssen sich
  zwei Zahlen nicht mehr auf 0,5 px einigen.

Die Tests haben recht: `browse_11` fordert ausdrücklich keine Gleichheit,
sondern eine Toleranz von zwei Zeilen; gemessen sind sieben. Die Toleranz
**nicht** aufweichen — sie ist die Aussage des Tests.

Da dieser Code heute erst gelandet ist: sieh dir `git log` des Branches an,
bevor du eingreifst. Wenn eine der zwölf Änderungen einen halb fertigen
Zwischenstand hinterlassen hat, ist ihn zu Ende zu führen die richtige
Reparatur — nicht, drumherum zu bauen.

## Baustelle 6: Fokus-Restore hält an einer recycelten Zeile fest

Rot:
- `ui::track_list::tag_mutation_refresh::display_tests::tag_1_restoring_dialog_focus_after_a_save_keeps_the_viewport`
  — `restoring the dialog's focus moved the viewport: before=6656, after=7982`

Eigener, älterer Pfad — **nicht** die Geometrie. Der Test bestätigt vor dem
eigentlichen Fehler ausdrücklich, dass der Save-Refresh den Ausschnitt korrekt
zurückgestellt hat; erst das Fokus-Restore danach verschiebt ihn.

Ursache: `TransientFocusGuard::restore()` in
`crates/reprise-gnome/src/ui/transient_focus.rs` ruft `row.grab_focus()` auf
einer vor dem Speichern erfassten Zeilen-Widget-Referenz. Abgesichert ist das
nur über den Vergleich der **Zeilenanzahl** (`rows_at_capture`). Ein
Artist-Edit ändert die Zeilenzahl nicht (300 == 300), löst aber ein
vollständiges `items_changed(0, 300, 300)` aus — und GTK recycelt Row-Widgets
bei einem Full-Reset unabhängig von der Zeilenzahl. Der Guard greift also
genau dann nicht, wenn er müsste. Der Doc-Kommentar in derselben Datei
beschreibt dieses Recycling bereits als bekannte Ursache („regularly threw the
library to the top"); der Zähl-Guard aus `1e2de9c6b8` (#351) deckt den Fall
nicht ab.

Reparatur: `restore()` muss zusätzlich sicherstellen, dass die gemerkte Zeile
noch dieselbe Zeile ist — etwa indem die Track-ID statt (oder neben) der
Zeilenzahl mitgeführt und beim Restore gegengeprüft wird, oder indem die
gecachte Zeilen-Referenz bei jedem vollständigen Reset aktiv verworfen wird.
`row.is_visible()` plus Zeilenzahl reicht nachweislich nicht.

Wenn die Zeile nicht mehr sicher identifizierbar ist, ist „gar nicht fokussieren"
das richtige Verhalten — ein verlorener Fokus ist deutlich billiger als ein
springender Ausschnitt, und genau das ist die Aussage des Tests.

## Abnahme

Alle elf Tests einzeln grün, jeder mit dem Einzelbefehl oben. Danach einmal
die ganze regelbenannte Suite:

```
DISPLAY_TEST_JOBS=2 scripts/check-display-tests.sh --rule-named
```

Sie muss `failed: 0` melden. Falls einzelne Tests nur im Rudel rot sind, sie
einzeln nachfahren, bevor du sie dir zuschreibst — `stats_19` und
`fb_9_chip_end_inset` sind dafür die bekannten Kandidaten.

Zusätzlich, weil die Reparaturen Produktionscode anfassen:

```
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace --exclude reprise-platform-linux
```
