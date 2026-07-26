use std::path::PathBuf;

use super::LibraryLocation;

pub(super) fn configured_library_location(
    explicit: Option<PathBuf>,
    explicit_sha256: Option<String>,
    bundled: Option<&str>,
    bundled_sha256: Option<&str>,
    model_dir: Option<PathBuf>,
) -> LibraryLocation {
    let mut candidates = Vec::new();
    let has_explicit = explicit.is_some();
    if let Some(explicit) = explicit {
        candidates.push(explicit);
    } else if let Some(bundled) = bundled.filter(|path| !path.is_empty()) {
        candidates.push(PathBuf::from(bundled));
    }
    if let Some(model_dir) = model_dir {
        candidates.push(model_dir.join("libonnxruntime.so"));
    }
    let expected_sha256 = explicit_sha256.or_else(|| {
        (!has_explicit)
            .then_some(bundled_sha256)
            .flatten()
            .filter(|checksum| !checksum.is_empty())
            .map(str::to_owned)
    });
    LibraryLocation {
        candidates,
        expected_sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_runtime_path_and_checksum_form_one_verified_location() {
        let location = configured_library_location(
            None,
            None,
            Some("/app/lib/reprise/libonnxruntime.so.1.22.0"),
            Some("abc123"),
            Some(PathBuf::from("/data/reprise/models")),
        );
        assert_eq!(
            location.candidates,
            vec![
                PathBuf::from("/app/lib/reprise/libonnxruntime.so.1.22.0"),
                PathBuf::from("/data/reprise/models/libonnxruntime.so"),
            ]
        );
        assert_eq!(location.expected_sha256.as_deref(), Some("abc123"));
    }

    #[test]
    fn explicit_runtime_override_never_inherits_the_packaged_checksum() {
        let location = configured_library_location(
            Some(PathBuf::from("/opt/dev/libonnxruntime.so")),
            None,
            Some("/app/lib/reprise/libonnxruntime.so.1.22.0"),
            Some("package-hash"),
            None,
        );
        assert_eq!(
            location.candidates,
            vec![PathBuf::from("/opt/dev/libonnxruntime.so")]
        );
        assert_eq!(location.expected_sha256, None);
    }
}
