//! Round 3 of the review of #1374: a recognised path takes the rest of its
//! line, whatever it is called, whatever quotes it holds and whatever sits in
//! front of it. Same context as `redact_review_tests`: music folder
//! `/home/marvin/Music`, home `/home/marvin`, user `marvin`, host `my-nas`.

use super::{redact_log_message, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("marvin".into()),
        hostname: Some("my-nas".into()),
    }
}

fn redacted(input: &str) -> String {
    redact_log_message(input, &context())
}

/// None of `leaks` survives (compared without case), and a second pass
/// changes nothing.
fn assert_no_leak(input: &str, leaks: &[&str]) -> String {
    let output = redacted(input);
    let lowered = output.to_lowercase();
    for leaked in leaks {
        assert!(
            !lowered.contains(&leaked.to_lowercase()),
            "{leaked:?} leaked from {input:?}: {output:?}"
        );
    }
    assert_eq!(redacted(&output), output, "not idempotent: {input:?}");
    output
}

#[test]
fn a_key_glued_to_an_absolute_path_does_not_shorten_it() {
    let names = &["Pink", "Floyd", "Wall", "flesh", "wish", "were", "Albums"];
    for input in [
        "scan: path=/run/media/marvin/DISK/Music/Pink Floyd/The Wall/01 - in the flesh.flac",
        "location=/run/media/marvin/SANDISK/Music/Pink Floyd/wish you were here.flac",
        "scan path=/mnt/usb/Albums/Pink Floyd/wish you were here",
        "src=/media/marvin/Disk/Rock/pink floyd and more",
        "dir:/mnt/NAS/Music/pink floyd wish you were here",
        "dir=/mnt/NAS/Share/pink floyd wish you were here",
        "uri=file:///mnt/NAS/Share/pink floyd wish you were here",
        "--output=/mnt/NAS/Share/pink floyd wish you were here",
        "moved to=/srv/Media/Albums/the dark side",
        "filesrc location=/run/media/marvin/Elements/Music/pink floyd/wish you were here.flac ! decodebin",
        "path:/run/media/marvin/Elements/Music/pink floyd wish you were here",
        "db=/mnt/Data/Reprise/my secret library name",
    ] {
        let output = assert_no_leak(input, names);
        assert!(!output.contains("floyd"), "{output}");
    }
}

#[test]
fn a_path_under_any_root_takes_the_rest_of_the_line() {
    for input in [
        "/tank/audio/Pink Floyd/Wish you were here/01 - shine on you crazy diamond.flac",
        "/tank/audio/pink floyd/wish you were here",
        "/var/mnt/nas/The Dark side of the Moon",
        "/opt/media/Wish you Were Here",
        "cannot open /nas/media/Wish you Were Here: permission denied",
        "/srv/Pink Floyd the wall",
        "/mnt/NAS/Share/pink floyd wish you were here",
    ] {
        assert_no_leak(
            input,
            &[
                "tank", "Pink", "Floyd", "Wish", "were", "shine", "diamond", "Dark", "Moon",
                "denied", "wall", "here",
            ],
        );
    }
}

#[test]
fn a_path_followed_by_a_caseless_script_does_not_leak_the_tail() {
    assert_no_leak("/tank/audio/핑크 플로이드 벽", &["핑크", "플로이드", "벽"]);
    assert_no_leak(
        "/tank/audio/ピンク フロイド 狂気",
        &["ピンク", "フロイド", "狂気"],
    );
    assert_no_leak("/tank/audio/פינק פלויד החומה", &["פלויד", "החומה"]);
    assert_no_leak("핑크 플로이드/벽/01.flac", &["핑크", "플로이드", "벽"]);
    assert_no_leak(
        "ピンク フロイド/狂気/01 time.flac",
        &["ピンク", "フロイド", "狂気"],
    );
}

