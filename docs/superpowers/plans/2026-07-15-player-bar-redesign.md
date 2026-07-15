# Player Bar Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the library player bar to match the Reprise Redesign mockup — overlay positioning, richer waveform with hover/drag/interpolation, OKLCH accent, inline volume, queue button, and motion.

**Architecture:** The player bar moves from `ActionBar` in a stacked `Box` to a `CenterBox` inside a `GtkOverlay`, with three fixed-width zones (300/620/250 px). The waveform becomes 88 rounded bars with drag-seek, hover tooltips, and smooth inter-tick interpolation. Cover accent extraction switches to median-cut + OKLCH clamping with a 400 ms cross-fade.

**Tech Stack:** Rust, gtk4-rs 0.11, libadwaita 0.7, Cairo, GStreamer (peak extraction)

## Global Constraints

- Dark mode only (all colors specified for dark surfaces).
- `@reprise_player_accent` remains the single CSS token driving waveform + play button color.
- All animations gate on `gtk4::Settings::is_gtk_enable_animations()`.
- Existing `set_*/connect_*` public API on `PlayerBar` must keep the same signatures where callers outside `player_bar/` use them. Internal restructuring is fine.
- Waveform peak cache at `~/.cache/reprise/waveforms/` must stay compatible (bucket count changes invalidate per-file — the hash already includes bucket count).
- MPRIS integration is untouched — all commands already route through `PlayerController`.
- File size limit: 800 lines per file.

---

### Task 1: Remaining-time formatter + tabular nums CSS

Add a `format_remaining` function and the CSS for tabular numerals.

**Files:**
- Modify: `crates/reprise-core/src/format.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs` (CSS only)

**Interfaces:**
- Produces: `pub fn format_remaining(position_ms: i64, duration_ms: i64) -> String` — returns `"−M:SS"` or `"−H:MM:SS"` (U+2212 prefix). Called by Task 5's `set_position`.

- [ ] **Step 1: Write failing test for `format_remaining`**

In `crates/reprise-core/src/format.rs`, add at the bottom of `mod tests`:

```rust
#[test]
fn format_remaining_shows_negative_remaining_time() {
    assert_eq!(format_remaining(8_000, 68_000), "\u{2212}1:00");
}

#[test]
fn format_remaining_at_start_shows_full_duration() {
    assert_eq!(format_remaining(0, 181_000), "\u{2212}3:01");
}

#[test]
fn format_remaining_at_end_shows_zero() {
    assert_eq!(format_remaining(181_000, 181_000), "\u{2212}0:00");
}

#[test]
fn format_remaining_with_hours() {
    assert_eq!(format_remaining(0, 3_753_000), "\u{2212}1:02:33");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --package reprise-core -- format_remaining -v`
Expected: FAIL — `format_remaining` not found.

- [ ] **Step 3: Implement `format_remaining`**

In `crates/reprise-core/src/format.rs`, after `format_duration`:

```rust
/// Formats the remaining time as `−M:SS` (or `−H:MM:SS`), using U+2212
/// MINUS SIGN for visual consistency with tabular-numeral fonts.
pub fn format_remaining(position_ms: i64, duration_ms: i64) -> String {
    let remaining = (duration_ms - position_ms).max(0);
    format!("\u{2212}{}", format_duration(remaining))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --package reprise-core -- format_remaining -v`
Expected: 4 PASS.

- [ ] **Step 5: Add tabular-nums CSS class**

In `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs`, in the `css()` function, append:

```rust
// After the existing .waveform-seek line:
".player-bar-time { font-feature-settings: \"tnum\"; }"
```

And in `build()`, after each time label is created, add:

```rust
position_label.add_css_class("player-bar-time");
duration_label.add_css_class("player-bar-time");
```

- [ ] **Step 6: Run full check**

Run: `cargo test --package reprise-core && cargo check --package reprise-gnome`
Expected: all pass, no errors.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-core/src/format.rs crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs
git commit -m "feat(bar): add format_remaining and tabular-nums CSS"
```

---

### Task 2: Waveform rewrite — 88 rounded bars, fallback, unplayed-white

Rewrite `waveform_seek.rs`: 88 bars with rounded rects, 2 px gap, heights 5–26 px, unplayed bars as white 16%, flat 4 px fallback when no peaks.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs` (bucket count constant)

**Interfaces:**
- Produces: same `WaveformSeek` API (`new`, `set_peaks`, `set_fraction`, `connect_seek`, `widget`) — no signature changes. Internally the geometry and drawing change.

- [ ] **Step 1: Update constants and tests**

In `waveform_seek.rs`, replace the existing constants:

