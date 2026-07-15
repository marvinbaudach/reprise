# Mini-Player Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the three-layout compact player (Cover/Pill/Card) with a single 430×76 px "Mini-Player" that shows cover + waveform + play button, with hover overlay buttons and right-click context menu.

**Architecture:** A new `MiniWidgets` struct built with `build_mini()` replaces the old `LayoutWidgets` + `gtk4::Stack`. The outer `gtk4::WindowHandle` enables drag-to-move; `gtk4::Overlay` layers the hover revealer and volume-feedback bar on top. Crossfade uses `adw::TimedAnimation` on `opacity`, gated on GTK animations setting.

**Tech Stack:** Rust, GTK4, libadwaita, `adw::TimedAnimation`, `gtk4::Overlay`, `gtk4::Revealer` (CrossFade), `gtk4::WindowHandle`, `gtk4::DrawingArea` (volume bar), existing `WaveformSeek`/`CoverLoader`

## Global Constraints

- English everywhere in code and comments
- `cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`, `cargo test --workspace`, `cargo audit` must all pass
- Only allowed RUSTSEC advisory: RUSTSEC-2024-0436
- No app windows on the live desktop — headless only (`dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) ...`)
- File size < 800 lines per file
- RefCell discipline: never hold `Ref`/`RefMut` across GTK callbacks
- `CompactLayout` enum stays in `reprise-core`, DB persistence unchanged; only the UI picker is removed

---

### Task 1: waveform_seek.rs — mini-player geometry

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs`

**Interfaces:**
- Produces: `WaveformSeek::new_mini() -> WaveformSeek` — 16 px content height, 2–13 px bar range

- [ ] **Step 1: Add MINI_* constants and new_mini() constructor**

In `waveform_seek.rs`, after the existing constants block add:

```rust
const MINI_CONTENT_HEIGHT: i32 = 16;
const MINI_MAX_BAR_HEIGHT: f64 = 13.0;
const MINI_MIN_BAR_HEIGHT: f64 = 2.0;
const MINI_FALLBACK_BAR_HEIGHT: f64 = 3.0;
```

Then refactor the internal `draw` closure to accept heights via a helper, and add:

```rust
pub fn new_mini() -> Self {
    Self::new_with_heights(
        MINI_CONTENT_HEIGHT,
        MINI_MAX_BAR_HEIGHT,
        MINI_MIN_BAR_HEIGHT,
        MINI_FALLBACK_BAR_HEIGHT,
    )
}
```

Create `new_with_heights(content_height, max_h, min_h, fallback_h)` as the shared constructor that takes over from `new()` (which calls it with the existing constants).

- [ ] **Step 2: Add failing unit test**

```rust
#[test]
fn mini_waveform_has_16px_height() {
    let w = WaveformSeek::new_mini();
    assert_eq!(w.widget().height_request(), 16);
}
```

Run: `cargo test -p reprise-gnome waveform_seek -- --nocapture`
Expected: FAIL (new_mini not yet public / wired)

- [ ] **Step 3: Implement new_with_heights and wire new_mini**

Ensure `new()` calls `new_with_heights(CONTENT_HEIGHT, MAX_BAR_HEIGHT, MIN_BAR_HEIGHT, FALLBACK_BAR_HEIGHT)`.

- [ ] **Step 4: Run test to verify pass**

Run: `cargo test -p reprise-gnome waveform_seek`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs
git commit -m "feat(waveform): add new_mini() constructor with 16px height"
```

---

### Task 2: compact_player_layouts.rs — single MiniWidgets

**Files:**
- Rewrite: `crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs`

**Interfaces:**
- Consumes: `WaveformSeek::new_mini()` (Task 1), `CoverLoader::set_placeholder(&cover)`, existing CSS token `TRANSITION`
- Produces:
  - `pub struct MiniWidgets { pub root: gtk4::WindowHandle, pub overlay: gtk4::Overlay, pub card: gtk4::Box, pub cover: gtk4::Image, pub title_label: gtk4::Label, pub artist_label: gtk4::Label, pub waveform: WaveformSeek, pub play_pause_button: gtk4::Button, pub hover_revealer: gtk4::Revealer, pub restore_button: gtk4::Button, pub close_button: gtk4::Button, pub volume_bar: gtk4::DrawingArea }`
  - `pub fn build_mini() -> MiniWidgets`
  - `pub fn mini_css() -> String`
  - `pub const MINI_WIDTH: i32 = 430`
  - `pub const MINI_HEIGHT: i32 = 76`

- [ ] **Step 1: Write the full replacement file**

