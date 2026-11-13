# hill_meadow and peak_granite: integration notes

Status: built ahead, pending the owner. The references are unpicked (spec Q1), so no reference
approval exists (R2) and `kit-provenance` cannot run. Nothing here is an owner approval. Spec:
`docs/superpowers/specs/2026-11-13-single-tile-hills-design.md` (rulings D1-D19, questions Q1-Q4).

## Delivery

- **Committed on the agent branch:** the family helper `scripts/blender/kit/_hills.py`, the
  references under `assets-source/refs/{hill_meadow,peak_granite}/`, the spec, and this folder
  plus `docs/checkpoints/peak_granite/` (review sheets, the review renderer, the walker-part
  measure, the in-engine shot script).
- **`integration.patch` (text only, this folder):** both thin generators, the two manifest entries,
  the two tile files, each piece's `approvals.json` (a pending entry), `friction.md` and its three
  build reports, the runtime tables, their tests, the HUD names, the regenerated bridge schema and
  rules parity, and the PRD lines. Apply it with `git apply docs/checkpoints/hill_meadow/integration.patch`
  on the agent branch's head.
- **Binary artifacts, uncommitted in the agent worktree** (the orphan gate refuses them without a
  manifest entry, and a manifest entry without provenance fails the provenance gate): listed with
  their SHA-256 under "Artifacts". Copy them in as built, or rebuild them with `make kit-build` and
  `make kit-compress` once the references are approved. `kit-determinism` passed twice per piece,
  so the GLBs rebuild byte for byte. Both were built from the final `_hills.py`
  (`8694babe1fb3…`); the hill's rebuild after the peak's last helper edit came out byte-identical.

After the owner picks references: write each `assets-source/refs/<id>/approval.json`, apply the
patch, copy or rebuild the artifacts, run `make kit-provenance PIECE=<id> REFS=assets-source/refs/<id>`
per piece (it re-renders the six views and stamps validation), run `make thumbs`, and empty
`AWAITING_THUMBNAILS` in `crates/hex-core/tests/data_validator.rs` (the thumbnail test fails
until then, because the manifest lists the two tiles it skips).

## Patch

`integration.patch` (SHA-256 `1fe7dba141118a0fc71fa47fcb7219da82a60fd1c79bac58eec876a1817de9b9`,
34 files, no binary hunks) was written by `git diff` from the working tree the gates ran on.
Verified: `git apply --check` against this branch's head (on a scratch index of `HEAD`), applied
there, and the result equals the gated working tree for every patched path (`git diff` empty);
`git apply --check -R` passes on the working tree. `web/ui/test/rules-parity.json` and
`web/ui/src/bridge-schema.json` are `make ui-schema` output: on a newer main, rerun `make ui-schema`
rather than resolving conflicts in them by hand.

## Artifacts (`assets/kit/p1/<id>/`, SHA-256)

Staged GLB (pre-meshopt, the determinism hash): hill_meadow `3f568986b75647145cc40e6e41dca9381a4cb39507043df1a96fd2b101c84f6d`,
peak_granite `ce3c03bf4b1f5d748ab4add88bde5348a4960ca61d8c76d1bb42c49a5140f0ef`.