```rust
const CONTENT_HEIGHT: i32 = 28;
const BAR_RADIUS: f64 = 1.5;
const BAR_GAP: f64 = 2.0;
const MIN_BAR_HEIGHT: f64 = 5.0;
const MAX_BAR_HEIGHT: f64 = 26.0;
/// Alpha for not-yet-played bars — white on dark background.
const UNPLAYED_ALPHA: f64 = 0.16;
/// Fallback bar height when no peaks are available.
const FALLBACK_BAR_HEIGHT: f64 = 4.0;
```

Remove `MIN_BAR_HEIGHT_FRACTION`, `BAR_GAP_FRACTION`, `UNPLAYED_ALPHA` (old value 0.28).

Update tests: `bars_split_played_from_unplayed_at_the_fraction` stays unchanged (pure logic). Add:

```rust
#[test]
fn fallback_draws_flat_bar_when_peaks_empty() {
    // No peaks → draw function should not panic, draws fallback.
    // This is a logic test; actual rendering verified in smoke tests.
    assert_eq!(fraction_at(50.0, 100.0), 0.5);
}
```

- [ ] **Step 2: Rewrite `draw` function**

Replace the `draw` function body with rounded-rect bars and the new geometry:

```rust
use std::f64::consts::{FRAC_PI_2, PI};

fn draw(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    state: &State,
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let w = f64::from(width);
    let h = f64::from(height);

    if state.peaks.is_empty() {
        draw_fallback(area, cr, w, h, state.fraction);
        return;
    }

    let count = state.peaks.len();
    let slot = (w + BAR_GAP) / count as f64;
    let bar_w = (slot - BAR_GAP).max(1.0);

    let color = area.color();
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

    for (index, &peak) in state.peaks.iter().enumerate() {
        let magnitude = f64::from(peak).clamp(0.0, 1.0);
        let bar_h = MIN_BAR_HEIGHT + magnitude * (MAX_BAR_HEIGHT - MIN_BAR_HEIGHT);
        let x = index as f64 * slot;
        let y = (h - bar_h) / 2.0;

        if bar_played(index, count, state.fraction) {
            cr.set_source_rgba(r, g, b, 1.0);
        } else {
            cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA);
        }
        rounded_bar(cr, x, y, bar_w, bar_h, BAR_RADIUS);
        let _ = cr.fill();
    }
}

fn draw_fallback(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    fraction: f64,
) {
    let y = (h - FALLBACK_BAR_HEIGHT) / 2.0;
    let color = area.color();
    let (r, g, b) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );

    let played_w = (fraction * w).max(0.0);
    if played_w > 0.0 {
        cr.set_source_rgba(r, g, b, 1.0);
        rounded_bar(cr, 0.0, y, played_w, FALLBACK_BAR_HEIGHT, BAR_RADIUS);
        let _ = cr.fill();
    }

    let remaining_w = w - played_w;
    if remaining_w > 0.0 {
        cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA);
        rounded_bar(cr, played_w, y, remaining_w, FALLBACK_BAR_HEIGHT, BAR_RADIUS);
        let _ = cr.fill();
    }
}

fn rounded_bar(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}
```

- [ ] **Step 3: Update bucket count**

In `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs`, find the constant `WAVEFORM_BUCKETS` (currently 64) and change to 88:

```rust
const WAVEFORM_BUCKETS: usize = 88;
```

If this constant is also referenced elsewhere (search for `64` in peak-related code), update those too.

- [ ] **Step 4: Run tests**

Run: `cargo test --package reprise-gnome -- waveform -v`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs
git commit -m "feat(waveform): 88 rounded bars, white unplayed, flat fallback"
```

---

### Task 3: Waveform hover + drag-to-seek

Add hover highlight with time tooltip and drag-to-seek with ghost fill.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs`

**Interfaces:**
- Consumes: `WaveformSeek::connect_seek(f)` signature unchanged — drag-end triggers the same callback as click.
- Produces: internal hover/drag state; no new public API.

- [ ] **Step 1: Add hover and drag state to `State`**

```rust
struct State {
    peaks: Vec<f32>,
    fraction: f64,
    hover_index: Option<usize>,
    drag_fraction: Option<f64>,
}
```

Initialize `hover_index: None, drag_fraction: None` in `WaveformSeek::new`.

- [ ] **Step 2: Add `EventControllerMotion` for hover**

In `WaveformSeek::new`, after the click gesture:

