---
slug: list-views-fixes
worktree: /home/marvin/Projects/reprise-list-views-fixes
branch: feature/list-views-fixes
phase: shipped
codex_session:
created: 2026-07-30
---
# Plan: Podcasts- & YouTube-Listenansichten korrigieren

Quelle: Claude-Design-Projekt `8fb24732`, Datei `agent-prompt-list-views-fixes.md`
(Frames 9a, 6a, 6b, 8c in `Tourdaten Varianten.dc.html`).

Basis: `origin/dev` @ `b5467e6ae5` — nur dieser Stand enthält die gruppierte
Listenansicht (`podcasts_groups.rs`, `source_image.rs`,
`youtube_channel_detail.rs`); `main` und `feature/podcasts-radio` sind älter.
Arbeits-Worktree: `../reprise-list-views-fixes`, Branch `feature/list-views-fixes`.

## 1. Befundlage (verifiziert am Code, nicht geraten)

| # | Symptom | Fundstelle | Ursache (Stand Recherche) |
|---|---------|-----------|---------------------------|
| A1 | Cover ~600 px, eine Show füllt das Fenster | `source_image.rs:104-114` | `Gtk::Picture` mit `can_shrink(true)` + `set_size_request(40,40)`: `size_request` ist nur ein **Minimum**, die natürliche Messung bleibt die volle Texturgröße. Kein `halign/valign`, kein Downscale der Textur. |
| A2 | Titel mitten im Wort abgeschnitten | `feed.rs:209-211` | `builder.title.get_or_insert_with(...)` übernimmt nur das **erste** Text-Event. quick-xml liefert nach einer Entity (`&amp;`, `&#39;`) bzw. einem CDATA-Wechsel ein weiteres Text-Event → Rest fällt weg. Erklärt die unbalancierte Klammer. |
| A3/B6 | `0.0 MB` | `podcasts_presentation.rs:159-171` | `file_size(Some(0))` formatiert `0.0 MB`, statt nichts zu liefern. |
| A4 | redundante Autorenzeile | `podcasts_groups.rs:153-164` | Autor wird ungeprüft gerendert. |
| B5 | kein Publikationsdatum bei YouTube | `youtube.rs:34-42`, `pipeline.rs:400-418` | Für `/channel/UC…` läuft der Atom-Pfad (mit `<published>`), für alle anderen Kanal-URLs (`@handle`, `/c/…`) der yt-dlp-Flat-Playlist-Pfad → `published_at: None`. `YtDlpVideo` trägt weder `timestamp`/`upload_date` noch Thumbnail. **Vor dem Fix zu verifizieren**, welcher Pfad die real abonnierten Kanäle nimmt. |
| C7 | `0:53` mehrdeutig | `podcasts_presentation.rs:149-157` | Format ist `h:mm`. |
| C8 | „15 episodes · 15 new" | `podcasts_presentation.rs:61-106` | „new" = `played_at.is_none()`, also beim Erstabo alles. `podcast_subscriptions.added_at` existiert bereits und wird nicht genutzt. |
| C9 | `Not downloaded` auf jeder Zeile | `podcasts_groups.rs:343-345` | Negativzustand wird gelabelt. |
| C10 | doppelte Zeilenhöhe, Play-Knopf je Zeile | `podcasts_groups.rs:236-299` | Play-Button + 7 px Margins + 130-px-Statusspalte. |
| C11 | alle Episoden expandiert | `podcasts_groups.rs:101-113` | Kein Fenster; `core::podcasts::channel_window` existiert, wird aber nur von der Kanalseite genutzt. |
| C12 | kein Episoden-Artwork | `feed.rs`, Schema | `podcast_episodes` hat keine `image_url`-Spalte; `itunes:image`/`media:thumbnail` werden nicht geparst. |
| C13 | YouTube-Titel wiederholen ihren Tail | `podcasts_groups.rs:259` | Titel wird ungeteilt gerendert. |
| C14 | Stern nur bei einer Show | `podcasts_groups.rs:192-214` | Hover-Controller sitzt auf der Header-`Box` innerhalb des `Expander`-Labels; die Box füllt die Zeilenbreite nicht, Enter/Leave feuert dadurch uneinheitlich. |
| Copy | „Add YouTube channel" | `strings_podcasts.rs:29` | `YOUTUBE_ADD`. |

Beide Ansichten teilen `podcasts_presentation.rs` (Datum/Dauer/Größe/Status) —
Formatfixes wirken automatisch auf Liste **und** Kanalseite.

