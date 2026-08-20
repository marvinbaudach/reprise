import {
  PERFORMANCE,
  PERFORMANCE_PRICE,
  PERFORMANCE_RECORD,
  permalink,
} from '../../data/measurements';
import './ChapterFive.css';

export function ChapterFive() {
  return (
    <section
      id="ch-05"
      className="chapter-five"
      data-chapter="05"
      data-ground="oklch(12.5% 0.018 24)"
      aria-labelledby="ch-05-heading"
    >
      <div className="chapter-five__frame">
        <header className="chapter-five__heading" data-reveal>
          <p>CH.05</p>
          <h2 id="ch-05-heading">Measured afterwards. Price attached.</h2>
          <p>
            The title index was rebuilt for one reason: the track list was too slow over a large
            library. What it bought was measured after the fact rather than asserted before it — and
            what it cost sits in the same table, not in the small print.
          </p>
        </header>

        <div className="ledger-card" data-reveal>
          <header className="ledger-card__heading">
            <span>The ledger</span>
            <span>What the title index cost, and what it bought</span>
          </header>
          <div className="ledger-card__body">
            <table className="ledger">
              <caption>
                Measured over 100&apos;000 tracks, before and after the index rebuild. Quoted from{' '}
                <a href={permalink(PERFORMANCE_RECORD)}>the record</a>, which carries the commit,
                the date and the method behind every row.
              </caption>
              <thead>
                <tr>
                  <th scope="col">What</th>
                  <th scope="col">Before</th>
                  <th scope="col">After</th>
                  <th scope="col">Delta</th>
                </tr>
              </thead>
              <tbody>
                {PERFORMANCE.map((row) => (
                  <tr key={row.what}>
                    <th scope="row">{row.what}</th>
                    <td className="ledger__before">{row.before}</td>
                    <td className="ledger__after">{row.after}</td>
                    <td className="ledger__delta">{row.delta}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="ledger__price">{PERFORMANCE_PRICE}</p>
          </div>
        </div>
      </div>
    </section>
  );
}
