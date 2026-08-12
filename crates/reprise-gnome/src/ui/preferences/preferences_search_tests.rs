use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::preferences_window::{self, PageId};

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_13_one_escape_clears_and_closes_settings_search() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SettingsSearchEscapeTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.present();
    let shell = preferences_window::build(search_pages(), None, None);
    shell.dialog.present(Some(&parent));
    settle_layout();

    shell.search.reveal();
    shell.search.entry().set_text("cover");
    settle_layout();
    shell.search.entry().emit_stop_search();
    settle_layout();

    assert!(shell.search.entry().text().is_empty());
    assert!(!shell.search.is_revealed());
    assert!(!shell.search.entry().has_focus());

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_13_sidebar_counts_and_dims_without_changing_width() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SettingsSearchSidebarTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();

    let shell = preferences_window::build(search_pages(), None, None);
    shell.dialog.present(Some(&parent));
    settle_layout();
    let width_before = shell.sidebar.width();

    shell.search.reveal();
    shell.search.entry().set_text("cover");
    settle_layout();

    assert_eq!(shell.sidebar.width(), width_before);
    assert_eq!(
        shell.sidebar.selected_row(),
        Some(shell.search.all_results_row())
    );
    assert_eq!(shell.search.all_results_count().text(), "1");
    assert_eq!(shell.search.page_count(PageId::Plugins).text(), "1");
    assert_eq!(shell.search.page_row(PageId::Plugins).opacity(), 1.0);
    assert_eq!(shell.search.page_count(PageId::Playback).text(), "0");
    for page in [
        PageId::Playback,
        PageId::Appearance,
        PageId::Layout,
        PageId::Library,
    ] {
        assert!(
            (shell.search.page_row(page).opacity() - 0.42).abs() < 0.001,
            "{page:?} must use the specified 42% dimming"
        );
    }

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_13_real_row_returns_to_its_exact_origin_when_search_clears() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SettingsSearchReparentTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();

    let target = Rc::new(RefCell::new(gtk4::glib::WeakRef::<adw::ActionRow>::new()));
    let shell = preferences_window::build(reparent_pages(&target), None, None);
    shell.dialog.present(Some(&parent));
    settle_layout();

    shell.search.reveal();
    shell.search.entry().set_text("cover");
    settle_layout();
    let row = target
        .borrow()
        .upgrade()
        .expect("the Plugins factory must publish its target row");
    let origin = shell.search.origin_for(row.upcast_ref());

    assert_ne!(row.parent(), Some(origin.parent.clone().upcast()));
    assert!(
        row.is_visible(),
        "a collapsed origin must be visible as a hit"
    );
    assert_eq!(origin.index, 1, "the origin index must precede any moves");
    assert_eq!(origin.subtitle.as_deref(), Some("coverartarchive.org"));
    assert_eq!(
        shell.search.result_path_for(row.upcast_ref()),
        "Plugins › Online content"
    );
    assert!(row.uses_markup());
    assert!(row.title().contains("bgalpha=\"18%\""));
    assert!(row.subtitle().unwrap().contains("bgalpha=\"18%\""));

    shell.search.entry().set_text("");
    settle_layout();

    assert_eq!(row.parent(), Some(origin.parent.clone().upcast()));
    assert_eq!(row.index(), origin.index);
    assert!(!row.is_visible(), "the origin visibility must be restored");
    assert_eq!(row.title(), "Download cover art");
    assert_eq!(row.subtitle().as_deref(), Some("coverartarchive.org"));
    let restored_titles: Vec<_> = (0..3)
        .map(|index| {
            origin
                .parent
                .row_at_index(index)
                .expect("all three rows must be restored")
                .downcast::<adw::PreferencesRow>()
                .expect("the restored child must still be a preference row")
                .title()
                .to_string()
        })
        .collect();
    assert_eq!(
        restored_titles,
        ["Cover before", "Download cover art", "Cover after"]
    );

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_13_result_path_opens_its_page_and_closes_search() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SettingsSearchPathTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();

    let target = Rc::new(RefCell::new(gtk4::glib::WeakRef::<adw::ActionRow>::new()));
    let expander = Rc::new(RefCell::new(gtk4::glib::WeakRef::<adw::ExpanderRow>::new()));
    let shell = preferences_window::build(path_pages(&target, &expander), None, None);
    shell.dialog.present(Some(&parent));
    settle_layout();
    shell.search.reveal();
    shell.search.entry().set_text("plugins");
    settle_layout();
    let row = target
        .borrow()
        .upgrade()
        .expect("the Plugins factory must publish its target row");

    let path_button = shell.search.result_path_button_for(row.upcast_ref());
    let path_label = path_button
        .child()
        .and_downcast::<gtk4::Label>()
        .expect("the path button must contain its caption");
    assert!(
        path_label.label().contains("bgalpha=\"18%\""),
        "the searched page name must use the shared hit highlight"
    );
    path_button.emit_clicked();
    settle_layout();

    assert_eq!(shell.stack.visible_child_name().as_deref(), Some("plugins"));
    assert!(shell.search.entry().text().is_empty());
    assert!(!shell.search.is_revealed());
    assert!(
        row.has_focus(),
        "the restored real control must receive focus"
    );
    let expander = expander
        .borrow()
        .upgrade()
        .expect("the Plugins factory must publish its expander");
    assert!(
        expander.is_expanded(),
        "the path must reveal a target inside a collapsed expander"
    );
    assert!(row.is_visible(), "the path must reveal a hidden target row");

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_13_matching_reads_the_rows_current_subtitle() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SettingsSearchLiveTextTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();

    let target = Rc::new(RefCell::new(gtk4::glib::WeakRef::<adw::ActionRow>::new()));
    let shell = preferences_window::build(live_subtitle_pages(&target), None, None);
    shell.dialog.present(Some(&parent));
    settle_layout();
    shell.search.reveal();
    shell.search.entry().set_text("connected");
    settle_layout();
    assert_eq!(shell.search.all_results_count().text(), "1");

    shell.search.entry().set_text("");
    settle_layout();
    let row = target
        .borrow()
        .upgrade()
        .expect("the Plugins factory must publish its live row");
    row.set_subtitle("Connected as alice - 12 scrobbles");
    shell.search.entry().set_text("alice");
    settle_layout();
    assert_eq!(shell.search.all_results_count().text(), "1");

    shell.search.entry().set_text("");
    settle_layout();
    row.set_subtitle("Not connected");
    shell.search.entry().set_text("alice");
    settle_layout();
    assert_eq!(
        shell.search.all_results_count().text(),
        "0",
        "text removed from the live row must stop matching"
    );

    shell.dialog.force_close();
    parent.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_13_sidebar_counts_only_results_that_can_be_rendered() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.SettingsSearchRenderableCountTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let parent = adw::ApplicationWindow::new(&app);
    parent.set_default_size(900, 760);
    parent.present();
    crate::ui::style::install();

    let live = Rc::new(RefCell::new(gtk4::glib::WeakRef::<adw::ActionRow>::new()));
    let detached = Rc::new(RefCell::new(gtk4::glib::WeakRef::<adw::ActionRow>::new()));
    let shell = preferences_window::build(renderable_count_pages(&live, &detached), None, None);
    shell.dialog.present(Some(&parent));
    settle_layout();
    shell.search.reveal();
    shell.search.entry().set_text("build-index-without-a-hit");
    settle_layout();
    shell.search.entry().set_text("");
    settle_layout();

    let detached = detached
        .borrow()
        .upgrade()
        .expect("the Plugins factory must publish its detachable row");
    let origin = detached
        .parent()
        .and_downcast::<gtk4::ListBox>()
        .expect("the indexed row must begin in a preferences list");
    origin.remove(&detached);
    shell.search.entry().set_text("provider");
    settle_layout();

    let live = live
        .borrow()
        .upgrade()
        .expect("the Plugins factory must publish its renderable row");
    let _rendered_origin = shell.search.origin_for(live.upcast_ref());
    assert_eq!(shell.search.all_results_count().text(), "1");
    assert_eq!(shell.search.page_count(PageId::Plugins).text(), "1");

    shell.dialog.force_close();
    parent.close();
}

