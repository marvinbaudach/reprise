/**
 * Count-up for the figures.
 *
 * The number is read back out of the DOM rather than passed in, so the markup
 * stays the single source of a figure and there is no second copy to drift. The
 * thousands separator the page uses (a typographic apostrophe) is preserved,
 * along with any prefix or suffix around the number.
 */

const DURATION_MS = 1250;
const NUMBER = /(\d[\d'’]*(?:\.\d+)?)/;

type Counter = {
  target: number;
  decimals: number;
  grouped: boolean;
  separator: string;
  prefix: string;
  suffix: string;
};

const counters = new WeakMap<HTMLElement, Counter>();
const started = new WeakSet<HTMLElement>();

function format(value: number, counter: Counter): string {
  let text = counter.decimals ? value.toFixed(counter.decimals) : String(Math.round(value));
  if (counter.grouped) {
    const decimalIndex = text.indexOf('.');
    const integer = decimalIndex === -1 ? text : text.slice(0, decimalIndex);
    const fraction = decimalIndex === -1 ? '' : text.slice(decimalIndex);
    text = integer.replace(/\B(?=(\d{3})+(?!\d))/g, counter.separator) + fraction;
  }
  return text;
}

/** Parses the figure and zeroes it, unless it is already on screen. */
export function prepareCounter(element: HTMLElement): void {
  const raw = element.textContent?.trim() ?? '';
  const match = raw.match(NUMBER);
  if (!match || match.index === undefined) return;
  const number = match[1];
  if (number === undefined) return;
  const separator = number.match(/['’]/)?.[0] ?? '’';
  const plain = number.replace(/['’]/g, '');
  const counter: Counter = {
    target: Number.parseFloat(plain),
    decimals: (plain.split('.')[1] ?? '').length,
    grouped: /['’]/.test(number),
    separator,
    prefix: raw.slice(0, match.index),
    suffix: raw.slice(match.index + number.length),
  };
  counters.set(element, counter);
  if (element.getBoundingClientRect().top >= window.innerHeight * 0.86) {
    element.textContent = counter.prefix + format(0, counter) + counter.suffix;
  }
}

export function runCounter(element: HTMLElement, delay: number, still: boolean): void {
  const counter = counters.get(element);
  if (!counter || started.has(element)) return;
  started.add(element);

  const settle = () => {
    element.textContent = counter.prefix + format(counter.target, counter) + counter.suffix;
  };
  if (still) {
    settle();
    return;
  }

  const start = performance.now() + delay;
  const tick = (now: number) => {
    const t = Math.max(0, Math.min(1, (now - start) / DURATION_MS));
    // Quartic ease-out: the figure lands rather than creeping to its value.
    const eased = 1 - (1 - t) ** 4;
    element.textContent = counter.prefix + format(counter.target * eased, counter) + counter.suffix;
    if (t < 1) requestAnimationFrame(tick);
    else settle();
  };
  requestAnimationFrame(tick);
}

/** Runs the counters inside (or on) a revealed element. */
export function runCountersIn(element: HTMLElement, delay: number, still: boolean): void {
  if (element.hasAttribute('data-counter')) runCounter(element, delay, still);
  for (const child of element.querySelectorAll<HTMLElement>('[data-counter]')) {
    runCounter(child, delay, still);
  }
}
