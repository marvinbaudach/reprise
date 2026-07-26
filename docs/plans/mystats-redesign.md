---
slug: mystats-redesign
worktree: /home/marvin/Projects/reprise-mystats-redesign
branch: feature/mystats-redesign
phase: shipped
codex_session:
created: 2026-07-24
---
# My Stats Redesign („Variante 2a — Eine Erzählung") — Implementierungsplan

Status: FINAL. Gegrillt und konvergiert am 2026-07-24; alle Entscheidungen
(D1–D16) sind verbindlich. Der Plan ist für einen headless Codex-Agenten ohne
Rückfragen umsetzbar.

Sprachhinweis: Design-Docs sind deutsch (AGENTS.md); Code, Tests, UI-Strings und
Bezeichner bleiben englisch. Regeltexte für `docs/ux-rules.md` stehen wörtlich in
Abschnitt 4.

Ziel: Die bestehende editoriale „My Stats"-Seite (Frame 25a, Plan
`mystats-optimization.md`, vollständig in `dev` gemerged) wird auf die gewählte
Design-Variante 2a umgebaut. Die Seite erzählt von oben nach unten:
**Gesamtzahl → Trend → Band → Songs → Genres.** Verbindliche Spec + exakte
Mockup-Werte: Scratchpad-Dokument `mystats-redesign-design-context.md` (der
dortige Nutzer-Prompt ist die Design-Wahrheit; Nocturne-Hexwerte sind NICHT zu
kopieren, siehe D11).

Nicht verhandelbar (aus AGENTS.md / ux-rules.md, gilt für jeden Task):
Play-Definition nur `listen_events` (STATS-0), Grouping via `group_key`-Fold
(STATS-9), Bucketing timezone-aware in Rust (D3 des Vorgängerplans), Motion nur
über Tokens aus `motion.rs` (MOT-1, `scripts/check-motion-tokens.sh`),
`gtk-enable-animations` respektieren (MOT-7), 800-Zeilen-Limit,
Regel-IDs append-only mit Test-Traceability (`scripts/check-ux-traceability.sh`),
Gates `cargo fmt --check` / `clippy --all-targets --workspace -- -D warnings` /
`test --workspace` / `cargo audit` / Core-Purity-Grep.

---

## 1. Ist-Zustand (verifiziert am Code, Stand dev 2026-07-24)

