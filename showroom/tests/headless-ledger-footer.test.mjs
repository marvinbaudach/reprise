import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;

async function prerenderedPage() {
  return readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
}

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

async function sourceCss(file) {
  return readFile(join(showroomRoot, 'src', 'components', 'chapters', file), 'utf8');
}

test('chapter four presents the exact CLI commands and six MCP capability defaults', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const authoredCss = await sourceCss('ChapterFour.css');
  const chapter = html.match(/<section id="ch-04"[\s\S]+?<section[^>]+data-chapter="05"/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(13% 0\.016 269\)"/);
  assert.match(chapter, /Two frontends with no screen at all\./);
  assert.doesNotMatch(chapter, / style=/);
  for (const command of [
    'library summary',
    'search &quot;portishead&quot;',
    'playlist create &quot;Focus&quot;',
    'scan ~/Music',
    'instrumental create 481',
    'jobs status --batch b-2f9c',
    'events tail --since 0',
    'concerts list --all --json',
  ]) {
    assert.match(chapter, new RegExp(`reprise-cli ${command}`));
  }
  for (const [capability, state] of [
    ['library:read', 'on'],
    ['playback:control', 'on'],
    ['playlist:create', 'off'],
    ['playlist:manage', 'off'],
    ['sources:manage', 'off'],
    ['device:sync', 'off'],
  ]) {
    assert.match(
      chapter,
      new RegExp(
        `${capability}[\\s\\S]+?class="capability__state capability__state--${state}">${state}<`,
      ),
    );
  }
  assert.equal((chapter.match(/class="capability"/g) ?? []).length, 6);
  assert.match(
    css,
    /\.headless-grid\{[^}]*grid-template-columns:repeat\(auto-fit,minmax\(min\(100%,340px\),1fr\)/,
  );
  assert.match(
    css,
    /\.headless-card\{[^}]*border:1px solid oklch\(28% \.018 269\)[^}]*border-radius:12px/,
  );
  assert.match(authoredCss, /background: oklch\(17% 0\.016 269 \/ 0\.72\)/);
});

test('chapter five exposes the complete measured ledger and its price without a fold', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const authoredCss = await sourceCss('ChapterFive.css');
  const chapter = html.match(/<section[^>]+data-chapter="05"[\s\S]+?<\/section>/)?.[0];

  assert.ok(chapter);
  assert.match(chapter, /data-ground="oklch\(12\.5% 0\.018 24\)"/);
  assert.match(chapter, /Measured afterwards\. Price attached\./);
  assert.match(chapter, /<table[^>]+class="ledger"/);
  assert.doesNotMatch(chapter, /<details|Folded away/);
  assert.doesNotMatch(chapter, / style=/);
  // The figures themselves are quoted from the record, so they are read from it
  // here rather than typed a second time. SHOW-13 owns whether the quote is
  // faithful; this test owns that the ledger arrives whole and unfolded.
  const record = await readFile(
    join(showroomRoot, '..', 'docs', 'measurements', 'index-rebuild.md'),
    'utf8',
  );
  const rows = [
    ...record.matchAll(/^\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|\s*$/gm),
  ]
    .map((match) => match.slice(1, 5).map((cell) => cell.trim()))
    .filter(([what]) => what !== 'What' && !/^-+$/.test(what));
  assert.ok(rows.length > 0, 'the measurement record must carry its table');
  const shown = (value) =>
    new RegExp(value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replaceAll("'", "(?:'|&#x27;)"));
  for (const cells of rows) {
    for (const cell of cells) assert.match(chapter, shown(cell));
  }
  assert.equal((chapter.match(/<tr>/g) ?? []).length, rows.length + 1);
  // The price rides along with the table, out of the same record.
  const price = record.split('\n## The price\n')[1]?.split('\n#')[0]?.trim();
  assert.ok(price, 'the record must carry a price paragraph');
  for (const fragment of price.split(' ').filter((word) => /\d/.test(word))) {
    assert.match(chapter, shown(fragment));
  }
  assert.match(css, /\.chapter-five\{[^}]*padding:clamp\(4rem,3rem \+ 5vw,7rem\) 0/);
  assert.match(css, /\.ledger-card\{[^}]*border-radius:12px/);
  assert.match(authoredCss, /background: oklch\(17% 0\.016 269 \/ 0\.72\)/);
});

test('the design footer carries provenance availability and the exact contact treatment', async () => {
  const html = await prerenderedPage();
  const css = await builtCss();
  const footer = html.match(/<footer[^>]+class="site-footer"[\s\S]+?<\/footer>/)?.[0];

  assert.ok(footer);
  assert.match(footer, /data-ground="oklch\(10\.5% 0\.012 269\)"/);
  assert.match(footer, /Three kinds of number appear on this page/);
  assert.match(footer, /code-census\.mjs/);
  assert.match(footer, /index-rebuild\.md/);
  assert.match(footer, /src="\/reprise\/brand\/reprise-mark\.svg"/);
  assert.match(footer, /Availability/);
  assert.match(footer, /<section id="availability" class="availability"/);
  assert.match(footer, /Open to work\./);
  assert.match(footer, /github\.com\/marvinbaudach ↗/);
  assert.match(footer, /GPL-3\.0-or-later · active alpha/);
  assert.doesNotMatch(footer, / style=/);
  assert.match(
    css,
    /\.site-footer\{[^}]*padding:clamp\(3\.5rem,2\.5rem \+ 4vw,6rem\) 0 clamp\(3rem,2rem \+ 3vw,5rem\)/,
  );
  assert.match(
    css,
    /\.availability\{[^}]*grid-template-columns:repeat\(auto-fit,minmax\(min\(100%,300px\),1fr\)/,
  );
  // Lightning CSS (Vite 8's minifier) sorts declarations inside a block, so the
  // contact button is checked for what it carries, not for the order.
  const contact = css.match(/\.availability__contact\{[^}]*\}/)?.[0];
  assert.ok(contact, '.availability__contact must exist in the built CSS');
  for (const declaration of [
    'padding:14px 22px',
    'border:1px solid #4fdbd4',
    'border-radius:8px',
  ]) {
    assert.ok(contact.includes(declaration), `.availability__contact must carry ${declaration}`);
  }
});
