import { TIMELINE } from 'virtual:build-timeline';
import { permalink } from '../../data/measurements';
import './chapters.css';

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/**
 * The display form of a week: two ISO days in, `11–17 Jul` out. A week whose
 * ends fall in different months names both, `28 Jul – 3 Aug`.
 *
 * Computed rather than written down, so the record keeps one spelling of each
 * date and the display form cannot drift from it. That is also why no date
 * appears in this file, not even as an example.
 */
export function displaySpan(from: string, to: string): string {
  // The build guarantees ISO days, and this reads them back rather than casting
  // that guarantee into place: a cast would turn a weakened guarantee into a
  // silently wrong date, which is the one thing this whole feature is against.
  const parts = (day: string): { month: number; date: number } => {
    const [, month, date] = day.split('-').map(Number);
    if (month === undefined || date === undefined || !MONTHS[month - 1]) {
      throw new Error(`the timeline gave "${day}", which is not an ISO day`);
    }
    return { month, date };
  };
  const start = parts(from);
  const end = parts(to);
  const name = (month: number) => MONTHS[month - 1] ?? '';
  return start.month === end.month
    ? `${start.date}–${end.date} ${name(end.month)}`
    : `${start.date} ${name(start.month)} – ${end.date} ${name(end.month)}`;
}

/**
 * The tempo band between the hero and the first chapter: the weeks it took, one
 * card each, on a rail.
 *
 * It sits on its own ground colour and between two hairlines, so it reads as a
 * caesura rather than as a chapter of its own. Neither the weeks nor their
 * number are written here — both come from `docs/showroom/timeline.md`, so the
 * headline figure cannot end up saying something the rail does not show.
 */
export function TempoBand() {
  return (
    <section className="tempo" data-ground="oklch(14% 0.03 195)">
      <div className="tempo__frame frame">
        <div className="tempo__head">
          <div>
            <p className="eyebrow tempo__label" data-reveal>
              Idea to alpha
            </p>
            <p className="tempo__figure" data-reveal>
              <span data-counter>{TIMELINE.length}</span>
              <span className="tempo__unit">weeks</span>
            </p>
          </div>
          <p className="tempo__note" data-reveal>
            Counted from the first commit — a design document, not a line of product code — to a
            running alpha on all four frontends. The weeks are the record in{' '}
            <a href={permalink('docs/showroom/timeline.md')}>timeline.md</a>, and the number beside
            it is how many rows it has.
          </p>
        </div>

        <ol className="tempo__track" data-reveal data-weeks={TIMELINE.length}>
          {TIMELINE.map((week) => (
            <li className="tempo__week" key={week.week} data-week={week.week}>
              <p className="data tempo__span">{displaySpan(week.from, week.to)}</p>
              <p className="tempo__theme">{week.theme}</p>
              <p className="tempo__landed">{week.landed}</p>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}
