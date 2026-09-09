---
slug: the-mini-play-button-follows-the-appearance
worktree: /home/marvin/Projects/reprise-the-mini-play-button-follows-the-appearance
branch: feature/the-mini-play-button-follows-the-appearance
phase: shipped
codex_session:
created: 2026-09-09
---
# The mini play button follows the appearance

## Goal

`.mini-player-play` is the last surface in the compact player whose shadows are
hardcoded accent literals instead of appearance-aware tokens. In light it renders
a teal halo that reads as glare on the pale card; in dark it must not move by a
single pixel. Give it its own tokens, the way #891 did for `.player-bar-play` and
#898 for the mini card.

## Why this is its own task

#898 (`c478c685f3`) tokenised the mini card's background, edges and artist
foreground and stopped there, because PLAY-16's scope was misread as covering the
whole button. It covers only the accent fill and the white glyph — never the
glow, ring or shadow.

The halo is **not a regression from #898**. It was identical before; the card
turning pale is what made it visible. Measured in that session's 430×76 capture,
rings around the button centre:

```
r=21  Δ vs card (-23, -11, -12)   ← red falls twice as far: a teal halo
r=24  Δ vs card (-11,  -5,  -6)   ← fading out
r=28  Δ vs card (-35, -34, -34)   ← neutral = the card edge, not glow
```

The shift is chromatic, not neutral — an accent glow, not a shadow.

## Starting point on `origin/dev`

`crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs`, three rules,
every shadow value a literal:

```
.mini-player-play        box-shadow: 0 0 12px alpha(@reprise_player_accent, 0.40);
.mini-player-play:hover  box-shadow: 0 0 18px alpha(@reprise_player_accent, 0.60);
.mini-player-play:active box-shadow: 0 0 0 3px alpha(@reprise_player_accent, 0.45),
                                     0 0 18px alpha(@reprise_player_accent, 0.70);
```

The big player resolves everything but its press ring through tokens whose light
branch is zeroed (`PLAY_GLOW_NEAR_LIGHT_ALPHA`, `PLAY_GLOW_FAR_LIGHT_ALPHA` are
both `"0"`) and compensates with a ring (`PLAY_RING_LIGHT_ALPHA = "0.12"`) and a
drop (`PLAY_DROP_LIGHT_ALPHA = "0.18"`).

---

# Part 1 — what Codex implements

Four Rust files and one new test. **Nothing in Part 2 is Codex's task**; the
visual proof is run afterwards, outside the worktree, by the session driving the
pipeline.

## 1. Two new mini-owned glow tokens

Not the big player's `@reprise_play_glow_near/_far` — those are dark 0.60/0.35
while mini's rest glow is 0.40, so adopting them would move dark. Mini gets its
own pair, each dark branch reproducing the literal it replaces byte for byte.

| Token | dark | light |
| --- | --- | --- |
| `reprise_mini_play_glow` | `alpha(@reprise_player_accent, 0.40)` | `alpha(@reprise_player_accent, 0)` |
| `reprise_mini_play_glow_hover` | `alpha(@reprise_player_accent, 0.60)` | `alpha(@reprise_player_accent, 0)` |

`alpha(…, 0)` is the exact form the light branch already ships for the big
player's four glow tokens, so it is known to parse in GTK4's CSS parser.

## 2. `@reprise_play_ring` is adopted, not minted

Zeroing the glow alone would leave a flat accent circle with no edge on a pale
card — the subtraction without the compensation. `PLAY_RING_LIGHT_ALPHA`'s own
doc comment names this case: *"The transparent dark twin leaves no edge around
the accent circle on a pale background."*

The existing token is consumed directly, with **no new token and no dark change**,
because it is `alpha(@window_fg_color, {select(0, 0.12)})` — alpha 0 in dark, and
a fully transparent shadow layer contributes nothing when composited. This is not
a deduction: `.player-bar-play` has rendered `inset 0 0 0 1px @reprise_play_ring`
in dark since #891.

Its definition is already pinned by
`dark_appearance_tokens_reproduce_every_replaced_literal`, so adopting it needs
no test change.

## 3. The ring is repeated in all three states

