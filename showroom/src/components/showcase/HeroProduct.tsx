import { useCallback, useState } from 'react';
import { HERO_CAPTURES } from '../../data/showcase';
import { VisualizerPlate } from '../../visualizer/VisualizerPlate';
import { Lightbox } from './Lightbox';
import { ShotTile } from './ShotTile';
import './showcase.css';

const [desktop, phone] = HERO_CAPTURES;

export function HeroProduct() {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [returnFocus, setReturnFocus] = useState<HTMLButtonElement | null>(null);
  const close = useCallback(() => setActiveIndex(null), []);
  const next = useCallback(
    () =>
      setActiveIndex((current) => (current === null ? null : (current + 1) % HERO_CAPTURES.length)),
    [],
  );
  const previous = useCallback(
    () =>
      setActiveIndex((current) =>
        current === null ? null : (current - 1 + HERO_CAPTURES.length) % HERO_CAPTURES.length,
      ),
    [],
  );
  const open = (index: number, trigger: HTMLButtonElement) => {
    setReturnFocus(trigger);
    setActiveIndex(index);
  };

  return (
    <div className="hero-product" data-reveal="" data-showcase="hero-product">
      <ShotTile
        className="hero-product__desktop"
        capture={desktop}
        eager
        variant="desktop"
        onOpen={(trigger) => open(0, trigger)}
      />

      <ShotTile
        className="hero-product__phone"
        capture={phone}
        eager
        variant="phone"
        onOpen={(trigger) => open(1, trigger)}
      >
        <VisualizerPlate />
      </ShotTile>

      {activeIndex !== null && (
        <Lightbox
          activeIndex={activeIndex}
          captures={HERO_CAPTURES}
          returnFocus={returnFocus}
          onClose={close}
          onNext={next}
          onPrevious={previous}
        />
      )}
    </div>
  );
}
