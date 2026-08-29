//! Opt-in diagnostics for source-artwork startup and render passes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("REPRISE_MEASURE_SOURCE_ARTWORK").is_some())
}

pub(in crate::ui::podcasts) fn record_render_pass(
    passes: &AtomicU64,
    surface: &str,
    groups: usize,
    rows: usize,
) {
    if enabled() {
        eprintln!("{}", render_pass_line(passes, surface, groups, rows));
    }
}

fn render_pass_line(passes: &AtomicU64, surface: &str, groups: usize, rows: usize) -> String {
    let pass = passes.fetch_add(1, Ordering::Relaxed) + 1;
    format!(
        "source-artwork-measure phase=render surface={surface} pass={pass} groups={groups} rows={rows}"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    #[test]
    fn render_pass_measurement_names_surface_and_counts() {
        let passes = AtomicU64::new(0);

        assert_eq!(
            super::render_pass_line(&passes, "podcasts_view", 11, 266),
            "source-artwork-measure phase=render surface=podcasts_view pass=1 groups=11 rows=266"
        );
        assert_eq!(
            super::render_pass_line(&passes, "podcasts_view", 3, 18),
            "source-artwork-measure phase=render surface=podcasts_view pass=2 groups=3 rows=18"
        );
    }
}
