import { seekBarLight } from './seekLight';
import type { MeasuredSeekTrack } from './seekTrack';
import {
  CENTROID_WINDOW_S,
  centroidAt,
  liftLightness,
  type Rgb,
  shapeCentroid,
  smoothCentroidOverSeconds,
  spectralColour,
} from './spectralColour';
import { shapeDisplayPeaks } from './waveform';

export interface SeekSample {
  readonly elapsedMs: number;
  readonly remainingMs: number;
  readonly centroid: number;
  readonly level: number;
  readonly colour: Rgb;
}

interface PreparedTrack {
  readonly bars: number;
  readonly levels: Float32Array;
  readonly silent: readonly boolean[];
  readonly centroids: Float32Array;
}

interface SeekRendererOptions {
  readonly canvas: HTMLCanvasElement;
  readonly track: MeasuredSeekTrack;
  readonly hero: boolean;
  readonly onSample: (sample: SeekSample) => void;
}

export interface SeekCanvasRenderer {
  draw(timestamp: number, still: boolean): void;
  setHover(position: number | null): void;
}

interface RegisteredRenderer {
  readonly draw: (timestamp: number, still: boolean) => void;
  readonly isVisible: () => boolean;
}

export const SEEK_FRAME_EVENT = 'reprise:seek-frame';
const BAR_STEP_PX = 4;
const BAR_WIDTH_PX = 2;
const MIN_BAR_COUNT = 40;
const MAX_DEVICE_SCALE = 2;
const renderers = new Set<RegisteredRenderer>();

function colourCss([red, green, blue]: Rgb): string {
  return `rgb(${(red * 255).toFixed(2)} ${(green * 255).toFixed(2)} ${(blue * 255).toFixed(2)})`;
}

function clampUnit(value: number): number {
  return Math.min(1, Math.max(0, value));
}

export function createSeekRenderer({
  canvas,
  track,
  hero,
  onSample,
}: SeekRendererOptions): SeekCanvasRenderer {
  const durationS = track.durationMs / 1_000;
  const smoothedCentroids = smoothCentroidOverSeconds(
    track.centroids,
    durationS,
    CENTROID_WINDOW_S,
  );
  let prepared: PreparedTrack | null = null;
  let hover: number | null = null;
  let startedAt: number | null = null;

  const prepare = (bars: number): PreparedTrack => {
    if (prepared?.bars === bars) return prepared;
    const peaks = shapeDisplayPeaks(track.peaks, bars);
    prepared = {
      bars,
      levels: peaks.levels,
      silent: peaks.silent,
      centroids: shapeCentroid(smoothedCentroids, bars),
    };
    return prepared;
  };

  const draw = (timestamp: number, still: boolean) => {
    const context = canvas.getContext('2d');
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (!context || width === 0 || height === 0) return;
    const scale = Math.min(MAX_DEVICE_SCALE, window.devicePixelRatio || 1);
    const pixelWidth = Math.round(width * scale);
    const pixelHeight = Math.round(height * scale);
    if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
      canvas.width = pixelWidth;
      canvas.height = pixelHeight;
      prepared = null;
    }

    startedAt ??= timestamp;
    const position = still ? 0 : ((timestamp - startedAt) % track.durationMs) / track.durationMs;
    const bars = Math.max(MIN_BAR_COUNT, Math.floor(width / BAR_STEP_PX));
    const shaped = prepare(bars);
    const renderedWidth = bars * BAR_STEP_PX;
    const left = (width - renderedWidth) / 2;
    const middle = height / 2;
    const maximumHeight = height * 0.9;
    const playBar = Math.floor(position * bars);
    const pulse = still ? 0.5 : 0.5 + 0.5 * Math.sin(timestamp * 0.002_3);

    context.setTransform(scale, 0, 0, scale, 0, 0);
    context.clearRect(0, 0, width, height);
    for (let index = 0; index < bars; index += 1) {
      const x = left + index * BAR_STEP_PX;
      const light = seekBarLight(index, playBar, pulse);
      if (shaped.silent[index]) {
        context.fillStyle = light.played ? 'oklch(70% 0.02 269)' : 'oklch(34% 0.014 269)';
        context.fillRect(x, middle - 1, BAR_WIDTH_PX, 2);
        continue;
      }
      const barHeight = Math.max(3, (shaped.levels[index] ?? 0) * maximumHeight);
      context.fillStyle = light.played
        ? colourCss(liftLightness(spectralColour(shaped.centroids[index] ?? 0.5), light.lift))
        : `oklch(${light.lightness.toFixed(1)}% 0.012 269)`;
      const y = middle - barHeight / 2;
      context.beginPath();
      context.roundRect(x, y, BAR_WIDTH_PX, barHeight, 1);
      context.fill();
    }

    const playheadX = left + position * renderedWidth;
    const playheadColour = colourCss(
      liftLightness(spectralColour(centroidAt(smoothedCentroids, position)), 6),
    );
    context.save();
    context.shadowColor = playheadColour;
    context.shadowBlur = 12 + 14 * pulse;
    context.fillStyle = playheadColour;
    context.fillRect(playheadX - 1, middle - maximumHeight / 2 - 5, 2, maximumHeight + 10);
    context.beginPath();
    context.arc(playheadX, middle, 3 + pulse, 0, Math.PI * 2);
    context.fill();
    context.restore();

    if (hover !== null && !hero) {
      const hoverX = left + hover * renderedWidth;
      context.fillStyle = 'oklch(96% 0.004 269 / 0.35)';
      context.fillRect(hoverX, middle - maximumHeight / 2 - 4, 1, maximumHeight + 8);
    }

    const samplePosition = hover ?? position;
    const sampleIndex = Math.min(bars - 1, Math.max(0, Math.floor(samplePosition * bars)));
    const centroid = centroidAt(smoothedCentroids, samplePosition);
    onSample({
      elapsedMs: samplePosition * track.durationMs,
      remainingMs: Math.max(0, track.durationMs - samplePosition * track.durationMs),
      centroid,
      level: shaped.levels[sampleIndex] ?? 0,
      colour: spectralColour(centroid),
    });
  };

  return {
    draw,
    setHover(position) {
      hover = position === null ? null : clampUnit(position);
    },
  };
}

export function registerSeekRenderer(renderer: RegisteredRenderer): () => void {
  renderers.add(renderer);
  requestSeekFrame();
  return () => renderers.delete(renderer);
}

/** Draw visible seek surfaces from the page's one animation-frame owner. */
export function drawSeekTracks(timestamp: number, still: boolean): boolean {
  let hasVisibleRenderer = false;
  for (const renderer of renderers) {
    if (!renderer.isVisible()) continue;
    hasVisibleRenderer = true;
    renderer.draw(timestamp, still);
  }
  return hasVisibleRenderer && !still;
}

export function requestSeekFrame(): void {
  if (typeof window !== 'undefined') window.dispatchEvent(new Event(SEEK_FRAME_EVENT));
}
