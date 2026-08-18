---
slug: jump-to-playing-source-item
worktree: ~/Projects/reprise-jump-to-playing-source-item
branch: feature/jump-to-playing-source-item
phase: shipped
codex_session:
created: 2026-08-04
---
# Plan: Zum laufenden Quellen-Element springen

Umsetzung von `docs/superpowers/specs/2026-08-04-jump-to-playing-source-item-design.md`.
Alle Grundsatzfragen sind entschieden; dieser Plan ist die alleinige Anweisung.

**Basis: `origin/dev` @ `8b69f127fba5478f83473897356c96660eb3cf72`.** Jede Zeilenangabe
unten bezieht sich auf diesen Stand. Der lokale Hauptcheckout liegt 203 Commits
zurück und enthält Teile des hier beschriebenen Codes gar nicht — Quelltext
ausschließlich über den Worktree auf `origin/dev` lesen, niemals gegen `main`
prüfen.

---

## 0. Was gebaut wird, in einem Absatz

Läuft eine Podcast-Folge, ein YouTube-Video oder ein Radiosender, sind Titel,
Kanalzeile und Cover der Player-Leiste tote Flächen: alle drei hängen an
`reveal_playing_track` / `…_album` / `…_artist`, und die steigen bei
`current_track_id() == None` aus — was in jedem externen Modus der Fall ist,
weil `prepare_external_playback` `now_playing` vorher auf `None` setzt
(`external_media.rs:160`). Die Aufdeck-Mechanik in den Quellenansichten
existiert vollständig (`SRC-13`); es fehlt nur der Einstieg. Wir bauen: zwei
neue Navigations-Intents im Core, einen Accessor am `PlayerController`, einen
Kanal-Reveal in der Podcast-Liste, eine modusabhängige Beschriftung aus einer
einzigen Tabelle, und eine neue Regel (`PLAY-12`), die künftige
Wiedergabequellen zu drei landenden Links verpflichtet.

---

## 1. Ausgangslage (belegt gegen `origin/dev`)

### 1.1 Der Einstieg fehlt tatsächlich

`window_runtime_wiring.rs:266–288` — `reveal_playing_track` steigt bei
`player.current_track_id() == None` aus (`:273–275`). Dasselbe gilt für
`reveal_playing_album` (`:296–298`, `current_album_identity()`) und
`reveal_playing_artist` (`:315–317`, `current_artist_identity()`). Alle drei
Accessoren lesen `self.now_playing` (`album_identity.rs:8, 15, 28, 41`).

Die Verdrahtung ist genau eine Stelle: Cover (`:329`), Titel (`:333`),
Interpret (`:337`), Info-Panel (`:341, :345, :349`), `jump-to-now-playing` /
`Ctrl+L` (`:351–354`).

### 1.2 Kein `PodcastController`

Es gibt keinen. `playback_mode`, `play_external`, `begin_podcast`,
`begin_radio` sind allesamt `impl PlayerController` (`external_media.rs:28`),
`album_identity.rs:5` ebenso. Der neue Accessor gehört an `PlayerController`.

### 1.3 `PodcastKind` liegt beim Sitzungsstart schon vor — ohne DB-Query

`begin_podcast` berechnet `kind` bereits (`external_media.rs:207–209`) und
`subscription_id` (`:210`) aus der `EpisodeRow`, die
`play_external_with_context_and_origin` ohnehin lädt (`:122`). In
`PodcastSession` landet nur `subscription_id` (`external_media_state.rs:235`),
`kind` wird weggeworfen. Der Kaltstart-Pfad hat es ebenfalls:
`restored_session` baut die Sitzung aus einer vollen `EpisodeRow`
(`external_media_session.rs:40–58`), `EpisodeRow.kind` existiert
(`podcasts.rs:86`). ⇒ `kind` mitzuführen ist eine Ein-Zeilen-Ergänzung an zwei
Konstruktionsstellen, kein DB-Query.

### 1.4 Der Router

`NavigationIntent` (`navigation.rs:32–52`) hat 7 Varianten. Exhaustive
`match`-Arme darauf gibt es nur zwei:

- `navigation.rs:139–195` (`BrowserNavigation::navigate`) — wird erweitert.
- `metadata_navigation.rs:17–36` (`normalize_catalog_intent`) — hat einen
  `other => Some(other)`-Arm (`:35`), bricht also **nicht**.

Alle übrigen Fundstellen (`nav_history.rs`, `window.rs`,
`window_action_wiring.rs`) *konstruieren* Intents, sie zerlegen sie nicht.

### 1.5 Zwei Eigenheiten des Routers, die den Entwurf bestimmen

**(a) `go_metadata_scope` liefert in der bereits offenen Ansicht `None`.**
`navigation.rs:221–224`: `if self.current == target { return None; }`.
`BrowserPlace::Podcasts`, `::Youtube`, `::Radio` sind Unit-Varianten — steht
man schon dort, greift das immer. Es gibt **keinen** `Replace`-Übergang zu
beobachten, und das wird nicht umgebogen. Der Reveal-Auftrag wird deshalb
*unabhängig vom Übergangsergebnis* erteilt (siehe AP9).

**(b) `MetadataNavigator::navigate` bricht bei `None` ab**
(`metadata_navigation.rs:83–88`). Genau der Fall „Nutzer steht schon in der
Liste“ — der Fall, um den es geht. Auch das behebt AP9.

**(c) `same_destination` kennt `BrowserPlace::Youtube` nicht**
(`navigation.rs:322–330`: `ImportErrors`, `MyStats`, `Releases`, `Concerts`,
`Podcasts`, `Radio`, `Conversions` — kein `Youtube`). Heute folgenlos, weil
`==` vorher greift. Wird in AP1 als eigener Commit mitgefixt.

### 1.6 Der Weg in die Zielansicht existiert

`route_to_place` (`library_shell.rs:265–299`) routet Nicht-Scope-Quellen über
`sidebar.refresh_and_select(source, reason)` (`:289`), was `on_select`
(`library_shell.rs:185–234`) auslöst; dort ruft `:218–226`
`podcasts_view.refresh()` bzw. `youtube_view.refresh()` / `radio_view.refresh()`
und schaltet den Content-Stack. Ein *Reveal-Auftrag* wird dabei nirgends
transportiert — das ist die Lücke, die AP7/AP9 schließen.

### 1.7 Reveal-Mechanik

`source_reveal.rs:14–25` (`LoadedItemChange`, 4 Varianten), `:49–57`
(`reveal_policy`), `:38` (`USER_SCROLL_GRACE = 1500 ms`).

Podcast-Seite: `podcasts_view_marker.rs:50–107` (`reveal_loaded_episode`) —
Policy (`:51–54`), Ziel aus `playing_episode` (`:55`), gefilterte Gruppen
(`:60–65`), Gruppe/Fenster aufklappen (`:78–90`), Zeilen-Widget aus
`download_widgets` (`:94–98`), `center_row` (`:102–106`).
`install_reveal_tracking` (`:30–45`) hängt `ViewEntered` an `root.connect_map`
(`:40–44`).

`podcasts_reveal.rs:38–52` (`reveal_target`), `:59–70` (`centered_value`),
`:121–147` (`center_row` — `idle_add_local_once` + Tick-Callback,
`MAX_LAYOUT_FRAMES = 60` `:22`), `:87–117` (`apply` — MOT-7-Gate, Sprung statt
Animation bei ausgeschalteter Animation).

Radio: `radio_reveal.rs:67–91` (`reveal`), `:101–109` (`on_external_change`),
`:142–169` (`install`, `ViewEntered` an `root.connect_map` `:163–167`),
`:34–40` (`station_position`), `:30–32` (`connected_station`).
**Wichtig:** `reveal` deckt ausschließlich `connected_station(&self.live)` auf
(`:72`) — eine beliebige `station_id` kann es nicht. Deshalb heißt der neue
Eingang `request_reveal_connected()` ohne ID-Parameter.

### 1.8 Kopfzeilen sind heute nicht adressierbar

`podcasts_groups.rs:49–52` — `RenderedRowWidgets { downloads, selection }`.
`build_group` (`:123–214`) baut die Kopfzeile in `group_header` (`:150–159`)
und hängt sie per `expander.set_label_widget(Some(&header))` (`:160`) an; das
Widget wird nirgends behalten. `podcasts_view.rs:113–114` hält die beiden
Maps, `:401–402` füllt sie pro `render()`.

Der `Expander` selbst ist im **aufgeklappten** Zustand kein brauchbares
Zentrierziel: seine Höhe umfasst alle Episodenzeilen. Ob das Header-Widget im
**eingeklappten** Zustand eine brauchbare Höhe meldet, ist ungeprüft — das
klärt AP5 empirisch, bevor es die Mechanik festzurrt.

### 1.9 Filter

Podcasts: `podcasts_presentation.rs:22` (`type PodcastFilter =
reprise_core::podcasts::config::PodcastFilterConfig`), `:257–261`
(`matches_filter`), `:263–268` (`apply_filter`), `:270–272` (`active` — beachtet
nur `unplayed_only` und `downloaded_only`), `:107–127` (`rendered_source_groups`
— eine Gruppe, deren Episoden alle wegfiltern, verschwindet ganz, aber nur wenn
`active(filter)`).
`PodcastFilterConfig { unplayed_only, source, downloaded_only }`
(`core/podcasts/config.rs:163–167`). Die Filterschlüssel sind global, nicht pro
`kind` (`config.rs:218–225`) — Podcasts- und YouTube-Ansicht teilen sich einen
persistierten Filter.
`PodcastsFilterBar::clear_all` (`podcasts_filter_bar.rs:155–157`) → privates
`apply` (`:159–171`): persistiert via `podcasts::config::save_filter` (`:160`),
baut die Chips neu und ruft **synchron** `on_changed` (`:167–170`) →
`podcasts_view.rs:336–339` → `render()`.