```
966473b2945a95a88d8074cc451d716704c7de70e4f10c6b9ec4d38674e28bde  hill_meadow.glb
b1d8e39699be8d60c10954f54bdc4db05acd1fb73a59c7a992918f2ca3458ad0  hill_meadow.albedo.bc7.ktx2
f90971095a2fd3e4d1900eb9922f4a628585485ee64cf3a701cba7b2422542cd  hill_meadow.albedo.astc.ktx2
e0db4062addf2f825b920352c1bd7eaea3fc56fffbd1a9621db6f478ede530fc  hill_meadow.albedo.png
c33de2044afd748a643cfbe157b36e2201489c587a03ca3944ab4e84e16f01da  hill_meadow.normal.bc7.ktx2
88b4db447cbc5bd717128b5deb8f8eba475c67f42a8c2a92e6bf8b46d85e9c45  hill_meadow.normal.astc.ktx2
9a060b1ea906f6f1c4fe32e44326803886996aa9000d2ee6c6dc1c91374e26b4  hill_meadow.normal.png
2773f43074abc2021b41888b2dc775d0b66a3e5c46138e5176a7d861fa4df0b4  hill_meadow.orm.bc7.ktx2
cce4c8f87e94fc3bcb69efed35cac7f4af31279e7d1a464fcf8137c9e0932747  hill_meadow.orm.astc.ktx2
87e50a2fa52ad75fbb479a34fb10cf8feca54a0853c2cf88f7143fa3607bf6f5  hill_meadow.orm.png
0e7935c1d6609c27a2e268d7a9f17d1d5fce992ab3dea8698d89e5561bac2b04  hill_meadow.masks.bc7.ktx2
74f3fd3bc6a0e410bcfc23c6b16b6d04412d589b2ae7d56dd52098b31bb6ddea  hill_meadow.masks.astc.ktx2
bc26d74086cd58167bce36203a4f9f02c3c7d36a83c527f991db8c41c78cb0a8  hill_meadow.masks.png
e53859d8c02d62828b081cba8b57de5bd8c4aa209c64f9a84be0eb9d83a61b08  peak_granite.glb
fa1d18ffd2f6a0377266ed0c5640382fe0c9ee61bbc6475c0a5019db7ea30844  peak_granite.albedo.bc7.ktx2
c96a651ffcac51cdaecbab57c23eba06b3ef2ee6e43f874e3b82d8d4060db958  peak_granite.albedo.astc.ktx2
b1f3388aa8968a21a8fd4adc0b078bff343a521c460a8eebf87c45ccb0583d19  peak_granite.albedo.png
eeddcb97b600552eac26c60fcd3e83c15f733c32595fd81e1e84bab4810e11e4  peak_granite.normal.bc7.ktx2
9e5a680f295768be059242d1fdcec344b670e8f445179e3b54b37d8aa2059c68  peak_granite.normal.astc.ktx2
6bfc3826a3ab71798dad02af26524fe803d8c931cbe25d54967853ba8a8f5d1a  peak_granite.normal.png
41d86af48cd261aba071443d3bc3ec103cdb0b437522cdd6f2ee4f0b1a4b2e04  peak_granite.orm.bc7.ktx2
3f293912d7a05bc1639a0e1c0f2b6e7ae8af800fcc71e6e5bb852d526475d312  peak_granite.orm.astc.ktx2
d25e5ad09fcfc8912459d19e31992a2a2d97ea11acb316f690d0ef92558e1bd6  peak_granite.orm.png
4e7d54dac16d71302f4955ff85e7d0e0f0776b5cbce698506051ac29bf1f1d00  peak_granite.masks.bc7.ktx2
3b55cee9f920b6a21d76ced39c2b278d0dbb781f74db1a629bb152ee4539c4d8  peak_granite.masks.astc.ktx2
d0695f76a3fbac15c2f88d9ef49c7ac1a4850a11b0c090b704164789c45c3c71  peak_granite.masks.png
```

The six-view renders under `assets-source/kit/<id>/renders/` are rewritten by `kit-provenance`.

## Numbers

| | hill_meadow | peak_granite |
|---|---|---|
| LOD0 / LOD1 / LOD2 tris | 3684 / 1758 (47.7%) / 486 (13.2%) | 3525 / 1705 (48.4%) / 507 (14.4%) |
| LOD0 slots (cap, rock, scree, skirt) | 2950, 464, 78, 192 | 1493, 1713, 127, 192 |
| Top (LOD0) | 9.44 m | 17.68 m |
| Rock texel, median px/m (LOD0 / LOD1 / LOD2) | 54.65 / 54.18 / 15.46 | 24.52 / 24.17 / 5.93 |
| Scree texel, median px/m (LOD0 / LOD1 / LOD2) | 54.36 / 39.79 / 15.46 | 24.52 / 30.34 / 5.91 |
| Scoped texel floor (D8) | 50 px/m | 20 px/m |
| Visible rock / scree, top-down | 57 m² in 4 slabs / 9 m² | 195 m² / 21 m² |
| GLB (meshopt) | 163,084 B | 216,168 B |
| KTX2 family, BC7 / ASTC (class piece, 1.5 MB) | 882,996 B / 901,535 B | 1,235,302 B / 1,241,627 B |
| Steepest cap or scree triangle (walker limit 4) | gradient 1.26 | gradient 1.53 |
| Ground 0.6 m inside the border | under 0.25 m | under 0.10 m |

The LOD2 rock texel is low by design: LOD2 draws from 400 m, where a 16 m hex covers a few dozen
pixels. No slot leaves LOD2 (`lod2Drops` is empty for both).

