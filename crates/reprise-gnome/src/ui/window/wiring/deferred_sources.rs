use super::*;

pub(super) fn wire_deferred_sources(w: &RuntimeWiring<'_>) {
    let RuntimeWiring {
        preferences,
        cover_batch,
        toast_overlay,
        stats_view,
        concerts_view,
        releases_view,
        podcasts_view,
        youtube_view,
        radio_view,
        ..
    } = *w;
    deferred_source_wiring::install(
        preferences,
        cover_batch,
        toast_overlay,
        stats_view,
        concerts_view,
        releases_view,
        podcasts_view,
        youtube_view,
        radio_view,
    );
}
