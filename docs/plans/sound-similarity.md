---
slug: sound-similarity
worktree: /home/marvin/Projects/reprise-sound-similarity
branch: feature/sound-similarity
phase: coded
codex_session:
created: 2026-08-05
---
# Sound Similarity

Erweiterung „Sound Similarity": ein Merkmalsmodell, abgeleitet aus dem bereits
gespeicherten Spektrogramm, ein Reiter **Sound** in der rechten Spalte, und ein
`provides`-Feld im Plugin-Register, das sagt, wo sich eine Erweiterung einhängt.

**Basis: `origin/dev` (`527d5cbbbc`).** Der Haupt-Checkout liegt weit zurück;
alles unten ist gegen `origin/dev` inventarisiert, nicht gegen ihn.

Design-Quelle: Projekt `89a8e3a7-2b40-407f-990c-258502b0b47d`,
`Reprise Mobile.dc.html`, Frames **18c** (Reiter), **19a** (Plugins-Dialog mit
Marken), **19b** (Reiterleiste bei vier).

---

## 0. Ausgangslage (gemessen)

| Baustein | Zustand auf `origin/dev` |
|---|---|
| Spektrogramm | `track_spectrograms`, Migration **v55** (`db_spectrogram.rs`): 24 log. Bänder 20 Hz–16 kHz, 20 fps, 1 `u8`/Zelle, **absolute dBFS ohne AGC** |
| Erzeugung | `spectrogram.rs::SpectrogramAccumulator`, zwei FFT-Größen (4096, darunter 16384 unter 100 Hz), gemeinsamer Dekodier-Pass mit `waveform.rs` bei 32 kHz |
| Invalidierung | SQL-Trigger `invalidate_track_render_data` auf `tracks(file_mtime, file_size, device, inode)` |
| Backfill | `spectrogram_backfill.rs::run_render_data_backfill`, resumierbar |
| Reiterleiste | `now_playing.rs:172–218`: `adw::ViewStack` + `adw::InlineViewSwitcher`, `PanelTab::{UpNext, Lyrics, Visual}` (`panel_state.rs`) |
| Reiter-Gating | `set_song_visuals_enabled` → `visual_page.set_visible(enabled)` + Rückfall auf `UP_NEXT_PAGE` (`now_playing.rs:534–542`) |
| Plugin-Register | `reprise-core/src/modules.rs`, `ModuleDescriptor` als `const` |
| Plugins-Dialog | `preference_plugins.rs` (615 Z.), hat bereits `ExpanderRow` (`plugin_uses_expander`, `settings_plugin_row`) |
| Kontextmenü | `track_list/track_menu.rs`, `track_list_context_menu.rs` |

**Datenbestand der laufenden Installation:** `~/.local/share/reprise/reprise.db`
steht auf `user_version = 54`. 1843 Tracks, 1645 Waveforms, **0 Spektrogramme** —
v55 ist erst heute auf `origin/dev` gelandet, die Tabelle existiert lokal noch
nicht. Der Backfill ist Voraussetzung für die Abnahme, nicht Teil dieses Plans.

---

## 1. Festgelegte Entscheidungen

Diese Punkte sind entschieden. Sie sind keine Vorschläge — sie ersetzen die
Stellen, an denen der Brief mit dem realen Code kollidiert.

### E1 — Eigene Tabelle, das Spektrogramm bleibt unberührt

`docs/superpowers/plans/2026-08-05-spectrogram-s1.md` sagt: *„It is a rendering
dataset … this is not a way back in. No tempo, no brightness, no similarity
scalars, no per-track summary numbers."*

Das bleibt für das Spektrogramm gültig: **kein Feld dazu, kein Skalar hinein.**
Der Merkmalsvektor lebt in einer eigenen Tabelle `track_sound_features` und ist
jederzeit aus dem Spektrogramm neu ableitbar — er ist ein Cache, keine Quelle.

Im selben Commit wird der Absatz im S1-Plan präzisiert: das Rendering-Dataset
bleibt rein, **Ableitungen daraus stehen woanders und sind erlaubt.** Ohne diese
Korrektur widersprächen sich zwei verbindliche Dokumente im Repo.

### E2 — UI-Sprache ist Englisch

Das Mockup ist deutsch, die App ist es nicht. `ux-rules.md` ist englisch, alle
`strings_*.rs` sind englisch; beim FIL-Umbau wurde derselbe Punkt schon einmal
so entschieden.

