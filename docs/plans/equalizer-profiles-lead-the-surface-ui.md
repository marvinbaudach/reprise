---
slug: equalizer-profiles-lead-the-surface-ui
worktree: /home/marvin/Projects/reprise-equalizer-profiles-lead-the-surface-ui
branch: feature/equalizer-profiles-lead-the-surface-ui
phase: planned
codex_session:
created: 2026-08-23
mother: docs/plans/equalizer-profiles-lead-the-surface.md
---
# Strang `ui` — Das Profil führt, die Regler klappen auf

Strang von [`equalizer-profiles-lead-the-surface.md`](equalizer-profiles-lead-the-surface.md).
Der Mutterplan enthält den Beschluss, die Bestandsaufnahme, die Kurventabelle
und die Regeln für den Umsetzer. **Zuerst dort die Abschnitte „Die zehn
Profile" und „Regeln für den Umsetzer" lesen.**

## Vorbedingung

Dieser Strang setzt auf dem Strang `core` auf: die sechs neuen Varianten von
`EqualizerPreset` und die gespiegelte `AndroidEqualizerPreset` sind in diesem
Zweig bereits vorhanden. Sie werden hier **nicht** erneut angelegt und nicht
verändert.

## Dateibesitz dieses Strangs

```
crates/reprise-gnome/src/ui/preferences/**
crates/reprise-gnome/src/ui/strings.rs
android/app/src/main/java/de/reprise/spike/**
android/app/src/test/java/de/reprise/spike/**
po/reprise.pot
po/{ar,bn,de,es,fr,hi,zh_CN}.po
docs/ux-rules.md
docs/plans/equalizer-profiles-lead-the-surface-ui.md
```

Nichts sonst. `crates/reprise-core/src/equalizer.rs` und
`crates/reprise-android-ffi/src/playback_settings.rs` gehören dem Strang `core`
und werden hier **nicht** angefasst — auch nicht „nur kurz".

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
