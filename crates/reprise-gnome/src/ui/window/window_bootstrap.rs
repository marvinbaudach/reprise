//! Appearance and persisted-window bootstrap kept outside the composition root.

use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::session::SessionState;
use reprise_core::waveform::RenderDataBackend;
use std::sync::Arc;

use crate::ui::first_run::FirstRunDecision;

const MIN_WIDTH: i32 = 600;
const MIN_HEIGHT: i32 = 400;

pub(super) struct Bootstrap {
    pub window: adw::ApplicationWindow,
    pub session_state: SessionState,
    pub first_run_decision: FirstRunDecision,
}

pub(super) fn prepare(app: &adw::Application, conn: &Db) -> Bootstrap {
    let accent_source = reprise_core::library::settings::get_setting(
        conn,
        crate::ui::style::accent::ACCENT_SOURCE_SETTING_KEY,
    )
    .ok()
    .flatten()
    .as_deref()
    .map_or(
        crate::ui::style::accent::AccentSource::DEFAULT,
        crate::ui::style::accent::AccentSource::from_id,
    );
    crate::ui::style::set_accent_source(accent_source);
    let stored = reprise_core::library::settings::get_setting(
        conn,
        crate::ui::style::theme::THEME_SETTING_KEY,
    )
    .ok()
    .flatten();
    let theme = stored
        .as_deref()
        .and_then(crate::ui::style::theme::Theme::from_id)
        .unwrap_or(crate::ui::style::theme::Theme::DEFAULT);
    crate::ui::style::set_theme(theme);
    crate::ui::style::set_color_scheme(reprise_core::library::settings::get_color_scheme(conn));
    // Theme, accent source, and appearance are now final, so installation
    // loads the palette provider once instead of repainting it for each value.
    crate::ui::style::install();
    crate::ui::startup_report::mark("style::install");

    let session_state = crate::ui::session_restore::load(conn);
    let first_run_decision = crate::ui::first_run::initial_decision(conn);
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(crate::ui::strings::text(crate::ui::strings::APP_NAME))
        .default_width(session_state.window_width)
        .default_height(session_state.window_height)
        .width_request(MIN_WIDTH)
        .height_request(MIN_HEIGHT)
        .build();
    super::focus_evidence::install(&window);
    crate::ui::session_restore::apply_initial_geometry(&window, &session_state);

    Bootstrap {
        window,
        session_state,
        first_run_decision,
    }
}

pub(super) fn waveform_backend() -> Arc<dyn RenderDataBackend> {
    Arc::new(reprise_platform_linux::waveform::GstreamerWaveformBackend)
}
