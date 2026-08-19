import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  census,
  countLines,
  countRust,
  countTestFunctions,
  declaredModules,
  rustTestRanges,
  scanRust,
} from '../derive/code-census.mjs';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

/**
 * The scanner exists because a brace is not a brace everywhere. Each of these
 * fixtures puts one where a naive counter would read it as structure, and every
 * one of them would move a whole test module into the product column — in the
 * flattering direction, and without a word of complaint.
 */

test('a brace inside a string literal does not close the module', () => {
  const source = `#[cfg(test)]
mod tests {
    fn message() -> &'static str { "a } that is not a brace" }
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 4 }]);
  assert.deepEqual(countRust(source), { product: 1, test: 4 });
});

test('a brace inside a line comment does not close the module', () => {
  const source = `#[cfg(test)]
mod tests {
    // }
    fn t() {}
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 5 }]);
});

test('a brace inside a nested block comment does not close the module', () => {
  const source = `#[cfg(test)]
mod tests {
    /* outer /* inner } still a comment */ still outer } */
    fn t() {}
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 5 }]);
  assert.deepEqual(countRust(source), { product: 1, test: 5 });
});

test('a brace inside a raw string does not close the module', () => {
  const source = `#[cfg(test)]
mod tests {
    const RULES: &str = r#"{"op":"=","value":"}"}"#;
    fn t() {}
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 5 }]);
});

test('a lifetime is not the start of a character literal', () => {
  const source = `#[cfg(test)]
mod tests {
    fn borrow<'a>(v: &'a str) -> &'a str { v }
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 4 }]);
});

test('a closing brace as a character literal does not close the module', () => {
  const source = `#[cfg(test)]
mod tests {
    const CLOSE: char = '}';
    const ESCAPED: char = '\\'';
    fn t() {}
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 6 }]);
});

test('cfg(all(test, ...)) marks the module too', () => {
  const source = `#[cfg(all(test, feature = "slow"))]
mod tests {
    fn t() {}
}
fn product() {}
`;
  assert.deepEqual(rustTestRanges(source), [{ start: 1, end: 4 }]);
});

test('a test module declared without a body names its file', () => {
  const source = `#[cfg(test)]
mod queue_tests;

fn product() {}
`;
  const { testMods, testPaths } = scanRust(source);
  assert.deepEqual(testMods, ['queue_tests']);
  assert.deepEqual(testPaths, []);
});

test('a #[path] attribute names the file instead of the module', () => {
  const source = `#[cfg(test)]
#[path = "playlists_tests.rs"]
mod tests;

fn product() {}
`;
  const { testMods, testPaths } = scanRust(source);
  assert.deepEqual(testPaths, ['playlists_tests.rs']);
  assert.deepEqual(testMods, []);
});

test('#[path] is read even when it precedes the cfg attribute', () => {
  const source = `#[path = "lib_tests.rs"]
#[cfg(test)]
mod tests;
`;
  assert.deepEqual(scanRust(source).testPaths, ['lib_tests.rs']);
});

test('a #[path] on one declaration is never read as the next one‘s', () => {
  const source = `#[cfg(test)]
#[path = "first_tests.rs"]
mod first;

#[cfg(test)]
mod second;
`;
  const { testMods, testPaths } = scanRust(source);
  assert.deepEqual(testPaths, ['first_tests.rs']);
  assert.deepEqual(testMods, ['second']);
});

test('declaredModules pairs every module with the path that overrides it', () => {
  // The real shape from crates/reprise-runtime: a test file declares `mod
  // effects;` under a #[path] that names the *test* file. Reading the name
  // alone reaches src/effects.rs — the equalizer, which is product.
  const source = `mod devices;
mod effects;

#[path = "runtime_effects_tests.rs"]
mod effects_under_test;
`;
  assert.deepEqual(declaredModules(source), [
    { name: 'devices', explicit: null },
    { name: 'effects', explicit: null },
    { name: 'effects_under_test', explicit: 'runtime_effects_tests.rs' },
  ]);
});

test('blank lines count for nothing and comments count as lines', () => {
  assert.equal(countLines('a\n\n   \nb\n'), 2);
  assert.equal(countLines('// only a comment\n'), 1);
});

test('declared test functions are counted where they are written', () => {
  const rust = `#[test]
fn plain() {}

#[tokio::test]
async fn asynchronous() {}

#[rstest]
fn parameterised() {}

// #[test] in a comment is still a declaration on its own line, and that is
// accepted: the count is a floor for how much test surface exists, and a
// comment that looks exactly like a test is rare enough to leave alone.
fn product() {}
`;
  assert.equal(countTestFunctions(rust, false), 3);
  assert.equal(countTestFunctions('@Test\nfun a() {}\n@Test\nfun b() {}\n', true), 2);
  // The Rust and Kotlin spellings never count each other.
  assert.equal(countTestFunctions('@Test\nfun a() {}\n', false), 0);
  assert.equal(countTestFunctions('#[test]\nfn a() {}\n', true), 0);
});

test('the census reads the repository it is standing in', () => {
  const counted = census(repoRoot);

  // No exact figure is asserted — the tree changes with every merge, and a test
  // that pins the count would have to be edited to stay green, which is the
  // habit this whole strand exists to break. What is asserted is that the
  // derivation ran, saw all three bodies of code, and is internally consistent.
  assert.ok(counted.files > 1000, `expected the whole tree, counted ${counted.files} files`);
  assert.ok(counted.rust.product > 0 && counted.rust.test > 0);
  assert.ok(counted.bridge.product > 0);
  assert.ok(counted.kotlin.product > 0 && counted.kotlin.test > 0);
  assert.equal(
    counted.total,
    counted.rust.product +
      counted.rust.test +
      counted.bridge.product +
      counted.bridge.test +
      counted.kotlin.product +
      counted.kotlin.test,
  );
  assert.equal(counted.test, counted.rust.test + counted.bridge.test + counted.kotlin.test);
  assert.ok(counted.test < counted.total);
  assert.ok(counted.testFunctions > 1000, `counted ${counted.testFunctions} declared tests`);
});

test('an empty tree is a failure, not a page full of zeroes', () => {
  assert.throws(() => census(fileURLToPath(new URL('.', import.meta.url))), /counted no source/);
});