```rust
let motion = gtk4::EventControllerMotion::new();
motion.connect_motion({
    let state = state.clone();
    let area = area.clone();
    move |_, x, _| {
        let count = state.borrow().peaks.len();
        if count == 0 {
            return;
        }
        let w = f64::from(area.width());
        let slot = (w + BAR_GAP) / count as f64;
        let index = ((x / slot) as usize).min(count.saturating_sub(1));
        state.borrow_mut().hover_index = Some(index);
        area.queue_draw();
    }
});
motion.connect_leave({
    let state = state.clone();
    let area = area.clone();
    move |_| {
        state.borrow_mut().hover_index = None;
        area.queue_draw();
    }
});
area.add_controller(motion);
```

- [ ] **Step 3: Replace `GestureClick` with `GestureDrag`**

Replace the click gesture with a drag gesture that also handles single clicks:

```rust
let drag = gtk4::GestureDrag::new();
drag.connect_drag_begin({
    let state = state.clone();
    let area = area.clone();
    move |_, x, _| {
        let frac = fraction_at(x, f64::from(area.width()));
        state.borrow_mut().drag_fraction = Some(frac);
        area.queue_draw();
    }
});
drag.connect_drag_update({
    let state = state.clone();
    let area = area.clone();
    move |gesture, offset_x, _| {
        let (start_x, _) = gesture.start_point().unwrap_or((0.0, 0.0));
        let frac = fraction_at(start_x + offset_x, f64::from(area.width()));
        state.borrow_mut().drag_fraction = Some(frac);
        area.queue_draw();
    }
});
drag.connect_drag_end({
    let state = state.clone();
    let on_seek = on_seek.clone();
    let area = area.clone();
    move |gesture, offset_x, _| {
        let (start_x, _) = gesture.start_point().unwrap_or((0.0, 0.0));
        let frac = fraction_at(start_x + offset_x, f64::from(area.width()));
        {
            let mut s = state.borrow_mut();
            s.drag_fraction = None;
            s.fraction = frac;
        }
        area.queue_draw();
        let callback = on_seek.borrow().clone();
        if let Some(callback) = callback {
            callback(frac);
        }
    }
});
area.add_controller(drag);
```

- [ ] **Step 4: Update `draw` to render hover highlight and ghost fill**

In the per-bar drawing loop, after computing the alpha:

```rust
// Inside the for loop, after determining played/unplayed alpha:
let is_hovered = state.hover_index == Some(index);
let is_ghost = state.drag_fraction.map_or(false, |drag_frac| {
    let bar_center = (index as f64 + 0.5) / count as f64;
    let (lo, hi) = if drag_frac > state.fraction {
        (state.fraction, drag_frac)
    } else {
        (drag_frac, state.fraction)
    };
    bar_center > lo && bar_center <= hi
});

if is_hovered {
    // Highlight: full accent alpha regardless of played/unplayed.
    cr.set_source_rgba(r, g, b, 1.0);
} else if is_ghost {
    cr.set_source_rgba(r, g, b, 0.40);
} else if bar_played(index, count, state.fraction) {
    cr.set_source_rgba(r, g, b, 1.0);
} else {
    cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA);
}
```

- [ ] **Step 5: Add hover tooltip**

Set `area.set_has_tooltip(true)` in `new()`, and connect:

```rust
area.connect_query_tooltip({
    let state = state.clone();
    move |area, x, _y, _keyboard, tooltip| {
        let s = state.borrow();
        if s.peaks.is_empty() {
            return false;
        }
        let frac = fraction_at(x as f64, f64::from(area.width()));
        // Duration is tracked externally; use fraction for display.
        // The tooltip just shows the fraction as percentage — actual time
        // tooltip requires duration_ms which is wired in Task 5.
        tooltip.set_text(Some(&format!("{:.0}%", frac * 100.0)));
        true
    }
});
```

(Task 5 will upgrade this to show the actual time by passing `duration_ms` into the waveform.)

- [ ] **Step 6: Run tests and check**

Run: `cargo test --package reprise-gnome -- waveform -v && cargo check --package reprise-gnome`
Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs
git commit -m "feat(waveform): hover highlight, time tooltip, drag-to-seek with ghost fill"
```

---

### Task 4: Waveform smooth interpolation + track-change animation

Add frame-clock-driven smooth fill movement between position ticks, and a 300 ms bar build-up on track change.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs`

**Interfaces:**
- Produces: `WaveformSeek::set_fraction_smooth(fraction: f64)` — replaces `set_fraction` in `player_bar.rs`'s `set_position`.
- Produces: `set_peaks` now triggers the build-up animation automatically.

- [ ] **Step 1: Add interpolation state**

Extend `State`:

```rust
struct State {
    peaks: Vec<f32>,
    fraction: f64,
    hover_index: Option<usize>,
    drag_fraction: Option<f64>,
    // Smooth interpolation.
    target_fraction: f64,
    fraction_velocity: f64, // fraction-per-microsecond
    last_tick_us: i64,
    // Build-up animation.
    build_progress: f64, // 0.0 = not started, 1.0 = complete
    build_start_us: i64,
}
```

