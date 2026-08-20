import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';
import { promisify } from 'node:util';

const run = promisify(execFile);
const showroomRoot = new URL('..', import.meta.url).pathname;
const repoRoot = new URL('../..', import.meta.url).pathname;

/**
 * Every path the page hands a reader, and whether the pinned commit carries it.
 *
 * This exists because it did not. `BASELINE.commit` sat at a commit that
 * predated `index-rebuild.md`, `timeline.md` and `code-census.mjs`, so four
 * links the page had been shipping resolved to a GitHub 404 — on a page whose
 * whole claim is that every figure points at a file. Nothing caught it, because
 * a permalink is a string until someone clicks it.
 *
 * Answered from the local object database rather than over the network: the
 * suite has to give the same verdict offline and in CI, and a 404 is a property
 * of the tree at that commit, not of GitHub's availability.
 */

async function sourceFiles() {
  const files = [];
  const roots = [join(showroomRoot, 'src')];
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

async function pinnedCommit() {
  const source = await readFile(join(showroomRoot, 'src', 'data', 'measurements.ts'), 'utf8');
  const commit = source.match(/commit:\s*'([0-9a-f]{7,40})'/)?.[1];
  assert.ok(commit, 'measurements.ts must pin a commit');
  return commit;
}

/**
 * Literal arguments to `permalink(…)` and `treelink(…)`, plus the values of the
 * path constants those calls name. A call whose argument this cannot resolve is
 * reported rather than skipped — a silently unchecked link is the bug.
 */
async function citedPaths() {
  const constants = new Map();
  const literals = new Set();
  const unresolved = new Set();

  const files = await sourceFiles();
  for (const file of files) {
    const source = await readFile(file, 'utf8');
    for (const [, name, value] of source.matchAll(
      /(?:export\s+)?const\s+([A-Z][A-Z0-9_]*)\s*=\s*'([^']+)'/g,
    )) {
      constants.set(name, value);
    }
  }

  for (const file of files) {
    const source = await readFile(file, 'utf8');
    for (const match of source.matchAll(/(?:permalink|treelink)\(\s*([^)]*?)\s*\)/g)) {
      const argument = match[1];
      const before = source.slice(Math.max(0, match.index - 16), match.index);
      if (before.endsWith('function ')) continue;
      assert.ok(argument !== undefined);
      if (argument === '') continue;
      const literal = argument.match(/^'([^']*)'$/)?.[1];
      if (literal !== undefined) {
        if (literal !== '') literals.add(literal);
        continue;
      }
      const named = constants.get(argument);
      if (named !== undefined) {
        literals.add(named);
        continue;
      }
      // `CENSUS_SCOPE.source` and friends are derived at build time; their value
      // is asserted where it is produced, not here.
      if (/^[A-Z][A-Z0-9_]*\.[a-z]/.test(argument)) continue;
      unresolved.add(`${file}: permalink(${argument})`);
    }
  }

  return { paths: [...literals].sort(), unresolved: [...unresolved] };
}

test('show-17 every permalinked path exists at the commit the page pins', async () => {
  const commit = await pinnedCommit();
  const { paths, unresolved } = await citedPaths();

  assert.deepEqual(unresolved, [], 'a cited path could not be resolved, so it went unchecked');
  // Positive control: an empty list would satisfy the loop below in silence.
  assert.ok(paths.length >= 5, `expected the page to cite several paths, found ${paths.length}`);

  const missing = [];
  for (const path of paths) {
    try {
      await run('git', ['cat-file', '-e', `${commit}:${path}`], { cwd: repoRoot });
    } catch {
      missing.push(path);
    }
  }

  assert.deepEqual(
    missing,
    [],
    `these paths do not exist at ${commit}, so the page links to a 404`,
  );
});
