import './rhythmbox.css';

const FLATPAKREF_URL = 'https://marvinbaudach.github.io/reprise-flatpak/reprise.flatpakref';

export function ComingFromRhythmbox() {
  return (
    <section id="rhythmbox" className="frame rhythmbox" aria-labelledby="rhythmbox-heading">
      <header className="film-heading">
        <h2 id="rhythmbox-heading">Coming from Rhythmbox?</h2>
        <p>
          Reprise imports ratings, play counts and last played times, date added, and playlists.
          Start the import on first run or later from Preferences.
        </p>
      </header>
      <p>
        The Flatpak can access your XDG music directory. Music elsewhere needs a filesystem override
        before import.
      </p>
      <p>
        <a href={FLATPAKREF_URL}>Install Reprise from its Flatpak repository</a>
      </p>
    </section>
  );
}
