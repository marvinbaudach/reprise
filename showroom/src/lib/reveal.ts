/**
 * The reveal pass.
 *
 * Deliberately driven by the scroll position rather than an `IntersectionObserver`:
 * one throttled sweep over a shrinking set of hidden elements behaves the same in
 * every browser, and — unlike an observer that never fires for an element the
 * layout moved out from under it — it cannot leave content invisible. Anything
 * already scrolled past is revealed without delay instead of waiting for a
 * threshold it will never cross again.
 */

/** Elements enter once their top edge is inside this share of the viewport. */
const TRIGGER_FRACTION = 0.88;
/** Travel of the entrance, in pixels. The hero starts closer, so it settles sooner. */
const TRAVEL_PX = 24;
const HERO_TRAVEL_FACTOR = 0.7;
/** Stagger: siblings step, and everything entering in the same sweep steps again. */
const SIBLING_STEP_MS = 70;
const SIBLING_STEP_CAP = 7;
const BATCH_STEP_MS = 20;
const BATCH_STEP_CAP = 4;
const SETTLE_MS = 1100;

const TRANSITION = [
  'opacity 780ms cubic-bezier(0.16, 1, 0.3, 1)',
  'transform 900ms cubic-bezier(0.16, 1, 0.3, 1)',
  'filter 900ms ease',
].join(', ');

export type RevealState = { pending: HTMLElement[] };

function isHero(element: HTMLElement): boolean {
  return Boolean(element.closest('#hero'));
}

/** Hides everything that has not been scrolled past yet and returns the queue. */
export function prepareReveals(root: HTMLElement, still: boolean): RevealState {
  // Anything already shown stays shown. A second pass — a hot reload, a changed
  // motion preference, any re-run of the effect — would otherwise hide it again
  // while `reveal` refuses to touch it a second time, and the page stays blank.
  const pending = Array.from(root.querySelectorAll<HTMLElement>('[data-reveal]')).filter(
    (element) => !element.dataset.shown,
  );

  for (const element of pending) {
    element.style.willChange = 'opacity, transform';
    element.style.transition = TRANSITION;
    if (still) continue;
    const hero = isHero(element);
    const belowFold = element.getBoundingClientRect().top >= window.innerHeight * TRIGGER_FRACTION;
    if (!hero && !belowFold) continue;
    const travel = Math.round(hero ? TRAVEL_PX * HERO_TRAVEL_FACTOR : TRAVEL_PX);
    element.style.opacity = '0';
    element.style.transform = `translate3d(0, ${travel}px, 0) scale(0.99)`;
    if (element.dataset.reveal === 'img') element.style.filter = 'blur(7px)';
  }

  if (still) {
    for (const element of pending) reveal(element, 0);
    return { pending: [] };
  }
  return { pending };
}

export function reveal(element: HTMLElement, delay: number): void {
  if (element.dataset.shown) return;
  element.dataset.shown = '1';
  element.style.transitionDelay = `${delay}ms`;
  element.style.opacity = '1';
  element.style.transform = 'none';
  element.style.filter = 'none';
  window.setTimeout(() => {
    element.style.willChange = 'auto';
    element.style.transitionDelay = '0ms';
  }, SETTLE_MS + delay);
}

/**
 * Reveals everything that has come into range and returns what is still waiting.
 * Elements above the viewport are shown without a delay — their entrance already
 * happened off-screen, and staging it now would only look like a glitch.
 */
export function sweepReveals(
  state: RevealState,
  onReveal: (element: HTMLElement) => void,
): RevealState {
  if (!state.pending.length) return state;
  const viewportHeight = window.innerHeight;
  const limit = viewportHeight * TRIGGER_FRACTION;

  // Read the whole queue before touching anything. Revealing an element writes
  // styles, and measuring the next one right after forces the layout to be
  // recomputed — once per element, every frame, for as long as the queue is
  // long. One read pass costs one layout no matter how many elements it covers.
  const boxes = state.pending.map((element) => element.getBoundingClientRect());

  const rest: HTMLElement[] = [];
  const entering: { element: HTMLElement; delay: number }[] = [];
  let batch = 0;

  for (const [index, element] of state.pending.entries()) {
    const box = boxes[index];
    if (!box || box.top >= limit) {
      if (element) rest.push(element);
      continue;
    }
    const staged = box.bottom > -viewportHeight;
    entering.push({
      element,
      delay: staged
        ? Math.min(siblingIndex(element), SIBLING_STEP_CAP) * SIBLING_STEP_MS +
          Math.min(batch, BATCH_STEP_CAP) * BATCH_STEP_MS
        : 0,
    });
    if (staged) batch += 1;
  }

  for (const { element, delay } of entering) reveal(element, delay);

  // Every element is visible before the first side effect runs, and each side
  // effect is on its own. A counter or a ratio bar that throws would otherwise
  // abandon the rest of the queue: those elements stay at `opacity: 0`, the
  // queue is never handed back, and the next sweep breaks at the same element
  // again — the page loses everything below the fault, for good.
  for (const { element } of entering) {
    try {
      onReveal(element);
    } catch (error) {
      console.error('reveal: a side effect of an entering element failed', error);
    }
  }

  return { pending: rest };
}

function siblingIndex(element: HTMLElement): number {
  const parent = element.parentElement;
  if (!parent) return 0;
  const siblings = Array.from(parent.children).filter((node) => node.hasAttribute('data-reveal'));
  return Math.max(0, siblings.indexOf(element));
}
