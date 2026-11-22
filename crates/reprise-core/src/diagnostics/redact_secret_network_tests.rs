//! Round 3 of the review of #1374: a secret word followed by a value in any
//! separator form takes the rest of the line, an address is found inside a
//! word, and the user's names go wherever they stand in an identifier.
//! Context: music `/home/marvin/Music`, home `/home/marvin`, user `marvin`,
//! host `my-nas`.

use super::field_policy::KEEP_FIELDS;
use super::redact_secrets::is_secret_key;
use super::{redact_log_message, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("marvin".into()),
        hostname: Some("my-nas".into()),
    }
}

fn redact_with(context: &RedactionContext, input: &str, leaks: &[&str]) -> String {
    let output = redact_log_message(input, context);
    let lowered = output.to_lowercase();
    for leaked in leaks {
        assert!(
            !lowered.contains(&leaked.to_lowercase()),
            "{leaked:?} leaked from {input:?}: {output:?}"
        );
    }
    assert_eq!(
        redact_log_message(&output, context),
        output,
        "not idempotent: {input:?}"
    );
    output
}

fn assert_no_leak(input: &str, leaks: &[&str]) -> String {
    redact_with(&context(), input, leaks)
}

fn named(user: &str, host: &str) -> RedactionContext {
    RedactionContext {
        music_dir: None,
        home_dir: None,
        username: Some(user.into()),
        hostname: Some(host.into()),
    }
}

// Secrets

#[test]
fn more_secret_keys_take_their_value() {
    for input in [
        "Access-Key: AKIAIOSFODNN7EXAMPLE",
        "X-Access-Key: AKIAIOSFODNN7EXAMPLE",
        "Private-Key: AKIAIOSFODNN7EXAMPLE",
        "AccessKey: AKIAIOSFODNN7EXAMPLE",
        "passcode: AKIAIOSFODNN7EXAMPLE",
        "psk: AKIAIOSFODNN7EXAMPLE",
        "pw: AKIAIOSFODNN7EXAMPLE",
        "jwt: AKIAIOSFODNN7EXAMPLE",
        "sk: AKIAIOSFODNN7EXAMPLE",
        "Passwort: AKIAIOSFODNN7EXAMPLE",
        "lastfm sk=AKIAIOSFODNN7EXAMPLE",
        "password＝AKIAIOSFODNN7EXAMPLE",
        "ｐａｓｓｗｏｒｄ: AKIAIOSFODNN7EXAMPLE",
    ] {
        assert_no_leak(input, &["AKIAIOSFODNN7EXAMPLE"]);
    }
}

#[test]
fn a_secret_word_glued_to_any_separator_takes_the_rest_of_the_line() {
    for input in [
        "password(AKIAIOSFODNN7EXAMPLE)",
        "password[AKIAIOSFODNN7EXAMPLE]",
        "password/AKIAIOSFODNN7EXAMPLE",
        "password|AKIAIOSFODNN7EXAMPLE",
        "password;AKIAIOSFODNN7EXAMPLE",
        "password@AKIAIOSFODNN7EXAMPLE",
        "token(AKIAIOSFODNN7EXAMPLE) and more",
        "login failed, token|AKIAIOSFODNN7EXAMPLE|x",
    ] {
        let output = assert_no_leak(input, &["AKIAIOSFODNN7EXAMPLE", "more", "|x"]);
        // `password/x` is a path first: the slash word takes the line.
        assert!(
            output.contains("$REDACTED") || output.contains('…'),
            "{output}"
        );
    }
}

