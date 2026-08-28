import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const filmDir = join(showroomRoot, 'public', 'media', 'showreel');

/** The cut, to the frame. Every caption cue has to land inside it. */
const FILM_SECONDS = 60.0;

test('CH.03 carries the film where the screenshot mosaic used to be', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const chapter = html.match(/<section id="ch-03"[\s\S]+?<section id="ch-04"/)?.[0];
  assert.ok(chapter);

  assert.match(chapter, /data-showcase="showreel-film"/);
  assert.doesNotMatch(chapter, /data-layout="design-mosaic"/);

  // Both codecs, both ladder steps, and the small step offered first so the
  // element picks it before it ever considers the 1080 file.
  for (const source of [
    'showreel-720.webm',
    'showreel-1080.webm',
    'showreel-720.mp4',
    'showreel-1080.mp4',
  ]) {
    assert.match(chapter, new RegExp(source.replace('.', '\\.')));
  }
  assert.ok(
    chapter.indexOf('showreel-720.webm') < chapter.indexOf('showreel-1080.webm'),
    'the smaller step has to be offered before the larger one',
  );

  // Muted, and with a control that can undo that. A landing page does not get
  // to make noise at a reader who has not asked for it.
  assert.match(chapter, /<video[^>]*\bmuted\b/);
  assert.match(chapter, /Sound on/);
  assert.match(chapter, /Play the film|>Play</);

  // Nothing here may be eager: the page-wide count of two belongs to the hero.
  assert.doesNotMatch(chapter, /loading="eager"/);

  assert.match(chapter, /poster="[^"]*showreel-poster\.webp"/);
  assert.match(chapter, /kind="captions"/);
});

test('every file the film section names is actually shipped', async () => {
  for (const name of [
    'showreel-720.webm',
    'showreel-1080.webm',
    'showreel-720.mp4',
    'showreel-1080.mp4',
    'showreel-poster.webp',
    'showreel.vtt',
  ]) {
    const info = await stat(join(filmDir, name));
    assert.ok(info.size > 0, `${name} is empty`);
  }
});

test('the caption cues stay inside the cut and never run backwards', async () => {
  const vtt = await readFile(join(filmDir, 'showreel.vtt'), 'utf8');
  assert.match(vtt, /^WEBVTT/);

  const seconds = (stamp) => {
    const [h, m, s] = stamp.split(':');
    return Number(h) * 3600 + Number(m) * 60 + Number(s);
  };
  const cues = [...vtt.matchAll(/^(\d\d:\d\d:\d\d\.\d\d\d) --> (\d\d:\d\d:\d\d\.\d\d\d)$/gm)].map(
    ([, from, to]) => [seconds(from), seconds(to)],
  );

  assert.ok(cues.length >= 10, `expected the shot list, found ${cues.length} cues`);
  let previousEnd = 0;
  for (const [from, to] of cues) {
    assert.ok(from < to, `a cue at ${from}s does not move forward`);
    assert.ok(from >= previousEnd, `a cue at ${from}s starts before the one before it ended`);
    assert.ok(to <= FILM_SECONDS, `a cue ends at ${to}s, past the ${FILM_SECONDS}s cut`);
    previousEnd = to;
  }
  assert.equal(previousEnd, FILM_SECONDS, 'the last cue has to reach the end of the cut');
});
