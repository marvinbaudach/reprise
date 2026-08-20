import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { readout, toggle } from '../src/lib/mergeGates.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;
const repoRoot = new URL('../..', import.meta.url).pathname;

const GATE_SCRIPT = join(repoRoot, 'scripts', 'check-merge-readiness.sh');
const INCIDENT_RECORD = join(repoRoot, 'docs', 'plans', 'queue-anchor-grill-followups.md');
const STYLE_SOURCE = join(repoRoot, 'crates', 'reprise-gnome', 'src', 'ui', 'style', 'mod.rs');

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

const prerenderedPage = () => readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');

const chapterTwo = async () => {
  const html = await prerenderedPage();
  const section = html.match(/<section id="ch-02"[\s\S]+?<section id="ch-03"/)?.[0];
  assert.ok(section, 'the prerendered page must carry CH.02 ahead of CH.03');
  return section;
};

/**
 * The gate names, derived here a second time and independently of the build. If
 * the page and this list disagree, one of them was not rebuilt — which is the
 * whole point of deriving instead of typing.
 */
async function gateNames() {
  const script = await readFile(GATE_SCRIPT, 'utf8');
  const names = [...script.matchAll(/^gate "([^"]+)"/gm)].map((match) => match[1]);
  assert.ok(names.length > 0, 'the gate script must carry gate calls');
  return names;
}

/** Source files that put the gate count in front of a reader. */
async function displaySources() {
  const files = [join(showroomRoot, 'src', 'data', 'measurements.ts')];
  const roots = [join(showroomRoot, 'src', 'components')];
  while (roots.length) {
    const dir = roots.pop();
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) roots.push(full);
      else if (/\.tsx?$/.test(entry.name)) files.push(full);
    }
  }
  return files;
}

test('show-6 the gate strip carries one mark per gate call, named in script order', async () => {
  const chapter = await chapterTwo();
  const names = await gateNames();

  const marks = [...chapter.matchAll(/data-gate="([^"]*)"/g)].map((match) => match[1]);
  assert.equal(
    marks.length,
    names.length,
    'every gate call in the script must reach the strip exactly once',
  );
  assert.deepEqual(marks, names, 'the marks must follow the script, not an authored order');

  // The wall of visible labels is gone — a name is what a mark announces, not
  // what it prints. Reading them off the surface meant nothing to anyone
  // outside this repository.
  assert.doesNotMatch(chapter, /gate-wall/);
  for (const [index, name] of names.entries()) {
    const label = `${String(index + 1).padStart(2, '0')} · ${name}`;
    assert.ok(
      chapter.includes(`aria-label="${label}"`),
      `mark ${index + 1} must announce itself as "${label}"`,
    );
  }
});

