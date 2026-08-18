import { useEffect, useState } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

/**
 * Tracks the reader's motion preference, and keeps tracking it: the media query
 * can flip while the page is open, and a page that only reads it once would keep
 * animating for someone who just asked it to stop.
 *
 * Returns `false` during the prerender, which is the honest answer there — the
 * server has no reader to ask, and the first client effect corrects it before
 * anything moves.
 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    const query = window.matchMedia(QUERY);
    setReduced(query.matches);
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, []);

  return reduced;
}
