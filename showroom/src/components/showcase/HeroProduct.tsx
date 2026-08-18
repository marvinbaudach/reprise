import { HERO_CAPTURES } from '../../data/showcase';
import { VisualizerPlate } from '../../visualizer/VisualizerPlate';
import { ProductShot } from './ProductShot';
import './showcase.css';

const [desktop, phone] = HERO_CAPTURES;

export function HeroProduct() {
  return (
    <figure className="hero-product" data-showcase="hero-product">
      <div className="hero-product__light" aria-hidden="true" />

      <div className="hero-product__desktop">
        <div className="capture-chrome data" aria-hidden="true">
          <span>GNOME · GTK4</span>
          <span>native desktop</span>
        </div>
        <ProductShot capture={desktop} eager />
      </div>

      <div className="hero-product__phone">
        <ProductShot capture={phone} eager />
        <VisualizerPlate />
      </div>

      <figcaption className="hero-product__caption frame">
        {HERO_CAPTURES.map((capture) => (
          <span className="hero-product__fact" key={capture.id}>
            <span className="eyebrow">{capture.platform}</span>
            <strong>{capture.title}</strong>
            <span className="data">{capture.description}</span>
          </span>
        ))}
      </figcaption>
    </figure>
  );
}
