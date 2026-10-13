macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const APP_NAME: &str = N_!("Reprise");
pub const ARTIST_PORTRAIT_CREDIT: &str = N_!("Photo: {author} · {license} · Wikimedia Commons");
pub const ARTIST_PORTRAIT_OPEN_SOURCE: &str = N_!("Open image source");

pub fn artist_portrait_credit(author: &str, license: &str) -> String {
    super::formatted(
        ARTIST_PORTRAIT_CREDIT,
        &[("author", author), ("license", license)],
    )
}
