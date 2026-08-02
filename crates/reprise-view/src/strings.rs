/// The plural half of a [`Message`]: the plural msgid together with the count
/// that selects between it and the singular.
///
/// The two live in one field on purpose. As two independent `Option`s a
/// producer could name a plural msgid without a count, and every renderer
/// would quietly fall back to `gettext` on the singular — showing untranslated
/// source text rather than a wrong plural form, because catalogs only carry
/// plural entries for msgids that actually go through `ngettext`. Nothing in
/// the type system would have objected. Here that state cannot be described.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plural {
    pub id: &'static str,
    pub count: u64,
}

/// Translatable text as a value: the msgid, its optional plural form, and its
/// named placeholders. Each surface chooses how to render it — GTK through
/// gettext, Android through `strings.xml`.
///
/// The msgids are `&'static str` because they are compile-time literals: that
/// is what `xgettext` extracts from these files, and it keeps the GTK path
/// free of an allocation per message. A UniFFI record cannot carry this shape
/// — records must own their data and cannot hold anonymous tuples — so the
/// Android surface converts into an owned record at the boundary rather than
/// this type being bent to serve both sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: &'static str,
    pub plural: Option<Plural>,
    pub args: Vec<(&'static str, String)>,
}

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub mod playlists;
pub mod scan;
