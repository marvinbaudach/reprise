/**
 * Counts the lines the showroom claims, out of the tree the build is standing
 * in — so the page can stop repeating a number somebody typed once.
 *
 * **What a "line" is here:** a line with something other than whitespace on it.
 * Comments count. That is a coarser rule than the `cloc` run the old figures
 * came from, and it is chosen because it can be stated in one sentence and
 * checked by eye. The page says which rule it used.
 *
 * **Product against test.** A Rust file under a `tests/` directory is a test
 * whole. Inside every other file, the lines of a `#[cfg(test)]` item are test
 * lines and the rest are product. Finding those items means knowing where a
 * brace really is, so this module walks the source character by character and
 * steps over line comments, nested block comments, strings, raw strings, byte
 * strings and character literals — and does not mistake the `'` of a lifetime
 * for the start of one. A brace inside an error message would otherwise move
 * thousands of lines from one column to the other, silently and in the
 * flattering direction.
 *
 * Kotlin has no such attribute: there, Gradle's own layout decides, and the
 * tests are the files under `src/test/` and `src/androidTest/`.
 *
 * **One rule, everywhere, always.** This module is the only place the project
 * counts its own lines. The showroom reads it at build time; anything outside
 * this repository that quotes a line count — the CV in `Projects/bewerbung`
 * above all — takes its number from the same run rather than counting again.
 * Two counts of the same tree that disagree are worse than no count at all,
 * because both look authoritative and only one can be right. Run it by hand
 * with `node showroom/derive/code-census.mjs` and quote what it prints,
 * together with the commit it printed for.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, sep } from 'node:path';

/** Directories that never hold source we count. */
const SKIP_DIRS = new Set(['target', 'node_modules', '.git', 'build', '.gradle', 'dist']);

const IDENT = /[A-Za-z0-9_]/;

/**
 * The `#[cfg(test)]` shapes that mark an item as test-only. `all(test, …)` is
 * the other spelling this workspace uses; anything else is left as product,
 * which is the direction that under-claims rather than over-claims.
 */
const CFG_TEST = /^#\[cfg\((?:test\)|all\(\s*test\b)/;

/** `#[cfg(test)] mod foo;` — the test lives in another file entirely. */
const TEST_MOD_DECL = /\bmod\s+([A-Za-z0-9_]+)\s*;/;

/**
 * `#[path = "playlists_tests.rs"]` — and this is the shape that actually
 * dominates here. The architecture lint holds files under 800 lines, so a
 * suite that outgrows its module is moved into a sibling file and pulled back
 * in as `#[cfg(test)] #[path = "…_tests.rs"] mod tests;`. Reading only the
 * module *name* out of that finds a file called `tests.rs` that does not
 * exist, and files the whole suite under product: 434 of these attributes in
 * this workspace, 71'263 lines behind them.
 */
const PATH_ATTR = /#\[path\s*=\s*"([^"]+)"\]/;

/**
 * Scans a Rust source once and reports both things the count needs: the line
 * ranges of `#[cfg(test)]` items with a body, and the names of the test modules
 * declared without one. The second kind is not a detail — `#[cfg(test)] mod
 * queue_tests;` is one line here and several thousand in the file it names, and
 * a count that stops at the declaration files the whole of that file under
 * product.
 *
 * @param {string} source
 * @returns {{ ranges: Array<{ start: number, end: number }>, testMods: string[], testPaths: string[] }}
 */
