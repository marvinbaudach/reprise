use std::sync::Arc;

use super::*;

pub(super) fn wire_library_doctor(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let RuntimeWiring {
        conn,
        db_path,
        content_nav,
        content_stack,
        library_doctor_navigation,
        doctor_chrome,
        window,
        track_list,
        scan_controls,
        sidebar,
        toast_overlay,
        stats_view,
        ..
    } = *w;
    let refresh_doctor_views = {
        let stats = stats_view.clone();
        let conn = conn.clone();
        Rc::new(move || {
            stats.if_materialized(|view| view.refresh(&conn));
        }) as Rc<dyn Fn()>
    };
    let library_doctor = super::library_doctor_ui::LibraryDoctorLauncher::new(
        super::library_doctor_ui::LibraryDoctorContext {
            conn,
            db_path,
            content_navigation: content_nav,
            content_stack,
            doctor_navigation: library_doctor_navigation,
            doctor_chrome,
            window,
            track_list,
            scan_controls,
            fingerprint: Arc::new(reprise_platform_linux::fingerprint::GstreamerFingerprintBackend),
            sidebar,
            toast_overlay,
            refresh_views: refresh_doctor_views,
        },
    );
    super::startup_report::mark("LibraryDoctorLauncher::new");
    library_doctor.observe_tag_writes_from(track_list);
    {
        let library_doctor = Rc::downgrade(&library_doctor);
        stats_view.on_materialized(move |stats| {
            stats.set_on_unify_spellings(move |ids| {
                if let Some(library_doctor) = library_doctor.upgrade() {
                    library_doctor.open_for_selection(ids);
                }
            });
        });
    }
    assert!(
        scratch.library_doctor.set(library_doctor).is_ok(),
        "library doctor wiring must run once"
    );
}
