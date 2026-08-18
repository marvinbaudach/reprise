import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

test('chapter one carries the design figures counters and animated ratio band', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const chapter = html.match(/<section id="ch-01"[\s\S]+?<section id="ch-02"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(14\.5% 0\.016 258\)"/);
  for (const value of ["347(?:'|&#x27;)842", '45\\.8 %', '21']) {
    assert.match(chapter, new RegExp(`data-counter="(?:true)?">${value}<`));
  }
  assert.doesNotMatch(chapter, /data-counter="(?:true)?">1 → 4</);

  const ratio = chapter.match(/<div[^>]+data-ratio="(?:true)?"[\s\S]+?<\/div>/)?.[0];
  assert.ok(ratio);
  for (const width of ['49.6', '41.7', '2.9', '5.8']) {
    assert.match(ratio, new RegExp(`data-w="${width}"`));
  }
  assert.match(css, /\.ratio__bar\{[^}]*height:40px[^}]*border-radius:6px/);
  assert.match(css, /width 1\.4s cubic-bezier\(\.16,1,\.3,1\)/);
});

test('chapter two carries the design evidence and rulebook totals', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const chapter = html.match(/<section id="ch-02"[\s\S]+?<section id="ch-03"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(13\.5% 0\.02 205\)"/);
  for (const value of [
    "5(?:'|&#x27;)596",
    '676',
    '9',
    '11',
    '6',
    '571',
    '100 %',
    '897',
    '250',
    '24',
  ]) {
    assert.match(chapter, new RegExp(`data-counter="(?:true)?">${value}<`));
  }
  assert.match(chapter, /Evidence, weakest first/);
  assert.match(chapter, /Five levels of proof\. The top two are agents nobody scripted\./);
  assert.match(
    css,
    /\.rungs__rung\{[^}]*grid-template-columns:minmax\(90px,110px\) minmax\(180px,1\.1fr\) 1fr 1fr/,
  );
  assert.match(css, /\.agent-workflow(?:,\.exploration-loop)?\{[^}]*border-radius:12px/);
});

test('reduced motion settles every prepared counter at its authored value', async () => {
  const choreography = await readFile(
    join(showroomRoot, 'src', 'hooks', 'usePageChoreography.ts'),
    'utf8',
  );

  assert.match(choreography, /if \(still\) runCounter\(element, 0, true\)/);
});
