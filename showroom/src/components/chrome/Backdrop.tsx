import './backdrop.css';

/** Static light and grain: the document stays still while the reader moves. */
export function Backdrop() {
  return <div className="backdrop-ground" aria-hidden="true" />;
}