| Mockup | UI-String |
|---|---|
| Klang | **Sound** |
| Klangprofil | **Sound profile** |
| Klangfarbe · dunkel ↔ hell | **Timbre · dark ↔ bright** |
| Dynamik · dicht ↔ offen | **Dynamics · dense ↔ open** |
| Tempo · langsam ↔ schnell | **Tempo · slow ↔ fast** |
| Klingt ähnlich · von 1.821 | **Sounds like · of 1,821** |
| In die Warteschlange | **Add to queue** |
| Ähnliche suchen | **Find similar tracks** |

Alle neuen Strings in eine eigene `strings_sound_similarity.rs`, nach dem
Muster von `strings_song_visuals.rs`. Neue Strings gehören in `po/POTFILES.in`.

### E3 — Reiterleiste: vier kurze Wörter, kein Icon

Die 352 px des Briefs sind die **dp**-Zahl aus dem Mobile-Frame 19b. **NPP-1**
[active] fixiert das Desktop-Panel auf **300 px** → ~276 px innen → **~69 px**
pro Reiter bei vier.

Dazu ein Fund, der im Brief fehlt: `now_playing.rs:206–216` setzt einen
Breakpoint auf `MaxWidth 320 px`, der auf `DisplayMode::Icons` schaltet. Beim
300-px-Panel ist diese Bedingung **immer erfüllt** — die Leiste läuft heute
faktisch im Icons-Modus, obwohl der Builder `Labels` setzt.

Umsetzung:

1. Den Breakpoint-Schwellwert so weit senken, dass er beim regulären 300-px-Panel
   **nicht** greift (Labels), im Compact-Mode aber weiterhin (Icons). Der neue
   Wert wird **gemessen**, nicht geraten — NPP-1 endet ausdrücklich auf
   „measured rather than assumed".
2. `strings::VISUAL` von „Visualizer" auf **„Visuals"** kürzen (7 Zeichen,
   ~45 px + Padding), damit vier Labels in 276 px passen.
3. Icons bleiben für jeden Reiter gesetzt — sie sind der Rückfall unterhalb des
   Breakpoints, nicht Zierrat.
4. **Abnahmemessung, bevor die Regel auf `[active]` geht:** bei 300 px Panel und
   vier Reitern darf kein Label ellipsiert werden. Messen über die tatsächliche
   Label-Breite, nicht über eine Schätzung.

Passt (4) nicht: NICHT NPP-1 aufweichen und nicht die Icons wieder einschalten,
sondern **innehalten und melden** — dann ist die Wortwahl das Problem, nicht die
Geometrie.

### E4 — Tempo wird geschätzt, bleibt aus, und sagt warum

Tempo steckt nicht in den Daten. Es wird per Autokorrelation über die
Frame-Energie der Bassbänder geschätzt — 20 fps sind ein 50-ms-Raster.

- Standardmäßig **aus**, wie im Brief.
- Gewicht `w_tmp = 0.0`, solange aus.
- Die Tempo-Achse ist ausgegraut, solange die Option aus ist.
- Der Hinweistext benennt den **Oktavfehler** (Faktor 2), nicht bloß
  „unzuverlässig" — das ist die Fehlerart, die die Trefferliste tatsächlich
  kippt: *„Estimated from onsets; halftime and time changes can put it out by a
  factor of two."*
- `tempo` ist `Option<f32>` — nicht schätzbar heißt `None`, nicht 0.

### E5 — Leerzustand: Fortschritt mit Zahl

Solange Vektoren fehlen, zeigt der Reiter **„Analysing your library — 412 of
1843"** mit Balken statt einer leeren Fläche, und kippt in die Trefferliste,
sobald genug Vektoren da sind. Kein Auftauchen/Verschwinden des Reiters — das
wäre genau der springende Reiter, den 19b verbietet.

Schwellwert für „genug": **mindestens 50 Vektoren** *und* ein Vektor für den
laufenden Titel. Darunter wären die Perzentile Rauschen.

### E6 — Dateizeile aus Frame 18c wird mitgebaut

