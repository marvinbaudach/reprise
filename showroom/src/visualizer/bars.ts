import { hslaToRgb } from './color.ts';
import { BAND_COUNT, type VisualizerFrame } from './engine.ts';

export const SEGMENT_COUNT = 16;
export const HORIZONTAL_MARGIN = 0.045;
export const BAR_GAP = 0.0025;
export const BASELINE = 0.82;
export const MAX_HEIGHT = 0.68;
export const SEGMENT_GAP = 2.5;
export const PEAK_CAP_HEIGHT = 2.5;
export const PEAK_MIN = 0.04;
export const REFLECTION_SEGMENTS = 6;
export const HUE_START = 188;
export const HUE_END = 315;
export const BASS_GLOW_ALPHA = 0.35;
export const BASS_GLOW_RADIUS = 0.44;

const BACKGROUND = 'rgb(19,24,26)';
const NEON_SATURATION = 0.88;
const NEON_LIGHTNESS = 0.6;
const BAR_GLOW_THRESHOLD = 0.1;
const BAR_GLOW_ALPHA_MIN = 0.08;
const BAR_GLOW_ALPHA_RANGE = 0.13;
const BAR_GLOW_RADIUS_MIN = 1.25;
const BAR_GLOW_RADIUS_RANGE = 0.55;
const SEGMENT_ALPHA = 0.96;
const REFLECTION_HEIGHT = 0.42;
const REFLECTION_ALPHA = 0.13;
const REFLECTION_ALPHA_STEP = 0.02;
const REFLECTION_GAP = 2;
const PEAK_WIDTH_INSET = 0.08;
const PEAK_WIDTH = 0.84;
const PEAK_ALPHA_MIN = 0.38;
const PEAK_ALPHA_RANGE = 0.48;
const PEAK_COLOR = '240,245,255';
const MIN_GRADIENT_RADIUS = 0.01;

function clampUnit(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function smoothstep(value: number): number {
  const clamped = clampUnit(value);
  return clamped * clamped * (3 - 2 * clamped);
}

function neonRgb(bar: number): string {
  const across = bar / (BAND_COUNT - 1);
  const hue = HUE_START + (HUE_END - HUE_START) * across;
  return hslaToRgb(hue, NEON_SATURATION, NEON_LIGHTNESS).join(',');
}

function neon(bar: number, alpha: number): string {
  return `rgba(${neonRgb(bar)},${clampUnit(alpha).toFixed(4)})`;
}

function drawGlow(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  centerX: number,
  centerY: number,
  radius: number,
  bar: number,
  alpha: number,
): void {
  const rgb = neonRgb(bar);
  const gradient = context.createRadialGradient(
    centerX,
    centerY,
    0,
    centerX,
    centerY,
    Math.max(MIN_GRADIENT_RADIUS, radius),
  );
  gradient.addColorStop(0, `rgba(${rgb},${clampUnit(alpha).toFixed(4)})`);
  gradient.addColorStop(1, `rgba(${rgb},0)`);
  context.fillStyle = gradient;
  context.fillRect(0, 0, width, height);
}

export function drawBars(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  frame: VisualizerFrame,
): void {
  const margin = width * HORIZONTAL_MARGIN;
  const gap = width * BAR_GAP;
  const barWidth = (width - margin * 2 - gap * (BAND_COUNT - 1)) / BAND_COUNT;
  const baseline = height * BASELINE;
  const maxHeight = height * MAX_HEIGHT;
  const segmentHeight = (maxHeight - SEGMENT_GAP * (SEGMENT_COUNT - 1)) / SEGMENT_COUNT;

  context.setTransform(1, 0, 0, 1, 0, 0);
  context.fillStyle = BACKGROUND;
  context.fillRect(0, 0, width, height);

  if (frame.bassImpact > 0) {
    const radius = Math.max(width, height) * BASS_GLOW_RADIUS;
    drawGlow(
      context,
      width,
      height,
      width * 0.28,
      height * 0.68,
      radius,
      0,
      BASS_GLOW_ALPHA * frame.bassImpact,
    );
    drawGlow(
      context,
      width,
      height,
      width * 0.72,
      height * 0.68,
      radius,
      BAND_COUNT - 1,
      BASS_GLOW_ALPHA * frame.bassImpact,
    );
  }

  for (let bar = 0; bar < BAND_COUNT; bar += 1) {
    const x = margin + bar * (barWidth + gap);
    const value = frame.bars[bar] ?? 0;
    const active = Math.min(SEGMENT_COUNT, Math.ceil(value * SEGMENT_COUNT));
    const fraction = value * SEGMENT_COUNT;

    if (value > BAR_GLOW_THRESHOLD) {
      const top = baseline - value * maxHeight;
      drawGlow(
        context,
        width,
        height,
        x + barWidth / 2,
        top + segmentHeight,
        barWidth * (BAR_GLOW_RADIUS_MIN + value * BAR_GLOW_RADIUS_RANGE),
        bar,
        BAR_GLOW_ALPHA_MIN + value * BAR_GLOW_ALPHA_RANGE,
      );
    }

    for (let segment = 0; segment < active; segment += 1) {
      const transition = smoothstep(fraction - segment);
      const y = baseline - (segment + 1) * (segmentHeight + SEGMENT_GAP);
      context.fillStyle = neon(bar, SEGMENT_ALPHA * transition);
      context.fillRect(x, y, barWidth, segmentHeight);

      if (segment < REFLECTION_SEGMENTS) {
        const reflectionHeight = segmentHeight * REFLECTION_HEIGHT;
        context.fillStyle = neon(
          bar,
          (REFLECTION_ALPHA - segment * REFLECTION_ALPHA_STEP) * transition,
        );
        context.fillRect(
          x,
          baseline + SEGMENT_GAP + segment * (reflectionHeight + REFLECTION_GAP),
          barWidth,
          reflectionHeight,
        );
      }
    }

    const peak = frame.peaks[bar] ?? 0;
    if (peak > PEAK_MIN) {
      context.fillStyle = `rgba(${PEAK_COLOR},${(PEAK_ALPHA_MIN + peak * PEAK_ALPHA_RANGE).toFixed(4)})`;
      context.fillRect(
        x + barWidth * PEAK_WIDTH_INSET,
        baseline - peak * maxHeight - PEAK_CAP_HEIGHT,
        barWidth * PEAK_WIDTH,
        PEAK_CAP_HEIGHT,
      );
    }
  }
}