`box-shadow` is not additive across pseudo-classes — `:hover` and `:active`
replace the whole list. A ring declared only at rest vanishes the moment the
pointer arrives, in light exactly where it is needed. `.player-bar-play` repeats
`inset 0 0 0 1px @reprise_play_ring` in all three rules for this reason.

**This is the only way `:active` is touched.** Its two accent layers
(`0 0 0 3px … 0.45`, `0 0 18px … 0.70`) stay hardcoded and byte-identical — the
same reasoning `player_bar_layout.rs` records for its own press ring: the pulse
is momentary, and in light `@reprise_player_accent` already resolves to the
darkened accent, so it reads as a dark ring rather than the glare the glow tokens
were zeroed to avoid. Only the ring layer is prepended.

## Target CSS

```
.mini-player-play {
  … background-color: @reprise_player_accent; color: #ffffff;
  box-shadow: inset 0 0 0 1px @reprise_play_ring,
              0 0 12px @reprise_mini_play_glow;
  transition: …; }
.mini-player-play:hover {
  box-shadow: inset 0 0 0 1px @reprise_play_ring,
              0 0 18px @reprise_mini_play_glow_hover; }
.mini-player-play:active {
  box-shadow: inset 0 0 0 1px @reprise_play_ring,
              0 0 0 3px alpha(@reprise_player_accent, 0.45),
              0 0 18px alpha(@reprise_player_accent, 0.70); }
```

## File by file

### `crates/reprise-gnome/src/ui/style/tokens.rs`

Four `&'static str` constants beside the existing `PLAY_*` block, in the
established doc-comment voice — the dark ones name the literal they preserve, the
light ones say why the dark twin is glare:

```rust
MINI_PLAY_GLOW_DARK_ALPHA        = "0.40"
MINI_PLAY_GLOW_LIGHT_ALPHA       = "0"
MINI_PLAY_GLOW_HOVER_DARK_ALPHA  = "0.60"
MINI_PLAY_GLOW_HOVER_LIGHT_ALPHA = "0"
```

### `crates/reprise-gnome/src/ui/style/theme_tokens.rs`

Two `String` fields on `ThemeTokens` — `mini_play_glow`, `mini_play_glow_hover` —
placed after `mini_artist_fg` so the mini surface's tokens stay contiguous, each
built as `format!("alpha(@reprise_player_accent, {})", select(dark, light))`.

Two new entries in `dark_appearance_tokens_reproduce_every_replaced_literal`:

```
"@define-color reprise_mini_play_glow alpha(@reprise_player_accent, 0.40);",
"@define-color reprise_mini_play_glow_hover alpha(@reprise_player_accent, 0.60);",
```

### `crates/reprise-gnome/src/ui/style/theme.rs`

Two `@define-color` lines in the format string after `reprise_mini_artist_fg`,
plus their two `name = appearance.field` bindings in the argument list.

### `crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs`

The three `box-shadow` declarations above. Extend the existing PLAY-16 comment to
record that ring and glow now resolve through tokens while fill and glyph stay
exempt, and that the ring is repeated per state because `box-shadow` is not
additive.

One new test beside the existing `mini_*` tests, asserting the CSS text:

- all three states contain `inset 0 0 0 1px @reprise_play_ring`;
- rest contains `0 0 12px @reprise_mini_play_glow`;
- hover contains `0 0 18px @reprise_mini_play_glow_hover`;
- `alpha(@reprise_player_accent, 0.40)` and `alpha(@reprise_player_accent, 0.60)`
  no longer occur anywhere in `mini_css()`;
- `alpha(@reprise_player_accent, 0.45)` and `alpha(@reprise_player_accent, 0.70)`
  still do — the press ring is deliberately literal.

## Deliberately out of scope

**No drop shadow.** The big player's light branch keeps
`0 6px 12px @reprise_play_drop` (0.18); mini gets no equivalent. Two reasons, in
order of weight:

1. *The mini surface has no shadows by construction.* The card carries
   `box-shadow: none` and so does the toplevel, both added deliberately under
   MINI-1/MINI-2 to kill Adwaita's CSD halo. An elevation shadow inside a surface
   whose design brief was "no shadows" contradicts its own language.
