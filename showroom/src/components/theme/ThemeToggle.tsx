import { useTheme, type ThemeChoice } from '../../hooks/useTheme';
import './theme-toggle.css';

const OPTIONS: readonly { value: ThemeChoice; label: string; glyph: string }[] = [
  { value: 'system', label: 'Follow the system setting', glyph: '◐' },
  { value: 'light', label: 'Light', glyph: '☀' },
  { value: 'dark', label: 'Dark', glyph: '☾' },
];

export function ThemeToggle() {
  const { choice, setChoice } = useTheme();

  return (
    <div className="theme-toggle" role="group" aria-label="Colour theme">
      {OPTIONS.map((option) => (
        <button
          key={option.value}
          type="button"
          className="theme-toggle__option"
          aria-pressed={choice === option.value}
          title={option.label}
          onClick={() => setChoice(option.value)}
        >
          <span aria-hidden="true">{option.glyph}</span>
          <span className="theme-toggle__label">{option.label}</span>
        </button>
      ))}
    </div>
  );
}
