import { useCallback, useState } from 'react';
import { GALLERY_MOSAIC_CAPTURES, GALLERY_MOSAIC_ROWS } from '../../data/showcase';
import { Lightbox } from './Lightbox';
import { ShotTile } from './ShotTile';
import './showcase.css';

export function ProductGallery() {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [returnFocus, setReturnFocus] = useState<HTMLButtonElement | null>(null);
  const close = useCallback(() => setActiveIndex(null), []);
  const next = useCallback(
    () =>
      setActiveIndex((current) =>
        current === null ? null : (current + 1) % GALLERY_MOSAIC_CAPTURES.length,
      ),
    [],
  );
  const previous = useCallback(
    () =>
      setActiveIndex((current) =>
        current === null
          ? null
          : (current - 1 + GALLERY_MOSAIC_CAPTURES.length) % GALLERY_MOSAIC_CAPTURES.length,
      ),
    [],
  );
  const open = (id: string, trigger: HTMLButtonElement) => {
    const index = GALLERY_MOSAIC_CAPTURES.findIndex((capture) => capture.id === id);
    if (index < 0) return;
    setReturnFocus(trigger);
    setActiveIndex(index);
  };

  return (
    <>
      <div className="frame mosaic-frame">
        <header className="mosaic-heading">
          <h3 id="mosaic-heading" data-reveal>
            Two platforms. Every view, tab and dialogue.
          </h3>
          <p data-reveal>Click any plate to enlarge</p>
        </header>

        <section
          className="mosaic"
          data-showcase="product-gallery"
          data-layout="design-mosaic"
          aria-labelledby="mosaic-heading"
        >
          {GALLERY_MOSAIC_ROWS.map((row) => (
            <div className="mosaic-row" key={row.map((capture) => capture.id).join('-')}>
              {row.map((capture) => (
                <ShotTile
                  className={`mosaic-tile mosaic-tile--${capture.id}`}
                  capture={capture}
                  reveal="img"
                  variant={capture.platform === 'Android' ? 'phone' : 'desktop'}
                  onOpen={(trigger) => open(capture.id, trigger)}
                  key={capture.id}
                />
              ))}
            </div>
          ))}
        </section>
      </div>

      {activeIndex !== null && (
        <Lightbox
          activeIndex={activeIndex}
          captures={GALLERY_MOSAIC_CAPTURES}
          returnFocus={returnFocus}
          onClose={close}
          onNext={next}
          onPrevious={previous}
        />
      )}
    </>
  );
}
