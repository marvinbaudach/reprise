use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CacheStats {
    pub(super) hits: u64,
    pub(super) misses: u64,
}

/// File-keyed texture LRU plus the request paths that resolved to each file.
pub(super) struct CoverCache<T> {
    capacity: usize,
    resolved: HashMap<String, PathBuf>,
    textures: HashMap<PathBuf, T>,
    lru: VecDeque<PathBuf>,
    stats: CacheStats,
}

impl<T: Clone> CoverCache<T> {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            resolved: HashMap::new(),
            textures: HashMap::new(),
            lru: VecDeque::new(),
            stats: CacheStats::default(),
        }
    }

    pub(super) fn resolve(&mut self, request: String, path: PathBuf) {
        self.resolved.insert(request, path);
    }

    pub(super) fn resolved_path(&self, request: &str) -> Option<PathBuf> {
        self.resolved.get(request).cloned()
    }

    #[cfg(test)]
    pub(super) fn texture_for_request(&mut self, request: &str) -> Option<(T, PathBuf)> {
        let path = self.resolved_path(request)?;
        self.texture(&path).map(|texture| (texture, path))
    }

    pub(super) fn texture(&mut self, path: &Path) -> Option<T> {
        let Some(texture) = self.textures.get(path).cloned() else {
            self.stats.misses = self.stats.misses.saturating_add(1);
            return None;
        };
        self.stats.hits = self.stats.hits.saturating_add(1);
        self.touch(path);
        Some(texture)
    }

    pub(super) fn insert_texture(&mut self, path: PathBuf, texture: T) {
        if self.textures.insert(path.clone(), texture).is_some() {
            self.touch(&path);
            return;
        }
        if self.textures.len() > self.capacity {
            if let Some(cold) = self.lru.pop_front() {
                self.textures.remove(&cold);
            }
        }
        self.lru.push_back(path);
    }

    pub(super) fn invalidate_requests(&mut self, paths: &[PathBuf]) {
        let prefixes: Vec<String> = paths
            .iter()
            .map(|path| format!("{}|", path.to_string_lossy()))
            .collect();
        let invalidated_files: HashSet<PathBuf> = self
            .resolved
            .iter()
            .filter(|(request, _)| prefixes.iter().any(|prefix| request.starts_with(prefix)))
            .map(|(_, path)| path.clone())
            .collect();
        if invalidated_files.is_empty() {
            return;
        }
        self.textures
            .retain(|path, _| !invalidated_files.contains(path));
        self.lru.retain(|path| !invalidated_files.contains(path));
        self.resolved
            .retain(|_, path| !invalidated_files.contains(path));
    }

    pub(super) fn invalidate_file(&mut self, path: &Path) {
        self.textures.remove(path);
        self.lru.retain(|candidate| candidate != path);
        self.resolved.retain(|_, candidate| candidate != path);
    }

    #[cfg(test)]
    pub(super) fn stats(&self) -> CacheStats {
        self.stats
    }

    #[cfg(test)]
    fn texture_count(&self) -> usize {
        self.textures.len()
    }

    #[cfg(test)]
    fn contains_texture(&self, path: &Path) -> bool {
        self.textures.contains_key(path)
    }

    fn touch(&mut self, path: &Path) {
        self.lru.retain(|candidate| candidate != path);
        self.lru.push_back(path.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};
    use std::path::{Path, PathBuf};

    use super::super::cover_cache::CoverCache;

    #[test]
    fn fifteen_tracks_sharing_one_file_decode_once_and_hit_fourteen_times() {
        let mut cache = CoverCache::new(256);
        let cover = PathBuf::from("/cache/album-cover.png");

        for track in 0..15 {
            let request = format!("/music/album/{track}.flac|96");
            cache.resolve(request.clone(), cover.clone());
            if cache.texture_for_request(&request).is_none() {
                cache.insert_texture(cover.clone(), "decoded");
            }
        }

        assert_eq!(cache.texture_count(), 1);
        assert_eq!(cache.stats().hits, 14);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn a_hit_moves_the_file_behind_newer_entries() {
        let mut cache = CoverCache::new(3);
        for name in ["a", "b", "c"] {
            cache.insert_texture(PathBuf::from(name), name);
        }

        assert_eq!(cache.texture(Path::new("a")), Some("a"));
        cache.insert_texture(PathBuf::from("d"), "d");

        assert!(cache.contains_texture(Path::new("a")));
        assert!(!cache.contains_texture(Path::new("b")));
    }

    #[test]
    fn invalidating_one_track_evicts_the_shared_cover_path_for_the_album() {
        let mut cache = CoverCache::new(256);
        let cover = PathBuf::from("/cache/shared-cover.png");
        cache.resolve("/music/album/one.flac|96".to_string(), cover.clone());
        cache.resolve("/music/album/two.flac|96".to_string(), cover.clone());
        cache.insert_texture(cover.clone(), "old pixels");

        cache.invalidate_requests(&[PathBuf::from("/music/album/one.flac")]);

        assert!(!cache.contains_texture(&cover));
        assert_eq!(
            cache.texture_for_request("/music/album/two.flac|96"),
            None,
            "the sibling track must not retain a route to stale album pixels"
        );
        cache.resolve("/music/album/two.flac|96".to_string(), cover.clone());
        cache.insert_texture(cover, "new pixels");
        assert_eq!(
            cache.texture_for_request("/music/album/two.flac|96"),
            Some(("new pixels", PathBuf::from("/cache/shared-cover.png")))
        );
    }

    #[test]
    fn a_five_hundred_row_down_and_back_trace_avoids_the_fifo_cliff() {
        let lru = scroll_trace_lru();
        let fifo = scroll_trace_fifo();

        assert_eq!((lru.hits, lru.misses), (36_136, 744));
        assert_eq!((fifo.hits, fifo.misses), (36_097, 783));

        assert!(
            lru.hits > fifo.hits,
            "LRU must preserve reused viewport files"
        );
        assert!(
            lru.misses < fifo.misses,
            "LRU must require fewer re-decodes"
        );
    }

    fn scroll_trace_lru() -> super::super::cover_cache::CacheStats {
        let mut cache = CoverCache::new(256);
        for start in (0..=460).chain((0..=460).rev()) {
            for row in start..start + 40 {
                let request = format!("/music/{row}.flac|96");
                let path = PathBuf::from(format!("/cache/{row}.png"));
                cache.resolve(request.clone(), path.clone());
                if cache.texture_for_request(&request).is_none() {
                    cache.insert_texture(path, row);
                }
            }
        }
        cache.stats()
    }

    fn scroll_trace_fifo() -> super::super::cover_cache::CacheStats {
        let mut cache = LegacyFifo::new(256);
        for start in (0..=460).chain((0..=460).rev()) {
            for row in start..start + 40 {
                cache.access(PathBuf::from(format!("/cache/{row}.png")), row);
            }
        }
        super::super::cover_cache::CacheStats {
            hits: cache.hits,
            misses: cache.misses,
        }
    }

    struct LegacyFifo {
        capacity: usize,
        textures: HashMap<PathBuf, usize>,
        order: VecDeque<PathBuf>,
        hits: u64,
        misses: u64,
    }

    impl LegacyFifo {
        fn new(capacity: usize) -> Self {
            Self {
                capacity,
                textures: HashMap::new(),
                order: VecDeque::new(),
                hits: 0,
                misses: 0,
            }
        }

        fn access(&mut self, path: PathBuf, value: usize) {
            if self.textures.contains_key(&path) {
                self.hits += 1;
                return;
            }
            self.misses += 1;
            if self.textures.len() >= self.capacity {
                if let Some(old) = self.order.pop_front() {
                    self.textures.remove(&old);
                }
            }
            self.order.push_back(path.clone());
            self.textures.insert(path, value);
        }
    }
}