* Composer `crates/reprise-gnome/src/ui/stats/stats_view.rs` (756 Zeilen, nahe
  Limit): Hero → Ribbon → Spotlight → Genres → Clock+Highlights → Top Tracks;
  Leer-/Fehlerzustand über `page_stack` („sections" / „empty" / „failed").
* Sektionen: `stats_hero.rs` (Zeit + Pill + Subline + Perioden-`DropDown` + ⋮),
  `stats_ribbon.rs`/`stats_ribbon_math.rs` (Cairo-Flächenchart, Peak-/Open-Dots,
  Hover-Tooltip), `stats_spotlight.rs` (Artist-Karte, Cover 150 px, Chips,
  Play/Go-to-artist/Unify), `stats_genre_bar.rs` (Segment-Grid + Legende,
  display-only), `stats_highlights.rs`, `hourly_chart.rs`, `stats_customize.rs`,
  `stats_metadata_links.rs` (`StatsMetadataTarget`), `stats_css.rs`,
  `stats_view_widgets.rs`.
* Core: `stats_snapshot.rs` (`compute` = pure Funktion, 8 Statements),
  `stats_period.rs` (`StatsPeriod`, `granularity_for`: ≤45 Tage Day, ≤120 Week,
  sonst Month; `apply_activity_granularity`), `stats_screen.rs` (`ListenRow`,
  `NamedRow` mit `path`, `RankedGroup.representative_track_path`, `TopTrack`,
  `group_track_ids`), `group_key.rs` (Fold, `KeyResolver`).
* Navigation: `NavigationIntent::{OpenArtist{artist, anchor_track_id},
  OpenAlbum{album, anchor_track_id}, RevealTrack}` existiert und ist für die
  Stats-Metadaten-Links bereits verdrahtet (`window_action_wiring.rs:142-221`).
  „Library gefiltert auf Interpret + Track fokussiert" ist also **vorhanden**:
  `OpenArtist { anchor_track_id: Some(id) }`. Es gibt KEIN Genre- und KEIN
  Zeitraum-Scope in der Library (`LibraryScope::{All, Album, Artist}`).
* Player-API: `play_from_view(ids, index, origin)`, `play_next(&ids)`,
  `append_to_queue(&ids)` (Wiring-Belege ebd.).
* Bilder: nur Album-Cover über `CoverLoader` (Generation-Token-Muster). Keine
  Interpretenfotos, keine Netzquelle erlaubt (STATS-0: alles lokal).
* Motion: Tokens `MICRO(150)/STANDARD(250)/AMBIENT(400)`, `timed()` pinnt
  `follow_enable_animations_setting(true)`; Literal-Dauern außerhalb
  `motion.rs` fallen im Gate durch (`check-motion-tokens.sh`, Policy-Datei ist
  `motion.rs` selbst — neue Tokens gehören genau dorthin).
* UX-Regeln STATS-0..9 sind `[aktiv]` mit regelbenannten Tests; `RELEASING.md`
  referenziert STATS-1..4 und STATS-9 in der Manual-QA-Liste (bei Ersetzungen
  im selben Commit anpassen, sonst Gate-Fehler „ersetzte ID referenziert").

---

## 2. Entscheidungen (D1–D16, final — gegrillt am 2026-07-24)

**D1 — Band-Hero-Bild: Album-Cover + Verlaufs-Ausblendung, Fallback Initialen.**
Das Hero-Bild der Band-Karte ist das Cover von
`RankedGroup.representative_track_path` des #1-Artists (per Konstruktion der
meistgespielte Track der Gruppe — kein neues Aggregat nötig), geladen als
`ThumbnailSize::Portrait` in ein `gtk4::Picture` (`content_fit: Cover`). Die
„Alpha-Maske nach unten" wird NICHT als echte Maske gebaut, sondern als
`gtk4::Overlay`: Karte bekommt opaken Grund
(`.stats-band-card { background-color: @card_bg_color; }`), darüber das Bild,
darüber ein Gradient-Layer `linear-gradient(to top, @card_bg_color 8%,
alpha(@card_bg_color, 0) 55%)` — visuell identisch zum Mockup-Fade, rein CSS,
theme-reaktiv. Fällt das Cover aus (Platzhalter-Fall des CoverLoaders), zeigt
die Karte stattdessen eine Initialen-Kachel (erste Buchstaben des Labels,
Akzent-getönt) — Mockup-Variante 1b als Fallback. Verworfen: echte
Cairo-/`gsk::MaskNode`-Maske, Online-Artist-Bilder (STATS-0) und
`mix-blend-mode` (kann GTK-CSS nicht).

**D2 — Periodenwahl bleibt das Dropdown.** `DropDown` rechts in der neuen
Kopfzeile, kein `adw::ToggleGroup`: `StatsPeriod::available` liefert zusätzlich
historische Jahre und Last 30 days, und STATS-6c verlangt die Jahresliste.
STATS-6c und STATS-8 („Zeitraum-Dropdown ist der einzige Ansichts-Regler")
bleiben unverändert `[aktiv]`. Verworfen: Segmented/ToggleGroup mit
Overflow-Menü (zwei Mechanismen für eine Rolle).

**D3 — Wochen-Buckets für das Chart.** `granularity_for` ändert sich: der
Week-Zweig deckt künftig Spannen bis **730 Tage** ab (statt 120), Month erst
darüber (nur noch lange AllTime-Historien). Day-Zweig (≤45 Tage bzw. <8 aktive
Tage) bleibt: ein Januar-YTD mit 20 Tagen zeigt weiter Tage, STATS-6-konform
(„feinere Granularität"). Der bestehende Test
`stats_6_sparse_uses_finer_granularity` erwartet Month bei 200 Tagen und wird
**im selben Commit** auf die neuen Schwellen angepasst (Regeltext von STATS-6
bleibt wörtlich erfüllt — Wochen statt leerer Monate sind genau seine Aussage).
Buckets/Labels entstehen weiter in Core (`build_buckets`, Week existiert
schon); die Monats-Beschriftung JAN–JUL wird NICHT pro Bucket, sondern als
Monats-Ticks aus den Bucket-Starts abgeleitet (Chart-Math, T6). Verworfen:
eigene Chart-Granularität parallel zur Snapshot-Granularität.

**D4 — Beste Woche als eigenes Core-Feld.** Neu in `stats_snapshot.rs`:
`pub struct BestWeek { pub start: NaiveDate, pub total_ms: i64 }` und
`StatsSnapshot.best_week: Option<BestWeek>`, gefaltet aus `listen_rows` über
lokale Kalenderwochen (Montag-Start, `chrono::NaiveDate::week(Weekday::Mon)`,
zone-aware via `local_parts`) — unabhängig von der Chart-Granularität, damit
der KPI auch bei Day-/Month-Achse stimmt. Der Chart-Marker erscheint nur bei
`Granularity::Week` (Index der besten Woche in den Buckets); bei Day/Month
gibt es keinen Marker, der KPI bleibt.

**D5 — Trend-KPI: absolute Stunden auf der BESTEHENDEN Vergleichsbasis.** Die
saisonal deckungsgleiche Vergleichsperiode aus STATS-1 (YTD vs. gleiche Spanne
Vorjahr etc.) bleibt die Basis. Der Trend-KPI zeigt das **absolute Delta**
`total_ms − previous_ms` in Stunden („+9 h", Trend-Icon
`pan-up-symbolic`/`pan-down-symbolic`, Akzentfarbe) mit der kurzen Referenz
(„vs 2025"); der Tooltip trägt volle Semantik + Prozent/Faktor nach den
STATS-1a-Formregeln. `ComparisonPresentation::New` wird zum Kopfzeilen-Badge
„New this year" (exakt das Badge des Mockups). Core: `HeroSection` erhält
`previous_ms: Option<i64>` (das Delta rechnet die UI; Prozent/Faktor-Logik
bleibt unverändert). Verworfen: neues Vorquartals-Aggregat à la „vs. spring"
(verwirft die beschlossene Saisonalitäts-Begründung von STATS-1 nicht).

**D6 — Jahres-Hochrechnung („Pace")** nur für `YearToDate`: `HeroSection.
pace_projection_ms: Option<i64>` = `total_ms / elapsed_days * days_in_year`
(i64-Arithmetik, `elapsed_days ≥ 1` via bestehendem `elapsed_days`). Für
Year/Last30Days/AllTime `None` → KPI wird nicht gerendert (kein Platzhalter).

**D7 — Wegfall: Clock, Highlights, Customize ersatzlos — aus UI UND Core;
Top-Tracks werden zum Reveal.** Das Design definiert die Seite abschließend;
zwei konkurrierende Seitenwahrheiten gibt es nicht. Im Grill verschärft:
auch die Core-Berechnungen fallen mit (das frühere Follow-up dazu ist damit
in Scope gezogen).
* UI (T10): `hourly_chart.rs`, `hourly_chart_math.rs`, `stats_highlights.rs`,
  `stats_customize.rs` werden gelöscht. `playlists::create_smart` bleibt in
  Core (getestet, generisch); die CTA-Verdrahtung (`set_on_create_smart_mix`,
  `create_stats_smart_mix`) entfällt.
* Core (T11, NACH dem UI-Rückbau, weil die UI bis T10 konsumiert):
  `ClockSection`/`HourlyListens` UND `HighlightsSection` (`streak_days`,
  `discovered_tracks`, `busiest_day`, `on_repeat`) werden aus
  `stats_snapshot.rs` entfernt, die zugehörigen Queries aus `stats_screen.rs`,
  Begleittypen (`BusiestDay` etc.) mit — sofern der Grep keine Fremdnutzer
  zeigt (in T11 verifizieren). STATS-0-Tests, die Clock/Highlights als Vehikel
  nutzen, werden auf verbleibende Sektionen umgeschrieben — die Play-Definition
  bleibt zu jedem Zeitpunkt voll getestet.
* Customize: `settings::StatsLayout` samt Accessoren und Tests wird aus Core
  entfernt (YAGNI; verwaiste `stats.section.*`-Keys in Alt-DBs sind harmlos).
* Top-Tracks: Die sortierbare Liste (by plays/by time) bleibt als Inhalt eines
  `gtk4::Revealer` unter der Songs-Karte, geöffnet durch den Ghost-Button
  „Show all top tracks" (Design-Element!). Default eingeklappt.
* Regel-Nachfolge (append-only, Markierung im selben Commit wie
  Test-Umhängung, Choreografie in Abschnitt 5): STATS-1 → `[ersetzt durch
  STATS-11/STATS-12]`, STATS-1a → `[ersetzt durch STATS-11a]`, STATS-2 →
  `[ersetzt durch STATS-13]`, STATS-3 → `[ersetzt durch STATS-15]`, STATS-4 →
  `[ersetzt durch STATS-10]`, STATS-5 → `[ersetzt durch STATS-14]`, STATS-7 →
  `[ersetzt durch STATS-10]`. Die Highlights-/on_repeat-bezogenen Regeln waren
  Teil dieser Kaskade (STATS-4/7 → STATS-10); der Core-Rückbau ändert an der
  Kaskade nichts. STATS-0, -6, -6a, -6c, -8, -9 bleiben unverändert `[aktiv]`;
  ihre Tests dürfen zu keinem Zeitpunkt brechen. `RELEASING.md`-Bullets, die
  ersetzte IDs zitieren, werden im selben Commit auf die Nachfolger
  umgeschrieben.

**D8 — Leerzustand zweistufig.** `plays == 0`: unverändert `adw::StatusPage`
(STATS-6/6c bleiben). `1 ≤ plays < 10` (`const MIN_PLAYS_FOR_TREND: i64 = 10`):
Hero zeigt echte Zahlen, statt des Charts erscheint eine Inline-Hinweiszeile
„Keep listening — stats grow with you" (englischer UI-String,
`strings_stats.rs`); Band-/Songs-/Genre-Karten rendern nur, wenn sie Daten
haben (Spotlight `None` → Karte weg, wie heute) — Platzhalterkarten gibt es
nicht. Schwelle gilt pro gewähltem Zeitraum. Neue Regel STATS-16.

**D9 — Genre-Interaktionen: Kachel-Cover klickbar, Segmente bleiben Anzeige.**
Kein Genre-Scope in diesem Branch (ein neues `LibraryScope` zieht Änderungen
durch >17 exhaustive Matches; Begründung im Vorgängerplan D9, unverändert
gültig — Follow-up STATS-15a, Abschnitt 8). Die 4 Genre-Kacheln zeigen Cover
des meistgehörten Albums im Genre + „top: <Artist>"; **Klick auf das Cover** →
`NavigationIntent::OpenAlbum` (existiert). Segmentleiste und Genre-Namen
bleiben ohne Navigation (kein Hover-Anheben, keine Klick-Affordance ohne
Klickziel — P-2/P-3); Segment-Tooltip wird auf „Metalcore · 57 % · 6 h 50"
erweitert. Verworfen: Genre-Scope in diesem Branch, Hover-Navigation (P-3).

**D10 — Best-Week-Marker ist reine Anzeige, kein Klickziel.** Die Library hat
keinen Zeitraum-Filter; „in dieser Woche gehört" ist ohne neues Scope nicht
ehrlich baubar. Marker + Label „best week · 4 h 12" + Tooltip, kein
Controller. Zeitraum-Filter als Follow-up notiert (Abschnitt 8).

**D11 — Akzent-Abstufungen ausschließlich aus `@accent_bg_color` abgeleitet.**
Keine Nocturne-Hexwerte. GTK-CSS-Funktionen: `shade(@accent_bg_color, f)` für
die Rangfarben (Anker: 400≈1.15, 500≈1.0, 600≈0.85, 700≈0.70, 800≈0.55;
Feintuning bei der Sichtprüfung), `alpha(@window_fg_color, 0.05/0.06)` für
Balken-Hintergründe, `alpha(@accent_bg_color, …)` für Pillen/Flächen.
Songs-Balken-Verlauf: `linear-gradient(to right, shade(@accent_bg_color,0.7),
shade(@accent_bg_color,1.15))` auf dem gefüllten LevelBar-Block. Der dezente
radiale Seiten-Schimmer wird versucht
(`background-image: radial-gradient(… alpha(@accent_bg_color, 0.08),
transparent …)` auf der Stats-Page-Box); akzeptiert GTKs CSS-Parser die Syntax
nicht warnungsfrei, entfällt der Schimmer ersatzlos (nice-to-have, kein
Akzeptanzkriterium).

**D12 — Motion: fünf neue benannte Tokens in `motion.rs`, zwei Wiederverwendungen.**
Die Design-Dauern kollidieren mit dem Token-Zwang → sie werden Tokens:

```rust
// motion.rs — stats entrance choreography (design variant 2a)
pub(in crate::ui) const STATS_COUNT_MS: u32 = 600;   // hero count-up
pub(in crate::ui) const STATS_REVEAL_MS: u32 = 500;  // chart left→right reveal
pub(in crate::ui) const STATS_BAR_MS: u32 = 350;     // bar grow to target
pub(in crate::ui) const STATS_TWEEN_MS: u32 = 200;   // period-switch value tween
pub(in crate::ui) const STATS_STAGGER_MS: u32 = 70;  // per-card entrance offset
pub(in crate::ui) const STATS_COUNT: MotionToken = /* 600, EaseOutCubic */;
// … analoge MotionToken-Konstanten für REVEAL/BAR/TWEEN
```

Karten-Einblendung = bestehendes `STANDARD` (250 ms ✓), Reveal-Delay =
`MICRO_MS` (150 ms ✓). Alle Animationen laufen über `motion::timed()`
(MOT-7 via `follow_enable_animations_setting`); handgebaute Pfade
(Reveal-Fraction-Zeichnung) gaten zusätzlich über `animations_enabled()` und
springen sonst in den Endzustand. `motion_tokens_match_the_approved_values`
wird um die neuen Werte erweitert. Budget-Nachweis: max(600, 150+500,
3·70+250, …) < 900 ms. Verworfen: Mapping auf Bestandstokens (600→AMBIENT
verfälscht die abgenommene Choreografie) und Literale außerhalb `motion.rs`.

**D13 — Einstiegs-Animation: Fade + Stagger, KEIN 8-px-Translate.** Karten
faden gestaffelt (Opacity 0→1, `STANDARD`, `STATS_STAGGER_MS`-Versatz via
`glib::timeout_add_local_once`, bei `animations_enabled() == false` sofort
1.0), Hero-Zahl zählt via `adw::CallbackAnimationTarget` hoch (`STATS_COUNT`,
formatiert über `strings::hero_listening_time`), Chart zeichnet über eine
`reveal_fraction: Cell<f64>` im Ribbon (Clip auf Bruchteil der Breite,
`STATS_REVEAL` nach `MICRO_MS` Delay), Balken sind `gtk4::LevelBar`s, deren
`value` mit `STATS_BAR` animiert wird. Einmaligkeit: `StatsView` erhält
`entrance_pending: Cell<bool>`; `library_shell` setzt es beim Route auf
`MyStats` (`prepare_entrance()`), der nächste Render konsumiert es.
Periodenwechsel animiert nie die Choreografie, nur `STATS_TWEEN` auf
Zahlen/Balken. Bewusste, dokumentierte Design-Abweichung: das 8-px-Slide des
Mockups entfällt ersatzlos — GTK4 hat keine billige Per-Widget-Transform-
Animation, ein Margin-Tween wäre eine ruckelnde Layout-Animation, und ein
TranslateBin-/gsk-Transform-Wrapper wird NICHT gebaut.

**D14 — Songs-Karte: vorhandene Bausteine wiederverwenden, drei neue.**
Zeilen-Klick (Titel/Artist) → bestehender `StatsMetadataTarget`-Pfad
(`OpenArtist { anchor_track_id: Some(track_id) }` = „Library gefiltert auf
Interpret + Track fokussiert" — exakt die Design-Interaktion, null neue
Navigation). Neu: (a) Hover-Play-Overlay am Cover (Button
`media-playback-start-symbolic` in `gtk4::Overlay`, sichtbar bei Hover/Fokus)
→ neuer Callback `set_on_play_track(track_id)` → Wiring
`player.play_from_view(vec![track_id], 0, play_origin::from_artist(&artist))`;
(b) Kontextmenü (Rechtsklick/Menütaste, `gtk4::PopoverMenu` mit gio-Actions):
„Play next" → `player.play_next(&[id])`, „Add to queue" →
`player.append_to_queue(&[id])`, „Go to album" → `OpenAlbum`; (c) Balken mit
Verlauf (D11) relativ zu Platz 1. Die Karte zeigt genau 5 Zeilen
(`const SONG_ROW_LIMIT: usize = 5`); darunter Ghost-Button → Revealer (D7).

**D15 — Modul-Zuschnitt (800er-Limit, Composer bleibt Composer).**
Neu/umgebaut unter `ui/stats/` (Geschwister, `mod.rs` append-only):
* `stats_header.rs` (neu): Kopfzeile — Titel „My Stats" (19 px-Äquivalent),
  Badge-Slot „New this year", rechts Perioden-DropDown.
* `stats_hero.rs` (Umbau): große Zahl (84 px, weight 500, ls −0.03em) +
  Subline „205 plays · 76 artists" + KPI-Reihe (4 Label-über-Wert-Paare,
  Baseline-Ausrichtung, `adw::WrapBox` wie heute für schmale Fenster).
* `stats_ribbon.rs`/`stats_ribbon_math.rs` (Umbau): Wochen-Flächenchart —
  Fläche Akzent 30 %→0 vertikal, Linie 2 px, Best-Week-Markerlinie
  (`stroke-dasharray`-Äquivalent in Cairo: `set_dash`), Endpunkt-Dot für die
  laufende Woche (ersetzt den bisherigen Open-Bucket-Strich), Monats-Ticks
  statt Bucket-Labels, `reveal_fraction`. Hover-Tooltips bleiben.
* `stats_band_card.rs` (neu, ersetzt `stats_spotlight.rs`): Bild-Hero (D1),
  Kicker „MOST PLAYED BAND", Name, Meta „N plays · N h · N % of your artist
  listening", Ränge 2–5 mit 4-px-Balken relativ zu Platz 1 (Anteil je Rang
  aus `RankedGroup.group.ms` / Rang-1-ms — reine UI-Rechnung), Klick auf
  Karte/Namen → `OpenArtist`; Unify-Hint (STATS-9/D21-Vertrag) bleibt an der
  Meta-Zeile bzw. den Rängen.
* `stats_songs_card.rs` (neu): D14, inkl. Revealer mit der bestehenden
  sortierbaren Top-Tracks-Liste (Code zieht aus `stats_view.rs` um:
  `render_tracks`, `build_sort_controls`, `metric`).
* `stats_genre_card.rs` (Umbau aus `stats_genre_bar.rs`, Datei umbenannt):
  gestapelter 22-px-Balken (2-px-Lücken, Rangfarben D11, letztes Segment
  neutral `alpha(@window_fg_color, 0.25)`) + 4-Spalten-Grid der Kacheln
  (Cover 40 px klickbar, „Metalcore · 57 %", „6 h 50 · top: Lorna Shore").
  Unify-Hint bleibt.
* `stats_entrance.rs` (neu): Choreografie D13 an einem Ort.
* Gelöscht: `stats_spotlight.rs`, `stats_highlights.rs`, `hourly_chart.rs`,
  `hourly_chart_math.rs`, `stats_customize.rs`.
* `stats_view.rs` bleibt Composer (< 800; schrumpft durch Auszug von
  Top-Tracks-Rendering und Wegfall Clock/Highlights/Customize).

**D16 — Fehlende Tracks in der Songs-Karte: der FB-6-Toast-Pfad genügt.**
`TopTrack` bekommt KEIN `missing: bool`, und Zeile, Hover-Play und Kontextmenü
bekommen KEINEN Sonderpfad für fehlende Dateien: Hover-Play auf einen
fehlenden Track läuft in den bestehenden FB-6-Skip-Pfad (Toast) — das ist die
beschlossene Behandlung. Verworfen: PLAY-4b-Behandlung (Einreihen disabled)
samt Missing-Join, weil der Sonderzustand den Mehraufwand hier nicht trägt.

---

## 3. Core-Datenmodell-Änderungen (Zusammenfassung)

`compute` bleibt pure Funktion von `(conn, period, now_unix, tz)`. Additiv in
T2–T4, Rückbau in T11:

* `HeroSection` += `previous_ms: Option<i64>` (D5),
  `pace_projection_ms: Option<i64>` (D6).
* `StatsSnapshot` += `best_week: Option<BestWeek>` (D4).
* `GenreSegment` += `top_artist: Option<String>`,
  `representative_track_path: String` (leer für „Other"; Kachel ohne Cover
  rendert Platzhalter-Icon wie überall).
* Neue Query in `stats_screen.rs`: `genre_artist_rows(conn, start, end)` →
  Zeilen `(genre_raw, artist_raw, mbid, plays, ms, last_played_at, path)`,
  pro Genre-Schlüssel per `fold_groups` zum Top-Artist gefaltet (eine
  Schlüsselauflösung, STATS-9; Artist-Fold bekommt die MBID, Genre-Fold nie).
* Rückbau (D7, T11): `ClockSection`/`HourlyListens`, `HighlightsSection`
  (`streak_days`, `discovered_tracks`, `busiest_day`, `on_repeat`) und
  Begleittypen (`BusiestDay` etc.) verschwinden aus `stats_snapshot.rs`, die
  zugehörigen Queries aus `stats_screen.rs` — Fremdnutzer vorher per grep
  ausschließen.
* Statement-Zahl pro `compute` ändert sich zweimal (T4: +Genre-Query, T11:
  −Clock-/Highlights-Queries). Implementierungsdetail: die exakte Zahl wird
  beim Umsetzen im jeweiligen Task neu ausgezählt und im Funktionskommentar
  dokumentiert (Ausgangslage heute: 8).
* `granularity_for`: Week-Schwelle 120 → 730 Tage (D3);
  `apply_activity_granularity` unverändert in der Mechanik.
* Wochen-Fold-Helper in `stats_period.rs`:
  `pub fn week_start<Tz: TimeZone>(tz: &Tz, unix: i64) -> Option<NaiveDate>`
  (Montag, via `local_parts`), genutzt von `best_week` und den Tests.
* Kein Schema-Change, keine Migration, kein Cache (D4 des Vorgängerplans gilt).

---

## 4. Neue UX-Regeln (wörtlich nach `docs/ux-rules.md`, Abschnitt V anhängen)

Alle mit Status `[geplant]` landen in T1; der Task, der das Verhalten fertig
implementiert, flippt auf `[aktiv]` und markiert die ersetzte Altregel im
selben Commit (Mapping in D7, Choreografie in Abschnitt 5). Test-Namensschema
`stats_1x_…` (das Traceability-Gate parst `fn stats_[0-9]+[a-z]?_`).

```markdown
- **STATS-10** [geplant] [gtk] — My Stats erzählt in fester Reihenfolge von
  oben nach unten: Kopfzeile (Titel, optionales „New this year"-Badge,
  Zeitraumwahl) · Hero (Gesamtzahl, Subline, KPI-Reihe) · Wochen-Chart ·
  zweispaltige Reihe aus Band-Karte und Songs-Karte · Genre-Karte. Mehr
  Sektionen gibt es nicht: keine Listening Clock, keine Highlight-Kacheln,
  kein Customize-Menü — die Seite ist kuratiert und nicht konfigurierbar.
  Im schmalen Fenster stapelt die zweispaltige Reihe, ohne die Reihenfolge zu
  ändern. Die Zeitraumwahl bleibt gemäß STATS-8 der einzige Ansichts-Regler.
- **STATS-11** [geplant] [core] — Der Hero zeigt die Gesamt-Hörzeit riesig
  (volle Stunden, unter einer Stunde Minuten, nie „0 hours"), darunter die
  Subline „N plays · N artists", rechts an der Grundlinie vier KPI-Paare:
  „Per day" (Ø min/Tag) · Trend (absolutes Stunden-Delta zur Vergleichsspanne
  mit Richtungs-Icon in Akzentfarbe) · „Pace for <Jahr>" (lineare
  Jahres-Hochrechnung, nur im laufenden Jahr) · „Best week" (Startdatum und
  Hörzeit der stärksten lokalen Kalenderwoche). Die Vergleichsspanne ist
  unverändert die saisonal deckungsgleiche Vorperiode: „<Jahr> so far" gegen
  dieselbe Spanne des Vorjahrs, ein volles Jahr gegen das Vorjahr, das
  30-Tage-Fenster gegen die 30 Tage davor; „All time" hat keinen Trend-KPI.
  KPIs ohne Wert entfallen ersatzlos statt Platzhalter zu zeigen.
- **STATS-11a** [geplant] [core] — Der Trend bleibt bei jedem Verhältnis
  ehrlich lesbar: Der KPI nennt das absolute Delta und die kurze Referenz
  („vs 2025"); der Tooltip trägt die vollständige Semantik samt Prozentwert,
  ab ×10-Verhältnissen als gerundeter Faktor nach den bisherigen Formregeln.
  War die Vergleichszeit effektiv null (unter einer Minute), erscheint statt
  des KPI das Badge „New this year" in der Kopfzeile — nie „∞ %" und nie
  „×0". Der KPI ellipsiert nicht.
- **STATS-12** [geplant] [core] — Das Chart zeigt die Hörzeit je lokaler
  Kalenderwoche als Flächenverlauf: Achse exakt der gewählte Zeitraum,
  Monatslabels darunter, Linie und Fläche in Abstufungen der Akzentfarbe.
  Die beste Woche trägt eine gestrichelte Markerlinie mit Label
  („best week · 4 h 12"); die laufende Woche endet in einem offenen Punkt.
  Hover nennt Woche und exakten Wert. Marker und Punkte sind reine Anzeige.
  Nur wenn der Zeitraum zu kurz für Wochen ist, fällt die Achse auf Tage
  zurück (STATS-6); sehr lange „All time"-Spannen dürfen Monate zeigen und
  lassen dann den Wochen-Marker weg — der Best-week-KPI bleibt.
- **STATS-13** [geplant] [gtk] — Die Band-Karte zeigt den meistgehörten
  Interpreten als Bild-Hero: das Album-Cover seines meistgespielten Tracks
  füllt die Karte und blendet nach unten in den Kartengrund aus; fehlt ein
  Cover, steht eine Initialen-Kachel an seiner Stelle — nie eine leere
  Fläche. Darüber Kicker „MOST PLAYED BAND", Name und die Zeile „N plays ·
  N h · N % of your artist listening". Darunter die Ränge 2–5 mit dünnem
  Balken relativ zu Platz 1. Klick auf Karte oder Rangzeile öffnet die
  Library gefiltert auf den Interpreten (regulärer History-Push). Fasst eine
  Gruppe mehrere Schreibweisen zusammen, bleibt der Vereinheitlichungs-Hinweis
  aus STATS-9 erhalten.
- **STATS-14** [geplant] [gtk] — Die Songs-Karte zeigt die fünf meistgespielten
  Tracks: Cover, Titel und Interpret zweizeilig, horizontaler Balken relativ
  zu Platz 1 in einem Akzent-Verlauf, rechts die Play-Zahl. Klick auf die
  Zeile öffnet die Library gefiltert auf den Interpreten mit fokussiertem
  Track; Hover oder Fokus zeigt am Cover einen Play-Button, der genau diesen
  Track sofort abspielt; das Kontextmenü bietet „Play next", „Add to queue"
  und „Go to album". Der Ghost-Button „Show all top tracks" klappt darunter
  die vollständige nummerierte Liste mit dem Sort-Toggle „by plays / by time"
  auf; deren Balken bleibt relativ zum Spitzenreiter der Liste.
- **STATS-15** [geplant] [core] — Die Genre-Karte besteht aus einem
  gestapelten Balken (Segmentbreite = Anteil, Akzent-Abstufungen nach Rang,
  letztes Segment neutral, Tooltip „<Genre> · N % · N h") und bis zu vier
  Kacheln der stärksten Genres: Cover des meistgehörten Albums im Genre,
  „<Genre> · N %", darunter „N h · top: <Interpret>". Top-Interpret und
  Cover je Genre entstehen über dieselbe Schlüsselauflösung wie alle
  Gruppierungen (STATS-9). Klick auf das Kachel-Cover öffnet die Library
  gefiltert auf das Album; Segmente und Genre-Namen sind keine Navigation.
  Tracks ohne Genre zählen weiterhin weder als Segment noch als „Other".
- **STATS-16** [geplant] [gtk] — Unter zehn Plays im gewählten Zeitraum ist
  die Datenlage zu dünn für einen Trend: Statt des Charts erscheint der
  Hinweis „Keep listening — stats grow with you"; Hero-Zahlen bleiben echt,
  und nur Karten mit Daten werden gerendert — nie Platzhalterkarten. Ohne
  jeden Play gilt unverändert der Leerzustand aus STATS-6/STATS-6c samt
  bedienbarer Zeitraumwahl.
- **STATS-17** [geplant] [gtk] — Die Seite animiert genau einmal pro Öffnen:
  Hero-Zahl zählt hoch, das Chart zeichnet sich von links nach rechts, Karten
  faden gestaffelt ein, Balken wachsen auf ihren Zielwert; alle Dauern sind
  benannte Motion-Tokens, das Gesamtbudget bleibt unter einer Sekunde
  (Design-Intent, manuell geprüft). Ein Zeitraumwechsel wiederholt die
  Choreografie nie, sondern interpoliert nur Zahlen und Balken kurz auf die
  neuen Werte. Bei `gtk-enable-animations=false` steht alles sofort im
  Endzustand.
```

---

## 5. Regel-Ersetzungs-Choreografie (am Gate-Skript verifiziert)

Fakten aus `scripts/check-ux-traceability.sh` (am Skript verifiziert,
verbindlich für alle Tasks):

1. Jede `[aktiv]`-Regel braucht ≥ 1 regelbenannten Test.
2. Tests DÜRFEN auf `[geplant]`-Regeln zeigen (kein Fehler).
3. Eine `[ersetzt …]`-Regel darf KEINEN Test mehr tragen und nicht mehr in
   `RELEASING.md` referenziert sein.
4. `#[ignore = "requires a display; run via xvfb-run"]` zählt als Abdeckung;
   jedes andere `#[ignore]` ist nur auf `[geplant]`-Regeln erlaubt, im Format
   `UX <ID> [geplant] — …`.

**Invariante für jeden ersetzenden Task (T5, T7, T8, T9, T10):** Vor dem
Umhängen die Tests der Altregel greppen (`grep -rn 'fn stats_<n>_' crates/`
bzw. `fn stats_<n>a_`). Nach dem Commit trägt die ersetzte Regel keinen Test
und keine `RELEASING.md`-Referenz mehr; jede neu `[aktiv]`e Regel trägt ≥ 1
grünen Test.

**T5 pensioniert STATS-1 UND STATS-1a vollständig in EINEM Commit.**
Umbenennungsregel: `stats_1_*` → `stats_11_*`, `stats_1a_*` → `stats_11a_*`;
einzige Ausnahme: `stats_1_ribbon_axis_matches_period` →
`stats_12_axis_matches_period` — zulässig, weil STATS-12 ab T1 `[geplant]`
existiert und Fakt 2 Tests auf geplante Regeln erlaubt. Checkliste aller
umzuhängenden Tests (Bestandsaufnahme dev 2026-07-24, vor dem Commit per grep
gegen den aktuellen Stand verifizieren):

- [ ] `stats_snapshot_tests.rs`:
      `stats_1_comparison_uses_the_same_span_of_the_previous_year`,
      `stats_1_all_time_reports_no_comparison`
- [ ] `stats_period_tests.rs`: `stats_1_ribbon_axis_matches_period`
      (→ `stats_12_axis_matches_period`),
      `stats_1_year_to_date_compares_the_same_span_of_the_previous_year`,
      `stats_1_full_year_compares_against_the_whole_previous_year`,
      `stats_1_last_30_days_compares_against_the_30_days_before`,
      `stats_1_all_time_has_no_compared_span`,
      `stats_1_leap_day_clamps_the_compared_span_to_february`
- [ ] `stats_comparison_tests.rs`: alle fünf `stats_1a_*`-Tests → `stats_11a_*`
- [ ] `strings_stats.rs`:
      `stats_1a_comparison_copy_renders_every_presentation_without_decimal_noise`
- [ ] `stats_view_tests.rs`:
      `stats_1_pill_names_the_seasonally_congruent_compared_span`,
      `stats_1_realistic_width_keeps_the_hero_copy_unellipsized`,
      `stats_1a_comparison_pill_is_not_ellipsized_at_a_realistic_width`

Die Core-Tests behalten ihre Semantik unverändert (reine Umbenennung); die
View-Tests werden im selben Commit inhaltlich an die neue Hero-Darstellung
angepasst (KPI + Tooltip statt Pill — T5 baut den Hero ohnehin um).
`RELEASING.md`-Bullets zu STATS-1/1a ziehen im selben Commit auf die
Nachfolger um.

**T6 flippt STATS-12 nur noch auf `[aktiv]`** und ergänzt
Marker-/Ticks-/Reveal-Tests. Kein STATS-1-Rest in T6 —
`stats_12_axis_matches_period` existiert dann bereits und deckt die Achse.

**Analog in T7–T10** (jeweils mit Grep-Bestandsaufnahme vor dem Umhängen):
STATS-2 → STATS-13 (T7), STATS-5 → STATS-14 (T8), STATS-3 → STATS-15 (T9),
STATS-4 und STATS-7 → STATS-10 (T10).

---

## 6. Tasks (TDD-Reihenfolge; `mod.rs`-Registries append-only)

Sequenz: **T1 → (T2 ‖ T3 ‖ T4) → T5 → T6 → (T7 ‖ T8 ‖ T9) → T10 → T11 →
T12 → T13.** „‖" markiert Unabhängigkeit; ein einzelner Agent arbeitet die
Gruppen sequenziell ab (T2–T4 berühren `stats_snapshot.rs` in disjunkten
Abschnitten). Jeder Task: Tests zuerst rot, dann Implementierung, dann volle
Gates (`fmt`, `clippy -D warnings`, `test --workspace`, `cargo audit`,
Core-Purity-Grep, `check-ux-traceability.sh`, `check-display-tests.sh`,
`check-motion-tokens.sh`). Branch: `feat/mystats-redesign` von `dev`, PR-Base
`dev`, nichts pushen ohne Auftrag.

### T1 — Regelwerk
Dateien: `docs/ux-rules.md` (nur Anhängen der Regeln aus Abschnitt 4).
Akzeptanz: `check-ux-traceability.sh` grün (neue Regeln `[geplant]` brauchen
keine Tests); keine bestehende Regel angefasst. Regeln: legt STATS-10..17 an.

### T2 — Core: Wochen-Granularität + beste Woche
Dateien: `library/stats_period.rs` (+`stats_period_tests.rs`),
`library/stats_snapshot.rs` (+`stats_snapshot_tests.rs`, nur die
Granularitäts-/BestWeek-Anteile).
Tests zuerst: `stats_12_year_axis_uses_week_buckets` (YTD Jul 2026 → ~29
Week-Buckets, letzter offen; Year(2025) → 52/53, keiner offen),
`stats_12_best_week_is_zone_aware` (Events um lokale Wochengrenze mit
`FixedOffset`-Zone; BestWeek-Start = lokaler Montag),
`week_start_folds_days_onto_monday`, Anpassung
`stats_6_sparse_uses_finer_granularity` (200-Tage-Fall erwartet Week; Month
erst > 730). Hinweis: `stats_1_ribbon_axis_matches_period` wird hier bei
Bedarf INHALTLICH an die neuen Schwellen angepasst, aber erst in T5 UMBENANNT
(Abschnitt 5). Akzeptanz: `best_week` unabhängig von Chart-Granularität
befüllt; leere Periode → `None`; alle bestehenden STATS-0/-6-Tests grün.
Regeln: liefert Core-Anteil von STATS-12 (Flip erst in T6).

### T3 — Core: Hero-Erweiterung
Dateien: `library/stats_snapshot.rs` (+Tests, Hero-Anteile),
`library/stats_comparison_tests.rs` (falls betroffen).
Tests zuerst: `stats_11_pace_projects_only_year_to_date` (YTD: total/elapsed·
Jahrestage; Year/AllTime/Last30 → `None`),
`stats_11_previous_ms_carries_the_seasonal_baseline` (previous_ms == Summe der
Vergleichsspanne; AllTime → `None`), Bestandstests für
`ComparisonPresentation` unverändert grün. Akzeptanz: `compute` weiterhin
pure/repeatable (Bestandstest), Statement-Zählung im Doc-Kommentar aktuell.
Regeln: Core-Anteil STATS-11/11a (Flip in T5).

### T4 — Core: Genre-Kacheln-Daten
Dateien: `library/stats_screen.rs` (+`stats_screen_tests.rs`),
`library/stats_snapshot.rs` (GenreSegment-Erweiterung, Tests).
Tests zuerst: `stats_15_genre_top_artist_uses_group_key` (Genres
„Deathcore"/„deathcore" mergen; Top-Artist je Genre über den Fold, Cover-Pfad
= meistgespielter Track im Genre; „Other" ohne Artist/Pfad),
`genre_artist_rows_exclude_blank_genres`. Akzeptanz: eine Schlüsselauflösung
(kein `lower(trim())` in SQL), STATS-9-Tests grün; Statement-Zählung im
Doc-Kommentar von `compute` aktualisiert (Abschnitt 3). Regeln: Core-Anteil
STATS-15 (Flip in T9).

### T5 — GTK: Kopfzeile + Hero-KPIs + Pensionierung STATS-1/STATS-1a
Dateien: `ui/stats/stats_header.rs` (neu), `ui/stats/stats_hero.rs` (Umbau),
`ui/stats/stats_css.rs`, `ui/strings_stats.rs`, `ui/stats/stats_view.rs`
(nur Hero-/Header-Einbindung), `library/stats_snapshot_tests.rs`,
`library/stats_period_tests.rs`, `library/stats_comparison_tests.rs`,
`ui/stats/stats_view_tests.rs` (Umbenennungen gemäß Abschnitt 5),
`docs/ux-rules.md` (STATS-11/STATS-11a → `[aktiv]`; STATS-1 → `[ersetzt durch
STATS-11/STATS-12]`; STATS-1a → `[ersetzt durch STATS-11a]`), `RELEASING.md`
(Bullets STATS-1/1a → Nachfolger).
Tests zuerst (Display, xvfb, Muster `view_and_conn()`):
`stats_11_hero_renders_kpi_pairs_without_placeholders` (YTD: 4 KPIs; AllTime:
weder Trend noch Pace-KPI-Widget vorhanden),
`stats_11a_zero_baseline_shows_new_badge_not_a_delta`; dazu im selben Commit
die VOLLSTÄNDIGE Umbenennungs-Checkliste aus Abschnitt 5 (17 Tests, inkl.
`stats_1_ribbon_axis_matches_period` → `stats_12_axis_matches_period`).
Akzeptanz: Zahl-Label mit Klasse `.stats-hero-number` (Größe/Gewicht/
Letterspacing per CSS), KPI-Labels uppercase-Kicker-Stil, Subline vorhanden;
kein fester Hexwert im CSS; nach dem Commit tragen STATS-1/1a keinen Test und
keine `RELEASING.md`-Referenz mehr (Invariante Abschnitt 5),
`check-ux-traceability.sh` grün. Regeln: STATS-11, STATS-11a.

### T6 — GTK: Wochen-Chart
Dateien: `ui/stats/stats_ribbon.rs`, `ui/stats/stats_ribbon_math.rs`,
`docs/ux-rules.md` (STATS-12 → `[aktiv]`).
Tests zuerst: Math-Tests ohne Display —
`stats_12_marker_sits_on_the_best_week_bucket`,
`month_ticks_derive_from_bucket_starts`,
`reveal_fraction_clips_the_area_path`. Kein STATS-1-Rest: der Achsen-Test
heißt seit T5 `stats_12_axis_matches_period` und deckt STATS-12 mit ab.
Akzeptanz: Fläche = vertikaler Akzent-Verlauf 0.3→0, Linie 2 px, Marker
gestrichelt (`cairo set_dash`), Endpunkt-Dot für offene Woche, Monatslabels;
Hover-Tooltip nennt „Week of <Datum> · <Dauer>". Regeln: STATS-12.

### T7 — GTK: Band-Karte
Dateien: `ui/stats/stats_band_card.rs` (neu), Löschung
`ui/stats/stats_spotlight.rs`, `ui/stats/stats_css.rs` (Band-Klassen),
`docs/ux-rules.md` (STATS-13 → `[aktiv]`, STATS-2 → `[ersetzt durch
STATS-13]`), `RELEASING.md`.
Vorab: `grep -rn 'fn stats_2_' crates/` — alle Treffer im selben Commit
umhängen/ersetzen (Invariante Abschnitt 5).
Tests zuerst (Display): `stats_13_band_card_shows_ranks_relative_to_leader`
(Balken-Fraktionen = ms/rang1_ms), `stats_13_missing_cover_falls_back_to_
initials`, `unify_hint_survives_on_the_band_card` (STATS-9-Vertrag),
Klick-Callback-Test (Karte → `on_open_artist(label)`). Akzeptanz: Kicker/
Name/Meta-Zeile gemäß Design; Bild über CoverLoader mit Generation-Token;
Gradient-Overlay auf opakem `@card_bg_color`; Play-Button entfällt (D7/D14 —
Abspielen wohnt an Songs und in der Artist-Ansicht). Regeln: STATS-13.

### T8 — GTK: Songs-Karte
Dateien: `ui/stats/stats_songs_card.rs` (neu), `ui/stats/stats_view.rs`
(Auszug `render_tracks`/`build_sort_controls` hierher),
`ui/stats/stats_css.rs`, `docs/ux-rules.md` (STATS-14 → `[aktiv]`, STATS-5 →
`[ersetzt durch STATS-14]`), `RELEASING.md`.
Vorab: `grep -rn 'fn stats_5_' crates/` — alle Treffer im selben Commit
umhängen/ersetzen (Invariante Abschnitt 5).
Tests zuerst (Display): `stats_14_song_row_focuses_track_in_artist_scope`
(Callback liefert `OpenArtist { anchor_track_id: Some(id) }`-Payload),
`stats_14_hover_play_targets_exactly_one_track`,
`stats_14_show_all_reveals_the_sortable_list` (Revealer default zu; Toggle
by plays/by time sortiert um — Umhängen des bisherigen `stats_5_*`-Tests).
Kontextmenü-Aktionen als Callback-Tests (`play_next`, `append_to_queue`,
`open_album`). Kein Missing-Sonderpfad (D16). Akzeptanz: exakt 5 Zeilen,
Balken-Verlauf relativ Platz 1, Play-Zahl rechtsbündig; Fokus-Ring/Hover-Tint
auf allen Klickzielen. Regeln: STATS-14.

### T9 — GTK: Genre-Karte
Dateien: `ui/stats/stats_genre_card.rs` (umbenannt aus `stats_genre_bar.rs`),
`ui/stats/stats_css.rs`, `docs/ux-rules.md` (STATS-15 → `[aktiv]`, STATS-3 →
`[ersetzt durch STATS-15]`), `RELEASING.md`.
Vorab: `grep -rn 'fn stats_3_' crates/` — alle Treffer im selben Commit
umhängen/ersetzen (Invariante Abschnitt 5).
Tests zuerst (Display): `stats_15_tiles_show_cover_share_and_top_artist`,
`stats_15_segment_carries_no_click_controller` (Nachfolger des
`genre_bar_has_no_click_controller`-Garantietests), Cover-Klick-Callback-Test
(→ `OpenAlbum`). Akzeptanz: Balken 22 px, 2-px-Lücken, Rangfarben aus
`shade()`, letztes Segment neutral; Tooltip „<Genre> · N % · N h"; Unify-Hint
bleibt. Regeln: STATS-15.

### T10 — Komposition, UI-Wegfall, Dünnstand, Wiring
Dateien: `ui/stats/stats_view.rs` (Rebuild als Composer), `ui/stats/mod.rs`,
`ui/stats/stats_view_tests.rs`, Löschungen (`stats_highlights.rs`,
`hourly_chart.rs`, `hourly_chart_math.rs`, `stats_customize.rs`),
`library/settings.rs` (+Tests; `StatsLayout` entfernen),
`ui/window/window_action_wiring.rs`, `ui/window/window.rs`,
`ui/window/library_shell.rs` (Stats-Zeilen), `ui/strings_stats.rs`,
`docs/ux-rules.md` (STATS-10, STATS-16 → `[aktiv]`; STATS-4 und STATS-7 →
`[ersetzt durch STATS-10]`), `RELEASING.md`.
Vorab: `grep -rn 'fn stats_4_' crates/` und `grep -rn 'fn stats_7_' crates/`
— alle Treffer im selben Commit umhängen/ersetzen (Invariante Abschnitt 5).
UI-seitige STATS-0-Tests, die Clock/Highlights als Vehikel nutzen, werden
hier auf verbleibende Sektionen umgeschrieben (Core-seitige in T11).
Tests zuerst: `stats_10_page_orders_header_hero_chart_row_genres` (liest die
Sektionsreihenfolge vom echten Widget-Baum, Nachfolger von `SECTION_ORDER`),
`stats_10_no_clock_highlights_or_customize_widgets`,
`stats_16_thin_history_swaps_chart_for_hint` (9 Plays → Hint sichtbar, Chart
nicht; 10 Plays → Chart), Bestand `stats_6c_*`, `stats_8_*`, STATS-6a-Pfad
(„failed"-Page) unverändert grün; Wiring-Tests für Band-/Songs-/Genre-
Callbacks nach dem Muster der bestehenden (`spotlight_play_uses_the_group_
track_ids` entfällt mit dem Play-Button — im selben Commit entfernen).
Akzeptanz: zweispaltige Reihe 5:7 über `adw::WrapBox` (Breitenverhältnis via
`width_request`, stapelt schmal); `stats_view.rs` < 800 Zeilen; keine toten
`set_on_create_smart_mix`/StatsLayout-Reste; `page_stack` um „sections"-
internen Hint erweitert (Hint ist Teil der sections-Seite, kein vierter
Stack-Zustand — Hero bleibt bedienbar). Regeln: STATS-10, STATS-16.

### T11 — Core-Rückbau: Clock + Highlights (D7)
Dateien: `library/stats_snapshot.rs`, `library/stats_screen.rs`,
`library/stats_snapshot_tests.rs` (+`stats_screen_tests.rs`, falls betroffen).
Vorab: `grep -rn 'ClockSection\|HourlyListens\|HighlightsSection\|BusiestDay'
crates/` — nach T10 dürfen nur noch Core-Definitionen und Core-Tests treffen;
jeder andere Treffer ist ein Fremdnutzer und stoppt die Entfernung des
betroffenen Typs.
Tests zuerst: STATS-0-Tests, die Clock/Highlights als Berechnungs-Vehikel
nutzen, auf verbleibende Sektionen (Hero/Ranks/Genres/BestWeek) umschreiben —
die Play-Definition („nur `listen_events`") bleibt in jedem Zwischenstand
voll getestet; danach Sektionen, Queries und Begleittypen entfernen.
Akzeptanz: keine Clock-/Highlights-Symbole mehr in Core; `compute` weiterhin
pure/repeatable; Statement-Zählung im Doc-Kommentar neu ausgezählt
(Abschnitt 3); alle STATS-0/-6/-9-Tests grün; `check-ux-traceability.sh` grün
(STATS-4/7 sind seit T10 ersetzt und tragen keine Tests mehr — dieser Task
flippt keine Regel). Regeln: keine — reiner Rückbau unter bestehender
Abdeckung.

### T12 — Motion
Dateien: `ui/motion.rs` (Tokens D12 + Test-Erweiterung),
`ui/stats/stats_entrance.rs` (neu), `ui/stats/stats_view.rs` (Konsum:
`entrance_pending`, `prepare_entrance()`), `ui/window/library_shell.rs`
(Route-Hook), `docs/ux-rules.md` (STATS-17 → `[aktiv]`).
Tests zuerst: Token-Werte-Test (600/500/350/200/70) in `motion.rs`;
`stats_17_entrance_runs_once_per_open` (Display: zweimal `refresh` nach einem
`prepare_entrance` → Choreografie-Flag nur einmal konsumiert),
`stats_17_period_switch_only_tweens_values` (kein Entrance-Restart),
`stats_17_reduced_motion_lands_in_end_state` (mit
`gtk-enable-animations=false`: `reveal_fraction == 1.0`, Opacity 1.0,
Balkenwert final). Akzeptanz: `check-motion-tokens.sh` grün (keine Literale
außerhalb `motion.rs`); alle Animationen über `motion::timed()` bzw.
`animations_enabled()`-gegatete Pfade; Fade-only ohne Translate (D13).
Regeln: STATS-17.

### T13 — Close-out
Dateien: `.superpowers/sdd/progress.md`.
Voller Gate-Sweep auf dem Branch-Endstand inkl.
`scripts/check-display-tests.sh --rule-named`; Ledger-Eintrag inkl. der je
Task festgehaltenen Test-Totals (Abschnitt 9). Keine Code-Änderungen.

---

## 7. Migration / Schema

Keine. Kein neuer Index, keine Spalte, kein Backfill. Core-API-Änderungen:
additive Felder (Abschnitt 3) plus die Entfernung von `StatsLayout` (T10) und
der Clock-/Highlights-Sektionen (T11) — beides bricht keine Fremdnutzer
(Draft-Verifikation: einzige Aufrufer in `ui/stats/` und
`stats_view_tests.rs`; in T11 per grep erneut absichern).

---

## 8. Bewusste Follow-ups (nicht dieser Branch)

1. **Genre-Scope in der Library** (`LibraryScope::Genre` o. ä.) + Klick auf
   Genre-Segment/-Name. Eigener Branch (Kollisionfläche >17 Dateien),
   eigene Regel (STATS-15a).
2. **Zeitraum-Filter „in dieser Woche gehört"** für den Best-Week-Marker —
   braucht ein Listen-Event-basiertes Library-Scope, konzeptionell neu.
3. Übernahme der Woche-730-Schwelle in eine adaptive AllTime-Achse (Zoom).

---

## 9. Risiken & Implementierungshinweise

* **CSS-Sichtprüfung nötig:** `shade()`-Abstufungen und der
  `radial-gradient`-Schimmer sind theme-abhängig in der Wirkung; die
  konkreten Faktoren brauchen eine Sichtprüfung auf echtem Display (headless
  beweist nur Parser-Akzeptanz, TESTING.md). Eingeplant als manuelle QA,
  nicht als Test; der Schimmer ist kein Akzeptanzkriterium (D11).
* **84-px-Zahl vs. schmale Fenster:** WrapBox-Umbrüche mit einer so großen
  Type sind ungeprüft; ggf. Clamp-Stufe für die Font-Größe per Breakpoint
  nötig (der Regeltext von STATS-11 lässt das absichtlich offen).
* **Wochen-Buckets bei Year/AllTime verlängern die Cairo-Pfade** (52+
  Segmente statt 12) — unkritisch erwartet, aber der Hover-Bucket-Match
  (`bucket_at_x`) wird mit 53 Buckets getestet (T6).
* **Baseline-Testzahl sinkt:** `stats_view_tests.rs` verliert viele
  Bestandstests (Clock/Customize/Spotlight), Core-Tests werden umgehängt oder
  fallen mit ihren Sektionen (T10/T11). Erwartete neue Test-Totals je Task
  beim Implementieren im Ledger festhalten (Close-out T13 sammelt sie ein).
