import { PERFORMANCE, PERFORMANCE_PRICE } from '../../data/measurements';
import { ProductGallery } from '../showcase/ProductGallery';
import { SpectralAxis } from '../ui/SpectralAxis';
import './chapters.css';

export function ChapterThree() {
  return (
    <section id="ch-03" className="chapter" aria-labelledby="ch-03-heading">
      <div className="frame">
        <p className="rule eyebrow">CH.03</p>
        <h2 id="ch-03-heading" className="display chapter-title">
          Two frameworks. One visual signature.
        </h2>
      </div>

      <ProductGallery />

      <div className="stage">
        <div className="frame">
          <SpectralAxis />
        </div>
      </div>

      <div className="frame chapter__body">
        <p className="prose">
          <strong>The two apps look different on purpose.</strong> GNOME conventions on the desktop,
          Material on the phone. Making them match would not show craft, it would show missing
          platform UX. What is shared is the signature — and that is the harder half: two rendering
          stacks, GSK against Skia, two layout systems, two languages, the same visualisation and
          the same physics. Not a shared component. A shared specification.
        </p>

        <p className="prose">
          The seek bar is the case in point. The decision: show the structure of the track instead of
          an empty gutter. The implementation: a portable visuals layer that neither frontend owns.
          The result: physics that were measured afterwards rather than asserted.
        </p>

        <details className="fold">
          <summary className="fold__summary">
            <span className="eyebrow">Folded away</span>
            <span className="fold__title">What the title index cost, and what it bought</span>
          </summary>

          <div className="fold__body">
            <table className="ledger">
              <caption className="data ledger__caption">
                Measured over 100&apos;000 tracks. Before and after the index rebuild.
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

            <p className="prose ledger__price">{PERFORMANCE_PRICE}</p>
          </div>
        </details>
      </div>
    </section>
  );
}