export function scanRust(source) {
  /** @type {Array<{ start: number, end: number }>} */
  const ranges = [];
  /** @type {string[]} */
  const testMods = [];
  /** @type {string[]} */
  const testPaths = [];
  let line = 1;
  let depth = 0;
  /** @type {{ startLine: number, startOffset: number, baseDepth: number, opened: boolean } | null} */
  let pending = null;

  /**
   * `#[path]` may sit either side of `#[cfg(test)]`. Looking forward is the
   * common case; the lookbehind stops at the end of the previous item so a
   * neighbour's `#[path]` can never be read as this one's.
   */
  const explicitPath = (startOffset, endOffset) => {
    const forward = PATH_ATTR.exec(source.slice(startOffset, endOffset));
    if (forward !== null) return forward[1];
    let from = startOffset;
    while (from > 0 && !';}{'.includes(source[from - 1])) from -= 1;
    const behind = PATH_ATTR.exec(source.slice(from, startOffset));
    return behind === null ? null : behind[1];
  };

  const closePending = (endLine, endOffset) => {
    if (pending === null) return;
    if (!pending.opened) {
      const path = explicitPath(pending.startOffset, endOffset);
      if (path !== null) {
        testPaths.push(path);
      } else {
        const declared = TEST_MOD_DECL.exec(source.slice(pending.startOffset, endOffset));
        if (declared !== null) testMods.push(declared[1]);
      }
    }
    ranges.push({ start: pending.startLine, end: endLine });
    pending = null;
  };

  for (let i = 0; i < source.length; i += 1) {
    const c = source[i];

    if (c === '\n') {
      line += 1;
      continue;
    }

    // --- line comment -------------------------------------------------------
    if (c === '/' && source[i + 1] === '/') {
      while (i < source.length && source[i] !== '\n') i += 1;
      i -= 1;
      continue;
    }

    // --- block comment, nesting ---------------------------------------------
    if (c === '/' && source[i + 1] === '*') {
      let nesting = 1;
      i += 2;
      while (i < source.length && nesting > 0) {
        if (source[i] === '\n') line += 1;
        else if (source[i] === '/' && source[i + 1] === '*') {
          nesting += 1;
          i += 1;
        } else if (source[i] === '*' && source[i + 1] === '/') {
          nesting -= 1;
          i += 1;
        }
        i += 1;
      }
      i -= 1;
      continue;
    }

    // --- raw string: r"…", r#"…"#, br##"…"## --------------------------------
    if (c === 'r' || (c === 'b' && source[i + 1] === 'r')) {
      const prev = i > 0 ? source[i - 1] : '';
      if (!IDENT.test(prev)) {
        let j = c === 'b' ? i + 2 : i + 1;
        let hashes = 0;
        while (source[j] === '#') {
          hashes += 1;
          j += 1;
        }
        if (source[j] === '"') {
          const terminator = `"${'#'.repeat(hashes)}`;
          let k = j + 1;
          while (k < source.length) {
            if (source[k] === '\n') line += 1;
            else if (source.startsWith(terminator, k)) {
              k += terminator.length;
              break;
            }
            k += 1;
          }
          i = k - 1;
          continue;
        }
      }
    }

    // --- string, byte string ------------------------------------------------
    if (c === '"' || (c === 'b' && source[i + 1] === '"')) {
      let k = c === 'b' ? i + 2 : i + 1;
      while (k < source.length) {
        if (source[k] === '\\') {
          if (source[k + 1] === '\n') line += 1;
          k += 2;
          continue;
        }
        if (source[k] === '\n') line += 1;
        else if (source[k] === '"') {
          k += 1;
          break;
        }
        k += 1;
      }
      i = k - 1;
      continue;
    }

    // --- character literal, but not a lifetime ------------------------------
    if (c === "'") {
      if (source[i + 1] === '\\') {
        let k = i + 2;
        while (k < source.length && source[k] !== "'") k += 1;
        i = k;
        continue;
      }
      // `'a'` is a character; `'a` followed by anything else is a lifetime, and
      // `'_` and `'static` are lifetimes too.
      if (source[i + 2] === "'") {
        i += 2;
        continue;
      }
      continue;
    }

    // --- the attribute we are hunting ---------------------------------------
    if (c === '#' && pending === null && CFG_TEST.test(source.slice(i, i + 24))) {
      pending = { startLine: line, startOffset: i, baseDepth: depth, opened: false };
      continue;
    }

    // --- braces -------------------------------------------------------------
    if (c === '{') {
      if (pending !== null && !pending.opened && depth === pending.baseDepth) {
        pending.opened = true;
      }
      depth += 1;
      continue;
    }
    if (c === '}') {
      depth -= 1;
      if (pending !== null && pending.opened && depth === pending.baseDepth) {
        closePending(line, i);
      }
      continue;
    }

    // `#[cfg(test)] use …;` and friends: an item with no body of its own.
    if (c === ';' && pending !== null && !pending.opened && depth === pending.baseDepth) {
      closePending(line, i + 1);
    }
  }

  return { ranges, testMods, testPaths };
}

/**
 * The line ranges alone — the shape the tests assert against.
 *
 * @param {string} source
 * @returns {Array<{ start: number, end: number }>}
 */
export function rustTestRanges(source) {
  return scanRust(source).ranges;
}

const MOD_DECL = /^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+([A-Za-z0-9_]+)[ \t]*;/gm;

/**
 * Every `mod <name>;` in a source, each with the `#[path]` that overrides its
 * file — read together, because apart they disagree and the disagreement is
 * silent.
 *
 * @param {string} source
 * @returns {Array<{ name: string, explicit: string | null }>}
 */
