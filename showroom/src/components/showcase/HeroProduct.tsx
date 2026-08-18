import { HERO_CAPTURES } from '../../data/showcase';
import { VisualizerPlate } from '../../visualizer/VisualizerPlate';
import { ProductShot } from './ProductShot';
import './showcase.css';

const [desktop, phone] = HERO_CAPTURES;

export function HeroProduct() {
  return (
    <div className="hero-product" data-reveal="" data-showcase="hero-product">
      <button
        className="hero-shot hero-product__desktop"
        type="button"
        data-shot=""
        aria-label={`Open screenshot: ${desktop.title}`}
      >
        <ProductShot capture={desktop} eager />
        <span className="hero-shot__sweep" data-sweep="" aria-hidden="true">
          <span />
        </span>
        <span
          className="hero-shot__sheen hero-shot__sheen--desktop"
          data-sheen=""
          aria-hidden="true"
        />
        <span className="hero-shot__caption hero-shot__caption--desktop">
          <span className="hero-shot__platform" data-p="">
            {desktop.platform}
          </span>
          <span className="hero-shot__title" data-t="">
            {desktop.title}
          </span>
          <span className="hero-shot__description-wrap" data-dwrap="">
            <span className="hero-shot__description" data-d="">
              {desktop.description}
            </span>
          </span>
        </span>
      </button>

      <button
        className="hero-shot hero-product__phone"
        type="button"
        data-shot=""
        aria-label={`Open screenshot: ${phone.title}`}
      >
        <ProductShot capture={phone} eager />
        <VisualizerPlate />
        <span className="hero-shot__sweep" data-sweep="" aria-hidden="true">
          <span />
        </span>
        <span
          className="hero-shot__sheen hero-shot__sheen--phone"
          data-sheen=""
          aria-hidden="true"
        />
        <span className="hero-shot__caption hero-shot__caption--phone">
          <span className="hero-shot__platform" data-p="">
            {phone.platform}
          </span>
          <span className="hero-shot__title" data-t="">
            {phone.title}
          </span>
          <span className="hero-shot__description-wrap" data-dwrap="">
            <span className="hero-shot__description" data-d="">
              {phone.description}
            </span>
          </span>
        </span>
      </button>
    </div>
  );
}
