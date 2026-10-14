# P1 texture budget probe

Measured 2026-10-13 for the owner, before the meadow kit was built. It answers whether the D2 texel densities fit the provisional D3 budget of 1,500,000 B of KTX2 per piece per family. The owner has since ruled on the budgets (see Owner decision below).

## Answer

D2 and D3 cannot both hold for the tree. They also cannot both hold for a tileable set at the 2 m tile that a 1024² map gives at 512 px/m.

- **Base unique bake at 32 px/m.** This is a 1024² `masks` map, and it fits easily: 136-592 KB, which is 0.10-0.39× the budget.
- **Tileable sets at 512 px/m.** A 1024² set (2 m tile) is 2.9-3.4 MB, which is 1.9-2.3× the budget. A 512² set (1 m tile) is about 0.85 MB and fits.
- **Tree at 256 px/m.** Four 2048² maps come to 9.0-11.0 MB, which is 6-7× the budget. Every fallback that keeps any 1024² map fails as well. Only an all-512² tree fits, at 0.98 MB.
- **RDO lambda does not close the gap.** Raising `--uastc-rdo-l` from 0.5 to 4 saves 11% on albedo and 3.5% on normal. The size is set by resolution and content.

## Owner decision (2026-10-13): raise to full detail

The D2 densities stand. The single 1.5 MB budget is replaced with per-class KTX2 budgets per family (`KTX_BUDGETS` in `scripts/assets/kit-lib.mjs`). `kit_manifest.rs` mirrors the same three numbers (S33).

| Class | KTX2 bytes per family | Covers |
|---|---|---|
| `piece` (default) | 1,500,000 | a piece's unique maps, such as a base's 1k masks |
| `tileable` | 3,600,000 | one tileable set at 1024² (2 m tile at 512 px/m) |
| `hero` | 11,500,000 | a full-detail hero piece, such as the tree at 4 × 2048² |

Hero pieces and their tileable sets ship in streamed biome packs. The 30 MB initial download keeps only the WASM, the core kit and the UI. The headroom arithmetic under option 1 below is therefore moot.

## Method

- **Pipeline.** Every map goes through `encodeMap` in `scripts/assets/kit-compress.mjs`, the shipping path. That is UASTC quality 4 with RDO (lambda 0.5, or 0.25 for normal), zstd 18 and a full mip chain, transcoded to BC7 and ASTC 4x4 and zstd'd again. sRGB or linear follows `CHANNELS` in `kit-lib.mjs`. The tool is KTX-Software 4.4.2.
- **Content.** The maps are built from the owner-approved reference JPEGs (`assets-source/refs/{meadow_base,road_straight,tree_hero_oak}/ref-1.jpg`, all 2400×1792).
  - The crops are taken at native resolution and never upscaled. 1024² and 512² are centre crops. 2048² is a 2048×1792 centre crop, mirror-extended by 256 rows.
  - **albedo.** The RGB of the crop. The tree's albedo has a hard leaf-card alpha cutout.
  - **normal.** A Sobel filter over the luminance height (box blur r=1, strength 4).
  - **orm.** R is AO (blurred luminance), but 1.0 for tileable sets, as `bake_tileable_set` writes it. G is roughness (high-pass luminance). B is metallic, 0.
  - **masks.** R snow and G wetness come from blurred luminance. B night is 0. A is AO.
- **Bias.** Maps derived from a photo are noisier than a real high-to-low bake, so the main table is pessimistic. Two "as-baked" variants bracket that bias.
  - `normal_smooth` blurs the height with r=3.
  - `masks_baked` uses constant R, G and B and a smooth AO, which is what a real base's hook masks look like.
- **Bytes.** All values are exact file bytes. The two families never differ by more than about 5%.

Reproduce with:

