import { shouldPlay } from './policy';

/** Pause decorative work when it cannot be seen or motion is unwanted. */
export function observeSceneActivity(element: HTMLElement, update: (active: boolean) => void) {
  const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const root = element.closest('#showroom-root');
  let visible = false;
  const synchronize = () =>
    update(
      shouldPlay({ intersecting: visible, reducedMotion: motion.matches }) &&
        !document.hidden &&
        !root?.hasAttribute('inert'),
    );
  const intersection = new IntersectionObserver(([entry]) => {
    visible = entry?.isIntersecting ?? false;
    synchronize();
  });
  intersection.observe(element);
  const covered = new MutationObserver(synchronize);
  if (root) covered.observe(root, { attributes: true, attributeFilter: ['inert'] });
  motion.addEventListener('change', synchronize);
  document.addEventListener('visibilitychange', synchronize);
  synchronize();
  return () => {
    intersection.disconnect();
    covered.disconnect();
    motion.removeEventListener('change', synchronize);
    document.removeEventListener('visibilitychange', synchronize);
  };
}