Radio hat die Facettenstruktur schon: `radio_filter_bar.rs:31–34`
(`RadioFilterFacet { Genre, Country }`), `:36–43` (`remove_filter`), `:46–53`
(`filter_rows`), `:204–206` (`clear_all`), `:229–239` (privates `apply`,
persistiert ebenfalls), `radio_view.rs:383–392` (`render_rows`).

### 1.10 Player-Leiste

`player_bar_layout.rs:85–92` (Cover-Button: Tooltip + Accessible-Label
`REVEAL_PLAYING_ALBUM`), `:110–117` (Titel-Button: `JUMP_TO_NOW_PLAYING`,
Tooltip + Label), `:126–130` (Interpret-Button: **nur** Tooltip
`GO_TO_PLAYING_ARTIST`, kein Accessible-Label).

`player_bar_cover.rs:43–54` (`set_track`) — schreibt Accessible-Labels für
Titel und Interpret, setzt `artist_button.set_sensitive(!artist.trim().is_empty())`
und überschreibt bei jedem Trackwechsel das Cover-Label mit
`REVEAL_PLAYING_ALBUM`. `clear_track` (`:125–136`) setzt nur
`artist_button.set_sensitive(false)`.

Aufrufer von `PlayerBar::set_track`: `now_playing_wiring.rs:153`
(`PlayerController::sync_track` — der einzige Bibliotheks-Fanout) und
`player_bar_external.rs:30` (`set_external_snapshot`). `CompactPlayer` hat ein
eigenes, gleichnamiges `set_track` (`compact_player.rs:214`) und **keine**
Links — nicht anfassen.

Info-Panel: `now_playing.rs:388–405` armiert vier Flächen über
`link_activation::arm_slot(widget, label, slot)`; die Beschriftung wird dort
einmalig beim Bau gesetzt (`GO_TO_PLAYING_ALBUM`, `REVEAL_PLAYING_TRACK`,
`GO_TO_PLAYING_ARTIST`). `link_activation.rs:24–54` (`arm`) setzt
`accessible::Property::Label`, aber keinen Tooltip.

Strings: `strings.rs:472–473` (`JUMP_TO_NOW_PLAYING`, `GO_TO_PLAYING_ARTIST`),
`strings_app_shell.rs:7` (`REVEAL_PLAYING_ALBUM`). `Ctrl+L` in der Hilfe:
`help.rs:47–48`.

**Quelltext-Test:** `player_bar_layout_tests.rs:296–305`
(`tip_1d_player_bar_artist_names_its_navigation_action`) behauptet, der
Literal-String `".tooltip_text(strings::text(strings::GO_TO_PLAYING_ARTIST))"`
komme in `player_bar_layout.rs` genau einmal vor. Er wird umgeschrieben (AP4),
nicht umgangen.

### 1.11 Der Modus ist bereits ein Enum

`playback/preview.rs:10–18`:

```rust
pub(in crate::ui) enum PlaybackMode { Queue, QueuedEpisode, Preview, Podcast, Radio }
```

abgeleitet in `external_media_state.rs:331–341` aus Sitzung und
`preview_path`, gelesen über `PlayerController::playback_mode()`
(`external_media.rs:29–31`). **Das ist der Angelpunkt für die neue Regel
`PLAY-12`**: ein Test kann über dieses Enum iterieren, und eine künftige
Wiedergabequelle, die eine Variante hinzufügt, bricht die exhaustive
Zuordnungstabelle beim Kompilieren. `PlaybackMode::Preview` hat auf
`origin/dev` **keinen produktiven Aufrufer** (`begin_preview`,
`external_media_state.rs:372`, wird nirgends gerufen; das Modul trägt
`#![allow(dead_code)]`) — die Tabelle beantwortet es trotzdem, sonst ist die
Iteration lückenhaft.

### 1.12 Regelwerk & Gates

`docs/ux-rules.md:3387–3393` (BROWSE-4), `:3962–3973` (SRC-13),
`:1443–1449` (TIP-1d) — alle `[active] [gtk]`. Höchste vergebene PLAY-Nummer
ist `PLAY-11` (`:259`) ⇒ die neue Regel wird **`PLAY-12`**.

`scripts/check-ux-traceability.sh` verlangt pro aktiver Regel ≥ 1 Test, dessen
`fn`-Name die ID im snake_case trägt (`play_12_…`, `browse_4_…`, `src_13_…`,
`tip_1d_…`) und über dem ein `#[test]` innerhalb der 5 Zeilen darüber steht;
`#[ignore = "requires a display; run via xvfb-run"]` zählt weiterhin als
Abdeckung, jedes andere `#[ignore]` auf einem `[active]`-Test ist ein Fehler.

`scripts/tests/gettext-catalogs.sh:22` erzeugt die `.pot` per `xgettext …
'--keyword=N_!:1'` neu und prüft **jeden** Katalog per `msgcmp
--use-fuzzy --use-untranslated` (`:31`) gegen diese frische `.pot`; für `de`
und `es` sind zusätzlich **null** unübersetzte Messages erlaubt (`:40–42`).
Aufgerufen aus `scripts/check-release.sh:24–25`.

---

## 2. Arbeitspakete

### Parallelität

```
Welle A (gleichzeitig, keine gemeinsamen Dateien):
  AP1  Core-Intents            crates/reprise-core/src/browser/navigation.rs
  AP2  Player-Accessor         ui/playback/*
  AP3  Reveal-Policy           ui/source_reveal.rs
  AP4  Link-Tabelle + Strings  ui/playing_links.rs, strings_*, player_bar_layout*
  AP6  Filter-Facetten         podcasts_presentation.rs, radio_filter_bar.rs

Welle B:  AP5 (braucht AP3)          AP8 (braucht AP4)
Welle C:  AP7 (braucht AP3+AP5+AP6)
Welle D:  AP9 (braucht AP1+AP2+AP4+AP7)
Welle E:  AP10 (braucht alles — Testnamen und Strings)
```

`AP4` und `AP8` fassen beide die Player-Leiste an, aber verschiedene Dateien:
AP4 nur `player_bar_layout.rs` + `player_bar_layout_tests.rs` + die neue
`playing_links.rs` + die `strings_*`-Dateien; AP8 nur `player_bar_cover.rs`,
`player_bar_external.rs`, `now_playing*`, `player_bar_tests.rs`. Trotzdem
sequenziell, weil AP8 die Funktionen aus AP4 aufruft.

Commit-Schnitt: ein Commit je Arbeitspaket, plus einen eigenen für den
`same_destination`-Youtube-Fix in AP1.

---

### AP1 — Core: `SourceKind`, zwei Intents, `source_target()` *(parallel)*

**Datei:** `crates/reprise-core/src/browser/navigation.rs` (nur diese)

1. Neues browser-eigenes Enum, direkt neben `SidebarTarget`:

   ```rust
   /// Which source list a reveal intent belongs to. Deliberately *not*
   /// `PodcastKind`: `reprise_core::browser` imports `BrowseFilter` and
   /// `ViewSource` and nothing else, and the navigation grammar stays free
   /// of the podcast domain.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum SourceKind { Podcasts, Youtube }
   ```

2. Zwei `NavigationIntent`-Varianten (`:32–52`), jeweils mit `BROWSE-4` im
   Doc-Kommentar:

   ```rust
   RevealEpisode { subscription_id: i64, episode_id: Option<i64>, kind: SourceKind },
   RevealStation { station_id: i64 },
   ```

   `episode_id: None` heißt „nur den Kanal aufdecken“ (Kanalzeile und Cover).

3. Eine **öffentliche reine Funktion**, die Gültigkeit und Ziel an genau einer
   Stelle entscheidet — die GTK-Seite braucht dieselbe Antwort und darf sie
   nicht nachbauen:

   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum SourceTarget {
       Episode { subscription_id: i64, episode_id: Option<i64>, kind: SourceKind },
       Station { station_id: i64 },
   }

   impl NavigationIntent {
       /// The source-list reveal this intent asks for, or `None` for every
       /// other intent and for ids that cannot address anything
       /// (`<= 0`, like `RevealTrack`'s `track_id <= 0` rule).
       #[must_use]
       pub fn source_target(&self) -> Option<SourceTarget>;
   }
   ```

   Regeln: `subscription_id <= 0` ⇒ `None`; `station_id <= 0` ⇒ `None`;
   `episode_id == Some(id)` mit `id <= 0` ⇒ `None` (der ganze Auftrag, nicht
   nur die Episode — eine kaputte ID ist ein kaputter Auftrag).

4. `navigate` (`:139–195`) bekommt zwei Arme, die **auf `source_target()
   aufsetzen**, damit Gültigkeitsprüfung und Zielwahl nicht doppelt existieren:

   ```rust
   NavigationIntent::RevealEpisode { .. } | NavigationIntent::RevealStation { .. } => {
       let target = intent.source_target()?;          // ungültige IDs: No-op
       self.go_metadata_scope(match target {
           SourceTarget::Episode { kind: SourceKind::Podcasts, .. } => BrowserPlace::Podcasts,
           SourceTarget::Episode { kind: SourceKind::Youtube,  .. } => BrowserPlace::Youtube,
           SourceTarget::Station { .. } => BrowserPlace::Radio,
       })
   }
   ```

   (Die `match`-Struktur nach Geschmack; verbindlich ist nur: *eine*
   Gültigkeitsregel, und `go_metadata_scope` als Übergang.)

