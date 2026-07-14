use std::cell::Cell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gdk::prelude::ToplevelExt;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::settings::WindowDecorationMode;

const SERVER_DECORATION_CLASS: &str = "ssd";

fn client_controls_visible(mode: WindowDecorationMode, desktop_decorated: bool) -> bool {
    mode == WindowDecorationMode::Client || !desktop_decorated
}

fn desktop_decorated(window: &adw::ApplicationWindow) -> bool {
    window.has_css_class(SERVER_DECORATION_CLASS)
}

pub(super) struct WindowDecorations {
    window: adw::ApplicationWindow,
    headers: Vec<adw::HeaderBar>,
    controls: Vec<gtk4::WindowControls>,
    mode: Cell<WindowDecorationMode>,
}

impl WindowDecorations {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        library_header: &adw::HeaderBar,
        compact_root: Option<&gtk4::Widget>,
    ) -> Rc<Self> {
        let mut headers = vec![library_header.clone()];
        let mut controls = Vec::new();
        if let Some(root) = compact_root {
            collect_decorations(root, &mut headers, &mut controls);
        }
        let decorations = Rc::new(Self {
            window: window.clone(),
            headers,
            controls,
            mode: Cell::new(WindowDecorationMode::Client),
        });
        let weak = Rc::downgrade(&decorations);
        window.connect_realize(move |_| {
            if let Some(decorations) = weak.upgrade() {
                decorations.apply_surface_request();
                decorations.sync_controls();
            }
        });
        let weak = Rc::downgrade(&decorations);
        window.connect_css_classes_notify(move |_| {
            if let Some(decorations) = weak.upgrade() {
                decorations.sync_controls();
            }
        });
        decorations
    }

    pub(super) fn apply(&self, mode: WindowDecorationMode) {
        // Keep GtkWindow's own CSD resize frame enabled. The lower-level
        // GdkToplevel hint below selects who should draw the outer chrome.
        self.window.set_decorated(true);
        self.mode.set(mode);
        self.apply_surface_request();
        self.sync_controls();
        tracing::info!(?mode, "window decoration mode applied");
    }

    pub(super) fn mode(&self) -> WindowDecorationMode {
        self.mode.get()
    }

    fn apply_surface_request(&self) {
        let Some(surface) = self.window.surface() else {
            return;
        };
        let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() else {
            tracing::warn!("window surface is not a GDK toplevel; decoration request skipped");
            return;
        };
        toplevel.set_decorated(self.mode.get() == WindowDecorationMode::System);
    }

    fn sync_controls(&self) {
        let desktop_decorated = desktop_decorated(&self.window);
        let visible = client_controls_visible(self.mode.get(), desktop_decorated);
        for header in &self.headers {
            header.set_show_start_title_buttons(visible);
            header.set_show_end_title_buttons(visible);
        }
        for controls in &self.controls {
            controls.set_visible(visible);
        }
    }
}

fn collect_decorations(
    root: &gtk4::Widget,
    headers: &mut Vec<adw::HeaderBar>,
    controls: &mut Vec<gtk4::WindowControls>,
) {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Ok(header) = widget.clone().downcast::<adw::HeaderBar>() {
            headers.push(header);
        } else if let Ok(window_controls) = widget.clone().downcast::<gtk4::WindowControls>() {
            controls.push(window_controls);
        }
        collect_decorations(&widget, headers, controls);
        child = widget.next_sibling();
    }
}

#[cfg(test)]
mod tests {
    use gtk4::gio;
    use libadwaita::prelude::AdwApplicationWindowExt;
    use reprise_core::library::settings::CompactLayout;

    use super::*;
    use crate::ui::compact_player_layouts;

    #[test]
    fn system_mode_keeps_client_controls_until_desktop_decorations_are_active() {
        assert!(client_controls_visible(WindowDecorationMode::Client, false));
        assert!(client_controls_visible(WindowDecorationMode::Client, true));
        assert!(client_controls_visible(WindowDecorationMode::System, false));
        assert!(!client_controls_visible(WindowDecorationMode::System, true));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn client_and_system_modes_project_to_every_window_control() {
        if gtk4::init().is_err() {
            return;
        }
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.DecorationTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let library_header = adw::HeaderBar::new();
        let library_root = adw::ToolbarView::new();
        library_root.add_top_bar(&library_header);
        library_root.set_content(Some(&gtk4::Label::new(Some("Library"))));
        window.set_content(Some(&library_root));
        let compact_root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        for layout in [
            CompactLayout::Cover,
            CompactLayout::Pill,
            CompactLayout::Card,
        ] {
            compact_root.append(&compact_player_layouts::build(layout).root);
        }
        let decorations =
            WindowDecorations::new(&window, &library_header, Some(compact_root.upcast_ref()));

        decorations.apply(WindowDecorationMode::Client);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(window.is_decorated());
        assert!(!surface_decorated(&window));
        assert!(library_header.shows_start_title_buttons());
        assert!(library_header.shows_end_title_buttons());
        assert!(descendants(library_header.upcast_ref())
            .filter_map(|widget| widget.downcast::<gtk4::WindowControls>().ok())
            .any(|controls| controls.side() == gtk4::PackType::End && !controls.is_empty()));
        assert!(all_compact_headers_match(compact_root.upcast_ref(), true));
        assert!(decorations
            .controls
            .iter()
            .all(gtk4::prelude::WidgetExt::is_visible));

        decorations.apply(WindowDecorationMode::System);
        assert!(surface_decorated(&window));
        window.remove_css_class(SERVER_DECORATION_CLASS);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(library_header.shows_start_title_buttons());
        assert!(library_header.shows_end_title_buttons());
        assert!(all_compact_headers_match(compact_root.upcast_ref(), true));
        assert!(decorations
            .controls
            .iter()
            .all(gtk4::prelude::WidgetExt::is_visible));

        window.add_css_class(SERVER_DECORATION_CLASS);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(!library_header.shows_start_title_buttons());
        assert!(!library_header.shows_end_title_buttons());
        assert!(all_compact_headers_match(compact_root.upcast_ref(), false));
        assert!(decorations
            .controls
            .iter()
            .all(|controls| !controls.is_visible()));

        window.remove_css_class(SERVER_DECORATION_CLASS);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(library_header.shows_start_title_buttons());
        assert!(library_header.shows_end_title_buttons());
        assert!(all_compact_headers_match(compact_root.upcast_ref(), true));
        assert!(decorations
            .controls
            .iter()
            .all(gtk4::prelude::WidgetExt::is_visible));
        window.close();
    }

    fn surface_decorated(window: &adw::ApplicationWindow) -> bool {
        window
            .surface()
            .unwrap()
            .downcast::<gdk::Toplevel>()
            .unwrap()
            .is_decorated()
    }

    fn all_compact_headers_match(root: &gtk4::Widget, expected: bool) -> bool {
        let headers = descendants(root)
            .filter_map(|widget| widget.downcast::<adw::HeaderBar>().ok())
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 3);
        headers.into_iter().all(|header| {
            header.shows_start_title_buttons() == expected
                && header.shows_end_title_buttons() == expected
        })
    }

    fn descendants(root: &gtk4::Widget) -> impl Iterator<Item = gtk4::Widget> {
        let mut found = Vec::new();
        collect_descendants(root, &mut found);
        found.into_iter()
    }

    fn collect_descendants(root: &gtk4::Widget, found: &mut Vec<gtk4::Widget>) {
        let mut child = root.first_child();
        while let Some(widget) = child {
            found.push(widget.clone());
            collect_descendants(&widget, found);
            child = widget.next_sibling();
        }
    }
}
