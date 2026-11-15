# Open, walkable tunnels (P4, owner request 2026-11-14)

Status: approved by the owner 2026-11-14 ("approved") after the short design in chat. It adjusts every tunnel, road and rail.

## Intent

A complete tunnel is a tunnel, not two doors onto a black wall. The player sees its stone-lined bore with vehicles driving through it and can walk the whole length in walk mode.

## Goals

- A complete tunnel (`hex_core::tunnels`) draws a bore under every cover cell, and its two portals open onto it: the black back wall of each portal is hidden, and the portal's lining runs to the cell edge.
- Cars and trains stay visible inside, riding the tunnel floor (the portal level), not the 4 m cover top.
- The walker enters at either arch and walks the corridor to the far portal.
- Cars and trains carry headlights (owner, 2026-11-14: they will need them for the day and night cycle). They light inside tunnels now and will light at night when P5 lands.

## Non-goals and anti-goals

- A lone portal looks as it does now: the black wall stays. The portal exteriors are unchanged (owner-approved looks).
- No real-time lights (the headlights are lens glow and a beam decal, not scene lights) and no sound changes (no echo or reverb). The orbit view is unchanged.
- No change to tunnel pairing, cover protection or save data.
- Anti-goal: a visible seam, gap or hole between portal lining, bore and floor at any tunnel length, including zero cover (adjacent portals).

## Constraints

- Road bore section = the road portal's opening (7.8 m wide, springing 0.6 m, crown 3.15 m). Rail bore section = the rail portal's (5.2 m opening, springing 2.4 m, crown 5.0 m). The bore floor carries the road section or the rail track and ballast at the portal level (floor y = portal level x 4.0).
- Each bore piece spans one cell edge to edge (27.7128 m) on the tunnel axis (the straight's edge 0 to edge 3 line), so a tunnel is the portals plus one bore per cover cell. A portal's lining is extended to its cell edge (road back plane 13.656 to 13.856, rail 11.456 to 13.856) with the back wall kept as its own slot at the back plane.
- Lamps are always lit inside (not driven by the P5 night factor), as baked glow or an always-on emissive slot. Light budget stays: no new real lights.
- Kit rules: headless Blender, new family files (never edit approved helpers), the owner approves each look in engine, pack `core` for the road bore and `village` for the rail bore, draw budget in the same slots as the portals.
- The walker's ground is a heightfield, so the bore corridor is a special case in `WalkGround`, not a second surface.

## Approaches considered

- A. Bore pieces per cover cell, portal back slot hidden while complete, bore corridor in the walker (chosen). Reuses the approved portal looks, adds two new pieces, and keeps lone portals unchanged.
- B. Separate open portal variants. Rejected: a second copy of each approved portal piece, and a tile respawn (a rise animation) every time a tunnel completes.
- C. One long procedural tube mesh built at runtime. Rejected: no stone look, no review path, and the hero look needs the Blender kit.

## Design

1. **Pieces.** `road_tunnel_bore` and `rail_tunnel_bore` (kind `prop`), plus the portal lining extension in `road_tunnel_portal` and `rail_tunnel_portal` (back wall stays a separate slot, named `<piece>_back`). The bore lining samples the portal lining's stone family so joints do not show.
2. **Runtime.** A play system spawns one bore entity per cover cell of every complete tunnel (rotation from the tunnel direction, y from the tunnel level) and despawns them when the tunnel changes. Portal back-wall parts are hidden while their tunnel is complete. Pieces wait for their pack like every other.
3. **Vehicles.** The tunnel fade goes: vehicles inside are opaque. Cars (`ride_car`) and trains (`level_y`) ride the portal level over cover cells.
4. **Walker.** `WalkGround` holds each complete tunnel's corridor (axis from face to face, half width of the arch); inside it the ground is the floor and outside it the old ground applies, so the cover top and the head wall still block.
5. **Headlights.** Runtime-built, no change to the approved vehicle pieces: each car kind and the engine get a lamp pair (small emissive lenses at the front corners from the piece's bake-report extent) and a soft additive beam on the road ahead, drawn as unlit blended meshes. The sim publishes a `lamps` level per vehicle (0 to 1, ramping on over the metres before an arch, 1 inside, ramping off after); the renderer glows the lamps and beam by it. P5's night factor will raise the same level, so the day and night cycle needs no new vehicle code. A train also lights its rear lamp pair red when it leads backwards (carriage end), a car its tail lamps dim.
6. **Evidence.** `docs/checkpoints/open_tunnels/`: native play shots of a road and a rail tunnel from the portal, mid-bore and with a vehicle, plus an eye-level walk view.

## Left to the builder

Bore lamp spacing and glow, the exact corridor margins, the system's set-up order against `sync`, and how the back-wall part is identified (slot suffix `_back`).
