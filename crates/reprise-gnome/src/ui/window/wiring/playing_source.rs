use super::*;

pub(super) fn wire_playing_source(w: &RuntimeWiring<'_>) {
    let RuntimeWiring {
        app,
        window,
        player,
        info_panel,
        metadata_navigator,
        podcasts_view,
        youtube_view,
        radio_view,
        ..
    } = *w;
    playing_source_wiring::install(
        app,
        window,
        player.as_ref(),
        info_panel,
        metadata_navigator,
        podcasts_view,
        youtube_view,
        radio_view,
    );
    super::startup_report::mark("playing_source_wiring::install");
}
