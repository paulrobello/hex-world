# Contended-process samples, post-ARC-001 trace (2026-10-16)

The one-second `ps` dump taken beside `contended-trace.json` was replaced by this summary. It held 475
snapshots (11:38:18 to 11:46:41 AM PDT, 24 to 43 processes each) with the full command line of every
Chromium-family process on the host, which included machine paths. Nothing in the repo read it.

Summed CPU percent per process family across each snapshot (a core is 100%):

| Family | Role | Peak | Mean | Final snapshot |
| --- | --- | --- | --- | --- |
| Measurement-run Chromium (chromium-1243) | renderer | 381.6 | 140.8 | 0 (exited) |
| Measurement-run Chromium (chromium-1243) | GPU helper | 184.1 | 86.4 | 0 (exited) |
| Foreign Chromium (chromium-1248, the par-factory job) | renderer | 241.4 | 39.8 | 39.5 |
| Foreign Chromium (chromium-1248, the par-factory job) | GPU helper | 42.7 | 12.0 | 16.5 |
| Foreign Chromium (chromium-1248, the par-factory job) | browser | 67.9 | 1.9 | 0 |
| Installed Google Chrome (all helpers) | any | 1.8 | 0.1 | 0 |

The foreign GPU helper and renderer were still running at the end, which is the contention the README
describes. The measurement run's own Chromium exited before the last snapshot.
