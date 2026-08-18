---
slug: device-sync-bars-join-the-page
worktree: /home/marvin/Projects/reprise-device-sync-bars-join-the-page
branch: feature/device-sync-bars-join-the-page
phase: shipped
codex_session:
created: 2026-08-12
---
# Die beiden Balken der Geräteseite schließen sich der Seite an

Dach-Plan: `device-sync-rebuild.OVERVIEW.md`. Dessen Abschnitt „Globale
Vorgaben" gilt auch hier.

**Reihenfolge: vor Welle 1.** Dieser Plan fasst `device_sync_dock.rs` an, das
Plan E ebenfalls anfasst, sowie `device_sync_page_layout.rs` (Plan D) und
`device_sync_strings.rs` (D und E). Er ist klein und überwiegend mechanisch —
D und E rebasen danach, nicht umgekehrt.

## Herkunft und ein Namenskonflikt

Entwurfsquelle: Claude-Design-Projekt `e7d4a2fe-46d5-4757-8fae-19542ab6ff8d`,
Datei `Device Sync Layout.dc.html`. Der dortige Ticket-Text nennt sich selbst
„MTP-62" — **diese Nummer ist vergeben**: Plan E (`device-sync-remembered-state.md:229`)
hält sie für den erinnerten Zustand. Dieser Plan bekommt deshalb **MTP-65**.

**Die Farben des Entwurfs werden nicht übernommen.** Der Mock läuft auf der
Nocturne-Palette (`--color-neutral-*`, `--color-accent-*`); übernommen wird
allein das *Wertverhältnis*: der Dock liegt eine Stufe über der Seitenfläche
und trägt eine sichtbare Oberkante, der Banner-Streifen trägt eine sichtbare
Unterkante. Umgesetzt wird das mit libadwaita-Namensfarben
(`@headerbar_bg_color`, `@window_bg_color`, `alpha(@window_fg_color, …)`,
`@accent_color`) über die bestehende CSS-Kette: jedes UI-Modul liefert seine
Regeln als `css()`, eingesammelt in `ui/style/mod.rs:101` (`app_css`) — siehe
`sidebar_device_card::css()` als Vorbild. Keine Literalfarben, keine neue
Palette. Auch die Maße des Mocks (Schriftgrade, Polsterungen in px) sind
Richtung, nicht Vorgabe — GNOME-Klassen statt Pixel.

## Der Befund, nachgemessen

Sichtprüfung am 12.08.2026 gegen `dev` @ `5ce6b3d8c2`, echter Desktop,
isoliertes Profil, `GDK_BACKEND=x11` (Fenster 2980×1900 physisch):

- **Dock**: Füllung `rgb(39,45,51)` — identisch mit der Kartenfüllung, über die
  er liegt. Die Player-Leiste darunter ist `rgb(38,43,49)`, getrennt nur durch
  eine 2 px-Haarlinie bei y=1676. Zwei Balken, ein Farbton: der untere Rand
  wirkt als ein einziger grauer Block. Die Dock-Oberkante (y≈1539) schneidet
  bei mittlerer Scrollposition Text mitten durch, ohne jede Kantenbehandlung.
  Der Dock läuft von x=530 bis x=2927, die Karten enden bei x≈2720 — „Sync now"
  steht damit 175 px weiter rechts als die Inhaltsspalte.
- **Banner**: Kopfleiste `rgb(30,33,37)`, Bannerfläche `rgb(29,31,34)` — einen
  Wert *dunkler* als die Kopfleiste, also keine erkennbare Bannerfläche. Der
  Streifen endet bei x≈2854, das Fenster bei x≈2929; in diesen ~75 px sitzt
  „Not now" auf dem nackten Fensterhintergrund. Im AT-SPI-Baum ist das
  eindeutig: `[41] "Review in Preferences"` liegt **in** der Bannergruppe,
  `[42] "Not now"` als **Geschwister daneben**.

## Was der Code festnagelt — nicht umbauen

- `mtp_60_the_sync_bar_is_not_inside_the_scrollview`: der Dock ist bereits
  Geschwister des Scrollers und bleibt es. `dock.root()` bleibt direktes Kind
  der Dashboard-Root (`device_sync_page_layout.rs:183`).
