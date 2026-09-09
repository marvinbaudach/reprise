import { ChapterFive } from './components/chapters/ChapterFive';
import { ChapterFour } from './components/chapters/ChapterFour';
import { ChapterOne } from './components/chapters/ChapterOne';
import { ChapterThree } from './components/chapters/ChapterThree';
import { ChapterTwo } from './components/chapters/ChapterTwo';
import { Hero } from './components/chapters/Hero';
import { ProjectHighlights } from './components/chapters/ProjectHighlights';
import { SiteFooter } from './components/chapters/SiteFooter';
import { Backdrop } from './components/chrome/Backdrop';
import { ScrollProgress } from './components/chrome/ScrollProgress';
import { SiteHeader } from './components/chrome/SiteHeader';
import { ShowreelFilm } from './components/showcase/ShowreelFilm';
import { usePageChoreography } from './hooks/usePageChoreography';
import { useReducedMotion } from './hooks/useReducedMotion';
import './styles/global.css';
import './styles/portfolio.css';

export function App() {
  const reduced = useReducedMotion();
  usePageChoreography(reduced);

  return (
    <div id="showroom-root" className="page">
      <a className="skip-link" href="#main-content">
        Skip to the showroom
      </a>
      <Backdrop />
      <ScrollProgress />
      <SiteHeader />
      <main id="main-content" tabIndex={-1}>
        <Hero />
        <ProjectHighlights />
        <ShowreelFilm />
        <ChapterOne />
        <ChapterFive />
        <ChapterTwo />
        <ChapterThree reducedMotion={reduced} />
        <ChapterFour />
      </main>
      <SiteFooter />
    </div>
  );
}
