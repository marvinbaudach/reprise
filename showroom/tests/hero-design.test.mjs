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
  assert.match(hero, /A music player for GNOME and Android/);
  assert.match(hero, /href="#film"/);
  assert.doesNotMatch(hero, /data-showcase="scroll-cue"/);
  assert.equal((hero.match(/<button[^>]+type="button"[^>]+data-shot=""/g) ?? []).length, 2);
  assert.match(css, /\.hero__actions/);
  assert.match(css, /\.hero__grid\{[^}]*max-width:78rem/);
  assert.match(
    css,
    /\.hero__grid\{[^}]*grid-template-columns:repeat\(auto-fit,minmax\(min\(100%,22rem\),1fr\)\)/,
  );
  // Vite 8 minifies CSS with Lightning CSS, which sorts the declarations inside a
  // block. The order they appear in is therefore the minifier's business; what
  // this test owns is that the rule carries all three.
  const phone = css.match(/\.hero-product__phone\{[^}]*\}/)?.[0];
  assert.ok(phone, '.hero-product__phone must exist in the built CSS');
  // The lean is capped at the room the frame actually has: below roughly 800px
  // 5% of the frame is wider than --frame-pad, and the tile was drawn outside
  // the window and clipped. `max` keeps -5% wherever it fits.
  for (const declaration of [
    'right:max(-5%, calc(-1 * var(--frame-pad)))',
    'bottom:-6%',
    'width:24%',
  ]) {
    assert.ok(phone.includes(declaration), `.hero-product__phone must carry ${declaration}`);
  }
});

test('the header and hero offer one in-page route to availability', async () => {
  const [html, chromeCss] = await Promise.all([
    readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8'),
    readFile(join(showroomRoot, 'src', 'components', 'chrome', 'chrome.css'), 'utf8'),
  ]);
  const hero = html.match(/<section[^>]+data-showcase="design-hero"[\s\S]+?<\/section>/)?.[0];
  const header = html.match(/<header[^>]+class="site-header"[\s\S]+?<\/header>/)?.[0];
  assert.ok(hero);
  assert.ok(header);

  assert.match(
    header,
    /site-header__source[\s\S]*site-header__split[\s\S]*class="site-header__hire" href="#availability">Work with me<\/a>/,
  );
  const hire = header.match(/<a class="site-header__hire"[^>]*>/)?.[0];
  assert.ok(hire);
  assert.doesNotMatch(hire, /data-navlink/);

  const normalizedHero = hero.replace(/<!-- -->/g, '').replace(/\s+/g, ' ');
  assert.match(normalizedHero, /Marvin Baudach/);
  assert.match(normalizedHero, /Product design, architecture and quality/);
  assert.match(normalizedHero, /href="#availability"/);
  assert.match(normalizedHero, /href="#film"/);
  assert.doesNotMatch(html, /mailto:/);
  assert.match(
    chromeCss,
    /\.site-header__nav a:not\(\.site-header__hire\):hover,\s*\.site-header__nav a\[data-current="true"\]\s*\{/,
  );

  const mobile = chromeCss.slice(
    chromeCss.indexOf('@media (max-width: 70rem)'),
    chromeCss.indexOf('@media (max-width: 26.5rem)'),
  );
  assert.match(mobile, /\.site-header__source,\s*\.site-header__split\s*\{[^}]*display: none;/);
  assert.match(mobile, /\.site-header__hire\s*\{[^}]*padding: 9px 14px;/);

  // A backdrop blur behind a fixed, lifted header is a compositing pass on every
  // scrolled frame. The lightbox already refuses it at this width; so does this.
  const phones = chromeCss.slice(chromeCss.indexOf('@media (max-width: 720px)'));
  assert.match(phones, /\.site-header\[data-lifted="true"\]\s*\{[^}]*backdrop-filter: none;/);
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