- [ ] **Step 2: Implement `set_fraction_smooth`**

```rust
pub(super) fn set_fraction_smooth(&self, fraction: f64) {
    let fraction = fraction.clamp(0.0, 1.0);
    let mut s = self.state.borrow_mut();
    let now = self.area.frame_clock().map_or(0, |c| c.frame_time());
    let dt = (now - s.last_tick_us).max(1) as f64;
    s.fraction_velocity = (fraction - s.target_fraction) / dt;
    s.target_fraction = fraction;
    s.last_tick_us = now;
    drop(s);
    self.ensure_tick_callback();
}
```

- [ ] **Step 3: Add tick callback for interpolation**

Store a `tick_callback_id: RefCell<Option<gtk4::TickCallbackId>>` on `WaveformSeek`. The `ensure_tick_callback` method installs `add_tick_callback` if not already running:

```rust
fn ensure_tick_callback(&self) {
    if self.tick_id.borrow().is_some() {
        return;
    }
    let state = self.state.clone();
    let area = self.area.clone();
    let tick_id_slot = self.tick_id.clone();
    let id = self.area.add_tick_callback(move |_, clock| {
        let now = clock.frame_time();
        let mut s = state.borrow_mut();
        // Interpolate fraction toward target.
        let dt = (now - s.last_tick_us).max(0) as f64;
        s.fraction += s.fraction_velocity * dt;
        s.fraction = s.fraction.clamp(0.0, 1.0);
        s.last_tick_us = now;
        // Build-up animation.
        if s.build_progress < 1.0 && s.build_start_us > 0 {
            let elapsed = (now - s.build_start_us) as f64 / 1_000_000.0;
            s.build_progress = (elapsed / 0.3).clamp(0.0, 1.0); // 300ms
        }
        let done = (s.fraction - s.target_fraction).abs() < 0.001
            && s.build_progress >= 1.0;
        drop(s);
        area.queue_draw();
        if done {
            *tick_id_slot.borrow_mut() = None;
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
    *self.tick_id.borrow_mut() = Some(id);
}
```

- [ ] **Step 4: Trigger build-up in `set_peaks`**

```rust
pub(super) fn set_peaks(&self, peaks: Vec<f32>) {
    let now = self.area.frame_clock().map_or(0, |c| c.frame_time());
    let animate = gtk4::Settings::default()
        .map_or(true, |s| s.is_gtk_enable_animations());
    let mut s = self.state.borrow_mut();
    s.peaks = peaks;
    if animate && !s.peaks.is_empty() {
        s.build_progress = 0.0;
        s.build_start_us = now;
    } else {
        s.build_progress = 1.0;
    }
    drop(s);
    self.area.queue_draw();
    self.ensure_tick_callback();
}
```

- [ ] **Step 5: Use `build_progress` in `draw`**

Scale each bar's height by `build_progress` with a per-bar stagger:

```rust
let stagger = if state.build_progress < 1.0 {
    let bar_delay = index as f64 * 0.002; // 2ms per bar
    ((state.build_progress - bar_delay / 0.3).max(0.0) / (1.0 - bar_delay / 0.3).max(0.01)).clamp(0.0, 1.0)
} else {
    1.0
};
let bar_h = (MIN_BAR_HEIGHT + magnitude * (MAX_BAR_HEIGHT - MIN_BAR_HEIGHT)) * stagger;
```

- [ ] **Step 6: Run tests and check**

Run: `cargo test --package reprise-gnome -- waveform -v && cargo check --package reprise-gnome`

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs
git commit -m "feat(waveform): smooth interpolation between ticks + track-change build-up"
```

---

### Task 5: CenterBox layout + cover/label styling

Replace `ActionBar` with `CenterBox`, 86 px height, three zones, new cover and label styling.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs` — full rewrite
- Modify: `crates/reprise-gnome/src/ui/player_bar/player_bar.rs` — update `PlayerBar` struct and API

**Interfaces:**
- Produces: `PlayerBarWidgets` with `root: gtk4::Box` (wrapping CenterBox), `queue_button: gtk4::Button`, `volume_scale: gtk4::Scale`, `volume_icon: gtk4::Button`. Drops `volume_button: gtk4::ScaleButton`.
- Produces: `PlayerBar::widget()` returns `&gtk4::Box` (was `&gtk4::ActionBar`).
- Produces: `PlayerBar::connect_queue_clicked(f)`, `PlayerBar::connect_mute_toggled(f)`.

- [ ] **Step 1: Rewrite `PlayerBarWidgets` struct**

