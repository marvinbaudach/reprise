//! Artist News runtime: the module's enabled state and its subscribers.
//!
//! The background fetch worker went with its last caller. Frame 22a, which
//! will show news in the artist detail view, adds a request path again when it
//! has a consumer. The Now Playing panel deliberately has no news consumer.

pub(in crate::ui) mod artist_news_worker;
