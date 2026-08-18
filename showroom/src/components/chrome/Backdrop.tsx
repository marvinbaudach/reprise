import './backdrop.css';

/**
 * Three fixed layers behind the page: the ground colour each chapter sets, a
 * slow drift of coloured light, and grain over both.
 *
 * All three are `position: fixed` and pointer-transparent, so they never enter
 * layout and never cost a scroll frame. The ground colour is the only one that
 * changes with the reading position — the choreography writes it, and the long
 * transition here is what turns a chapter change into a mood change instead of
 * a flash.
 */
export function Backdrop() {
  return (
    <>
      <div id="backdrop-ground" className="backdrop-ground" aria-hidden="true" />
      <div id="backdrop-oil" className="backdrop-oil" aria-hidden="true">
        <span className="backdrop-oil__blob backdrop-oil__blob--teal" />
        <span className="backdrop-oil__blob backdrop-oil__blob--coral" />
        <span className="backdrop-oil__blob backdrop-oil__blob--violet" />
        <span className="backdrop-oil__sweep" />
      </div>
      <div className="backdrop-grain" aria-hidden="true" />
    </>
  );
}