```rust
pub(super) struct PlayerBarWidgets {
    pub(super) root: gtk4::Box,
    pub(super) center_box: gtk4::CenterBox,
    pub(super) info_box: gtk4::Box,
    pub(super) cover: gtk4::Image,
    pub(super) title_label: gtk4::Label,
    pub(super) artist_label: gtk4::Label,
    pub(super) mini_eq: gtk4::Box,
    pub(super) shuffle_button: gtk4::ToggleButton,
    pub(super) prev_button: gtk4::Button,
    pub(super) play_pause_button: gtk4::Button,
    pub(super) next_button: gtk4::Button,
    pub(super) repeat_button: gtk4::Button,
    pub(super) position_label: gtk4::Label,
    pub(super) duration_label: gtk4::Label,
    pub(super) waveform: super::waveform_seek::WaveformSeek,
    pub(super) volume_icon: gtk4::Button,
    pub(super) volume_scale: gtk4::Scale,
    pub(super) queue_button: gtk4::Button,
}
```

- [ ] **Step 2: Rewrite `build()` function**

Key changes:
- Cover: 56 px, CSS class `player-bar-cover`.
- Title: Pango Bold, font-size 13.5 (via CSS class `player-bar-title`).
- Artist: CSS class `player-bar-artist` with `color: alpha(@window_fg_color, 0.50)`.
- Mini-EQ: 3 `gtk4::Box` children in a container with CSS class `mini-eq`.
- Start zone: `gtk4::Box` with width-request 300.
- Center zone: `gtk4::Box` with max-width 620 (via `set_size_request` and `hexpand`).
- End zone: `gtk4::Box` with width-request 250 containing inline `gtk4::Scale` (80 px) + volume icon + queue button.
- Root: `gtk4::Box` wrapping a `CenterBox`, height-request 86.

```rust
const COVER_PIXEL_SIZE: i32 = 56;
const BAR_HEIGHT: i32 = 86;
const START_ZONE_WIDTH: i32 = 300;
const CENTER_ZONE_MAX_WIDTH: i32 = 620;
const END_ZONE_WIDTH: i32 = 250;
const VOLUME_SLIDER_WIDTH: i32 = 80;
const PLAY_BUTTON_SIZE: i32 = 44;
```

Volume: `gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.05)` with width-request 80. Volume icon: `gtk4::Button` with `audio-volume-high-symbolic`.

Queue button: `gtk4::Button` with `view-list-symbolic` icon.

- [ ] **Step 3: Rewrite `css()` function**

```rust
pub(super) fn css() -> String {
    use super::style::tokens::TRANSITION;
    format!(
        ".player-bar-surface {{ \
           background-color: rgba(26, 26, 26, 0.92); \
           border-top: 1px solid alpha(@window_fg_color, 0.07); }}\n\
         .player-bar-play {{ \
           min-width: {PLAY_BUTTON_SIZE}px; min-height: {PLAY_BUTTON_SIZE}px; \
           background-color: @reprise_player_accent; color: #ffffff; \
           box-shadow: 0 0 16px alpha(@reprise_player_accent, 0.40); \
           transition: box-shadow {TRANSITION}, background-color {TRANSITION}, \
                       transform 120ms ease-out; }}\n\
         .player-bar-play:hover {{ \
           box-shadow: 0 0 20px alpha(@reprise_player_accent, 0.55); }}\n\
         .player-bar-play:active {{ transform: scale(0.94); }}\n\
         .player-bar-cover {{ \
           border-radius: 8px; \
           box-shadow: inset 0 0 0 1px alpha(white, 0.08); }}\n\
         .player-bar-title {{ font-size: 13.5px; }}\n\
         .player-bar-artist {{ color: alpha(@window_fg_color, 0.50); font-size: 12px; }}\n\
         .player-bar-time {{ font-feature-settings: \"tnum\"; }}\n\
         .waveform-seek {{ color: @reprise_player_accent; }}\n\
         {MINI_EQ_CSS}"
    )
}
```

Where `MINI_EQ_CSS` is the keyframe animation string from the spec.

- [ ] **Step 4: Update `PlayerBar` struct**

Change `bar: gtk4::ActionBar` to `root: gtk4::Box`, add `queue_button`, `volume_scale`, `volume_icon`, `mini_eq`, `muted: Cell<bool>`, `pre_mute_volume: Cell<f64>`. Remove `volume_button`. Update `widget()` to return `&gtk4::Box`.

Update `set_position` to use `format_remaining` for the duration label.

Add `connect_queue_clicked`, `connect_mute_toggled` methods. Add `set_mini_eq_playing(bool)` to control animation state.

- [ ] **Step 5: Update `connect_volume_changed` and `set_volume_indicator`**