#[test]
fn an_apostrophe_or_an_inner_quote_does_not_end_a_path() {
    for (input, names) in [
        (
            "Input #0, flac, from '/home/marvin/Music/Bon Jovi/Slippery When Wet/03 Livin' on a Prayer (Live at Wembley 1988).flac':",
            &["Prayer", "Wembley", "Livin"][..],
        ),
        (
            "could not decode '/home/marvin/Music/Bon Jovi/03 Livin' on a prayer.flac'",
            &["prayer", "Livin"][..],
        ),
        (
            "open '/home/marvin/Music/Neil Young/Rockin' in the free world.mp3' failed",
            &["Rockin", "free", "world", "failed"][..],
        ),
        (
            "open '/run/media/marvin/DISK/Otis Redding/Sittin' on the dock of the bay.flac' failed",
            &["Sittin", "dock", "bay"][..],
        ),
        (
            "Could not open file \"/home/marvin/Music/David Bowie/\"Heroes\" live in berlin.flac\" for reading.",
            &["Heroes", "berlin", "reading"][..],
        ),
        (
            "Could not open file \"/home/marvin/Music/Frankie/Relax (12\" sex mix).flac\" for reading.",
            &["Relax", "sex", "mix"][..],
        ),
        (
            "Could not open file \"/home/marvin/Music/New Order/Blue Monday (12\" Version).flac\" for reading.",
            &["Blue", "Monday", "Version"][..],
        ),
        (
            "open \"/home/marvin/Music/David Bowie/\\\"Heroes\\\" live in berlin.flac\" failed",
            &["Heroes", "berlin"][..],
        ),
        (
            "open '/mnt/usb/Singin' in the rain' failed",
            &["Singin", "rain"][..],
        ),
        (
            "could not read '/home/marvin/Music/Sinéad O'Connor/nothing compares.flac': not found",
            &["Connor", "nothing", "compares"][..],
        ),
        (
            "could not read \"/home/marvin/Music/7\" vinyl rips and other stuff\"",
            &["vinyl", "rips", "stuff"][..],
        ),
        (
            "could not read “/mnt/usb/Pink Floyd zqxa” failed",
            &["Floyd", "zqxa"][..],
        ),
        (
            "open ‘/mnt/usb/Guns N’ Roses’ failed",
            &["Roses", "Guns"][..],
        ),
    ] {
        assert_no_leak(input, names);
    }
}

#[test]
fn a_path_keeps_its_line_even_when_a_known_key_follows() {
    for input in [
        "open /home/marvin/Music/Pink Floyd/Live count=3 secret sessions/01.flac",
        "open /home/marvin/Music/My Mix [mode=shuffle] Summer Party Hits",
        "open /home/marvin/Music/Mix (id=5) Summer Party",
        "open /home/marvin/Music/Album kind=Live at Wembley",
        "open /home/marvin/Music/AC ~/DC/Back in Black",
        "open /mnt/usb/Pink Floyd count=3 Wembley",
    ] {
        assert_no_leak(
            input,
            &[
                "Floyd", "Summer", "Party", "Hits", "Wembley", "DC", "Black", "sessions",
            ],
        );
    }
}

#[test]
fn a_path_ends_at_its_line_and_the_next_line_stays_readable() {
    let output = redacted("open /mnt/usb/Pink Floyd\nscan finished in 5s");

    assert!(!output.contains("Pink"), "{output}");
    assert!(output.ends_with("\nscan finished in 5s"), "{output:?}");
}

#[test]
fn windows_and_encoded_paths_take_the_line() {
    for input in [
        "open \"C:\\Users\\marvin\\Music\\Pink Floyd\\x.flac\" failed",
        "open C:\\Users\\bob\\Documents\\My Stuff\\Pink Floyd failed",
        "open C:/Users/bob/Documents/My Stuff/Pink Floyd failed",
        "open \\\\nas\\music\\Pink Floyd\\x",
        "open \\\\?\\C:\\Users\\bob\\Pink Floyd",
        "dir %USERPROFILE%\\Music\\Pink Floyd",
        "dir %APPDATA%\\Reprise\\Pink Floyd",
        "path %2Fhome%2Fmarvin%2FMusic%2FPink%20Floyd",
        "%2Fhome%2Fmarvin%2FMusic%2FPink%20Floyd%2Fx.flac",
        "path %252Fhome%252Fmarvin%252FMusic%252FPink%2520Floyd",
        "／home／marvin／Music／Pink Floyd",
        "path＝／mnt／usb／Pink Floyd",
        "Music\\Pink Floyd\\The Wall\\01.flac failed",
        "Music\\pink floyd\\the wall\\01.flac failed",
        "Documents\\pink floyd\\album notes",
    ] {
        assert_no_leak(
            input,
            &["Pink", "Floyd", "Users", "Documents", "My Stuff", "Wall"],
        );
    }
}

#[test]
fn a_relative_path_with_spaces_and_a_lowercase_tail_does_not_leak() {
    for input in [
        "copy to phone: Pink Floyd/the wall/01 in the flesh.flac",
        "Pink Floyd/the wall/01 in the flesh.flac",
        "pink floyd/the wall/01 in the flesh.flac",
        "copied Various Artists/Now 42/cd 1/03 - some song.mp3 to device",
        "Earth, Wind & Fire/Greatest Hits/01 september.flac",
        "sync Pink Floyd/The Wall",
        "copied 3 files to Pink Floyd/The Wall",
        "copied ~3 files to Pink Floyd/The Wall",
        "took $5 for Pink Floyd/The Wall",
        "to=Pink Floyd/the wall/x",
        "scan: (Pink Floyd)/the wall/01.flac",
        "marvin/Pink Floyd/The Wall",
        "my-nas/Pink Floyd/The Wall",
        "192.168.1.5/Pink Floyd/The Wall",
        "x/home/bob/Docs/Private/secret plans here",
        "Artist/Album/01 Track.flac",
    ] {
        assert_no_leak(
            input,
            &[
                "Pink", "Floyd", "wall", "flesh", "Various", "Artists", "Earth", "Wind", "Fire",
                "secret", "Artist", "Album", "Track", "Docs",
            ],
        );
    }
}

