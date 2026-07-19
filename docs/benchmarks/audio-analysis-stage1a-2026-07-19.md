# Audio analysis Stage 1A benchmark — 2026-07-19

This is same-host evidence, not a portable performance promise.

- Host: Linux 6.18.38-1-MANJARO, x86_64
- Rust: 1.96.1
- GStreamer: 1.28.4
- Build: Cargo `release`
- Short input: repository-generated `sine.flac`, 51,200 mono samples at
  44.1 kHz
- Long input: test-generated 120-second mono PCM WAV at 8 kHz

Command:

```sh
env XDG_DATA_HOME=/tmp/reprise-audio-character-mcp-test-data \
  XDG_CACHE_HOME=/tmp/reprise-audio-character-mcp-test-cache \
  cargo test --locked --release -p reprise-platform-linux \
  release_profile_short_and_long_fixture_benchmark -- --ignored --nocapture
```

Measured inside the already-started test process, excluding long-fixture
generation:

| Input | Decode and analysis | Rate |
| --- | ---: | ---: |
| 1.16-second FLAC | 29 ms | — |
| 120-second WAV | 84 ms | 42 ms per audio minute |

The test process reported a 17,632 KiB peak RSS through `/proc/self/status`.
This includes the Rust test harness and loaded GStreamer plugins, not only the
accumulator. The hard memory evidence is structural: AppSink queues at most two
PCM buffers, each sample is consumed blockwise, and the Core accumulator's
buffer count is duration-independent. No source audio is written or uploaded.
