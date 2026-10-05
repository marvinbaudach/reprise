use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;
use reprise_core::browser::navigation::NavigationIntent;
use reprise_core::browser::{AlbumKey, BrowserPlace};
use reprise_core::view_source::ViewSource;
use reprise_view::quick_open::{QuickOpenAction, QuickOpenCandidate, QuickOpenKind, QuickOpenRow};

use super::quick_open::QuickOpenPanel;
use super::quick_open_wiring::wire_quick_open_shortcut;

fn settle_until(label: &str, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(std::time::Instant::now() < deadline, "timed out: {label}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_17_ctrl_k_navigates_to_an_album_and_escape_restores_focus() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.QuickOpenTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    window.set_default_size(900, 700);
    let invoker = gtk4::Button::with_label("Library focus");
    window.set_content(Some(&invoker));
    window.present();
    invoker.grab_focus();

    let panel = Rc::new(QuickOpenPanel::new());
    let history = Rc::new(crate::ui::nav_history::NavHistory::default());
    history.record_route(&crate::ui::nav_history::NavPlace::source(
        ViewSource::Library,
    ));
    let navigated = Rc::new(RefCell::new(None));
    wire_quick_open_shortcut(
        &app,
        &window,
        &panel,
        {
            let panel = panel.clone();
            Rc::new(move |_, generation, _| {
                panel.set_results(
                    generation,
                    vec![QuickOpenRow::Item(QuickOpenCandidate {
                        kind: QuickOpenKind::Album,
                        title: "Blue".into(),
                        subtitle: "Joni Mitchell".into(),
                        search_text: vec!["Blue".into()],
                        play_count: 10,
                        action: QuickOpenAction::NavigateAlbum {
                            album: "Blue".into(),
                            album_artist: "Joni Mitchell".into(),
                        },
                    })],
                );
            })
        },
        {
            let history = history.clone();
            let navigated = navigated.clone();
            Rc::new(move |row, _play_next| {
                let QuickOpenRow::Item(item) = row else {
                    return;
                };
                let QuickOpenAction::NavigateAlbum {
                    album,
                    album_artist,
                } = item.action
                else {
                    return;
                };
                let place = history
                    .navigate_from(
                        NavigationIntent::OpenAlbum {
                            album: AlbumKey::new(album, album_artist),
                            anchor_track_id: None,
                        },
                        BrowserPlace::from(ViewSource::Library),
                    )
                    .unwrap();
                *navigated.borrow_mut() = Some(place.view_source());
            })
        },
    );

    ActionGroupExt::activate_action(&window, "quick-open", None);
    settle_until("quick open entry has focus", || {
        panel.entry_contains_focus()
    });
    panel.entry().set_text("blue");
    settle_until("album result is visible", || panel.result_count() == 1);
    panel.entry().emit_activate();
    assert_eq!(
        navigated.borrow().as_ref(),
        Some(&ViewSource::Album {
            album: "Blue".into(),
            album_artist: "Joni Mitchell".into(),
        })
    );
    assert_eq!(
        history.go_back().map(|place| place.view_source()),
        Some(ViewSource::Library)
    );

    settle_until("quick open closed after activation", || !panel.is_visible());
    invoker.grab_focus();
    settle_until("invoker regained focus before reopening", || {
        invoker.has_focus()
    });
    ActionGroupExt::activate_action(&window, "quick-open", None);
    settle_until("quick open reopened", || panel.entry_contains_focus());
    panel.press_escape();
    settle_until("Escape closed quick open", || !panel.is_visible());
    settle_until("focus returned to the invoker", || invoker.has_focus());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_17_play_restores_focus_to_the_invoking_list_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.QuickOpenFocusTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let list = gtk4::ListBox::new();
    let invoker = gtk4::ListBoxRow::new();
    invoker.set_child(Some(&gtk4::Label::new(Some("Library track"))));
    invoker.set_focusable(true);
    list.append(&invoker);
    window.set_content(Some(&list));
    window.present();
    invoker.grab_focus();

    let panel = Rc::new(QuickOpenPanel::new());
    let played = Rc::new(std::cell::Cell::new(false));
    wire_quick_open_shortcut(
        &app,
        &window,
        &panel,
        {
            let panel = panel.clone();
            Rc::new(move |_, generation, _| {
                panel.set_results(
                    generation,
                    vec![QuickOpenRow::Item(QuickOpenCandidate {
                        kind: QuickOpenKind::Track,
                        title: "Blue".into(),
                        subtitle: "Joni Mitchell".into(),
                        search_text: vec!["Blue".into()],
                        play_count: 10,
                        action: QuickOpenAction::PlayTrack {
                            track_id: 7,
                            album: Some("Blue".into()),
                            album_artist: Some("Joni Mitchell".into()),
                            artist: Some("Joni Mitchell".into()),
                        },
                    })],
                );
            })
        },
        {
            let played = played.clone();
            Rc::new(move |_, _| played.set(true))
        },
    );

    ActionGroupExt::activate_action(&window, "quick-open", None);
    settle_until("quick open entry has focus", || {
        panel.entry_contains_focus()
    });
    panel.entry().set_text("blue");
    settle_until("track result is visible", || panel.result_count() == 1);
    panel.entry().emit_activate();

    settle_until("play action dispatched", || played.get());
    settle_until("quick open closed", || !panel.is_visible());
    settle_until("focus returned to the track row", || invoker.has_focus());
    window.close();
}
