import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const chapterThreeSource = join(showroomRoot, 'src', 'components', 'chapters', 'ChapterThree.tsx');
const mounted = /ShowreelFilm/.test(await readFile(chapterThreeSource, 'utf8'));
const filmDir = mounted
  ? join(showroomRoot, 'public', 'media', 'showreel')
  : join(showroomRoot, 'media', 'showreel');
const filmFiles = [
  'showreel-1080.mp4',
  'showreel-1080.webm',
  'showreel-720.mp4',
  'showreel-720.webm',
  'showreel-poster.jpg',
  'showreel-poster.webp',
  'showreel.vtt',
];

/** The cut, to the frame. Every caption cue has to land inside it. */
const FILM_SECONDS = 60.0;

test('CH.03 does not carry the film yet', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const chapter = html.match(/<section id="ch-03"[\s\S]+?<section id="ch-04"/)?.[0];
  assert.ok(chapter);

  // The film is finished code but not finished work. Keep it off the public
  // page until someone decides it is ready, without treating it as abandoned.
  assert.doesNotMatch(chapter, /data-showcase="showreel-film"/);
  assert.doesNotMatch(chapter, /<video/);
  assert.match(chapter, /data-layout="design-mosaic"/);
});

test('the film never starts itself', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'ShowreelFilm.tsx'),
    'utf8',
  );

  // A self-start is a client-side effect, and the prerenderer never runs
  // effects, so the built page cannot expose this regression.
  assert.doesNotMatch(source, /IntersectionObserver/);
  assert.doesNotMatch(source, /useEffect/);
});

test('the encodes are served exactly when the film is on the page', async () => {
  const publicFilmDir = join(showroomRoot, 'public', 'media', 'showreel');

  if (mounted) {
    for (const name of filmFiles) await stat(join(publicFilmDir, name));
    return;
  }

  await assert.rejects(stat(publicFilmDir), { code: 'ENOENT' });
  const repositoryFilmDir = join(showroomRoot, 'media', 'showreel');
  for (const name of filmFiles) await stat(join(repositoryFilmDir, name));
});

test('every file the film section names is present in the repository', async () => {
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
