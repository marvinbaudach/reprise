import { useEffect } from 'react';
import { drawSeekTracks, SEEK_FRAME_EVENT } from '../lib/seekRenderer';

const HEADER_SCROLL_PX = 60;
const NAV_ACTIVE_FRACTION = 0.4;

/** Reading updates navigation only. Content never waits for a scroll animation. */
export function usePageChoreography(still: boolean): void {
  useEffect(() => {
    const root = document.getElementById('showroom-root');
    if (!root) return undefined;
    const progress = document.getElementById('scroll-progress');
    const header = document.getElementById('site-header');
    const links = Array.from(root.querySelectorAll<HTMLAnchorElement>('[data-navlink]')).map(
      (link) => ({ link, target: document.getElementById(link.hash.slice(1)) }),
    );
    let frame: number | null = null;
    let disposed = false;
    const tick = (timestamp: number) => {
      frame = null;
      const top = window.scrollY;
      const max = Math.max(1, document.documentElement.scrollHeight - window.innerHeight);
      if (progress) progress.style.width = `${Math.min(100, (top / max) * 100)}%`;
      if (header) header.dataset.lifted = String(top > HEADER_SCROLL_PX);
      let active: HTMLAnchorElement | null = null;
      for (const { link, target } of links) {
        if (
          target &&
          target.getBoundingClientRect().top <= window.innerHeight * NAV_ACTIVE_FRACTION
        )
          active = link;
      }
      for (const { link } of links) {
        link.dataset.current = String(link === active);
        if (link === active) link.setAttribute('aria-current', 'location');
        else link.removeAttribute('aria-current');
      }
      if (drawSeekTracks(timestamp, still)) schedule();
    };
    const schedule = () => {
      if (!disposed && frame === null) frame = requestAnimationFrame(tick);
    };
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule, { passive: true });
    window.addEventListener(SEEK_FRAME_EVENT, schedule);
    const growth = new ResizeObserver(schedule);
    growth.observe(root);
    document.fonts?.ready.then(schedule).catch(() => undefined);
    schedule();
    return () => {
      disposed = true;
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      window.removeEventListener(SEEK_FRAME_EVENT, schedule);
      growth.disconnect();
      if (frame !== null) cancelAnimationFrame(frame);
    };
  }, [still]);
}
