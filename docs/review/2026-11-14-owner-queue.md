# Owner review queue, 2026-11-14

Requests waiting on you. Nothing here is approved unless it says answered. Paths are relative to the repo root, on branch `rail-levels-p1`. Every piece is committed with a `pending` approval entry. The in-engine shots come from the booked window of 2026-11-15 2:30 AM PST (`make shots-open-tunnels`, `make shots-raised-rail`) and go in `docs/checkpoints/open_tunnels/` and `docs/checkpoints/rail-levels/raised/`.

## In-engine results, 2026-11-15 (booked window)

Frames are committed: open tunnels in `docs/checkpoints/open_tunnels/` (`rail-*`, `road-*`: overview, mouth, side, inside, inside-back with vehicles, walk), raised rail in `docs/checkpoints/rail-levels/raised/`, viaduct stills in `docs/checkpoints/rail_viaduct/`. The window ran at host load 11 to 65 because other work shared the host, so these are visual evidence only (clocks fixed), not timing.

- Verified in engine: both bores draw under their cover, the portals open into them, the far end shows through, lamps glow, the walker stands on the bore floor and looks down the tunnel, a train and a car ride through, a road car's headlights and road beam light up inside the tunnel, no soil plug stands in either bore, and the turf ridge covers the rail bore. Raised rail: the level-1 loop carries two trains on the rail top. Viaducts: all three heights and their seams draw as designed.
- Found and fixed: the first shot layout never closed the rail tunnel (the far portal took the first legal turn, facing away), so the first frames showed solid ground. The script now rotates it, and `play.tunnels` in the state dump reports each complete tunnel. The walk shots faced the wall because `walk_to` headings run clockwise from -Z.
- Tuned: the headlight beam was a hard-edged tan patch. It now ramps up from nothing over its first quarter and peaks at 0.3 instead of 0.5.
- Not verified: a train's headlights head-on. The one train that came back toward the camera inside the bore did so caboose-first (the sim reverses a train at a dead end without turning its consist), so the locomotive's lenses never faced the camera. Cars are verified.
- Looks to judge: the rail bore lining reads dark with only the lamp boxes lit (RT-Q8), and the bore floor is darker than the road pieces at the mouth (RT-Q4).

## RT. Rail tunnel portal and open tunnels

**Answered 2026-11-14:** "approved" for the portal look (`assets-source/kit/rail_tunnel_portal/approvals.json`), and "approved" for open tunnels (spec `docs/superpowers/specs/2026-11-14-open-tunnels-design.md`). Headlights added on your request the same day. Rail bore: **turf ridge chosen** (RB O1) over a two-level cover or a lowered bore.

Built, all gated, waiting on your look in engine:
- `rail_tunnel_bore`, the rail portal's open interior (`_open` slots and the `_back` slots hidden while a tunnel is complete): sheet `docs/checkpoints/rail_tunnel_bore/sheet.jpg`.
- `road_tunnel_bore` and the road portal's open interior: sheet `docs/checkpoints/road_tunnel_bore/sheet.jpg`.
- Vehicle headlights (runtime lens glow and a road beam, no scene lights), lit inside tunnels now and by the night factor later.
- The walker walks a complete tunnel on its floor, cars and trains ride it, and nothing grows or stands on the rail ridge.

- **RT-Q1. Portal LOD0 budget.** The open rail portal is 1,739 triangles at LOD0, over the 1,500 prop cap. I granted a scoped 1,800 for `rail_tunnel_portal` only (the other route was a separate prop). Reply if you want the separate prop instead.
- **RT-Q2. Dark lining band at the road bore's joint.** The road portal's flat dark lining slot shows as a dark ring about 1 m long where the bore meets the portal (1.75 m at LOD2). Hiding that slot while the tunnel is complete removes it, at the cost of a small bore change. Recommendation: hide it, after you see it in engine.
- **RT-Q3. A lone road portal from behind** now shows a dark arch patch where the stone back used to be. A cover cell normally hides it. Recommendation: accept.
- **RT-Q4. Floor brightness step at the road mouth.** The bore floor reads darker than the road pieces (masks A 0.30, pools 0.85, against about 1). Recommendation: judge in engine.
- **RT-Q5. Lamp brightness.** Emissive is capped at 1.0 and reads well in the kit shots. A brighter lamp needs a manifest emissive-strength factor (a runtime change). Recommendation: leave it.
- **RT-Q6. Grass verge inside the road tunnel.** The bore follows the exact road section, grass verge included. Keep the grass, or swap it for gravel? Recommendation: gravel.
- **RT-Q7. Mixed LODs inside one tunnel** leave a 6 to 16 cm gap at the springings where neighbours switch (about 1 px at 150 m). I did not pin one LOD per tunnel. Recommendation: accept, revisit if it shows in engine.
- **RT-Q8. Interior ambient light.** The rail bore's lining takes the same ambient light as the portal face and is darkened only by sun shadow. Recommendation: accept, or ask for a darker interior bake.

## VD. Rail viaducts (`rail_viaduct_l1`, `_l2`, `_l3`)

Built: sheet `docs/checkpoints/rail_viaduct/sheet.jpg`, the three heights `heights.jpg`, 15 play stills (captured at host load 26 to 37, above the 15 guard, so they are re-shot in the booked window). One design, pier height scaled by the deck (RL-Q4), no walking on the deck (RL-Q3).

- **VD-Q1. Deck ferns and drip staining.** ref-4 shows ferns on the coping and rain staining. They are missing. Add a pass? Recommendation: yes, a small one.
- **VD-Q2. Deck 1's arches** spring at 0.5 m and read as a row of culverts. Keep? Recommendation: keep.
- **VD-Q3. Pier walker obstacle kind.** The piers read as `rock` to the walker. A `pier` kind touches Rust and the UI. Recommendation: not now.
- **VD-Q4. One shared map set** in the manifest instead of three byte-identical sets? Recommendation: yes, with the next kit-compress.
- A viaduct is placed only between two rail tiles, and its HUD refusal no longer crashes the Tiles sheet (fixed on this branch).

## RR. Rail on raised ground

Built: rail straight, both bends and the junction are raisable, a raised rail edge meets only raised rail at its own level, and trains ride the level of their rail. Evidence shots (`docs/checkpoints/rail-levels/raised/`) come from the booked window. Not raisable (by design, RL): the station, the level crossing and the river rail bridge.

- **RR-Q1. Is that the right set to leave on the ground?** Recommendation: yes for now.

## KT. KTX-Software 5.0

Not released as of 2026-11-14 (newest is v5.0.0-rc2 of 2026-08-17). Card 01a179379e46 stays blocked on the release. No question for you.
