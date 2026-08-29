//! Direct coverage for the source-image load-policy boundary.

use super::*;

fn request(url: &'static str) -> ArtworkRequest<'static> {
    ArtworkRequest::new(
        Some(url),
        None,
        (40, 40),
        true,
        CacheScope::Transient,
        StartupTiming::Immediate,
    )
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn source_image_load_policy_controls_queue_registration() {
    gtk4::init().unwrap();
    let before = source_artwork_measurement::registration_count_for_test();

    let _deferred = SourceImage::new_with_dimensions_when(
        request("https://images.test/load-policy-deferred.png"),
        "audio-input-microphone-symbolic",
        ArtworkLoadPolicy::Defer,
    );
    assert_eq!(
        source_artwork_measurement::registration_count_for_test(),
        before,
        "deferred construction must not register artwork"
    );

    let _immediate = SourceImage::new_with_dimensions_when(
        request("https://images.test/load-policy-immediate.png"),
        "audio-input-microphone-symbolic",
        ArtworkLoadPolicy::Load,
    );
    assert_eq!(
        source_artwork_measurement::registration_count_for_test(),
        before + 1,
        "immediate construction must register exactly one artwork request"
    );
}
