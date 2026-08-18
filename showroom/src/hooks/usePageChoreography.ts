import { useEffect } from 'react';
import { prepareCounter, runCountersIn } from '../lib/counters';
import { prepareReveals, type RevealState, sweepReveals } from '../lib/reveal';

/**
 * Everything the page does while it is scrolled, in one pass.
 *
 * One listener, one rAF-throttled tick, one read of the layout per frame: the
 * reveal sweep, the ground colour under the page, the progress line, the header
 * state, the active chapter link and the parallax of the light. Splitting these
 * into a hook each would read tidier and cost a forced reflow per hook, which is
 * the thing the app itself is measured against.
 */

const COUNTER_DELAY_MS = 80;
const HEADER_SCROLL_PX = 60;
/** A chapter counts as current once its top edge passes this share of the viewport. */
const NAV_ACTIVE_FRACTION = 0.4;
const OIL_POINTER_X_PX = 26;
const OIL_POINTER_Y_PX = 20;
const OIL_SCROLL_PX = 44;

export function usePageChoreography(still: boolean): void {
  useEffect(() => {
    const root = document.getElementById('showroom-root');
    if (!root) return undefined;

    const ground = document.getElementById('backdrop-ground');
    const oil = document.getElementById('backdrop-oil');
    const progress = document.getElementById('scroll-progress');
    const header = document.getElementById('site-header');
    const sections = Array.from(root.querySelectorAll<HTMLElement>('[data-ground]'));
    const navLinks = Array.from(root.querySelectorAll<HTMLAnchorElement>('[data-navlink]'));

    for (const element of root.querySelectorAll<HTMLElement>('[data-counter]'))
      prepareCounter(element);
    let reveals: RevealState = prepareReveals(root, still);
    let ratioRun = false;
    let currentGround: HTMLElement | null = null;
    let pointerX = 0;
    let pointerY = 0;
    let scrollBias = 0;
    let frame: number | null = null;

    const runRatio = () => {
      if (ratioRun) return;
      ratioRun = true;
      for (const bar of root.querySelectorAll<HTMLElement>('[data-ratio] > span')) {
        bar.style.width = `${bar.dataset.w ?? 0}%`;
      }
    };

    const moveOil = () => {
      if (!oil) return;
      if (still) {
        oil.style.transform = 'none';
        return;
      }
      const x = pointerX * OIL_POINTER_X_PX;
      const y = pointerY * OIL_POINTER_Y_PX + scrollBias * OIL_SCROLL_PX;
      oil.style.transform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0) scale(1.02)`;
    };

    const tick = () => {
      frame = null;
      const doc = document.documentElement;
      const max = Math.max(1, doc.scrollHeight - window.innerHeight);
      const top = window.scrollY || doc.scrollTop || 0;
      const progressed = Math.min(1, Math.max(0, top / max));

      reveals = sweepReveals(reveals, (element) => {
        const delay = Number.parseInt(element.style.transitionDelay, 10) || 0;
        runCountersIn(element, delay + COUNTER_DELAY_MS, still);
        if (element.matches('[data-ratio]') || element.querySelector('[data-ratio]')) runRatio();
      });

      // The ground is the section that owns the middle of the viewport, so the
      // colour changes when a chapter takes over rather than when it first peeks in.
      if (ground) {
        const middle = window.innerHeight / 2;
        const owner = sections.find((section) => {
          const box = section.getBoundingClientRect();
          return box.top <= middle && box.bottom > middle;
        });
        if (owner && owner !== currentGround) {
          currentGround = owner;
          ground.style.backgroundColor = owner.dataset.ground ?? '';
        }
      }

      scrollBias = progressed * 2 - 1;
      moveOil();

      if (progress) progress.style.width = `${(progressed * 100).toFixed(2)}%`;
      if (header) header.dataset.lifted = top > HEADER_SCROLL_PX ? 'true' : 'false';

      let active: HTMLAnchorElement | null = null;
      for (const link of navLinks) {
        const target = document.getElementById(link.getAttribute('href')?.slice(1) ?? '');
        if (
          target &&
          target.getBoundingClientRect().top <= window.innerHeight * NAV_ACTIVE_FRACTION
        ) {
          active = link;
        }
      }
      for (const link of navLinks) link.dataset.current = link === active ? 'true' : 'false';
    };

    const schedule = () => {
      if (frame === null) frame = requestAnimationFrame(tick);
    };

    const onPointerMove = (event: PointerEvent) => {
      pointerX = (event.clientX / window.innerWidth) * 2 - 1;
      pointerY = (event.clientY / window.innerHeight) * 2 - 1;
      moveOil();
    };

    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule, { passive: true });
    window.addEventListener('pointermove', onPointerMove, { passive: true });
    tick();
    // Fonts and images land after the first frame and move everything below them;
    // two more passes catch the elements that were mid-air at that moment.
    requestAnimationFrame(schedule);
    const late = window.setTimeout(schedule, 400);

    return () => {
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
      window.removeEventListener('pointermove', onPointerMove);
      window.clearTimeout(late);
      if (frame !== null) cancelAnimationFrame(frame);
    };
  }, [still]);
}
