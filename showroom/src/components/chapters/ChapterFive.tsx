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
          <p>Performance</p>
          <h2 id="ch-05-heading">A faster library. A measured trade-off.</h2>
          <p>
            A slow title-list query led me to rebuild its database index. The measurements below
            show the before and after, alongside the extra storage it needs.
          </p>
        </header>

        <div className="ledger-card" data-reveal>
          <header className="ledger-card__heading">
            <span>The ledger</span>
            <span>What the title index cost, and what it bought</span>
          </header>
          <div className="ledger-card__body">
            {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
            <table className="ledger" role="table">
              <caption>
                Measured over 100&apos;000 tracks, before and after the index rebuild. Quoted from{' '}
                <a href={permalink(PERFORMANCE_RECORD)}>the record</a>, which carries the commit,
                the date and the method behind every row.
              </caption>
              {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
              <thead role="rowgroup">
                {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                <tr role="row">
                  {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                  <th scope="col" role="columnheader">
                    What
                  </th>
                  {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                  <th scope="col" role="columnheader">
                    Before
                  </th>
                  {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                  <th scope="col" role="columnheader">
                    After
                  </th>
                  {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                  <th scope="col" role="columnheader">
                    Delta
                  </th>
                </tr>
              </thead>
              {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
              <tbody role="rowgroup">
                {PERFORMANCE.map((row) => (
                  // biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver.
                  <tr key={row.what} role="row">
                    {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                    <th scope="row" role="rowheader">
                      {row.what}
                    </th>
                    {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                    <td className="ledger__before" role="cell" data-label="Before">
                      {row.before}
                    </td>
                    {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                    <td className="ledger__after" role="cell" data-label="After">
                      {row.after}
                    </td>
                    {/* biome-ignore lint/a11y/noRedundantRoles: Mobile display changes require explicit table semantics in Safari and VoiceOver. */}
                    <td className="ledger__delta" role="cell" data-label="Delta">
                      {row.delta}
                    </td>
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
