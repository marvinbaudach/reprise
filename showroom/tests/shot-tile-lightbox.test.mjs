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

test('ShotTile owns the design tilt sheen loading sweep and expandable caption', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'ShotTile.tsx'),
    'utf8',
  );

  assert.equal((html.match(/<button[^>]+class="[^"]*shot-tile[^"]*"/g) ?? []).length, 11);
  for (const attribute of ['data-shot', 'data-sheen', 'data-sweep', 'data-dwrap']) {
    assert.match(html, new RegExp(`${attribute}=""`));
  }

  assert.match(source, /const TILT_DEGREES = 8/);
  assert.match(source, /setProperty\('--mx'/);
  assert.match(source, /setProperty\('--my'/);
  assert.match(source, /perspective\(1200px\)/);
  assert.doesNotMatch(source, /requestAnimationFrame/);
  assert.match(css, /@keyframes rp-sweep/);
  assert.match(css, /animation:rp-sweep 1\.5s linear infinite/);
  assert.match(css, /grid-template-rows:0fr/);
  assert.match(css, /grid-template-rows:1fr/);
  assert.match(css, /radial-gradient\(340px circle at var\(--mx\) var\(--my\)/);
  assert.match(
    css,
    /prefers-reduced-motion:reduce[^}]*\}[\s\S]*?\.shot-tile__sheen[^}]*display:none/,
  );
});

test('Lightbox traps focus and restores its trigger around keyboard navigation and click zoom', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'Lightbox.tsx'),
    'utf8',
  );
  const css = await builtCss();

  assert.match(source, /role="dialog"/);
  assert.match(source, /aria-modal="true"/);
  assert.match(source, /event\.key === 'Escape'/);
  assert.match(source, /event\.key === 'ArrowRight'/);
  assert.match(source, /event\.key === 'ArrowLeft'/);
  assert.match(source, /event\.key !== 'Tab'/);
  assert.match(source, /document\.documentElement\.style\.overflow = 'hidden'/);
  assert.match(source, /returnFocus\.focus\(\)/);
  assert.match(source, /scale\(2\.1\)/);
  assert.match(source, /transformOrigin/);
  assert.match(css, /animation:rp-fade \.26s ease both/);
  assert.match(css, /animation:rp-lb-in \.42s cubic-bezier\(\.16,1,\.3,1\) both/);
  assert.match(css, /backdrop-filter:blur\(14px\)/);
});
