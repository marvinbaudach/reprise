import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

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
