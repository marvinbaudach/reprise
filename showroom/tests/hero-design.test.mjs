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

test('the design hero opens with two screenshot buttons', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();
  const hero = html.match(/<section[^>]+data-showcase="design-hero"[\s\S]+?<\/section>/)?.[0];

  assert.ok(hero);
  assert.match(hero, /data-reveal=""[^>]*>A music player for GNOME and Android/);
  assert.match(hero, /data-showcase="scroll-cue"/);
  assert.equal((hero.match(/<button[^>]+type="button"[^>]+data-shot=""/g) ?? []).length, 2);
  assert.match(
    hero,
    /class="[^"]*hero-product__phone[^"]*"[\s\S]+?data-showcase="visualizer-plate"/,
  );
  assert.match(
    css,
    /\.hero\{[^}]*padding:clamp\(6rem,4\.5rem \+ 6vw,10rem\) 0 clamp\(3rem,2rem \+ 4vw,6rem\)/,
  );
  assert.match(css, /\.hero__grid\{[^}]*max-width:78rem/);
  assert.match(
    css,
    /\.hero__grid\{[^}]*grid-template-columns:repeat\(auto-fit,minmax\(min\(100%,22rem\),1fr\)\)/,
  );
  assert.match(css, /\.hero-product__phone\{[^}]*right:-5%[^}]*bottom:-6%[^}]*width:24%/);
  assert.match(
    css,
    /\.hero-product__visualizer\{[^}]*left:19\.63%[^}]*top:24\.46%[^}]*width:60\.74%[^}]*height:27\.87%/,
  );
  assert.match(css, /@keyframes rp-cue/);
});

test('the reveal pass never hides what it has already shown', async () => {
  const source = await readFile(join(showroomRoot, 'src', 'lib', 'reveal.ts'), 'utf8');

  // `reveal` refuses to run twice on the same element, so a second
  // `prepareReveals` — a hot reload, a changed motion preference, any re-run of
  // the effect — must not put those elements back to opacity 0.
  assert.match(source, /querySelectorAll<HTMLElement>\('\[data-reveal\]'\)\)\.filter\(/);
  assert.match(source, /!element\.dataset\.shown/);
  assert.match(source, /if \(element\.dataset\.shown\) return;/);
});
