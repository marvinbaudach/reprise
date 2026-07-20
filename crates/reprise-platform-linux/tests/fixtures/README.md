# Audio codec fixtures

`sine.flac`, `blip.flac`, and `sine.mp3` contain generated test signals, not
recordings of third-party music. Their metadata identifies them as generated
waves; they were created for Reprise's codec tests and are distributed under
the repository's MIT license. They are evidence for decoder compatibility
only, not subjective mood or atmosphere labels. The MP3 fixture specifically
covers formats whose reported container duration is an estimate rather than
an exact decoded sample count.

WAV coverage is generated deterministically inside the Rust tests so no
additional binary fixture is required.
