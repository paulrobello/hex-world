# Rail tunnel bore: the section does not fit under one level of cover (decided: O1)

Ruling: the owner picked O1, the turf ridge over the bore (chat, 2026-11-14, relayed by the coordinator),
and accepted a ridge in the orbit view over complete rail tunnels (Q2). The coordinator ruled Q4: no tiles
on the cover cells, and it clears runtime scatter and grass in the ridge corridor. The built pieces and
their numbers are in `integration.md`; the rest of this file is the analysis as it stood before the
ruling.

Status 2026-11-15 (before the ruling): blocked before the look. The approved rail opening and the approved one-level cover
cannot both hold once the tunnel is open. Nothing was landed in the manifest, `kit_scene.rs`, the portal
or `data/`. `git diff --stat 170094d` over both portal generators, `_tunnel_portal.py`, `_rail.py`, the
portal's artifacts, the manifest, `crates/` and `data/` is empty, so the portal is still the approved
1,491 / 735 / 207 tris. The gate stub (`gate-findings.md`) shows that the bore itself builds and that no
kit gate other than the two provenance checks fails on it.

Measured with `section_probe.py`, which writes `section-probe.json` from `_tunnel_portal_rail` and the
committed train clearance record. The sheet is `section.jpg` (`section_sheet.py`).

## The conflict

The spec's rule "Rail bore section = the rail portal's (5.2 m opening, springing 2.4 m, crown 5.0 m)"
meets the rail-at-levels rule "A rail tunnel runs under one level of raised Meadow", which puts the
cover's ground at 4.0 m.

| Quantity | Value |
|---|---|
| Crown over the cover ground (4.0 m) | +1.00 m |
| Crown over a meadow relief dip (3.75 m) | +1.25 m |
| Width of the vault above the cover ground | \|s\| < 2.049 m (2.222 m at a dip) |
| Engine top 4.66 m | 0.66 m over the cover, \|y\| up to 1.41 m |
| Carriage B top 4.41 m | +0.41 m |
| Carriage top 4.18 m | +0.18 m |
| Approved portal mound at s = 0 | 5.70 m at the back plane (x 11.456), 4.0 m at edge 0 |
| Mound under the crown | from x 12.236 (under crown + 0.3 m lining cover from x 11.918, under the extrados from x 11.754) |

What follows:

1. An open bore under a cover cell is a vault that stands 1.0 m out of the meadow. Its lining is
   single-sided, so from outside the near half is culled and the inside face of the far half shows above
   the grass. Trains at full size stick out of the ground beside it.
2. The interior extension of (b) breaks out of the approved mound for its last 1.6 m (x 12.24 to
   13.86). A lone portal would show the far inner wall and the open end at edge 0 unless that part is
   hidden while the tunnel is incomplete.
3. Cover-cell grass, scatter and trees rooted at 4.0 m would stand inside the vault.
4. Every option also needs the runtime to cut the cover cells' cliff columns and skirts where the bore
   crosses a cell edge. These are the soil walls `play.rs` `spawn_column` draws down to level 0, and
   they face outward, so a neighbour's wall faces into the bore and closes it. This is read from the
   code and has not been rendered.

The road half is not affected. The road crown is 3.15 m, which leaves 0.85 m under the cover (0.60 m at
a dip).

## Options

| | Change | Spec line it breaks | Extra slots and runtime | Notes |
|---|---|---|---|---|
| **O1 (recommended)** | Cut-and-cover turf ridge over the bore. Extrados 5.45 m plus 0.3 m turf gives a 5.75 m crest, 1.75 m over the cover, easing back to 4.0 m by \|s\| = 7 m. Inside the cell, so neighbours meet it at 4.0 m. | "The orbit view is unchanged": a ridge line shows over complete rail tunnels only. | Bore: turf slot (grass tileable) and the ridge, an estimated +300 LOD0 tris (not built). Portal: complete-only slots (extension lining, plus a ridge turf from about x 11.5 to edge 0). Runtime hides `_back` while complete and shows the complete-only slots only then. Cover grass and scatter cleared inside \|s\| < 7. Walker collider over the ridge. Columns and skirts cut. | Section, clearance, lamps and seams unchanged. Trains stay visible inside. Reads as an embankment. |
| O2 | Lower bore: springing about 1.05 m, crown about 3.65 m (at least 0.10 m under the 3.75 m dip). | "Cars and trains stay visible inside": the 4.66 m engine cannot enter, so trains fade at the face as today. | Portal: the extension must taper from the 5.0 m opening to 3.65 m, or a stepped inner headwall with the smaller opening at X_B. Columns and skirts cut. | Exterior and orbit view unchanged. Walkers fit. |
| O3 | Rail cover two levels (8 m). | The approved portal mound (RL-Q1 O2) is redone, so the portal needs re-approval. A gameplay rule change. | `hex_core::tunnel` cover level for rail. Columns and skirts cut. | Section and trains unchanged. Steeper terrain. |
| O4 (not recommended) | Keep the section and clip vehicles above the cover while the camera is outside. | "a visible seam, gap or hole": the far-wall band and the vault stay visible from outside. | Shader clip plane on trains. Grass and scatter cleared. | Only hides the trains. |

## Lamps (settled under every option)

Use the piece's emissive map on a unique `glass` slot. `build_material` binds `emissive_texture` with
`emissive = WHITE`. Bevy 0.20.1 adds `emissive.rgb * mix(1, exposure, emissive.a)`, and the kit leaves
the exposure weight at 0, so the lamp light is added after exposure. It is always on and is not gated by
the P5 night factor. At the kit exposure (EV100 12.9) a 1.0 amber emissive is about 5 times sunlit grass
(about 0.2 before tone mapping), and Bloom::NATURAL is on.

Light pools baked into the albedo were rejected. Albedo only reflects ambient light, which is close to
zero in a shadowed bore, so the pools would barely show. Masks B also carries the lamps so P5 night can
brighten them later.

No manifest piece has used an emissive map before. The stub shows that `kit-compress` encodes it
(BC7/ASTC sRGB, 9 levels) and `kit-check` accepts it, but it has not been seen in the engine. The first
build of the look needs one in-engine capture during a booked load-sensitive window.

## Questions for the owner

- Q1: Which option, O1 (ridge), O2 (lower bore), O3 (two-level cover) or another?
- Q2: Under O1, may the orbit view show a turf ridge along complete rail tunnels only (lone portals
  unchanged)?
- Q3: Under O2, are trains that fade at the portal face acceptable for rail while road vehicles stay
  visible inside?
- Q4: Under O1, may the player place tiles or props on the cover cells over a rail tunnel? If so, they
  collide with the ridge and need a placement rule.
