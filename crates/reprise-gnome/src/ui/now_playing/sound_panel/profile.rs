use gtk4::prelude::*;

use reprise_core::sound_features::SoundFeatures;
use reprise_core::sound_stats::SoundStats;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ProfilePositions {
    pub(super) timbre: f32,
    pub(super) dynamics: f32,
    pub(super) tempo: Option<f32>,
}

pub(super) fn positions(
    features: &SoundFeatures,
    stats: &SoundStats,
    include_tempo: bool,
) -> ProfilePositions {
    ProfilePositions {
        timbre: stats.centroid_mean.percentile(features.centroid_mean),
        dynamics: stats.frame_crest_db.percentile(features.frame_crest_db),
        tempo: include_tempo
            .then(|| features.tempo.map(|tempo| stats.tempo.percentile(tempo)))
            .flatten(),
    }
}

pub(super) struct Profile {
    root: gtk4::Box,
    timbre: gtk4::ProgressBar,
    dynamics: gtk4::ProgressBar,
    tempo: gtk4::ProgressBar,
    tempo_row: gtk4::Box,
}

impl Profile {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.add_css_class("reprise-sound-profile");
        let heading = gtk4::Label::builder()
            .label(crate::ui::strings::text(crate::ui::strings::SOUND_PROFILE))
            .xalign(0.0)
            .build();
        heading.add_css_class("heading");
        root.append(&heading);
        let (timbre_row, timbre) = axis(crate::ui::strings::SOUND_TIMBRE_AXIS);
        let (dynamics_row, dynamics) = axis(crate::ui::strings::SOUND_DYNAMICS_AXIS);
        let (tempo_row, tempo) = axis(crate::ui::strings::SOUND_TEMPO_AXIS);
        root.append(&timbre_row);
        root.append(&dynamics_row);
        root.append(&tempo_row);
        Self {
            root,
            timbre,
            dynamics,
            tempo,
            tempo_row,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn render(&self, positions: ProfilePositions) {
        set_position(&self.timbre, positions.timbre);
        set_position(&self.dynamics, positions.dynamics);
        self.tempo_row.set_sensitive(positions.tempo.is_some());
        set_position(&self.tempo, positions.tempo.unwrap_or(0.0));
    }
}

fn axis(label: &str) -> (gtk4::Box, gtk4::ProgressBar) {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    let label = gtk4::Label::builder()
        .label(crate::ui::strings::text(label))
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    label.add_css_class("caption");
    let bar = gtk4::ProgressBar::new();
    bar.add_css_class("reprise-sound-axis");
    root.append(&label);
    root.append(&bar);
    (root, bar)
}

fn set_position(bar: &gtk4::ProgressBar, percentile: f32) {
    bar.set_fraction(f64::from(percentile.clamp(0.0, 100.0)) / 100.0);
}
