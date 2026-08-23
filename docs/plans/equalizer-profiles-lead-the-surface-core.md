---
slug: equalizer-profiles-lead-the-surface-core
worktree: /home/marvin/Projects/reprise-equalizer-profiles-lead-the-surface-core
branch: feature/equalizer-profiles-lead-the-surface-core
phase: planned
codex_session:
created: 2026-08-23
mother: docs/plans/equalizer-profiles-lead-the-surface.md
---
# Strang `core` — Zehn geteilte Profile

Strang von [`equalizer-profiles-lead-the-surface.md`](equalizer-profiles-lead-the-surface.md).
Der Mutterplan enthält den Beschluss, die Bestandsaufnahme, die Kurventabelle
und die Regeln für den Umsetzer. **Zuerst dort die Abschnitte „Die zehn
Profile" und „Regeln für den Umsetzer" lesen** — die Pegel dort sind bindend.

## Dateibesitz dieses Strangs

```
crates/reprise-core/src/equalizer.rs
crates/reprise-android-ffi/src/playback_settings.rs
crates/reprise-android-ffi/src/playback_settings_tests.rs
docs/plans/equalizer-profiles-lead-the-surface-core.md
```

Nichts sonst. Alles unter `crates/reprise-gnome/`, `android/`, `po/` und
`docs/ux-rules.md` gehört dem Strang `ui` und wird hier nicht angefasst.

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
