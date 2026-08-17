import { HeroProduct } from '../showcase/HeroProduct';
import './chapters.css';

export function Hero() {
  return (
    <section className="hero" aria-labelledby="hero-heading">
      <div className="frame hero__copy">
        <p className="eyebrow">A music player for GNOME and Android</p>

        <h1 id="hero-heading" className="display hero__headline">
          Two native apps.
          <br />
          One Rust core.
        </h1>

        <p className="lead hero__lead">
          Built with AI agents. What gets merged is decided by the gates, not by the agent.
        </p>

        <p className="prose hero__note">
          The core carries no interface. That is not an architectural preference, it is the reason
          the second platform had a price tag instead of a rewrite — and the reason a third one would
          have a price tag too.
        </p>
      </div>

      <HeroProduct />
    </section>
  );
}