Über der Trefferliste, unter dem Klangprofil: Format, Bittiefe/Samplerate,
Dateigröße und die tatsächlich belegte obere Grenzfrequenz. Letztere kommt aus
dem gespeicherten Spektrogramm (höchstes Band über dem Rauschboden) und kostet
damit **keine neue Analyse**. Das ist die zweite der beiden Fragen, für die der
Info-Knopf laut 13a gebaut wurde.

### E7 — `frame_crest_db` heißt so, weil es kein Crest-Faktor ist

Ein Crest-Faktor ist Spitze/Effektivwert auf **Sample**-Ebene. Die Zellen sind
dB-komprimierte, u8-quantisierte Band-Energien bei 20 fps — daraus lässt sich
nur ein Frame-Level-Crest bilden (lauteste Frame-Summe gegen mittlere). Das
misst genau, was „dicht ↔ offen" braucht, ist aber nicht DR im Audio-Sinn und
darf nicht so heißen.

`waveform_peaks` scheidet als Quelle aus: laut S1-Plan **pro Track normalisiert**
und damit prinzipiell untauglich für einen Dynamikvergleich *zwischen* Tracks.

### E8 — In welchem Raum gerechnet wird

Der Brief verlangt Z-Standardisierung *und* Kosinusabstand auf L2-normierten
Bändern. Beides zusammen ist unterbestimmt: z-standardisierte Bänder sind nicht
mehr L2-normiert, und Kosinus über Vektoren mit negativen Komponenten läuft über
−1..1 statt 0..1. Festlegung:

- **`band_mean` bleibt L2-normiert und un-z-standardisiert.** Kosinus ist bereits
  skaleninvariant — das ist sein Zweck. Eine Z-Schicht davor zerstört genau die
  Eigenschaft, für die er gewählt wurde.
- **Z-standardisiert werden nur die Skalare** — `centroid_mean`, `centroid_var`,
  `frame_crest_db`, `tempo` —, deren absolute Einheiten sonst unvergleichbar
  sind und deren Streuung sonst die Gewichte 0,5/0,25/0,25 bedeutungslos macht.
- Streuung 0 (alle Werte gleich) → Merkmal trägt 0 bei, keine Division durch 0.

### E9 — Z-Standardisierung und Perzentilrang ersetzen einander nicht

Sie lösen verschiedene Probleme, und der Brief begründet beide mit demselben
Satz. Das darf nicht dazu führen, dass eins gebaut und für ausreichend gehalten
wird:

- **Perzentilrang der Abstände** macht die *Anzeige* skalenunabhängig.
- **Z-Standardisierung** korrigiert die *Rangfolge* — ohne sie dominiert das
  Merkmal mit der größten absoluten Streuung.

**Beide sind zu bauen.**

### E10 — Zwei verschiedene Perzentil-Verteilungen

1. **Trefferquote je Zeile** — Rang in der Verteilung der Abstände *vom aktuellen
   Titel zu allen anderen*. Neu bei jedem Titelwechsel.
2. **Achsenposition im Klangprofil** — Rang des *aktuellen Titels* in der
   Verteilung *eines Merkmals über die Bibliothek*. Gehört zur
   Bibliotheks-Statistik und wird mit ihr gecacht (sortierte Merkmalsspalten).

### E11 — Ausschlüsse wirken auf die Anzeige, nicht auf die Verteilung

„Gleiches Album ausschließen" / „gleicher Künstler ausschließen" filtern die
**angezeigten Zeilen**. Der Perzentilrang bezieht sich laut Brief auf die ganze
Bibliothek und wird **vor** dem Filtern gebildet — sonst verschiebt jedes
Umlegen einer Option sämtliche Zahlen, und „näher als 96 % deiner Bibliothek"
wäre gelogen.

### E12 — Marken-Regel, deterministisch

„Pro Gruppe die häufigste kind-Menge" ist bei Gleichstand undefiniert. In
**Local** steht nach dem Umbau je einmal `{panel-tab}`, `{panel-tab,
context-item}`, `{window}` — kein Gewinner.

**Regel:** Eine Menge ist „die häufigste" nur bei **strikter Mehrheit gegenüber
der zweithäufigsten und mindestens zwei Vorkommen.** Gibt es keine, bekommt
jede Zeile der Gruppe eine Marke.

Probe gegen Frame 19a:
- *Online content*: `{sidebar-section}` 5× (YouTube, Podcasts, Radio, New
  Releases, Concerts), `{extends}` 4× → Gewinner `{sidebar-section}`, keine
  Marke; die vier Füller bekommen eine. ✔ deckt sich mit 19a
