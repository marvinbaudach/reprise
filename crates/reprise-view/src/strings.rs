/// Translatable text as a value: the msgid, its optional plural msgid and
/// count, and its named placeholders. Each surface chooses how to render it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: &'static str,
    pub plural_id: Option<&'static str>,
    pub count: Option<u64>,
    pub args: Vec<(&'static str, String)>,
}

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub mod scan;
