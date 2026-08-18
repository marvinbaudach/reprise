import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { loadSeekTrack } from '../src/lib/seekTrack.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;
const trackPath = join(showroomRoot, 'public', 'media', 'showroom', 'seek-track.bin');
const bucketCount = 1_000;
const durationByteCount = 4;
const expectedByteCount = durationByteCount + 2 * bucketCount;

function range(values) {
  return [Math.min(...values), Math.max(...values)];
}

test('the spectral seek bar carries a complete measured track', async () => {
  const track = await readFile(trackPath);
  assert.equal(track.byteLength, expectedByteCount);

  const durationMs = track.readUInt32LE(0);
  assert.ok(durationMs >= 60_000, 'track duration must be at least one minute');
  assert.ok(durationMs <= 20 * 60_000, 'track duration must be at most twenty minutes');

  const peaks = track.subarray(durationByteCount, durationByteCount + bucketCount);
  const centroids = track.subarray(durationByteCount + bucketCount);
  const [minimumPeak, maximumPeak] = range(peaks);
  const [minimumCentroid, maximumCentroid] = range(centroids);
  assert.notEqual(minimumPeak, maximumPeak, 'waveform peaks must not be constant');
  assert.notEqual(minimumCentroid, maximumCentroid, 'centroid curve must not be constant');
});

test("a failed load does not become every later caller's answer", async () => {
  const track = await readFile(trackPath);
  const originalFetch = globalThis.fetch;
  try {
    globalThis.fetch = async () => ({ ok: false, status: 503 });
    await assert.rejects(loadSeekTrack(), /503/);

    globalThis.fetch = async () => ({
      ok: true,
      status: 200,
      arrayBuffer: async () =>
        track.buffer.slice(track.byteOffset, track.byteOffset + track.byteLength),
    });
    const loaded = await loadSeekTrack();
    assert.equal(loaded.durationMs, track.readUInt32LE(0));
    assert.equal(loaded.peaks.length, bucketCount);
  } finally {
    globalThis.fetch = originalFetch;
  }
});
