use super::*;

pub(super) fn wire_section_search(
    w: &RuntimeWiring<'_>,
    library_doctor: &Rc<library_doctor_ui::LibraryDoctorLauncher>,
) -> Rc<section_search_ui::SectionSearch> {
    let RuntimeWiring {
        search_entry,
        search,
        search_toggle,
        track_list,
        podcasts_view,
        youtube_view,
        radio_view,
        releases_view,
        concerts_view,
        ..
    } = *w;
    // SEARCH-8a: one transient query for the active view. Built before the
    // routing below so the first route already lands in the right scope.
    let section_search =
        super::section_search_ui::SectionSearch::new(search_entry, search, search_toggle);
    super::section_search_wiring::install(
        &section_search,
        &super::section_search_wiring::SectionSearchViews {
            track_list,
            podcasts_view,
            youtube_view,
            radio_view,
            releases_view,
            concerts_view,
            library_doctor,
        },
    );
    section_search
}
