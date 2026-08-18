const WAKE_BARS = 14;

export type SeekBarLight =
  | { readonly played: true; readonly lift: number }
  | { readonly played: false; readonly lightness: number };

/** Return the playhead's asymmetric light wake without changing bar geometry. */
export function seekBarLight(index: number, playBar: number, pulse: number): SeekBarLight {
  const played = index <= playBar;
  const distance = playBar - index;
  const proximity = played
    ? Math.max(0, 1 - distance / WAKE_BARS)
    : Math.max(0, 1 + distance / (WAKE_BARS * 0.5));

  return played
    ? { played, lift: proximity * (3 + 6 * pulse) }
    : { played, lightness: 33 + proximity * 9 * (0.6 + 0.4 * pulse) };
}
