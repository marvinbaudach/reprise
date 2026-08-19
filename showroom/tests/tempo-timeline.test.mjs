import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const repoRoot = new URL('../..', import.meta.url).pathname;

const TIMELINE_DOC = join(repoRoot, 'docs', 'showroom', 'timeline.md');

const prerenderedPage = () => readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

/**
 * The weeks, derived here a second time and independently of the build. If the
 * page and this list disagree, one of them was not rebuilt — which is the whole
 * point of reading a record instead of typing it.
 */
async function recordedWeeks() {
  const text = await readFile(TIMELINE_DOC, 'utf8');
  const rows = [
    ...text.matchAll(
      /^\|\s*(\d+)\s*\|\s*(\d{4}-\d{2}-\d{2})\s*…\s*(\d{4}-\d{2}-\d{2})\s*\|([^|]+)\|(.+?)\|\s*$/gm,
    ),
  ].map(([, week, from, to, theme, landed]) => ({
    week: Number(week),
    from,
    to,
    theme: theme.trim(),
    landed: landed.trim(),
  }));
  assert.ok(rows.length > 0, 'the timeline record must carry its table');
  return rows;
}

/** Every source file that puts something in front of a reader. */
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

test('show-11 the timeline names its weeks from the record, and nothing types them', async () => {
  const weeks = await recordedWeeks();
  const html = await prerenderedPage();

  const band = html.match(/<section class="tempo"[\s\S]+?<\/section>/)?.[0];
  assert.ok(band, 'the tempo band must be prerendered');

  // Order, not just membership: the rail claims to run in the order it happened.
  const shown = [...band.matchAll(/data-week="(\d+)"/g)].map((match) => Number(match[1]));
  assert.deepEqual(
    shown,
    weeks.map((week) => week.week),
  );
  assert.match(band, new RegExp(`data-weeks="${weeks.length}"`));
  assert.match(band, new RegExp(`data-counter="(?:true)?">${weeks.length}<`));

  for (const week of weeks) {
    assert.match(band, new RegExp(`>${week.theme}<`), `week ${week.week} is missing its theme`);
  }

  // The display span is computed from the ISO dates, so the record keeps one
  // spelling of every date.
  const months = [
    'Jan',
    'Feb',
    'Mar',
    'Apr',
    'May',
    'Jun',
    'Jul',
    'Aug',
    'Sep',
    'Oct',
    'Nov',
    'Dec',
  ];
  for (const week of weeks) {
    const [, fromMonth, fromDay] = week.from.split('-').map(Number);
    const [, toMonth, toDay] = week.to.split('-').map(Number);
    const span =
      fromMonth === toMonth
        ? `${fromDay}–${toDay} ${months[toMonth - 1]}`
        : `${fromDay} ${months[fromMonth - 1]} – ${toDay} ${months[toMonth - 1]}`;
    assert.ok(band.includes(span), `the band must carry the span ${span}`);
  }

  // And no component may type any of it.
  const themes = weeks.map((week) => week.theme);
  const count = String(weeks.length);
  for (const file of await displaySources()) {
    const source = await readFile(file, 'utf8');
    for (const theme of themes) {
      assert.ok(
        !new RegExp(`\\b${theme}\\b`).test(source),
        `${file} types the week name ${theme} — it must read the record`,
      );
    }
    for (const week of weeks) {
      for (const date of [week.from, week.to]) {
        assert.ok(!source.includes(date), `${file} types the date ${date}`);
      }
    }
    for (const match of source.matchAll(new RegExp(`(?<![\\d.'’])${count}(?![\\d.'’%])`, 'g'))) {
      const around = source.slice(Math.max(0, match.index - 90), match.index + 90);
      assert.ok(
        !/week/i.test(around),
        `${file} types ${count} next to the word it claims — it must read the record`,
      );
    }
  }

  const bandSource = await readFile(
    join(showroomRoot, 'src', 'components', 'chapters', 'TempoBand.tsx'),
    'utf8',
  );
  assert.match(bandSource, /from 'virtual:build-timeline'/);
});

test('show-15 reduced motion leaves the timeline rail drawn', async () => {
  const css = await builtCss();

  // Several unrelated reduced-motion queries sit in this stylesheet. Take the
  // one that governs the rail; a lazy walk from the first one would call an
  // escaped rule guarded.
  const prelude = /@media\s*\(prefers-reduced-motion:reduce\)/g;
  const guards = [];
  for (let from = 0; ; ) {
    prelude.lastIndex = from;
    const start = prelude.exec(css)?.index ?? -1;
    if (start === -1) break;
    let depth = 0;
    let end = -1;
    for (let index = css.indexOf('{', start); index < css.length; index += 1) {
      if (css[index] === '{') depth += 1;
      else if (css[index] === '}') {
        depth -= 1;
        if (depth === 0) {
          end = index;
          break;
        }
      }
    }
    assert.notEqual(end, -1, 'unbalanced reduced-motion query');
    guards.push(css.slice(start, end + 1));
    from = end + 1;
  }

  const mine = guards.filter((guard) => guard.includes('.tempo__week'));
  assert.equal(mine.length, 1, 'exactly one reduced-motion query may govern the rail');
  const guarded = mine[0];
  // The minifier writes `:after` for `::after`, so accept either spelling.
  assert.match(guarded, /\.tempo__week::?after/);
  assert.match(guarded, /transform:scaleX\(1\)/);
  assert.match(guarded, /transition:none/);
  // With `transition:none` there is no delay left to state, and Lightning CSS
  // folds an explicit `0s` away for exactly that reason. What must never survive
  // inside this query is a delay that actually waits.
  assert.doesNotMatch(guarded, /transition-delay:(?!0(?:ms|s)?[;}])/);
});
