import { useCallback, useEffect, useState } from 'react';

/**
 * Three states, the way current frameworks do it: no choice means follow the
 * system, and an explicit choice wins until it is taken back.
 *
 * The stored value is applied by an inline script in `index.html` before the
 * first paint. This hook only keeps React in step with what that script already
 * did — it must never be the thing that first applies the theme, or the page
 * flashes on every load.
 */

export type ThemeChoice = 'system' | 'light' | 'dark';

const STORAGE_KEY = 'reprise-theme';

function readStoredChoice(): ThemeChoice {
  if (typeof document === 'undefined') return 'system';
  const attribute = document.documentElement.getAttribute('data-theme');
  return attribute === 'light' || attribute === 'dark' ? attribute : 'system';
}

export function useTheme(): { choice: ThemeChoice; setChoice: (next: ThemeChoice) => void } {
  const [choice, setChoiceState] = useState<ThemeChoice>(readStoredChoice);

  // The server renders 'system'; if storage said otherwise, catch up once mounted.
  useEffect(() => {
    setChoiceState(readStoredChoice());
  }, []);

  const setChoice = useCallback((next: ThemeChoice) => {
    setChoiceState(next);

    if (next === 'system') {
      document.documentElement.removeAttribute('data-theme');
    } else {
      document.documentElement.setAttribute('data-theme', next);
    }

    try {
      if (next === 'system') {
        localStorage.removeItem(STORAGE_KEY);
      } else {
        localStorage.setItem(STORAGE_KEY, next);
      }
    } catch (error) {
      // Blocked storage: the choice still holds for this page view.
    }
  }, []);

  return { choice, setChoice };
}
