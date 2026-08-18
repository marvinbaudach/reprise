export const PEAK_DECAY = 0.018;
export const GLOW_RELEASE = 0.06;
export const SETTLE_EPSILON = 0.002;

export const BAND_COUNT = 64;
export const BYTES_PER_FRAME = BAND_COUNT + 1;
export const SAMPLE_RATE = 44_100;
export const CHUNK_SAMPLES = 1_024;
export const FRAMES_PER_SECOND = SAMPLE_RATE / CHUNK_SAMPLES;

const UINT8_MAX = 255;

export interface VisualizerFrame {
  readonly bars: Float32Array;
  readonly peaks: Float32Array;
  readonly bassImpact: number;
}

export class VisualizerEngine {
  readonly #bars = new Float32Array(BAND_COUNT);
  readonly #peaks = new Float32Array(BAND_COUNT);
  #glow = 0;

  ingest(track: Uint8Array, offset: number): void {
    for (let band = 0; band < BAND_COUNT; band += 1) {
      const value = (track[offset + band] ?? 0) / UINT8_MAX;
      this.#bars[band] = value;
      this.#peaks[band] = Math.max(this.#peaks[band] ?? 0, value);
    }
    this.#glow = Math.max(this.#glow, (track[offset + BAND_COUNT] ?? 0) / UINT8_MAX);
  }

  tick(): void {
    this.#glow = Math.max(0, this.#glow - GLOW_RELEASE);
    for (let band = 0; band < BAND_COUNT; band += 1) {
      const current = this.#bars[band] ?? 0;
      const peak = Math.max((this.#peaks[band] ?? 0) - PEAK_DECAY, current);
      this.#peaks[band] = peak < SETTLE_EPSILON ? 0 : peak;
    }
  }

  frame(): VisualizerFrame {
    return {
      bars: this.#bars,
      peaks: this.#peaks,
      bassImpact: this.#glow,
    };
  }
}