```rust
//! Widget construction for the single Mini-Player layout.

use gtk4::prelude::*;
use libadwaita as adw;

use super::super::player_bar::cover_loader::CoverLoader;
use super::super::style::tokens::TRANSITION;
use super::super::player_bar::waveform_seek::WaveformSeek;

pub const MINI_WIDTH: i32 = 430;
pub const MINI_HEIGHT: i32 = 76;

const COVER_SIZE: i32 = 52;
const PLAY_SIZE: i32 = 38;
const CARD_RADIUS: i32 = 16;
const COVER_RADIUS: i32 = 10;
const PADDING: i32 = 12;
const INNER_SPACING: i32 = 10;

const CSS_CARD: &str = "mini-player-card";
const CSS_COVER: &str = "mini-player-cover";
const CSS_PLAY: &str = "mini-player-play";
const CSS_TITLE: &str = "mini-player-title";
const CSS_ARTIST: &str = "mini-player-artist";
const CSS_HOVER: &str = "mini-player-hover";
const CSS_ICON_BTN: &str = "mini-player-icon-btn";
const CSS_VOL_BAR: &str = "mini-player-vol-bar";

pub struct MiniWidgets {
    pub root: gtk4::WindowHandle,
    pub overlay: gtk4::Overlay,
    pub card: gtk4::Box,
    pub cover: gtk4::Image,
    pub title_label: gtk4::Label,
    pub artist_label: gtk4::Label,
    pub waveform: WaveformSeek,
    pub play_pause_button: gtk4::Button,
    pub hover_revealer: gtk4::Revealer,
    pub restore_button: gtk4::Button,
    pub close_button: gtk4::Button,
    pub volume_bar: gtk4::DrawingArea,
}

pub fn build_mini() -> MiniWidgets {
    // — Cover —
    let cover = gtk4::Image::new();
    cover.set_pixel_size(COVER_SIZE);
    cover.add_css_class(CSS_COVER);
    cover.set_valign(gtk4::Align::Center);
    CoverLoader::set_placeholder(&cover);

    // — Text column (title on top, waveform on bottom) —
    let title_label = gtk4::Label::new(None);
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_xalign(0.0);
    title_label.add_css_class(CSS_TITLE);

    let artist_label = gtk4::Label::new(None);
    artist_label.set_halign(gtk4::Align::Start);
    artist_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    artist_label.set_xalign(0.0);
    artist_label.add_css_class(CSS_ARTIST);

    let meta_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    meta_row.append(&title_label);
    meta_row.append(&artist_label);
    meta_row.set_hexpand(true);

    let waveform = WaveformSeek::new_mini();
    waveform.widget().set_hexpand(true);

    let text_col = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    text_col.append(&meta_row);
    text_col.append(waveform.widget());
    text_col.set_hexpand(true);
    text_col.set_valign(gtk4::Align::Center);

    // — Play/Pause button —
    let play_pause_button = gtk4::Button::from_icon_name("media-playback-start-symbolic");
    play_pause_button.set_valign(gtk4::Align::Center);
    play_pause_button.add_css_class("circular");
    play_pause_button.add_css_class(CSS_PLAY);

    // — Card row —
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, INNER_SPACING);
    card.set_margin_start(PADDING);
    card.set_margin_end(PADDING);
    card.set_margin_top(PADDING);
    card.set_margin_bottom(PADDING);
    card.append(&cover);
    card.append(&text_col);
    card.append(&play_pause_button);
    card.add_css_class(CSS_CARD);
    card.set_size_request(MINI_WIDTH, MINI_HEIGHT);

    // — Volume feedback bar (3 px, top edge, hidden by default) —
    let volume_bar = gtk4::DrawingArea::new();
    volume_bar.set_height_request(3);
    volume_bar.set_hexpand(true);
    volume_bar.set_valign(gtk4::Align::Start);
    volume_bar.add_css_class(CSS_VOL_BAR);
    volume_bar.set_opacity(0.0);

    // — Hover overlay buttons —
    let restore_button = gtk4::Button::from_icon_name("window-restore-symbolic");
    restore_button.add_css_class("circular");
    restore_button.add_css_class(CSS_ICON_BTN);
    restore_button.set_tooltip_text(Some("Restore full window (Ctrl+M)"));
    restore_button.set_width_request(26);
    restore_button.set_height_request(26);

    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("circular");
    close_button.add_css_class(CSS_ICON_BTN);
    close_button.set_tooltip_text(Some("Minimize to tray (playback continues)"));
    close_button.set_width_request(26);
    close_button.set_height_request(26);

    let hover_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    hover_row.append(&restore_button);
    hover_row.append(&close_button);
    hover_row.set_halign(gtk4::Align::End);
    hover_row.set_valign(gtk4::Align::Start);
    hover_row.set_margin_top(6);
    hover_row.set_margin_end(6);
    hover_row.add_css_class(CSS_HOVER);

    let hover_revealer = gtk4::Revealer::new();
    hover_revealer.set_transition_type(gtk4::RevealerTransitionType::Crossfade);
    hover_revealer.set_transition_duration(150);
    hover_revealer.set_child(Some(&hover_row));
    hover_revealer.set_reveal_child(false);
    hover_revealer.set_can_target(false); // pass events through when hidden
    hover_revealer.set_valign(gtk4::Align::Fill);
    hover_revealer.set_halign(gtk4::Align::Fill);

    // — Overlay stacks card + volume bar + hover —
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&card));
    overlay.add_overlay(&volume_bar);
    overlay.add_overlay(&hover_revealer);

    // — WindowHandle enables drag-to-move —
    let root = gtk4::WindowHandle::new();
    root.set_child(Some(&overlay));

    MiniWidgets {
        root,
        overlay,
        card,
        cover,
        title_label,
        artist_label,
        waveform,
        play_pause_button,
        hover_revealer,
        restore_button,
        close_button,
        volume_bar,
    }
}

pub fn mini_css() -> String {
    format!(
        ".{CSS_CARD} {{ \
           background-color: rgba(34, 34, 34, 0.92); \
           border: 1px solid alpha(white, 0.09); \
           border-radius: {CARD_RADIUS}px; \
           box-shadow: 0 8px 32px rgba(0, 0, 0, 0.55); }}\n\
         .{CSS_COVER} {{ \
           border-radius: {COVER_RADIUS}px; \
           box-shadow: inset 0 0 0 1px alpha(white, 0.06); }}\n\
         .{CSS_PLAY} {{ \
           min-width: {PLAY_SIZE}px; min-height: {PLAY_SIZE}px; \
           background-color: @reprise_player_accent; \
           color: #ffffff; \
           box-shadow: 0 0 12px alpha(@reprise_player_accent, 0.40); \
           transition: box-shadow {TRANSITION}, background-color {TRANSITION}, \
                       transform 120ms ease-out; }}\n\
         .{CSS_PLAY}:hover {{ box-shadow: 0 0 18px alpha(@reprise_player_accent, 0.60); }}\n\
         .{CSS_PLAY}:active {{ transform: scale(0.92); }}\n\
         .{CSS_TITLE} {{ font-weight: bold; font-size: 13px; }}\n\
         .{CSS_ARTIST} {{ color: alpha(@window_fg_color, 0.55); font-size: 11px; }}\n\
         .{CSS_ICON_BTN} {{ min-width: 26px; min-height: 26px; padding: 3px; \
           background-color: alpha(@window_bg_color, 0.80); \
           transition: background-color {TRANSITION}; }}\n\
         .{CSS_ICON_BTN}:hover {{ background-color: alpha(@window_bg_color, 0.95); }}\n\
         .{CSS_VOL_BAR} {{ background-color: @reprise_player_accent; \
           border-radius: 0 {CARD_RADIUS}px 0 {CARD_RADIUS}px; }}\n\
         .waveform-seek {{ color: @reprise_player_accent; }}"
    )
}
```