- *Local*: 1/1/1 → kein Gewinner, alle drei bekommen Marken. ✔ deckt sich mit 19a

Zweiter Punkt: Die Regel rechnet über die **statische Registry**, nicht über den
Aktivierungszustand — sonst springen die Marken, sobald der Nutzer Plugins
abschaltet.

### E13 — Kein Überlaufmenü

Der Brief verlangt „ab dem fünften Überlaufmenü mit Zähler". Nach diesem Umbau
gibt es genau vier Reiter und kein weiteres Plugin, das einen beisteuert — fünf
ist nicht erreichbar. Das Menü wäre unbenutzbarer, untestbarer Code, und die
Reachability-Prozessregel verlangt, dass ein Test den Weg *aus dem Startzustand*
geht.

**Reihenfolge-Regel und Rückfall auf Queue werden gebaut** (beides erreichbar
und testbar). Das Überlaufmenü wird als `[planned]`-Regel dokumentiert und
**nicht** gebaut.

### E14 — `provides` ist ein Rust-Feld, kein Manifest-Format

Plugins sind hier keine ladbaren Artefakte; `ModuleDescriptor` ist ein `const`.
`provides` wird ein statisches `&'static [Provision]`. Kein Ladesystem, kein
Dateiformat, keine Registrierung zur Laufzeit.

---

## 2. Bauplan

Sieben Pakete, in dieser Reihenfolge. P1–P3 sind reines `reprise-core` und
haben keine GTK-Abhängigkeit.

### P1 — Merkmalsmodell und Speicher (`reprise-core`)

> **Abweichung bei der Umsetzung, festgehalten nach dem Review:** Der Plan
> nimmt an, `v56` sei frei. Das war schon bei Planaufstellung falsch —
> `db_new_releases_accent::migrate_v56` liegt auf der Basis `527d5cbbbc` und
> belegt die Nummer bereits (`db.rs:742`). Die Umsetzung teilt sich `v56`
> deshalb: `db_sound_features::migrate_v56` prüft die Anwesenheit von Tabelle
> und Trigger, statt `user_version` zu vertrauen, und klemmt auf
> `version.max(56)`. `SUPPORTED_SCHEMA_VERSION` bleibt bei 56 und wird
> **nicht** wie im Plantext angewiesen erhöht. Ein Test deckt die Koexistenz
> beider `v56`-Schritte ab.

**Neu:** `src/sound_features.rs`, `src/sound_features_tests.rs`,
`src/db_sound_features.rs`

```rust
pub struct SoundFeatures {
    pub band_mean: [f32; SPECTROGRAM_BAND_COUNT], // L2-normiert
    pub centroid_mean: f32,
    pub centroid_var: f32,
    pub frame_crest_db: f32,
    pub tempo: Option<f32>,
}
```

- Ableitung aus `&TrackSpectrogram`: **rein, ohne Datei-I/O, ohne DB.** Das ist
  die testbare Kernfunktion.
- `centroid_*`: spektraler Schwerpunkt je Frame über den Bandindex, dann
  Mittel und Streuung über die Frames.
- `frame_crest_db`: lauteste Frame-Summe gegen mittlere Frame-Summe, in dB.
- `tempo`: Autokorrelation über die Frame-zu-Frame-Energiedifferenz der unteren
  Bänder; `None`, wenn kein Maximum sicher über dem Untergrund liegt.
- Migration **v56**: `track_sound_features` (`track_id` PK →
  `tracks(id) ON DELETE CASCADE`, `format_version INTEGER NOT NULL`, `data BLOB`).
  `format_version` = `SPECTROGRAM_FORMAT_VERSION`; abweichende Version = Zeile
  ist ungültig und wird neu abgeleitet.
- Trigger `invalidate_track_render_data` um `DELETE FROM track_sound_features
  WHERE track_id = NEW.id` erweitern — dieselbe Invalidierung wie beim
  Spektrogramm, an derselben Stelle, nicht als zweiter Mechanismus.
- `SUPPORTED_SCHEMA_VERSION` auf 56; die Migrations-Assertions in den
  bestehenden Tests mitziehen (beim v27-Umbau waren das 31 Stellen — hier
  ebenso vollständig nachziehen).

### P2 — Bibliotheks-Statistik und Abstand (`reprise-core`)

