/**
 * Where the playhead stands, and who is allowed to move it.
 *
 * Three things can claim the position and they must not fight: the clock while
 * the track plays, a pointer or a key while someone is seeking, and a held
 * value when motion is off and there is no clock at all. Keeping that apart
 * from the drawing means the rule can be checked without a canvas.
 */

/** The drawn strip is centred in its canvas, so a click maps through it. */
export interface SeekStrip {
  readonly left: number;
  readonly width: number;
}

export function clampUnit(value: number): number {
  return Math.min(1, Math.max(0, value));
}

/** The track position under a client x, given the canvas box and the strip in it. */
export function positionInStrip(clientX: number, boundsLeft: number, strip: SeekStrip): number {
  if (strip.width === 0) return 0;
  return clampUnit((clientX - boundsLeft - strip.left) / strip.width);
}

export interface SeekClock {
  /** The position for this frame, and the one `position()` reports afterwards. */
  advance(timestamp: number, still: boolean): number;
  /** Holds the playhead while a pointer or a key is moving it. */
  scrubTo(at: number): void;
  /** Drops the hold and lets the clock run on from wherever the playhead is. */
  releaseScrub(now: number): void;
  position(): number;
}

export function createSeekClock(durationMs: number): SeekClock {
  let startedAt: number | null = null;
  let scrub: number | null = null;
  let held = 0;
  let position = 0;

  return {
    advance(timestamp, still) {
      startedAt ??= timestamp;
      const clock = ((timestamp - startedAt) % durationMs) / durationMs;
      position = scrub ?? (still ? held : clock);
      return position;
    },
    scrubTo(at) {
      scrub = clampUnit(at);
      position = scrub;
    },
    releaseScrub(now) {
      if (scrub === null) return;
      held = scrub;
      // The clock is rewound so playback carries on from where the seek left
      // it, rather than jumping to wherever it would have reached by now.
      startedAt = now - scrub * durationMs;
      scrub = null;
    },
    position() {
      return position;
    },
  };
}
