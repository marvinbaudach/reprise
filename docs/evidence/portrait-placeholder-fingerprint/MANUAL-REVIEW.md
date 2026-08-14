# Deezer portrait visible acceptance review

- Compare `before/my-stats.png` and `after/my-stats.png` at the same ranks.
- Both screenshots are captured after expanding the ranking; confirm the
  `Hide more top artists` control and Oceano are visible in the retained CUA evidence.
- Ranks move with the listening history; find the artists by name, not by number.
- The run copy is seeded. Aetheriality, In Your Grave, Our Vices and Wake Me were
  lifted into the rendered ranking with synthetic listen events, because only a
  rendered rank fetches a portrait and those four sit at ranks 40, 122, 131 and —
  with zero plays — nowhere at all. Read `before/seeded-ranking-proof.txt`: it
  names every injected millisecond, and both arms received the identical copy.
  Ranks 1-15 are untouched, so the listening history the screenshots show above
  the seeded block is the real one.
- The before arm therefore shows four grey person silhouettes at ranks 16-19 and
  the after arm shows initials in their place. That contrast is the point of the
  whole change: those four silhouettes arrive under ordinary, artist-specific
  image identifiers, which no fixed list can enumerate in advance.
- A silhouette anywhere *else* — at any rank outside 16-19, in either arm — is a
  finding. The baseline already rejects the two structural identifiers.
- Oceano is the only intended difference: a photograph before, initials after. Its
  most popular exact-name candidate now reaches content validation, is rejected as
  the known silhouette, and must not fall back to the pictured namesake.
- The Devil Wears Prada is the control: the same photograph in both arms. It hides
  behind the empty-string MD5, which the baseline already catches at selection.
- Confirm every other artist shows the same identity in both arms, or record each
  change. Only the artists rendered in the ranking are fetched at all — silhouettes
  further down the library are covered by the corpus measurement in
  `docs/evidence/portrait-placeholder-fingerprint/rust-separation.txt`, not here.
- Read `settings-proof.txt`, `cache-before.txt`, `cache-listing.txt`, and
  `named-cache-proof.txt` alongside the screenshots. The empty cache plus the
  named images created afterward is the positive portrait-request proof.
- Confirm both application processes ended; the script waits for each smoke timer.