## Gates (exit code, tail)

Blender gates ran on the final pieces; the hill was re-run after its rebuild.

- `make kit-build PIECE=hill_meadow`: 0, reports copied to `assets-source/kit/hill_meadow/`.
  `PIECE=peak_granite`: 0. (Skip-bake drafts first, both 0.)
- `make kit-compress`: hill 0 (`glb 163084 B, ktx bc7 882996 B, astc 901535 B`), peak 0
  (`glb 216168 B, ktx bc7 1235302 B, astc 1241627 B`).
- `make kit-determinism`: hill 0 and 0 (`deterministic 3f568986…`), peak 0 and 0
  (`deterministic ce3c03bf…`).
- `make kit-renders`: hill 0, peak 0.
- `make kit-provenance PIECE=<id> REFS=assets-source/refs/<id>` (the `--check` step only): 1 for
  both, `approval.json missing; references are not owner-approved (R2)`. This is expected until Q1.
- `make kit-check` / `cargo test -p hex-play --test kit_gate`: 72 passed, 1 failed, 2 ignored. The
  failure is `provenance_complete_and_hashes_match` (`provenance missing` for both pieces, R2).
- `cargo test --workspace --no-fail-fast`: every target green except that same kit_gate test
  (hex-play lib 361 passed, hex-core 244, hex-app 204, hex-render 119, the rest green).
  `data_validator`: 15 passed, `every_tile_rotation_has_a_thumbnail` included, which skips only the
  two new tiles until `make thumbs` (`AWAITING_THUMBNAILS`).
- `make fmt-check`: 0. `make typecheck` (native and wasm32): 0. `make ui-schema`: 0 (bridge schema
  and rules parity regenerated). `make ui-check`: 0 (243 of 243 UI tests). `make shots-test`: 0.
- `make lint`: 2 on this branch's base, in `crates/hex-play/src/play/coalesce.rs`
  (`needless_range_loop`, Rust 1.100), a file this work does not touch. Main fixed it in
  `af7c4be`. With that commit applied to the working tree, `make lint` exited 0, and it was then
  reverted.
- `make web-size`, `make initial-download`, `make check-wgpu`: 0, see "Web budgets". With the lint
  leg's base caveat, every `checkall-ci` leg ran, and only the kit gate's provenance test is red
  (R2).
- `make ui-e2e`: 0 (ui, ui-motion, photo-fx, persist, saves-ui, audio, debug and version, 625
  checks, none failed; tail `version e2e OK`), on GPU Chromium at a host load of 143-195. It
  drives walk mode on the default world, which has no landform; the landforms' walker ground and
  rock are covered by `walker_climbs_the_landforms_to_their_rock`.

## Web budgets

Run on this worktree's `make build-web` with both pieces' artifacts in `assets/kit/p1/`:

- `make web-size`: 0, `wasm raw=28880232 gzip=9084737` (budget 12 MiB gzip).
- `make initial-download`: 0, `PASS: initial download 23,862,719 B (22.76 MiB, family astc) vs
  budget 31,457,280 B (30.00 MiB)`. The pieces are in the `hero` pack, which streams: 30 files,
  26.01 MiB on the wire per BC7 family with them (their share is about 1.1 MB for the hill and
  1.45 MB for the peak). Core and the initial download are unchanged.
- `make check-wgpu`: 0, `single wgpu OK: wgpu v30.0.1`.

## Integration items

