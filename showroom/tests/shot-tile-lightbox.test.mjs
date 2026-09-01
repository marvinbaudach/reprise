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

function assertShownCaptureDrivesRatio(source) {
  assert.match(source, /const capture = captures\[shownIndex\];/);
  assert.match(source, /'--lb-ratio': capture\.width \/ capture\.height/);
  assert.equal(
    (source.match(/\bconst capture\s*=/g) ?? []).length,
    1,
    'the frame ratio and bitmap must share the sole capture binding',
  );
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

  // The picture wrap keeps the hero phone's visualizer canvas — a sibling of
  // the image — on the same geometry as the screenshot.
  assert.match(source, /className="shot-tile__picture"[\s\S]*?<ProductShot[\s\S]*?\{children\}/);
  assert.doesNotMatch(source, /requestAnimationFrame/);
  assert.match(css, /@keyframes rp-sweep/);
  // Lightning CSS rewrites the `animation` shorthand into its canonical order, so
  // the parts are asserted, not the sequence they were authored in.
  const sweep = css.match(/animation:[^;}]*rp-sweep[^;}]*/)?.[0];
  assert.ok(sweep, 'the sweep must be driven by an animation shorthand');
  for (const part of ['1.5s', 'linear', 'infinite']) {
    assert.ok(sweep.includes(part), `the sweep animation must carry ${part}`);
  }
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
  // Same rewrite as the sweep above, plus one drop: `ease` is the initial timing
  // function, so Lightning CSS leaves it out of the shorthand. The eased curve is
  // still what runs — asserting the literal word would assert the minifier.
  const fade = css.match(/animation:[^;}]*rp-fade[^;}]*/)?.[0];
  assert.ok(fade, 'the backdrop must fade in through an animation shorthand');
  for (const part of ['.17s', 'both']) {
    assert.ok(fade.includes(part), `the fade animation must carry ${part}`);
  }
  // The ground closes first and the dialog's own text follows it, so the page
  // underneath is never legible next to a half-drawn dialog.
  const zoomIn = css.match(/\.lightbox__zoom\{[^}]*\}/)?.[0];
  assert.ok(zoomIn, '.lightbox__zoom must exist in the built CSS');
  for (const part of ['.32s', 'cubic-bezier(.16,1,.3,1)', '.11s', 'both', 'rp-lb-in']) {
    assert.ok(zoomIn.includes(part), `the opening animation must carry ${part}`);
  }
  assert.match(css, /backdrop-filter:blur\(14px\)/);
  // A phone drops the blur and closes the ground instead: the blur is a
  // compositing pass per frame there, and translucency without it leaves the
  // page's headlines readable straight through the dialog.
  // Lightning CSS rewrites `max-width: 720px` to the range form.
  assert.match(
    css,
    /@media \(width<=720px\)\{\.lightbox\{[^}]*backdrop-filter:none[^}]*background:oklch\(7% \.012 269\)\}/,
  );
  const frame = css.match(/\.lightbox__frame\{[^}]*\}/)?.[0];
  assert.ok(frame, '.lightbox__frame must exist in the built CSS');
  // The frame fits itself on whichever axis binds first. `height:100%` with
  // `width:auto` only fitted while the height was the binding side: on a phone
  // the width clamped, the height stayed at 100%, and the picture sat as a band
  // inside a tall empty bordered box.
  for (const declaration of ['width:min(100cqw, 100cqh * var(--lb-ratio', 'height:auto']) {
    assert.ok(frame.includes(declaration), `.lightbox__frame must carry ${declaration}`);
  }
  assert.match(css, /\.lightbox__viewport\{[^}]*container-type:size/);
  // The entry animation may not live on the frame: with `fill: both` it keeps
  // writing its final `transform:none` after it ends, which outranks the inline
  // transform the zoom sets and leaves the zoom dead.
  assert.ok(!frame.includes('animation'), '.lightbox__frame must not carry an animation');
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

  // The zoom belongs to one picture, not to a bare boolean. Its display follows
  // the requested index so the outgoing shot settles before the incoming one
  // replaces it.
  assert.match(source, /zoom\.index === activeIndex/);
  assert.match(source, /setZoom\(\{\s*index: shownIndex/);
  // ...and the origin outlives the zoom itself: dropping the state outright
  // would snap the origin back to the centre while the picture is still
  // travelling, swinging it across the viewport instead of letting it settle.
  assert.match(source, /transformOrigin: frameZoom\?\.origin/);
  assert.match(source, /setZoom\(\{ \.\.\.activeZoom, zoomed: false \}\)/);

  // Neither the closing backdrop nor any other tabindex="-1" node is a tab stop.
  assert.match(source, /className="lightbox__backdrop"[\s\S]*?tabIndex=\{-1\}/);
  assert.match(source, /button:not\(\[disabled\]\):not\(\[tabindex="-1"\]\)/);
});

