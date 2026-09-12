# The now-playing panel settles — design

Source: Claude Design project "Player cover animation variants", file
`Cover Varianten.dc.html`, section `turn-4`, variant **4b "Optimiert"**
(4a is the current state, re-staged from a screenshot). The mockup is drawn
for a 430 px panel; the real panel is `now_playing_column::PANEL_WIDTH` =
**300 logical px** (the screenshot was taken at ~1.67× scale). Decision: every
length in the mockup is scaled by 300/430 ≈ 0.70 and rounded to the nearest
even pixel; typography keeps today's tokens.

Scope: the GNOME right-hand panel only. Android's `NowPlayingScene.kt` has its
own numbers and is not touched. Lands **after** the 0.1.183/0.1.184 promotion
that is in flight, as its own feature branch.

## What changes, top to bottom

### Background — the clouds move above the blur

Today the artwork band stacks, bottom to top: drifting clouds → cover bloom
(blurred, enlarged cover, breathing with the bass) → cover. On a real cover
the bloom hides the clouds, and the band reads as one still blur.

- Order becomes **bloom → clouds → cover**. The bloom stays the audio-coupled
  layer and keeps its breath; it is capped so that rest + pressure + swell
  never exceeds **0.35** in dark (light keeps its ratio: cap ≈ 0.60).
- The clouds keep their colour source (cut from the blurred cover, never a
  palette) and take the mockup's strength: blob alphas back 0.85/0.80, front
  0.70/0.60; drift path X −0.20…0.16, Y −0.12…0.12 of the field, scale
  1.20…1.55, rotation 0…10°, periods 16 s / 20 s reverse with the 10 s offset.
  (The drift and alpha numbers land separately as "The clouds drift a longer
  path"; this spec only fixes the stacking and relies on them.)
- **Vertical fade** into the panel colour (`@sidebar_bg_color`): 0 % at the top
  of the band, **15 % at two thirds of the way down to the title**, **100 % at
  the title's top edge** — the title stands entirely on calm ground. In code
  the scrim stops are expressed relative to the head geometry
  (`top + cover + gap`), not to the field's arbitrary height.
- **Horizontal fade on the left**: panel colour at 100 % on the panel's left
  edge → transparent at **22 % of the panel width** (66 px), so no colour
  bleeds into the track list beside it. Same colour token, both themes.
- Both fades are drawn by the cloud layer's scrim (one owner), above the
  bloom and the clouds, below the cover.

### Cover

- Size **184 px** (today 168), top spacing **50 px** (today 22).
- **No hairline**: the `inset 0 0 0 1px @reprise_cover_edge` ring goes.
- Radius stays `RADIUS_SURFACE` (12 px); the picture itself is clipped to it
  (already true for the `GtkImage`, must stay true if the widget changes).
- Shadow **`0 12px 30px rgba(0,0,0,.42)`** in both themes.

### Title block

- Gap cover → title **34 px** (today 12 + band remainder ≈ 150 in the mockup's
  reading of the screenshot).
- No box, no backdrop, no shadow behind the text. Whatever paints the
  translucent "readout" ground behind the three lines today is removed.
- Title: today's token (15 px bold), ellipsized end, wrapping off. Two lines
  become one: **"Artist · Album"** at the subtitle token (12 px), artist at
  the primary-secondary tone (70 % of fg, weight 500). In dark, the separator
  and album use the same 70 % alpha because the glow leaves no lower tone above
  the 4.5:1 floor; weight 400 and the separator carry their quieter role. In
  light they retain a 65 % tonal step. The line ellipsizes at the end; the
  artist keeps priority (album is what gets cut first).
- Side padding **18 px** (unchanged), centred.

### Segment control (queue / lyrics / visualizer)

- Height **30 px** (today 50), outer radius **7 px**, inner **5 px** (today a
  99 px pill), padding **2 px**, gap **2 px**.
- Width: full panel minus 18 px margins (262 px). Spacing above **16 px**.

### Transition to the track list

- **20 px** below the segment control a **1 px** separator in the theme's
  border tone (`@borders`), running out to transparent over **34 px** at both
  ends (a horizontal gradient, not a widget with margins).
- **8 px** from the separator to the first row.

### General

- No motion except the clouds and the bloom's breath. Reduce-animation is
  already honoured by both (`DriftClock::hold`, the bloom's pinned tick) — the
  new layers add no timers.
- Every value holds in dark and light; only the panel/fade colour changes with
  the theme, through the named colour, never a literal.

## Rules that move with it

- `docs/ux-rules.md` **NPP-2** currently reads "cover 168 px (radius 12,
  shadow + 1 px inset hairline) → title 15 px bold → „Artist · Album" 12 px
  white 55 % → pill toggle". It becomes: cover 184 px (radius 12, shadow, no
  hairline), 50 px top, 34 px to the title, title 15 px bold, "Artist · Album"
  one line, segment control 30 px / 7 px radius, hairline-with-run-out before
  the list. The "Artist · Album" one-liner the rule already asks for is what
  the code will finally do.
- Tokens gain what they lack: `NOW_PLAYING_COVER_SIZE` 184,
  `NOW_PLAYING_HEAD_TOP` 50, `NOW_PLAYING_COVER_TO_TITLE` 34,
  `NOW_PLAYING_SEGMENT_HEIGHT` 30, `NOW_PLAYING_SEGMENT_RADIUS` 7,
  `NOW_PLAYING_LIST_RULE_RUN_OUT` 34. Cloud/bloom caps stay in their modules.

## Verification

- Display tests under Xvfb, isolated XDG: the head geometry (cover top edge at
  50, title top at 50 + 184 + 34), the segment control's allocated height, the
  rule's presence and height, both fades' alpha at three probe points
  (top-left corner = panel colour, band centre = transparent, title top edge =
  panel colour), and the overlay order (bloom below clouds below cover) — each
  with a control arm that fails when the change is reverted.
- The bloom cap: a pure test on `bloom_opacity` at pressure 1, swell 1.
- Screenshot pair dark/light from the harness for the eye, not as the proof.