#[test]
fn a_secret_word_followed_by_a_value_in_running_text_takes_the_line() {
    for input in [
        "--username bob --token AKIAIOSFODNN7EXAMPLE",
        "passwords hunter2 hunter3",
        "sent Basic dXNlcjpwYXNz to server",
        "token AKIAIOSFODNN7EXAMPLE expired",
        "api key AKIAIOSFODNN7EXAMPLE invalid",
        "X-Api-Key AKIAIOSFODNN7EXAMPLE",
        "apikey AKIAIOSFODNN7EXAMPLE",
        "last.fm session key 0123456789abcdef0123456789abcdef",
        "secret_key AKIAIOSFODNN7EXAMPLE",
        "username bob",
        "password -> hunter2",
        "password, hunter2",
    ] {
        assert_no_leak(
            input,
            &[
                "AKIAIOSFODNN7EXAMPLE",
                "bob",
                "hunter2",
                "hunter3",
                "dXNlcjpwYXNz",
                "0123456789abcdef",
                "expired",
                "invalid",
            ],
        );
    }
}

#[test]
fn prose_that_names_a_network_takes_the_rest_of_the_line() {
    assert_no_leak(
        "connected to wifi network Pink Floyd Guest",
        &["Pink", "Floyd", "Guest"],
    );
    assert_no_leak("ssid Home Net 5G", &["Home", "5G"]);
    assert_no_leak("joined Wi-Fi network My Home WiFi", &["Home", "WiFi"]);
}

#[test]
fn a_name_the_redactor_replaced_does_not_hand_its_value_to_the_next_pass() {
    // `marvin:hunter2` is a credential pair; once the name is a marker the
    // value after it must still go. `$HOST:445` is an address and a port.
    for input in [
        "auth failed for marvin:hunter2",
        "auth failed for $USER:hunter2",
        "login my-nas:swordfish",
        "{\"marvin\": \"hunter2\"}",
        "\"$USER\": hunter2 and more",
    ] {
        assert_no_leak(input, &["hunter2", "swordfish", "more"]);
    }
    for input in ["connect to $HOST:445 failed", "opened by $USER: failed"] {
        assert_eq!(redact_log_message(input, &context()), input);
    }
}

/// Kept keys that already read as secret keys before round 3, in the colon
/// form only: `exit_code: 1` is told apart by its value, and `signature` is
/// read as the secret it may name. The `=` form of all three is kept by the
/// policy.
const COLON_FORM_COLLISIONS: [&str; 3] = ["exit_code", "signature", "started_signature"];

#[test]
fn a_kept_field_never_reads_as_a_secret() {
    // The key rules read the first word of a token. A key the policy keeps for
    // its readability must not collide with the secret words, in any form the
    // free-text pass reads, except the two that predate this round.
    let mut collisions: Vec<&str> = KEEP_FIELDS
        .iter()
        .copied()
        .filter(|field| is_secret_key(field))
        .collect();
    collisions.sort_unstable();
    assert_eq!(
        collisions, COLON_FORM_COLLISIONS,
        "a new kept-key collision"
    );
    for field in KEEP_FIELDS {
        let colon_form = !COLON_FORM_COLLISIONS.contains(field);
        let forms = [
            (true, format!("{field}=3")),
            (colon_form, format!("{field}: 3")),
            (true, format!("{field} 3 more")),
            (true, format!("{field}(3)")),
        ];
        for (checked, text) in forms {
            if checked {
                assert_eq!(
                    redact_log_message(&text, &context()),
                    text,
                    "kept field {field:?} is read as a secret in {text:?}"
                );
            }
        }
    }
}

#[test]
fn ordinary_text_around_secret_looking_words_stays_readable() {
    for input in [
        "playback failed: the stream ended after 3s",
        "scan finished in 5s count=3",
        "the pin is not used here",
        "decoded the code path without an error",
    ] {
        assert_eq!(redact_log_message(input, &context()), input);
    }
}

// Network

#[test]
fn an_address_glued_to_a_word_is_found() {
    for input in [
        "peer:192.168.1.5:8080 failed",
        "tcp:192.168.1.5:445 refused",
        "host 192.168.1.5_backup",
        "connect 192.168.1.5%eth0",
        "connect 192.168.1.5:http",
        "connect v192.168.1.5",
        "connect ip6-fe80::1",
        "connect peer:fe80::1%wlan0",
        "connect 192。168。1。5",
        "connect 192．168．1．5",
        "connect ip-192-168-1-5",
        "connect ip_192_168_1_5 up",
        "resolved my-nas2 to 192.168.1.5_eth0",
        "connecting to tcp:192.168.1.5:445",
        "range 192.168.1.5-192.168.1.9",
    ] {
        let output = assert_no_leak(input, &["192", "fe80", "168"]);
        assert!(output.contains("$IP"), "{output}");
    }
}

