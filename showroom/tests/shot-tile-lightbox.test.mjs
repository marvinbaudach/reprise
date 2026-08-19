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

test('ShotTile owns the still frame the loading sweep and the zoom cue', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'ShotTile.tsx'),
    'utf8',
  );

  assert.equal((html.match(/<button[^>]+class="[^"]*shot-tile[^"]*"/g) ?? []).length, 11);
  for (const attribute of ['data-shot', 'data-sweep', 'data-zoom']) {
    assert.match(html, new RegExp(`${attribute}=""`));
  }

  // The picture wrap carries the zoom so the hero phone's visualizer canvas —
  // a sibling of the image — travels with the screenshot instead of standing
  // still on top of a growing one.
  assert.match(source, /className="shot-tile__picture"[\s\S]*?<ProductShot[\s\S]*?\{children\}/);
  assert.doesNotMatch(source, /requestAnimationFrame/);
  assert.match(css, /@keyframes rp-sweep/);
  assert.match(css, /animation:rp-sweep 1\.5s linear infinite/);
  assert.match(
    css,
    /prefers-reduced-motion:reduce[^}]*\}[\s\S]*?\.shot-tile__sweep[^}]*display:none/,
  );
  assert.match(
    css,
    /prefers-reduced-motion:reduce[^{]*\{[^}]*\.shot-tile__zoom[^}]*transition:none/,
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
  // The zoom wrapper must stay a resolved box and the frame must keep the
  // picture's ratio, or the picture is cropped and the plate sits off it.
  assert.match(css, /\.lightbox__zoom\{[^}]*height:100%/);
  assert.match(css, /\.lightbox__frame\{[^}]*height:100%[^}]*width:auto/);
  assert.match(css, /\.lightbox__image\{[^}]*width:100%[^}]*height:100%/);
});

test('the Lightbox inerts the page behind it and forgets its zoom with the picture', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'Lightbox.tsx'),
    'utf8',
  );

  // The dialog sits in a portal on <body>, so the whole page can go inert.
  assert.match(source, /createPortal\(/);
  assert.match(source, /document\.body,/);
  assert.match(source, /getElementById\('showroom-root'\)/);
  assert.match(source, /setAttribute\('inert', ''\)/);
  assert.match(source, /setAttribute\('aria-hidden', 'true'\)/);
  assert.match(source, /removeAttribute\('inert'\)/);
  assert.match(source, /removeAttribute\('aria-hidden'\)/);
  // Focus may only return once the page is reachable again.
  const cleanup = source.match(/removeAttribute\('aria-hidden'\)[\s\S]*?returnFocus\.focus\(\)/);
  assert.ok(cleanup, 'returnFocus must be restored after the inert attributes are dropped');

  // The zoom belongs to one picture, not to a bare boolean.
  assert.match(source, /zoom\.index === activeIndex/);
  assert.doesNotMatch(source, /zoomed: (true|false)/);

  // Neither the closing backdrop nor any other tabindex="-1" node is a tab stop.
  assert.match(source, /className="lightbox__backdrop"[\s\S]*?tabIndex=\{-1\}/);
  assert.match(source, /button:not\(\[disabled\]\):not\(\[tabindex="-1"\]\)/);
});

test('the full view carries the live plate for the capture that has one', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'Lightbox.tsx'),
    'utf8',
  );
  const data = await readFile(join(showroomRoot, 'src', 'data', 'showcase.ts'), 'utf8');

  assert.match(source, /import \{ VisualizerPlate \}/);
  assert.match(source, /capture\.visualizer && <VisualizerPlate \/>/);
  // The frame, not the image, carries the zoom — otherwise the plate stays put
  // while the picture under it grows.
  assert.match(
    source,
    /className="lightbox__frame"[\s\S]*?transform: activeZoom \? 'scale\(2\.1\)'/,
  );
  assert.match(source, /aspectRatio: `\$\{capture\.width\} \/ \$\{capture\.height\}`/);
  // Exactly one capture claims the plate today: the Android Now Playing scene.
  assert.equal((data.match(/visualizer: true/g) ?? []).length, 1);
  assert.match(data, /id: 'android-visualizer'[\s\S]{0,420}?visualizer: true/);
});
