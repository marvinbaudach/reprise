import { useEffect } from 'react';
import { prepareCounter, runCounter, runCountersIn } from '../lib/counters';
import { prepareReveals, type RevealState, sweepReveals } from '../lib/reveal';
import { drawSeekTracks, SEEK_FRAME_EVENT } from '../lib/seekRenderer';

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
/**
 * How far above the viewport middle the next chapter's top edge starts tinting
 * the ground, as a share of the viewport. Half a screen is long enough that the
 * blend never reads as a switch and short enough that the colour still clearly
 * belongs to the chapter the reader is looking at.
 */
const GROUND_BLEND_FRACTION = 0.5;
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
    // Resolved once: looking the target up by id on every link on every frame
    // is a document query per link per frame for a set that never changes.
    const navLinks = Array.from(root.querySelectorAll<HTMLAnchorElement>('[data-navlink]'))
      .map((link) => ({
        link,
        target: document.getElementById(link.getAttribute('href')?.slice(1) ?? ''),
      }))
      .filter((entry): entry is { link: HTMLAnchorElement; target: HTMLElement } =>
        Boolean(entry.target),
      );

    for (const element of root.querySelectorAll<HTMLElement>('[data-counter]')) {
      prepareCounter(element);
      if (still) runCounter(element, 0, true);
    }
    if (!still) {
      for (const bar of root.querySelectorAll<HTMLElement>('[data-ratio] > span')) {
        if (bar.getBoundingClientRect().top >= window.innerHeight * 0.88) bar.style.width = '0%';
      }
      // Same bargain for the timeline's rail: it is drawn in the markup, and
      // only a run that is going to animate it may take it away first.
      for (const track of root.querySelectorAll<HTMLElement>('[data-weeks]')) {
        if (track.getBoundingClientRect().top >= window.innerHeight * 0.88) {
          track.dataset.collapsed = '';
        }
      }
    }
    let reveals: RevealState = prepareReveals(root, still);
    let ratioRun = false;
    let currentGround = '';
    let pointerX = 0;
    let pointerY = 0;
    let scrollBias = 0;
    let frame: number | null = null;
    let pointerFrame: number | null = null;
    let pageHeight: number | null = null;

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

    const tick = (timestamp = performance.now()) => {
      frame = null;
      const doc = document.documentElement;
      const viewportHeight = window.innerHeight;
      // `scrollHeight` forces a layout, and the page only changes height when
      // something in it does — which the observer below already reports.
      if (pageHeight === null) pageHeight = doc.scrollHeight;
      const max = Math.max(1, pageHeight - viewportHeight);
      const top = window.scrollY || doc.scrollTop || 0;
      const progressed = Math.min(1, Math.max(0, top / max));

      reveals = sweepReveals(reveals, (element) => {
        const delay = Number.parseInt(element.style.transitionDelay, 10) || 0;
        runCountersIn(element, delay + COUNTER_DELAY_MS, still);
        if (element.matches('[data-ratio]') || element.querySelector('[data-ratio]')) runRatio();
      });

      // The ground is a blend of the chapter that owns the middle of the viewport
      // and the one coming after it, weighted by how far the next chapter's top
      // edge still has to travel. That makes the colour a function of the scroll
      // position instead of an event with a tail: it stops when the reader stops
      // and it retraces itself on the way back up. Rects are read at most one
      // past the owner, so this stays the same cost as the old ownership test.
      if (ground) {
        const middle = viewportHeight / 2;
        let ownerIndex = 0;
        let nextTop = Number.POSITIVE_INFINITY;
        for (let i = 0; i < sections.length; i += 1) {
          const section = sections[i];
          if (!section) break;
          const sectionTop = section.getBoundingClientRect().top;
          if (sectionTop <= middle) {
            ownerIndex = i;
            continue;
          }
          nextTop = sectionTop;
          break;
        }
        const from = sections[ownerIndex]?.dataset.ground ?? '';
        const to = sections[ownerIndex + 1]?.dataset.ground ?? from;
        const band = Math.max(1, viewportHeight * GROUND_BLEND_FRACTION);
        const blend = Math.min(1, Math.max(0, 1 - (nextTop - middle) / band));
        // Mixing in oklab is the browser's job: these are oklch colours, and
        // interpolating them by hand would mean owning hue wrap-around too.
        const mixed =
          blend <= 0 || to === from
            ? from
            : `color-mix(in oklab, ${to} ${(blend * 100).toFixed(1)}%, ${from})`;
        if (mixed !== currentGround) {
          currentGround = mixed;
          ground.style.backgroundColor = mixed;
        }
      }

      scrollBias = progressed * 2 - 1;
      moveOil();

      if (progress) progress.style.width = `${(progressed * 100).toFixed(2)}%`;
      if (header) header.dataset.lifted = top > HEADER_SCROLL_PX ? 'true' : 'false';

      let active: HTMLAnchorElement | null = null;
      const navLine = viewportHeight * NAV_ACTIVE_FRACTION;
      for (const { link, target } of navLinks) {
        if (target.getBoundingClientRect().top <= navLine) active = link;
      }
      for (const { link } of navLinks) link.dataset.current = link === active ? 'true' : 'false';
      if (drawSeekTracks(timestamp, still)) schedule();
    };

    const schedule = () => {
      if (frame === null) frame = requestAnimationFrame(tick);
    };

    const onResize = () => {
      pageHeight = null;
      schedule();
    };

    // The pointer fires far more often than the screen refreshes. Writing the
    // transform straight from the event means several style writes for a single
    // painted frame; recording the position and letting a dedicated frame move
    // only the oil costs one write per frame and no page-wide layout pass.
    const onPointerMove = (event: PointerEvent) => {
      pointerX = (event.clientX / window.innerWidth) * 2 - 1;
      pointerY = (event.clientY / window.innerHeight) * 2 - 1;
      if (pointerFrame === null) {
        pointerFrame = requestAnimationFrame(() => {
          pointerFrame = null;
          moveOil();
        });
      }
    };

    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', onResize, { passive: true });
    if (!still) window.addEventListener('pointermove', onPointerMove, { passive: true });
    window.addEventListener(SEEK_FRAME_EVENT, schedule);
    tick();
    // Fonts and images land after the first frame and move everything below them;
    // two more passes catch the elements that were mid-air at that moment.
    requestAnimationFrame(schedule);
    const late = window.setTimeout(schedule, 400);

    // A timer is a guess. Anything that changes the height of the page after it
    // has run — a late webfont, a decoded image, a browser extension restyling
    // the document — would otherwise leave the elements it moved into view
    // hidden until the reader scrolls, and a reader who sees an empty page has
    // no reason to scroll. The observer turns that guess into a fact.
    const growth = new ResizeObserver(() => {
      pageHeight = null;
      schedule();
    });
    growth.observe(root);
    document.fonts?.ready.then(schedule).catch(() => undefined);

    return () => {
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener(SEEK_FRAME_EVENT, schedule);
      window.clearTimeout(late);
      growth.disconnect();
      if (frame !== null) cancelAnimationFrame(frame);
      if (pointerFrame !== null) cancelAnimationFrame(pointerFrame);
    };
  }, [still]);
}
