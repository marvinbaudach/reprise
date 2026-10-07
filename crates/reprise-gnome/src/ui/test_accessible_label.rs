//! Test-only reader for a widget's accessible label.
//!
//! gtk4-rs has no getter for the label GTK holds, and
//! `gtk4::test_accessible_has_property` only reports that a label exists, so
//! an empty one passes. [`accessible_label_mismatch`] calls GTK's own
//! comparison, `gtk_test_accessible_check_property`, a variadic C function that
//! cannot be called without `unsafe`. That is why this module is named on
//! `check_frontend_allowlist`'s unsafe list in `scripts/check-architecture.sh`.
//! [`has_labelled_by_relation`] needs no `unsafe`: gtk4-rs wraps that call.

use gtk4::glib::translate::ToGlibPtr;
use gtk4::prelude::*;

/// `None` when `widget`'s accessible label property equals `expected`, otherwise
/// the label GTK holds.
pub(crate) fn accessible_label_mismatch(
    widget: &impl IsA<gtk4::Accessible>,
    expected: &str,
) -> Option<String> {
    let expected = std::ffi::CString::new(expected).expect("a label has no interior NUL");
    // SAFETY: `widget` is a live GtkAccessible and the variadic argument for
    // the label property is one NUL-terminated string, as GTK documents. The
    // returned string is owned by the caller and freed with g_free.
    unsafe {
        let actual = gtk4::ffi::gtk_test_accessible_check_property(
            widget.upcast_ref::<gtk4::Accessible>().to_glib_none().0,
            gtk4::ffi::GTK_ACCESSIBLE_PROPERTY_LABEL,
            expected.as_ptr(),
        );
        if actual.is_null() {
            return None;
        }
        let text = std::ffi::CStr::from_ptr(actual)
            .to_string_lossy()
            .into_owned();
        gtk4::glib::ffi::g_free(actual.cast());
        Some(text)
    }
}

/// Whether GTK holds a labelled-by relation for `widget`. A name read through
/// that relation wins over the label property, so a label that matches is no
/// proof the widget is announced by it.
pub(crate) fn has_labelled_by_relation(widget: &impl IsA<gtk4::Accessible>) -> bool {
    gtk4::test_accessible_has_relation(widget, gtk4::AccessibleRelation::LabelledBy)
}