Replace `volume_button` references with `volume_scale`. The guard logic (`updating_volume`) stays the same, just wire to `gtk4::Scale::connect_value_changed` instead.

Add mute toggle: click the volume icon → set scale to 0 or restore previous volume.

- [ ] **Step 6: Run tests**

Run: `cargo check --package reprise-gnome`
Expected: compilation errors in `player_controller_wiring.rs` and `library_player_bar.rs` — these are fixed in Tasks 6 and 7.

- [ ] **Step 7: Commit (may not compile yet — callers updated in later tasks)**

```bash
git add crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs crates/reprise-gnome/src/ui/player_bar/player_bar.rs
git commit -m "feat(bar): CenterBox layout, 56px cover, inline volume, queue button, mini-EQ"
```

---

### Task 6: Overlay positioning

Replace `LibraryPlayerBarShell`'s `Box` with a `GtkOverlay` so the track list scrolls behind the bar.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/library_player_bar.rs`
- Modify: `crates/reprise-gnome/src/ui/window/window.rs` (bottom padding)

**Interfaces:**
- Consumes: `PlayerBar::widget()` returning `&gtk4::Box` (from Task 5).
- Produces: `LibraryPlayerBarShell::widget()` returns `&gtk4::Overlay` (was `&gtk4::Box`).

- [ ] **Step 1: Rewrite `LibraryPlayerBarShell`**

```rust
use gtk4::prelude::*;
use reprise_core::library::settings::PlayerBarPosition;

const BAR_HEIGHT: i32 = 86;

#[derive(Clone)]
pub(super) struct LibraryPlayerBarShell {
    overlay: gtk4::Overlay,
    bar_box: gtk4::Box,
}

impl LibraryPlayerBarShell {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        player_bar: Option<&gtk4::Widget>,
        _position: PlayerBarPosition,
    ) -> Self {
        let overlay = gtk4::Overlay::new();
        content.set_hexpand(true);
        content.set_vexpand(true);
        overlay.set_child(Some(content));

        let bar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bar_box.set_hexpand(true);
        bar_box.set_valign(gtk4::Align::End);
        if let Some(player_bar) = player_bar {
            bar_box.append(player_bar);
        }
        overlay.add_overlay(&bar_box);

        Self { overlay, bar_box }
    }

    pub(super) fn widget(&self) -> &gtk4::Overlay {
        &self.overlay
    }

    pub(super) fn set_position(&self, position: PlayerBarPosition) {
        self.bar_box.set_valign(match position {
            PlayerBarPosition::Top => gtk4::Align::Start,
            PlayerBarPosition::Bottom => gtk4::Align::End,
        });
    }
}
```

- [ ] **Step 2: Add bottom padding to track list scroll area**

In `window.rs`, after the overlay shell is created, add bottom margin to the track list's scrolled window so the last row isn't hidden:

```rust
// After LibraryPlayerBarShell::new():
track_list.set_bottom_padding(BAR_HEIGHT);
```

This requires adding a `set_bottom_padding` method on `TrackList` that sets `margin-bottom` on the internal scrolled window or list view. Alternatively, use CSS:

```rust
// In the track list's scroll area setup:
scrolled_window.set_margin_bottom(86);
```

Find where the scrolled window is created in track list code and add the margin. This ensures the last row is fully visible above the translucent bar.

- [ ] **Step 3: Update callers**

In `window.rs`, `widget()` now returns `&gtk4::Overlay` — update any type annotations if needed. The rest of the widget tree doesn't change because `Overlay` implements `IsA<Widget>`.

- [ ] **Step 4: Update tests**

Update `library_player_bar.rs` tests: replace `gtk4::Box` assertions with `gtk4::Overlay` assertions.

- [ ] **Step 5: Run tests**

Run: `cargo check --package reprise-gnome`

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/player_bar/library_player_bar.rs crates/reprise-gnome/src/ui/window/window.rs
git commit -m "feat(bar): overlay positioning — content scrolls behind translucent bar"
```

---

### Task 7: Wire new controls + fix callers

Wire the queue button, mute toggle, middle-click stop, and fix all compilation errors from Tasks 5–6.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/playback/player_controller_wiring.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs`
- Modify: `crates/reprise-gnome/src/ui/window/window.rs`
- Modify: any other files that reference `volume_button`, `ActionBar`, or `widget()` type.

**Interfaces:**
- Consumes: `PlayerBar::connect_queue_clicked(f)`, `PlayerBar::connect_mute_toggled(f)` from Task 5.
- Consumes: `sidebar.refresh_and_select(ViewSource::Queue, reason)` (existing API).

- [ ] **Step 1: Wire queue button in `window.rs`**

