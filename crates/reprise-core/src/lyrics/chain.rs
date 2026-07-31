use std::path::Path;

use super::{
    LyricsBody, LyricsError, LyricsHit, LyricsProvider, LyricsQuery, LyricsSource, SourceOutcome,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChainReport {
    pub(super) result: Result<LyricsHit, LyricsError>,
    pub(super) network_consensus_not_found: bool,
}

/// Runs the network tier. The local tier already ran in [`super::best_local`],
/// which returns early for a local `Synced` or `Instrumental` hit — so the only
/// local outcome that can still matter is a plain text, and it arrives here as
/// `local_plain` instead of being looked up a second time (a local lookup costs
/// a sidecar read plus a full tag parse per track).
pub(super) fn run_chain(
    query: &LyricsQuery,
    track_path: Option<&Path>,
    local_plain: Option<LyricsHit>,
    network_providers: &[&dyn LyricsProvider],
) -> ChainReport {
    let mut first_plain = local_plain;
    let mut clean_not_found = !network_providers.is_empty();
    for provider in network_providers {
        let outcome = provider.lookup(query, track_path);
        clean_not_found &= matches!(outcome, SourceOutcome::NotFound);
        if let Some(result) = consider_outcome(outcome, &mut first_plain) {
            return ChainReport {
                result: Ok(result),
                network_consensus_not_found: false,
            };
        }
    }

    if let Some(hit) = first_plain {
        return ChainReport {
            result: Ok(hit),
            network_consensus_not_found: clean_not_found,
        };
    }
    ChainReport {
        result: Err(if clean_not_found {
            LyricsError::NotFound
        } else {
            LyricsError::Temporary
        }),
        network_consensus_not_found: clean_not_found,
    }
}

fn consider_outcome(
    outcome: SourceOutcome,
    first_plain: &mut Option<LyricsHit>,
) -> Option<LyricsHit> {
    let SourceOutcome::Hit(hit) = outcome else {
        return None;
    };
    match &hit.body {
        LyricsBody::Synced(_) => Some(hit),
        LyricsBody::Plain(_) => {
            if first_plain.is_none() {
                *first_plain = Some(hit);
            }
            None
        }
        LyricsBody::Instrumental => match first_plain {
            Some(plain) if is_local(plain.source) => Some(plain.clone()),
            _ => Some(hit),
        },
    }
}

fn is_local(source: LyricsSource) -> bool {
    matches!(source, LyricsSource::Tag | LyricsSource::Sidecar)
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;