export function declaredModules(source) {
  /** @type {Array<{ name: string, explicit: string | null }>} */
  const found = [];
  MOD_DECL.lastIndex = 0;
  for (let match = MOD_DECL.exec(source); match !== null; match = MOD_DECL.exec(source)) {
    // Walk back over the attribute run that belongs to this declaration:
    // attribute lines, doc comments and blank lines, and nothing beyond them.
    let from = match.index;
    while (from > 0) {
      const lineEnd = source.lastIndexOf('\n', from - 1);
      if (lineEnd < 0) break;
      const lineStart = source.lastIndexOf('\n', lineEnd - 1) + 1;
      const previous = source.slice(lineStart, lineEnd).trim();
      if (previous === '' || previous.startsWith('#[') || previous.startsWith('//')) {
        from = lineStart;
        if (lineStart === 0) break;
      } else {
        break;
      }
    }
    const attributes = source.slice(from, match.index);
    const explicit = PATH_ATTR.exec(attributes);
    found.push({ name: match[1], explicit: explicit === null ? null : explicit[1] });
  }
  return found;
}

/**
 * Non-blank lines of a Rust source, split by the `#[cfg(test)]` ranges.
 *
 * @param {string} source
 * @returns {{ product: number, test: number }}
 */
export function countRust(source) {
  const lines = source.split('\n');
  const isTest = new Uint8Array(lines.length + 2);
  for (const { start, end } of rustTestRanges(source)) {
    for (let n = start; n <= end && n < isTest.length; n += 1) isTest[n] = 1;
  }

  let product = 0;
  let test = 0;
  for (let n = 0; n < lines.length; n += 1) {
    if (lines[n].trim() === '') continue;
    if (isTest[n + 1] === 1) test += 1;
    else product += 1;
  }
  return { product, test };
}

/**
 * Non-blank lines of any source, counted whole.
 *
 * @param {string} source
 * @returns {number}
 */
export function countLines(source) {
  let n = 0;
  for (const line of source.split('\n')) {
    if (line.trim() !== '') n += 1;
  }
  return n;
}

/**
 * @param {string} root
 * @param {(path: string) => boolean} keep
 * @returns {string[]}
 */
function walk(root, keep) {
  /** @type {string[]} */
  const found = [];
  /** @type {string[]} */
  const stack = [root];
  while (stack.length > 0) {
    const dir = /** @type {string} */ (stack.pop());
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name)) stack.push(path);
      } else if (entry.isFile() && keep(path)) {
        found.push(path);
      }
    }
  }
  return found;
}

const isUnder = (path, segment) => path.split(sep).includes(segment);

/**
 * @typedef {object} Census
 * @property {{ product: number, test: number }} rust every crate but the Android bridge
 * @property {{ product: number, test: number }} bridge crates/reprise-android-ffi
 * @property {{ product: number, test: number }} kotlin the Android frontend
 * @property {number} total every non-blank line the three add up to
 * @property {number} test every non-blank test line of the three
 * @property {number} files how many files were read
 */

/**
 * Walks the repository and counts it.
 *
 * @param {string} repoRoot
 * @returns {Census}
 */
