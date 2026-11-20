import { HeroProduct } from '../showcase/HeroProduct';
import './chapters.css';

export function Hero() {
  return (
    <section
      id="rp-top"
      className="hero"
      aria-labelledby="hero-heading"
      data-ground="oklch(13% 0.014 269)"
      data-showcase="design-hero"
    >
      <div className="hero__grid">
        <div className="hero__copy">
          <h1 id="hero-heading" className="hero__headline">
            A music player for GNOME and Android, built on one Rust core.
          </h1>
          <p className="hero__lead">
            Your whole library on the desktop and in your pocket, native on both and synced between
            them.
          </p>
          <p className="hero__note">
            Designed and built by <strong>Marvin Baudach</strong>.<br />
            Product design, architecture and quality by one engineer, AI-assisted.
          </p>
          <div className="hero__actions">
            <a className="action action--primary" href="#film">
              <span aria-hidden="true">▶</span> Watch the film{' '}
              <span className="action__duration">0:58</span>
            </a>
            <a className="action action--text" href="#ch-01">
              Explore the engineering <span aria-hidden="true">↓</span>
            </a>
          </div>
        </div>
        <HeroProduct />
      </div>
      <div className="hero__byline frame">
        <span>Rust · GTK4 · Kotlin · Compose</span>
      </div>
    </section>
  );
}
