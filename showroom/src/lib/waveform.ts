/** Display shaping for measured waveform peaks, ported from reprise-view. */

export const SILENCE_RMS = 0.003_162_28;
export const PERCENTILE_LOW = 0.1;
export const PERCENTILE_HIGH = 0.95;
export const HEIGHT_GAMMA = 1.6;

const F32_EPSILON = 1.192_092_895_507_812_5e-7;

export interface DisplayPeaks {
  readonly levels: Float32Array;
  readonly silent: readonly boolean[];
}

/** Undo the stored square-root compression and aggregate in the RMS domain. */
export function aggregateRms(raw: Uint8Array, count: number): Float32Array {
  if (raw.length === 0 || count === 0) return new Float32Array();

  const output = new Float32Array(count);
  for (let index = 0; index < count; index += 1) {
    const start = Math.floor((index * raw.length) / count);
    const end = Math.min(
      raw.length,
      Math.max(start + 1, Math.floor(((index + 1) * raw.length) / count)),
    );
    let power = 0;
    for (let rawIndex = start; rawIndex < end; rawIndex += 1) {
      const rms = ((raw[rawIndex] ?? 0) / 255) ** 2;
      power += rms * rms;
    }
    output[index] = Math.sqrt(power / (end - start));
  }
  return output;
}

function percentile(sorted: readonly number[], proportion: number): number {
  const last = sorted.length - 1;
  const rank = Math.round(last * proportion);
  return sorted[Math.min(rank, last)] ?? 0;
}

/** Apply the Rust port's 25/50/25 neighbouring-bucket smoothing. */
export function smoothNeighbors(values: Float32Array): Float32Array {
  const output = new Float32Array(values.length);
  for (let index = 0; index < values.length; index += 1) {
    const previous = values[Math.max(0, index - 1)] ?? 0;
    const current = values[index] ?? 0;
    const next = values[Math.min(values.length - 1, index + 1)] ?? 0;
    output[index] = 0.25 * previous + 0.5 * current + 0.25 * next;
  }
  return output;
}

/** Stretch p10..p95, apply gamma, smooth, and retain true-silence dots. */
export function shapeDisplayPeaks(raw: Uint8Array, count: number): DisplayPeaks {
  const rms = aggregateRms(raw, count);
  if (rms.length === 0) return { levels: new Float32Array(), silent: [] };

  const audible = Array.from(rms)
    .filter((value) => value >= SILENCE_RMS)
    .sort((left, right) => left - right);
  if (audible.length === 0) {
    return {
      levels: new Float32Array(rms.length),
      silent: Array.from({ length: rms.length }, () => true),
    };
  }

  const low = percentile(audible, PERCENTILE_LOW);
  const high = percentile(audible, PERCENTILE_HIGH);
  const span = high - low;
  const shaped = new Float32Array(rms.length);
  for (let index = 0; index < rms.length; index += 1) {
    const value = rms[index] ?? 0;
    if (value < SILENCE_RMS) continue;
    const normalized =
      span <= F32_EPSILON
        ? value > high
          ? 1
          : 0.5
        : Math.min(1, Math.max(0, (value - low) / span));
    shaped[index] = normalized ** HEIGHT_GAMMA;
  }

  return {
    levels: smoothNeighbors(shaped),
    silent: Array.from(rms, (value) => value < SILENCE_RMS),
  };
}