test('the full view holds its frame until the next picture has decoded', async () => {
  const source = await readFile(
    join(showroomRoot, 'src', 'components', 'showcase', 'Lightbox.tsx'),
    'utf8',
  );

  // The frame's ratio comes from the capture it draws, and the image inside it
  // is `object-fit: contain`. Advancing the ratio on the press re-letterboxes
  // the picture that is still on screen — the outgoing shot visibly rescales
  // for as long as the next file takes to arrive, which on a cold phone is
  // seconds. So the frame draws a picture that lags the request.
  assert.match(source, /const \[shownIndex, setShownIndex\] = useState\(activeIndex\)/);
  assertShownCaptureDrivesRatio(source);

  const preloadEffect = source.match(
    /useEffect\(\(\) => \{\s*if \(activeIndex === shownIndex\)[\s\S]*?\}, \[activeIndex, shownIndex, captures\]\);/,
  )?.[0];
  assert.ok(
    preloadEffect,
    'the preload effect must stay scoped to the requested and shown captures',
  );

  // The gate waits for pixels, not bytes, and survives a file it cannot decode.
  assert.match(preloadEffect, /new Image\(\)/);
  assert.match(preloadEffect, /preload\.decode\(\)\.then\(settle, settle\)/);
  assert.match(preloadEffect, /preload\.onerror = settle/);

  // A transfer that never settles must eventually release the held frame, and
  // both a normal settlement and effect cleanup must disarm that recovery.
  assert.match(source, /const IMAGE_PRELOAD_TIMEOUT_MS = 10_000/);
  assert.match(preloadEffect, /let timedOut = false/);
  const timeoutCommit = preloadEffect.match(/const commit = \(\) => \{([\s\S]*?)^\s*\};/m)?.[1];
  assert.ok(
    timeoutCommit,
    'the preload timeout must identify its forced commit before releasing the frame',
  );
  assert.match(timeoutCommit, /timedOut = true;/);
  assert.match(timeoutCommit, /settle\(\);/);
  assert.match(
    preloadEffect,
    /const timeout = window\.setTimeout\(commit, IMAGE_PRELOAD_TIMEOUT_MS\)/,
  );
  const settleBody = preloadEffect.match(/const settle = \(\) => \{([\s\S]*?)^\s*\};/m)?.[1];
  assert.ok(settleBody, 'the preload effect must define a settle function');
  assert.match(settleBody, /window\.clearTimeout\(timeout\);/);

  // A second press must supersede the first preload rather than race it: a
  // stale resolution landing late would send the reader back a picture, while
  // leaving its fetch alive would compete with the picture now being requested.
  assert.match(preloadEffect, /let superseded = false/);
  assert.match(preloadEffect, /if \(!superseded\) setShownIndex\(activeIndex\)/);
  const preloadCleanup = preloadEffect.match(/return \(\) => \{([\s\S]*?)^\s*\};/m)?.[1];
  assert.ok(preloadCleanup, 'the preload effect must return a cleanup function');
  for (const statement of [
    /superseded = true;/,
    /window\.clearTimeout\(timeout\);/,
    /preload\.src = '';/,
    /preload\.srcset = '';/,
  ]) {
    assert.match(preloadCleanup, statement);
  }
  assert.match(preloadCleanup, /if \(!timedOut\) \{/);

  // The press still needs a visual and assistive answer while the frame holds.
  assert.match(source, /data-swapping=\{swapping \? 'true' : 'false'\}/);
  assert.match(source, /aria-busy=\{swapping \? 'true' : undefined\}/);
  const counterTag = source.match(/<span\b[^>]*\bclassName="lightbox__counter"[^>]*>/)?.[0];
  assert.ok(counterTag, 'the lightbox counter must remain a span');
  assert.match(counterTag, /aria-live="polite"/);
  assert.match(counterTag, /aria-atomic="true"/);
  const counterStart = source.indexOf(counterTag);
  const counterRegion = source.slice(counterStart, source.indexOf('<button', counterStart));
  assert.match(counterRegion, /className="visually-hidden"/);
  assert.match(counterRegion, /\{capture\.title\}/);
  assert.match(counterRegion, /\{counter\}/);
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
  assertShownCaptureDrivesRatio(source);
  // Exactly one capture claims the plate today: the Android Now Playing scene.
  assert.equal((data.match(/visualizer: true/g) ?? []).length, 1);
  assert.match(data, /id: 'android-visualizer'[\s\S]{0,420}?visualizer: true/);
});
