//! The single set of button interaction states (UX rules BTN-1..4).
//!
//! Adwaita already ships hover and pressed states for `button`. The reason
//! Reprise still felt dead under the cursor is that app CSS runs at
//! `STYLE_PROVIDER_PRIORITY_APPLICATION` and therefore beats the theme
//! regardless of specificity — a *stateless* `background-color: transparent`
//! on a button selector silently deletes Adwaita's `:hover` and `:active`
//! along with the resting fill. This module puts one shared vocabulary back,
//! so feature CSS never has to re-tint buttons locally (BTN-4).
//!
//! The three loudness tiers of BTN-3:
//! - [`PRIMARY_CLASS`] — accent surface, strongest hover and press,
//! - [`ICON_CLASS`] / [`TOGGLE_CLASS`] — flat at rest, hover surface, press sink,
//! - [`TERTIARY_CLASS`] — background hover only, no scale (menu rows).
//!
//! Hover and press are alphas over `currentColor`, not over the accent and not
//! over a literal white. An accent wash sinks into the themed player surface
//! player bar and the Now Playing tab strip, and a fixed white would be
//! invisible on the light palettes the app also ships. `currentColor` is the
//! button's own foreground, so the state always contrasts with the surface it
//! actually sits on — which is what BTN-4 means by measuring on the tint.

use gtk4::prelude::*;

/// Standard flat icon buttons: transport, header actions.
pub(in crate::ui) const ICON_CLASS: &str = "reprise-btn-icon";
/// Toggles that must show their state permanently (Shuffle, Repeat).
pub(in crate::ui) const TOGGLE_CLASS: &str = "reprise-btn-toggle";
/// Accent-surface primary actions (Play/Pause).
pub(in crate::ui) const PRIMARY_CLASS: &str = "reprise-btn-primary";
/// Tertiary/flat entries: background hover only, deliberately no press scale.
pub(in crate::ui) const TERTIARY_CLASS: &str = "reprise-btn-tertiary";

/// Composite widgets whose inner buttons are built by Adwaita and can never
/// carry [`ICON_CLASS`] — the vocabulary has to reach them by selector.
///
/// Kept as short as it can be — every surface that *can* carry a class does,
/// via [`arm`]. These are the ones that cannot: Adwaita builds their buttons
/// internally, or (the tab strip) their own `:checked` rule outranks anything
/// a class could say.
///
/// Naming other modules' classes here is a coupling this file would rather not
/// have. The alternative is worse: copying the state definitions into every
/// feature that hosts a composite is exactly the per-button re-tinting BTN-4
/// forbids, and it would silently leave holes in [`press_scale_selectors`] —
/// the reduced-motion override is only complete if that list is. The owning
/// modules keep their resting look; only the states live here.
const HOSTED_STANDARD_SELECTORS: &[&str] = &[
    // `window::library_chrome` — AdwInlineViewSwitcher's internal buttons.
    ".reprise-view-switcher > button",
    // `now_playing` — AdwInlineViewSwitcher's internal pill-tab buttons. Its
    // own `:checked` rule outranks a class-based state, so use the host selector.
    ".reprise-now-playing-tabs > button",
    // `library_views::artist_master` — GtkDropDown's internal button.
    ".artist-master-sort > button",
    // `tag_edit` — star row and pager, built as button loops.
    ".reprise-tag-stars button",
    ".reprise-tag-nav button",
];

/// Hosted surfaces on the tertiary tier: background hover only, never a scale,
/// because a row inside a list must not jump under the cursor.
const HOSTED_TERTIARY_SELECTORS: &[&str] = &[
    // `style::menus` — GtkPopoverMenu builds its own modelbuttons.
    "popover.menu modelbutton",
];

/// Every selector that sinks on press, in one place so the reduced-motion
/// override (BTN-4) can neutralise all of them without hunting call sites.
fn press_scale_selectors() -> Vec<String> {
    let mut selectors: Vec<String> = [
        ".reprise-btn-icon:active",
        ".reprise-btn-toggle:active",
        ".reprise-btn-toggle:checked:active",
        ".reprise-btn-primary:active",
        "button.suggested-action:active",
    ]
    .iter()
    .map(|selector| (*selector).to_owned())
    .collect();
    selectors.extend(HOSTED_STANDARD_SELECTORS.iter().flat_map(|selector| {
        [
            format!("{selector}:active"),
            format!("{selector}:checked:active"),
        ]
    }));
    selectors
}