| Item | State | Where |
|---|---|---|
| Car height | not applicable | No lanes (D16): a road facing a hill ends as at any laneless edge. |
| Walker ground | handled | `slope_ground::tile_ground_tris` reads the LOD0 cap and scree, unchanged. `walker_climbs_the_landforms_to_their_rock`: ground equals the piece's within 2 cm at three turns, the border at the meadows' level, no refused step on 12 bearings. |
| Walker colliders | handled | `walk::FOOTPRINT_OBSTACLES` rows `HILL_ROCK_FOOT` (4 hulls, 62 m²) and `PEAK_ROCK_FOOT` (18 fans, 206 m²), measured top-down on LOD0 by `walk_parts.py`. Walks stop at the rock 4.6-8.5 m up on the hill and 1.1-5.1 m up on the peak. |
| `prop_kind` | handled, no row | The obstacle part id is the tile id; `prop_kind` falls through to `rock`. |
| `walk::OBSTACLES`, `walk::CAR_GROUND` | not applicable | `OBSTACLES` lists placed props whose collider blocks; a ground piece's rock blocks through `FOOTPRINT_OBSTACLES`, as the massif's (it has no `OBSTACLES` row). `CAR_GROUND` lists road-surface slots; the landforms carry no road. |
| `scatter::FOOTPRINTS`, `VARIANTS`, `GROUND_BOXES` | not applicable | Keyed by the props scatter places (their reach, variants and box colliders). The landforms are ground pieces that scatter stands on, not props, and the massif has no row either. |
| `scatter::NOT_GROUND` | not applicable | For ground-piece slots that stand on the ground (crop cards, the portal's ferns). The cap, rock and scree are ground; the rock and scree are kept clear by `excluded_slot`. |
| `variant::HERO_VARIANTS` | not applicable | No hero slots on either tile. |
| Scatter | handled | Caps join `floor_slots`; rock and scree are excluded (`excluded_slot`); `LANDFORMS` scales Meadow's rules to Meadow density per m² of free ground (`meadow_share`); peak pines in `PEAK_PINE_BAND` 8.5-12.5 m. The outcrop rule finds no free ground on the peak and plans none. Tests: `landform_scatter_matches_meadow_density`, `peak_pines_stand_on_the_lower_slopes`. |
| Grass | handled | `<id>_cap` is a grass slot; `grass.rs` slot list test updated. |
| Boid perches and sky band | not applicable | D11: no perches baked. The band lifts with the highest placed level, so a landform's top stays under it at any level; `boids::plan`'s peak lift (`MASSIF_PEAK`) is for mega tiles only and the landforms need none. `landform_tops_are_the_lod0_tops_under_the_sky_band` pins both tops 3 m under `SKY_LOW`. |
| Sim nodes | not applicable | No lanes, no sim nodes (D16). `no_tunnel_runs_under_a_single_tile_hill` keeps portals open across them. |
| Picking and orbit | handled | `LANDFORM_TOPS` in `hex_play::play`, used by `game.rs::top_of`; `pick_into_a_single_tile_peak_takes_the_peak`. |
| Thumbnails | needs the orchestrator | 12 images (6 turns per tile, D5): run `make thumbs`, then empty `AWAITING_THUMBNAILS`. `landform_summits_fit_the_shared_pose` pins both summits inside the shared pose. |
| Draw budget | handled | Four slots per tile at each LOD (cap, rock, scree, skirt), as the massif's, under the base kind's 4000 LOD0 tris. Meadow draws two (cap, skirt). |
| Sound | handled | Biome `Grass`: Meadow's ambience and stinger (`audio.test.ts` maps both to the meadow family). |
| Kit showcase | handled | `kit_scene` places both beside the massif; `tile_counts` 40 to 42. |
| Pack | handled, Q3 | `hero`, as the massif (D6). Core is untouched. |
| Saves | not applicable | No format change (D15). |

## Captures for the orchestrator

- Thumbnails: `make thumbs` (12 new images, `hill_meadow` and `peak_granite`, six turns each).
- In-engine shots (book a load-sensitive window):
  `caffeinate -d -u make shots-piece SCRIPT=docs/checkpoints/hill_meadow/hills-shots.json OUT=docs/checkpoints/hill_meadow ARGS='--scenario play --clock fixed --water_clock fixed'`.
  It lays a road row, a forest, a hill and a peak at level 0, a raised hill (level 1) and a raised
  peak (level 2) among meadows, and takes hills-overview, hill-play, peak-play, hill-low, peak-low,
  hill-seam-low, peak-road-low, hill-raised-low, peak-raised-low and hills-raised-play, plus a
  `dump_state`.

## Known differences from ref-1

- The peak's granite reads rounder than ref-1's: the massif's slab facets and joints are carved,
  but at a 16 m hex the pinnacles show as rounded blocks, and the lichen and moss tint reads as
  blotches on the lower rock in the EEVEE stills. This is spec Q4 (O1, recommended: judge it in the
  in-engine shots first; O2: a jointing pass now, with a rebuild, new walker rows and tops, the
  gates and a new patch).
- The hill's dome fills the hex: the crown is 8.5 m of grass (D2, the brief's 8-10 m), so the
  slope starts at the rim rather than on a flat Meadow apron as ref-1 suggests.
