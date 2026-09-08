import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const chapterThreeSource = join(showroomRoot, 'src', 'components', 'chapters', 'ChapterThree.tsx');
// Intentionally loose: a false positive fails loudly, while a false negative would fail silently.
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
];

test('CH.03 closes on the film, in place of the mosaic it replaced', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const chapter = html.match(/<section id="ch-03"[\s\S]+?<section id="ch-04"/)?.[0];
  assert.ok(chapter);

  // The film is the chapter's closing statement now. The screenshot mosaic it
  // replaced must be gone from the page — leaving both would say the same thing
  // twice, once in stills and once in motion.
  assert.match(chapter, /data-showcase="showreel-film"/);
  assert.match(chapter, /<video/);
  assert.doesNotMatch(chapter, /data-layout="design-mosaic"/);
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

test('the film runs once, with sound, and offers itself again', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'ShowreelFilm.tsx'),
    'utf8',
  );
  // Up to the first <source>: the arrow functions in the handlers make a
  // non-greedy match on `>` stop inside the open tag.
  const element = source.match(/<video[\s\S]+?<source/)?.[0];
  assert.ok(element);

  // A reader who presses play asked for the film, sound included, and asked for
  // it once — so neither attribute may come back onto the element.
  assert.doesNotMatch(element, /^\s*muted$/m);
  assert.doesNotMatch(element, /^\s*loop$/m);

  // Running out is a state of its own: the last frame stays up, and the button
  // has to offer the film again from the top.
  assert.match(element, /onEnded=/);
  assert.match(source, /currentTime = 0/);

  // The cut fades its end card to black, so the true last frame is an empty
  // rectangle. Coming to rest has to mean the card, not the black after it.
  assert.match(source, /duration - END_CARD_HOLD_SECONDS/);
});

test('nothing is left of the caption track', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'ShowreelFilm.tsx'),
    'utf8',
  );

  // The CC toggle went unused, so the track went with it. A <track> that comes
  // back without its `showreel.vtt` would be a 404 on every play.
  assert.doesNotMatch(source, /<track/);
  assert.doesNotMatch(source, /textTracks/);
  assert.doesNotMatch(source, /\.vtt/);
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
  ]) {
    const info = await stat(join(filmDir, name));
    assert.ok(info.size > 0, `${name} is empty`);
  }
});

test('the ladder stays a ladder, and no step of it runs away', async () => {
  // Mounting the film puts these bytes into the deploy, and nothing in CI weighs
  // them. A visitor downloads exactly one encode, so the cap is per file — but
  // the smaller step has to stay the smaller step, or the ladder is decoration.
  const weigh = async (name) => (await stat(join(filmDir, name))).size;

  for (const name of ['showreel-1080.mp4', 'showreel-1080.webm']) {
    assert.ok((await weigh(name)) < 8_000_000, `${name} exceeds eight megabytes`);
  }
  assert.ok((await weigh('showreel-720.mp4')) < (await weigh('showreel-1080.mp4')));
  assert.ok((await weigh('showreel-720.webm')) < (await weigh('showreel-1080.webm')));
  assert.ok((await weigh('showreel-poster.webp')) < 200_000);
});
