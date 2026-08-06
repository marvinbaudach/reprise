# Lyrics provider fallbacks — source and licensing review

Measured on 2026-08-05 from the checked-out Reprise sources and provider-owned
documentation. No production endpoint was called, and no user music, database,
or cached lyrics were read. The legal classifications below are engineering
risk assessments, not legal advice.

## Result

**Do not add another scraped or undocumented provider as the next fallback.**
The highest-value, lowest-complexity improvement is a second lookup through
LRCLIB itself:

1. local `.lrc` sidecar and embedded tags;
2. LRCLIB `/api/get` with the exact track signature, as today;
3. LRCLIB `/api/search` after a clean miss, or to upgrade an exact plain result
   to a conservatively matched synchronized result;
4. a separately licensed provider only if Reprise obtains terms that explicitly
   permit desktop display, caching, and `.lrc` sidecar creation.

The documented search endpoint returns the same record shape as `/api/get`,
including `instrumental`, `plainLyrics`, and `syncedLyrics`, but does not require
the exact album-and-duration signature. It can therefore recover remasters,
compilations, and imperfect album tags without adding another content source or
response model. LRCLIB limits search to 20 results without pagination, which is
enough for a narrowly filtered fallback but requires Reprise to reject ambiguous
matches ([LRCLIB API documentation](https://lrclib.net/docs)).

Before increasing LRCLIB traffic, Reprise must also handle `429 Too Many
Requests` by honoring `Retry-After`. The official documentation requires this.
The existing 250 ms per-host spacing satisfies LRCLIB's recommended 200–500 ms
batch delay. Reprise records the server deadline in its host breaker rather than
sleeping a worker, and even an explicit retry observes that deadline.

## Current Reprise contract

Reprise implements local-first lookup followed by **LRCLIB and then NetEase**
(`docs/ux-rules.md`, LYR-5; `crates/reprise-core/src/lyrics/mod.rs`). Its
provider-neutral output is `Synced(Vec<TimedLine>)`, `Plain(String)`, or
`Instrumental`, with explicit `Tag`, `Sidecar`, `Lrclib`, and `Netease` source
identities (`crates/reprise-core/src/lyrics/model.rs`). The first synchronized
result wins while the first plain result remains a fallback
(`crates/reprise-core/src/lyrics/chain.rs`). A synchronized network hit may be
persisted beside the track as a new `.lrc`, so provider terms must permit more
than transient on-screen display.

The LRCLIB integration sends title, artist, album, and rounded duration to
`/api/get`. LRCLIB documents album and duration as required for that endpoint
and accepts only a duration match within approximately two seconds. That is a
high-confidence matcher but a predictable coverage loss when album tags or
edition durations differ ([LRCLIB API documentation](https://lrclib.net/docs)).

## Provider assessment

| Provider | Lyrics and matching | API, authentication, and limits | Licence and redistribution fit | Recommendation |
| --- | --- | --- | --- | --- |
| **LRCLIB `/api/search`** | Plain, line-synchronized LRC, and instrumental in the same record shape as `/api/get`. Search accepts a general query or title with optional artist and album; results include duration. Maximum 20 results, no pagination. | Public API, no key or registration. An identifying `User-Agent` is required. Clients must serialize requests, delay batch calls, and honor `429 Retry-After`. | The service explicitly invites applications to use its free API, and the server software is MIT-licensed. The [MIT file](https://github.com/tranxuanthang/lrclib/blob/main/LICENSE) licenses the **software**, not the contributed lyric texts; LRCLIB's public pages do not state a separate lyric-content licence. This is the same content-risk surface Reprise already accepts, not a newly cleared redistribution right. | **Add directly after exact `/api/get`.** Best immediate coverage improvement. Obtain a legal/content-policy review before claiming that permanent sidecar copies are licensed. |
| **NetEase Cloud Music** (already present) | Reprise searches five songs by title and artist, requires an exact normalized title/artist and duration within three seconds, then reads LRC/plain/instrumental fields from `/api/song/lyric` (`crates/reprise-core/src/lyrics/netease.rs`). | Reprise uses keyless consumer endpoints. No official public developer contract, authentication model, quota, or rate-limit documentation was found for these paths. The 250 ms delay is Reprise policy, not a published NetEase allowance. | NetEase announces catalogue licences for distribution through **its streaming platform and associated digital services in China**; that is not evidence of a sublicense to Reprise ([NetEase/UMG announcement](https://ir.netease.com/news-releases/news-release-details/netease-cloud-music-and-universal-music-group-announce-strategic)). | **Do not treat as a release-grade fallback without written permission.** Keep only as an explicitly provisional risk if the owner chooses; otherwise remove or disable before release rather than adding similar private endpoints. |
| **Happi.dev** | Official endpoints expose plain lyrics and a beta synchronized-lyrics form. Artist and track are required; album and duration can improve matching. | Requires an `x-happi-token`. The published limit is 60 requests/minute; successful lyric responses consume paid credits. See the official [plain endpoint](https://happi.readme.io/reference/lyrics-search-api), [synchronized endpoint](https://happi.readme.io/reference/lyrics-search-api-beta-copy), [rate limits](https://happi.readme.io/reference/rate-limiting), and [pricing](https://happi.dev/). | Happi's public [terms](https://happi.dev/terms) do not clearly grant a desktop client the right to cache returned lyric text or create permanent sidecars. A shared token also cannot be kept secret in GPL source or a distributed binary. | **Best small-provider pilot only after written storage permission, using a user-supplied token.** Keep after LRCLIB to conserve quota; do not ship as an automatic default yet. |
| **Musixmatch Lyrics API** | Official endpoints provide plain lyrics, LRC subtitles, and richer synchronization. Matcher calls accept title/artist plus duration, and can use ISRC for stronger identity matching. | Every call needs an API key, which Musixmatch says must remain secret. Public documentation does not publish a generally applicable quota; entitlement and limits are account/plan dependent. See the provider-owned [Lyrics API collection](https://www.postman.com/musixmatch-dev/musixmatch-apis/documentation/pqm8o6w/lyrics-api). | The responses carry copyright, restriction, territory, publisher, attribution, and tracking fields. Reprise would need negotiated terms that cover its countries, offline cache, and sidecar write. A secret cannot safely be embedded in GPL source or a distributed binary; deployment needs a user credential, a permitted client credential, or a Reprise-operated service. | **Technically strong and realistic only under a licence agreement.** Preferred commercial fallback if Musixmatch confirms caching and sidecars in writing. Not a keyless default. |
| **LyricFind** | Offers licensed static, line-by-line, and word-by-word display; its public site does not expose enough API or matching detail to design an adapter. | No public authentication, quota, or pricing contract is documented; integration starts through sales. | LyricFind presents licensed and verified lyrics, worldwide coverage through more than 60,000 publisher partners, and royalty reporting ([products](https://www.lyricfind.com/products), [display formats](https://www.lyricfind.com/products/lyric-display)). This is the clearest rights-managed candidate, but only the signed partner agreement can establish Reprise's caching and sidecar rights. | **Commercial alternative, not an implementable public fallback today.** Ask `sales@lyricfind.com` for desktop/API terms ([contact](https://www.lyricfind.com/contact/)). |
| **Genius** | The official API provides song/search metadata and page URLs, not a supported raw-lyrics download or synchronized LRC endpoint ([official API documentation](https://docs.genius.com/)). | OAuth/token API for metadata; a lyrics downloader would have to scrape public pages outside that contract. | Scraped text has no provider grant suitable for Reprise's cache and sidecar behavior; Genius's [terms](https://genius.com/static/terms) govern page use. | **Reject as a fallback.** Linking to a Genius page could be a separate user action, but it does not fill Reprise's lyrics model. |
| **lyrics.ovh** | Keyless title/artist endpoint returns only plain text. No duration, album, ISRC, synchronized text, or documented disambiguation. | The upstream publishes no rate-limit or availability contract. | Its own [upstream README](https://github.com/NTag/lyrics.ovh) says it scrapes Genius, AZLyrics, Paroles.net, LyricsMania, Letras, and Lyrics.com in parallel and returns the first result. The repository's MIT licence covers the scraper software, not those sites' lyric content. | **Reject as a shipped fallback.** Weak matching and inherited scraping/licensing risk outweigh the extra plain-text coverage. |

## Conservative LRCLIB search selection

Call `/api/search` after a clean `/api/get` `404`, or after an exact plain hit
to look specifically for a synchronized upgrade. Never search after a
transport, `429`, or server failure. If search fails or returns no unique
synchronized candidate, preserve the exact plain result. This avoids doubling
load during outages and preserves the current circuit-breaker meaning.

Request only `track_name` and `artist_name`, then score the at-most-20 results
locally:

- require title and artist equality after Unicode-aware lowercase conversion
  and whitespace collapse;
- require duration within two seconds and reject candidates without duration;
- use album equality as a ranking signal, not a reason to accept a different
  title or artist;
- rank synchronized lyrics above plain lyrics and instrumental records;
- reject a tie between equally ranked records instead of writing a possibly
  wrong `.lrc` beside the user's audio;
- retain an exact plain result unless a unique valid synchronized record wins.

This keeps matching stricter than a free-text scrape, while allowing the exact
failure modes `/api/get` cannot absorb. Tests pin remaster and compilation
recovery, duration and identity rejection, ambiguous-result rejection,
synchronized-over-plain selection, exact-plain upgrade and preservation,
`429 Retry-After`, and the rule that search is not attempted after transport
failure.

## Decision boundary

Adding an HTTP adapter to GPL code is not itself the hard part. The unresolved
question is the right to display and permanently copy third-party lyrics. Do
not bundle a lyric corpus or provider secret, and do not describe any provider
as legally cleared merely because its client or server source code is open
source. For a release, retain a provider only when its published terms or a
written agreement cover Reprise's actual behavior: desktop display, automated
library batches, local cache, and creation of a persistent `.lrc` sidecar.
