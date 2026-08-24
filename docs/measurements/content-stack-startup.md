# Content-stack startup measurement

Measured 2026-08-24 at base `7eaf16e4d3` plus B1.0 instrumentation. Each run
used the same 2,335-track real-library benchmark snapshot, copied into a fresh
`XDG_DATA_HOME`, with a fresh `XDG_CACHE_HOME` and `XDG_CONFIG_HOME`, private
D-Bus session, fresh Xvfb server, forced X11 backend, fake audio sink, and the
repository's smoke-quit hook. The live Reprise database was never opened.

The retained build artefacts identify the measured executable as the Cargo dev
profile (`target/debug/reprise`); no `target/release/reprise` artefact exists in
the measurement worktree.

Five cold runs produced the following construction and `GtkStack::add_named`
durations. Values are median milliseconds with `[min-max]` spread. Durations
come from env-gated tracing spans in `REPRISE_PERF_STARTUP_REPORT`, not from an
external process clock.

| Content page | Construct | `add_named` | B1.1 decision |
| --- | ---: | ---: | --- |
| Library | 0.105 [0.095-0.109] | 0.027 [0.023-0.033] | Keep eager; opening page |
| My Stats | 97.165 [91.049-105.206] | 0.013 [0.012-0.019] | Defer |
| Library Doctor | 0.231 [0.216-0.275] | 0.015 [0.013-0.024] | Keep eager |
| Concerts | 14.126 [13.772-29.158] | 0.036 [0.034-0.046] | Defer |
| Releases | 3.820 [3.301-4.914] | 0.011 [0.009-0.015] | Keep eager |
| Podcasts | 43.165 [32.519-63.119] | 0.040 [0.024-0.045] | Defer |
| YouTube | 36.581 [29.259-49.324] | 0.009 [0.006-0.024] | Defer |
| Radio | 24.834 [24.352-27.215] | 0.008 [0.004-0.011] | Defer |

Deferring five content-stack view constructors so they no longer run before
first paint was verified structurally: My Stats (97.2 ms), Podcasts (43.2 ms),
YouTube (36.6 ms), Radio (24.8 ms), and Concerts (14.1 ms) cost about 216 ms
together, and their construction spans are absent from all five post-change
startup reports. Library, Library Doctor, and Releases remain present in every
post-change report as intended.

## Wall-clock context

The five baseline activate-to-first-painted-frame values were 17,409, 36,506,
35,018, 14,748, and 40,269 ms: median 35,018 ms `[14,748-40,269]`. The five
post-change values were 23,537, 15,919, 16,865, 13,803, and 24,055 ms: median
16,865 ms `[13,803-24,055]`.

These two five-run samples overlap heavily, were not subjected to a statistical
test, and measure a same-harness total dominated by host Xvfb, portal and dconf
failures plus frame scheduling outside the measured view spans. The wall-clock
delta therefore must not be read as a halved user-facing cold start. It is
context for the structurally verified removal of about 216 ms of pre-paint
constructor work, not evidence that the constructors caused the total delta.
