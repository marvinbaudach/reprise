import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const root = new URL('..', import.meta.url);

test('the overview names its author and puts the film before technical evidence', async () => {
  const html = await readFile(new URL('dist/index.html', root), 'utf8');
  const hero = html.match(/data-showcase="design-hero"[\s\S]*?<\/section>/)?.[0];
  assert.ok(hero);
  assert.match(
    html,
    /<video[^>]+controls=""/,
    'native video controls are available before JavaScript enhancement',
  );
  assert.match(hero, /Marvin Baudach/);
  assert.match(hero, /href="#film"/);
  assert.ok(html.indexOf('id="film"') < html.indexOf('id="ch-01"'));
  assert.match(html, /<summary[^>]*>Code breakdown/);
  assert.match(html, /<summary[^>]*>Read the quality case study/);
  assert.match(html, /<summary[^>]*>Sources and methodology/);
});

test('reading never depends on entrances or count-up effects', async () => {
  const source = await readFile(new URL('src/hooks/usePageChoreography.ts', root), 'utf8');
  assert.doesNotMatch(
    source,
    /prepareReveals|sweepReveals|prepareCounter|runCounter|moveOil|onPointerMove/,
  );
  const backdrop = await readFile(new URL('src/components/chrome/backdrop.css', root), 'utf8');
  assert.doesNotMatch(backdrop, /animation:|transition:|@keyframes/);
});

test('architecture and quality diagrams do not run decorative loops', async () => {
  for (const path of [
    'src/components/architecture/architecture.css',
    'src/components/chapters/ChapterTwo.css',
  ]) {
    const source = await readFile(new URL(path, root), 'utf8');
    assert.doesNotMatch(source, /animation: (?:architecture|pipeline)-flow/);
  }
});

/**
 * The page is a pitch, not a changelog. The prose a visitor sees without
 * opening a disclosure — paragraphs, list items, captions — has a word budget,
 * and no visible paragraph may grow past a quick read. Diagram labels and the
 * ledger table are figures, not prose, and do not count. A newly landed feature
 * earns a line in a feature list, never another paragraph; depth belongs
 * inside a `<details>`. The ledger price is exempt because it is quoted
 * verbatim from its measurement record.
 *
 * Set on 2026-11-04 at about 495 words of page copy, plus roughly 45 for the
 * "Coming from Rhythmbox?" section strand C of the community launch adds, plus
 * a small margin. Raising it needs a reason, not a new feature paragraph.
 */
const VISIBLE_PROSE_BUDGET = 560;
const PARAGRAPH_WORD_LIMIT = 30;

function visibleText(html) {
  return html
    .replace(/<(script|style)\b[\s\S]*?<\/\1>/g, '')
    .replace(/<details\b[\s\S]*?<\/details>/g, '');
}

function proseBlocks(html) {
  const visible = visibleText(html.slice(html.indexOf('<body')));
  return [...visible.matchAll(/<(p|li|figcaption)\b([^>]*)>([\s\S]*?)<\/\1>/g)]
    .map(([, , attributes, inner]) => ({ attributes, inner }))
    .filter(({ attributes }) => !/ledger__price/.test(attributes));
}

function countWords(fragment) {
  return fragment
    .replace(/<[^>]+>/g, ' ')
    .split(/\s+/)
    .filter(Boolean).length;
}

test('the prose visible without a disclosure stays within its word budget', async () => {
  const html = await readFile(new URL('dist/index.html', root), 'utf8');
  const words = proseBlocks(html).reduce((sum, { inner }) => sum + countWords(inner), 0);
  assert.ok(
    words <= VISIBLE_PROSE_BUDGET,
    `${words} visible prose words exceed the budget of ${VISIBLE_PROSE_BUDGET}`,
  );
});

test('no visible paragraph or caption is longer than a quick read', async () => {
  const html = await readFile(new URL('dist/index.html', root), 'utf8');
  for (const { inner } of proseBlocks(html)) {
    const words = countWords(inner);
    const start = inner.replace(/<[^>]+>/g, '').slice(0, 60);
    assert.ok(words <= PARAGRAPH_WORD_LIMIT, `${words} words: "${start}…"`);
  }
});
