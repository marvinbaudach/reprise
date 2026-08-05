//! Persistent status chip and transient actions for an Updates release row.
//!
//! The chip and the actions deliberately occupy separate children: revealing
//! actions must not replace the release status or make the row change width.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::NaiveDate;
use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;
use reprise_core::artist_news::StoredRelease;

use super::release_row::{
    chip_presentation, primary_action, ChipPresentation, OnShowAlbum, PrimaryAction,
};
use crate::ui::strings;

pub(super) struct ReleaseRowActions {
    pub root: gtk4::Box,
    pub actions: gtk4::Box,
    pub hide: gtk4::Button,
}

/// Pointer hover and keyboard focus share one reveal rule. Keeping this pure
/// pins the ACC-1 truth table without synthesising input events.
pub(super) fn actions_revealed(hovered: bool, focused: bool) -> bool {
    hovered || focused
}

/// Selects a fallback when the running icon theme lacks the preferred icon.
fn icon_with_fallback(primary: &'static str, fallback: &'static str) -> &'static str {
    let Some(display) = gtk4::gdk::Display::default() else {
        return primary;
    };
    if gtk4::IconTheme::for_display(&display).has_icon(primary) {
        primary
    } else {
        fallback
    }
}

/// Builds a flat icon button with matching tooltip and accessible label.
fn action_button(icon_name: &str, label: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon_name);
    button.add_css_class("flat");
    button.add_css_class("new-release-action");
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk4::accessible::Property::Label(label)]);
    button
}

/// Opens an announcement or ticket URL externally and logs any failure.
///
/// The URL comes from provider JSON, so it goes through the shared scheme
/// allowlist: anything that is not a web link is silently not opened.
pub(in crate::ui) fn launch_uri(url: &str) {
    crate::ui::external_link::launch(url, "announcement", None);
}

fn primary_button(
    release: &StoredRelease,
    today: NaiveDate,
    on_show_album: &OnShowAlbum,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Button {
    match primary_action(release, today) {
        PrimaryAction::ShowInLibrary => {
            let icon = icon_with_fallback("go-jump-symbolic", "folder-music-symbolic");
            let button = action_button(icon, &strings::text(strings::SHOW_IN_LIBRARY));
            let close_popover = close_popover.clone();
            let on_show_album = on_show_album.clone();
            let title = release.title.clone();
            let artist = release.artist_name.clone();
            button.connect_clicked(move |_| {
                close_popover();
                on_show_album(&title, &artist);
            });
            button
        }
        PrimaryAction::OpenAnnouncement(url) => {
            let icon = icon_with_fallback("external-link-symbolic", "web-browser-symbolic");
            let button = action_button(icon, &strings::text(strings::OPEN_ANNOUNCEMENT));
            let close_popover = close_popover.clone();
            button.connect_clicked(move |_| {
                close_popover();
                launch_uri(&url);
            });
            button
        }
    }
}

fn chip(release: &StoredRelease, today: NaiveDate) -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_valign(gtk4::Align::Center);
    label.add_css_class("new-release-row-status");
    match chip_presentation(release, today) {
        ChipPresentation::Upcoming(copy) => {
            label.set_label(&copy);
            label.add_css_class("new-release-chip");
        }
        ChipPresentation::Released => {
            label.set_label(&strings::text(strings::RELEASED));
            label.add_css_class("new-release-chip-neutral");
        }
        ChipPresentation::PartiallyOwned => {
            label.set_label(&strings::text(strings::NEW_RELEASES_PARTIALLY_OWNED));
            label.add_css_class("new-release-chip-partial");
        }
        ChipPresentation::InLibrary => {
            label.set_label(&strings::text(strings::IN_LIBRARY));
            label.add_css_class("new-release-chip-neutral");
        }
    }
    label
}

pub(super) fn build(
    release: &StoredRelease,
    today: NaiveDate,
    on_show_album: &OnShowAlbum,
    close_popover: &Rc<dyn Fn()>,
) -> ReleaseRowActions {
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    actions.set_valign(gtk4::Align::Center);
    actions.set_opacity(0.0);
    actions.set_can_target(false);
    actions.add_css_class("new-release-row-actions");
    actions.append(&primary_button(
        release,
        today,
        on_show_album,
        close_popover,
    ));
    let hide = action_button(
        "view-conceal-symbolic",
        &strings::text(strings::HIDE_RELEASE),
    );
    actions.append(&hide);

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    root.set_valign(gtk4::Align::Center);
    root.append(&actions);
    root.append(&chip(release, today));

    ReleaseRowActions {
        root,
        actions,
        hide,
    }
}

