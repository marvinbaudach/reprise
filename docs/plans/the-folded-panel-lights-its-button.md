---
slug: the-folded-panel-lights-its-button
worktree: /home/marvin/Projects/reprise-the-folded-panel-lights-its-button
branch: feature/the-folded-panel-lights-its-button
phase: reviewed
codex_session:
created: 2026-09-09
---
# The folded panel lights its button

## Goal

The two panel-collapse toggles — the sidebar toggle at the top left and the
Now Playing panel toggle at the top right — carry their accent highlight while
their panel is **folded away**, and are neutral while it is open. Today it is
the other way round for the Now Playing toggle, and the sidebar toggle carries
no highlight in either state.

The search toggle and the navigation back button hang on the same
`.reprise-panel-toggle` class and must not change at all.

## Base

`origin/dev` @ `c19ae351c6`. **Not** the checkout this plan was written in: that
tree is older than `abb205cef9` (#891, "The light appearance gets its own
edges"), the commit that introduced the de-coloured sidebar toggle. Branching
from there would silently re-open #891's diff. `worktree.sh` already bases new
branches on `origin/dev`; do not override it.

## Decisions taken in the grill

1. **Both toggles invert.** The Now Playing toggle loses today's lit-when-open
   look; open is the neutral state for both.
2. **The paint is exactly today's**, only moved to the other state:
   `alpha(@accent_bg_color, HOVER_BG_ALPHA)` plus `@reprise_accent_text_color`.
   No new value, therefore no new contrast proof needed — it is the same paint
   the user can see working in their own light-mode screenshot.
3. **One rule regardless of who folded the panel.** At constrained widths
   `responsive_side_panels.rs` folds panels on its own; both buttons then sit
   highlighted with no user action. Accepted: the highlight means "a panel is
   hidden", not "you hid a panel".
4. **Colour is the only signal.** No dot, unlike `.reprise-btn-toggle`. The
   hidden panel is itself the louder non-colour signal. Record the reasoning as
   a comment in the CSS block so a later review does not read the omission as an
   oversight.

## What is actually there today

Four widgets carry `.reprise-panel-toggle`:

| Widget | Built at | Type | `:checked` means |
|---|---|---|---|
| Sidebar toggle | `ui/window/window_header.rs:37` | `ToggleButton`, `+ reprise-sidebar-toggle` from `ui/window/window_navigation.rs:171` | sidebar **shown** |
| Now Playing toggle | `ui/now_playing/now_playing.rs:374` | `ToggleButton`, `.active(visible)` | panel **shown** |
| Search toggle | `ui/window/library_chrome.rs:43` | `ToggleButton` | search active — **unchanged** |
| Back button | `ui/window/library_chrome.rs:135` | plain `Button` — never `:checked` | — |

So "folded away" is exactly `:not(:checked)` on the first two, and the change
must be scoped to those two only.

Two CSS sources are in play, both concatenated into the **appearance-independent**
`app_css()` (`ui/style/mod.rs:104` `buttons::css()`, then `:106`
`interactions::css()` — later in the string, so equal-specificity ties go to
`interactions`):

- `ui/style/interactions.rs:39-45` — the generic accent state, which is what
  lights the Now Playing toggle today:
  ```
  .reprise-panel-toggle:checked        { color: @reprise_accent_text_color;
                                         background-color: alpha(@accent_bg_color, HOVER_BG_ALPHA); }
  .reprise-panel-toggle:checked:hover  { background-color: alpha(@accent_bg_color, HOVER_BG_ALPHA_STRONG); }
  ```
- `ui/style/buttons.rs:219-224` — #891's sidebar exception, which is why the
  left button is neutral in both appearances:
  ```
  .reprise-panel-toggle.reprise-sidebar-toggle:checked        { background-color: transparent; background-image: none; color: inherit; }
  .reprise-panel-toggle.reprise-sidebar-toggle:checked:hover  { background-color: alpha(currentColor, BTN_HOVER_ALPHA); }
  .reprise-panel-toggle.reprise-sidebar-toggle:checked:active { background-color: alpha(currentColor, BTN_PRESS_ALPHA); }
  ```

`buttons.rs` is in `app_css()`, which never sees `is_dark` — that is *why* the
reported symptom is identical in both appearances. This change keeps that
property: **no `is_dark` branch belongs anywhere in this diff.**

**This does not revert #891.** Its decision was "the checked sidebar toggle is
neutral"; that survives verbatim. What is added is the unchecked state, which
#891 never addressed.

## The mechanism

One shared class, `reprise-collapse-toggle`, on the two collapse toggles, with
the inverted state pair hung on it. The class carries the *meaning* ("this
button folds a panel away"), which `reprise-sidebar-toggle` cannot — that one
exists for the window-level Space-key routing (`ui/shortcuts.rs:56`) and is
reused for CSS only by accident of history.

Specificity is what makes this safe without touching the generic rule:

| Selector | Specificity | Beats |
|---|---|---|
| `.reprise-panel-toggle` | 0,1,0 | — |
| `.reprise-panel-toggle:checked` (generic, interactions) | 0,2,0 | — |
| `.reprise-panel-toggle.reprise-collapse-toggle` | 0,2,0 | matches every state; the more specific checked-state rules below win whenever the toggle is checked |
| `.reprise-panel-toggle.reprise-collapse-toggle:checked` | 0,3,0 | the generic `:checked`, **regardless of source order** |
| `.reprise-panel-toggle.reprise-collapse-toggle:checked:hover` | 0,4,0 | the generic `:checked:hover` (0,3,0) |

The generic `.reprise-panel-toggle:checked` rule therefore stays untouched and
keeps serving the search toggle — the only remaining widget that is both a
`ToggleButton` and not a collapse toggle. That is a deliberate non-change.

### The rules

```
.reprise-panel-toggle.reprise-collapse-toggle
        { color: @reprise_accent_text_color;
          background-color: alpha(@accent_bg_color, HOVER_BG_ALPHA); }               /* 0.10 */
.reprise-panel-toggle.reprise-collapse-toggle:hover
        { background-color: alpha(@accent_bg_color, HOVER_BG_ALPHA_STRONG); }        /* 0.18 */
.reprise-panel-toggle.reprise-collapse-toggle:active
        { background-color: alpha(@accent_bg_color, BTN_CHECKED_FILL_PRESS_ALPHA); } /* 0.26 */
.reprise-panel-toggle.reprise-collapse-toggle:checked
        { background-color: transparent; background-image: none; color: inherit; }
.reprise-panel-toggle.reprise-collapse-toggle:checked:hover
        { background-color: alpha(currentColor, BTN_HOVER_ALPHA); }                  /* 0.08 */
.reprise-panel-toggle.reprise-collapse-toggle:checked:active
        { background-color: alpha(currentColor, BTN_PRESS_ALPHA); }                  /* 0.14 */
```

The `:active` line for the **unchecked** state is load-bearing, not symmetry for
its own sake: Reprise's app CSS runs at
`STYLE_PROVIDER_PRIORITY_APPLICATION`, so it beats Adwaita's theme CSS
regardless of selector specificity. Without an explicit press state, the base
rule's resting `HOVER_BG_ALPHA` fill (0.10) would remain in effect instead of
changing to `BTN_CHECKED_FILL_PRESS_ALPHA` (0.26), and the highlighted button
would swallow its own click feedback.

## Tasks

1. **Add the class constant.** `pub(in crate::ui) const COLLAPSE_TOGGLE_CSS_CLASS:
   &str = "reprise-collapse-toggle";` next to the other class constants in
   `ui/style/buttons.rs`, with a doc comment naming its meaning and the two
   widgets that carry it.
2. **Attach it to both collapse toggles**: `ui/window/window_header.rs:40` and
   `ui/now_playing/now_playing.rs:377`, in the `css_classes` builder call, via
   the constant — not a string literal.
3. **Replace the sidebar exception block** (`ui/style/buttons.rs:219-224`) with
   the six rules above, addressed by `COLLAPSE_TOGGLE_CSS_CLASS`. The three
   `reprise-sidebar-toggle` CSS rules go away entirely; the constant in
   `ui/shortcuts.rs` stays, because the Space-key routing still uses it. Carry
   decision 4's reasoning into the block as a comment, in the style of the
   surrounding `BTN-n` comments.
4. **Leave `ui/style/interactions.rs` functionally alone.** Add one assertion to
   its existing test that the generic `:checked` accent rule is still present —
   it is now the search toggle's rule and nothing else's.
5. **Update the three sidebar-toggle tests** in `ui/style/buttons.rs`:
   - `sidebar_toggle_checked_state_keeps_no_mode_slab` → retarget to the collapse
     selector; the assertions themselves survive unchanged.
   - `sidebar_toggle_carries_no_accent_in_any_checked_state` → same retarget.
   - `sidebar_toggle_checked_state_renders_no_mode_slab` (display-gated) → the
     assertion **inverts** to `assert_ne!(checked_background, unchecked_background)`
     for a button carrying the collapse class, because the unchecked state is now
     the painted one. Keep the `.reprise-btn-toggle` control arm exactly as it is;
     it is what proves the sampler can see a fill at all. Rename all three tests
     to say what they now prove.
6. **New test — the search toggle does not follow.** Assert on the built CSS that
   every collapse rule carries **both** classes, and that a bare
   `.reprise-panel-toggle:checked` accent rule still exists. This is the
   regression guard for the one widget the user explicitly excluded.
7. **New test — both production widgets carry the class.** Assert against the
   real constructors (`window_header::build()` and the Now Playing panel's
   toggle), not a hand-built button, so a future refactor that drops the class
   fails here. Display-gated if GTK must be initialised.

## Verification fence

This change touches `crates/reprise-gnome/src/ui/` only.

- `cargo fmt`
- `cargo clippy -p reprise-gnome --all-targets -- -D warnings`
- `cargo test -p reprise-gnome`
- `scripts/check-gnome-idioms.sh`
- the inverted display-gated test from task 5, once under `xvfb-run` — it is the
  only check that looks at real pixels, and the reported bug was visual.

`cargo test -p reprise-gnome --lib` **does not exist** — the package has no
library target. Do **not** run a workspace-wide build or test, `cargo audit`,
`gradlew`, `uniffi-bindgen`, the Android suite, or any repo-wide gate script.
AGENTS.md's repo-wide gate instruction is **explicitly overridden** for this
task; without saying so, a run burns half an hour on gates a CSS class cannot
affect.

## Acceptance

- Sidebar folded away → left toggle tinted; sidebar open → left toggle neutral.
- Now Playing panel closed → right toggle tinted; open → neutral.
- Search toggle: tinted while search is active, exactly as today.
- Back button: unchanged.
- `git diff` contains no `is_dark`, no `StyleManager`, and no new colour literal.

## Known consequence, accepted

At constrained window widths `responsive_side_panels.rs` folds the panels
itself. Both toggles then sit highlighted without the user having done anything.
That is the honest reading of "highlight means something is hidden" and was
accepted in the grill; it is recorded here so it is not later mistaken for a bug.

## Parallelität

**No cut. One strand.**

The tasks form a single dependency chain through three files. Task 1 defines the
constant that tasks 2 and 3 consume; task 3 rewrites the very CSS block whose
tests tasks 5–7 rewrite, in the same file (`ui/style/buttons.rs`). Any cut would
put the constant's definition in one strand and every use of it in another — the
second strand would not compile before the merge, which is precisely the failure
mode this section exists to prevent.

The whole change is well under a hundred lines. Splitting it would buy no
wall-clock and cost a merge.

**Post-merge cross-checks:** none — there is no seam.
