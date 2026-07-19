//! The `gtk-enable-animations` override provider for button feedback (BTN-4).
//!
//! GTK's own CSS machinery already honours the setting — `transition:`
//! hard-switches and `@keyframes` do not run at all (proven by
//! `mot_7_css_honours_enable_animations_setting`). One thing it cannot cover:
//! a `transform` inside `:active` is a *static state style*, not a transition,
//! so a pressed button would keep jumping even with animations off.
//!
//! This provider sits one step above the app CSS and neutralises exactly that
//! scale, leaving every colour and surface change intact. BTN-4's point is
//! that motion is reduced, never that feedback disappears.

use std::cell::RefCell;

thread_local! {
    /// The override provider, kept so the settings handler can reload it.
    static MOTION_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
}

/// Installs the (initially empty) override provider above application
/// priority and keeps it in sync with `gtk-enable-animations`.
pub(super) fn install(display: &gtk4::gdk::Display) {
    let provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    MOTION_PROVIDER.with(|slot| *slot.borrow_mut() = Some(provider));
    refresh();

    if let Some(settings) = gtk4::Settings::default() {
        settings.connect_gtk_enable_animations_notify(|_| refresh());
    }
}

/// Reloads the override to match the current setting. A no-op before
/// [`install`] has run.
pub(in crate::ui) fn refresh() {
    let css = if crate::ui::motion::animations_enabled() {
        String::new()
    } else {
        super::buttons::reduced_motion_css()
    };
    MOTION_PROVIDER.with(|slot| {
        if let Some(provider) = slot.borrow().as_ref() {
            provider.load_from_string(&css);
        }
    });
}

/// The override CSS currently loaded, for tests.
#[cfg(test)]
fn current_css() -> Option<String> {
    MOTION_PROVIDER.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|provider| provider.to_str().into())
    })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn btn_4_reduced_motion_keeps_state_change() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let display = gtk4::gdk::Display::default().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();

        super::install(&display);

        settings.set_gtk_enable_animations(true);
        let with_motion = super::current_css().expect("provider installed");
        assert!(
            with_motion.trim().is_empty(),
            "the override must stay out of the way while animations are on: {with_motion}"
        );

        settings.set_gtk_enable_animations(false);
        let reduced = super::current_css().expect("provider installed");

        settings.set_gtk_enable_animations(previous);

        // The press scale is gone …
        assert!(
            reduced.contains("transform: none"),
            "press scale survived gtk-enable-animations=false: {reduced}"
        );
        // … while the state change itself is untouched: the override says
        // nothing about colour or surface, so hover/checked/active fills from
        // the app CSS still switch — hard instead of smooth.
        assert!(
            !reduced.contains("background-color"),
            "the override must not touch fills: {reduced}"
        );

        let app_css = crate::ui::style::buttons::css();
        assert!(app_css.contains("background-color: alpha(currentColor,"));
        assert!(app_css.contains(".reprise-btn-toggle:checked"));
    }
}