## 2. Entscheidungen, die ich treffe (statt zurückzufragen)

1. **Dauer** (C7): `< 1 min` → `< 1 min`; `< 1 h` → `53 min`; sonst `2 h 05`.
   Unbekannt → leerer String, das Trennzeichen entfällt mit.
2. **„New"** (C8): `is_new` wird im Core berechnet — `episode.first_seen_at >
   subscription.added_at`. Beim ersten erfolgreichen Fetch schreibt der Store
   `first_seen_at = subscription.added_at`, damit der Backlog exakt (nicht
   sekundenabhängig) unter die Schwelle fällt. Keine „als gespielt markiert"-Lüge,
   keine Schemaerweiterung dafür nötig.
3. **Stern** (C14): nur bei Hover, aber der Controller wandert auf den
   `Expander` bzw. die volle Zeilenbreite — eine Regel für beide Ansichten.
   Zusätzlich sichtbar bei Tastaturfokus (sonst wäre die Aktion nur mit Maus
   erreichbar, was die Accessibility-Gates des Repos als Defekt behandeln).
4. **Zustands-Label** (C9): nur `NotDownloaded` verliert sein Label. `Queued`,
   `Downloading`, `Downloaded`, `Missing`, `Failed` bleiben — das sind aktive
   bzw. Fehlzustände.
5. **Fenster** (C11): 10 Episoden je Gruppe, danach eine Zeile
   `Alle 15 Episoden anzeigen`, die nur diese Gruppe aufklappt (lokaler
   View-State neben `expanded_sources`, nicht persistent).
6. **Episoden-Artwork** (C12): neue Spalte `podcast_episodes.image_url`,
   Migration `v49`, `SUPPORTED_SCHEMA_VERSION 48 → 49`. Fallback-Kette
   Episodenbild → Showcover → Glyph.
7. **Titel-Tail** (C13): reine Funktion, die den längsten gemeinsamen Suffix der
   Episodentitel einer Gruppe ab einem Trenner (`|`, `–`, `-`, `•`) ermittelt und
   als gedimmtes Pango-Markup rendert. Greift erst ab ≥ 3 Episoden mit demselben
   Suffix, damit ein Einzeltitel nie zerschnitten wird.

## 3. Arbeitspakete

Ownership ist strikt: kein Paket editiert eine Datei eines anderen. P1–P3 sind
untereinander unabhängig und können parallel laufen; P4 und P5 setzen auf P1/P3 auf.

### P1 — Formatierung & Textlogik (rein, testbar ohne Display)
Dateien: `podcasts_presentation.rs`, neu `podcasts_title.rs`
- `duration()` auf `53 min` / `2 h 05` / leer umstellen (C7).
- `file_size()` → `Option<String>`: `0` und negativ ⇒ nichts (A3/B6).
- `relative_date()` behält `Today`/`Yesterday`, liefert bei `None` leer statt `—`
  (Akzeptanz: keine `—`-Platzhalter).
- Detailzeile baut sich aus den **nichtleeren** Teilen, damit nie `— · · New` entsteht.
- `author_line()`: unterdrückt Autor, wenn er Präfix von / identisch mit dem Titel ist (A4).
- `podcasts_title.rs`: gemeinsamer Suffix + Split in (distinkt, gedimmt) (C13).
- Tests: Tabellentests je Funktion, inkl. Grenzfälle 59 s / 60 s / 3599 s / 3600 s.

### P2 — Zeilen-Anatomie & Cover (GTK)
Dateien: `source_image.rs`, `podcasts_groups.rs`, `css.rs`
- `SourceImage`: Textur beim Laden auf `2 × size` skalieren und **so** cachen;
  `Picture` mit `halign/valign = Center`, `hexpand/vexpand = false`; Stack mit
  fixer Größe. Zeilenhöhe muss identisch sein für „Cover da / lädt / fehlt" (A1).
- Episodenzeile: Play-Button raus (Aktivierung spielt), Margins auf ~4 px,
  zwei Zeilen, Ziel ~46 px; Download-Icon + `⋮` bleiben (C10).
- `NotDownloaded` ohne Label, Statusspalte nur bei aktiven Zuständen breit (C9).
- Episoden-Thumbnail links (16:9 bei YouTube, quadratisch bei RSS), Play-Glyph
  bei Hover darüber (C10/C12-UI-Teil).
- Hover-/Fokus-Regel für den Stern vereinheitlichen (C14).
- Tests: Display-Tests (`#[ignore]`, xvfb) — gemessene Zeilenhöhe ≤ 52 px mit und
  ohne Cover; kein Play-Button in der Zeile; kein Label bei `NotDownloaded`.