2. *There is almost no room.* `CARD_MARGIN` is **0**, so the toplevel is exactly
   the card and clips anything outside it. With `padding: 10px …` and a 38 px
   button centred in the 56 px content box, a `0 6px 12px` drop reaches y=75 in a
   76 px window — one pixel of clearance, and clipped if it renders at all.

Minting `reprise_mini_play_drop` with dark alpha `0` stays possible later if the
light button measures as too flat. It is not in this cut.

**PLAY-16 stays untouched.** Accent fill and white glyph are unchanged;
`play_16_the_play_buttons_keep_the_playback_accent_and_white_glyph` must stay
green without edits.

**`panel_contrast.rs` is not modified.** Its PLAY-16 exemption strips the whole
`.mini-player-play` rule body before the fixed-foreground sweep, and both of its
assertions (accent background, `color: #ffffff`) survive this change verbatim.

**No capture script.** The measurement harness is not committed and is not
Codex's work — see Part 2.

## Static proof Codex must leave green

1. `dark_appearance_tokens_reproduce_every_replaced_literal` — 3 themes × 2 accent
   sources × the two new definitions. This is the dark-identity guarantee.
2. The new `mini_css` assertion above.
3. `play_16_…` and `panel_contrast`'s fixed-foreground sweep — green, unedited.
4. `generated_light_theme_css_parses_for_every_theme_and_accent_source` is
   `#[ignore]`d and needs a display; run it under `xvfb-run` so the two new
   `@define-color` lines are proven against GTK4's real parser, not only against
   a string assertion.

---

# Part 2 — verification run outside the worktree

Not Codex's task. Run by the pipeline session after the code phase, using the
Xvfb recipe recorded in
`docs/plans/HANDOVER-2026-09-09-mini-player-appearance.md` (openbox — without a
WM the window never maps; `GDK_BACKEND=x11`, `WAYLAND_DISPLAY` unset, the full
`XDG_DATA_HOME`/`CACHE`/`CONFIG` triple; `xdotool windowactivate --sync` *before*
`xdotool key ctrl+m`; `import -window`, PNG never JPG).

**Seed the library; do not play anything.** `now_playing_wiring.rs:445` derives
`play_available = queue_has_tracks || library_has_tracks`, and
`compact_player.rs:264` sets the button insensitive when it is false — an empty
throwaway profile therefore renders a *dimmed* button and measures the wrong
state. With tracks in the library and no playback the button is sensitive, the
waveform carries no position, and the window is deterministic across runs. This
also drops the whole D-Bus `PlayTrackIds` + MPRIS `Play` + 30 s fixture chain the
#898 session needed for its label-contrast measurement.

### Compare a crop, not the full frame

Derived from the constants, not measured off a screenshot — `MINI_WIDTH` 430,
`MINI_HEIGHT` 76, card padding `10px 14px 10px 10px`, `PLAY_SIZE` 38,
`INNER_SPACING` 13, `CARD_MARGIN` 0 (so the window *is* the card):

```
button   x 378..416   (430 − 14 right padding − 38)
         y  19..57    (10 top padding + (56 − 38)/2)
hover glow (18px)     x 360..434 → clipped at 430,  y 1..75
text column ends at   x 365
```

**Crop `x=[360,430], y=[0,76]`** — 70×76. It holds the button, the full hover
glow falloff, and the card's right edge as a reference. It cannot also exclude
the waveform: the glow reaches 5 px past the text column's right edge at x=365.
That is fine here and would not have been with a playing waveform — the seeded,
non-playing scene has no animated pixels — but it must be *shown*, not assumed.

Comparing this crop rather than the full frame is the stricter test: every pixel
in it is one the change can plausibly affect, where a full frame dilutes the
signal with the cover, the labels and hundreds of pixels of flat card.

### Harness self-test first

Before any before/after comparison, take **two before-captures** in the same
appearance and confirm they are already byte-identical to each other. This is the
step that keeps a false negative from being read as a real finding.

It empirically validates every assumption the crop rests on at once: that the
seeded track is identical across runs (cover placeholder, labels), that the
static waveform sliver at x=360..365 does not vary, that the volume-bar overlay
at `y=0..3` composites to nothing at `set_opacity(0.0)`, and that the accent
source resolves the same way under Xvfb both times. If the two before-captures
differ, the harness is broken — fix that before drawing any conclusion about the
ring. `scripts/ptr-e2e/harness-self-test.sh` is the local precedent for the idea.

