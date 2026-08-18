/** Spectral-position shaping and the fixed coral-to-teal brand axis. */

export const CORAL = [255, 111, 94] as const;
export const TEAL = [79, 219, 212] as const;
export const CENTROID_WINDOW_S = 8;
export const SECTION_MIN_SPACING_S = 20;
export const SECTION_STEP_THRESHOLD = 26;

const TAU = Math.PI * 2;
const SECTION_STEP_SPAN_S = 2;

export type Rgb = readonly [number, number, number];

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

function srgbChannelToLinear(channel: number): number {
  const bounded = clamp(channel, 0, 1);
  return bounded <= 0.040_45 ? bounded / 12.92 : ((bounded + 0.055) / 1.055) ** 2.4;
}

function linearChannelToSrgb(channel: number): number {
  const srgb = channel <= 0.003_130_8 ? channel * 12.92 : 1.055 * channel ** (1 / 2.4) - 0.055;
  return clamp(srgb, 0, 1);
}

function srgbToOklab([r, g, b]: Rgb): Rgb {
  const linearR = srgbChannelToLinear(r);
  const linearG = srgbChannelToLinear(g);
  const linearB = srgbChannelToLinear(b);
  const l = Math.cbrt(
    0.412_221_470_8 * linearR + 0.536_332_536_3 * linearG + 0.051_445_992_9 * linearB,
  );
  const m = Math.cbrt(
    0.211_903_498_2 * linearR + 0.680_699_545_1 * linearG + 0.107_396_956_6 * linearB,
  );
  const s = Math.cbrt(
    0.088_302_461_9 * linearR + 0.281_718_837_6 * linearG + 0.629_978_700_5 * linearB,
  );
  return [
    0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
    1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
    0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
  ];
}

function oklabToSrgb([labL, labA, labB]: Rgb): Rgb {
  const l = labL + 0.396_337_777_3 * labA + 0.215_803_757_9 * labB;
  const m = labL - 0.105_561_346_2 * labA - 0.063_854_174_7 * labB;
  const s = labL - 0.089_484_177_5 * labA - 1.291_485_548 * labB;
  const lCubed = l * l * l;
  const mCubed = m * m * m;
  const sCubed = s * s * s;
  return [
    linearChannelToSrgb(
      4.076_741_661_3 * lCubed - 3.307_711_590_8 * mCubed + 0.230_969_929_5 * sCubed,
    ),
    linearChannelToSrgb(
      -1.268_437_973 * lCubed + 2.609_757_401_1 * mCubed - 0.341_319_427_9 * sCubed,
    ),
    linearChannelToSrgb(
      -0.004_196_086_3 * lCubed - 0.703_418_614_7 * mCubed + 1.707_614_701 * sCubed,
    ),
  ];
}

function normalizeRgb([r, g, b]: readonly [number, number, number]): Rgb {
  return [r / 255, g / 255, b / 255];
}

/** Walk the long, falling-hue OKLCH route from coral to teal. */
export function spectralColour(position: number): Rgb {
  const amount = Number.isNaN(position) ? 0.5 : clamp(position, 0, 1);
  const start = srgbToOklab(normalizeRgb(CORAL));
  const end = srgbToOklab(normalizeRgb(TEAL));
  const startChroma = Math.hypot(start[1], start[2]);
  const endChroma = Math.hypot(end[1], end[2]);
  const startHue = Math.atan2(start[2], start[1]);
  let endHue = Math.atan2(end[2], end[1]);
  while (endHue >= startHue) endHue -= TAU;
  const lightness = start[0] + (end[0] - start[0]) * amount;
  const chroma = startChroma + (endChroma - startChroma) * amount;
  const hue = startHue + (endHue - startHue) * amount;
  return oklabToSrgb([lightness, chroma * Math.cos(hue), chroma * Math.sin(hue)]);
}

function halfWindowFrames(frames: number, durationS: number, windowS: number): number {
  if (frames < 2 || !Number.isFinite(durationS) || durationS <= 0) return 0;
  if (!Number.isFinite(windowS) || windowS <= 0) return 0;
  const half = Math.round((windowS / 2) * (frames / durationS));
  if (!Number.isFinite(half) || half < 1) return 0;
  return Math.min(half, frames);
}

/** Average centroid support points over a fixed window of track time. */
export function smoothCentroidOverSeconds(
  raw: Uint8Array,
  durationS: number,
  windowS: number,
): Uint8Array {
  const half = halfWindowFrames(raw.length, durationS, windowS);
  if (half === 0) return raw.slice();

  const prefix: number[] = [0];
  let running = 0;
  for (const value of raw) {
    running += value;
    prefix.push(running);
  }
  return Uint8Array.from(raw, (_, index) => {
    const start = Math.max(0, index - half);
    const end = Math.min(raw.length, index + half + 1);
    const count = end - start;
    const sum = (prefix[end] ?? 0) - (prefix[start] ?? 0);
    return Math.floor((sum + Math.floor(count / 2)) / count);
  });
}

/** Find well-spaced structural turns in an already-smoothed centroid curve. */
export function sectionBoundaries(smoothed: Uint8Array, durationS: number): number[] {
  const span = halfWindowFrames(smoothed.length, durationS, SECTION_STEP_SPAN_S * 2);
  if (span === 0 || smoothed.length <= span * 2) return [];

  const spacing = Math.max(1, Math.round((SECTION_MIN_SPACING_S / durationS) * smoothed.length));
  const candidates: Array<readonly [number, number]> = [];
  for (let index = span; index < smoothed.length - span; index += 1) {
    const before = smoothed[index - span] ?? 0;
    const after = smoothed[index + span] ?? 0;
    const step = Math.abs(after - before);
    if (step >= SECTION_STEP_THRESHOLD) candidates.push([step, index]);
  }
  candidates.sort((left, right) => right[0] - left[0] || left[1] - right[1]);
  const accepted: number[] = [];
  for (const [, index] of candidates) {
    if (accepted.some((taken) => Math.abs(taken - index) < spacing)) continue;
    accepted.push(index);
  }
  accepted.sort((left, right) => left - right);
  return accepted.map((index) => (index + 0.5) / smoothed.length);
}

/** Average centroid support points into display-bar windows. */
export function shapeCentroid(raw: Uint8Array, count: number): Float32Array {
  if (raw.length === 0 || count === 0) return new Float32Array();
  const output = new Float32Array(count);
  for (let index = 0; index < count; index += 1) {
    const start = Math.floor((index * raw.length) / count);
    const end = Math.min(
      raw.length,
      Math.max(start + 1, Math.floor(((index + 1) * raw.length) / count)),
    );
    let sum = 0;
    for (let rawIndex = start; rawIndex < end; rawIndex += 1) sum += raw[rawIndex] ?? 0;
    output[index] = sum / (end - start) / 255;
  }
  return output;
}

/** Linearly sample the stored centroid curve at a normalized position. */
export function centroidAt(raw: Uint8Array, position: number): number {
  if (raw.length === 0) return 0.5;
  if (raw.length === 1) return (raw[0] ?? 0) / 255;
  const amount = Number.isNaN(position) ? 0 : clamp(position, 0, 1);
  const index = amount * (raw.length - 1);
  const lower = Math.floor(index);
  const upper = Math.min(lower + 1, raw.length - 1);
  const fraction = index - lower;
  const lowerValue = raw[lower] ?? 0;
  const upperValue = raw[upper] ?? 0;
  return (lowerValue + (upperValue - lowerValue) * fraction) / 255;
}
