# Netz-Features opt-in — Taskplan (2026-07-18)

Setzt `docs/superpowers/plans/2026-07-18-network-opt-in-beschluesse.md`
**einschließlich seiner Korrekturen nach dem Audit** um. Branch
`feat/network-opt-in`, Basis `main@c2569e8a`.

Regeln gehen als **Sektion T** nach `docs/ux-rules.md` (S ist seit heute von
STYLE-1 belegt). `[geplant]` beim Anlegen, Flip im Implementierungs-Commit.

## Ist-Zustand (auditiert, mit Zeilenangaben)

- **Modul-Maschinerie** `reprise-core/src/modules.rs`: `ModuleDescriptor:12`
  (id, name, description, default_enabled — **kein** `applies_live`),
  `ALL_MODULES:52` = `[NEW_RELEASES, LISTENBRAINZ, LASTFM]`,
  `enabled_key:54` → `module.<id>.enabled`, `is_enabled:58`/`set_enabled:64`.
- **Vorbild für ein live wirkendes Gate**: `ArtistNewsRuntime`
  (`ui/artist_news/artist_news_worker.rs`) — `enabled: Rc<Cell<bool>>:97`,
  aus `is_enabled` geseedet `:104`, `set_enabled:119` schreibt durch,
  `subscribe_enabled:135`, Gate in `request:145`.
- **Cover-Download**: HTTP in `cover_download.rs:187`/`:174`. **Zwei** Pfade:
  `CoverLoader::load_target` (`cover_loader.rs:212`, prüft bereits
  `download_enabled:249`) **und** `CoverDownloadBatch::start`
  (`cover_download_batch.rs:193`), der direkt an den Worker sendet und
  `runtime.enabled` **nicht** konsultiert. Runtime-Flag hardcodiert `true`
  (`cover_download_worker.rs:34`). `CoverLoader` kopiert den Schalter einmalig
  (`cover_loader.rs:38,84`) — deshalb `Rc<Cell<bool>>` statt `bool`.
- **Portraits**: HTTP in `artist_portrait/deezer.rs:61,80`; Einstieg
  `artist_portrait/mod.rs:25` `load_or_fetch`, der **zuerst den Cache** bedient
  (`mod.rs:52-57`). Worker `artist_portrait_worker.rs:27` (`request`).
  `cache_dir()` und `IMAGE_EXTS` sind `pub(crate)` (`cache.rs:16,9`).
- **Lyrics**: HTTP `lyrics.rs:436`; Kette
  `player_lyrics.rs:213 → :57 → start_request:99 → lyrics_worker.rs:45`.
  Der Leerzustand ist **keine** `adw::StatusPage`, sondern eine handgebaute Box
  (`lyrics_view.rs:106-116`) mit einem einzigen Button, der an `on_retry`
  hängt (`:120`). Stack-Seiten `:25-27`.
- **Plugins-Seite** `ui/preferences/preferences.rs:637`: Sonderlocken per
  String-ID an vier Stellen (`:645`, `:649`, `:671`, `:690`, `:702`), dazu
  `preference_plugins.rs:5/8/20`.
- **Migrationen**: `db.rs`, Kopf **v12** (`:447`), Muster rein
  `execute_batch` einer statischen SQL-Konstante in einer Transaktion mit dem
  Versions-Bump (`:326-351`).
- **Einmal-Flag-Vorbild**: `ONBOARDING_COMPLETED_KEY`
  (`library/settings.rs:17`, `get/set` `:96/:100`).
- **Hinweis-Platz**: Album-Grid `album_view.rs:181` (vertikale Box, Header dann
  Stack), Artists `artist_master.rs:137-141` (identisch). Es gibt **kein**
  wiederverwendbares dismissbares Inline-Banner; `adw::Banner`
  (`preference_rhythmbox.rs:145`) hat kein ×.

## Tasks (strikt in Reihenfolge)

### T1 · Regeln anlegen (Sektion T)

- NET-1, NET-2, LYR-2, LYR-3, DISCOVER-1, DISCOVER-2 als `[geplant]`.
  **LYR-1 ebenfalls anlegen, bleibt aber dauerhaft `[geplant]`** (Korrektur 1)
  mit dem Vermerk, dass lokale Songtexte noch nicht existieren.
- Commit: `docs(ux-rules): add section T — network features are opt-in`

### T2 · `applies_live` am Deskriptor + drei neue Module

- Red: Modul-Defaults (`cover_download`, `artist_portraits`, `online_lyrics`
  alle `default_enabled: false`), `applies_live` für alle drei `true`;
  `ALL_MODULES` enthält sie in sinnvoller Reihenfolge.
- Green: `ModuleDescriptor` um `applies_live: bool` erweitern; die
  String-ID-Sonderlocke `plugin_applies_live` (`preference_plugins.rs:5`)
  entfällt und liest das Feld. Bestehende Tests
  (`modules.rs:105`, `:170`, `preferences.rs:765`) mitziehen.
- Commit: `feat(modules): describe live-applying modules and register the three network ones`

### T3 · Migration: Bestandsschutz + verwaistes Flag (NET-2)

- Red: `net_2_migration_preserves_existing_cover_usage` [core] plus Fälle für
  Portraits, Lyrics (DB existiert → an) und **das verwaiste
  `module.artist_news.enabled` → `module.new_releases.enabled`** (Korrektur 4).
- Green: v13-Schritt, der in **derselben Transaktion** eine Rust-Funktion ruft
  (reines SQL kann kein Dateisystem prüfen). **Beide Cache-Verzeichnisse als
  Parameter injizieren**, damit Tests nicht vom echten `~/.cache` abhängen;
  `.notfound`-Marker zählen **nicht** als Nutzung. Frische DB → alle aus.