#[test]
fn a_relative_path_keeps_the_log_prefix_of_its_line() {
    let output = redacted(
        "2026-11-08T12:00:00.123Z WARN reprise::sync: copy to phone: Pink Floyd/the wall/01.flac",
    );

    // The words in front of a relative path may be its first segment
    // (`copy to phone: Pink Floyd/…`): only the log prefix is kept.
    assert_eq!(output, "2026-11-08T12:00:00.123Z WARN reprise::sync: …");
    assert!(!output.contains("Pink"), "{output}");
}

#[test]
fn ordinary_slashes_are_not_paths() {
    for input in [
        "decoded 12/11/2026 at 3/12 with I/O",
        "mode read/write",
        "type audio/flac",
        "rate 5 MB/s",
        "ratio 3/12 and 4/5 done",
    ] {
        assert_eq!(redacted(input), input);
    }
}

#[test]
fn a_forged_ellipsis_does_not_disable_path_detection() {
    for input in [
        "Metallica/…And Justice for All/01 Blackened.flac",
        "/home/marvin/Music/…And Justice for All/01 Blackened.flac",
        "/tank/audio/Metallica/…And Justice for All/01 Blackened.flac",
        "~/…And Justice for All/01 Blackened.flac",
        "$HOME/Music/…And Justice for All/01 Blackened.flac",
        "$HOME/…And Justice for All",
        "smb://…nas/share/Pink Floyd/x",
        "/…secret/Pink Floyd/x",
        "$XDG_MUSIC_DIR/…Pink Floyd",
        "scan: … /mnt/usb/Pink Floyd",
        "scan: …/mnt/usb/Pink Floyd",
    ] {
        assert_no_leak(
            input,
            &["Justice", "Blackened", "Pink", "Floyd", "secret", "nas"],
        );
    }
}

#[test]
fn a_dollar_name_that_is_not_a_marker_followed_by_a_slash_is_a_path() {
    for input in [
        "$USER/Pink Floyd/The Wall",
        "$HOST/Pink Floyd/The Wall/01 Time.flac",
        "$IP/Pink Floyd/The Wall/01 Time.flac",
        "$MAC/Pink Floyd",
        "$HOSTNAME_REAL/Pink Floyd",
        "$XDG_MUSIC_DIR/Pink Floyd",
        "${HOME}/Music/Pink Floyd",
        "$HOME/Downloads/pink floyd wish you were here",
        "~/Downloads/pink floyd wish you were here",
        "~marvin and ~bob/pink floyd",
    ] {
        assert_no_leak(input, &["Pink", "Floyd", "pink", "wish"]);
    }
}

#[test]
fn the_markers_the_redactor_writes_are_left_alone() {
    for input in [
        "could not scan $XDG_MUSIC_DIR/…",
        "could not scan $HOME/…",
        "open file://…",
        "inet $IP/24 and $IP/64",
        "failed for $USER on $HOST",
    ] {
        assert_eq!(redacted(input), input);
    }
}

#[test]
fn a_path_in_a_known_marker_form_followed_by_more_text_takes_that_text() {
    let output = redacted("could not scan $XDG_MUSIC_DIR/… Pink Floyd");

    assert!(!output.contains("Pink"), "{output}");
}

#[test]
fn a_path_inside_a_url_or_a_remote_root_takes_the_line() {
    for input in [
        "http://my-nas:8080/Music/Pink Floyd, The Wall/01.mp3 failed",
        "failed to open file:///home/bob/My Music/Pink Floyd/x.flac: not found",
        "failed to open smb://nas.local/share/Pink Floyd/x.flac: not found",
        "sftp://bob:pw@nas.example.com:22/home/bob/Pink Floyd, retry",
        "GET https://api.example.com/v1/x?token=abc123&user=bob stalled badly",
        "mounted nas:/export/Pink Floyd/the wall",
        "mounted bob@nas:/export/Pink Floyd",
    ] {
        assert_no_leak(
            input,
            &[
                "Pink",
                "Floyd",
                "retry",
                "badly",
                "export",
                "wall",
                "not found",
            ],
        );
    }
}