/// Selectors that take the keyboard focus ring. Focus is never expressed as
/// the hover state alone (BTN-1).
fn focus_ring_selectors() -> Vec<String> {
    let mut selectors: Vec<String> = [
        ".reprise-btn-icon:focus-visible",
        ".reprise-btn-toggle:focus-visible",
        ".reprise-btn-primary:focus-visible",
        ".reprise-btn-tertiary:focus-visible",
        "button.suggested-action:focus-visible",
    ]
    .iter()
    .map(|selector| (*selector).to_owned())
    .collect();
    selectors.extend(
        HOSTED_STANDARD_SELECTORS
            .iter()
            .chain(HOSTED_TERTIARY_SELECTORS)
            .map(|selector| format!("{selector}:focus-visible")),
    );
    selectors
}

/// Marks `widget` as an interactive button surface: adds `class` and gives it
/// the pointer cursor.
///
/// GTK4 CSS has no `cursor` property, so the cursor has to be set on the
/// widget — the same route [`crate::ui::link_activation::arm`] takes. This is
/// applied to app-authored surfaces only (transport, primary actions, cards),
/// never blanket-wide, so stock dialogs and Preferences keep native GNOME
/// cursor behaviour.
pub(in crate::ui) fn arm(widget: &impl IsA<gtk4::Widget>, class: &str) {
    arm_cursor(widget);
    widget.upcast_ref::<gtk4::Widget>().add_css_class(class);
}

/// The cursor half of [`arm`], for surfaces whose states are reached by
/// selector rather than by class.
pub(in crate::ui) fn arm_cursor(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    // input-parity: ACC-8 keyboard=native-button-activation
    widget.set_cursor_from_name(Some("pointer"));
}

