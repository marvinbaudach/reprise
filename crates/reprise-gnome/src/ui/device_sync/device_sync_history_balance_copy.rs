use reprise_core::device_sync::sync_log::{Deviation, DeviationKind, RunRecord};

use super::device_sync_strings::{
    formatted, text, FAILED, KEPT_ORIGINAL, NOTHING_TO_TRANSFER, PLAYLIST_FAILED, REMOVED, SKIPPED,
};

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn sync_history_balance(run: &RunRecord) -> String {
    let mut parts = Vec::new();
    if run.planned > 0 || run.copied > 0 {
        parts.push(history_plural(
            "device sync history balance",
            "{copied} of {planned} copied",
            "{copied} of {planned} copied",
            run.planned.max(run.copied),
            &[
                ("copied", &run.copied.to_string()),
                ("planned", &run.planned.to_string()),
            ],
        ));
    }
    if run.skipped > 0 {
        parts.push(history_plural(
            "device sync history balance",
            "{count} skipped",
            "{count} skipped",
            run.skipped,
            &[("count", &run.skipped.to_string())],
        ));
    }
    if run.failed > 0 {
        parts.push(history_plural(
            "device sync history balance",
            "{count} failed",
            "{count} failed",
            run.failed,
            &[("count", &run.failed.to_string())],
        ));
    }
    if run.deleted > 0 {
        parts.push(history_plural(
            "device sync history balance",
            "{count} removed",
            "{count} removed",
            run.deleted,
            &[("count", &run.deleted.to_string())],
        ));
    }
    if let Some(detail) = &run.detail {
        parts.push(detail.clone());
    }
    if parts.is_empty() {
        return text(NOTHING_TO_TRANSFER);
    }
    parts.join(" · ")
}

pub fn sync_history_deviation_line(deviation: &Deviation) -> String {
    let kind = sync_history_deviation_kind(deviation.kind);
    formatted(
        N_!("{kind} · {path} — {detail}"),
        &[
            ("kind", &kind),
            ("path", &deviation.device_path),
            ("detail", &deviation.detail),
        ],
    )
}

fn sync_history_deviation_kind(kind: DeviationKind) -> String {
    text(match kind {
        DeviationKind::Skipped => SKIPPED,
        DeviationKind::Failed => FAILED,
        DeviationKind::Deleted => REMOVED,
        DeviationKind::ConversionFallback => KEPT_ORIGINAL,
        DeviationKind::PlaylistWriteFailed => PLAYLIST_FAILED,
    })
}

fn history_plural(
    context: &str,
    singular: &str,
    plural: &str,
    count: u32,
    values: &[(&str, &str)],
) -> String {
    crate::i18n::format_message(
        &crate::i18n::npgettext(context, singular, plural, count),
        values,
    )
}
