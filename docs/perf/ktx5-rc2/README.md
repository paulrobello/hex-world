# KTX-Software 5.0.0-rc2 measurement (2026-10-29, owner A50 O1)

Measurement only: nothing here ships. KTX-Software 5.0 is still a release candidate, and kit-compress stays pinned to 4.4.2 with the ASLR-off spawn until 5.0 is released (card 01a179379e46). The package was `KTX-Software-5.0.0-rc2-Darwin-arm64.pkg` from the KhronosGroup/KTX-Software GitHub release (sha256 `8fe97686…3978f`, signed by Developer ID Installer: The Khronos Group, Inc., notarized). It was expanded, not installed, into the main checkout's gitignored `tools/ktx-5.0.0-rc2/`. `ktx --version` reports `v5.0.0-rc2~3`.

## Determinism without the ASLR-off workaround

The known trigger is `scripts/assets/test-fixtures/river_fork_center.masks.png`. It was encoded with the kit-compress flags (`--encode uastc --uastc-quality 4 --uastc-rdo --uastc-rdo-l 0.5 --zstd 18 --generate-mipmap`), spawned normally with ASLR on, on macdaddy at load 47-67.

| encoder | runs | distinct files |
|---|---|---|
| 4.4.2, `--threads 1`, ASLR on (control) | 20 | 2 (17 × `2fe3e8e7…`, the pinned hash, and 3 × `556d255c…`) |
| 5.0.0-rc2, `--threads 1`, ASLR on | 20 | 1 (`4b17dc26…`) |
| 5.0.0-rc2, default threads, ASLR on | 10 | 1 (`0284e966…`) |

The struct-padding hash bug is fixed in rc2, so `scripts/assets/no-aslr.py` can go once 5.0 is pinned. Keep `--threads 1`: the multi-threaded RDO race in 4.4.2 showed only at load 100+, and these 10 runs were not taken at that load.

## Effect on the kit

`measure.mjs` re-encoded every map whose committed PNG is the full-resolution source with rc2: 391 of 395. The other 4 ship a reduced PNG fallback, and their full-resolution sources are not in the repo. It wrote the new BC7 and ASTC files over the committed ones in a throwaway worktree. Raw data: `results.json`.

- Bytes: all 391 maps change in both families. BC7 total 94,838,565 → 94,705,110 B (−0.14%). ASTC total 94,657,380 → 94,493,705 B (−0.17%).
- Loading: the engine loads and renders the rc2 files, since `make thumbs` exits 0 on them.
- Look: all 216 thumbnails change (`thumbs-diff.json`). PSNR against the committed thumbnails is 42.3 dB at the median and 35.9 dB at the worst (`rail_station-r4`). The largest single-channel difference is 49 of 255, measured after the thumbnails' lossy WebP q85 encode. `worst-thumbs.png` shows the five worst before (left) and after (right), and they read the same.

So the 5.x upgrade with owner A48 O1 is one byte change across the whole kit, which changes every thumbnail. It needs one thumbs pass and one owner look review, with the in-engine shots taken then.
