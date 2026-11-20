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
  // The hero says who built it and that the work was AI-assisted, in one plain line.
  assert.match(hero, /quality by one engineer, AI-assisted\./);
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
    'left:max(-5%, calc(-1 * var(--frame-pad)))',
    'bottom:-6%',
    'width:24%',
  ]) {
    assert.ok(phone.includes(declaration), `.hero-product__phone must carry ${declaration}`);
  }
});

test('the header and hero carry no call to action, only the chapter nav and the source link', async () => {
  const [html, chromeCss] = await Promise.all([
    readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8'),
    readFile(join(showroomRoot, 'src', 'components', 'chrome', 'chrome.css'), 'utf8'),
  ]);
  const hero = html.match(/<section[^>]+data-showcase="design-hero"[\s\S]+?<\/section>/)?.[0];
  const header = html.match(/<header[^>]+class="site-header"[\s\S]+?<\/header>/)?.[0];
  assert.ok(hero);
  assert.ok(header);

  assert.match(header, /site-header__source/);
  assert.doesNotMatch(
    header,
    /site-header__hire|site-header__split|Work with me|href="#availability"/,
  );

  const normalizedHero = hero.replace(/<!-- -->/g, '').replace(/\s+/g, ' ');
  assert.match(normalizedHero, /Marvin Baudach/);
  assert.match(normalizedHero, /Product design, architecture and quality/);
  // The hero byline names the stack and offers no link.
  assert.doesNotMatch(
    normalizedHero,
    /href="#availability"|Work with me|Open to (?:work|opportunities)/,
  );
  assert.match(
    normalizedHero,
    /<div class="hero__byline frame"><span>Rust · GTK4 · Kotlin · Compose<\/span><\/div>/,
  );
  // The showroom is about the project: nothing on the page may say its author
  // is looking for work, and nothing may point at the removed availability block.
  assert.doesNotMatch(
    html,
    /permanent role|open to work|open to opportunit|work with me|freelanc|hire me|id="availability/i,
  );
  // No outbound arrow on an in-page anchor.
  for (const anchor of html.match(/<a[^>]+href="#[^"]*"[^>]*>[\s\S]*?<\/a>/g) ?? []) {
    assert.doesNotMatch(anchor, /↗/, `an in-page link points outward: ${anchor}`);
  }
  assert.doesNotMatch(chromeCss, /site-header__hire|site-header__split/);
  assert.match(normalizedHero, /href="#film"/);
  assert.doesNotMatch(html, /mailto:/);
  assert.match(
    chromeCss,
    /\.site-header__nav a:hover,\s*\.site-header__nav a\[data-current="true"\]\s*\{/,
  );

  const mobile = chromeCss.slice(
    chromeCss.indexOf('@media (max-width: 70rem)'),
    chromeCss.indexOf('@media (max-width: 26.5rem)'),
  );
  assert.match(mobile, /\.site-header__source\s*\{[^}]*display: none;/);

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
