//! Addresses inside a word: `peer:192.168.1.5:8080`, `192.168.1.5_backup`,
//! `ip-192-168-1-5`, `ip6-fe80::1`, `dev_AA_BB_CC_DD_EE_FF`, `aabb.ccdd.eeff`.
//!
//! A run of address characters is judged as a whole first (`redact_network`).
//! A run that is not an address as a whole is read again here, for the
//! addresses that stand in it, and only those are replaced.

use super::redact_network::{is_ipv6, is_lan_host, HOST_MARKER, IP_MARKER, MAC_MARKER};

/// The longest stretch an address candidate is read over.
const MAX_CANDIDATE_CHARS: usize = 48;

/// The pairs of a MAC address, and the groups of a Cisco one.
const MAC_PAIRS: usize = 6;
const CISCO_GROUPS: usize = 3;
const CISCO_GROUP_CHARS: usize = 4;

/// The octets of an IPv4 address and the digits of one.
const OCTETS: usize = 4;
const MAX_OCTET_DIGITS: usize = 3;
const MAX_OCTET: u16 = 255;
const MAX_PORT_DIGITS: usize = 5;

/// Replaces every address that stands inside `run`.
pub(super) fn redact_inside(run: &str) -> String {
    let mut text = run.to_owned();
    for pass in [redact_macs, redact_ipv4, redact_ipv6, redact_lan_hosts] {
        text = pass(&text);
    }
    text
}

/// Replaces each stretch `find` reports, scanning left to right. `find` gets
/// the characters and an index and answers how many characters an address
/// that starts there spans.
fn replace_found(
    text: &str,
    marker: &str,
    find: impl Fn(&[char], usize) -> Option<usize>,
) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        match find(&characters, index) {
            Some(length) if length > 0 => {
                output.push_str(marker);
                index += length;
            }
            _ => {
                output.push(characters[index]);
                index += 1;
            }
        }
    }
    output
}

fn is_alphanumeric_at(characters: &[char], index: usize) -> bool {
    characters.get(index).is_some_and(|ch| ch.is_alphanumeric())
}

/// Whether the address that starts at `index` stands alone: no letter or digit
/// touches it on the left.
fn starts_alone(characters: &[char], index: usize) -> bool {
    index == 0 || !is_alphanumeric_at(characters, index - 1)
}

fn hex_group(characters: &[char], index: usize, length: usize) -> bool {
    (0..length).all(|offset| {
        characters
            .get(index + offset)
            .is_some_and(char::is_ascii_hexdigit)
    })
}

// MAC addresses

fn redact_macs(text: &str) -> String {
    replace_found(text, MAC_MARKER, |characters, index| {
        if !starts_alone(characters, index) {
            return None;
        }
        mac_pairs(characters, index).or_else(|| cisco_groups(characters, index))
    })
}

/// Six hex pairs joined by one of `:`, `-` and `_`.
fn mac_pairs(characters: &[char], index: usize) -> Option<usize> {
    let separator = *characters.get(index + 2)?;
    if !matches!(separator, ':' | '-' | '_') {
        return None;
    }
    let step = 3;
    for pair in 0..MAC_PAIRS {
        let at = index + pair * step;
        if !hex_group(characters, at, 2)
            || (pair + 1 < MAC_PAIRS && characters.get(at + 2) != Some(&separator))
        {
            return None;
        }
    }
    let end = index + (MAC_PAIRS - 1) * step + 2;
    (!is_alphanumeric_at(characters, end)).then_some(end - index)
}

/// `aabb.ccdd.eeff`.
fn cisco_groups(characters: &[char], index: usize) -> Option<usize> {
    let step = CISCO_GROUP_CHARS + 1;
    for group in 0..CISCO_GROUPS {
        let at = index + group * step;
        if !hex_group(characters, at, CISCO_GROUP_CHARS)
            || (group + 1 < CISCO_GROUPS && characters.get(at + CISCO_GROUP_CHARS) != Some(&'.'))
        {
            return None;
        }
    }
    let end = index + (CISCO_GROUPS - 1) * step + CISCO_GROUP_CHARS;
    (!is_alphanumeric_at(characters, end)).then_some(end - index)
}

// IPv4 addresses

