use std::time::Duration;

use crate::net::client::AgentPolicy;

use super::{sidecar, wikimedia};

fn imageinfo(author: &str, license: &str, license_url: &str, file_page_url: &str) -> String {
    serde_json::json!({
        "query": {"pages": {"1": {"imageinfo": [{
            "thumburl": "https://upload.wikimedia.org/portrait.jpg",
            "descriptionurl": file_page_url,
            "extmetadata": {
                "Artist": {"value": author},
                "LicenseShortName": {"value": license},
                "LicenseUrl": {"value": license_url}
            }
        }]}}}
    })
    .to_string()
}

#[test]
fn net_9_bare_q_is_not_a_wikidata_entity_id() {
    let json = r#"{"relations":[{"url":{"resource":"https://www.wikidata.org/wiki/Q"}}]}"#;

    assert_eq!(wikimedia::parse_wikidata_qid(json), Ok(None));
}

#[test]
fn net_9_wikimedia_agent_is_https_only_and_refuses_redirects() {
    assert_eq!(
        wikimedia::agent_policy(),
        AgentPolicy {
            timeout: Duration::from_secs(15),
            status_as_error: true,
            https_only: true,
            max_redirects: Some(0),
            proxy_from_env: true,
        }
    );
}

#[test]
fn net_9_credit_decodes_entities_once_keeps_lone_angles_and_strips_controls() {
    let json = imageinfo(
        "<b>A&amp;B</b> &amp;lt; &#39;Photo&#x67;rapher' <lone\u{0007}",
        "<i>CC &amp; BY &lt;4.0&gt;</i>\u{0000}",
        "https://creativecommons.org/licenses/by/4.0",
        "https://commons.wikimedia.org/wiki/File:Portrait.jpg",
    );

    let credit = wikimedia::parse_commons_imageinfo(&json)
        .unwrap()
        .unwrap()
        .credit;

    assert_eq!(credit.author, "A&B &lt; 'Photographer' <lone");
    assert_eq!(credit.license, "CC & BY <4.0>");
}

#[test]
fn net_9_credit_fields_are_bounded() {
    let json = imageinfo(
        &"a".repeat(300),
        &"l".repeat(300),
        &format!("https://creativecommons.org/{}", "x".repeat(3_000)),
        &format!(
            "https://commons.wikimedia.org/wiki/File:{}",
            "x".repeat(3_000)
        ),
    );

    let credit = wikimedia::parse_commons_imageinfo(&json)
        .unwrap()
        .unwrap()
        .credit;

    assert_eq!(credit.author.chars().count(), 256);
    assert_eq!(credit.license.chars().count(), 256);
    assert!(credit.license_url.len() <= 2_048);
    assert!(credit.file_page_url.len() <= 2_048);
}

#[test]
fn net_9_file_page_requires_https_commons_and_license_url_is_allowlisted() {
    for invalid_file_page in [
        "http://commons.wikimedia.org/wiki/File:Portrait.jpg",
        "https://commons.wikimedia.org.evil.example/File:Portrait.jpg",
        "https://example.org/wiki/File:Portrait.jpg",
    ] {
        assert_eq!(
            wikimedia::parse_commons_imageinfo(&imageinfo(
                "Photo",
                "CC BY 4.0",
                "https://creativecommons.org/licenses/by/4.0",
                invalid_file_page,
            )),
            Ok(None)
        );
    }

    let credit = wikimedia::parse_commons_imageinfo(&imageinfo(
        "Photo",
        "Custom licence",
        "https://evil.example/license",
        "https://commons.wikimedia.org/wiki/File:Portrait.jpg",
    ))
    .unwrap()
    .unwrap()
    .credit;
    assert_eq!(credit.license, "Custom licence");
    assert!(credit.license_url.is_empty());
}

#[test]
fn net_9_untrusted_sidecar_revalidates_credit_urls_before_ffi_loading() {
    let dir = tempfile::tempdir().unwrap();
    let name = "Band";
    let record = sidecar::PortraitRecord::commons(sidecar::PortraitCredit {
        author: "Photo".to_owned(),
        license: "Custom licence".to_owned(),
        license_url: "https://evil.example/license".to_owned(),
        file_page_url: "https://commons.wikimedia.org/wiki/File:Portrait.jpg".to_owned(),
    });
    std::fs::write(
        sidecar::path(dir.path(), name),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();

    let credit = super::load_cached_credit_from(name, dir.path()).unwrap();

    assert_eq!(credit.license, "Custom licence");
    assert!(credit.license_url.is_empty());

    let unsafe_record = sidecar::PortraitRecord::commons(sidecar::PortraitCredit {
        file_page_url: "https://evil.example/file".to_owned(),
        ..credit
    });
    std::fs::write(
        sidecar::path(dir.path(), name),
        serde_json::to_vec(&unsafe_record).unwrap(),
    )
    .unwrap();
    assert!(super::load_cached_credit_from(name, dir.path()).is_none());
}