#[test]
fn an_address_after_an_address_like_key_is_found_in_any_shape() {
    for input in [
        "host=3232235781 failed",
        "ip 3232235781",
        "ip: 0xC0A80105",
        "addr=0xc0a80105",
        "peer 3232235781",
        "server=0300.0250.0001.0005",
        "remote 0xC0.0xA8.0x01.0x05",
        "gateway=3232235777",
    ] {
        assert_no_leak(
            input,
            &["3232235781", "c0a80105", "0300", "0xC0", "3232235777"],
        );
    }
}

#[test]
fn a_mac_address_in_any_separator_form_is_found() {
    for input in [
        "mac aabb.ccdd.eeff ok",
        "mac AA_BB_CC_DD_EE_FF ok",
        "sink bluez_sink.AA_BB_CC_DD_EE_FF.a2dp_sink selected",
        "sink bluez_output.AA_BB_CC_DD_EE_FF.1 selected",
        "device dev_AA_BB_CC_DD_EE_FF connected",
        "device dev_aa_bb_cc_dd_ee_ff",
        "mac AA:BB:CC:DD:EE:FF, aa-bb-cc-dd-ee-ff.",
    ] {
        let output = assert_no_leak(input, &["AA_BB", "aabb", "ccdd", "EE_FF", "EE:FF", "ee-ff"]);
        assert!(output.contains("$MAC") || output.contains('…'), "{output}");
    }
}

#[test]
fn rust_paths_and_ordinary_words_are_not_addresses() {
    for input in [
        "at <gtk4::auto::application::Application as glib::object::Cast>::run",
        "core::f64::consts::PI and db::fa and cafe::beef",
        "version=1.2.3.4 and 1.2.3 and 1.2.3.4.5 and 192.168.1.256",
    ] {
        let output = redact_log_message(input, &context());
        assert!(!output.contains("$IP"), "{input:?} -> {output:?}");
    }
}

// Names

#[test]
fn the_user_name_goes_inside_an_identifier() {
    for input in [
        "marvin_laptop",
        "marvin2",
        "user_marvin",
        "xmarvin",
        "marvinbaudach",
        "/var/lib/marvin_backup",
        "Marvin-PC",
        "marvin.local",
        "$marvin",
        "$marvin's files",
    ] {
        assert_no_leak(input, &["marvin"]);
    }
}

#[test]
fn a_dollar_word_is_exempt_only_when_it_is_exactly_one_of_our_markers() {
    let context = named("marvin", "MyNas");
    for input in ["connect to $MyNas failed", "$MYNAS and $mynas and MyNas"] {
        redact_with(&context, input, &["mynas"]);
    }
    assert_eq!(
        redact_log_message("failed for $USER on $HOST at $IP via $MAC", &context),
        "failed for $USER on $HOST at $IP via $MAC"
    );
}

#[test]
fn the_short_host_of_a_fully_qualified_host_name_goes() {
    let context = named("marvin", "my-nas.example.org");
    redact_with(
        &context,
        "connect my-nas and my-nas.example.org and MY-NAS:445",
        &["my-nas", "example.org"],
    );
}

#[test]
fn a_name_that_is_also_a_marker_word_never_changes_a_marker() {
    for (user, host) in [
        ("user", "host"),
        ("redacted", "ip"),
        ("e", "a"),
        ("home", "mac"),
    ] {
        let context = named(user, host);
        let input =
            "failed for $USER on $HOST at $IP via $MAC in $HOME/…\nand $XDG_MUSIC_DIR/…\n$REDACTED";
        let once = redact_log_message(input, &context);
        assert_eq!(redact_log_message(&once, &context), once, "{user}/{host}");
    }
}