### The captures

1. **Dark, rest — before and after.** Byte-identical crops. This is the
   load-bearing proof: the ring is alpha 0 in dark and must therefore be free.
2. **Dark, hover — before and after.** Same bar. `xdotool mousemove` onto the
   button, 300 ms settle (`TRANSITION` is 150 ms, `motion.rs:18`), then capture.
3. **Light, rest and hover — after.** Sample the same rings (r=21/24/28) around
   the button centre that found the halo; all three sit well inside the crop. The
   chromatic delta at r=21 must be gone, and a neutral edge at the button
   boundary is the ring doing its job.

State the measured numbers. "Looks unchanged" is not the bar — a byte comparison
either holds or it does not.

If a dark pair differs **after the self-test passed**, the ring is not free after
all: the fix is then a mini-owned zero-alpha ring token instead of adopting
`@reprise_play_ring`, and that is a plan change, not a patch. If the self-test
itself fails, the arm has proven nothing in either direction.

---

## Facts established while planning — do not re-derive

- Both play buttons carry Adwaita's `circular` class
  (`compact_player_layouts.rs:116`, `player_bar_layout.rs:191`), so the inset ring
  traces the circle through the same mechanism on both. The 38 px vs 44 px size
  gap does not affect the ring; it is why the big player's inset *shaping* layers
  (`inset 0 2px 1px`, `inset 0 -4px 3px`) are deliberately not adopted.
- The mini card packs as `[cover | text_col(meta_row + waveform) | play]`
  (`compact_player_layouts.rs:104–127`) — the button is rightmost, the waveform
  sits to its left. Relevant because a playing waveform is what would make a
  full-frame byte comparison impossible.
- `rule_body` in `panel_contrast.rs:104` splits on the first occurrence of the
  selector, which is the rest rule. The PLAY-16 exemption therefore keeps working
  unchanged.
- `dark_appearance_tokens_reproduce_every_replaced_literal` pins `@define-color`
  lines in the generated theme CSS, not the mini selector's output. No test pins
  mini's `box-shadow` today — the dark-identity guarantee for this change comes
  from the two new pinned definitions plus the Part 2 control arm, not from an
  existing assertion.

## Risks

- **The ring is not free.** Caught by proof 1; fallback is a mini-owned
  zero-alpha ring token. Cheap to detect, cheap to correct.
- **Seeding leaves the button disabled anyway.** Confirm in the capture that the
  glyph is not dimmed before trusting any measurement; a dimmed button invalidates
  the whole arm silently.
- **Hover capture races the transition.** 300 ms settle against a 150 ms
  transition; if the two dark hover frames differ from each other in the *button
  interior*, the settle was too short, not the change wrong.
- **A false negative read as a finding.** The dark arm failing for a reason
  unrelated to the ring — a different seeded track, a varying accent source, the
  volume-bar overlay not compositing to nothing — would send the work to the
  fallback token for nothing. The two-before-captures self-test exists precisely
  to catch that, and no dark comparison counts until it has passed.

## Parallelität

**No cut. Single strand.**

The four files form a strict dependency chain, not disjoint groups: `tokens.rs`
must declare the constants before `theme_tokens.rs` can select them,
`theme_tokens.rs` must own the struct fields before `theme.rs` can bind them into
the format string, and `theme.rs` must emit the `@define-color` lines before
`compact_player_layouts.rs` may reference them. No intermediate state compiles.
Splitting along the obvious seam ("tokens" vs. "CSS") produces two strands that
both edit `compact_player_layouts.rs`.

Splitting by concern — glow tokens as one strand, ring adoption as the other —
fails disjointness on the same file: both write the same three `box-shadow`
declarations.

The one file group that *would* have been disjoint, a committed capture script,
is deliberately not in the cut (Part 2 is run ad hoc, not committed), so it
cannot form a second strand either.

The task is roughly 40 lines across 4 files plus one test. There is no wall-clock
to win; a cut would buy coordination cost and nothing else.

**Merge order:** n/a.

**Post-merge cross-checks:** n/a. Every verification step reads only files this
strand owns. The dark control arm compares two commits of the *same* worktree,
not two strands, so it is a within-strand check and does not move.
