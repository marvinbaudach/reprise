use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gdk::prelude::ToplevelExt;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;
use reprise_core::library::settings::WindowDecorationMode;

fn integrated_chrome_visible(mode: WindowDecorationMode) -> bool {
    mode == WindowDecorationMode::Client
}

#[derive(Clone)]
pub(super) struct WindowContentHost {
    root: adw::ToolbarView,
    separate_titlebar: gtk4::HeaderBar,
}

impl WindowContentHost {
    pub(super) fn new(window: &adw::ApplicationWindow) -> Self {
        let title = window.title().unwrap_or_else(|| "Reprise".into());
        let separate_titlebar = gtk4::HeaderBar::new();
        separate_titlebar.set_title_widget(Some(&adw::WindowTitle::new(&title, "")));
        separate_titlebar.set_show_title_buttons(true);
        separate_titlebar.set_visible(false);

        let root = adw::ToolbarView::new();
        root.add_top_bar(&separate_titlebar);
        window.set_content(Some(&root));
        Self {
            root,
            separate_titlebar,
        }
    }

    pub(super) fn set_content(&self, content: &impl IsA<gtk4::Widget>) {
        self.root.set_content(Some(content));
    }

    #[cfg(test)]
    pub(super) fn content(&self) -> Option<gtk4::Widget> {
        self.root.content()
    }

    pub(super) fn additional_height(&self) -> i32 {
        if !self.separate_titlebar.is_visible() {
            return 0;
        }
        let (_, natural, _, _) = self
            .separate_titlebar
            .measure(gtk4::Orientation::Vertical, -1);
        natural.max(self.root.top_bar_height())
    }

    fn set_separate_titlebar_visible(&self, visible: bool) {
        self.separate_titlebar.set_visible(visible);
    }
}

pub(super) struct WindowDecorations {
    window: adw::ApplicationWindow,
    content_host: WindowContentHost,
    library_header: adw::HeaderBar,
    compact_headers: Vec<adw::HeaderBar>,
    compact_titles: Vec<adw::WindowTitle>,
    controls: Vec<gtk4::WindowControls>,
    mode: Cell<WindowDecorationMode>,
    on_mode_changed: RefCell<Option<Rc<dyn Fn()>>>,
}

impl WindowDecorations {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        library_header: &adw::HeaderBar,
        compact_root: Option<&gtk4::Widget>,
    ) -> Rc<Self> {
        let mut compact_headers = Vec::new();
        let mut compact_titles = Vec::new();
        let mut controls = Vec::new();
        if let Some(root) = compact_root {
            collect_decorations(
                root,
                &mut compact_headers,
                &mut compact_titles,
                &mut controls,
            );
        }
        let content_host = WindowContentHost::new(window);
        let decorations = Rc::new(Self {
            window: window.clone(),
            content_host,
            library_header: library_header.clone(),
            compact_headers,
            compact_titles,
            controls,
            mode: Cell::new(WindowDecorationMode::Client),
            on_mode_changed: RefCell::new(None),
        });
        let weak = Rc::downgrade(&decorations);
        window.connect_realize(move |_| {
            if let Some(decorations) = weak.upgrade() {
                decorations.apply_surface_request();
                decorations.sync_controls();
            }
        });
        decorations
    }

    pub(super) fn apply(&self, mode: WindowDecorationMode) {
        self.window.set_decorated(true);
        self.mode.set(mode);
        self.content_host
            .set_separate_titlebar_visible(mode == WindowDecorationMode::System);
        self.apply_surface_request();
        self.sync_controls();
        let on_mode_changed = self.on_mode_changed.borrow().clone();
        if let Some(on_mode_changed) = on_mode_changed {
            on_mode_changed();
        }
        tracing::info!(?mode, "window decoration mode applied");
    }

    pub(super) fn mode(&self) -> WindowDecorationMode {
        self.mode.get()
    }

    pub(super) fn content_host(&self) -> WindowContentHost {
        self.content_host.clone()
    }

    pub(super) fn set_on_mode_changed(&self, on_mode_changed: Rc<dyn Fn()>) {
        self.on_mode_changed.replace(Some(on_mode_changed));
    }

    fn apply_surface_request(&self) {
        let Some(surface) = self.window.surface() else {
            return;
        };
        let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() else {
            tracing::warn!("window surface is not a GDK toplevel; decoration request skipped");
            return;
        };
        // Both supported modes are client-drawn. The separate native GTK
        // titlebar is the reliable GNOME Wayland fallback for unavailable SSD.
        toplevel.set_decorated(false);
    }

    fn sync_controls(&self) {
        let visible = integrated_chrome_visible(self.mode.get());
        self.library_header.set_show_start_title_buttons(visible);
        self.library_header.set_show_end_title_buttons(visible);
        for header in &self.compact_headers {
            header.set_show_start_title_buttons(visible);
            header.set_show_end_title_buttons(visible);
        }
        for title in &self.compact_titles {
            title.set_visible(visible);
        }
        for controls in &self.controls {
            controls.set_visible(visible);
        }
    }
}