- [ ] **Step 2: Add unit test for CSS and widget structure**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_css_has_accent_and_card_class() {
        let css = mini_css();
        assert!(css.contains("mini-player-card"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains("rgba(34, 34, 34, 0.92)"));
        assert!(css.contains("border-radius: 16px"));
    }

    #[test]
    fn mini_width_height_constants_match_spec() {
        assert_eq!(MINI_WIDTH, 430);
        assert_eq!(MINI_HEIGHT, 76);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p reprise-gnome compact_player_layouts`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs
git commit -m "feat(mini-player): single MiniWidgets layout replaces three-layout stack"
```

---

### Task 3: compact_player_menu.rs — 7 actions, remove layout/shuffle/repeat

**Files:**
- Rewrite: `crates/reprise-gnome/src/ui/compact/compact_player_menu.rs`

**Interfaces:**
- Produces: `CompactMenu` with callbacks: `set_on_restore()`, `set_on_play_pause()`, `set_on_next()`, `set_on_previous()`, `set_on_always_on_top()`, `set_on_preferences()`, `set_on_quit()`, `set_always_on_top_state(bool)`, `menu()` → `&gio::Menu`

- [ ] **Step 1: Write replacement file**

```rust
//! Right-click context menu for the Mini-Player.
//!
//! Menu order (per spec):
//!   Restore Full Window (Ctrl+M) — bold, topmost
//!   ─ separator ─
//!   Play/Pause (Space)
//!   Next (Ctrl+→)
//!   Previous (Ctrl+←)
//!   ─ separator ─
//!   Always on Top  [check item]
//!   ─ separator ─
//!   Preferences (Ctrl+,)
//!   Quit (Ctrl+Q)

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;

const ACTION_RESTORE: &str = "restore";
const ACTION_PLAY_PAUSE: &str = "play-pause";
const ACTION_NEXT: &str = "next";
const ACTION_PREVIOUS: &str = "previous";
const ACTION_ALWAYS_ON_TOP: &str = "always-on-top";
const ACTION_PREFERENCES: &str = "preferences";
const ACTION_QUIT: &str = "quit";

const GROUP: &str = "compact";

pub struct CompactMenu {
    group: gio::SimpleActionGroup,
    menu: gio::Menu,
    on_restore: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_play_pause: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_next: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_previous: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_always_on_top: Rc<RefCell<Option<Box<dyn Fn(bool)>>>>,
    on_preferences: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    on_quit: Rc<RefCell<Option<Box<dyn Fn()>>>>,
    always_on_top_action: gio::SimpleAction,
}

impl CompactMenu {
    pub fn new() -> Self {
        let group = gio::SimpleActionGroup::new();

        // Stateless actions
        let act_restore = gio::SimpleAction::new(ACTION_RESTORE, None);
        let act_play_pause = gio::SimpleAction::new(ACTION_PLAY_PAUSE, None);
        let act_next = gio::SimpleAction::new(ACTION_NEXT, None);
        let act_previous = gio::SimpleAction::new(ACTION_PREVIOUS, None);
        let act_preferences = gio::SimpleAction::new(ACTION_PREFERENCES, None);
        let act_quit = gio::SimpleAction::new(ACTION_QUIT, None);

        // Stateful toggle for Always on Top
        let always_on_top_action = gio::SimpleAction::new_stateful(
            ACTION_ALWAYS_ON_TOP,
            None,
            &false.to_variant(),
        );

        group.add_action(&act_restore);
        group.add_action(&act_play_pause);
        group.add_action(&act_next);
        group.add_action(&act_previous);
        group.add_action(&always_on_top_action);
        group.add_action(&act_preferences);
        group.add_action(&act_quit);

        // Build menu model
        let menu = gio::Menu::new();

        let section0 = gio::Menu::new();
        section0.append(Some("Restore Full Window"), Some(&format!("{GROUP}.{ACTION_RESTORE}")));
        menu.append_section(None, &section0);

        let section1 = gio::Menu::new();
        section1.append(Some("Play/Pause"), Some(&format!("{GROUP}.{ACTION_PLAY_PAUSE}")));
        section1.append(Some("Next"), Some(&format!("{GROUP}.{ACTION_NEXT}")));
        section1.append(Some("Previous"), Some(&format!("{GROUP}.{ACTION_PREVIOUS}")));
        menu.append_section(None, &section1);

        let section2 = gio::Menu::new();
        let aot_item = gio::MenuItem::new(Some("Always on Top"), Some(&format!("{GROUP}.{ACTION_ALWAYS_ON_TOP}")));
        section2.append_item(&aot_item);
        menu.append_section(None, &section2);

        let section3 = gio::Menu::new();
        section3.append(Some("Preferences"), Some(&format!("{GROUP}.{ACTION_PREFERENCES}")));
        section3.append(Some("Quit"), Some(&format!("{GROUP}.{ACTION_QUIT}")));
        menu.append_section(None, &section3);

        let on_restore: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_play_pause: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_next: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_previous: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_always_on_top: Rc<RefCell<Option<Box<dyn Fn(bool)>>>> = Rc::new(RefCell::new(None));
        let on_preferences: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let on_quit: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));

        // Wire stateless actions
        {
            let cb = on_restore.clone();
            act_restore.connect_activate(move |_, _| {
                if let Some(f) = cb.borrow().as_ref() { f(); }
            });
        }
        {
            let cb = on_play_pause.clone();
            act_play_pause.connect_activate(move |_, _| {
                if let Some(f) = cb.borrow().as_ref() { f(); }
            });
        }
        {
            let cb = on_next.clone();
            act_next.connect_activate(move |_, _| {
                if let Some(f) = cb.borrow().as_ref() { f(); }
            });
        }
        {
            let cb = on_previous.clone();
            act_previous.connect_activate(move |_, _| {
                if let Some(f) = cb.borrow().as_ref() { f(); }
            });
        }
        {
            let cb = on_preferences.clone();
            act_preferences.connect_activate(move |_, _| {
                if let Some(f) = cb.borrow().as_ref() { f(); }
            });
        }
        {
            let cb = on_quit.clone();
            act_quit.connect_activate(move |_, _| {
                if let Some(f) = cb.borrow().as_ref() { f(); }
            });
        }

        // Wire always-on-top toggle
        {
            let cb = on_always_on_top.clone();
            let action = always_on_top_action.clone();
            action.connect_activate(move |act, _| {
                let current = act.state()
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(false);
                let next = !current;
                act.set_state(&next.to_variant());
                if let Some(f) = cb.borrow().as_ref() { f(next); }
            });
        }

        Self {
            group,
            menu,
            on_restore,
            on_play_pause,
            on_next,
            on_previous,
            on_always_on_top,
            on_preferences,
            on_quit,
            always_on_top_action,
        }
    }

    /// Install the action group on a widget so menu actions resolve.
    pub fn install_on(&self, widget: &impl gtk4::prelude::IsA<gtk4::Widget>) {
        widget.insert_action_group(GROUP, Some(&self.group));
    }

    pub fn menu(&self) -> &gio::Menu {
        &self.menu
    }

    pub fn set_on_restore(&self, f: impl Fn() + 'static) {
        *self.on_restore.borrow_mut() = Some(Box::new(f));
    }
    pub fn set_on_play_pause(&self, f: impl Fn() + 'static) {
        *self.on_play_pause.borrow_mut() = Some(Box::new(f));
    }
    pub fn set_on_next(&self, f: impl Fn() + 'static) {
        *self.on_next.borrow_mut() = Some(Box::new(f));
    }
    pub fn set_on_previous(&self, f: impl Fn() + 'static) {
        *self.on_previous.borrow_mut() = Some(Box::new(f));
    }
    pub fn set_on_always_on_top(&self, f: impl Fn(bool) + 'static) {
        *self.on_always_on_top.borrow_mut() = Some(Box::new(f));
    }
    pub fn set_on_preferences(&self, f: impl Fn() + 'static) {
        *self.on_preferences.borrow_mut() = Some(Box::new(f));
    }
    pub fn set_on_quit(&self, f: impl Fn() + 'static) {
        *self.on_quit.borrow_mut() = Some(Box::new(f));
    }

    /// Sync the Always on Top check-item to match external state.
    pub fn set_always_on_top_state(&self, on_top: bool) {
        self.always_on_top_action.set_state(&on_top.to_variant());
    }
}
```

- [ ] **Step 2: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_has_seven_action_strings() {
        let m = CompactMenu::new();
        // Spot-check the action group has all 7 actions
        let group = &m.group;
        for action in [
            ACTION_RESTORE, ACTION_PLAY_PAUSE, ACTION_NEXT, ACTION_PREVIOUS,
            ACTION_ALWAYS_ON_TOP, ACTION_PREFERENCES, ACTION_QUIT,
        ] {
            assert!(group.lookup_action(action).is_some(), "missing action: {action}");
        }
    }

    #[test]
    fn always_on_top_toggles_state() {
        let m = CompactMenu::new();
        m.set_always_on_top_state(true);
        let state = m.always_on_top_action.state()
            .and_then(|v| v.get::<bool>())
            .unwrap_or(false);
        assert!(state);
        m.set_always_on_top_state(false);
        let state2 = m.always_on_top_action.state()
            .and_then(|v| v.get::<bool>())
            .unwrap_or(true);
        assert!(!state2);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p reprise-gnome compact_player_menu`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/reprise-gnome/src/ui/compact/compact_player_menu.rs
git commit -m "feat(mini-player): simplified 7-action context menu"
```

---

### Task 4: compact_player_state.rs — remove shuffled/repeat

**Files:**
- Modify: `crates/reprise-gnome/src/ui/compact/compact_player_state.rs`

**Interfaces:**
- Produces: `CompactPresentation { title, artist, state, position_ms, duration_ms, transport_enabled, volume_percent }`

- [ ] **Step 1: Remove shuffled and repeat fields**

Remove `shuffled: bool` and `repeat: Repeat` from `CompactPresentation`. Remove any `use` imports that become dead.

- [ ] **Step 2: Run tests**

Run: `cargo test -p reprise-gnome compact_player_state`
Expected: PASS (or compile-time confirmation if no tests exist)

- [ ] **Step 3: Commit**

```bash
git add crates/reprise-gnome/src/ui/compact/compact_player_state.rs
git commit -m "refactor(mini-player): remove shuffle/repeat from CompactPresentation"
```

---

### Task 5: compact_player.rs — full rewrite

**Files:**
- Rewrite: `crates/reprise-gnome/src/ui/compact/compact_player.rs`

**Interfaces:**
- Consumes: `MiniWidgets` + `build_mini()` + `mini_css()` (Task 2), `CompactMenu` (Task 3)
- Produces:
  - `pub struct CompactPlayer` (opaque, `Clone`)
  - `pub fn new(window: &adw::ApplicationWindow) -> Self`
  - `pub fn handle(&self) -> &gtk4::WindowHandle`
  - `pub fn cover_image(&self) -> &gtk4::Image`
  - `pub fn waveform(&self) -> &WaveformSeek`
  - `pub fn set_track(&self, title: &str, artist: &str)`
  - `pub fn set_playing(&self, playing: bool)`
  - `pub fn set_transport_enabled(&self, enabled: bool)`
  - `pub fn set_position(&self, pos_ms: u64, dur_ms: u64)`
  - `pub fn set_volume(&self, volume: f64)` — triggers volume feedback bar
  - `pub fn connect_play_pause(&self, f: impl Fn() + 'static)`
  - `pub fn connect_restore(&self, f: impl Fn() + 'static)`
  - `pub fn connect_close(&self, f: impl Fn() + 'static)`
  - `pub fn menu(&self) -> &CompactMenu`

- [ ] **Step 1: Write replacement file**

```rust
//! Mini-Player — single 430×76 px compact view.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::gdk;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::compact_player_layouts::{build_mini, mini_css, MiniWidgets, MINI_HEIGHT, MINI_WIDTH};
use super::compact_player_menu::CompactMenu;
use super::super::player_bar::waveform_seek::WaveformSeek;
use super::super::style;

const ICON_PLAY: &str = "media-playback-start-symbolic";
const ICON_PAUSE: &str = "media-playback-pause-symbolic";

/// Duration of each half of the cover/title crossfade.
const CROSSFADE_HALF_MS: u32 = 125;
/// How long the volume bar stays visible after a scroll event.
const VOL_BAR_LINGER_MS: u64 = 800;
/// Delay before hiding hover overlay after mouse leaves.
const HOVER_HIDE_DELAY_MS: u64 = 1000;

struct Inner {
    window: adw::ApplicationWindow,
    widgets: MiniWidgets,
    menu: CompactMenu,
    // Pending title/artist for crossfade second half
    pending_title: RefCell<Option<String>>,
    pending_artist: RefCell<Option<String>>,
    // Volume bar hide timer handle
    vol_bar_hide_source: RefCell<Option<gtk4::glib::SourceId>>,
    // Hover hide timer handle
    hover_hide_source: RefCell<Option<gtk4::glib::SourceId>>,
    always_on_top: Cell<bool>,
}

#[derive(Clone)]
pub struct CompactPlayer(Rc<Inner>);

impl CompactPlayer {
    pub fn new(window: &adw::ApplicationWindow) -> Self {
        let widgets = build_mini();
        let menu = CompactMenu::new();
        menu.install_on(&widgets.card);

        // Load CSS
        style::load_css_string(&mini_css());

        let inner = Rc::new(Inner {
            window: window.clone(),
            widgets,
            menu,
            pending_title: RefCell::new(None),
            pending_artist: RefCell::new(None),
            vol_bar_hide_source: RefCell::new(None),
            hover_hide_source: RefCell::new(None),
            always_on_top: Cell::new(false),
        });

        let player = Self(inner);
        player.wire_hover();
        player.wire_double_click();
        player.wire_right_click();
        player
    }

    pub fn handle(&self) -> &gtk4::WindowHandle {
        &self.0.widgets.root
    }

    pub fn cover_image(&self) -> &gtk4::Image {
        &self.0.widgets.cover
    }

    pub fn waveform(&self) -> &WaveformSeek {
        &self.0.widgets.waveform
    }

    pub fn menu(&self) -> &CompactMenu {
        &self.0.menu
    }

    pub fn set_track(&self, title: &str, artist: &str) {
        if gtk4::Settings::default()
            .map(|s| s.is_gtk_enable_animations())
            .unwrap_or(true)
        {
            self.start_crossfade(title.to_owned(), artist.to_owned());
        } else {
            self.0.widgets.title_label.set_text(title);
            self.0.widgets.artist_label.set_text(artist);
        }
    }

    pub fn set_playing(&self, playing: bool) {
        let icon = if playing { ICON_PAUSE } else { ICON_PLAY };
        self.0.widgets.play_pause_button.set_icon_name(icon);
    }

    pub fn set_transport_enabled(&self, enabled: bool) {
        self.0.widgets.play_pause_button.set_sensitive(enabled);
    }

    pub fn set_position(&self, pos_ms: u64, dur_ms: u64) {
        self.0.widgets.waveform.set_position(pos_ms, dur_ms);
    }

    pub fn set_volume(&self, volume: f64) {
        self.show_volume_bar(volume);
    }

    pub fn connect_play_pause(&self, f: impl Fn() + 'static) {
        self.0.widgets.play_pause_button.connect_clicked(move |_| f());
    }

    pub fn connect_restore(&self, f: impl Fn() + 'static) {
        let f = Rc::new(f);
        let f2 = f.clone();
        self.0.widgets.restore_button.connect_clicked(move |_| f());
        self.0.menu.set_on_restore(move || f2());
    }

    pub fn connect_close(&self, f: impl Fn() + 'static) {
        self.0.widgets.close_button.connect_clicked(move |_| f());
    }

    // ── private helpers ──────────────────────────────────────────────────────

    fn wire_hover(&self) {
        let inner = self.0.clone();
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_enter({
            let inner = inner.clone();
            move |_, _, _| {
                // Cancel any pending hide
                if let Some(id) = inner.hover_hide_source.borrow_mut().take() {
                    id.remove();
                }
                inner.widgets.hover_revealer.set_reveal_child(true);
                inner.widgets.hover_revealer.set_can_target(true);
            }
        });
        motion.connect_leave({
            let inner = inner.clone();
            move |_| {
                let inner2 = inner.clone();
                let id = gtk4::glib::timeout_add_local_once(
                    Duration::from_millis(HOVER_HIDE_DELAY_MS),
                    move || {
                        inner2.widgets.hover_revealer.set_reveal_child(false);
                        inner2.widgets.hover_revealer.set_can_target(false);
                        *inner2.hover_hide_source.borrow_mut() = None;
                    },
                );
                *inner.hover_hide_source.borrow_mut() = Some(id);
            }
        });
        self.0.widgets.card.add_controller(motion);
    }

    fn wire_double_click(&self) {
        let inner = self.0.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(gdk::BUTTON_PRIMARY);
        gesture.connect_pressed(move |g, n_press, _, _| {
            if n_press == 2 {
                g.set_state(gtk4::EventSequenceState::Claimed);
                inner.window.present();
            }
        });
        // Attach to cover and title so double-clicking either restores
        self.0.widgets.cover.add_controller(gesture.clone());
        self.0.widgets.title_label.add_controller(gesture);
    }

    fn wire_right_click(&self) {
        let menu_model = self.0.menu.menu().clone();
        let inner = self.0.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);
        gesture.connect_pressed(move |g, _, x, y| {
            g.set_state(gtk4::EventSequenceState::Claimed);
            let popover = gtk4::PopoverMenu::from_model(Some(&menu_model));
            popover.set_parent(&inner.widgets.card);
            let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            popover.set_pointing_to(Some(&rect));
            popover.popup();
        });
        self.0.widgets.card.add_controller(gesture);
    }

    fn start_crossfade(&self, title: String, artist: String) {
        *self.0.pending_title.borrow_mut() = Some(title);
        *self.0.pending_artist.borrow_mut() = Some(artist);

        let cover = self.0.widgets.cover.clone();
        let title_label = self.0.widgets.title_label.clone();
        let artist_label = self.0.widgets.artist_label.clone();
        let inner = self.0.clone();

        // Phase 1: fade out (opacity 1 → 0)
        let target_cover = adw::PropertyAnimationTarget::new(&cover, "opacity");
        let fade_out = adw::TimedAnimation::new(
            &cover,
            1.0,
            0.0,
            CROSSFADE_HALF_MS,
            target_cover,
        );
        fade_out.connect_done(move |_| {
            // Swap content
            let title = inner.pending_title.borrow_mut().take().unwrap_or_default();
            let artist = inner.pending_artist.borrow_mut().take().unwrap_or_default();
            title_label.set_text(&title);
            artist_label.set_text(&artist);
            // CoverLoader will reload cover separately via cover_image()

            // Phase 2: fade in (opacity 0 → 1)
            let target = adw::PropertyAnimationTarget::new(&cover, "opacity");
            let fade_in = adw::TimedAnimation::new(&cover, 0.0, 1.0, CROSSFADE_HALF_MS, target);
            fade_in.play();
        });
        fade_out.play();
    }

    fn show_volume_bar(&self, _volume: f64) {
        let bar = self.0.widgets.volume_bar.clone();
        bar.set_opacity(1.0);

        // Cancel previous timer
        if let Some(id) = self.0.vol_bar_hide_source.borrow_mut().take() {
            id.remove();
        }

        let bar2 = bar.clone();
        let inner = self.0.clone();
        let id = gtk4::glib::timeout_add_local_once(
            Duration::from_millis(VOL_BAR_LINGER_MS),
            move || {
                bar2.set_opacity(0.0);
                *inner.vol_bar_hide_source.borrow_mut() = None;
            },
        );
        *self.0.vol_bar_hide_source.borrow_mut() = Some(id);
    }
}
```

- [ ] **Step 2: Run compile check**

Run: `cargo build -p reprise-gnome 2>&1 | head -60`
Fix any errors before proceeding.

- [ ] **Step 3: Commit**

```bash
git add crates/reprise-gnome/src/ui/compact/compact_player.rs
git commit -m "feat(mini-player): core CompactPlayer rewrite with crossfade, hover, drag"
```

---

### Task 6: compact_player_scroll.rs — volume feedback integration

**Files:**
- Modify: `crates/reprise-gnome/src/ui/compact/compact_player_scroll.rs`

**Interfaces:**
- Consumes: `CompactPlayer::set_volume(f64)` (Task 5)
- Produces: `pub fn install_with_feedback(widget, on_volume_change: impl Fn(f64) + 'static, on_feedback: impl Fn(f64) + 'static)`

- [ ] **Step 1: Add install_with_feedback**

Read the current file first, then add a new public function that wraps `install()` but additionally calls `on_feedback(new_volume)` after each step, where `new_volume` is the clamped volume value.

- [ ] **Step 2: Run tests**

Run: `cargo test -p reprise-gnome compact_player_scroll`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/reprise-gnome/src/ui/compact/compact_player_scroll.rs
git commit -m "feat(mini-player): scroll volume with visual feedback hook"
```

---

### Task 7: Downstream cleanup

**Files:**
- Modify: `crates/reprise-gnome/src/ui/compact/minimal_view.rs`
- Modify: `crates/reprise-gnome/src/ui/compact/compact_mode_controls.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/player_controller_wiring.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs`
- Modify: `crates/reprise-gnome/src/ui/window/window_decorations.rs` (test only)
- Modify: `crates/reprise-gnome/src/ui/compact/mod.rs` (re-exports)

- [ ] **Step 1: minimal_view.rs — use MINI_WIDTH/MINI_HEIGHT constants**

Replace `apply_compact_metrics(layout)` / `select_layout()` calls with fixed constants. Set window size using `MINI_WIDTH` × `MINI_HEIGHT` from `compact_player_layouts`.

- [ ] **Step 2: compact_mode_controls.rs — remove layout wiring**

Remove `set_on_layout()`, `arm_smoke_layout()` calls. Remove shuffle/repeat compact wiring.

- [ ] **Step 3: player_controller_wiring.rs — remove wire_compact_controls shuffle/repeat/layout**

Remove `connect_shuffle_toggled`, `connect_repeat_clicked`, `set_on_layout` from `wire_compact_controls()`.

- [ ] **Step 4: now_playing_wiring.rs — update set_track signature**

Change `compact_player.set_track(title, artist, album, year)` → `compact_player.set_track(title, artist)`. Remove `sync_shuffle_indicator()` and `sync_repeat_indicator()` compact player calls.

- [ ] **Step 5: window_decorations.rs test — update to use build_mini()**

In the test `client_and_system_modes_project_to_every_window_control`, change:
```rust
// OLD:
for layout in [CompactLayout::Cover, CompactLayout::Pill, CompactLayout::Card] {
    compact_root.append(&compact_player_layouts::build(layout).root);
}
// asserts: headers.len() == 2, titles.len() == 2
```
To:
```rust
// NEW:
let mini = compact_player_layouts::build_mini();
compact_root.append(mini.root.upcast_ref());
// assert: headers.len() == 0, titles.len() == 0
```

- [ ] **Step 6: mod.rs — remove re-exports of deleted items**

Remove `pub use compact_player_layouts::{build, LayoutWidgets, metrics, LayoutMetrics}` etc. Add `pub use compact_player_layouts::{build_mini, MiniWidgets, MINI_WIDTH, MINI_HEIGHT}`.

- [ ] **Step 7: Run full build**

Run: `cargo build --workspace 2>&1 | head -80`
Fix all errors.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-gnome/src/ui/compact/
git add crates/reprise-gnome/src/ui/playback/
git add crates/reprise-gnome/src/ui/window/
git commit -m "refactor(mini-player): wire new compact player, remove old layout plumbing"
```

---

### Task 8: Gate battery

- [ ] **Step 1: cargo fmt**

Run: `cargo fmt --all`
Then: `cargo fmt --check`
Expected: no output (clean)

- [ ] **Step 2: cargo clippy**

Run: `cargo clippy --all-targets --workspace -- -D warnings 2>&1 | head -100`
Fix all warnings before proceeding.

- [ ] **Step 3: cargo test**

Run: `cargo test --workspace 2>&1 | tail -40`
Expected: all tests pass (ignored tests remain ignored)

- [ ] **Step 4: cargo audit**

Run: `cargo audit 2>&1 | grep -v RUSTSEC-2024-0436 | grep -i "error\|warning\|vulnerability" || echo "clean"`
Expected: "clean" or only RUSTSEC-2024-0436

- [ ] **Step 5: Final commit**

```bash
git commit --allow-empty -m "chore(mini-player): gate battery passes — fmt, clippy, test, audit"
```
