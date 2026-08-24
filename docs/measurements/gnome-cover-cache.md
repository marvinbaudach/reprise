# B2 cover-cache measurement

Measured on 2026-08-24 with the cache lookup's hit/miss counters and the
deterministic access traces in `ui::cover::cover_cache::tests`.

| Trace | Previous track-keyed/FIFO shape | File-keyed LRU | Result |
| --- | ---: | ---: | --- |
| Open 15 tracks sharing one album cover | 0 hits, 15 misses, 15 texture entries | 14 hits, 1 miss, 1 texture entry | 14 decodes avoided |
| Scroll a 500-row, 40-row viewport down and back | 36,097 hits, 783 misses | 36,136 hits, 744 misses | 39 repeat decodes avoided |

The second trace replays every visible row at each scroll position. This makes
the eviction policy observable: a hit refreshes an LRU entry, while the former
FIFO leaves its age unchanged. The tests also cover the behavioural hazard:
invalidating one track evicts the shared cover file and every sibling mapping,
so a replacement file cannot serve stale album pixels.
