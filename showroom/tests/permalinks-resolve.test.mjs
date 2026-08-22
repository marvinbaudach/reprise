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
    for (const [, objectName, body] of source.matchAll(
      /(?:export\s+)?const\s+([A-Z][A-Z0-9_]*)\s*=\s*\{([\s\S]*?)\}\s*(?:as const)?\s*;/g,
    )) {
      assert.ok(body !== undefined);
      for (const [, propertyName, value] of body.matchAll(
        /(?:^|,)\s*([A-Za-z_$][\w$]*)\s*:\s*'([^']+)'/g,
      )) {
        constants.set(`${objectName}.${propertyName}`, value);
      }
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
      unresolved.add(`${file}: permalink(${argument})`);
    }
  }

  return { paths: [...literals].sort(), unresolved: [...unresolved] };
}

/**
 * Whether the pinned commit is reachable from the published branch.
 *
 * This exists because it was not. `a776f8a963` was rewritten out of the
 * history; the commit survived in every local object store, so `show-17` kept
 * passing on developer machines while every link on the page 404ed and a fresh
 * CI clone could not resolve the commit at all. Path existence alone cannot see
 * that — a dangling commit still carries its whole tree.
 */
test('show-22 the pinned commit is reachable from the published branch', async () => {
  const commit = await pinnedCommit();

  // `scripts/ci-quality.sh` already leans on this ref being there; a checkout
  // without `fetch-depth: 0` is the one way it is not. Erroring beats skipping:
  // a skip is the hole this test was written to close.
  const branch = 'origin/main';
  await assert.doesNotReject(
    run('git', ['rev-parse', '--verify', `${branch}^{commit}`], { cwd: repoRoot }),
    `${branch} is unavailable; checkout must use fetch-depth: 0`,
  );

  // Positive control: the same check against a commit no branch carries has to fail.
  await assert.rejects(
    run(
      'git',
      ['merge-base', '--is-ancestor', '0000000000000000000000000000000000000000', branch],
      {
        cwd: repoRoot,
      },
    ),
  );

  await assert.doesNotReject(
    run('git', ['merge-base', '--is-ancestor', commit, branch], { cwd: repoRoot }),
    `${commit} is not reachable from ${branch}, so every permalink on the page is a 404`,
  );
});

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
