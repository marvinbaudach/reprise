export interface ProductCapture {
  readonly id: string;
  readonly title: string;
  readonly platform: 'GNOME' | 'Android';
  readonly description: string;
  readonly alt: string;
  readonly filename: string;
  readonly width: number;
  readonly height: number;
  /** Carries the live visualizer plate wherever the capture is shown. */
  readonly visualizer?: boolean;
}

function capture(
  value: Omit<ProductCapture, 'filename'> & { readonly filename: `${string}.webp` },
): ProductCapture {
  return value;
}

export const HERO_CAPTURES: readonly [ProductCapture, ProductCapture] = [
  capture({
    id: 'gnome-library',
    title: 'Music library',
    platform: 'GNOME',
    description: 'A dense native table, spectral seek, lyrics and Now Playing in one window.',
    alt: 'Reprise running on GNOME with the music library and Now Playing visible',
    filename: 'gnome-library.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'android-visualizer',
    title: 'Now Playing',
    platform: 'Android',
    description: "The scene is the engine's own, ported from bars.rs.",
    alt: 'Reprise on Android showing the audio-reactive Now Playing scene',
    filename: 'android-visualizer.webp',
    width: 1080,
    height: 2404,
    visualizer: true,
  }),
] as const;

export const GALLERY_CAPTURES: readonly ProductCapture[] = [
  capture({
    id: 'gnome-podcasts',
    title: 'Podcasts',
    platform: 'GNOME',
    description: 'Shows, episodes, progress and downloads without leaving the library shell.',
    alt: 'The Reprise Podcasts view with grouped shows and episode download actions',
    filename: 'gnome-podcasts.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'gnome-youtube',
    title: 'YouTube',
    platform: 'GNOME',
    description: 'Subscribed channels become queueable audio sources with honest source identity.',
    alt: 'The Reprise YouTube view with channels and playable videos grouped in the library',
    filename: 'gnome-youtube.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'gnome-radio',
    title: 'Radio discovery',
    platform: 'GNOME',
    description: 'Search, country-aware discovery and live streams share one native dialogue.',
    alt: 'The Reprise radio station discovery dialogue over the native Radio view',
    filename: 'gnome-radio.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'gnome-library-doctor',
    title: 'Library Doctor',
    platform: 'GNOME',
    description:
      'Remote evidence stays reviewable: current, proposed, source and confidence together.',
    alt: 'The Reprise Library Doctor review with grouped metadata proposals and apply controls',
    filename: 'gnome-library-doctor.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'gnome-device-sync',
    title: 'Device sync',
    platform: 'GNOME',
    description: 'Capacity, playlists, transfer profile and deviations live in one explicit run.',
    alt: 'The Reprise device page showing a connected Android phone and playlist sync controls',
    filename: 'gnome-device-sync.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'gnome-layout-controls',
    title: 'Layout controls',
    platform: 'GNOME',
    description:
      'The desktop adapts without abandoning GNOME: player placement, panels and density.',
    alt: 'Reprise layout preferences shown over the listening statistics page',
    filename: 'gnome-layout-controls.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'gnome-listening-stats',
    title: 'Listening statistics',
    platform: 'GNOME',
    description: 'Portrait-led rankings turn local listening history into a private yearly report.',
    alt: 'The Reprise My Stats page with artist portraits rankings and listening totals',
    filename: 'gnome-listening-stats.webp',
    width: 2400,
    height: 1456,
  }),
  capture({
    id: 'android-library',
    title: 'Android library',
    platform: 'Android',
    description:
      'A native compact list, fast search and a persistent mini-player for one-handed use.',
    alt: 'The Reprise Android library with album art search favourites and bottom navigation',
    filename: 'android-library.webp',
    width: 1080,
    height: 2404,
  }),
  capture({
    id: 'android-cover',
    title: 'Artwork mode',
    platform: 'Android',
    description:
      'The scene can recede behind the cover without changing transport or seek physics.',
    alt: 'Reprise Android Now Playing with album artwork colour fog and spectral seek',
    filename: 'android-cover.webp',
    width: 1080,
    height: 2404,
  }),
] as const;

function galleryCapture(id: string): ProductCapture {
  const capture = GALLERY_CAPTURES.find((candidate) => candidate.id === id);
  if (!capture) throw new Error(`Unknown gallery capture: ${id}`);
  return capture;
}

/** The five authored mosaic rows, in their lightbox navigation order. */
export const GALLERY_MOSAIC_ROWS: readonly (readonly ProductCapture[])[] = [
  [galleryCapture('gnome-podcasts'), galleryCapture('android-library')],
  [galleryCapture('gnome-youtube'), galleryCapture('gnome-radio')],
  [galleryCapture('gnome-library-doctor'), galleryCapture('android-cover')],
  [galleryCapture('gnome-device-sync'), galleryCapture('gnome-layout-controls')],
  [galleryCapture('gnome-listening-stats')],
] as const;

export const GALLERY_MOSAIC_CAPTURES: readonly ProductCapture[] = GALLERY_MOSAIC_ROWS.flat();

export function captureUrl(capture: ProductCapture): string {
  return `${import.meta.env.BASE_URL}media/showroom/${capture.filename}`;
}
