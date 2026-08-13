//! Display-dependent contracts for the device-page card hierarchy.

use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_60_playlist_and_sync_overview_cards_share_the_same_edges() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (_surface, root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let profile = label_with_text(root.upcast_ref(), "Music transfer profile")
        .expect("transfer profile heading");
    let changes =
        label_with_text(root.upcast_ref(), "Playlist changes").expect("playlist changes heading");
    assert!(profile.has_css_class("heading"));
    assert!(changes.has_css_class("heading"));
    assert!(!profile.has_css_class("title-2"));
    assert!(!changes.has_css_class("title-2"));
    let profile_card = card_ancestor(&profile);
    let changes_card = card_ancestor(&changes);
    assert_ne!(
        profile_card, changes_card,
        "the two readings must be separate equally sized cards"
    );
    let pair = profile_card.parent().expect("responsive card pair");
    assert!(pair.is::<adw::WrapBox>());
    assert_eq!(changes_card.parent().as_ref(), Some(&pair));
    let window = gtk4::Window::new();
    window.set_default_size(PROBE_WINDOW_WIDTH, 800);
    window.set_child(Some(&root));
    window.present();
    gtk4::glib::MainContext::default().block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(50),
    ));
    let profile_bounds = profile_card.compute_bounds(&pair).expect("profile bounds");
    let changes_bounds = changes_card.compute_bounds(&pair).expect("changes bounds");
    if profile_bounds.y() == changes_bounds.y() {
        assert_eq!(
            profile_bounds.height(),
            changes_bounds.height(),
            "side-by-side overview cards must share top and bottom edges"
        );
    } else {
        assert_eq!(
            profile_bounds.x(),
            changes_bounds.x(),
            "stacked overview cards must share their left edge"
        );
        assert_eq!(
            profile_bounds.width(),
            changes_bounds.width(),
            "stacked overview cards must share their right edge"
        );
    }
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_61_the_rules_block_carries_both_device_switches() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let settings = Rc::new(RefCell::new(device().settings));
    let content_actions = OnDeviceActions {
        set_remove_deleted: {
            let settings = settings.clone();
            Rc::new(move |value| settings.borrow_mut().remove_deleted = value)
        },
        set_sync_automatically: {
            let settings = settings.clone();
            Rc::new(move |value| settings.borrow_mut().sync_automatically = value)
        },
        scan_device: Rc::new(|| {}),
        open_folder_browser: Rc::new(|_| {}),
        open_playlist_picker: Rc::new(|_| {}),
        dismiss_legacy_media_notice: Rc::new(|| {}),
        legacy_media_notice_pending: Rc::new(|| false),
    };
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &content_actions,
    );

    let text = surface.root_text();
    assert!(text.contains("Rules for this phone"));
    assert!(text.contains("Remove from phone when removed from a playlist"));
    assert!(text.contains("Sync automatically when this phone connects"));
    let mut rules = Vec::new();
    switches(surface.on_device.root().upcast_ref(), &mut rules);
    assert_eq!(rules.len(), 2);
    let rules_title = label_with_text(
        surface.on_device.root().upcast_ref(),
        "Rules for this phone",
    )
    .expect("rules heading");
    let balance = label_with_text(
        surface.on_device.root().upcast_ref(),
        "1 playlist · 0 tracks · 0 B",
    )
    .expect("device balance");
    let rules_card = card_ancestor(&rules_title);
    let balance_card = card_ancestor(&balance);
    assert_ne!(
        rules_card, balance_card,
        "the rules must have their own card after the balance card"
    );
    assert_eq!(
        surface.on_device.root().last_child(),
        Some(rules_card),
        "the rules card must finish the On this device section"
    );
    assert_eq!(
        separator_count(surface.on_device.root().upcast_ref()),
        0,
        "card boundaries replace the old separator lines"
    );
    rules[0].set_active(true);
    rules[1].set_active(false);
    assert!(settings.borrow().remove_deleted);
    assert!(!settings.borrow().sync_automatically);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn disabled_size_limit_explains_why_the_control_is_unavailable() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    let set_limit = button_with_label(surface.on_device.root().upcast_ref(), "Set limit…")
        .expect("disabled size-limit control");
    assert!(!set_limit.is_sensitive());
    assert_eq!(
        set_limit.tooltip_text().as_deref(),
        Some("Size limits are not implemented yet.")
    );
}

fn label_with_text(widget: &gtk4::Widget, text: &str) -> Option<gtk4::Widget> {
    if widget
        .clone()
        .downcast::<gtk4::Label>()
        .is_ok_and(|label| label.text() == text)
    {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = label_with_text(&current, text) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn card_ancestor(widget: &gtk4::Widget) -> gtk4::Widget {
    std::iter::successors(widget.parent(), gtk4::prelude::WidgetExt::parent)
        .find(|ancestor| ancestor.has_css_class("card"))
        .expect("section row must belong to a card")
}

fn switches(widget: &gtk4::Widget, found: &mut Vec<gtk4::Switch>) {
    if let Ok(switch) = widget.clone().downcast::<gtk4::Switch>() {
        found.push(switch);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        switches(&current, found);
        child = current.next_sibling();
    }
}

fn button_with_label(widget: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = button_with_label(&current, label) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn separator_count(widget: &gtk4::Widget) -> usize {
    let mut count = usize::from(widget.is::<gtk4::Separator>());
    let mut child = widget.first_child();
    while let Some(current) = child {
        count += separator_count(&current);
        child = current.next_sibling();
    }
    count
}