fn renderable_count_pages(
    live: &Rc<RefCell<gtk4::glib::WeakRef<adw::ActionRow>>>,
    detached: &Rc<RefCell<gtk4::glib::WeakRef<adw::ActionRow>>>,
) -> Rc<dyn Fn(PageId) -> adw::PreferencesPage> {
    let live = live.clone();
    let detached = detached.clone();
    Rc::new(move |id| {
        let page = adw::PreferencesPage::builder()
            .title(id.title())
            .icon_name(id.icon_name())
            .build();
        let group = adw::PreferencesGroup::builder().title("Providers").build();
        if id == PageId::Plugins {
            let live_row = adw::ActionRow::builder().title("Provider account").build();
            let detached_row = adw::ActionRow::builder()
                .title("Provider diagnostics")
                .build();
            live.borrow_mut().set(Some(&live_row));
            detached.borrow_mut().set(Some(&detached_row));
            group.add(&live_row);
            group.add(&detached_row);
        } else {
            group.add(&adw::ActionRow::builder().title(id.title()).build());
        }
        page.add(&group);
        page
    })
}

fn live_subtitle_pages(
    target: &Rc<RefCell<gtk4::glib::WeakRef<adw::ActionRow>>>,
) -> Rc<dyn Fn(PageId) -> adw::PreferencesPage> {
    let target = target.clone();
    Rc::new(move |id| {
        let page = adw::PreferencesPage::builder()
            .title(id.title())
            .icon_name(id.icon_name())
            .build();
        let group = adw::PreferencesGroup::builder().title("Accounts").build();
        let title = if id == PageId::Plugins {
            "ListenBrainz".to_owned()
        } else {
            id.title()
        };
        let row = adw::ActionRow::builder()
            .title(&title)
            .subtitle(if id == PageId::Plugins {
                "Not connected"
            } else {
                "No account"
            })
            .build();
        if id == PageId::Plugins {
            target.borrow_mut().set(Some(&row));
        }
        group.add(&row);
        page.add(&group);
        page
    })
}