### P3 — Core: Feed-Parsing, YouTube-Datum, Artwork
Dateien: `feed.rs`, `youtube.rs`, `ytdlp.rs`, `pipeline.rs`, `store.rs`,
`db_podcasts_radio.rs`, `db.rs` (nur Versionskonstante)
- **A2**: Textsegmente je Element akkumulieren statt `get_or_insert_with`
  (Regressionstest mit `&amp;` und CDATA im Titel).
- **B5**: (a) Kanal-Identität beim Anlegen und beim Refresh über `channel_id`
  auflösen, sodass der keyless Atom-Feed auch für `@handle`-Kanäle greift;
  (b) `YtDlpVideo` um `timestamp`/`upload_date` erweitern als Fallback;
  (c) Upsert trägt ein fehlendes `published_at` bei bestehenden Zeilen nach.
  Vorgeschaltet: Verifikation am echten Kanal, welcher Pfad läuft.
- **C12**: `itunes:image`/`media:thumbnail` je Item parsen, Spalte `image_url`
  (Migration v49), `EpisodeRow.image_url`.
- Tests: Feed-Fixtures (RSS mit Entity-Titel + `itunes:image`, YouTube-Atom mit
  `published` + `media:thumbnail`), Migrationstest v48→v49.

### P4 — Core: „New"-Semantik (nach P3, gleiche Store-Dateien)
Dateien: `query.rs`, `store.rs`, `podcasts_presentation.rs` (Anschluss nach P1)
- `EpisodeRow.is_new` aus `first_seen_at > subscription.added_at`.
- Erstfetch setzt `first_seen_at = added_at`.
- `SourceSummary.unplayed_count` → `new_count`, `LibrarySummary.new` analog;
  `status_pill` nutzt `is_new` statt „ungespielt".
- Tests: frisches Abo mit 15 Backlog-Episoden ⇒ `0 new`; nächster Refresh mit
  einer neuen Episode ⇒ `1 new`.

### P5 — Episodenfenster in der Liste (nach P2)
Dateien: `podcasts_groups.rs`, `podcasts_view.rs`
- 10 Episoden je Gruppe + „Alle N Episoden anzeigen"-Zeile (C11).
- Tests: Gruppe mit 15 Episoden rendert 10 Zeilen + 1 Aktionszeile.

### P6 — Copy & i18n (jederzeit, isoliert)
Dateien: `strings_podcasts.rs`, `po/*.pot|po`
- `YOUTUBE_ADD` → „Add channel"; neue Strings aus P1/P5 aufnehmen,
  `POTFILES.in` prüfen, `.pot` regenerieren.

## 4. Verifikation

1. `cargo test -p reprise-core -p reprise-gnome` (Einzelläufe für die
   Display-Tests via `xvfb-run`, weil die Suite im Rudel flaky ist).
2. `cargo clippy --workspace --all-targets -- -D warnings`.
3. Headless-Screenshot der beiden Ansichten (bestehender cage+grim-Harness) und
   Abgleich gegen die Akzeptanzliste:
   - mehrere Shows pro Bildschirm, Zeilenhöhe unabhängig vom Cover,
   - keine abgeschnittenen Titel, keine `—`, kein `0.0 MB`,
   - Dauer eindeutig oder abwesend,
   - frisches Abo zeigt `0 new`,
   - Leerlaufzeilen ohne Statustext, ~doppelte Zeilendichte.
4. Migrationspfad einmal gegen eine Kopie der echten DB (read-only URI, WAL beachten).

## 5. Risiken

- **B5** ist der einzige Punkt mit offener Ursache; falls yt-dlp für die
  betroffenen Kanäle kein Datum liefert und die Atom-Auflösung scheitert, bleibt
  laut Vorgabe „nichts anzeigen" statt eines erfundenen Werts — das wäre dann
  eine bewusst dokumentierte Teil-Lieferung.
- Schemaversion 49 kollidiert, falls parallel auf `dev` eine weitere Migration
  landet — vor dem PR gegen `origin/dev` rebasen und die Nummer prüfen.
- Zeilenhöhe ~46 px ist mit zwei Textzeilen + `caption` nur erreichbar, wenn das
  CSS die Zeilenabstände mitzieht; ggf. Meta-Zeile auf `caption` mit
  reduziertem `line-height`.