```sh
make fetch-tools && (cd scripts/assets && npm ci --no-audit --no-fund)
node scripts/assets/kit-texture-probe.mjs                   # maps (slow: 2048² UASTC q4 + RDO), writes target/kit-probe/probe-maps.json
node scripts/assets/kit-texture-probe.mjs --part variants   # as-baked normal and masks
node scripts/assets/kit-texture-probe.mjs --part levers     # RDO lambda sweep (needs maps first)
```

The P0 baseline is a simple procedural 1024² albedo at UASTC quality 2 without RDO. It gives `hex_test.bc7.ktx2` = 594,157 B and `hex_test.astc.ktx2` = 590,420 B.

## Per map (BC7 / ASTC bytes)

| Source | Size | albedo | normal | orm | masks |
|---|---|---|---|---|---|
| meadow | 2048 | 3,281,227 / 3,265,331 | 4,396,389 / 4,361,470 | 2,382,589 / 2,416,086 | 1,142,328 / 1,185,897 |
| road | 2048 | 2,958,042 / 2,994,937 | 4,222,884 / 4,241,954 | 2,128,439 / 2,149,241 | 858,252 / 901,165 |
| tree | 2048 | 2,877,072 / 2,755,389 | 4,172,773 / 4,137,176 | 2,716,389 / 2,736,870 | 1,203,478 / 1,243,665 |
| meadow | 1024 | 1,225,519 / 1,210,160 | 1,306,728 / 1,239,213 | 858,054 / 867,820 | 577,882 / 591,706 |
| road | 1024 | 1,010,714 / 1,016,934 | 1,195,464 / 1,175,457 | 709,753 / 714,409 | 421,068 / 440,683 |
| tree | 1024 | 947,519 / 952,879 | 1,175,110 / 1,137,176 | 853,876 / 883,201 | 499,827 / 512,834 |
| meadow | 512 | 313,384 / 307,288 | 335,312 / 315,204 | 226,373 / 228,780 | 202,196 / 201,326 |
| road | 512 | 293,343 / 295,149 | 316,545 / 312,498 | 213,781 / 215,054 | 146,624 / 150,264 |
| tree | 512 | 264,789 / 270,599 | 314,574 / 305,791 | 233,482 / 241,617 | 166,472 / 169,596 |

The as-baked variants (tree source) measure as follows.

| Variant | 2048 | 1024 |
|---|---|---|
| `normal_smooth` | 3,175,505 / 3,180,350 | 1,029,797 / 1,017,929 |
| `masks_baked` | 262,496 / 268,294 | 136,227 / 143,355 |

BC7 and ASTC 4x4 cost a fixed 1 B/px, which is 1.33 B per base-level pixel with the mip chain. After zstd, detailed albedo, normal and ORM maps cost 0.90-1.25 B per base pixel at 1024² (normals are the worst, at 1.12-1.25) and 0.82-1.28 at 512². At 2048² the range is 0.51-1.05, because the mirror-extended rows repeat content. zstd therefore recovers only about 6-32% at 1024². Masks cost 0.20-0.77 B per base pixel.

## Per piece and per set against 1,500,000 B per family