fn path_pages(
    target: &Rc<RefCell<gtk4::glib::WeakRef<adw::ActionRow>>>,
    target_expander: &Rc<RefCell<gtk4::glib::WeakRef<adw::ExpanderRow>>>,
) -> Rc<dyn Fn(PageId) -> adw::PreferencesPage> {
    let target = target.clone();
    let target_expander = target_expander.clone();
    Rc::new(move |id| {
        let page = adw::PreferencesPage::builder()
            .title(id.title())
            .icon_name(id.icon_name())
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(if id == PageId::Plugins {
                "Online content"
            } else {
                "General"
            })
            .build();
        let title = if id == PageId::Plugins {
            "Cover downloads".to_owned()
        } else {
            id.title()
        };
        let row = adw::ActionRow::builder()
            .title(&title)
            .subtitle(if id == PageId::Plugins {
                "coverartarchive.org"
            } else {
                "No matching subtitle"
            })
            .build();
        if id == PageId::Plugins {
            target.borrow_mut().set(Some(&row));
            let expander = adw::ExpanderRow::builder().title("Provider").build();
            expander.add_row(&row);
            target_expander.borrow_mut().set(Some(&expander));
            group.add(&expander);
        } else {
            group.add(&row);
        }
        page.add(&group);
        page
    })
}

fn search_pages() -> Rc<dyn Fn(PageId) -> adw::PreferencesPage> {
    Rc::new(|id| {
        let page = adw::PreferencesPage::builder()
            .title(id.title())
            .icon_name(id.icon_name())
            .build();
        let group_title = if id == PageId::Plugins {
            "Online content"
        } else {
            "General"
        };
        let group = adw::PreferencesGroup::builder().title(group_title).build();
        let title = if id == PageId::Plugins {
            "Cover downloads".to_owned()
        } else {
            id.title()
        };
        let subtitle = if id == PageId::Plugins {
            "coverartarchive.org"
        } else {
            "No matching subtitle"
        };
        let row = adw::ActionRow::builder()
            .title(&title)
            .subtitle(subtitle)
            .build();
        if id == PageId::Plugins {
            let expander = adw::ExpanderRow::builder().title("Provider").build();
            expander.add_row(&row);
            group.add(&expander);
        } else {
            group.add(&row);
        }
        page.add(&group);
        page
    })
}

fn reparent_pages(
    target: &Rc<RefCell<gtk4::glib::WeakRef<adw::ActionRow>>>,
) -> Rc<dyn Fn(PageId) -> adw::PreferencesPage> {
    let target = target.clone();
    Rc::new(move |id| {
        let page = adw::PreferencesPage::builder()
            .title(id.title())
            .icon_name(id.icon_name())
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(if id == PageId::Plugins {
                "Online content"
            } else {
                "General"
            })
            .build();
        if id == PageId::Plugins {
            group.add(&adw::ActionRow::builder().title("Cover before").build());
            let row = adw::ActionRow::builder()
                .title("Download cover art")
                .subtitle("coverartarchive.org")
                .visible(false)
                .build();
            target.borrow_mut().set(Some(&row));
            group.add(&row);
            group.add(&adw::ActionRow::builder().title("Cover after").build());
        } else {
            group.add(&adw::ActionRow::builder().title(id.title()).build());
        }
        page.add(&group);
        page
    })
}

fn settle_layout() {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || {
        quit.quit();
    });
    main_loop.run();
}
