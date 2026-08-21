import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { census } from '../derive/code-census.mjs';

const showroomRoot = new URL('..', import.meta.url).pathname;
const repoRoot = new URL('../..', import.meta.url).pathname;

const LEDGER_DOC = join(repoRoot, 'docs', 'measurements', 'index-rebuild.md');

const prerenderedPage = () => readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');

/** The page's own grouping, spelled out again so the test does not import it. */
const group = (value) => String(Math.round(value)).replace(/\B(?=(\d{3})+(?!\d))/g, "'");
const escaped = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Every source file that puts a figure in front of a reader. */
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

/**
 * The ledger, parsed here a second time and independently of the build.
 * The header and the separator share a row's shape, so both are dropped.
 */
async function recordedLedger() {
  const text = await readFile(LEDGER_DOC, 'utf8');
  const rows = [
    ...text.matchAll(/^\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|([^|]+)\|\s*$/gm),
  ]
    .map(([, what, before, after, delta, commit, date, method]) => ({
      what: what.trim(),
      before: before.trim(),
      after: after.trim(),
      delta: delta.trim(),
      commit: commit.trim(),
      date: date.trim(),
      method: method.trim(),
    }))
    .filter((row) => row.what !== 'What' && !/^-+$/.test(row.what));
  assert.ok(rows.length > 0, 'the measurement record must carry its table');
  return rows;
}

test('show-12 the line figures are counted from the tree, never typed', async () => {
  const counted = census(repoRoot);
  const html = await prerenderedPage();
  const chapter = html.match(/<section id="ch-01"[\s\S]+?<section id="ch-02"/)?.[0];
  assert.ok(chapter, 'chapter one must be prerendered');

  const total = group(counted.total);
  const share = `${((counted.test / counted.total) * 100).toFixed(1)} %`;
  // The prerender escapes the apostrophe, so accept either spelling.
  const shown = (value) => new RegExp(escaped(value).replaceAll("'", "(?:'|&#x27;)"));

  assert.match(chapter, shown(total));
  assert.match(chapter, shown(share));

  const segments = [
    counted.rust.product,
    counted.rust.test,
    counted.bridge.product + counted.bridge.test,
    counted.kotlin.product + counted.kotlin.test,
  ];
  const ratio = chapter.match(/<div[^>]+data-ratio="(?:true)?"[\s\S]+?<\/div>/)?.[0];
  assert.ok(ratio, 'the ratio bar must be prerendered');
  for (const lines of segments) {
    assert.match(chapter, shown(group(lines)), `${group(lines)} is missing from the legend`);
    const width = ((lines / counted.total) * 100).toFixed(1);
    assert.match(ratio, new RegExp(`data-w="${escaped(width)}"`));
  }
  // The four segments are the whole count, not a selection of it.
  assert.equal(
    segments.reduce((sum, lines) => sum + lines, 0),
    counted.total,
  );

  // No display source may carry any of those numbers, in either spelling.
  const forbidden = [counted.total, ...segments].flatMap((value) => [group(value), String(value)]);
  for (const file of await displaySources()) {
    const source = await readFile(file, 'utf8');
    for (const literal of forbidden) {
      assert.ok(
        !source.includes(literal),
        `${file} types the line count ${literal} — it must read the census`,
      );
    }
    for (const match of source.matchAll(/(?<![\d.'’])\d{1,2}\.\d ?%/g)) {
      const around = source.slice(Math.max(0, match.index - 90), match.index + 90);
      assert.ok(
        !/lines?|tests?|Rust|Kotlin/i.test(around),
        `${file} types a share next to the words it claims — it must read the census`,
      );
    }
  }

  assert.match(
    await readFile(join(showroomRoot, 'src', 'data', 'measurements.ts'), 'utf8'),
    /from 'virtual:code-census'/,
  );
});

test('show-13 the performance figures quote a record that carries their provenance', async () => {
  const rows = await recordedLedger();
  const html = await prerenderedPage();
  const chapter = html.match(/<section[^>]+data-chapter="05"[\s\S]+?<\/section>/)?.[0];
  assert.ok(chapter, 'chapter five must be prerendered');

  const shown = (value) => new RegExp(escaped(value).replaceAll("'", "(?:'|&#x27;)"));
  for (const row of rows) {
    for (const cell of [row.what, row.before, row.after, row.delta]) {
      assert.match(chapter, shown(cell), `the ledger is missing ${cell}`);
    }
    // Provenance is what makes a quoted figure different from a typed one.
    assert.match(row.commit, /^[0-9a-f]{7,40}$/, `${row.what} has no commit`);
    assert.match(row.date, /^\d{4}-\d{2}-\d{2}$/, `${row.what} has no ISO date`);
    assert.ok(row.method.length > 20, `${row.what} has no method worth the name`);
  }
  // One header row plus one row per measurement.
  assert.equal((chapter.match(/<tr(?:\s[^>]*)?>/g) ?? []).length, rows.length + 1);

  for (const file of await displaySources()) {
    const source = await readFile(file, 'utf8');
    for (const row of rows) {
      for (const cell of [row.before, row.after, row.delta]) {
        // A cell carrying a unit or a sign is distinctive enough to forbid
        // outright. A bare number is not — `0` and `419` would match half the
        // file — so those are forbidden only in the company of the words that
        // would be claiming them, the way SHOW-10 reads the gate count.
        if (/\D/.test(cell)) {
          assert.ok(
            !source.includes(cell),
            `${file} types the measured value ${cell} — it must read the record`,
          );
          continue;
        }
        // The row's own words, not a generic list: `measured` alone appears in
        // components that have nothing to do with this ledger.
        const words = row.what
          .toLowerCase()
          .split(/[^a-z]+/)
          .filter((word) => word.length >= 4);
        const literal = new RegExp(`(?<![\\d.'’])${cell}(?![\\d.'’%])`, 'g');
        for (const match of source.matchAll(literal)) {
          const around = source
            .slice(Math.max(0, match.index - 90), match.index + 90)
            .toLowerCase();
          const hits = words.filter((word) => around.includes(word)).length;
          assert.ok(
            hits < 2,
            `${file} types ${cell} next to the words it claims — it must read the record`,
          );
        }
      }
    }
  }

  assert.match(
    await readFile(join(showroomRoot, 'src', 'data', 'measurements.ts'), 'utf8'),
    /from 'virtual:measurements'/,
  );
});

test('show-14 the footer says of every group whether it is counted, quoted or stated', async () => {
  const html = await prerenderedPage();
  const footer = html.match(/<footer[^>]+class="site-footer"[\s\S]+?<\/footer>/)?.[0];
  assert.ok(footer, 'the footer must be prerendered');

  for (const kind of ['Counted:', 'Quoted:', 'Stated:']) {
    assert.ok(footer.includes(kind), `the footer must name what is ${kind.toLowerCase()}`);
  }
  // The promise this page used to make, and can no longer keep truthfully.
  assert.doesNotMatch(footer, /does not measure them yet/);
  assert.doesNotMatch(footer, /measurement runs in CI next/);
  // And it points at both sources rather than describing them.
  assert.match(footer, /code-census\.mjs/);
  assert.match(footer, /index-rebuild\.md/);
});
