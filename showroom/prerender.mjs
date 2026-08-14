// Renders the app to static HTML and writes it into the built index.html.
//
// The showroom argues that reach beats spectacle: a presentation page has to be
// readable without JavaScript, or it contradicts the accessibility chapter it
// contains. So the client bundle hydrates a document that is already complete.

import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const templatePath = join(here, 'dist', 'index.html');
const MARKER = '<div id="root"></div>';

const { render } = await import(join(here, 'dist-ssr', 'entry-server.js'));
const template = await readFile(templatePath, 'utf8');

if (!template.includes(MARKER)) {
  throw new Error(`prerender: mount point ${MARKER} not found in dist/index.html`);
}

const html = render();
await writeFile(templatePath, template.replace(MARKER, `<div id="root">${html}</div>`), 'utf8');

process.stdout.write(`prerendered ${html.length} bytes into dist/index.html\n`);
