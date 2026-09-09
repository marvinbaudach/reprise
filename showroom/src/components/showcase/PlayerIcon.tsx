export type PlayerIconName =
  | 'play'
  | 'pause'
  | 'replay'
  | 'volume'
  | 'muted'
  | 'expand'
  | 'collapse';

const paths: Record<PlayerIconName, string> = {
  play: 'M8 5v14l11-7z',
  pause: 'M7 5v14M17 5v14',
  replay: 'M4 11a8 8 0 1 1 2 7M4 4v7h7',
  volume: 'M11 4 6 8H3v8h3l5 4zM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14',
  muted: 'M11 4 6 8H3v8h3l5 4zM16 9l6 6M22 9l-6 6',
  expand: 'M8 3H3v5M16 3h5v5M21 16v5h-5M8 21H3v-5',
  collapse: 'M3 8h5V3M21 8h-5V3M16 21v-5h5M8 21v-5H3',
};

export function PlayerIcon({ name }: { readonly name: PlayerIconName }) {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true" fill="none">
      <path
        d={paths[name]}
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill={name === 'play' ? 'currentColor' : 'none'}
      />
    </svg>
  );
}