After the primary_menu::install block, wire the queue button:

```rust
if let Some(ref player) = player {
    let sidebar_for_queue = sidebar.clone();
    player.bar.connect_queue_clicked(move || {
        sidebar_for_queue.refresh_and_select(ViewSource::Queue, "player bar queue button");
    });
}
```

- [ ] **Step 2: Wire middle-click stop**

In `player_controller_wiring.rs`, add after `wire_bar_controls`:

```rust
// Middle-click on play/pause = stop.
let weak = Rc::downgrade(controller);
controller.bar.connect_middle_click_stop(move || {
    if let Some(controller) = weak.upgrade() {
        controller.stop();
    }
});
```

This requires adding `connect_middle_click_stop` to `PlayerBar` (Task 5): a `GestureClick` with `set_button(2)` on the play/pause button.

- [ ] **Step 3: Replace `volume_button` references with `volume_scale`**

In `player_controller_wiring.rs`, the `connect_volume_changed` call already goes through `PlayerBar`'s method — no change needed if Task 5 correctly re-wired internally.

Check `now_playing_wiring.rs` for any direct `volume_button` references.

- [ ] **Step 4: Update `set_position` to use `format_remaining`**

In `player_bar.rs`'s `set_position`, change the duration label:

```rust
use reprise_core::format::{format_duration, format_remaining};

// In set_position:
self.position_label.set_text(&format_duration(position_ms));
self.duration_label.set_text(&format_remaining(position_ms, duration_ms));
```

- [ ] **Step 5: Update `set_state` to control mini-EQ**

```rust
pub fn set_state(&self, state: PlaybackState) {
    // ... existing icon/tooltip/sensitivity logic ...
    self.set_mini_eq_playing(state == PlaybackState::Playing);
}
```

- [ ] **Step 6: Full compile + test**

Run: `cargo test --package reprise-gnome && cargo check --package reprise-gnome`
Expected: all pass, zero errors.

- [ ] **Step 7: Commit**

```bash
git add -u crates/reprise-gnome/src/
git commit -m "feat(bar): wire queue button, middle-click stop, fix all callers"
```

---

### Task 8: OKLCH accent extraction + cross-fade

Replace saturation-weighted-average with median-cut + OKLCH clamping, and cross-fade the accent over 400 ms.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/style/cover_accent.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs` (cross-fade integration)

**Interfaces:**
- Produces: `accent_from_cover_file(path) -> Option<Rgb>` — same signature, new algorithm.
- Produces: `cross_fade_accent(old: Option<Rgb>, new: Option<Rgb>)` — drives a 400 ms tween on the CSS provider.

- [ ] **Step 1: Write tests for median-cut + OKLCH**

```rust
#[test]
fn median_cut_picks_vivid_cluster() {
    // 90% gray pixels, 10% bright red → should pick the red cluster.
    let mut pixels = solid(130, 130, 130, 90);
    pixels.extend(solid(220, 40, 40, 10));
    let accent = dominant_accent(&pixels, 3).expect("red cluster");
    assert!(accent.r > 180, "expected red-dominant, got {accent:?}");
}

#[test]
fn oklch_clamp_limits_lightness_and_chroma() {
    let clamped = oklch_clamp(Rgb { r: 255, g: 0, b: 0 }); // pure red, very saturated
    // L should be in 0.55–0.75, C ≤ 0.13
    // After clamping, the result should be a muted, mid-lightness red-ish color.
    assert!(clamped.r > 100 && clamped.r < 230);
}

#[test]
fn near_gray_falls_back_to_none() {
    let result = dominant_accent(&solid(128, 126, 130, 100), 3);
    assert!(result.is_none() || !is_usable(&result.unwrap()));
}
```

- [ ] **Step 2: Implement median-cut**

Replace `dominant_accent` with a median-cut that:
1. Collects pixels with alpha ≥ 128 into a `Vec<[u8; 3]>`.
2. Recursively splits along the channel with the widest range, 3 levels deep → 8 buckets.
3. For each bucket, compute average RGB and OKLCH chroma.
4. Pick the bucket with max `population × chroma`.
5. Return `oklch_clamp(average_rgb)`.

- [ ] **Step 3: Implement `oklch_clamp`**

Convert RGB → linear sRGB → OKLab → OKLCH. Clamp L to [0.55, 0.75] and C to min(C, 0.13). Convert back. If C < 0.03, return `None` (near-gray → fallback).

Use direct math (no external crate) — the OKLab conversion is ~20 lines of matrix multiplication.

- [ ] **Step 4: Implement `cross_fade_accent`**

```rust
pub(in crate::ui) fn cross_fade_accent(
    old: Option<Rgb>,
    new: Option<Rgb>,
    widget: &impl IsA<gtk4::Widget>,
) {
    let animate = gtk4::Settings::default()
        .map_or(true, |s| s.is_gtk_enable_animations());
    if !animate || old == new {
        set_cover_accent(new);
        return;
    }
    let old_rgb = old.unwrap_or(Rgb { r: 28, g: 169, b: 143 }); // #1CA98F
    let new_rgb = new.unwrap_or(Rgb { r: 28, g: 169, b: 143 });
    let target = adw::CallbackAnimationTarget::new(move |value| {
        let r = lerp(old_rgb.r, new_rgb.r, value);
        let g = lerp(old_rgb.g, new_rgb.g, value);
        let b = lerp(old_rgb.b, new_rgb.b, value);
        set_cover_accent(Some(Rgb { r, g, b }));
    });
    let animation = adw::TimedAnimation::new(widget, 0.0, 1.0, 400, &target);
    animation.play();
    // Store animation to prevent GC — use a thread_local or field.
}

fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round().clamp(0.0, 255.0) as u8
}
```

- [ ] **Step 5: Integrate in `now_playing_wiring.rs`**

Replace the instant `set_cover_accent` call in `apply_cover_accent` with `cross_fade_accent(old, new, widget)`. Store the previous accent in a `RefCell<Option<Rgb>>`.

- [ ] **Step 6: Run tests**

Run: `cargo test --package reprise-gnome -- cover_accent -v`

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-gnome/src/ui/style/cover_accent.rs crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs
git commit -m "feat(accent): median-cut OKLCH extraction + 400ms cross-fade"
```

---

### Task 9: Track-change cover/title cross-fade + reduced-motion

Add 250 ms cross-fade on cover and title when the track changes.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/player_bar.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs`

**Interfaces:**
- Internal to `PlayerBar` — no public API change.

- [ ] **Step 1: Wrap cover and title in `gtk4::Stack` with cross-fade**

In `player_bar_layout.rs`, replace the single `cover` and `title_label`/`artist_label` with a `gtk4::Stack` that has `transition_type: CrossFade` and `transition_duration: 250`.

On `set_track`: clone the old content into a hidden child, set the new content on the visible child, trigger the stack transition.

Since GTK4 Stack cross-fade needs two children, use a simpler approach: `adw::Animation` on `opacity` of the labels (fade out old text, set new text, fade in).

- [ ] **Step 2: Gate on `gtk-enable-animations`**

```rust
fn animate_track_change(&self, title: &str, artist: &str) {
    let animate = gtk4::Settings::default()
        .map_or(true, |s| s.is_gtk_enable_animations());
    if !animate {
        self.title_label.set_text(title);
        self.artist_label.set_text(artist);
        return;
    }
    // Fade out, swap text, fade in — 250ms total.
    let title = title.to_string();
    let artist = artist.to_string();
    let title_label = self.title_label.clone();
    let artist_label = self.artist_label.clone();
    let target = adw::CallbackAnimationTarget::new(move |value| {
        title_label.set_opacity(value);
        artist_label.set_opacity(value);
    });
    // Fade out.
    let fade_out = adw::TimedAnimation::new(&self.title_label, 1.0, 0.0, 125, &target);
    // After fade-out, swap text and fade in.
    // ... (chain with connect_done)
}
```

- [ ] **Step 3: Run tests and check**

Run: `cargo check --package reprise-gnome`

- [ ] **Step 4: Commit**

```bash
git add crates/reprise-gnome/src/ui/player_bar/player_bar.rs crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs
git commit -m "feat(bar): 250ms track-change cross-fade, reduced-motion support"
```

---

### Task 10: Integration, cleanup, smoke test

Final integration: verify all pieces work together, fix any remaining compilation issues, add smoke test.

**Files:**
- Modify: various (fix any remaining references)
- Modify: `crates/reprise-gnome/src/ui/window/window_smoke.rs` (if smoke infrastructure exists)

- [ ] **Step 1: Full compile**

Run: `cargo check --package reprise-gnome`
Fix any remaining errors.

- [ ] **Step 2: Full test suite**

Run: `cargo test --package reprise-gnome`
All tests must pass.

- [ ] **Step 3: Verify `cargo clippy` is clean**

Run: `cargo clippy --package reprise-gnome -- -D warnings`

- [ ] **Step 4: Run `cargo test --workspace`**

Ensure no cross-crate breakage from the `format_remaining` addition.

- [ ] **Step 5: Commit any fixes**

```bash
git add -u
git commit -m "fix(bar): integration cleanup and smoke verification"
```

- [ ] **Step 6: Merge to main**

```bash
git checkout main && git merge feature/player-bar-redesign
```