- Flip: **NET-2 → [aktiv]**.
- Commit: `feat(db): grandfather existing network-feature usage (NET-2)`

### T4 · Gates für Cover und Portraits (NET-1)

- Red: `net_1_cover_download_respects_the_module` — **beide** Pfade
  (`CoverLoader` *und* `CoverDownloadBatch`) laden nichts, wenn das Modul aus
  ist; `net_1_portraits_keep_cached_images_when_disabled` — bei
  ausgeschaltetem Modul werden **gecachte** Portraits weiterhin angezeigt.
- Green: `CoverDownloadRuntime`/`ArtistPortraitRuntime` bekommen
  `enabled: Rc<Cell<bool>>` nach dem `ArtistNewsRuntime`-Muster;
  `CoverLoader` hält den `Rc` statt einer Kopie. **`reprise-core` bekommt einen
  reinen Cache-Pfad für Portraits** — das Gate wählt, welche Core-Funktion der
  Worker ruft, statt den Versand zu unterdrücken.
- Flip: **NET-1 → [aktiv]**.
- Commit: `feat(covers,portraits): gate downloads behind opt-in modules (NET-1)`

### T5 · Lyrics-Gate und Aktivierungs-Leerzustand (LYR-2, LYR-3)

- Red: `lyr_2_fetch_only_when_enabled` — bei ausgeschaltetem Modul wird
  **kein** LRCLIB-Request abgesetzt; `lyr_3_disabled_state_offers_activation`
  [gtk] — der Leerzustand trägt Icon, Titel, Untertitel und einen Button.
- Green: Gate in `PlayerLyrics::start_request` (`player_lyrics.rs:99`), damit
  die View den Aktivierungszustand zeigen kann statt stumm nichts zu tun.
  Vierte Stack-Seite mit echter `adw::StatusPage` (die vorhandene Box hat
  keinen Icon-Slot und ihr Button hängt an `on_retry`).
  **Die Fußnote „Eingebettete Songtexte werden immer angezeigt" NICHT
  einbauen** — sie verspräche LYR-1, das es nicht gibt (Korrektur 1).
  Deep-Link nach dem `device_view`-Muster: Setter auf `LyricsView`, verdrahtet
  in `window.rs` nahe `:507`, wo `PreferencesContext` existiert.
- Flip: **LYR-2 → [aktiv]**, **LYR-3 → [aktiv]**.
- Commit: `feat(lyrics): opt-in fetching with an activation empty state (LYR-2/3)`

### T6 · Entdeckungszeile (DISCOVER-1, DISCOVER-2)

- Red: `discover_1_hint_needs_visible_evidence` — Hinweis erst ab **≥ 3
  gleichzeitig sichtbaren** Fallback-Kacheln bzw. Initialen-Avataren;
  `discover_1_hint_latches_and_never_returns` — einmal gezeigt bleibt er
  stehen (kein Flackern beim Scrollen), Dismiss ist dauerhaft;
  `discover_2_combined_line_when_both_apply` — treffen Portrait- und
  NR-Hinweis in der Artists-Ansicht zusammen, erscheint **eine** kombinierte
  Zeile, nie zwei.
- Green: kleine dismissbare Inline-Zeile (es gibt keine wiederverwendbare —
  `adw::Banner` hat kein ×), eingefügt zwischen Header und Stack
  (`album_view.rs:181`, `artist_master.rs:137`). Zähler an der Kachel-/
  Avatar-Erzeugung, **keine bibliotheksweite Vorberechnung**. Flags nach dem
  Muster `ONBOARDING_COMPLETED_KEY`, Namensraum `hint.<id>.shown` — dafür ist
  **keine** Migration nötig (settings ist Key/Value).
- Flip: **DISCOVER-1 → [aktiv]**, **DISCOVER-2 → [aktiv]**.
- Commit: `feat(discovery): one-time contextual hints for the opt-in features (DISCOVER-1/2)`

### T7 · Plugins-Seite aufräumen

- Green: die vier Netz-Module mit Privacy-Untertiteln; die verbliebenen
  String-ID-Sonderlocken so weit zurückbauen, wie es ohne Umbau der
  ListenBrainz-/Last.fm-ExpanderRows geht. **Zielzeile hervorheben**, wenn ein
  Deep-Link sie anspringt (kurzlebiger CSS-Zustand, MOT-Token, kein
  Dauerblinken) — dafür muss `plugins_page` die Zeilen-Handles beim Bauen
  ablegen, weil die Seite bei jedem `open()` neu entsteht.
- Commit: `feat(preferences): plugin rows for the network features with deep-link highlight`

## Gates vor jedem Commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`
- `scripts/check-architecture.sh`
- Display-Tests **einzeln je Prozess**: `xvfb-run -a scripts/check-display-tests.sh`
- **Neue UI-Strings sofort übersetzen**: `po/de.po` muss ohne unübersetzte und
  ohne fuzzy Einträge bleiben (`msgattrib --untranslated`/`--only-fuzzy` leer),
  sonst bricht der Release-Check. Glyphen/Symbole werden **nicht** mit `N_!`
  markiert.

## Abnahme (manuell)

Frische Installation → alle vier Module aus, keine Netzaktivität. Update einer
Bestands-DB mit Cover-Cache → Cover-Download bleibt an. Album-Grid mit vielen
Fallback-Kacheln → **eine** Hinweiszeile, die beim Scrollen stehen bleibt, nach
× nie wieder. Artists-Ansicht mit fehlenden Bildern und neuen Releases → **eine
kombinierte** Zeile. Lyrics-Tab bei ausgeschaltetem Modul → StatusPage mit
Aktivieren-Button, der in die Plugins-Seite springt und die Zeile hervorhebt.
