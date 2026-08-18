export type Rgb = readonly [number, number, number];

const HUE_CIRCLE = 360;
const HUE_SEGMENT = 60;
const BYTE_MAX = 255;

export function hslaToRgb(hue: number, saturation: number, lightness: number): Rgb {
  const normalizedHue = ((hue % HUE_CIRCLE) + HUE_CIRCLE) % HUE_CIRCLE;
  const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const intermediate = chroma * (1 - Math.abs(((normalizedHue / HUE_SEGMENT) % 2) - 1));
  const offset = lightness - chroma / 2;
  const segment = Math.floor(normalizedHue / HUE_SEGMENT);
  const rgb: Rgb =
    segment === 0
      ? [chroma, intermediate, 0]
      : segment === 1
        ? [intermediate, chroma, 0]
        : segment === 2
          ? [0, chroma, intermediate]
          : segment === 3
            ? [0, intermediate, chroma]
            : segment === 4
              ? [intermediate, 0, chroma]
              : [chroma, 0, intermediate];

  return [
    Math.round((rgb[0] + offset) * BYTE_MAX),
    Math.round((rgb[1] + offset) * BYTE_MAX),
    Math.round((rgb[2] + offset) * BYTE_MAX),
  ];
}