**Neu:** `src/sound_stats.rs`, `src/sound_distance.rs`, `src/sound_neighbours.rs`
(+ je `_tests.rs`)

- `sound_stats`: Mittel/Streuung je Skalar über alle Vektoren **plus** die
  sortierten Merkmalsspalten für die Achsen-Perzentile (E10.2).
- Neuberechnung bei **> 5 %** Bestandsänderung seit der letzten Statistik;
  Zähler in `library::settings`. Nicht bei jedem Import.
- `sound_distance`: die Formel des Briefs, im Raum aus E8.
  Gewichtsvorgaben als Konstanten: `Default` (band 0.5 / timbre 0.25 / dyn 0.25 /
  tempo 0.0), `Timbre`, `Dynamics`.
- `sound_neighbours`: aktueller Vektor gegen alle → sortierte Trefferliste mit
  Perzentilrang (E10.1), Ausschlüsse nach E11 **nach** der Perzentilbildung.

### P3 — Erzeugung und Nachzug (`reprise-core` / `reprise-platform-linux`)

- Vektor entsteht, sobald ein Spektrogramm geschrieben wird
  (`set_track_render_data`).
- Nachzug für bereits gespeicherte Spektrogramme über den bestehenden
  Backfill-Weg — **kein zweiter Dekodier-Pass.** Die Spektrogramme liegen in der
  DB; die Ableitung ist reine Rechenarbeit.

### P4 — Der Reiter (`reprise-gnome`)

**Neu:** `src/ui/now_playing/sound_panel/` (mod, Profil, Liste, Fußzeile),
`src/ui/strings_sound_similarity.rs`

- `PanelTab::Sound` + `SOUND_PAGE` in `panel_state.rs`, `TabFooters.sound`.
- Aufbau von oben, nach Frame 18c:
  1. **Sound profile** — drei waagerechte Achsen mit Markierung; Position =
     Perzentil in der Bibliothek. Tempo-Achse ausgegraut, solange die Option aus
     ist.
  2. **Dateizeile** (E6).
  3. Trennlinie, **„Sounds like"** mit der Trefferzahl der Bibliothek.
  4. Sieben Zeilen: Cover 34 px, Titel, Künstler, Balken, Perzentil. Klick
     spielt, Rechtsklick öffnet das übliche Titel-Kontextmenü.
  5. Fußzeile: **„Add to queue"** (füllend) + Überlaufknopf. Hängt die Treffer
     **in der gezeigten Reihenfolge** an, ohne Shuffle.
- Gating **exakt nach dem `song_visuals`-Muster**: `sound_page.set_visible(enabled)`,
  bei Abschaltung mit offenem Reiter Rückfall auf `UP_NEXT_PAGE`.
- Berechnung im Worker, Ergebnis gehalten. Der Reiter zeigt beim Öffnen sofort —
  beim Öffnen wird nichts gerechnet.
- Leerzustand nach E5.

### P5 — Einstieg und Reiterleiste (`reprise-gnome`)

- Info-Knopf rechts neben Repeat in `player_bar_layout.rs`, **außerhalb** von
  `transport_row` — er ist keine Wiedergabesteuerung, und das ist der Grund für
  die Absetzung. Öffnet die rechte Spalte und schaltet auf Sound.
- Tastenkürzel in `shortcuts.rs` (ein `i` ohne Kürzel findet man einmal).
- Breakpoint-Anpassung + „Visuals" nach **E3**, inklusive der Abnahmemessung.
- Reiterreihenfolge fest: die eingebauten drei zuerst, Erweiterungen dahinter in
  der Reihenfolge ihrer Aktivierung.
- Icon für Sound aus dem installierten Adwaita-Set wählen und die Wahl im
  Kommentar begründen — so wie es beim Visualizer-Icon
  (`network-cellular-signal-excellent-symbolic`) bereits vorgemacht ist. Das
  „hub"-Glyph des Mockups ist Material, nicht Adwaita.

### P6 — `provides` und der Plugins-Dialog

- `ModuleDescriptor.provides: &'static [Provision]`,
  `ProvisionKind::{PanelTab, SidebarSection, Window, ContextItem, Extends}`,
  `Provision { kind, label, target: Option<&'static str> }`.
- **Alle** bestehenden Module bekommen ihren Eintrag; Wortlaut aus Frame 19a
  (englisch nach E2).