5. Doc-Kommentar an beiden Armen, der 1.5(a) festhält, wörtlich sinngemäß:
   *steht man bereits in der Zielansicht, liefert `go_metadata_scope` `None` —
   es gibt keinen Übergang zu rendern. Der Reveal-Auftrag hängt nicht daran;
   ihn erteilt `MetadataNavigator::navigate` unabhängig vom Ergebnis.*

6. **Eigener Commit:** `same_destination` (`:317–331`) bekommt den fehlenden
   Arm `(BrowserPlace::Youtube, BrowserPlace::Youtube) => true`.

**Tests** (`mod tests` in derselben Datei, reines Core, kein Display):

- `browse_4_reveal_episode_targets_the_place_of_its_kind`
- `browse_4_reveal_episode_from_the_library_records_back_history`
- `browse_4_reveal_station_from_the_library_records_back_history`
- `browse_4_a_reveal_in_the_open_source_view_yields_no_transition` — belegt
  bewusst, dass `navigate` hier `None` liefert und **kein** `Replace` entsteht
  (ersetzt den ursprünglich in der Spec skizzierten `Replace`-Test)
- `browse_4_invalid_source_ids_have_no_target` — `source_target()` ist `None`
  und `navigate` liefert `None`
- eigener Commit: `browse_4_two_youtube_places_are_the_same_destination`
  (ruft `same_destination` direkt auf; über `==` wäre der Fix unsichtbar)

**Beweislast:** vollständig in `cargo test -p reprise-core`. Kein Display.

---

### AP2 — Player: `current_source_item()` *(parallel)*

**Dateien:**
- `crates/reprise-gnome/src/ui/playback/external_media_state.rs`
- `crates/reprise-gnome/src/ui/playback/external_media.rs`
- `crates/reprise-gnome/src/ui/playback/external_media_session.rs`
- **neu:** `crates/reprise-gnome/src/ui/playback/source_item_identity.rs`
- `crates/reprise-gnome/src/ui/playback/mod.rs` (Modul einhängen)

1. `PodcastSession` (`external_media_state.rs:231–249`) bekommt
   `pub(super) kind: PodcastKind`.
2. Befüllen an genau zwei Stellen: `begin_podcast` (`external_media.rs:220–235`
   — `kind` liegt in `:207–209` bereits vor) und `restored_session`
   (`external_media_session.rs:40–58` — `episode.kind`).
