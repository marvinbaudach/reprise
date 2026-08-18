import { StrictMode } from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';
import { App } from './App';

const root = document.getElementById('root');
if (!root) throw new Error('mount point #root is missing');

const tree = (
  <StrictMode>
    <App />
  </StrictMode>
);

// Only the built page carries prerendered markup; the dev server serves the bare
// shell. Hydrating an empty container makes React report a mismatch, throw the
// whole tree away and rebuild it — which delayed the first paint far enough for
// the reveal pass to measure a layout that was not settled yet.
if (root.firstChild) {
  hydrateRoot(root, tree);
} else {
  createRoot(root).render(tree);
}
