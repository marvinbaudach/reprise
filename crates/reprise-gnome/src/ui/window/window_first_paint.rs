use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::info_panel::InfoPanel;
use crate::ui::minimal_view::MinimalView;

pub(super) fn install_and_present(
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    split_view: &adw::OverlaySplitView,
    info_panel: &Rc<InfoPanel>,
    minimal_view: &Rc<MinimalView>,
) {
    let startup_report_armed = super::startup_report::mark("window_runtime_wiring::wire");
    super::responsive_side_panels::install(window, toast_overlay, split_view, info_panel);
    tracing::info!("main window built");
    let startup_window = minimal_view.active_window();
    let startup_completion = if startup_report_armed {
        let mapped = Rc::new(Cell::new(false));
        startup_window.connect_map(move |_| {
            if !mapped.replace(true) {
                super::startup_report::mark("window mapped");
            }
        });

        let first_frame_drawn = Rc::new(Cell::new(false));
        let first_idle_seen = Rc::new(Cell::new(false));
        let first_frame_for_tick = first_frame_drawn.clone();
        let first_idle_for_tick = first_idle_seen.clone();
        startup_window.add_tick_callback(move |_, frame_clock| {
            // A tick supplies the mapped window's frame clock. The report itself
            // waits until after paint and the first low-priority idle so
            // serialization cannot delay either milestone.
            let handler = Rc::new(RefCell::new(None));
            let handler_for_callback = handler.clone();
            let first_frame_drawn = first_frame_for_tick.clone();
            let first_idle_seen = first_idle_for_tick.clone();
            let id = frame_clock.connect_after_paint(move |frame_clock| {
                super::startup_report::mark("first frame drawn");
                first_frame_drawn.set(true);
                if first_idle_seen.get() {
                    super::startup_report::write_if_armed();
                }
                let id = handler_for_callback.borrow_mut().take();
                if let Some(id) = id {
                    frame_clock.disconnect(id);
                }
            });
            *handler.borrow_mut() = Some(id);
            gtk4::glib::ControlFlow::Break
        });
        Some((first_frame_drawn, first_idle_seen))
    } else {
        None
    };
    startup_window.present();
    super::startup_report::mark("window.present()");
    if let Some((first_frame_drawn, first_idle_seen)) = startup_completion {
        gtk4::glib::idle_add_local_full(gtk4::glib::Priority::LOW, move || {
            super::startup_report::mark("main loop first idle");
            first_idle_seen.set(true);
            if first_frame_drawn.get() {
                super::startup_report::write_if_armed();
            }
            gtk4::glib::ControlFlow::Break
        });
    }
}
