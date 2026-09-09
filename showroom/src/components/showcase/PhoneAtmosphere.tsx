import { useEffect, useRef } from 'react';
import { observeSceneActivity } from '../../visualizer/activity';
import './phone-atmosphere.css';

export function PhoneAtmosphere() {
  const atmosphereRef = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const atmosphere = atmosphereRef.current;
    if (!atmosphere) return;
    return observeSceneActivity(atmosphere, (active) => {
      atmosphere.dataset.active = String(active);
    });
  }, []);

  return (
    <span ref={atmosphereRef} className="phone-atmosphere" data-active="false" aria-hidden="true">
      <span className="phone-atmosphere__oil" />
    </span>
  );
}
