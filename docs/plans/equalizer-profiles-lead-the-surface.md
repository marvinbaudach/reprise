---
slug: equalizer-profiles-lead-the-surface
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-23
strands: core,ui
merge_order: ui
---
# Der Equalizer wird über Profile bedient, nicht über zehn Regler

Beschluss des Nutzers vom 23.08.2026, wörtlich:

> „gern noch den equilizer überarbieten, so dass man profile hat und nicht
> manuel etwas umstellt. das braucht keiner"

und auf Rückfrage nach dem Umfang:

> „mach doch die 10 profile"

Gelesen gegen `origin/dev` @ `206d1290dd`. Jede Zeilenangabe stammt aus diesem
Stand. **Der lokale Hauptcheckout hängt 40 Commits zurück** — wer dort prüft,
prüft das Falsche.

## Der Kern in einem Satz

Profile gibt es auf beiden Plattformen schon; sie sind nur zu wenige und stehen
gleichberechtigt neben den Reglern. Dieser Plan macht die Profilauswahl zur
Bedienoberfläche und die Regler zu einer Sache, die man aufklappen *kann*.

## Was heute da ist (gemessen)

| Stelle | Datei @ `206d1290dd` | Verhalten |
| --- | --- | --- |
| Kurven | `crates/reprise-core/src/equalizer.rs:22-44` | `EqualizerPreset` mit **vier** Varianten (`Flat`, `Rock`, `Pop`, `Bass`), `ten_band_levels()` als einzige Wahrheit, `ALL: [Self; 4]` |
| Projektion | `crates/reprise-core/src/equalizer.rs` (`EqualizerCurve::project_to_gstreamer`) | Eine authored Kurve wird auf die Bandtopologie des jeweiligen Backends projiziert — Profile sind topologie-unabhängig |
| Desktop-Auswahl | `crates/reprise-gnome/src/ui/preferences/preferences.rs:548-560` | `AdwComboRow` mit vier fest aufgezählten Strings; Auswahlabgleich über `(0..4).find(…)` |
| Desktop-Mapping | `.../preferences.rs:62-70` | `equalizer_preset(index: u32)` bildet `0..3` auf `EqualizerPreset` ab — Index-Literale, keine Ableitung aus `ALL` |
| Desktop-Wächter | `.../preferences.rs:603` Umfeld | `if updating.get() || row.selected() > 3 { return }` — **die `3` ist eine zweite Stelle, an der die Anzahl steht** |
| Desktop-Regler | `crates/reprise-gnome/src/ui/preferences/preference_playback.rs:22-78` | `build_equalizer_surface` baut zehn vertikale `GtkScale` in einer scrollbaren Karte, immer sichtbar |
| Desktop-Strings | `crates/reprise-gnome/src/ui/strings.rs:189-193` | `EQUALIZER_PRESET`, `PRESET_FLAT`, `PRESET_ROCK`, `PRESET_POP`, `PRESET_BASS` (Anzeigetext „Bass Boost") |
| Brücke | `crates/reprise-android-ffi/src/playback_settings.rs:14-20`, `:76-83` | `AndroidEqualizerPreset` **spiegelt** die vier Varianten von Hand, plus `From`-Arme |
| Brücke | `.../playback_settings.rs:105-…` | `standard_equalizer_presets()` iteriert `EqualizerPreset::ALL` — **wächst von allein mit** |
| Android-Auswahl | `android/app/src/main/java/de/reprise/spike/PlaybackSettingsScreen.kt:143-152`, `:233-273` | `EqualizerPresetPicker`: Button + `DropdownMenu`, zeigt „Custom", wenn die gespeicherte Kurve zu keinem Profil passt |
| Android-Namen | `.../MainActivity.kt:765-770` | `displayName()` als `when` über die vier Varianten — **hart, kein Fallback** |
| Android-Regler | `.../PlaybackSettingsScreen.kt:153-192` | Bandzeilen des **Geräts** (nicht die zehn GStreamer-Bänder), nur bei laufender Wiedergabe lesbar, hinter einem Bestätigungsdialog „Replace equalizer curve?" |

Drei Befunde, die den Zuschnitt bestimmen:

1. **Die Kurven sind schon geteilt.** Der Desktop dupliziert nichts, er mappt
   nur Indizes auf `EqualizerPreset`. Neue Profile in Core erscheinen deshalb
   fast von allein auf beiden Seiten — die Handarbeit steckt in den
   *Aufzählungen*, die die Anzahl vier festschreiben.
2. **Die Anzahl steht an fünf Stellen**: `EqualizerPreset::ALL`, das `match` in
   `equalizer_preset()`, der Vergleich `(0..4)`, der Wächter `row.selected() > 3`
   und die Stringliste in `preferences.rs`. Vier davon müssen verschwinden, sonst
   ist das nächste Profil wieder eine Sucharbeit.
3. **Die beiden „Regler" sind nicht dasselbe.** Der Desktop stellt zehn feste
   GStreamer-Bänder, Android stellt die Bänder des Geräts und braucht dafür
   laufende Wiedergabe. Ein gemeinsamer Reglerentwurf ist deshalb *nicht* das
   Ziel — gemeinsam ist nur, dass beide eingeklappt sind.

## Entscheidungen, die dieser Plan fällt

1. **Zehn feste Profile, keine eigenen.** Kein Speichern, kein Benennen, keine
   Verwaltungsfläche, keine neue Tabelle. Ausdrücklich vom Nutzer so entschieden.
2. **Die Regler verschwinden nicht, sie klappen zu.** Wer heute eine eigene
   Kurve gespeichert hat, verliert sie nicht und kommt weiter an sie heran.
   Standard ist zugeklappt.
3. **Eine verstellte Kurve ist kein Profil.** Sobald ein Band von Hand bewegt
   wird, zeigt die Auswahl „Eigen" bzw. „Custom" und **die Kurve wird
   gespeichert wie heute**. Sie bekommt keinen Namen und keinen Platz in der
   Liste. Das ist heute auf beiden Seiten schon so und bleibt so.
4. **Die Anzahl wird abgeleitet, nicht wiederholt.** Jede Stelle, die heute
   `4` oder `3` schreibt, liest künftig `EqualizerPreset::ALL`.
5. **Die Profilnamen sind übersetzbar** und stehen im GNOME-Katalog. Android
   bleibt bei englischen Literalen — dort ist heute die gesamte Einstellseite
   unübersetzt, und dieser Plan macht daraus keine Übersetzungsbaustelle.
6. **Keine neuen Bedingungen.** Kein „nur mit Kopfhörer", keine automatische
   Profilwahl nach Genre. Das wäre ein eigener Vorgang.

## Die zehn Profile

Kurven als GStreamer-Zehnband-Pegel in dB, Reihenfolge wie
`GSTREAMER_EQUALIZER_CENTRES_HZ` (29, 59, 119, 237, 474, 947, 1889, 3770, 7523,
15011 Hz). Grenzen bleiben −12 … +12 dB.

| Variante | Anzeigename | Pegel | Absicht |
| --- | --- | --- | --- |
| `Flat` | Flat | `[0,0,0,0,0,0,0,0,0,0]` | unverändert (bestehend) |
| `Rock` | Rock | `[4,3,2,0,-1,0,2,3,4,4]` | unverändert (bestehend) |
| `Pop` | Pop | `[-1,1,3,4,2,0,-1,-1,1,2]` | unverändert (bestehend) |
| `Bass` | Bass Boost | `[7,6,5,3,1,0,0,0,0,0]` | unverändert (bestehend) |
| `Classical` | Classical | `[2,2,1,0,0,0,0,1,2,3]` | breite, flache Anhebung an beiden Enden, Mitten unangetastet |
| `Jazz` | Jazz | `[3,2,1,2,-1,-1,0,1,2,3]` | warme Tiefen, leicht zurückgenommene Präsenz, offene Höhen |
| `Electronic` | Electronic | `[5,4,1,0,-2,1,0,1,4,5]` | Bass und Luft betont, Mitten abgesenkt |
| `Vocal` | Vocal & Podcast | `[-4,-3,-2,1,3,4,4,3,1,0]` | Tiefen weg, Sprachverständlichkeit bei 500 Hz–4 kHz vorn |
| `Headphones` | Headphones | `[4,3,1,0,-1,-1,0,2,3,2]` | Tiefenanhebung und leichte Präsenzsenke gegen Kopfhörerhärte |
| `LateNight` | Late Night | `[-4,-3,-1,1,2,3,2,1,-2,-4]` | Extreme gekappt, Mitten vorn — leise hören ohne Verständlichkeitsverlust |

Die sechs neuen Kurven sind Autorenentscheidungen dieses Plans, keine Kopie
einer fremden Tabelle. **Sie sind bindend** — wer sie ändern will, ändert diesen
Plan.

Reihenfolge in der Aufzählung und damit in beiden Oberflächen: `Flat`, `Rock`,
`Pop`, `Bass`, `Classical`, `Jazz`, `Electronic`, `Vocal`, `Headphones`,
`LateNight`. Flat bleibt an erster Stelle, die vier bestehenden behalten ihre
Indizes — eine gespeicherte Auswahl bleibt damit dieselbe.

## Regeln für den Umsetzer — zuerst lesen

- Die Dateiliste je Aufgabe ist **Startpunkt, kein Zaun**. Fehlt ein Feld oder
  eine Signatur, wo der Plan sie vermutet: weitersuchen und umsetzen. Anhalten
  nur, wenn der *Vertrag* falsch ist — dann melden, nicht raten.
- **PR 1 vor PR 2.** PR 2 braucht die neuen Varianten aus PR 1 und auf Android
  eine frische UniFFI-Bindingerzeugung.
- Jede Aufgabe endet mit ihren Tests. Kein „Tests kommen am Ende".
- Für den Gettext-Anteil: **alle sieben Kataloge** (`ar`, `bn`, `de`, `es`,
  `fr`, `hi`, `zh_CN`) plus `reprise.pot`. Das Gate bricht beim ersten fehlenden
  Locale ab — „fehlt in `ar`" heißt in aller Regel: es fehlt in allen sieben.

---

## PR 1 (`core`) — Zehn geteilte Profile, und die Anzahl steht nur noch an einer Stelle

### Aufgabe 1.1 — Sechs Varianten dazu

`crates/reprise-core/src/equalizer.rs`

- `EqualizerPreset` bekommt `Classical`, `Jazz`, `Electronic`, `Vocal`,
  `Headphones`, `LateNight` — in der oben festgelegten Reihenfolge **nach**
  `Bass`.
- `ALL` wird `[Self; 10]` und listet alle zehn in derselben Reihenfolge.
- `ten_band_levels()` bekommt die sechs Zeilen aus der Tabelle oben.

Tests in derselben Datei:

- `every_preset_stays_inside_the_gstreamer_gain_range` — über `ALL` iterieren,
  jeder Pegel liegt in `-12.0..=12.0`. Fängt einen Tippfehler in einer neuen
  Kurve ab, bevor ihn ein Backend klemmt.
- `every_preset_projects_back_to_its_own_levels` — für jedes Profil
  `curve().project_to_gstreamer()` gegen `ten_band_levels()` prüfen. Das ist der
  Nachweis, dass eine neue Kurve die Projektion unbeschadet übersteht, nicht nur
  die alten vier.
- `only_flat_is_silent` — genau ein Profil hat lauter Nullen. Fängt ab, dass
  eine neue Variante versehentlich mit `[0.0; 10]` angelegt und nie gefüllt wird
  (der Fehler, den `every_preset_…_range` gerade *nicht* sieht).
- `presets_are_pairwise_distinct` — keine zwei Profile haben dieselben Pegel.

### Aufgabe 1.2 — Die Brücke spiegelt zehn statt vier

`crates/reprise-android-ffi/src/playback_settings.rs`

- `AndroidEqualizerPreset` bekommt die sechs neuen Varianten.
- Der `From<EqualizerPreset>`-Arm bekommt die sechs Zeilen. **Kein
  `_ =>`-Auffangarm** — der `match` muss erschöpfend bleiben, damit das nächste
  Profil hier einen Compilerfehler auslöst statt still auf `Flat` zu fallen.
- `standard_equalizer_presets()` bleibt unverändert; es iteriert `ALL`.

Test in derselben Datei:

- `the_bridge_offers_every_shared_preset` — `standard_equalizer_presets().len()`
  ist `EqualizerPreset::ALL.len()`, und die Reihenfolge der `preset`-Felder
  entspricht `ALL`. **Der Vergleich geht gegen `ALL.len()`, nicht gegen die
  Zahl 10** — sonst ist der Test die sechste Stelle, an der die Anzahl steht.

### Abnahme PR 1

`cargo test -p reprise-core equalizer` und
`cargo test -p reprise-android-ffi playback_settings` grün, die neuen Tests
namentlich im Übergabebericht.

Zusätzlich ein Mutationsnachweis, der wirklich Produktionscode trifft: eine
neue Kurve in `ten_band_levels()` auf `[0.0; 10]` setzen → `only_flat_is_silent` und `presets_are_pairwise_distinct` müssen rot werden.
Änderung danach zurücknehmen und den zurückgenommenen Stand belegen.

---

## PR 2 (`ui`) — Das Profil führt, die Regler klappen auf

Braucht PR 1. **Beginnt auf Android mit einer Bindingerzeugung.**

### Aufgabe 2.1 — Der Desktop leitet die Liste ab, statt sie aufzuzählen

`crates/reprise-gnome/src/ui/preferences/preferences.rs`,
`crates/reprise-gnome/src/ui/strings.rs`

- `strings.rs`: sechs neue Konstanten neben den bestehenden — `PRESET_CLASSICAL`
  („Classical"), `PRESET_JAZZ` („Jazz"), `PRESET_ELECTRONIC` („Electronic"),
  `PRESET_VOCAL` („Vocal & Podcast"), `PRESET_HEADPHONES` („Headphones"),
  `PRESET_LATE_NIGHT` („Late Night"). Dazu `EQUALIZER_MANUAL` („Adjust bands
  manually") für den Aufklapper aus 2.2.
- `preferences.rs`: eine Funktion `preset_label(preset: EqualizerPreset) ->
  &'static str` mit erschöpfendem `match`. Die `StringList` entsteht aus
  `EqualizerPreset::ALL.map(preset_label)`, nicht aus vier Literalen.
- `equalizer_preset(index: u32)` wird zu
  `EqualizerPreset::ALL.get(index as usize).copied()` (Rückgabe `Option`), die
  Pegel holt der Aufrufer über `ten_band_levels()`.
- Der Auswahlabgleich `(0..4).find(…)` liest `EqualizerPreset::ALL` und
  vergleicht die projizierten Pegel.
- Der Wächter `row.selected() > 3` wird
  `row.selected() as usize >= EqualizerPreset::ALL.len()`.

Tests in `preferences_tests.rs`:

- `the_preset_row_offers_every_shared_preset` — das Modell der `ComboRow` hat
  `EqualizerPreset::ALL.len()` Einträge, und der Eintrag an Index `n` trägt
  `preset_label(ALL[n])`.
- `choosing_a_new_preset_stores_its_bands` — ein *neu hinzugekommenes* Profil
  (z. B. `Vocal`) auswählen und `settings::get_equalizer_bands` gegen
  `EqualizerPreset::Vocal.ten_band_levels()` prüfen. Ein Test, der nur `Rock`
  wählt, hätte auch vor diesem PR bestanden.
- `moving_a_band_clears_the_preset_selection` — bestehendes Verhalten
  festnageln, damit der Umbau es nicht verliert.

### Aufgabe 2.2 — Der Desktop klappt die Regler ein

`crates/reprise-gnome/src/ui/preferences/preference_playback.rs`,
`crates/reprise-gnome/src/ui/preferences/preferences.rs`

- `build_equalizer_surface` bleibt, was es ist, und behält seine Signatur und
  seinen Rückgabetyp. **Die Karte wandert lediglich in einen Aufklapper** —
  eine `adw::ExpanderRow` mit Titel `strings::EQUALIZER_MANUAL`,
  `expanded(false)`, deren einziges Kind die bisherige `surface.root` ist.
  Der Aufklapper wird dort in die Gruppe eingehängt, wo heute `surface.root`
  eingehängt wird, und **hinter** der Profil-`ComboRow`.
- Der Aufklapper übernimmt die `set_sensitive`-Steuerung, die heute an
  `surface.root` hängt (`self.equalizer_surfaces`) — er ist ab jetzt das
  Element in dieser Liste, sonst folgt ein zugeklappter Aufklapper dem
  Aus-Zustand des Equalizers nicht mehr.
- Reihenfolge auf der Playback-Seite bleibt sonst unverändert (Transitions
  führt, siehe Kommentar bei `preferences.rs:590`).

Tests:

- In `preference_playback.rs`: der bestehende
  `equalizer_bands_share_one_scrollable_card_and_follow_enabled_state` bleibt
  gültig und **unverändert** — er prüft die Karte selbst, nicht ihre Verpackung.
  Bleibt er es nicht, ist die Signatur doch angefasst worden; dann anhalten und
  melden.
- In `preferences_tests.rs`: `the_bands_start_collapsed_behind_the_profile` —
  der Aufklapper existiert, `is_expanded()` ist `false`, und die Profil-Zeile
  steht im Elternteil **vor** ihm.
- In `preferences_tests.rs`: `disabling_the_equalizer_dims_the_collapsed_bands`
  — Equalizer-Schalter aus, der Aufklapper ist nicht mehr `is_sensitive()`.

### Aufgabe 2.3 — Android klappt die Gerätebänder ein

`android/app/src/main/java/de/reprise/spike/PlaybackSettingsScreen.kt`,
`android/app/src/main/java/de/reprise/spike/MainActivity.kt`

**Zuerst die UniFFI-Bindings neu erzeugen**, sonst kennt Kotlin die sechs neuen
Varianten nicht.

- `MainActivity.kt:765-770`: `displayName()` bekommt die sechs neuen Arme
  (Anzeigenamen wie in der Tabelle). Der `when` bleibt erschöpfend über den
  Enum — **kein `else`-Zweig**.
- `PlaybackSettingsScreen.kt`: Alles ab „bands leer / Bandzeilen / Edit
  equalizer" (heute `:153-192`) wandert hinter einen zugeklappten Abschnitt mit
  der Überschrift „Adjust bands manually", der sich per Tippen öffnet
  (`rememberSaveable` für den Zustand, wie beim vorhandenen Picker). Der
  Bestätigungsdialog „Replace equalizer curve?" bleibt unverändert dahinter —
  er schützt eine andere Sache (das Überschreiben der gespeicherten Kurve mit
  den Gerätebändern) und wird von diesem Plan nicht angefasst.
- Die Abwesenheitstexte (`NO_PLAYBACK_YET`,
  `NO_EQUALIZER_ON_THIS_DEVICE`) ziehen **mit in den Abschnitt**. Sie erklären
  die Regler; über der Profilauswahl zu stehen, wo sie nichts erklären, wäre
  eine Verschlechterung.
- Der `EqualizerPresetPicker` bleibt, wo er ist, und ist damit das erste
  Bedienelement unter dem Equalizer-Schalter. Sein „Custom"-Fall bleibt
  unverändert.

Tests in `android/app/src/test/java/de/reprise/spike/` (dort, wo die
bestehenden `PlaybackSettings`-Tests liegen — vorher `grep` auf
`EqualizerPresetUi`, um die Datei zu finden):

- `the_picker_offers_every_shared_preset` — `standardEqualizerPresets()`
  gemappt auf `EqualizerPresetUi` ergibt so viele Einträge wie die Brücke
  liefert, und jeder hat einen nichtleeren Namen. **Fängt genau den Fehler ab,
  den ein `else`-Zweig in `displayName()` verstecken würde.**
- `the_band_section_starts_collapsed` — im frisch aufgebauten Zustand ist keine
  Bandzeile und kein „Edit equalizer" sichtbar, der Picker dagegen schon.

### Aufgabe 2.4 — Kataloge und Regelwerk

`po/reprise.pot`, `po/{ar,bn,de,es,fr,hi,zh_CN}.po`, `docs/ux-rules.md`

- Die sieben neuen Zeichenketten aus 2.1 in `reprise.pot` aufnehmen und in
  **allen sieben** Katalogen übersetzen. Keine leeren `msgstr`.
- `docs/ux-rules.md`: eine neue Regel **SET-17** (nächste freie Nummer,
  geprüft: `SET-16` ist die höchste) im Abschnitt der `SET`-Regeln:

  > **SET-17** [active] [gtk] — Der Equalizer wird über sein Profil bedient.
  > Unter dem Enable-Schalter steht die Profilauswahl; die zehn Bandregler
  > liegen darunter in einem `AdwExpanderRow`, der zugeklappt startet. Eine
  > von Hand verstellte Kurve bleibt gespeichert und lässt die Profilauswahl
  > leer — sie wird nie zu einem benannten Profil. Die Profilliste wird aus
  > `reprise_core::equalizer::EqualizerPreset::ALL` abgeleitet, nirgends
  > aufgezählt.

### Abnahme PR 2

1. `cargo test -p reprise-gnome preferences` und
   `cargo test -p reprise-gnome preference_playback` grün.
2. Die Android-JVM-Suite grün (JDK 21; das Skript setzt `LD_LIBRARY_PATH`
   selbst — **nicht von Hand setzen**, das entwertet den Beleg).
3. Das Gettext-Gate grün, mit dem Kommandotext im Bericht.
4. Von Hand am Desktop, mit Aufnahme im Übergabebericht:
   - Einstellungen → Playback: unter dem Schalter steht die Profilauswahl, die
     Regler sind zu.
   - Alle zehn Profile stehen in der Liste, in der Reihenfolge der Tabelle.
   - „Vocal & Podcast" wählen → hörbar/messbar andere Bänder; Aufklapper öffnen
     → die Regler stehen auf `[-4,-3,-2,1,3,4,4,3,1,0]`.
   - Ein Band von Hand bewegen → die Profilauswahl wird leer, die Kurve bleibt
     nach Neustart erhalten.
   - Equalizer-Schalter aus → der zugeklappte Aufklapper ist ausgegraut.
5. Von Hand am Handy, mit Aufnahme: Profilauswahl steht vorn, Bandabschnitt ist
   zu, alle zehn Namen erscheinen, keiner ist leer oder doppelt.

---

## Nicht in diesem Plan

- **Eigene, benennbare Profile.** Vom Nutzer ausdrücklich abgelehnt. Käme eine
  Speicherung, Verwaltung und eine neue Tabelle hinzu.
- **Automatische Profilwahl** nach Genre, Kopfhörer oder Tageszeit. Eigener
  Vorgang; `LateNight` ist hier ein Profil zum Auswählen, kein Automatismus.
- **Ein gemeinsamer Reglerentwurf für Desktop und Handy.** Die beiden stellen
  verschiedene Dinge (feste GStreamer-Bänder gegen die Bänder des Geräts).
  Gemeinsam ist nur, dass beide eingeklappt sind.
- **Übersetzung der Android-Einstellseite.** Sie ist heute vollständig
  englisch; dieser Plan ändert daran nichts in die eine oder andere Richtung.
- **Der Bestätigungsdialog „Replace equalizer curve?" auf Android.** Er schützt
  das Überschreiben der gespeicherten Kurve durch die Gerätebänder und ist von
  der Profilfrage unabhängig.
