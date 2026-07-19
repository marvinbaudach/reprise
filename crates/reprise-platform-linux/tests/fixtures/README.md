# Audio codec fixtures

`sine.flac` and `blip.flac` contain generated test signals, not recordings of
third-party music. Their FLAC metadata identifies them as `audiotest` waves;
they were generated for Reprise's codec tests and are distributed under the
repository's MIT license. They are evidence for decoder compatibility only,
not subjective mood or atmosphere labels.

WAV coverage is generated deterministically inside the Rust tests so no
additional binary fixture is required.
