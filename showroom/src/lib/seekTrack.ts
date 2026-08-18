const SEEK_BUCKET_COUNT = 1_000;
const DURATION_BYTE_COUNT = 4;
export const SEEK_TRACK_BYTE_COUNT = DURATION_BYTE_COUNT + SEEK_BUCKET_COUNT * 2;
const BASE_URL = import.meta.env?.BASE_URL ?? '/reprise/';
export const SEEK_TRACK_PATH = `${BASE_URL}media/showroom/seek-track.bin`;

export interface MeasuredSeekTrack {
  readonly durationMs: number;
  readonly peaks: Uint8Array;
  readonly centroids: Uint8Array;
}

export function parseSeekTrack(buffer: ArrayBuffer): MeasuredSeekTrack {
  if (buffer.byteLength !== SEEK_TRACK_BYTE_COUNT) {
    throw new Error(`Measured seek track has ${buffer.byteLength} bytes`);
  }
  const durationMs = new DataView(buffer).getUint32(0, true);
  if (durationMs === 0) throw new Error('Measured seek track has no duration');
  return {
    durationMs,
    peaks: new Uint8Array(
      buffer.slice(DURATION_BYTE_COUNT, DURATION_BYTE_COUNT + SEEK_BUCKET_COUNT),
    ),
    centroids: new Uint8Array(buffer.slice(DURATION_BYTE_COUNT + SEEK_BUCKET_COUNT)),
  };
}

let pendingTrack: Promise<MeasuredSeekTrack> | undefined;

/** Fetch both seek surfaces' one shared measured track. */
export function loadSeekTrack(): Promise<MeasuredSeekTrack> {
  pendingTrack ??= fetch(SEEK_TRACK_PATH).then(async (response) => {
    if (!response.ok) throw new Error(`Measured seek track returned ${response.status}`);
    return parseSeekTrack(await response.arrayBuffer());
  });
  return pendingTrack;
}

export function formatSeekTime(milliseconds: number): string {
  const totalSeconds = Math.max(0, Math.round(milliseconds / 1_000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = String(totalSeconds % 60).padStart(2, '0');
  return `${minutes}:${seconds}`;
}