fn redact_ipv4(text: &str) -> String {
    replace_found(text, IP_MARKER, |characters, index| {
        if !characters.get(index)?.is_ascii_digit() {
            return None;
        }
        ['.', '-', '_']
            .iter()
            .filter(|separator| starts_a_chain(characters, index, **separator))
            .find_map(|separator| quad(characters, index, *separator))
    })
}

/// No digit, and no digit-and-`separator`, comes before `index`: the quad is
/// not the tail of a longer chain of numbers (`1.2.3.4.5`).
fn starts_a_chain(characters: &[char], index: usize, separator: char) -> bool {
    let Some(before) = index.checked_sub(1).map(|at| characters[at]) else {
        return true;
    };
    if before.is_ascii_digit() {
        return false;
    }
    !(before == separator && index >= 2 && characters[index - 2].is_ascii_digit())
}

/// Four octets joined by `separator`, then an optional `:port` and an
/// optional `%zone`.
fn quad(characters: &[char], index: usize, separator: char) -> Option<usize> {
    let mut at = index;
    for octet in 0..OCTETS {
        let digits = characters[at..]
            .iter()
            .take_while(|ch| ch.is_ascii_digit())
            .count();
        if digits == 0 || digits > MAX_OCTET_DIGITS {
            return None;
        }
        let value: u16 = characters[at..at + digits]
            .iter()
            .collect::<String>()
            .parse()
            .ok()?;
        if value > MAX_OCTET {
            return None;
        }
        at += digits;
        if octet + 1 < OCTETS {
            if characters.get(at) != Some(&separator) {
                return None;
            }
            at += 1;
        }
    }
    // A fifth octet (`1.2.3.4.5`) or a longer number makes it no address.
    if characters.get(at).is_some_and(char::is_ascii_digit)
        || (characters.get(at) == Some(&separator)
            && characters.get(at + 1).is_some_and(char::is_ascii_digit))
    {
        return None;
    }
    at += port_length(characters, at) + zone_length(characters, at);
    Some(at - index)
}

fn port_length(characters: &[char], at: usize) -> usize {
    if characters.get(at) != Some(&':') {
        return 0;
    }
    let digits = characters[at + 1..]
        .iter()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if (1..=MAX_PORT_DIGITS).contains(&digits) {
        digits + 1
    } else {
        0
    }
}

fn zone_length(characters: &[char], at: usize) -> usize {
    if characters.get(at) != Some(&'%') {
        return 0;
    }
    let name = characters[at + 1..]
        .iter()
        .take_while(|ch| ch.is_ascii_alphanumeric() || **ch == '_')
        .count();
    if name > 0 {
        name + 1
    } else {
        0
    }
}

// IPv6 addresses

fn redact_ipv6(text: &str) -> String {
    replace_found(text, IP_MARKER, |characters, index| {
        if !starts_alone(characters, index)
            || !characters
                .get(index)
                .is_some_and(|ch| ch.is_ascii_hexdigit() || *ch == ':')
        {
            return None;
        }
        let reach = characters[index..]
            .iter()
            .take(MAX_CANDIDATE_CHARS)
            .take_while(|ch| ch.is_ascii_hexdigit() || matches!(ch, ':' | '.'))
            .count();
        // The longest stretch that reads as an address, shortest cut last.
        (2..=reach).rev().find_map(|length| {
            let candidate: String = characters[index..index + length].iter().collect();
            let end = index + length;
            (!is_alphanumeric_at(characters, end)
                && !candidate.ends_with(['.', ':'])
                && candidate.matches(':').count() >= 2
                && is_ipv6(&candidate))
            .then(|| length + zone_length(characters, end))
        })
    })
}

// Host names in the LAN

/// A name under a LAN-only domain (`nas.local`) that stands next to other text
/// (`host:nas.local`, `share_nas.lan`).
fn redact_lan_hosts(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut piece_start = 0;
    for (index, ch) in text.char_indices() {
        if matches!(ch, ':' | '_' | '%') {
            push_piece(&mut output, &text[piece_start..index]);
            output.push(ch);
            piece_start = index + ch.len_utf8();
        }
    }
    push_piece(&mut output, &text[piece_start..]);
    output
}

fn push_piece(output: &mut String, piece: &str) {
    if is_lan_host(piece) {
        output.push_str(HOST_MARKER);
    } else {
        output.push_str(piece);
    }
}
