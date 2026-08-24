//! Main-content stack sizing and source-transition policy.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

type PageFactory<T> = Box<dyn FnOnce() -> (Rc<T>, gtk4::Widget)>;
type PageWiring<T> = Box<dyn FnOnce(&Rc<T>)>;

struct DeferredPageInner<T> {
    holder: adw::Bin,
    factory: RefCell<Option<PageFactory<T>>>,
    value: OnceCell<Rc<T>>,
    wiring: RefCell<Vec<PageWiring<T>>>,
}

/// A named stack child whose content and external wiring appear on first use.
///
/// GTK property notifications are synchronous, so changing the visible child
/// finishes both construction and every registered wiring callback before the
/// caller that navigated by name resumes.
pub(in crate::ui) struct DeferredPage<T> {
    inner: Rc<DeferredPageInner<T>>,
}

impl<T> Clone for DeferredPage<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static> DeferredPage<T> {
    pub(in crate::ui) fn install(
        stack: &gtk4::Stack,
        name: &'static str,
        factory: impl FnOnce() -> (Rc<T>, gtk4::Widget) + 'static,
    ) -> Self {
        let holder = adw::Bin::new();
        holder.set_vexpand(true);
        stack.add_named(&holder, Some(name));
        let page = Self {
            inner: Rc::new(DeferredPageInner {
                holder,
                factory: RefCell::new(Some(Box::new(factory))),
                value: OnceCell::new(),
                wiring: RefCell::new(Vec::new()),
            }),
        };
        stack.connect_visible_child_name_notify({
            let page = page.clone();
            move |stack| {
                if stack.visible_child_name().as_deref() == Some(name) {
                    page.materialize();
                }
            }
        });
        page
    }

    pub(in crate::ui) fn materialize(&self) -> Rc<T> {
        if let Some(value) = self.inner.value.get() {
            return value.clone();
        }
        let factory = self
            .inner
            .factory
            .borrow_mut()
            .take()
            .expect("deferred page factory missing before materialization");
        let (value, widget) = factory();
        self.inner
            .value
            .set(value.clone())
            .unwrap_or_else(|_| panic!("deferred page materialized more than once"));
        let wiring = std::mem::take(&mut *self.inner.wiring.borrow_mut());
        for wire in wiring {
            wire(&value);
        }
        adw::prelude::BinExt::set_child(&self.inner.holder, Some(&widget));
        value
    }

    pub(in crate::ui) fn on_materialized(&self, wire: impl FnOnce(&Rc<T>) + 'static) {
        if let Some(value) = self.inner.value.get().cloned() {
            wire(&value);
        } else {
            self.inner.wiring.borrow_mut().push(Box::new(wire));
        }
    }

    pub(in crate::ui) fn if_materialized(&self, apply: impl FnOnce(&Rc<T>)) {
        if let Some(value) = self.inner.value.get() {
            apply(value);
        }
    }
}

pub(super) fn build() -> gtk4::Stack {
    let stack = gtk4::Stack::new();
    super::library_player_bar::configure_content_stack(&stack);
    stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
    stack
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageTransition {
    Crossfade,
    FadeThrough,
}

fn transition_for_switch(from: Option<&str>, to: &str) -> PageTransition {
    if matches!(
        (from, to),
        (Some("podcasts" | "youtube"), "library") | (Some("library"), "podcasts" | "youtube")
    ) {
        PageTransition::FadeThrough
    } else {
        PageTransition::Crossfade
    }
}

pub(in crate::ui) fn show_page(stack: &gtk4::Stack, name: &str) {
    let from = stack.visible_child_name();
    let transition = transition_for_switch(from.as_deref(), name);
    let Some(incoming) = stack.child_by_name(name) else {
        tracing::warn!(page = name, "content stack target is not installed");
        return;
    };
    // A dense source left transparent by an earlier fade-through becomes
    // fully visible before it is used as an incoming page again.
    incoming.set_opacity(1.0);
    if transition == PageTransition::FadeThrough {
        // MOT-8: retain the same Standard-duration surface transition as
        // other location switches without crossfading two readable tables.
        // Hiding only the outgoing child turns GtkStack's normal crossfade
        // into a single-surface fade-through; the incoming page still fades
        // in and the surrounding shell never hard-cuts.
        if let Some(outgoing) = from.as_deref().and_then(|name| stack.child_by_name(name)) {
            outgoing.set_opacity(0.0);
        }
    }
    stack.set_visible_child_full(name, gtk4::StackTransitionType::Crossfade);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    #[test]
    fn mot_8_dense_source_switch_retains_standard_motion() {
        assert_eq!(
            super::transition_for_switch(Some("podcasts"), "library"),
            super::PageTransition::FadeThrough
        );
        assert_eq!(
            super::transition_for_switch(Some("library"), "podcasts"),
            super::PageTransition::FadeThrough
        );
        assert_eq!(
            super::transition_for_switch(Some("youtube"), "library"),
            super::PageTransition::FadeThrough
        );
        assert_eq!(
            super::transition_for_switch(Some("library"), "stats"),
            super::PageTransition::Crossfade
        );
    }

    #[test]
    #[ignore = "requires a display; run through scripts/check-display-tests.sh"]
    fn deferred_page_materializes_once_and_runs_late_wiring_synchronously() {
        use std::cell::Cell;
        use std::rc::Rc;

        gtk4::init().unwrap();
        let stack = super::build();
        stack.add_named(&gtk4::Label::new(Some("Library")), Some("library"));
        stack.set_visible_child_name("library");
        let constructions = Rc::new(Cell::new(0));
        let wired = Rc::new(Cell::new(false));
        let page = super::DeferredPage::install(&stack, "stats", {
            let constructions = constructions.clone();
            move || {
                constructions.set(constructions.get() + 1);
                let label = gtk4::Label::new(Some("My Stats"));
                (Rc::new(label.clone()), label.upcast())
            }
        });
        page.on_materialized({
            let wired = wired.clone();
            move |_| wired.set(true)
        });

        assert_eq!(constructions.get(), 0);
        stack.set_visible_child_name("stats");
        assert_eq!(constructions.get(), 1);
        assert!(
            wired.get(),
            "external wiring must finish before navigation returns"
        );
        stack.set_visible_child_name("stats");
        assert_eq!(constructions.get(), 1);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn hidden_device_page_cannot_expand_the_visible_library_page() {
        gtk4::init().unwrap();
        let stack = super::build();

        assert!(!stack.is_hhomogeneous());
        assert!(
            !stack.is_vhomogeneous(),
            "hidden tall pages must not determine the visible page height"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_8_dense_source_switch_animates_without_overlapping_readable_tables() {
        gtk4::init().unwrap();
        let stack = super::build();
        let library = gtk4::Label::new(Some("Music table"));
        let podcasts = gtk4::Label::new(Some("Podcast table"));
        stack.add_named(&library, Some("library"));
        stack.add_named(&podcasts, Some("podcasts"));
        stack.set_visible_child_name("podcasts");
        let window = gtk4::Window::builder().child(&stack).build();

        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(podcasts.is_mapped(), "precondition: Podcasts is visible");

        super::show_page(&stack, "library");
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(
            library.is_mapped(),
            "Music must be visible after the switch"
        );
        assert!(
            podcasts.opacity() == 0.0,
            "the outgoing Podcast table must become visually unreadable before Music fades in"
        );
        assert!(
            stack.is_transition_running(),
            "dense source surfaces must retain the normal location-switch motion"
        );
        window.close();
    }
}
