//! Network identifiers in free text: IPv4 and IPv6 addresses (with a port or a
//! zone) and MAC addresses. They name the user's router, NAS or phone.
//!
//! A host name is not a shape: `my-nas` reads like any word. This machine's
//! own name is removed through `RedactionContext::hostname`; another
//! machine's name goes only with the URI, share or `host:/` root it stands in.
//! A bare foreign host name in running text is not detected.

const IP_MARKER: &str = "$IP";
const MAC_MARKER: &str = "$MAC";

/// Characters an address is written with. A run of them is read as one candidate.
fn is_address_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, ':' | '.' | '%' | '-' | '_')
}

/// Replaces every address the text holds. A kept number (`version=1.2.3.4`)
/// is not an address: the key pass has already vouched for it.
pub(super) fn redact_network_identifiers(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for token in input.split_inclusive(char::is_whitespace) {
        let body = token.trim_end_matches(char::is_whitespace);
        if super::redact_assignments::is_kept_number(body) {
            output.push_str(token);
        } else {
            output.push_str(&redact_token(token));
        }
    }
    output
}

fn redact_token(token: &str) -> String {
    let mut output = String::with_capacity(token.len());
    let mut run_start = None;
    for (index, ch) in token.char_indices() {
        match (run_start, is_address_char(ch)) {
            (None, true) => run_start = Some(index),
            (Some(start), false) => {
                push_run(&mut output, &token[start..index]);
                output.push(ch);
                run_start = None;
            }
            (None, false) => output.push(ch),
            (Some(_), true) => {}
        }
    }
    if let Some(start) = run_start {
        push_run(&mut output, &token[start..]);
    }
    output
}

/// Writes the run, or its marker. Sentence punctuation at either end of the
/// run (`192.168.0.12:`, `.1.2.3.4`) is not part of the address.
fn push_run(output: &mut String, run: &str) {
    let core = run.trim_matches(['.', ':', '-', '_']);
    let lead = &run[..run.len() - run.trim_start_matches(['.', ':', '-', '_']).len()];
    let tail = &run[lead.len() + core.len()..];
    output.push_str(lead);
    match marker_for(core) {
        Some(marker) => output.push_str(marker),
        None => output.push_str(core),
    }
    output.push_str(tail);
}

fn marker_for(core: &str) -> Option<&'static str> {
    if is_mac(core) {
        Some(MAC_MARKER)
    } else if is_ipv4(core) || is_ipv6(core) {
        Some(IP_MARKER)
    } else {
        None
    }
}

/// Six hex pairs joined by `:`, or by `-`.
fn is_mac(candidate: &str) -> bool {
    [':', '-'].iter().any(|separator| {
        let pairs: Vec<&str> = candidate.split(*separator).collect();
        pairs.len() == 6
            && pairs
                .iter()
                .all(|pair| pair.len() == 2 && pair.chars().all(|ch| ch.is_ascii_hexdigit()))
    })
}

/// Four octets, each at most 255, with an optional `:port`.
fn is_ipv4(candidate: &str) -> bool {
    let (address, port) = match candidate.split_once(':') {
        Some((address, port)) => (address, Some(port)),
        None => (candidate, None),
    };
    if port.is_some_and(|port| {
        port.is_empty() || port.len() > 5 || !port.chars().all(|ch| ch.is_ascii_digit())
    }) {
        return false;
    }
    is_dotted_quad(address)
}

fn is_dotted_quad(address: &str) -> bool {
    let octets: Vec<&str> = address.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|octet| {
            (1..=3).contains(&octet.len())
                && octet.chars().all(|ch| ch.is_ascii_digit())
                && octet.parse::<u16>().is_ok_and(|value| value <= 255)
        })
}

/// Hex groups with one `::` or exactly eight groups, an optional embedded IPv4
/// tail and an optional `%zone`. A candidate with no decimal digit at all
/// (`db::fa`, `cafe::beef`) is a Rust path, not an address.
fn is_ipv6(candidate: &str) -> bool {
    let (address, zone) = match candidate.split_once('%') {
        Some((address, zone)) => (address, Some(zone)),
        None => (candidate, None),
    };
    if zone.is_some_and(|zone| zone.is_empty() || zone.contains('%')) {
        return false;
    }
    if address.matches(':').count() < 2 || address.matches("::").count() > 1 {
        return false;
    }
    let compressed = address.contains("::");
    let groups: Vec<&str> = address.split(':').collect();
    let last = groups.len() - 1;
    let mut count = 0;
    for (index, group) in groups.iter().enumerate() {
        if group.is_empty() {
            continue;
        }
        if index == last && group.contains('.') {
            if !is_dotted_quad(group) {
                return false;
            }
            count += 2;
        } else if group.len() <= 4 && group.chars().all(|ch| ch.is_ascii_hexdigit()) {
            count += 1;
        } else {
            return false;
        }
    }
    let shaped = if compressed { count < 8 } else { count == 8 };
    shaped && (zone.is_some() || address.chars().any(|ch| ch.is_ascii_digit()))
}
