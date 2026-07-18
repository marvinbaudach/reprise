# Theme-Flächenhierarchie + Petrol-Fallback — Taskplan (2026-07-18)

Setzt die Beschlüsse **7** (14a-Flächenhierarchie) und **8** (Fallback-Akzent =
Theme-Akzent) aus dem NPP-Grilling um. Beschluss-Ledger:
`docs/superpowers/plans/2026-07-18-npp-beschluesse.md` auf Branch
`feat/now-playing-panel` (liegt bewusst nicht auf diesem Branch). Branch
`feat/theme-surface-hierarchy`, Basis `main@fec994c`.

Design-Referenz ist Frame **14a** („Korrektur-Pass"): Tabelle ist die dunkelste
zentrale Fläche (#242424), die Seitenflächen liegen eine Stufe darüber
(#282828), die Headerbar noch eine Stufe (#2c2c2c), getrennt durch 1-px-
Hairlines rgba(255,255,255,0.06). Diese **Helligkeits-Hierarchie** wird in die
Hue-Familien der drei Dark-Themes übertragen — nicht die neutralen Grauwerte
selbst. Die Light-Paletten haben die Hierarchie bereits und bleiben unberührt.

## Datei-Ownership (verbindlich, Konfliktschutz)

Dieser Branch darf **nur** anfassen:
`crates/reprise-gnome/src/ui/style/theme.rs`,
`crates/reprise-gnome/src/ui/style/cover_accent.rs`,
`crates/reprise-gnome/src/ui/window/library_chrome.rs`,
`crates/reprise-gnome/src/ui/window/library_shell.rs`,
eigene Testdateien dieser Module, `RELEASING.md` (nur eigener Abschnitt) und
diesen Plan. **Verboten** (gehören dem NPP-Branch): alles unter
`ui/info_panel/` bzw. `ui/now_playing/`, `ui/lyrics/`,
`ui/sidebar/sidebar_presentation.rs`, `style/mod.rs`, `style/tokens.rs`,
`ui/strings*`, `docs/ux-rules.md`, `docs/superpowers/plans/2026-07-18-npp-*`.

## Tasks (strikt in Reihenfolge)

### S1 · Flächenhierarchie der drei Dark-Paletten (Beschluss 7)

- Red: neuer Test in `theme.rs`: für jede Dark-Palette gilt die
  Kanal-Summen-Ordnung `view_bg < sidebar_bg < headerbar_bg` und
  `sidebar_bg < card_bg`; bestehende Tests
  (`dialog_bg_is_distinct_from_card_and_window`, …) bleiben grün.
- Green — neue Dark-Werte (Hue-Familie je Theme beibehalten):

  | Theme | sidebar_bg | headerbar_bg | card_bg | unverändert |
  |---|---|---|---|---|
  | Perpetual Rain | `#22262b` (war `#191c20`) | `#262b31` (war `#16181b`) | `#272d33` (war `#22262b`) | window `#16181b`, view `#1b1e22` |
  | Night Terrain | `#20252f` (war `#161a21`) | `#242a35` (war `#13161c`) | `#252b37` (war `#20252f`) | window `#13161c`, view `#191d25` |
  | Muted Bloom | `#282027` (war `#1d171b`) | `#2c242c` (war `#1a1518`) | `#2d252c` (war `#282027`) | window `#1a1518`, view `#201a1e` |

  Popover/Dialog/Fg/Akzente unverändert. Kommentar im Modulkopf um die
  14a-Hierarchie ergänzen (Tabelle dunkelste Fläche, Panels +1, Headerbar +2).
- Commit: `feat(theme): 14a surface hierarchy for the dark palettes`

### S2 · Hairlines Sidebar ↔ Inhalt und unter der Headerbar (Beschluss 7)

- Green: 1-px-Hairline `rgba(255, 255, 255, 0.06)` an der Kante linke
  Sidebar → Inhalt sowie unter der Headerbar. CSS gehört in
  `library_chrome::css()` (bereits app-weit registriert — **kein** Eintrag in
  `style/mod.rs`). Für sauberes Scoping bekommt die Library-SplitView in
  `library_shell.rs` eine eigene CSS-Klasse (z. B.
  `reprise-library-split`), damit die Regel **nur** die linke Sidebar trifft —
  die rechte Spalte gehört dem NPP-Branch. Feste `rgba(white)`-Hairlines sind
  hier Absicht (siehe Präzedenz in `track_list_header_style.rs`), keine
  Palette-Farbe.
- Verifikation headless (kein Fenster auf dem Desktop!): Xvfb-Screenshot laut
  `TESTING.md`/`scripts/`, Kante sichtbar bei allen drei Themes.
- Red vorab, wo testbar: [gtk] Struktur-Test, dass die SplitView die neue
  Klasse trägt; CSS-Parse-Test über die bestehende `css_parse_errors`-Hilfe.
- Commit: `feat(theme): hairline separators between chrome surfaces`

### S3 · Fallback-Akzent = Theme-Akzent, Petrol (Beschluss 8)

- Red: Test in `theme.rs`: für **alle** Themes (dark + light) gilt
  `player_accent == accent`; kein Vorkommen von `#e8703a`/`#d98a3d`/`#e08a5a`
  mehr im Quelltext (Grep-Assertion oder schlicht durch die Wertänderung
  abgedeckt).
- Green: `player_accent` je Dark-Palette auf den Theme-Akzent setzen
  (Perpetual Rain `#33c9a3`, Night Terrain `#4db6a9`, Muted Bloom `#c98bd0`);
  die Light-Paletten erben via `dark.player_accent` automatisch.
  Palettenkommentar aktualisieren („statischer Fallback = Theme-Akzent,
  Beschluss 8; pro Track überschreibt die Cover-Pipeline"). In
  `cover_accent.rs` Doku/Konstanten/Tests angleichen, die noch von einem
  eigenständigen Petrol-Literal oder vom Orange-Fallback sprechen — eine
  Quelle der Wahrheit: die Theme-Palette.
- Commit: `feat(theme): unify the static accent fallback on the theme accent`

## Gates vor jedem Commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`
- `scripts/check-architecture.sh`
- Display-Tests des berührten Bereichs via `xvfb-run -a` (siehe `TESTING.md`).

## Abnahme (headless-Screenshots, alle drei Dark-Themes)

Linke Sidebar hebt sich sichtbar von der Tabelle ab (heller + Hairline),
Headerbar ist die hellste Chrome-Fläche, Karten bleiben auf den Panels
ablesbar; Play-Button/Waveform fallen ohne Cover-Akzent auf Petrol (bzw. den
jeweiligen Theme-Akzent) zurück, nirgends mehr Orange.