- `device_page_sections_have_one_explicit_owner_and_order`: die Inhaltsspalte
  hat genau drei Kinder, „On this device" bleibt letztes. Keine neue
  Top-Level-Sektion.
- `playlist_and_sync_overview_cards_share_the_same_edges`: „Music transfer
  profile" und „Playlist changes" bleiben ein `adw::WrapBox`-Paar gleicher
  Größe, beide Überschriften bleiben **in** ihren Karten.
- `mtp_61_the_rules_block_carries_both_device_switches`: beide Schalter bleiben
  Nachfahren von `on_device.root()`.

## Die sechs Eingriffe

### 1 — Der Dock beginnt, wo die Seite beginnt

`device_sync_dock.rs`: den Inhalts-Box des Docks in einen `adw::Clamp` mit
`device_sync_page_layout::CONTENT_MAX_WIDTH` (1120) und denselben 32 px
Start-/End-Rändern wie die Inhaltsspalte legen. Die 32 aus
`device_sync_page_layout.rs:163` in eine benannte Konstante neben
`CONTENT_MAX_WIDTH` ziehen und an **beiden** Stellen verwenden — zwei
unabhängige 32er sind die Ursache der Drift.

Dazu bekommt die Dock-Root eine eigene Fläche mit sichtbarer Oberkante, damit
sie als Boden der Seite liest und nicht mit der Player-Leiste verschmilzt:
`css()` im Modul, registriert in `ui/style/mod.rs`, Fläche und Kante aus
libadwaita-Namensfarben. `dock.root()` bleibt direktes Kind — die Ahnenschaft
in `mtp_60…` mit `debug_assert` bzw. im Test festhalten, nicht verschieben.

### 2 — Der Banner wird ein Streifen statt eines AdwBanner mit Anbau

`online_discovery_banner.rs`: heute ein `adw::Banner` (ein Button, zentrierter
Titel) mit einem zweiten `gtk4::Button` in einer selbstgebauten Box daneben —
daher der zentrierte Text und der Knopf außerhalb der Polsterung. Ersetzen
durch **einen** horizontalen Streifen mit Stilklasse `.toolbar`: Label
linksbündig (`xalign 0`, umbruchfähig), rechts „Not now" (flat) und „Review in
Preferences" (`suggested-action`), plus sichtbare Unterkante, damit die
Oberkante der Seitenleiste weiter liest.

**Die Einhängung bleibt, wo sie ist**: `window/window.rs:481` hängt ihn als Top-Bar
von `library_chrome.root` ein — die Meldung gilt der ganzen Bibliothek, nicht
dieser Seite. Ein Umzug in die Inhalts-`toolbar_view` würde nichts eingrenzen
(`content_stack` trägt jede Seite), nur schmaler machen. Die Breite ist nicht
der Defekt.

Test `net_4_discovery_banner_persists_review_and_dismiss_actions_before_hiding`
prüft heute `banner.title()` und `button_label()`; er prüft künftig den
Labeltext und die **zwei** Buttonbeschriftungen. Einmaligkeit und beide
Persistenzpfade bleiben unverändert.

### 3 — Gleich große Nachbarn, gleiche Überschriftenebene

`device_sync_page_layout.rs:111`: `profile_title` trägt `title-2`,
`changes_heading` (Zeile 130) trägt `heading`. Beide bekommen `heading`. Keine
Überschrift verlässt ihre Karte.

### 4 — Sieben Nullen werden ein Satz

`reprise-view` `device_sync::change_summary` (`crates/reprise-view/src/device_sync.rs:202`):
sind alle Felder von `SyncChangeSummary` null, liefert die Funktion **eine**
Meldung „Nothing transferred yet." statt sieben Null-Klauseln. Die Karte selbst
bleibt (Paar-Invariante). Der neue String läuft durch den bestehenden
`Message`/`ngettext`-Pfad, damit die POT-Gegenprobe grün bleibt.

### 5 — Die Regeln bekommen eine eigene Karte