3. Neue Datei in der Form von `album_identity.rs`:

   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub(in crate::ui) enum LoadedSourceItem {
       Episode { subscription_id: i64, episode_id: i64, kind: PodcastKind },
       Station { station_id: i64 },
   }

   /// Pure projection, so the mapping is testable without a controller.
   pub(in crate::ui) fn loaded_source_item(session: Option<&ExternalSession>)
       -> Option<LoadedSourceItem>;

   impl PlayerController {
       pub(in crate::ui) fn current_source_item(&self) -> Option<LoadedSourceItem> {
           loaded_source_item(self.external.borrow().session.as_ref())
       }
   }
   ```

   `episode_id` aus `ExternalMedia::Podcast { episode_id, .. }`
   (`external_media_state.rs:24`), `station_id` aus `ExternalMedia::Radio
   { station_id, .. }` (`:32`). `self.external` ist eine andere `RefCell` als
   `now_playing` ⇒ kein Borrow-Konflikt mit `current_track_id()`.
   `PodcastOrigin::ManualQueue` (Modus `QueuedEpisode`) liefert dieselbe
   `Episode`-Antwort wie `Direct` — eine aus der Warteschlange gestartete
   Folge ist genauso auffindbar.

4. **Empirische Aufgabe: ist `subscription_id == 0` erreichbar?**
   `begin_podcast` nimmt `row.as_ref().map_or(0, …)`
   (`external_media.rs:210`) — eine Sitzung ohne `EpisodeRow` trägt
   `subscription_id: 0`. Zu messen ist: kann `begin_podcast` mit `row == None`
   aufgerufen werden?
   *Vorgehen:* alle Erzeuger von `ExternalMedia::Podcast` aufzählen
   (`git grep -n "ExternalMedia::Podcast\|media_from_episode\|play_external"`)
   und prüfen, ob einer davon eine `episode_id` liefert, die
   `reprise_core::podcasts::store::episode` (`external_media.rs:122`) nicht
   findet. Besonders zu prüfen: der Runtime-/D-Bus-Pfad
   (`runtime/commands.rs:87`, `reprise-platform-linux/src/runtime_service/
   interface.rs:302`), weil dessen Episode-ID von außen kommt.
   *Was gilt:*
   - Erreichbar (erwarteter Fall) ⇒ `loaded_source_item` liefert für
     `subscription_id <= 0` **`None`**, mit Kommentar, der den gefundenen Pfad
     benennt. Test `browse_4_a_session_without_a_subscription_has_no_source_item`.
   - Nachweislich unerreichbar ⇒ der Guard bleibt trotzdem stehen, der
     Kommentar sagt warum (Verteidigung gegen den D-Bus-Pfad), und derselbe
     Test bleibt. In keinem Fall darf ein Sprung mit `subscription_id: 0` in
     den Router laufen: er wäre dort per AP1 ohnehin ein No-op, aber ohne
     Toast (AP7) und damit stumm — genau das, was `PLAY-12` verbietet.

**Tests** (im neuen Modul, rein, kein Display; Vorbild ist der
`podcast_session`-Helfer in `external_media_state.rs:575`):

- `browse_4_library_playback_has_no_loaded_source_item` (Session `None`)
- `browse_4_an_rss_session_reports_its_subscription_and_kind`
- `browse_4_a_youtube_session_reports_the_youtube_kind`
- `browse_4_a_queued_episode_is_the_same_source_item_as_a_direct_one`
- `browse_4_a_radio_session_reports_its_station`
- `browse_4_a_restored_session_keeps_its_kind`
- `browse_4_a_session_without_a_subscription_has_no_source_item`

**Beweislast:** vollständig displaylos (`cargo test -p reprise-gnome
source_item_identity`).

---

### AP3 — Policy: `RequestedByUser` *(parallel)*

**Datei:** `crates/reprise-gnome/src/ui/source_reveal.rs` (nur diese)

Fünfte `LoadedItemChange`-Variante (`:14–25`) und ein Arm in `reveal_policy`
(`:49–57`), der `USER_SCROLL_GRACE` bewusst ignoriert:

```rust
/// `SRC-13`: the user asked for this jump from the player bar or `Ctrl+L`.
/// It always reveals — also in the already visible view and regardless of
/// the 1.5-second grace period. The grace protects a reading user from a
/// viewport that jumps under his hand; here he asked for the jump himself.
RequestedByUser,
```

Keine exhaustiven `match`-Arme brechen dadurch: `podcasts_view_marker.rs:52`
und `radio_reveal.rs:69` vergleichen nur das Ergebnis,
`podcasts_view_marker.rs:20–26` und `radio_view.rs:360–369` konstruieren.

**Tests:** `src_13_a_user_requested_jump_always_reveals` (beide
`user_scrolling`-Werte ⇒ `RevealPolicy::Reveal`).

**Beweislast:** displaylos, in derselben Datei.

---

### AP4 — Eine Tabelle für alle Links der Player-Leiste *(parallel)*

Dies ist die Verankerung der neuen Regel `PLAY-12` („Die Player-Leiste hat
keine toten Flächen“). Ziel: der Test hakt nicht die heute bekannten drei Modi
ab, sondern **iteriert über `PlaybackMode`**, und eine künftige Quelle bricht
die Zuordnung beim Kompilieren, nicht erst im Betrieb.

**Dateien:**
- **neu:** `crates/reprise-gnome/src/ui/playing_links.rs` (in `ui/mod.rs` einhängen)
- `crates/reprise-gnome/src/ui/playback/preview.rs` (`PlaybackMode::ALL`)
- `crates/reprise-gnome/src/ui/strings_podcasts.rs`, `strings_radio.rs`
  (+ die `pub use`-Fassade in `strings.rs`, über die `strings::PODCAST_ADD`
  heute schon erreichbar ist)
- `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs` (Bau-Defaults
  aus der Tabelle beziehen)
- `crates/reprise-gnome/src/ui/player_bar/player_bar_layout_tests.rs`
  (`tip_1d`-Quelltexttest entfernen)

1. In `preview.rs`:

   ```rust
   impl PlaybackMode {
       pub(in crate::ui) const ALL: [Self; 5] = [
           Self::Queue, Self::QueuedEpisode, Self::Preview, Self::Podcast, Self::Radio,
       ];
   }
   ```

2. In `playing_links.rs` (rein, kein GTK-Import außer nichts — die Datei darf
   keine Widgets kennen, damit ihre Tests displaylos bleiben):

   ```rust
   pub(in crate::ui) enum LinkSurface { Title, Subtitle, Cover }
   pub(in crate::ui) const SURFACES: [LinkSurface; 3] = [ … ];

   pub(in crate::ui) enum LinkTarget { Track, Album, Artist, Episode, Channel, Station }

   /// What the player bar knows about the loaded item right now.
   pub(in crate::ui) struct LinkAvailability { pub artist: bool, pub album: bool }

   /// Total over `PlaybackMode` — a new playback source cannot compile
   /// without answering all three surfaces. `PLAY-12`.
   pub(in crate::ui) fn link_target(mode: PlaybackMode, surface: LinkSurface) -> LinkTarget;

   /// `PLAY-12`'s "nie ins Leere": a surface whose own target does not exist
   /// falls back to the nearest one that does, instead of going dead.
   pub(in crate::ui) fn resolve(target: LinkTarget, available: LinkAvailability) -> LinkTarget;

   pub(in crate::ui) fn player_bar_label(target: LinkTarget) -> &'static str;
   pub(in crate::ui) fn panel_label(target: LinkTarget) -> &'static str;

   pub(in crate::ui) struct LinkLabels { pub title: &'static str,
                                         pub subtitle: &'static str,
                                         pub cover: &'static str }
   pub(in crate::ui) fn player_bar_labels(mode: PlaybackMode, available: LinkAvailability) -> LinkLabels;
   pub(in crate::ui) fn panel_labels(mode: PlaybackMode, available: LinkAvailability) -> LinkLabels;
   ```

   Tabelle `link_target`:

   | Modus | Titel | Untertitel (Interpret/Kanal) | Cover |
   | --- | --- | --- | --- |
   | `Queue` | `Track` | `Artist` | `Album` |
   | `Preview` | `Track` | `Artist` | `Album` |
   | `QueuedEpisode` | `Episode` | `Channel` | `Channel` |
   | `Podcast` | `Episode` | `Channel` | `Channel` |
   | `Radio` | `Station` | `Station` | `Station` |

   `Preview` bekommt einen Kommentar: auf `origin/dev` hat `begin_preview`
   keinen produktiven Aufrufer; wird Preview je verdrahtet, muss diese Zeile
   erneut beantwortet werden, weil eine Vorschau-Datei nicht zwingend in der
   Bibliothek steht.

   `resolve`: `Artist` ohne Interpret ⇒ `Track`; `Album` ohne Album ⇒ `Track`;
   alles andere unverändert. (Das ist die einzige Stelle, an der eine Fläche
   ihr Ziel wechselt — und der Grund, warum ein bibliothekarischer Titel ohne
   Interpret keine tote Untertitelzeile mehr hat.)

   Beschriftungen:

   | `LinkTarget` | Player-Leiste | Now-Playing-/Info-Panel |
   | --- | --- | --- |
   | `Track` | `JUMP_TO_NOW_PLAYING` | `REVEAL_PLAYING_TRACK` |
   | `Album` | `REVEAL_PLAYING_ALBUM` | `GO_TO_PLAYING_ALBUM` |
   | `Artist` | `GO_TO_PLAYING_ARTIST` | `GO_TO_PLAYING_ARTIST` |
   | `Episode` | `JUMP_TO_PLAYING_EPISODE` | `JUMP_TO_PLAYING_EPISODE` |
   | `Channel` | `GO_TO_PLAYING_CHANNEL` | `GO_TO_PLAYING_CHANNEL` |
   | `Station` | `JUMP_TO_PLAYING_STATION` | `JUMP_TO_PLAYING_STATION` |

3. Neue `N_!`-Konstanten (beide Dateien stehen bereits in `po/POTFILES.in:10–11`):
   - `strings_podcasts.rs`:
     `JUMP_TO_PLAYING_EPISODE = N_!("Jump to the playing episode")`,
     `GO_TO_PLAYING_CHANNEL = N_!("Go to the channel")`,
     `EPISODE_NOT_IN_SUBSCRIPTIONS = N_!("This episode is no longer in your subscriptions")`
   - `strings_radio.rs`:
     `JUMP_TO_PLAYING_STATION = N_!("Go to the playing station")`,
     `STATION_NOT_IN_FAVORITES = N_!("This station is no longer in your favorites")`

   Die beiden `*_NOT_*`-Strings gehören zu AP7, werden aber hier mit angelegt,
   damit AP10 alle fünf in einem Zug übersetzt.

4. `player_bar_layout.rs` (`:85–130`) bezieht seine Bau-Beschriftungen aus
   `player_bar_labels(PlaybackMode::Queue, LinkAvailability { artist: true,
   album: true })` statt aus Literalen. Damit gibt es die Beschriftung genau
   einmal, und der Bau-Default kann nicht mehr von dem abweichen, was
   `set_track` später schreibt. Nebeneffekt: der Literal-String
   `strings::GO_TO_PLAYING_ARTIST` verschwindet aus der Datei.

5. **`tip_1d` umschreiben, nicht umgehen.** Den Quelltexttest
   `tip_1d_player_bar_artist_names_its_navigation_action`
   (`player_bar_layout_tests.rs:296–305`) löschen und in `playing_links.rs`
   durch einen Verhaltenstest ersetzen (Name behält das `tip_1d_`-Präfix,
   sonst verliert die Regel ihre Abdeckung).

**Tests** (alle in `playing_links.rs`, rein, kein Display):

- `play_12_every_playback_mode_lands_all_three_links` — iteriert
  `PlaybackMode::ALL` × `SURFACES` × alle vier `LinkAvailability`-Kombinationen
  und prüft je Kombination: `strings::text(player_bar_label(resolved))` und
  `strings::text(panel_label(resolved))` sind nicht leer, und `resolved` ist
  weder `Artist` ohne Interpret noch `Album` ohne Album.
- `play_12_all_lists_every_playback_mode` — Schutz davor, dass `ALL` beim
  Hinzufügen einer Variante veraltet: eine lokale `fn index_of(mode:
  PlaybackMode) -> usize` mit **exhaustive** `match` (eine neue Variante bricht
  hier die Kompilierung), dann `assert_eq!(PlaybackMode::ALL.len(), 5)` und für
  jeden Modus `PlaybackMode::ALL[index_of(mode)] == mode`.
- `play_12_external_modes_point_at_their_own_source` — Podcast und
  QueuedEpisode ⇒ `Episode`/`Channel`/`Channel`; Radio ⇒ dreimal `Station`.
- `play_12_a_track_without_an_artist_falls_back_to_the_track`
- `tip_1d_every_surface_names_its_action_in_every_mode` — pro Modus tragen die
  drei Beschriftungen einen nichtleeren, benannten Aktionstext; Titel- und
  Untertitel-Beschriftung sind im selben Modus nie identisch, außer bei Radio,
  wo die flache Liste genau ein Ziel hat.

**Beweislast:** vollständig displaylos. Dieses Paket ist der Beweis für die
neue Regel.

---

### AP5 — Kanal-Reveal *(nach AP3)*

**Dateien:**
- `crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs`
- `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs`
- `crates/reprise-gnome/src/ui/podcasts/podcasts_reveal.rs`
- `crates/reprise-gnome/src/ui/podcasts/podcasts_view_marker.rs`
- **evtl. neu:** `crates/reprise-gnome/examples/podcast_header_geometry.rs`

1. `RenderedRowWidgets` (`podcasts_groups.rs:49–52`) bekommt
   `pub(super) channels: BTreeMap<i64, ChannelRowWidgets>` mit

   ```rust
   pub(super) struct ChannelRowWidgets {
       pub(super) header: gtk4::Widget,      // das Widget aus group_header (:150–159)
       pub(super) expander: gtk4::Expander,  // Fallback, siehe Punkt 5
   }
   ```

   `build_group` trägt beide ein, bevor es den Expander zurückgibt.
2. `PodcastsView` (`podcasts_view.rs:113–114`) bekommt
   `channel_widgets: RefCell<BTreeMap<i64, ChannelRowWidgets>>`, befüllt in
   `render` an derselben Stelle wie `download_widgets` (`:401–402`) —
   identischer Lebenszyklus, sonst zeigt die Map nach einem `render()` auf
   abgehängte Widgets.
3. `podcasts_reveal.rs`: reine Funktion

   ```rust
   /// Locates a channel in the rendered groups. `needs_full_window` is always
   /// false: whoever jumps to the channel wants to see it from the top, not a
   /// row in the middle of its episode list (Spec A.2).
   pub(super) fn channel_reveal_target(groups: &[SourceGroup], subscription_id: i64)
       -> Option<RevealTarget>;
   ```

4. `podcasts_view_marker.rs`: `reveal_loaded_episode` wird zielparametrisiert:

   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub(super) enum RevealRequest { Episode(i64), Channel(i64) }

   fn reveal(&self, request: RevealRequest, change: LoadedItemChange);
   ```

   `reveal_loaded_episode(change)` bleibt als dünner Aufrufer bestehen
   (`self.reveal(RevealRequest::Episode(mark.id), change)`), und der
   Episoden-Pfad bleibt Zeile für Zeile identisch — er ist Regressionsschutz
   für `SRC-13` und `START-3`. Der Kanal-Pfad klappt **nur** `expanded_sources`
   auf (nie `expanded_episode_sources`), rendert bei
   Strukturänderung, holt das Widget **danach** aus `channel_widgets` und
   zentriert mit **demselben** `podcasts_reveal::center_row` (`:121–147`).
   Kein zweiter Zentrier-Weg, kein `set_value` außerhalb des Tick-Callbacks.
   Selektion und Fokus bleiben unberührt (der `SessionRestore`-Sonderfall in
   `:88–90` gilt nur für Episoden).

5. **Empirische Aufgabe: meldet die Kopfzeile eine Höhe?**
   `centered_value` liefert `None`, solange `row_height <= 0`
   (`podcasts_reveal.rs:65`) — meldet das Label-Widget eines eingeklappten
   `Expander` in den ersten Frames keine Höhe, verbrennt der Kanal-Reveal
   60 Frames und tut nichts.
   *Zu messen:* `header.height()` und `expander.height()` für eine
   **eingeklappte** und eine **aufgeklappte** Gruppe, in genau dem Moment, in
   dem `centered_target` sie liest (Tick-Callback nach `idle_add_local_once`).
   *Wie:* ein Beispielprogramm unter `crates/reprise-gnome/examples/`
   (das Repo benutzt Examples bereits als GTK-Beweismittel, weil
   Display-Tests im Rudel flaky sind), das über `podcasts_groups::replace`
   einen Gruppenbaum in ein Fenster hängt und die Werte pro Frame ausgibt.
   Ausführung headless: `xvfb-run -a cargo run -p reprise-gnome --example
   podcast_header_geometry`.
   *Was gilt:*
   - `header.height() > 0` im eingeklappten Zustand ⇒ geplanter Weg:
     `center_row(&self.scroller, &channel.header, &self.reveal_animation)`.
     Das `expander`-Feld aus Punkt 1 entfällt dann; entfernen, nicht ungenutzt
     stehen lassen.
   - `header.height() == 0` im eingeklappten Zustand ⇒ der Kanal-Reveal klappt
     die Gruppe ohnehin auf, also ist die Kopfzeile spätestens nach dem
     `render()` allokiert; als Absicherung wählt der Tick-Callback das
     Header-Widget, solange `height() > 0`, sonst den `Expander`. Diese
     Auswahl gehört in `podcasts_reveal.rs` (eine Zeile im Callback), **nicht**
     in den Klick-Handler.
   In beiden Fällen: die gemessenen Zahlen als Kommentar über
   `channel_reveal_target` festhalten, damit der nächste Leser nicht erneut
   messen muss.

**Tests** (rein, kein Display, in `podcasts_reveal.rs`):

- `src_13_a_channel_reveal_only_expands_its_group` — `needs_full_window` ist
  false und die Subscription stimmt
- `src_13_a_channel_reveal_leaves_the_episode_window_closed` — auch dann,
  wenn die geladene Episode hinter dem Zehnerfenster liegt
- `src_13_a_channel_that_is_not_listed_has_nothing_to_reveal`
- die bestehenden `reveal_target`-Tests (`:192–250`) bleiben **unverändert**
  grün — sie sind der Regressionsschutz für den Episodenpfad

**Beweislast:** die Zielbestimmung (`channel_reveal_target`) ist displaylos
bewiesen; die Geometrie ist per Example gemessen und im Kommentar belegt. Der
Widget-Lebenszyklus (`channel_widgets` wird pro `render()` neu gefüllt) ist
nicht displaylos beweisbar — dafür ein `#[ignore = "requires a display; run via
xvfb-run"]`-Test in `podcasts_groups_tests.rs`, der prüft, dass `replace` für
jede Gruppe genau einen Eintrag zurückgibt. Er zählt nicht als Beweis, aber er
dokumentiert die Zusage.

---

### AP6 — Nur die verbergende Facette weicht *(parallel)*

**Dateien:**
- `crates/reprise-gnome/src/ui/podcasts/podcasts_presentation.rs`
- `crates/reprise-gnome/src/ui/podcasts/podcasts_filter_bar.rs`
- `crates/reprise-gnome/src/ui/radio/radio_filter_bar.rs`

`SRC-13` sagt für passive Reveals: ein verborgenes Element wird nicht
aufgedeckt und der Filter nie geräumt. Für einen **expliziten** Sprung ist das
eine Sackgasse — die laufende Episode ist gerade *nicht* mehr „Unplayed“, ein
aktiver Unplayed-Filter verbirgt sie also im Normalbetrieb. Es weicht deshalb
**genau die Facette, die verbirgt**, nicht der ganze Filter: `clear_all`
schreibt persistent und die Schlüssel sind global über Podcasts und YouTube
(`config.rs:218–225`), ein Sprung würde also die *andere* Ansicht mit
aufräumen. Bei einer einzelnen Facette ist derselbe Nebeneffekt hinnehmbar,
weil der Chip sichtbar verschwindet und der Nutzer den Sprung selbst ausgelöst
hat.

**Entscheidend: kein zweites Filterprädikat.** Die Doppelung derselben
Entscheidung an zwei Stellen war in diesem Repo schon zweimal ein hörbarer
Bug. Alles unten setzt auf `matches_filter` (`podcasts_presentation.rs:257`)
bzw. `filter_rows` (`radio_filter_bar.rs:46`) auf.

1. Podcasts, in `podcasts_presentation.rs`:

   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub(super) enum PodcastFilterFacet { Unplayed, Source, Downloaded }
   pub(super) const PODCAST_FACETS: [PodcastFilterFacet; 3] = [ … ];

   /// The filter with every facet but this one neutralised.
   fn only_facet(filter: &PodcastFilter, facet: PodcastFilterFacet) -> PodcastFilter;

   /// `matches_filter` is a conjunction of independent facets, so a facet
   /// hides the row exactly when the row fails that facet alone.
   pub(super) fn facet_hides(row: &EpisodeRow, filter: &PodcastFilter,
                             facet: PodcastFilterFacet) -> bool {
       !matches_filter(row, &only_facet(filter, facet))
   }

   pub(super) fn remove_facet(filter: &PodcastFilter, facet: PodcastFilterFacet) -> PodcastFilter;

   /// `SRC-13`: the filter an explicit jump to this episode needs — unchanged
   /// when the episode is visible, otherwise the same filter minus exactly
   /// the facets that hide it.
   pub(super) fn filter_without_hiding(row: &EpisodeRow, filter: &PodcastFilter) -> PodcastFilter;

   /// Same question for a channel jump: a group whose episodes all fail the
   /// filter disappears entirely (`rendered_source_groups`, :107–127), so the
   /// criterion is "no episode of this group survives".
   pub(super) fn filter_without_hiding_group(group: &SourceGroup,
                                             filter: &PodcastFilter) -> PodcastFilter;
   ```

   `filter_without_hiding_group`: ist `apply_filter(&group.episodes, filter)`
   nichtleer ⇒ Filter unverändert. Sonst jede Facette entfernen, für die
   **keine** Episode der Gruppe `only_facet` besteht.

2. Radio: die Struktur existiert bereits (`RadioFilterFacet`, `remove_filter`).
   Ergänzen:

   ```rust
   fn only_facet(filter: &RadioFilter, facet: RadioFilterFacet) -> RadioFilter;
   pub(super) fn facet_hides_station(row: &StationRow, filter: &RadioFilter,
                                     facet: RadioFilterFacet) -> bool {
       filter_rows(std::slice::from_ref(row), &only_facet(filter, facet)).is_empty()
   }
   pub(super) fn filter_without_hiding(row: &StationRow, filter: &RadioFilter) -> RadioFilter;
   ```

3. Anwenden können muss man den neuen Filter auch: beide Filterleisten
   bekommen einen dünnen öffentlichen Aufrufer für ihr vorhandenes privates
   `apply` — `PodcastsFilterBar::apply_filter(self: &Rc<Self>, filter:
   PodcastFilter)` (`podcasts_filter_bar.rs`, neben `clear_all` `:155`) und
   `RadioFilterBar::apply_filter(self: &Rc<Self>, filter: RadioFilter)`
   (`radio_filter_bar.rs`, neben `clear_all` `:204`). Damit bleiben Persistenz,
   Chip-Neubau und `on_changed` genau ein Weg.

**Alternative, wie erbeten, aber nicht geplant:** eine rein transiente
Abschaltung wäre nur zu haben, indem man `apply` in „persistieren“ und „nur
anwenden“ auftrennt. Sie ist billiger, als sie aussieht, aber sie erzeugt einen
sichtbar verschwundenen Chip, der beim nächsten Start wieder da ist — die
Anzeige widerspräche dem gespeicherten Zustand. Deshalb: persistent, wie jede
andere Filteränderung auch.

**Tests** (rein, kein Display):

- `src_13_only_the_hiding_facet_is_dropped_for_an_episode` — Unplayed **und**
  Downloaded aktiv, Episode gespielt aber heruntergeladen ⇒ nur `unplayed_only`
  fällt, `downloaded_only` bleibt stehen
- `src_13_a_visible_episode_leaves_every_facet_standing`
- `src_13_a_channel_whose_episodes_all_fail_the_filter_drops_that_facet`
- `src_13_only_the_hiding_facet_is_dropped_for_a_station`
- `src_13_a_visible_station_leaves_every_facet_standing`

**Beweislast:** vollständig displaylos. Hier liegt der Beweis für die
Filterhälfte von `SRC-13`s neuem Satz.

---

### AP7 — Eingänge in die Views *(nach AP3 + AP5 + AP6)*

**Dateien:**
- `crates/reprise-gnome/src/ui/podcasts/podcasts_view_marker.rs`
  (bei drohendem 800-Zeilen-Limit: neue Datei
  `podcasts_view_reveal_request.rs` im selben Modul)
- `crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs` (neues Feld)
- `crates/reprise-gnome/src/ui/radio/radio_view.rs`

1. `PodcastsView::request_reveal(self: &Rc<Self>, subscription_id: i64,
   episode_id: Option<i64>)`. Reihenfolge ist verbindlich, sie folgt aus den
   Borrow-Regeln (siehe Abschnitt 3):

   1. **Zuständigkeit:** die Klasse ist zweimal instanziiert
      (`source_views.rs:103–114`). Gehört die Subscription nicht zu
      `self.kind`, ist der Aufruf ein stiller No-op — der Router schickt ihn
      an beide Instanzen, und nur die zuständige antwortet. (`PodcastsView`
      hält `kind` privat, `podcasts_view.rs:82`; falls ein Accessor fehlt,
      einen `pub(in crate::ui) fn kind(&self) -> PodcastKind` ergänzen.)
   2. **Auffindbarkeit:** ist `subscription_id` in `self.groups` (ungefiltert!)
      nicht vorhanden — oder ist `episode_id` gesetzt und in keiner Gruppe —,
      dann Toast `EPISODE_NOT_IN_SUBSCRIPTIONS` über den vorhandenen
      `toast_overlay` (`podcasts_view.rs:122`, `:279`) und **return**. Das ist
      das Gegenstück zu `TRACK_NOT_IN_LIBRARY`
      (`metadata_navigation.rs:76–80`): ein Sprung, der nicht landen kann,
      sagt es, statt still zu verpuffen.
   3. **Überlagerung schließen:** ist die YouTube-Kanaldetailseite offen,
      erzwingt `render` die Stack-Seite `"youtube-channel"`
      (`podcasts_view.rs:466–468`) und die Gruppenliste ist unsichtbar —
      der Sprung landete hinter der Seite. Also `close_channel`
      (`youtube_channel_detail.rs:55`) aufrufen. **Bewusste
      Verhaltensänderung**, sie gehört in `BROWSE-4` (AP10).
   4. **Filter:** `filter_without_hiding` (Episode) bzw.
      `filter_without_hiding_group` (Kanal) aus AP6 rechnen; nur wenn sich
      etwas ändert, `filter_bar.apply_filter(neu)` rufen. Das feuert
      **synchron** `on_changed` → `render()` — deshalb muss es geschehen,
      **bevor** irgendein Feld der View geliehen ist.
   5. **Auftrag hinterlegen und ausführen:**
      `self.pending_reveal.replace(Some(request))`, dann:
      ist `self.root.is_mapped()` ⇒ `if let Some(request) =
      self.pending_reveal.take() { self.reveal(request,
      LoadedItemChange::RequestedByUser) }`. Ist die Seite noch nicht
      gemappt, bleibt der Auftrag liegen und der `connect_map`-Handler holt
      ihn ab.
2. **Angeforderter Reveal schlägt `ViewEntered`.** `install_reveal_tracking`s
   `root.connect_map`-Closure (`podcasts_view_marker.rs:40–44`) wird zu:

   ```rust
   if let Some(request) = view.pending_reveal.take() {
       view.reveal(request, LoadedItemChange::RequestedByUser);
   } else {
       view.reveal_loaded_episode(LoadedItemChange::ViewEntered);
   }
   ```

   Damit gewinnt beim Kanalsprung der angeforderte Reveal, und `ViewEntered`
   dieses einen Durchlaufs entfällt — sonst zielten beide im selben Frame auf
   verschiedene Zeilen. Neues Feld:
   `pending_reveal: RefCell<Option<RevealRequest>>` in `podcasts_view.rs`.
3. `RadioView::request_reveal_connected(&self)` — **ohne ID-Parameter**, weil
   `RadioReveal::reveal` ausschließlich `connected_station(&live)` aufdecken
   kann (`radio_reveal.rs:72`) und eine fremde ID gar nicht anspringen könnte.
   Ablauf:
   1. Keine verbundene Station (`connected_station` ⇒ `None`) ⇒ Toast
      `STATION_NOT_IN_FAVORITES`, return. `RadioView` hat den Overlay bereits
      (`radio_view.rs:66`, `:275`).
   2. Ist die Station in `self.rows`, aber nicht in den sichtbaren Zeilen
      (`station_position` über die gerenderten Zeilen ⇒ `None`), Filter per
      AP6 entschärfen und `render_rows` laufen lassen. **Nur dann** — die
      Radio-Liste ist ein `ColumnView` über `RadioModel::replace`
      (`radio_view.rs:387`), und ein unnötiges `items_changed` setzt die
      Fokuszeile auf 0 (bekannte Falle dieses Repos).
   3. Ist die Station gar nicht in `self.rows` ⇒ Toast, return.
   4. `self.reveal.reveal(LoadedItemChange::RequestedByUser)` **direkt**
      aufrufen. Das umgeht `on_external_change`s Wache
      (`radio_reveal.rs:101–109`), die nur bei *gewechselter* verbundener
      Station aufdeckt — der Sprung zur bereits verbundenen Station ist der
      Normalfall und wäre sonst tot geboren. Kein `pending_reveal`-Token
      nötig: Radios `ViewEntered` (`radio_reveal.rs:163–167`) zielt auf
      dieselbe Zeile, ein doppelter Reveal ist folgenlos.

**Tests:** die Ansichts-Eingänge selbst sind Display-Code (alle bestehenden
`PodcastsView`-Tests sind es, `podcasts_view_tests.rs:110 ff.`) und gelten
damit **nicht** als Beweis. Der beweisbare Anteil liegt vollständig in AP5
(Zielbestimmung), AP6 (Filterentscheidung) und AP3 (Policy). Was hier trotzdem
displaylos zu prüfen ist, weil es reine Entscheidungen sind:

- `src_13_an_unlisted_episode_is_reported_instead_of_ignored` — eine reine
  Funktion `fn reveal_outcome(groups: &[SourceGroup], subscription_id: i64,
  episode_id: Option<i64>) -> RevealOutcome` mit
  `RevealOutcome { Reveal(RevealRequest), NotListed }`, aus der
  `request_reveal` seinen Toast-Zweig ableitet. Diese Funktion gehört nach
  `podcasts_reveal.rs` und ist ohne GTK testbar.
- `src_13_a_station_that_is_gone_is_reported_instead_of_ignored` — dieselbe
  Form über `radio_reveal::station_position` (`:34–40`).

Ergänzende Display-Tests dürfen entstehen, tragen aber
`#[ignore = "requires a display; run via xvfb-run"]`.

---

### AP8 — Beschriftung anwenden *(nach AP4)*

**Dateien:**
- `crates/reprise-gnome/src/ui/player_bar/player_bar_cover.rs`
- `crates/reprise-gnome/src/ui/player_bar/player_bar_external.rs`
- `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs`
- `crates/reprise-gnome/src/ui/now_playing/now_playing.rs`
- `crates/reprise-gnome/src/ui/link_activation.rs`
- `crates/reprise-gnome/src/ui/player_bar/player_bar_tests.rs` (Aufrufstellen)

1. **`set_track` bekommt die Beschriftung mit**, statt sie nachträglich
   überschrieben zu bekommen:

   ```rust
   pub fn set_track(&self, title: &str, artist: &str, links: LinkLabels)
   ```

   Es setzt für alle drei Buttons Tooltip **und**
   `accessible::Property::Label` aus `links` — auch für den Interpreten-Button,
   der heute nur einen Tooltip hat. Der bedingungslose
   `REVEAL_PLAYING_ALBUM`-Schreibzugriff auf den Cover-Button
   (`player_bar_cover.rs:49–52`) entfällt; damit hängt nichts mehr an der
   Aufrufreihenfolge von `set_external_snapshot` → `set_track`.
2. **Bedienbarkeit folgt `PLAY-12`:** `set_track` setzt alle drei Buttons
   `set_sensitive(true)`; `clear_track` (`:125–136`) setzt alle drei auf
   `false`. Die alte Regel
   `artist_button.set_sensitive(!artist.trim().is_empty())` fällt weg — ein
   Titel ohne Interpret hat dank `resolve` (AP4) ein gültiges Ziel, nämlich
   den Titel selbst. Unbedienbar ist eine Fläche nur noch, wenn gar nichts
   geladen ist.
3. **Zwei Aufrufer, zwei Modi:**
   - `PlayerController::sync_track` (`now_playing_wiring.rs:145–156`) —
     Bibliotheksfall. Labels aus
     `playing_links::player_bar_labels(self.playback_mode(), LinkAvailability {
     artist: !artist.trim().is_empty(), album: self.current_album_identity()
     .is_some() })`.
   - `PlayerBar::set_external_snapshot` (`player_bar_external.rs:30`) —
     externer Fall. Modus aus `snapshot.media`:
     `ExternalMedia::Podcast` ⇒ `PlaybackMode::Podcast`,
     `ExternalMedia::Radio` ⇒ `PlaybackMode::Radio`. (Ob `Direct` oder
     `ManualQueue`, ist für die Beschriftung gleich — die Tabelle antwortet für
     beide identisch.) `LinkAvailability` ist im externen Modus irrelevant, weil
     `resolve` `Episode`/`Channel`/`Station` nie umleitet; `{ artist: true,
     album: true }` übergeben.
   - `CompactPlayer::set_track` (`compact_player.rs:214`) ist ein anderer Typ
     ohne Links — **nicht anfassen**.
4. **Now-Playing- und Info-Panel** (Spec D.2): ein Link, der etwas anderes
   sagt als er tut, verletzt `PLAY-12`. `link_activation.rs` bekommt

   ```rust
   pub(in crate::ui) fn relabel(widget: &impl IsA<gtk4::Widget>, accessible_label: &str);
   ```

   (setzt `accessible::Property::Label` neu — mehr macht `arm` auch nicht,
   `:36`). `NowPlayingPanel` bekommt
   `pub(in crate::ui) fn set_link_labels(&self, labels: LinkLabels)`, das
   Cover/Titel/Interpret/Album-Fläche (`now_playing.rs:388–405`) neu
   beschriftet; Cover und Album-Fläche teilen sich die `cover`-Beschriftung.
   Aufgerufen an denselben zwei Stellen wie die der Leiste: in `sync_track`
   und dort, wo `set_external_snapshot` gerufen wird
   (`git grep -n "set_external_snapshot("` — der Aufrufer bekommt die eine
   zusätzliche Zeile).
5. Alle `set_track`-Aufrufstellen in `player_bar_tests.rs` (`:26`, `:234`,
   `:235`, `:279`, `:298`) auf die neue Signatur ziehen.

**Tests:**

- `browse_4_external_podcast_playback_names_the_episode_and_channel_links` —
  displaylos über `playing_links::player_bar_labels(PlaybackMode::Podcast, …)`
- `browse_4_radio_playback_names_all_three_links_the_station` — dito
- `browse_4_leaving_external_playback_restores_the_library_labels` — dito für
  `PlaybackMode::Queue`
- Der Widget-seitige Teil (die Beschriftung landet wirklich auf den Buttons)
  ist Display-Code; wenn dafür ein Test entsteht, dann mit
  `#[ignore = "requires a display; run via xvfb-run"]`.

**Beweislast:** die Beschriftungsentscheidung ist displaylos in AP4 bewiesen;
AP8 fügt nur die Zustellung hinzu. Genau deshalb ist die Tabelle die einzige
Wahrheitsquelle — nichts darf die Labels danach noch anfassen.

---

### AP9 — Verdrahtung *(nach AP1 + AP2 + AP4 + AP7)*

**Dateien:**
- `crates/reprise-gnome/src/ui/window/metadata_navigation.rs`
- `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`
- `crates/reprise-gnome/src/ui/playback/source_item_identity.rs` (Ergänzung)

1. Reine Zuordnung, in `source_item_identity.rs`:

   ```rust
   /// Which intent a player-bar surface sends for the loaded source item.
   /// Built on the same `LinkTarget` table the labels come from — the link
   /// cannot say one thing and do another.
   pub(in crate::ui) fn source_reveal_intent(item: &LoadedSourceItem,
                                             surface: LinkSurface) -> NavigationIntent;
   ```

   `Episode` + `Title` ⇒ `RevealEpisode { episode_id: Some(id), kind }`;
   `Episode` + `Subtitle`/`Cover` ⇒ `RevealEpisode { episode_id: None, kind }`;
   `Station` + jede Fläche ⇒ `RevealStation`. Die `PodcastKind`→`SourceKind`-
   Übersetzung passiert hier, an genau einer Stelle.
2. `MetadataNavigator` (`metadata_navigation.rs:39–47`) bekommt

   ```rust
   on_source_reveal: Rc<RefCell<Option<Rc<dyn Fn(SourceTarget)>>>>,
   pub(in crate::ui) fn set_on_source_reveal(&self, callback: impl Fn(SourceTarget) + 'static)
   ```

   — post-construction, weil der Navigator in `window.rs:335` entsteht, die
   Source-Views aber erst in `window.rs:374`. Muster:
   `set_toast_overlay`.
3. `navigate` (`:68–98`) bekommt einen führenden Zweig **vor**
   `normalize_catalog_intent` (das den Intent nicht kennt und ihn über seinen
   `other`-Arm ohnehin durchreicht):

   ```rust
   if let Some(target) = intent.source_target() {
       // Order matters: the view must hold the request before the routing
       // maps its page, so the map handler runs the requested reveal instead
       // of `ViewEntered`. If the page is already mapped, `request_reveal`
       // reveals immediately and the routing below is a no-op.
       if let Some(callback) = self.on_source_reveal.borrow().clone() { callback(target); }
       if let Some(place) = self.history.navigate_from(intent, track_list.browser_place()) {
           library_shell::route_to_place( … );
       }
       return;
   }
   ```

   **Das ist der Kern des Tasks:** der Reveal-Auftrag geht raus, auch wenn
   `navigate_from` `None` liefert — der Fall „Nutzer steht schon in der Liste“
   (1.5a/b). Ungültige IDs kommen hier nicht an, weil `source_target()`
   sie bereits verworfen hat (AP1) — dieselbe Regel, ein Prädikat.
4. `window_runtime_wiring.rs:266–326`: jede der drei Closures bekommt genau
   einen führenden Zweig:

   ```rust
   if let Some(item) = player.current_source_item() {
       navigator.navigate(source_reveal_intent(&item, LinkSurface::Title), "playing episode link");
       return;
   }
   ```

   — `Title` für `reveal_playing_track` (`:266`, damit auch für `Ctrl+L`
   `:352`), `Cover` für `reveal_playing_album` (`:289`), `Subtitle` für
   `reveal_playing_artist` (`:308`). Info-Panel (`:341/:345/:349`) und
   Now-Playing erben, ohne angefasst zu werden — genau das verlangt
   `BROWSE-4`s „regardless of origin“.
   `Ctrl+L` wechselt damit bei laufendem Podcast die Ansicht. Gewollt.
5. Ebenfalls in diese drei Closures, als Umsetzung von `resolve` (AP4):
   liefert `current_artist_identity()` `None`, feuert die Interpreten-Closure
   den `RevealTrack`-Intent statt auszusteigen; liefert
   `current_album_identity()` `None`, tut die Cover-Closure dasselbe. Keine
   Fläche steigt mehr wortlos aus.
6. Direkt darunter (`podcasts_view`, `youtube_view`, `radio_view` sind über
   `RuntimeWiring` `:52–54` in Reichweite) das `set_on_source_reveal`
   verdrahten — genau eine Closure, die nach `SourceTarget` verteilt:
   `Episode { kind: Podcasts }` ⇒ `podcasts_view.request_reveal(…)`,
   `Episode { kind: Youtube }` ⇒ `youtube_view.request_reveal(…)`,
   `Station` ⇒ `radio_view.request_reveal_connected()`.

**Tests** (displaylos, in `source_item_identity.rs`):

- `browse_4_the_title_link_reveals_the_episode`
- `browse_4_the_cover_and_channel_links_reveal_the_channel`
- `browse_4_all_three_radio_links_reveal_the_station`
- `browse_4_a_youtube_episode_targets_the_youtube_place` (Kind-Übersetzung)

Die Navigator-Änderung selbst ist ohne Display nicht prüfbar
(`MetadataNavigator` braucht `Sidebar` und `TrackList`); die Aussage, auf die
es ankommt — *der Auftrag geht auch bei `None` raus* — ist durch AP1s
`browse_4_a_reveal_in_the_open_source_view_yields_no_transition` plus die
Codestruktur belegt (der Callback steht **vor** dem `let Some(place) = …`).
Diese Reihenfolge ist im Kommentar festzuhalten, damit sie kein späteres
Refactoring umdreht.

---

### AP10 — Regelwerk und Kataloge *(zuletzt)*

**Dateien:** `docs/ux-rules.md`, `po/reprise.pot`, `po/{ar,bn,de,es,fr,hi,zh_CN}.po`

1. **Neue Regel `PLAY-12` [active] [gtk]**, direkt hinter `PLAY-11`
   (`ux-rules.md:259 ff.`), Text sinngemäß wörtlich aus Spec Teil E:

   > **PLAY-12** [active] [gtk] — **Die Player-Leiste hat keine toten
   > Flächen.** Titel, Kanal-/Interpretenzeile und Cover sind in jedem
   > Wiedergabemodus Links. Was gerade läuft, ist auffindbar: jede der drei
   > Flächen führt zu dem Ort, an dem das laufende Element in einer Liste
   > steht. Gibt es zu einer Fläche in einem Modus kein eigenes Ziel, führt sie
   > zum nächstgelegenen vorhandenen — nie ins Leere. Eine Fläche darf nur dann
   > unbedienbar sein, wenn überhaupt nichts geladen ist; sie ist dann sichtbar
   > inaktiv, nicht stumm. Beschriftung und Tooltip nennen das tatsächliche
   > Ziel des jeweiligen Modus. Now-Playing- und Info-Panel teilen sich diese
   > Links und diese Beschriftung.

   (Der Regeltext steht auf Englisch, wie das übrige Dokument.)
2. **`BROWSE-4`** (`:3387–3393`) bekommt den Absatz aus Spec Teil E: Titel und
   `Ctrl+L` decken die Episode auf, Kanalzeile und Cover den Kanal, bei Radio
   führen alle drei zur Senderzeile; Ziel ist immer die Quellenliste, nie eine
   Detailseite — **eine offene Kanaldetailseite wird für den Sprung
   geschlossen** (das ist die Verhaltensänderung aus AP7.1.3 und muss hier
   stehen, sonst ist sie nirgends dokumentiert).
3. **`SRC-13`** (`:3962–3973`) bekommt den Satz für den expliziten Auslöser.
   Der bestehende Schlusssatz („an item hidden by the active filter is not
   revealed and the filter is never cleared to reach it“) bleibt **wörtlich
   stehen** — er gilt für die passiven Auslöser. Der neue Satz:

   > A jump the user asked for always reveals, also in the already visible view
   > and regardless of the 1.5-second grace period; it drops exactly those
   > filter facets that would otherwise keep the target hidden, and nothing
   > else.

4. `scripts/check-ux-traceability.sh` laufen lassen. Abdeckung: `play_12_*`
   (AP4), `browse_4_*` (AP1, AP2, AP8, AP9), `src_13_*` (AP3, AP5, AP6, AP7),
   `tip_1d_*` (AP4).
5. **Kataloge.** `po/reprise.pot` mit genau dem Kommando aus
   `scripts/tests/gettext-catalogs.sh:22` neu erzeugen, dann alle sieben
   Kataloge per `msgmerge` nachziehen. `msgcmp` prüft **jeden** Katalog gegen
   die frische `.pot` — die fünf neuen Messages müssen also überall auftauchen.
   `de` und `es` müssen zusätzlich **übersetzt** sein (null unübersetzte
   Messages), die anderen fünf brauchen nur die Einträge.

   | msgid | de | es |
   | --- | --- | --- |
   | `Jump to the playing episode` | Zur laufenden Episode springen | Ir al episodio en reproducción |
   | `Go to the channel` | Zum Kanal | Ir al canal |
   | `Go to the playing station` | Zum laufenden Sender | Ir a la emisora en reproducción |
   | `This episode is no longer in your subscriptions` | Diese Episode ist nicht mehr in den Abonnements | Este episodio ya no está en tus suscripciones |
   | `This station is no longer in your favorites` | Dieser Sender ist nicht mehr in den Favoriten | Esta emisora ya no está en tus favoritos |

   Anredeform an die Nachbareinträge des jeweiligen Katalogs angleichen, falls
   diese durchgehend „Sie“ oder „tú“ verwenden.
6. `help.rs:47–48` (`Ctrl+L`) bleibt unverändert: die Tastenkombination hat
   keine sichtbare Fläche und deshalb keinen modusabhängigen Text.

**Beweislast:** `scripts/check-ux-traceability.sh` und
`scripts/tests/gettext-catalogs.sh`, beide displaylos.

---

## 3. Heikle Stellen

**GTK-Reentranz beim Scrollen.** Das Repo hat dazu Narben (leere Trackliste am
Start: reentranter Schreibzugriff in `gtk_widget_allocate`).
`podcasts_reveal::apply` (`:87–117`) schreibt in eine `Adjustment`, aber
ausschließlich aus einem Tick-Callback, der selbst hinter einem
`idle_add_local_once` sitzt (`:131–134`). Der Kanal-Reveal muss durch **genau
denselben** Weg — kein `set_value` im Klick-Handler, keine zweite
Zentrier-Route. Spec A.2 verlangt das ausdrücklich.

**Stale Widgets nach `render()`.** `podcasts_view.rs:358` ersetzt jedes Widget;
`download_widgets` und die neue `channel_widgets`-Map werden erst in `:401–402`
neu gesetzt. Genau dafür beginnt `center_row` mit einem Idle. Ein
Kanal-Reveal, der das Header-Widget *vor* dem `render()` greift, zentriert ein
abgehängtes Widget — das Widget erst **nach** dem eventuellen `render()` aus
der Map holen.

**Synchroner Filterlauf mitten in einem Borrow.** `apply_filter` auf der
Filterleiste feuert **synchron** `on_changed` → `view.render()`
(`podcasts_filter_bar.rs:167–170` → `podcasts_view.rs:336–339`). Wird das
gerufen, während `groups`, `rows`, `download_states`, `selection`,
`download_widgets`, `channel_widgets` oder `expanded_*` geliehen sind,
paniert es mit `BorrowMutError`. Deshalb steht der Filterschritt in AP7 an
Position 4, **vor** jedem Borrow — und `reveal_loaded_episode` scoped
`expanded_episode_sources.borrow()` schon heute bewusst in einen Block
(`podcasts_view_marker.rs:66–73`), bevor `:80/:85` `borrow_mut()` nehmen. Der
Kanal-Pfad hält dieselbe Disziplin.

**Zielansicht noch nie gerendert.** `route_to_place` → `refresh_and_select` →
`on_select` → `podcasts_view.refresh()` + `show_page`
(`library_shell.rs:218–226`). `refresh` liest die DB und rendert, die Seite
wird im selben Durchlauf gemappt. Der Tick-Callback mit
`MAX_LAYOUT_FRAMES = 60` (`podcasts_reveal.rs:22`, `radio_reveal.rs:26`) trägt
das. Der `pending_reveal`-Token aus AP7 stellt sicher, dass der
`connect_map`-Handler den angeforderten Auftrag ausführt statt `ViewEntered`.

**Wiedergabe ohne geladene Liste.** Läuft eine Episode, während das Modul
abgeschaltet ist, landet `render` auf `MODULE_OFF_PAGE`/`EMPTY_PAGE`
(`podcasts_view.rs:449–457`); der Scroller ist nicht sichtbares Stack-Kind, der
Reveal verpufft nach 60 Frames. Der Auffindbarkeits-Check in AP7 (Schritt 2)
greift hier zuerst und macht daraus einen Toast statt eines stummen Klicks.

**`items_changed` und der Fokus.** Die Podcast-Liste ist ein `gtk4::Box`-Baum,
kein ListModel — die bekannte „`items_changed` setzt die Fokuszeile auf 0“-Falle
greift dort nicht. Radio dagegen ist ein `ColumnView` über
`RadioModel::replace` (`radio_view.rs:387`). Deshalb: `render_rows` in
`RadioView::request_reveal_connected` **nur** anfassen, wenn der Filter
tatsächlich geändert wurde.

**Doppelte Instanz.** `PodcastsView` existiert zweimal
(`source_views.rs:103–114`), `set_playing_episode` geht heute an beide
(`:144–151`). `request_reveal` verwirft in der unzuständigen Instanz still
(AP7, Schritt 1) — der Router darf sich nicht darauf verlassen, die richtige
Instanz zu treffen, und die View darf nicht darauf vertrauen, nur zuständig
gerufen zu werden.

**Reihenfolge in `MetadataNavigator::navigate`.** Der Reveal-Callback steht
**vor** dem Routing. Wer das umdreht, bricht den Kanalsprung aus einer anderen
Ansicht heraus (der `connect_map`-Handler fände keinen Auftrag und liefe auf
`ViewEntered`), ohne dass ein Test es merkt. Der Kommentar an der Stelle ist
Pflicht.

---

## 4. Verifikation

Vollständiger Durchlauf (aus dem Worktree-Root):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p reprise-core
cargo test -p reprise-gnome -- --test-threads=4   # Display-Tests tragen #[ignore]
scripts/check-ux-traceability.sh
scripts/tests/gettext-catalogs.sh
```

Gezielt je Paket:

```bash
cargo test -p reprise-core   browser::navigation::tests     # AP1
cargo test -p reprise-gnome  source_item_identity           # AP2, AP9
cargo test -p reprise-gnome  source_reveal                  # AP3
cargo test -p reprise-gnome  playing_links                  # AP4, AP8
cargo test -p reprise-gnome  podcasts_reveal                # AP5, AP7
cargo test -p reprise-gnome  podcasts_presentation          # AP6
cargo test -p reprise-gnome  radio_filter_bar               # AP6
cargo test -p reprise-gnome  play_12_                       # PLAY-12 quer
cargo test -p reprise-gnome  browse_4_
cargo test -p reprise-gnome  src_13_
```

**Wo die Beweislast liegt** — Display-Tests gelten in diesem Repo **nicht** als
Beweis (die Suite ist im Rudel flaky: je Lauf andere Tests rot, sogar andere
Testanzahl) und laufen in der Codex-Sandbox ohnehin nicht:

| Paket | displayloser Beweis |
| --- | --- |
| AP1 | `reprise-core`-Unit-Tests, vollständig |
| AP2 | `loaded_source_item` als reine Funktion über Session-Fixtures |
| AP3 | `reveal_policy`-Tabelle |
| AP4 | Modus-Iteration über `PlaybackMode::ALL` — trägt `PLAY-12` allein |
| AP5 | `channel_reveal_target` + `reveal_outcome`; Geometrie per Example gemessen |
| AP6 | Facetten-Prädikate über `matches_filter`/`filter_rows` |
| AP7 | `reveal_outcome` / `station_position`; der Widget-Teil ist unbewiesen und darf es sein |
| AP8 | `player_bar_labels`/`panel_labels` (AP4); Zustellung unbewiesen |
| AP9 | `source_reveal_intent`; Callback-Reihenfolge per Kommentar gesichert |
| AP10 | die beiden Skripte |

Braucht ein einzelner Display-Test doch eine Antwort, isoliert fahren — nie im
Rudel:

```bash
xvfb-run -a cargo test -p reprise-gnome <exakter_name> -- --ignored --exact --test-threads=1
```

Kein zweiter `cargo test --workspace` parallel: zwei gleichzeitige Läufe
überbuchen die Kerne (Load > 20, Swap). `--test-threads` immer begrenzen.
