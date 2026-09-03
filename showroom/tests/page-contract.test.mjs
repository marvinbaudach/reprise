import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test } from 'node:test';

const showroomRoot = new URL('..', import.meta.url).pathname;
const repositoryRoot = join(showroomRoot, '..');

async function builtCss() {
  const assets = join(showroomRoot, 'dist', 'assets');
  const stylesheet = (await readdir(assets)).find((entry) => entry.endsWith('.css'));
  assert.ok(stylesheet);
  return readFile(join(assets, stylesheet), 'utf8');
}

test('the prerendered document has a direct keyboard route into its single main landmark', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');

  assert.equal((html.match(/<main\b/g) ?? []).length, 1);
  assert.match(html, /<a[^>]+class="skip-link"[^>]+href="#main-content"/);
  assert.match(html, /<main id="main-content" tabindex="-1">/);
  assert.equal((html.match(/<h1\b/g) ?? []).length, 1);
  for (const chapter of ['ch-01', 'ch-02', 'ch-03', 'ch-04', 'ch-05']) {
    assert.match(html, new RegExp(`href="#${chapter}"`));
    assert.match(html, new RegExp(`<section id="${chapter}"`));
  }
});

test('motion, the in-flow film, and social previews survive the production build', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();

  assert.match(css, /prefers-reduced-motion:\s*reduce/);
  assert.match(css, /\.skip-link:focus-visible/);
  assert.match(html, /data-showcase="showreel-film"/);
  assert.doesNotMatch(html, /product-gallery-hint|Use arrow keys, drag, or scroll/);
  assert.equal((html.match(/loading="eager"/g) ?? []).length, 2);
  assert.match(html, /property="og:title"/);
  assert.match(
    html,
    /property="og:image"\s+content="https:\/\/marvinbaudach\.github\.io\/reprise\/media\/showroom\/gnome-library\.webp"/,
  );
  assert.match(html, /property="og:url" content="https:\/\/marvinbaudach\.github\.io\/reprise\/"/);
  assert.match(html, /name="twitter:card" content="summary_large_image"/);
});

test('the Pages workflow runs the same showroom contract suite as local development', async () => {
  const workflow = await readFile(join(repositoryRoot, '.github/workflows/pages.yml'), 'utf8');

  assert.match(workflow, /name: Test build and prerender/);
  assert.match(workflow, /run: npm test/);
  assert.doesNotMatch(workflow, /name: Prove the prerender produced content/);
});

test('the showroom ships one deliberate dark theme without a stored mode switch', async () => {
  const html = await readFile(join(showroomRoot, 'dist', 'index.html'), 'utf8');
  const css = await builtCss();

  assert.match(html, /<meta name="color-scheme" content="dark"/);
  assert.doesNotMatch(html, /Colour theme|reprise-theme|localStorage|data-theme/);
  assert.doesNotMatch(css, /prefers-color-scheme:\s*light|data-theme/);
});