- `SOUND_SIMILARITY_MODULE` in Gruppe **Local**, `default_enabled: false`,
  `applies_live: true`, Beschreibung *„Compare timbre, dynamics and tempo across
  the local library"*.
- Marken-Auswahl nach **E12**, deterministisch, über die statische Registry.
  Farbe: `PanelTab` und `SidebarSection` eingefärbt, alles andere grau.
- `ExpanderRow` für Sound Similarity (die Datei kennt das Muster schon):
  | Einstellung | Vorgabe |
  |---|---|
  | Exclude tracks from the same album | **an** |
  | Exclude tracks by the same artist | aus |
  | Include tempo | aus (+ Hinweis nach E4) |
  | Weighting | Default / Timbre / Dynamics |
  | Number of matches | 7 |
- Kontextmenü-Eintrag **„Find similar tracks"** in `track_menu.rs`, sichtbar nur
  bei aktivem Modul.

### P7 — Regelwerk

- Neue Sektion **AH. Sound Similarity** in `docs/ux-rules.md`, Regeln
  `SIM-1 … SIM-n`, englisch.
- Reiterleisten-Regeln gehören **nicht** dorthin, sondern als **NPP-14 ff.** in
  Sektion P — sie betreffen die Leiste, nicht dieses Plugin. IDs sind
  append-only.
- Das Überlaufmenü ab fünf Reitern als `[planned]` (E13).
- Status `[active]` **im selben Commit** wie die Umsetzung, mit regelbenannten
  Tests (`fn sim_1_…`, `fn npp_14_…`).
- Korrektur des S1-Absatzes nach **E1** im selben Commit.

---

## 3. Testdisziplin

Bindend, aus `ux-rules.md` „Process rules" und aus der Projekterfahrung:

- **Ein primärer Regel-ID je Testname.** Deckt ein Test nebenbei weitere Regeln
  ab, zählt das nicht — die zweite Regel braucht ihren eigenen Test.
- **`[gtk]`-Regeltests müssen display-frei sein** — das Merge-Gate läuft ohne
  Xvfb. Display-Beweise als nicht-regelbenannte `#[ignore]`-Tests mit dem
  Marker `"requires a display; run via xvfb-run"`.
- Der Kern von P1/P2 ist reine Rechenarbeit und gehört auf die **unterste
  Ebene** (`[core]`, Workspace-Suite) — nicht in GTK-Tests.
- Testebene ist immer die **niedrigste, die die Regel widerlegen kann**.

---

## 4. Abnahme

Vier der fünf Prüfungen des Briefs setzen einen vollständigen Backfill voraus
(0 Spektrogramme heute) und gehören deshalb ans Ende der Kette, **nicht ins
Merge-Gate**:

1. **Verteilung** — in der genre-homogenen Sammlung müssen die Perzentile über
   den ganzen Bereich streuen. Histogramm über ≥ 50 Stichproben-Titel. **Liegen
   alle Treffer über 90, greift die Z-Standardisierung nicht** — dann ist der
   Befund zu melden, nicht zu übertünchen.
2. **Leise gegen laut** — je ein sehr leiser und ein sehr lauter Titel; die
   Trefferlisten dürfen sich kaum überschneiden.
3. **Album-Ausschluss** — keine Geschwister in der Liste.
4. **Abschalten bei offenem Reiter** — Auswahl fällt auf Queue, kein Absturz,
   keine leere Fläche. Als display-freier `[gtk]`-Test baubar → **gehört ins
   Gate.**
5. **Titelwechsel blockiert nicht** — bei 1843 Titeln messen. **Nur per Timer
   messen**, nicht per Frame-Sampling: Frame-Sampling liefert 0 Samples und wird
   dadurch scheinbar grün. Gegenprobe mit deaktiviertem Pfad ist Pflicht.

Zusätzlich aus E3: **Kein Reiter-Label wird bei 300 px Panelbreite ellipsiert.**

---

## 5. Ausdrücklich nicht Teil dieser Arbeit

- Der Spektrogramm-Backfill selbst (S1, bereits gelandet).
- Das Überlaufmenü ab fünf Reitern (E13).
- Mobile/Android — die Frames 12, 17, 21 des Design-Dokuments gehören zu einer
  anderen Kette.
- Jede Änderung am gespeicherten Spektrogramm-Format (E1).