pub(in crate::ui) fn css() -> String {
    use super::tokens::{
        BTN_CHECKED_FILL_ALPHA, BTN_CHECKED_FILL_HOVER_ALPHA, BTN_CHECKED_FILL_PRESS_ALPHA,
        BTN_DOT_SIZE, BTN_DOT_VERTICAL_POSITION, BTN_HOVER_ALPHA, BTN_PRESS_ALPHA, BTN_PRESS_SCALE,
        FOCUS_GLOW_ALPHA, FOCUS_GLOW_BLUR, FOCUS_RING_OFFSET, FOCUS_RING_WIDTH, TRANSITION,
    };

    let focus_ring = format!(
        "{} {{ outline: {FOCUS_RING_WIDTH} solid @accent_color; \
           outline-offset: {FOCUS_RING_OFFSET}; }}",
        focus_ring_selectors().join(", ")
    );
    let press_scale = format!(
        "{} {{ transform: scale({BTN_PRESS_SCALE}); }}",
        press_scale_selectors().join(", ")
    );

    // Hosted composites: same four states, reached by selector because their
    // buttons cannot carry a class. The resting look stays with the owner.
    let hosted_standard = HOSTED_STANDARD_SELECTORS
        .iter()
        .map(|selector| {
            format!(
                "{selector} {{ transition: background-color {TRANSITION}, \
                   color {TRANSITION}, transform {TRANSITION}; }}\n\
                 {selector}:hover {{ \
                   background-color: alpha(currentColor, {BTN_HOVER_ALPHA}); }}\n\
                 {selector}:active {{ \
                   background-color: alpha(currentColor, {BTN_PRESS_ALPHA}); }}\n\
                 {selector}:checked:hover {{ \
                   background-color: alpha(currentColor, {BTN_CHECKED_FILL_HOVER_ALPHA}); }}\n\
                 {selector}:checked:active {{ \
                   background-color: alpha(currentColor, {BTN_CHECKED_FILL_PRESS_ALPHA}); }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let hosted_tertiary = HOSTED_TERTIARY_SELECTORS
        .iter()
        .map(|selector| {
            format!(
                "{selector} {{ transition: background-color {TRANSITION}; }}\n\
                 {selector}:hover {{ \
                   background-color: alpha(currentColor, {BTN_HOVER_ALPHA}); }}\n\
                 {selector}:active {{ \
                   background-color: alpha(currentColor, {BTN_PRESS_ALPHA}); }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "/* BTN-1: standard tier — flat at rest, surface on hover, sinking on \
            press. */\n\
         .{ICON_CLASS}, .{TOGGLE_CLASS} {{ \
           transition: background-color {TRANSITION}, color {TRANSITION}, \
                       transform {TRANSITION}; }}\n\
         .{ICON_CLASS}:hover, .{TOGGLE_CLASS}:hover {{ \
           background-color: alpha(currentColor, {BTN_HOVER_ALPHA}); }}\n\
         .{ICON_CLASS}:active, .{TOGGLE_CLASS}:active {{ \
           background-color: alpha(currentColor, {BTN_PRESS_ALPHA}); }}\n\
         /* BTN-2: the on-state is a permanent accent fill plus a dot below the \
            icon. It outlives hover — hover only modulates the fill's \
            brightness below — and the dot keeps the state readable without \
            relying on colour alone. */\n\
         .{TOGGLE_CLASS}:checked {{ \
           background-color: alpha(@accent_bg_color, {BTN_CHECKED_FILL_ALPHA}); \
           color: @accent_color; \
           background-image: radial-gradient(circle, \
                             @accent_color 0%, @accent_color 45%, transparent 50%); \
           background-size: {BTN_DOT_SIZE} {BTN_DOT_SIZE}; \
           background-position: 50% {BTN_DOT_VERTICAL_POSITION}; \
           background-repeat: no-repeat; }}\n\
         .{TOGGLE_CLASS}:checked:hover {{ \
           background-color: alpha(@accent_bg_color, {BTN_CHECKED_FILL_HOVER_ALPHA}); }}\n\
         .{TOGGLE_CLASS}:checked:active {{ \
           background-color: alpha(@accent_bg_color, {BTN_CHECKED_FILL_PRESS_ALPHA}); }}\n\
         /* BTN-3: primary tier — Adwaita already paints the accent surface, so \
            only the extra press sink and the hover glow are added here. */\n\
         .{PRIMARY_CLASS}, button.suggested-action {{ \
           transition: background-color {TRANSITION}, box-shadow {TRANSITION}, \
                       transform {TRANSITION}; }}\n\
         button.suggested-action:hover {{ \
           box-shadow: 0 0 {FOCUS_GLOW_BLUR} alpha(@accent_color, {FOCUS_GLOW_ALPHA}); }}\n\
         /* BTN-3: tertiary tier — background hover only, no scale, because menu \
            rows sitting in a list must not jump under the cursor. */\n\
         .{TERTIARY_CLASS} {{ transition: background-color {TRANSITION}; }}\n\
         .{TERTIARY_CLASS}:hover {{ \
           background-color: alpha(currentColor, {BTN_HOVER_ALPHA}); }}\n\
         .{TERTIARY_CLASS}:active {{ \
           background-color: alpha(currentColor, {BTN_PRESS_ALPHA}); }}\n\
         /* BTN-4: composites whose buttons Adwaita builds internally get the \
            very same states, addressed by selector. */\n\
         {hosted_standard}\n\
         {hosted_tertiary}\n\
         {press_scale}\n\
         /* BTN-1: keyboard focus is its own signal, never the hover look. */\n\
         {focus_ring}"
    )
}

/// The reduced-motion override (BTN-4): drops the press *scale* while every
/// colour and surface change stays. Loaded into its own provider by
/// [`super::reduced_motion`] when `gtk-enable-animations` is off.
///
/// CSS `transition:` and `@keyframes` already follow that setting on their own
/// (proven by `mot_7_css_honours_enable_animations_setting`), but a
/// `transform` inside `:active` is a static state style, not a transition, so
/// it would keep jumping. Feedback is reduced, never removed.
pub(in crate::ui) fn reduced_motion_css() -> String {
    format!(
        "{} {{ transform: none; }}",
        press_scale_selectors().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::super::tokens;

    #[test]
    fn btn_1_css_defines_all_four_states_for_every_tier() {
        let css = super::css();

        // Rest is the absence of a fill; hover, press and focus must each be present.
        for tier in [super::ICON_CLASS, super::TOGGLE_CLASS] {
            assert!(
                css.contains(&format!(".{tier}:hover")),
                "{tier} lacks hover"
            );
            assert!(
                css.contains(&format!(".{tier}:active")),
                "{tier} lacks press"
            );
            assert!(
                css.contains(&format!(".{tier}:focus-visible")),
                "{tier} lacks focus ring"
            );
        }
        assert!(css.contains(&format!("alpha(currentColor, {})", tokens::BTN_HOVER_ALPHA)));
        assert!(css.contains(&format!("alpha(currentColor, {})", tokens::BTN_PRESS_ALPHA)));
        // A literal white would vanish on the light palettes (theme.rs).
        assert!(!css.contains("alpha(white,"));
        assert!(css.contains(&format!("scale({})", tokens::BTN_PRESS_SCALE)));
        assert!(css.contains(&format!(
            "outline: {} solid @accent_color",
            tokens::FOCUS_RING_WIDTH
        )));
        // The focus ring must be its own treatment, not a reuse of the hover fill.
        assert!(!css.contains(":focus-visible { background-color"));
    }

    #[test]
    fn btn_2_checked_state_carries_a_non_colour_cue_and_outlives_hover() {
        let css = super::css();

        assert!(css.contains(&format!(".{}:checked", super::TOGGLE_CLASS)));
        // Second, non-colour signal: a dot painted as a background layer.
        assert!(css.contains("radial-gradient(circle"));
        assert!(css.contains(&format!("background-size: {0} {0}", tokens::BTN_DOT_SIZE)));
        // Hover on a checked toggle only re-tints the fill; it must not reset
        // the colour, the dot, or any other part of the state display.
        assert_eq!(
            declared_properties(&css, &format!(".{}:checked:hover", super::TOGGLE_CLASS)),
            vec!["background-color"],
            "hover must modulate the fill only, never tip the state display"
        );
    }

    /// The property names declared in the block introduced by `selector`.
    fn declared_properties(css: &str, selector: &str) -> Vec<String> {
        let block = css
            .split(selector)
            .nth(1)
            .unwrap_or_else(|| panic!("no rule for {selector}"))
            .split('}')
            .next()
            .unwrap_or_default()
            .trim_start_matches([' ', '{']);
        block
            .split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .map(|(property, _)| property.trim().to_owned())
            .collect()
    }

    #[test]
    fn btn_3_tertiary_tier_never_scales() {
        let css = super::css();

        assert!(css.contains(&format!(".{}:hover", super::TERTIARY_CLASS)));
        assert!(css.contains(&format!(".{}:active", super::TERTIARY_CLASS)));
        assert!(
            !super::press_scale_selectors()
                .iter()
                .any(|selector| selector.contains(super::TERTIARY_CLASS)),
            "menu rows must not jump under the cursor"
        );
        // The same holds for hosted rows reached by selector.
        for hosted in super::HOSTED_TERTIARY_SELECTORS {
            assert!(
                !super::press_scale_selectors()
                    .iter()
                    .any(|selector| selector.starts_with(hosted)),
                "{hosted} must not scale on press"
            );
            assert!(css.contains(&format!("{hosted}:hover")));
            assert!(css.contains(&format!("{hosted}:active")));
        }
    }

    #[test]
    fn btn_4_reduced_motion_override_covers_every_press_scale_selector() {
        let reduced = super::reduced_motion_css();

        for selector in super::press_scale_selectors() {
            assert!(
                reduced.contains(&selector),
                "{selector} keeps scaling with animations disabled"
            );
        }
        assert!(reduced.contains("transform: none"));
        // Only the scale is dropped — colour and surface changes must survive.
        assert!(!reduced.contains("background-color"));
    }

    #[test]
    fn btn_4_audio_character_view_switcher_uses_shared_button_states() {
        let selector = ".reprise-now-playing-tabs > button";

        assert!(super::HOSTED_STANDARD_SELECTORS.contains(&selector));
        assert!(super::css().contains(&format!("{selector}:hover")));
        assert!(super::reduced_motion_css().contains(&format!("{selector}:active")));
    }

    /// BTN-1 in its literal reading: the four states must be *visibly*
    /// different, so this renders the same button in each of them and compares
    /// the pixels. A CSS-string assertion could not catch a rule that parses
    /// but paints nothing (an alpha too low to see, a selector Adwaita
    /// out-specifies) — which is exactly the failure this whole change is about.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn btn_1_hover_active_focus_distinct() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        use gtk4::prelude::*;

        gtk4::init().unwrap();
        crate::ui::style::install();

        let button = gtk4::ToggleButton::builder()
            .icon_name("media-playlist-shuffle-symbolic")
            .build();
        button.add_css_class("flat");
        button.add_css_class(super::ICON_CLASS);
        button.add_css_class(super::TOGGLE_CLASS);

        // A dark host surface, like the player bar the transport actually sits
        // on: hover has to lift off the tint, not off a null background.
        let host = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        host.add_css_class("player-bar-surface");
        host.set_halign(gtk4::Align::Center);
        host.set_valign(gtk4::Align::Center);
        host.append(&button);

        let window = gtk4::Window::builder().child(&host).build();
        window.set_default_size(200, 120);
        window.present();
        pump();

        let rest = render(&window, &button);
        let hover = with_state(&window, &button, gtk4::StateFlags::PRELIGHT);
        let active = with_state(&window, &button, gtk4::StateFlags::ACTIVE);
        let focus = with_state(&window, &button, gtk4::StateFlags::FOCUS_VISIBLE);
        let checked = with_state(&window, &button, gtk4::StateFlags::CHECKED);

        window.close();

        for (left_name, left) in [
            ("rest", &rest),
            ("hover", &hover),
            ("active", &active),
            ("focus-visible", &focus),
            ("checked", &checked),
        ] {
            assert!(!left.is_empty(), "{left_name} rendered nothing");
        }
        for (left_name, left, right_name, right) in [
            ("rest", &rest, "hover", &hover),
            ("rest", &rest, "active", &active),
            ("rest", &rest, "focus-visible", &focus),
            ("hover", &hover, "active", &active),
            ("hover", &hover, "focus-visible", &focus),
            ("active", &active, "focus-visible", &focus),
            ("rest", &rest, "checked", &checked),
        ] {
            assert_ne!(
                left, right,
                "{left_name} and {right_name} render identically — \
                 the state is not visible to the eye"
            );
        }
    }

    /// Iterates the main context for `ms` wall-clock milliseconds. States are
    /// sampled only after their 150 ms transition has settled — sampling
    /// mid-flight compares two interpolations rather than two states.
    fn pump() {
        let done = std::rc::Rc::new(std::cell::Cell::new(false));
        let done_setter = done.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            done_setter.set(true);
        });
        let context = gtk4::glib::MainContext::default();
        while !done.get() {
            context.iteration(true);
        }
    }

    /// The button's rendered pixels, as encoded PNG bytes.
    fn render(window: &gtk4::Window, button: &gtk4::ToggleButton) -> Vec<u8> {
        use gtk4::prelude::*;

        let paintable = gtk4::WidgetPaintable::new(Some(button));
        let snapshot = gtk4::Snapshot::new();
        // The focus ring is drawn outside the widget's own allocation, so pad
        // the sampled area — otherwise focus-visible reads as rest.
        let width = f64::from(button.width()) + 8.0;
        let height = f64::from(button.height()) + 8.0;
        paintable.snapshot(&snapshot, width, height);
        let Some(node) = snapshot.to_node() else {
            return Vec::new();
        };
        let renderer = window
            .native()
            .and_then(|native| native.renderer())
            .expect("the presented window has a renderer");
        renderer
            .render_texture(&node, None)
            .save_to_png_bytes()
            .to_vec()
    }

    fn with_state(
        window: &gtk4::Window,
        button: &gtk4::ToggleButton,
        state: gtk4::StateFlags,
    ) -> Vec<u8> {
        use gtk4::prelude::*;

        button.set_state_flags(state, false);
        pump();
        let pixels = render(window, button);
        button.unset_state_flags(state);
        pump();
        pixels
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn buttons_css_parses_without_errors() {
        gtk4::init().unwrap();
        let errors = super::super::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
        let errors = super::super::css_parse_errors(&super::reduced_motion_css());
        assert!(errors.is_empty(), "reduced-motion CSS errors: {errors:?}");
    }

    #[test]
    fn css_contains_no_invalid_line_comments() {
        // GTK CSS has no `//` comments; one would discard the next rule.
        let css = super::css();
        assert!(css.lines().all(|line| !line.trim_start().starts_with("//")));
    }
}