fn collect_decorations(
    root: &gtk4::Widget,
    headers: &mut Vec<adw::HeaderBar>,
    titles: &mut Vec<adw::WindowTitle>,
    controls: &mut Vec<gtk4::WindowControls>,
) {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Ok(header) = widget.clone().downcast::<adw::HeaderBar>() {
            headers.push(header);
        } else if let Ok(title) = widget.clone().downcast::<adw::WindowTitle>() {
            titles.push(title);
        } else if let Ok(window_controls) = widget.clone().downcast::<gtk4::WindowControls>() {
            controls.push(window_controls);
        }
        collect_decorations(&widget, headers, titles, controls);
        child = widget.next_sibling();
    }
}

#[cfg(test)]
mod tests {
    use gtk4::gio;
    use reprise_core::library::settings::CompactLayout;

    use super::*;
    use crate::ui::compact_player_layouts;

    #[test]
    fn only_the_integrated_mode_shows_integrated_window_chrome() {
        assert!(integrated_chrome_visible(WindowDecorationMode::Client));
        assert!(!integrated_chrome_visible(WindowDecorationMode::System));
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
        window.set_title(Some("Reprise"));
        let library_header = adw::HeaderBar::new();
        let library_root = adw::ToolbarView::new();
        library_root.add_top_bar(&library_header);
        library_root.set_content(Some(&gtk4::Label::new(Some("Library"))));
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
        decorations.content_host.set_content(&library_root);

        decorations.apply(WindowDecorationMode::Client);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(window.is_decorated());
        assert!(!decorations.content_host.separate_titlebar.is_visible());
        assert_eq!(decorations.content_host.additional_height(), 0);
        assert_eq!(
            decorations.content_host.content().as_ref(),
            Some(library_root.upcast_ref())
        );
        assert!(!surface_decorated(&window));
        assert!(library_header.shows_start_title_buttons());
        assert!(library_header.shows_end_title_buttons());
        assert!(descendants(library_header.upcast_ref())
            .filter_map(|widget| widget.downcast::<gtk4::WindowControls>().ok())
            .any(|controls| controls.side() == gtk4::PackType::End && !controls.is_empty()));
        assert!(all_compact_headers_match(compact_root.upcast_ref(), true));
        assert!(all_compact_titles_match(compact_root.upcast_ref(), true));
        assert!(decorations
            .controls
            .iter()
            .all(gtk4::prelude::WidgetExt::is_visible));

        decorations.apply(WindowDecorationMode::System);
        assert!(decorations.content_host.separate_titlebar.is_visible());
        assert!(decorations.content_host.additional_height() > 0);
        assert!(decorations
            .content_host
            .separate_titlebar
            .shows_title_buttons());
        assert_eq!(separate_title(&decorations), "Reprise");
        assert_eq!(
            decorations.content_host.content().as_ref(),
            Some(library_root.upcast_ref())
        );
        assert!(!surface_decorated(&window));
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(!library_header.shows_start_title_buttons());
        assert!(!library_header.shows_end_title_buttons());
        assert!(all_compact_headers_match(compact_root.upcast_ref(), false));
        assert!(all_compact_titles_match(compact_root.upcast_ref(), false));
        assert!(decorations
            .controls
            .iter()
            .all(|controls| !controls.is_visible()));

        decorations.apply(WindowDecorationMode::Client);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(!decorations.content_host.separate_titlebar.is_visible());
        assert!(!surface_decorated(&window));
        assert!(library_header.shows_start_title_buttons());
        assert!(library_header.shows_end_title_buttons());
        assert!(all_compact_headers_match(compact_root.upcast_ref(), true));
        assert!(all_compact_titles_match(compact_root.upcast_ref(), true));
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

    fn separate_title(decorations: &WindowDecorations) -> String {
        decorations
            .content_host
            .separate_titlebar
            .title_widget()
            .unwrap()
            .downcast::<adw::WindowTitle>()
            .unwrap()
            .title()
            .into()
    }

    fn all_compact_headers_match(root: &gtk4::Widget, expected: bool) -> bool {
        let headers = descendants(root)
            .filter_map(|widget| widget.downcast::<adw::HeaderBar>().ok())
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 2);
        headers.into_iter().all(|header| {
            header.shows_start_title_buttons() == expected
                && header.shows_end_title_buttons() == expected
        })
    }

    fn all_compact_titles_match(root: &gtk4::Widget, expected: bool) -> bool {
        let titles = descendants(root)
            .filter_map(|widget| widget.downcast::<adw::WindowTitle>().ok())
            .collect::<Vec<_>>();
        assert_eq!(titles.len(), 2);
        titles
            .into_iter()
            .all(|title| title.is_visible() == expected)
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