fn set_actions_revealed(
    actions: &gtk4::Box,
    revealed: bool,
    animation: &RefCell<Option<libadwaita::TimedAnimation>>,
) {
    actions.set_can_target(revealed);
    let target_opacity = if revealed { 1.0 } else { 0.0 };
    if !crate::ui::motion::animations_enabled() {
        actions.set_opacity(target_opacity);
        return;
    }
    if (actions.opacity() - target_opacity).abs() < f64::EPSILON {
        return;
    }
    let target = libadwaita::PropertyAnimationTarget::new(actions, "opacity");
    let next = crate::ui::motion::timed(
        actions,
        actions.opacity(),
        target_opacity,
        crate::ui::motion::MICRO,
        target,
    );
    crate::ui::motion::replace_animation(animation, next.clone());
    next.play();
}

pub(super) fn wire_hover_and_focus(row: &gtk4::Box, actions: &gtk4::Box) {
    let pointer_inside = Rc::new(Cell::new(false));
    let focus_inside = Rc::new(Cell::new(false));
    let animation = Rc::new(RefCell::new(None));

    let motion = gtk4::EventControllerMotion::new();
    let enter_actions = actions.clone();
    let enter_pointer = pointer_inside.clone();
    let enter_focus = focus_inside.clone();
    let enter_animation = animation.clone();
    motion.connect_enter(move |_, _, _| {
        enter_pointer.set(true);
        set_actions_revealed(
            &enter_actions,
            actions_revealed(true, enter_focus.get()),
            &enter_animation,
        );
    });
    let leave_actions = actions.clone();
    let leave_pointer = pointer_inside.clone();
    let leave_focus = focus_inside.clone();
    let leave_animation = animation.clone();
    motion.connect_leave(move |_| {
        leave_pointer.set(false);
        set_actions_revealed(
            &leave_actions,
            actions_revealed(false, leave_focus.get()),
            &leave_animation,
        );
    });
    row.add_controller(motion);

    let focus = gtk4::EventControllerFocus::new();
    let focus_actions = actions.clone();
    let focus_pointer = pointer_inside.clone();
    let focus_inside_enter = focus_inside.clone();
    let focus_animation = animation.clone();
    focus.connect_enter(move |_| {
        focus_inside_enter.set(true);
        set_actions_revealed(
            &focus_actions,
            actions_revealed(focus_pointer.get(), true),
            &focus_animation,
        );
    });
    let blur_actions = actions.clone();
    focus.connect_leave(move |_| {
        focus_inside.set(false);
        set_actions_revealed(
            &blur_actions,
            actions_revealed(pointer_inside.get(), false),
            &animation,
        );
    });
    row.add_controller(focus);
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::artist_news::LibraryPresence;

    fn release() -> StoredRelease {
        StoredRelease {
            release_group_mbid: "rg-sample".into(),
            artist_name: "Artist".into(),
            artist_mbid: "artist-id".into(),
            title: "Release".into(),
            release_type: "Album".into(),
            first_release_date: "2026-01-01".into(),
            fetched_at: 100,
            seen_at: None,
            hidden: false,
            presence: LibraryPresence::Absent,
            announce_url: None,
            track_count: None,
            local_track_count: 0,
        }
    }

    #[test]
    fn nr_10a_actions_reveal_on_hover_or_focus() {
        assert!(!actions_revealed(false, false));
        assert!(actions_revealed(true, false));
        assert!(actions_revealed(false, true));
        assert!(actions_revealed(true, true));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_10a_chip_keeps_its_allocation_while_actions_become_visible() {
        gtk4::init().unwrap();
        let on_show_album: OnShowAlbum = Rc::new(|_, _| {});
        let close_popover: Rc<dyn Fn()> = Rc::new(|| {});
        let trailing = build(
            &release(),
            NaiveDate::from_ymd_opt(2026, 7, 21).unwrap(),
            &on_show_album,
            &close_popover,
        );
        let chip = trailing.root.last_child().unwrap();
        let window = gtk4::Window::builder().child(&trailing.root).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let reserved_actions_width = trailing.actions.width();
        let chip_width = chip.width();
        assert!(
            reserved_actions_width > 0,
            "hidden actions reserve their space"
        );
        assert!(chip_width > 0, "the status chip is allocated");
        assert!(chip.is_visible(), "the chip starts visible");

        let animation = RefCell::new(None);
        set_actions_revealed(&trailing.actions, true, &animation);
        if let Some(animation) = animation.borrow().as_ref() {
            animation.skip();
        }
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(
            trailing.actions.opacity(),
            1.0,
            "actions finish visibly revealed"
        );
        assert_eq!(trailing.actions.width(), reserved_actions_width);
        assert!(
            chip.is_visible(),
            "revealing actions does not evict the chip"
        );
        assert_eq!(chip.width(), chip_width, "the chip keeps its allocation");
        window.close();
    }
}
