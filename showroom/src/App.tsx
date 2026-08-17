import { ChapterOne } from './components/chapters/ChapterOne';
import { ChapterThree } from './components/chapters/ChapterThree';
import { ChapterTwo } from './components/chapters/ChapterTwo';
import { Hero } from './components/chapters/Hero';
import { SiteFooter } from './components/chapters/SiteFooter';
import { ScrollHairline } from './components/chrome/ScrollHairline';
import { SiteHeader } from './components/chrome/SiteHeader';
import './styles/global.css';

export function App() {
  return (
    <>
      <a className="skip-link" href="#main-content">
        Skip to the showroom
      </a>
      <div className="texture" aria-hidden="true" />
      <ScrollHairline />
      <div className="page">
        <SiteHeader />
        <main id="main-content" tabIndex={-1}>
          <Hero />
          <ChapterOne />
          <ChapterTwo />
          <ChapterThree />
        </main>
        <SiteFooter />
      </div>
    </>
  );
}