`device_sync_on_device.rs:210/212`: zwei `gtk4::Separator` trennen heute den
Regelblock von der Bilanz, dazu eine verschachtelte `heading`-Beschriftung in
einer Karte, die schon eine Überschrift hat. Stattdessen: die Regeln in eine
eigene `.card`-`Bin`, angehängt an die `on_device`-Root **nach** der
Bilanzkarte, beide Separatoren entfallen, die Überschrift wird erste Zeile
dieser Karte. Die Schalter bleiben Nachfahren von `on_device.root()`, die
Inhaltsspalte behält genau drei Kinder.

### 6 — Der Tooltip nennt den Grund, nicht den Zustand

`device_sync_on_device.rs:158`: `set_limit` trägt als Tooltip
`device_sync_strings::NO_SIZE_LIMIT` — den aktuellen Zustand, nicht den Grund
für die Deaktivierung. Eigener String, der den Grund nennt (Größenlimits sind
noch nicht umgesetzt).

## Die neue Regel

- **MTP-65** `[gtk]` — Die beiden waagerechten Balken der Geräteseite beginnen
  an derselben Kante wie ihre Inhaltsspalte und tragen eine eigene Fläche: der
  Sync-Dock spannt seinen Inhalt in denselben Clamp wie die Karten und grenzt
  sich mit einer sichtbaren Oberkante gegen die Player-Leiste ab; der
  Entdeckungs-Streifen setzt seinen Text linksbündig, hält beide Schaltflächen
  innerhalb derselben Polsterung und grenzt sich mit einer sichtbaren
  Unterkante gegen die Seitenleiste ab.

Regeltext in die Rulebook-Sektion des Geräte-Sync eintragen und
`scripts/check-ux-traceability.sh` grün halten (steht bei 365 aktiven Regeln).

## Nicht in diesem Ticket

- Die rote Meldung („Cannot synchronize" / „Select at least one playlist to
  synchronize.") aus dem Dock in die Playlist-Karte zu verschieben. Richtig,
  aber die Strings sind in `device_sync_page_tests.rs:191,328` festgenagelt und
  der dann überflüssige Link „Review playlists above" widerspricht
  `mtp_61_on_this_device_offers_no_playlist_selection`. Eigenes Ticket, mit
  diesen zwei Tests.
- Aus der Sichtprüfung offen, jeweils andere Fläche: der Playlist-Kartenkopf
  ohne seine Zahl („unique tracks · 0 B on device"), die Seitenleistenkarte, die
  „Up to date" sagt, während die Seite „Never synchronized" meldet, und dieselbe
  Karte, die den Zeitpunkt auf „Not connected · s…" kappt.
- `DeviceBackend::eject` hat eine Default-Implementierung, die `Ok(false)`
  liefert (`device_sync_types.rs:93`); das simulierte MTP-Backend überschreibt
  sie nicht. Auswerfen ist dort ein stiller Nulleingriff — ohne Logzeile und
  ohne Rückmeldung an die Bedienung. Eigenes Ticket.

## Nicht anfassen

Fortschritts-, Metrik- und Abbruch-Projektionen des Docks, der Speicherbalken,
der Clamp-Wert selbst (`NPP-1` nagelt ihn fest), die Verifikationstexte und der
Picker.

## Abnahme

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (rot auf dev und **nicht** die eigene Schuld:
  `mtp_51_filtered_picker_escape_leaves_the_dialog` und
  `search_4a_runtime_window_escape_reaches_section_search` — beide hängen am
  Fenstermanager)
- `scripts/check-architecture.sh`, `scripts/check-ux-traceability.sh`,
  `scripts/check-frontend-thinness.sh` (**Ratsche**: wird auch rot, wenn das
  Budget unterschritten wird — dann die Zahl im Skript nachziehen),
  `scripts/check-motion-tokens.sh`, gettext plus POT-Gegenprobe
- **`scripts/check-merge-readiness.sh` nicht starten** — die Stufe „rule-named
  display tests" fährt 437 einzelne cargo-Tests mit eigenem Xvfb und wird nie
  fertig.
