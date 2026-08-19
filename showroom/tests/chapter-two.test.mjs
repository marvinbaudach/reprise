import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

import { readout, toggle } from '../src/lib/mergeGates.ts';

const showroomRoot = new URL('..', import.meta.url).pathname;
const repoRoot = new URL('../..', import.meta.url).pathname;

const GATE_SCRIPT = join(repoRoot, 'scripts', 'check-merge-readiness.sh');
const PIPELINE_DOC = join(repoRoot, 'docs', 'agents', 'pipeline.md');

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

const prerenderedPage = () => readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');

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

async function pipelineSteps() {
  const doc = await readFile(PIPELINE_DOC, 'utf8');
  const steps = [...doc.matchAll(/^\|\s*(\d{2})\s*\|(.+?)\|(.+?)\|(.+?)\|(.+?)\|\s*$/gm)].map(
    ([, step, phase, actor, writes, judges]) => ({
      step,
      phase: phase.trim(),
      actor: actor.trim(),
      writes: writes.trim() === 'yes',
      judges: judges.trim() === 'yes',
    }),
  );
  assert.ok(steps.length > 0, 'the pipeline document must carry its table');
  return steps;
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

test('show-6 the gate wall names the checks the script runs, in script order', async () => {
  const names = await gateNames();
  const html = await prerenderedPage();

  const wall = html.match(/<figure[^>]+data-showcase="gate-wall"[\s\S]+?<\/figure>/)?.[0];
  assert.ok(wall, 'the gate wall must be prerendered');

  const shown = [...wall.matchAll(/data-gate="([^"]+)"/g)].map((match) => match[1]);
  // Order, not just membership: the wall claims to show the run order.
  assert.deepEqual(shown, names);

  assert.match(wall, new RegExp(`data-gates="${names.length}"`));
  assert.match(wall, new RegExp(`${names.length} checks green`));
});

test('show-7 no lane both writes and judges, and the human lane holds one mark', async () => {
  const steps = await pipelineSteps();
  const html = await prerenderedPage();

  const byActor = new Map();
  for (const step of steps) {
    const seen = byActor.get(step.actor) ?? { writes: false, judges: false, steps: 0 };
    byActor.set(step.actor, {
      writes: seen.writes || step.writes,
      judges: seen.judges || step.judges,
      steps: seen.steps + 1,
    });
  }

  for (const [actor, lane] of byActor) {
    assert.ok(
      !(lane.writes && lane.judges),
      `${actor} both writes and judges — the invariant the figure claims is broken`,
    );
  }
  assert.equal(byActor.get('Human')?.steps, 1, 'the human lane must carry exactly one mark');

  const swimlane = html.match(/<figure[^>]+data-showcase="agent-swimlane"[\s\S]+?<\/figure>/)?.[0];
  assert.ok(swimlane, 'the swimlane must be prerendered');
  for (const actor of byActor.keys()) {
    assert.match(swimlane, new RegExp(`<th[^>]*scope="row"[^>]*>${actor}</th>`));
  }
  for (const step of steps) {
    assert.match(
      swimlane,
      new RegExp(`>${step.phase}<`),
      `step ${step.step} is missing its column`,
    );
  }
  // One mark per step, no more: a mark the table does not license is a claim
  // nobody can check.
  assert.equal((swimlane.match(/data-mark=""/g) ?? []).length, steps.length);
});

test('show-8 a failed check blocks the readout and clearing it releases again', () => {
  const total = 26;

  const ready = readout(new Set(), total);
  assert.equal(ready.blocked, false);
  assert.match(ready.message, /^Ready to merge · 26 checks green$/);

  const one = readout(new Set(['Rust lint']), total);
  assert.equal(one.blocked, true);
  assert.equal(one.failed, 1);
  assert.match(one.message, /^Merge blocked · 1 of 26 failing$/);

  const three = readout(new Set(['Rust lint', 'Shell', 'AppStream']), total);
  assert.equal(three.failed, 3);
  assert.match(three.message, /^Merge blocked · 3 of 26 failing$/);

  // The toggle is what the cell does, and it must not mutate what it was given.
  const before = new Set(['Shell']);
  const after = toggle(before, 'Shell');
  assert.deepEqual([...before], ['Shell']);
  assert.equal(after.size, 0);
  assert.equal(readout(after, total).blocked, false);
  assert.equal(toggle(after, 'Shell').size, 1);
});

test('show-9 reduced motion places marks and gate cells in their end state', async () => {
  const css = await builtCss();

  // Several unrelated reduced-motion queries sit in this stylesheet. Take the
  // one that governs these two figures; a lazy walk from the first one would
  // call an escaped rule guarded.
  // Vite 8 minifies with Lightning CSS, which prints a space after `@media`
  // where esbuild printed none. The space is the minifier's, so it is optional here.
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

  const mine = guards.filter((guard) => guard.includes('.swimlane__mark'));
  assert.equal(mine.length, 1, 'exactly one reduced-motion query may govern the two figures');
  const guarded = mine[0];
  // The minifier writes `:before` for `::before`, so accept either spelling.
  assert.match(guarded, /\.swimlane__mark::?before/);
  assert.match(guarded, /\.gate-wall__cell::?after/);
  assert.match(guarded, /transform:scaleX\(1\)/);
  assert.match(guarded, /transition:none/);
  // With `transition:none` there is no delay left to state, and Lightning CSS
  // folds the explicit `transition-delay:0s` away for exactly that reason. What
  // must never survive inside this query is a delay that actually waits.
  assert.doesNotMatch(guarded, /transition-delay:(?!0(?:ms|s)?[;}])/);
});

test('show-10 the gate count is nowhere a literal', async () => {
  const names = await gateNames();
  const count = String(names.length);

  // A bare `26` may legitimately be an icon's width. What the rule forbids is
  // the count typed where the page states it, so the check is the literal in the
  // company of the words it would be claiming.
  const literal = new RegExp(`(?<![\\d.'’])${count}(?![\\d.'’%])`, 'g');
  for (const file of await displaySources()) {
    const source = await readFile(file, 'utf8');
    for (const match of source.matchAll(literal)) {
      const around = source.slice(Math.max(0, match.index - 90), match.index + 90);
      assert.ok(
        !/gate|merge|check/i.test(around),
        `${file} types ${count} next to the words it claims — it must read the derivation`,
      );
    }
  }

  // And the three places that show it read the same module.
  for (const file of [
    join(showroomRoot, 'src', 'data', 'measurements.ts'),
    join(showroomRoot, 'src', 'components', 'chapters', 'TempoBand.tsx'),
    join(showroomRoot, 'src', 'components', 'process', 'GateWall.tsx'),
  ]) {
    assert.match(await readFile(file, 'utf8'), /from 'virtual:merge-gates'/);
  }
});
