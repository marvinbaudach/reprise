# B3 list-delta measurement

Measured on 2026-08-24 with 100-row GTK list views under the repository's
isolated display harness. Each probe selected row 75, scrolled away from the
top, changed row 0, and then repeated the identical snapshot.

| Model | One-row refresh | Selection | Viewport movement | Identical-refresh binds |
| --- | ---: | ---: | ---: | ---: |
| Podcasts | 1 changed range (`1` removed, `1` added) | row 75 retained | at most 1 px | 0 |
| Releases | 1 changed range (`1` removed, `1` added) | row 75 retained | at most 1 px | 0 |
| Concerts | 1 changed range (`1` removed, `1` added) | row 75 retained | at most 1 px | 0 |
| Radio | 1 changed range (`1` removed, `1` added) | row 75 retained | at most 1 px | 0 |

The former replacement shape emitted one full removal plus 100 individual
appends for the same fixture. The shared keyed reconciliation now retains
unchanged GTK objects. A separate signal test confirms that disjoint insertion
and removal ranges emit only their own positions. Radio's existing equality
gate remains in front of reconciliation as its cheapest no-op path.
