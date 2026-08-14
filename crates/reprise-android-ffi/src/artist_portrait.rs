use std::path::PathBuf;

use crate::MusicLibrary;

impl MusicLibrary {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn portrait_dir(&self) -> PathBuf {
        self.cache_root.join("artist-portraits")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portraits_live_under_the_app_cache_root_not_the_xdg_cache() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        let library = MusicLibrary::open_with_portrait_fetch(
            directory.path().to_str().unwrap(),
            cache.to_str().unwrap(),
            |_, _| panic!("locating the portrait directory must not fetch"),
        )
        .unwrap();

        let portrait_dir = library.portrait_dir();

        assert!(portrait_dir.starts_with(&cache));
        assert!(portrait_dir.ends_with("artist-portraits"));
    }
}
