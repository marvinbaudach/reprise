//! Copy for trashing tracks cut from CUE sheets: trash acts on audio files, so
//! the dialog counts the files it moves to Trash apart from the CUE tracks it
//! only hides because their file holds other tracks too (CUE-11).

use crate::ui::strings::{plural, trash_confirmation_body};

/// The trash confirmation for `files` whole audio files and `hidden` CUE tracks
/// whose files stay because other tracks of theirs are not selected.
pub(super) fn trash_body(files: usize, hidden: usize) -> String {
    match (files, hidden) {
        (_, 0) => trash_confirmation_body(files),
        (0, _) => hidden_only_body(hidden),
        _ => format!("{} {}", trash_confirmation_body(files), hidden_note(hidden)),
    }
}

/// The note added to the delete choice when trashing would hide CUE tracks.
pub(super) fn choice_note(hidden: usize) -> Option<String> {
    (hidden > 0).then(|| hidden_note(hidden))
}

/// The result toast of a trash, followed by how many CUE tracks it hid.
pub(super) fn result_toast(toast: String, hidden: usize) -> String {
    if hidden == 0 {
        return toast;
    }
    let count = hidden.to_string();
    let note = plural(
        "{count} CUE track hidden",
        "{count} CUE tracks hidden",
        hidden,
        &[("count", &count)],
    );
    format!("{toast} · {note}")
}

fn hidden_only_body(hidden: usize) -> String {
    let count = hidden.to_string();
    plural(
        "This CUE track shares its music file with tracks that are not selected, so nothing is moved to Trash: it is hidden from the library instead.",
        "These {count} CUE tracks share their music files with tracks that are not selected, so nothing is moved to Trash: they are hidden from the library instead.",
        hidden,
        &[("count", &count)],
    )
}

fn hidden_note(hidden: usize) -> String {
    let count = hidden.to_string();
    plural(
        "{count} CUE track shares its music file with tracks that are not selected; it is hidden from the library instead.",
        "{count} CUE tracks share their music files with tracks that are not selected; they are hidden from the library instead.",
        hidden,
        &[("count", &count)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_11_the_trash_dialog_counts_files_apart_from_hidden_cue_tracks() {
        assert_eq!(trash_body(2, 0), trash_confirmation_body(2));
        assert!(trash_body(0, 3).contains("These 3 CUE tracks"));
        assert!(trash_body(0, 3).contains("nothing is moved to Trash"));
        let mixed = trash_body(1, 2);
        assert!(mixed.starts_with(&trash_confirmation_body(1)));
        assert!(mixed.contains("2 CUE tracks share their music files"));
        assert_eq!(choice_note(0), None);
        assert!(choice_note(1).unwrap().contains("1 CUE track shares"));
        assert_eq!(result_toast("done".into(), 0), "done");
        assert_eq!(result_toast("done".into(), 2), "done · 2 CUE tracks hidden");
    }
}