export function census(repoRoot) {
  const rust = { product: 0, test: 0 };
  const bridge = { product: 0, test: 0 };
  const kotlin = { product: 0, test: 0 };
  let files = 0;

  const bridgeRoot = join(repoRoot, 'crates', 'reprise-android-ffi');

  // Read once, then decide. A file named by `#[cfg(test)] mod <name>;` somewhere
  // else is a test file whole, and there is no way to know that from the file
  // itself — `queue_tests.rs` looks like any other module from the inside.
  const sources = new Map();
  for (const path of walk(join(repoRoot, 'crates'), (p) => p.endsWith('.rs'))) {
    sources.set(path, readFileSync(path, 'utf8'));
  }

  /**
   * `mod foo;` resolves either beside the declaring file or inside the
   * directory named after it — both spellings are legal Rust.
   *
   * @param {string} declaring @param {string} name @returns {string[]}
   */
  const candidatesFor = (declaring, name) => {
    const dir = declaring.slice(0, declaring.lastIndexOf(sep));
    const stem = declaring.slice(dir.length + 1, -3);
    return [
      join(dir, `${name}.rs`),
      join(dir, name, 'mod.rs'),
      join(dir, stem, `${name}.rs`),
      join(dir, stem, name, 'mod.rs'),
    ];
  };

  /** @type {Set<string>} */
  const wholeTestFiles = new Set();
  /** @type {string[]} */
  const worklist = [];

  for (const [path, source] of sources) {
    const dir = path.slice(0, path.lastIndexOf(sep));
    const { testMods, testPaths } = scanRust(source);
    for (const relative of testPaths) {
      const candidate = join(dir, relative);
      if (sources.has(candidate) && !wholeTestFiles.has(candidate)) {
        wholeTestFiles.add(candidate);
        worklist.push(candidate);
      }
    }
    for (const name of testMods) {
      for (const candidate of candidatesFor(path, name)) {
        if (sources.has(candidate) && !wholeTestFiles.has(candidate)) {
          wholeTestFiles.add(candidate);
          worklist.push(candidate);
        }
      }
    }
  }

  // A suite that outgrew 800 lines twice splits again, and the second split is
  // declared *inside* the first — where `#[cfg(test)]` is redundant and so is
  // not written. Every module a test file pulls in is therefore test as well,
  // however it is spelled; the worklist follows that to its end.
  for (const path of sources.keys()) {
    if (isUnder(path, 'tests') && !wholeTestFiles.has(path)) {
      wholeTestFiles.add(path);
      worklist.push(path);
    }
  }
  while (worklist.length > 0) {
    const path = /** @type {string} */ (worklist.pop());
    const source = /** @type {string} */ (sources.get(path));
    const dir = path.slice(0, path.lastIndexOf(sep));
    /** @type {string[]} */
    const reached = [];
    for (const { name, explicit } of declaredModules(source)) {
      // A `#[path]` on the declaration wins over the module's own name, and
      // reading the two apart is how a *production* module gets swept into the
      // test column: `#[path = "runtime_effects_tests.rs"] mod effects;` names
      // the test file, while `effects` alone names the equalizer.
      if (explicit !== null) reached.push(join(dir, explicit));
      else reached.push(...candidatesFor(path, name));
    }
    for (const candidate of reached) {
      if (sources.has(candidate) && !wholeTestFiles.has(candidate)) {
        wholeTestFiles.add(candidate);
        worklist.push(candidate);
      }
    }
  }

  for (const [path, source] of sources) {
    const into = path.startsWith(bridgeRoot + sep) ? bridge : rust;
    if (isUnder(path, 'tests') || wholeTestFiles.has(path)) {
      into.test += countLines(source);
    } else {
      const { product, test } = countRust(source);
      into.product += product;
      into.test += test;
    }
    files += 1;
  }

  for (const path of walk(
    join(repoRoot, 'android'),
    (p) => p.endsWith('.kt') || p.endsWith('.kts'),
  )) {
    const source = readFileSync(path, 'utf8');
    const lines = countLines(source);
    if (isUnder(path, 'test') || isUnder(path, 'androidTest')) kotlin.test += lines;
    else kotlin.product += lines;
    files += 1;
  }

  // An empty count is the one failure that would look like success: three zeroes
  // that agree with each other and with nothing else.
  if (files === 0) {
    throw new Error(`counted no source files under ${repoRoot} — the tree moved`);
  }

  const total =
    rust.product + rust.test + bridge.product + bridge.test + kotlin.product + kotlin.test;
  const test = rust.test + bridge.test + kotlin.test;
  return { rust, bridge, kotlin, total, test, files };
}

/** @param {string} path @returns {boolean} */
export function isRepoRoot(path) {
  try {
    return statSync(join(path, 'crates')).isDirectory();
  } catch {
    return false;
  }
}

/**
 * Printed by hand, so a number quoted anywhere else comes from this same run.
 *
 * `node showroom/derive/code-census.mjs` — the output names the commit it was
 * taken on, because a line count without one is a claim about no particular
 * tree.
 */
if (import.meta.main) {
  const { execFileSync } = await import('node:child_process');
  const root = fileURLToPath(new URL('../..', import.meta.url));
  const counted = census(root);
  const share = (n) => `${((n / counted.total) * 100).toFixed(1)} %`;
  const group = (n) => n.toLocaleString('en-GB').replaceAll(',', "'");
  let commit = 'unknown';
  try {
    commit = execFileSync('git', ['-C', root, 'rev-parse', '--short=10', 'HEAD'], {
      encoding: 'utf8',
    }).trim();
  } catch {
    // A tarball without a .git is still countable; it just cannot say where from.
  }
  const rows = [
    ['Rust, product', counted.rust.product],
    ['Rust, tests', counted.rust.test],
    ['Rust, Android bridge', counted.bridge.product + counted.bridge.test],
    ['Kotlin', counted.kotlin.product + counted.kotlin.test],
  ];
  process.stdout.write(`reprise line census — ${commit}, ${counted.files} files\n`);
  process.stdout.write('non-blank lines; a #[cfg(test)] item and every file it pulls in is test\n\n');
  for (const [label, lines] of rows) {
    process.stdout.write(`  ${label.padEnd(22)} ${group(lines).padStart(9)}   ${share(lines)}\n`);
  }
  process.stdout.write(`  ${'total'.padEnd(22)} ${group(counted.total).padStart(9)}\n`);
  process.stdout.write(`  ${'of them tests'.padEnd(22)} ${group(counted.test).padStart(9)}   ${share(counted.test)}\n`);
}
