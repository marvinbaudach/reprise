import { BASELINE, treelink } from '../../data/measurements';
import './chapters.css';

export function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="frame">
        <p className="rule eyebrow">Where the figures come from</p>

        <p className="prose site-footer__honesty">
          Every figure on this page was counted on{' '}
          <a href={treelink()}>
            <code className="data">dev@{BASELINE.commit}</code>
          </a>{' '}
          with cloc 2.08, product and test code separated by an AST pass over every Rust file, and
          verified a second time by hand on {BASELINE.countedOn}.{' '}
          <strong>This build does not measure them yet</strong> — the figures are typed into one
          module and the measurement runs in CI next. Until it does, that sentence stays here
          instead of a claim that would be nicer and untrue.
        </p>

        <p className="data site-footer__links">
          <a href={BASELINE.repository}>Source</a>
          <span aria-hidden="true"> · </span>
          <a href={`${BASELINE.repository}/blob/${BASELINE.commit}/docs/ux-rules.md`}>Rulebook</a>
          <span aria-hidden="true"> · </span>
          <a href={`${BASELINE.repository}/blob/${BASELINE.commit}/TESTING.md`}>How it is tested</a>
        </p>
      </div>
    </footer>
  );
}