test('show-7 the incident is quoted from the record, never recounted from the tree', async () => {
  const chapter = await chapterTwo();
  const record = await readFile(INCIDENT_RECORD, 'utf8');
  const style = await readFile(STYLE_SOURCE, 'utf8');

  // The quote is the chapter's load-bearing claim. It must still be the doc
  // comment's own words, character for character.
  const QUOTE =
    'A geometry assertion against unstyled widgets passes while the shipped button is a different size.';
  assert.ok(
    style
      .replace(/^\s*\/\/\/ ?/gm, '')
      .replace(/\s+/g, ' ')
      .includes(QUOTE),
  );
  assert.ok(
    chapter
      .replace(/&#x27;|&quot;|[“”]/g, '')
      .replace(/<!-- -->/g, '')
      .replace(/\s+/g, ' ')
      .includes(QUOTE),
    'CH.02 must quote the doc comment verbatim',
  );

  // Three heights, and all three are reported in §1 of the record. A figure
  // that grew a fourth number would be stating one, not quoting it.
  const normalizedChapter = chapter.replace(/<!-- -->/g, '');
  const drawn = [
    ...normalizedChapter.matchAll(/<span class="data incident-panel__value">(\d+) px<\/span>/g),
  ].map((match) => Number(match[1]));
  assert.deepEqual(drawn, [20, 34, 36, 36], 'the two panels must carry exactly four bars');
  const heights = new Set(drawn);
  assert.deepEqual(
    [...heights].sort((a, b) => a - b),
    [20, 34, 36],
  );
  assert.ok(record.includes('header_samples=[20.0, 34.0]'), 'the record must report 20 and 34');
  assert.ok(
    record.includes('SECTION_HEADER_MIN_HEIGHT: i32 = 36'),
    'the record must report the 36px floor',
  );

  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'chapters', 'ChapterTwo.tsx'),
    'utf8',
  );
  assert.match(source, /const FLOOR_PX = 36/);
  assert.match(source, /\{ px: 20,/);
  assert.match(source, /\{ px: 34,/);
  assert.doesNotMatch(
    source,
    /INCIDENT\.(?:measured|floor)/,
    'only the conflicting date may be derived from the incident record',
  );

  // Both links have to reach a real anchor, not the file's top.
  assert.match(chapter, /style\/mod\.rs#L41-L45/);
  assert.match(chapter, /queue-anchor-grill-followups\.md#c-gate-the-444-claim/);
  const heading = '### C. Gate the #444 claim on mutations, not on a green test';
  assert.ok(record.includes(heading), 'the §4C heading the link derives its fragment from moved');
});

test('show-8 a failed check blocks the readout and clearing it releases again', () => {
  const total = 26;

  const ready = readout(new Set(), total);
  assert.equal(ready.blocked, false);
  assert.match(ready.message, /^26 checks green · ready to merge$/);

  const one = readout(new Set(['Rust lint']), total);
  assert.equal(one.blocked, true);
  assert.equal(one.failed, 1);
  assert.match(one.message, /^1 of 26 red · the change does not land$/);

  const three = readout(new Set(['Rust lint', 'Shell', 'AppStream']), total);
  assert.equal(three.failed, 3);
  assert.match(three.message, /^3 of 26 red · the change does not land$/);

  // The toggle is what the mark does, and it must not mutate what it was given.
  const before = new Set(['Shell']);
  const after = toggle(before, 'Shell');
  assert.deepEqual([...before], ['Shell']);
  assert.equal(after.size, 0);
  assert.equal(readout(after, total).blocked, false);
  assert.equal(toggle(after, 'Shell').size, 1);
});

test('show-9 reduced motion leaves the figure and the strip without travel', async () => {
  const css = await builtCss();
  const sourceCss = await readFile(
    join(showroomRoot, 'src', 'components', 'chapters', 'ChapterTwo.css'),
    'utf8',
  );

  // A measurement may not arrive by moving: a bar that grows into place is a
  // bar whose value the reader watched change. The figure keeps the opacity
  // half of the shared reveal and drops its transform, in every motion setting.
  assert.match(css, /\.incident-figure\[data-reveal\]\{transform:none *!important\}/);

  const guarded = css.match(
    /@media\s*\(prefers-reduced-motion:reduce\)\{(?:[^{}]|\{[^{}]*\})*\.gate-strip__tick(?:[^{}]|\{[^{}]*\})*\}/,
  );
  assert.ok(guarded, 'a reduced-motion query must govern the gate strip');
  assert.match(guarded[0], /transition:none/);

  const touch = sourceCss.match(
    /@media \(hover: none\), \(max-width: 46rem\) \{[\s\S]*?\.gate-strip__tick \{[\s\S]*?\n {2}\}\n/,
  )?.[0];
  assert.ok(touch, 'a touch-width media query must widen the gate buttons');
  assert.match(touch, /\.gate-strip__tick \{\s*width: 44px;/);
});

test('show-10 the gate count is nowhere a literal, not even in the meta description', async () => {
  const names = await gateNames();
  const count = String(names.length);

  // Every place a reader meets the number, it has to have been derived.
  for (const file of await displaySources()) {
    const source = await readFile(file, 'utf8');
    assert.doesNotMatch(
      source,
      new RegExp(`(?<![\\d.])${count}(?![\\d.])`),
      `${file} types the gate count instead of deriving it`,
    );
  }

  // The meta description is the first number a reader sees — in a search
  // result, in every link unfurl — and it used to be the only typed one.
  const template = await readFile(join(showroomRoot, 'index.html'), 'utf8');
  assert.match(template, /content="[^"]*%GATE_COUNT% gates/);

  const built = await prerenderedPage();
  assert.doesNotMatch(built, /%GATE_COUNT%/, 'the placeholder must be filled at build time');
  assert.match(
    built,
    new RegExp(`<meta\\s+name="description"\\s+content="[^"]*decided by ${count} gates`),
  );
});

test('show-18 the six gate groups partition every parsed check exactly once', async () => {
  const chapter = await chapterTwo();
  const names = await gateNames();
  const cells = [...chapter.matchAll(/<article class="gate-group"[\s\S]*?<\/article>/g)].map(
    (match) => match[0],
  );

  assert.equal(cells.length, 6, 'the coverage figure must have exactly six groups');

  const assigned = [];
  let counted = 0;
  for (const cell of cells) {
    const count = Number(cell.match(/data-gate-count="(\d+)"/)?.[1]);
    const gates = cell.match(/data-gates="([^"]*)"/)?.[1]?.split('|') ?? [];
    assert.ok(Number.isInteger(count), 'every group must expose its derived count');
    assert.equal(count, gates.length, 'a group count must come from its assigned checks');
    counted += count;
    assigned.push(...gates);
  }

  assert.equal(counted, names.length, 'the six counts must sum to GATES.length');
  assert.deepEqual(
    [...assigned].sort(),
    [...names].sort(),
    'every parsed check must be assigned once, with no omissions or duplicates',
  );
});