| Option (D2) | Maps | BC7 | ASTC | Fits | × budget |
|---|---|---|---|---|---|
| Base unique bake, 32 px/m (meadow) | masks 1024 | 577,882 | 591,706 | yes | 0.39 |
| Base unique bake, 32 px/m (road) | masks 1024 | 421,068 | 440,683 | yes | 0.29 |
| Base masks, as-baked | masks_baked 1024 | 136,227 | 143,355 | yes | 0.10 |
| Base masks + 1k macro albedo | masks + albedo 1024 | 1,803,401 | 1,801,866 | **no** | 1.20 |
| Tileable set, 2 m tile (meadow) | albedo, normal, orm 1024 | 3,390,301 | 3,317,193 | **no** | 2.26 |
| Tileable set, 2 m tile (road) | albedo, normal, orm 1024 | 2,915,931 | 2,906,800 | **no** | 1.94 |
| Tileable set, 1 m tile (meadow) | albedo, normal, orm 512 | 875,069 | 851,272 | yes | 0.58 |
| Tileable set, 1 m tile (road) | albedo, normal, orm 512 | 823,669 | 822,701 | yes | 0.55 |
| Tileable set, 4 m tile (meadow) | albedo, normal, orm 2048 | 10,060,205 | 10,042,887 | **no** | 6.71 |
| Tree 256 px/m, all 2048 | albedo, normal, orm, masks 2048 | 10,969,712 | 10,873,100 | **no** | 7.31 |
| Tree, all 2048, as-baked normal and masks | as above with variants | 9,031,462 | 8,940,903 | **no** | 6.02 |
| Tree, T10 fallback | albedo 2048; normal, orm 1024; masks 512 | 5,072,530 | 4,945,362 | **no** | 3.38 |
| Tree, all 1024 | albedo, normal, orm, masks 1024 | 3,476,332 | 3,486,090 | **no** | 2.32 |
| Tree, C1 option | albedo, normal 1024; orm, masks 512 | 2,522,583 | 2,501,268 | **no** | 1.68 |
| Tree, C1 option, as-baked normal, no masks | albedo, normal_smooth 1024; orm 512 | 2,210,798 | 2,212,425 | **no** | 1.47 |
| Tree, all 512 | albedo, normal, orm, masks 512 | 979,317 | 987,603 | yes | 0.66 |

## RDO lambda lever (meadow 1024)

| Lambda | albedo BC7 / ASTC | normal BC7 / ASTC |
|---|---|---|
| 0.25 | 1,229,258 / 1,213,287 | 1,306,728 / 1,239,213 |
| 0.5 (albedo default) | 1,225,519 / 1,210,160 | 1,301,945 / 1,235,351 |
| 1 | 1,214,943 / 1,201,984 | 1,293,935 / 1,228,257 |
| 2 | 1,183,419 / 1,176,434 | 1,281,378 / 1,217,081 |
| 4 | 1,091,063 / 1,095,027 | 1,257,566 / 1,194,449 |

RDO barely helps even before the transcode. On the same albedo, the zstd'd UASTC intermediate is 1,325,390 B without RDO, 1,310,681 B at lambda 0.5 and 1,169,977 B at lambda 4, which is the same 11% as after the transcode. Shipping UASTC and transcoding at runtime is therefore not a budget lever either. At lambda 0.5 the UASTC file is 7% larger than the BC7 one.

## Options for the owner

These are inputs to the owner's decision. Nothing here has been applied.

1. **Raise D3 per class.** Measured needs are about 0.6 MB for a base piece's unique maps, about 3.4 MB for a 1024² tileable set and about 11 MB for a 2048² tree. PRD §10 caps the initial download at 30 MB for WASM, core kit and UI together. The WASM alone is 8,462,064 B gzip (measured by `make checkall-ci`, 2026-10-13). Two 1024² sets (about 6.3 MB) plus one 2048² tree (about 11 MB) would leave about 4.2 MB for every other piece's GLB and maps and for the UI.
2. **Keep 1.5 MB and lower resolution.** Use tileable sets at 512² (a 1 m tile at 512 px/m) and the tree at all 512². Over the same surface, a 512² atlas has a quarter of the linear density of a 2048² one, so a tree that needs 2048² for 256 px/m drops to 64 px/m. The tree's real surface area sets the actual density, which this probe does not measure.
3. **Mixed.** Use 1024² tileable sets on a separate, larger budget (they are shared across every base, so their cost is paid once). Keep 1.5 MB for unique piece maps, and give the tree an explicit exception.

Two more levers are unmeasured here. BC5 for normals is two-channel, but Bevy's reconstruction of BC5 normals is unverified. Direct ASTC encodes at a larger block size such as 6x6 (0.89 B/px) would need their own quality check.
