import './chrome.css';

const CHAPTERS: readonly { id: string; label: string }[] = [
  { id: 'ch-01', label: '01 Core' },
  { id: 'ch-02', label: '02 Gates' },
  { id: 'ch-03', label: '03 Signature' },
  { id: 'ch-04', label: '04 Headless' },
];

const MARK = `${import.meta.env.BASE_URL}brand/reprise-mark.svg`;

/**
 * The header is transparent over the hero and only takes on a surface once the
 * page has moved — the choreography sets `data-lifted`. The wordmark is set in
 * Archivo rather than pulled from the brand lockup: the lockup's own wordmark is
 * Fraunces on `currentColor`, which as an `<img>` renders black on this ground.
 */
export function SiteHeader() {
  return (
    <header id="site-header" className="site-header" data-lifted="false">
      <a className="site-header__id" href="#hero">
        <img className="site-header__mark" src={MARK} alt="" width={24} height={24} />
        <span className="site-header__wordmark">Reprise</span>
        <span className="site-header__state">Alpha</span>
      </a>

      <nav className="site-header__nav" aria-label="Chapters">
        {CHAPTERS.map((chapter) => (
          <a key={chapter.id} data-navlink data-current="false" href={`#${chapter.id}`}>
            {chapter.label}
          </a>
        ))}
        <a className="site-header__source" href="https://github.com/marvinbaudach/reprise">
          Source ↗
        </a>
      </nav>
    </header>
  );
}
