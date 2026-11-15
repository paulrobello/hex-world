# road_tunnel_bore and the open road_tunnel_portal: integration notes

Spec: `docs/superpowers/specs/2026-11-14-open-tunnels-design.md` (runtime sections 2 to 4 are the orchestrator's).
Both pieces are pending owner review (`approvals.json`), with the sheet at `docs/checkpoints/road_tunnel_bore/sheet.jpg`.

## Frames and placement

- **Bore frame.** Origin at the cell centre. The axis runs along +x from -13.8564 to +13.8564 (edge 3 to edge 0 of a
  straight), and h = 0 is the tunnel level's surface. The GLB (Blender x, s, h) loads as runtime (x, h, -s), the same
  as the portal.
- **Where bores go.** Spawn one bore per cover cell of a complete tunnel, centred on the cell and at the portal's
  level (y = level x 4.0).
- **Yaw.** Every bore in a tunnel takes the yaw of the tunnel's straight axis. The bore's section is symmetric, but
  the stone texture's girth direction flips under a 180 degree turn. A tunnel that mixes yaws then shows mirrored
  courses meeting at a cell edge.
- **Ends.** Both ends are the same section, which equals the portal lining's section at X_E = 13.8564. Bores tile end
  to end, and a bore meets either portal.

## Checklist

| Item | State | Notes |
|---|---|---|
| Back-wall slot and node | handled (asset side), runtime is needs-work | Slot `road_tunnel_portal_back` (already in the manifest). Each LOD mesh (`road_tunnel_portal_lod0..2`) carries it as one material primitive, not as its own node. That primitive holds two parts: the original fan at the back plane X_B = 13.656 (closes the lining from the front) and a two-triangle patch at X_E - 0.01 = 13.846 (closes the cut X_E face from behind). Hide that primitive on both portals while their tunnel is complete. Lone portals keep it. |
| Bore draw count and pack | handled | Pack `core` (loads before READY). Five slots: `road_tunnel_bore_stone` (unique: lining, lamps, shell) plus the road pieces' own `road_asphalt`, `road_verge`, `road_kerb` and `road_paint` definitions. That is at most 5 primitives per bore LOD. The textures are albedo 1024, normal, ORM and masks at 512, and emissive at 256. The stone slot binds all five. The emissive map is the first any kit piece uses, through StandardMaterial's existing emissive binding (`kit_material.rs` sets the factor to WHITE). It worked in the kit-scenario shots. The 16-texture `make web-smoke` clamp was not run here. |
| Floor height | handled | The floor is the road_straight section at the tunnel level: road 0.08 at abs(t) = 3.0, rising to the 0.11 crown. Kerb top is 0.20 for 3.0 <= abs(t) <= 3.25. Verge is 0.06 out to the lining at abs(t) = 3.9. Lane paint follows the road pieces' dash spans. Car heights from the road network need no change. `CAR_GROUND` already lists `road_asphalt` and `road_paint`, though bores are not tile pieces, so `ride_max` never sees them. |
| Walker corridor numbers | needs-work (runtime) | See "Corridor" below. The bore collider is a box over the whole tube (from the LOD2 shell). Do not add `road_tunnel_bore` to `OBSTACLES` or `FOOTPRINT_OBSTACLES`. Use the `WalkGround` corridor. |
| Scatter and grass (GROUND_BOXES, NOT_GROUND) | n/a | Bores are spawned by a play system, not drawn as tile pieces, so tile surfaces, scatter and grass never see them, and they need no row. The open portal is a `GROUND_PROPS` entry. Its new triangles are vertical (the X_E cut face and the back patch, which the surface drops) or sit under the turf top (the lining from 13.656 to 13.856), so the heightfield the walker and scatter use is unchanged. `cargo test -p hex-play -p hex-render --lib -- boids scatter walk kit_scene` is green. |
| Thumbnails | none for the bore (not a tile) | The portal tile's closed exterior is unchanged under back-face culling. Only its back (X_E) face changed, from panel cells to cut stone plus the back patch. `make thumbs` was not run here. A capture that sees the wall's back would dirty only `road_tunnel_portal`'s thumbnails. |
| Sound | none | |

## Corridor (walker), tunnel frame

- **Length.** The corridor axis runs from portal A's face plane to portal B's face plane, each X_F = 11.656 m from its
  own cell centre toward the tunnel. The jamb stones stand about 0.04 m proud of the face. With n cover cells the
  face-to-face length is 27.7128 (n + 1) - 23.312 m, which is 4.40 m for adjacent portals.
- **Section.** The section is the same through both linings and every bore. The half width is 3.9 m (the intrados)
  from the floor to the springing at 0.6 m. Above that, the half width follows the semi-ellipse
  `3.9 * sqrt(1 - ((h - 0.6) / 2.55)^2)`, which is 3.36 m at 1.9 m and 0 at the 3.15 m crown.
- **Walker margins.** With `BODY_RADIUS` 0.3, the walker centre stays within 3.6 m of the axis at foot level. The
  kerbs (0.12 over the road) are under `MAX_STEP` 0.4. The verge strip from 3.25 to 3.9 is walkable.
- **Floor.** The floor heights are the ones in the "Floor height" row above, plus the level's y.

## Needs-work for the runtime (found while building the pieces)

1. **Cell-edge columns.** The cover cells' cliff columns and soil skirts at cell edges cross the tube. Suppress them
   along the tunnel line, or they read as walls inside the bore. This is expected but unverified: no runtime builds a
   complete open tunnel yet. The EEVEE stand-in drops the skirts.
2. **Portal collider.** The portal's `_collider` box spans the opening. Anything that collides against piece
   colliders sees the arch as closed.
3. **Walker obstacles.** The bore's `_collider` is one box over the tube, so it must never become a walker obstacle.
4. **Cross-LOD joints.**
   - **The rule.** Every LOD's end section equals the same LOD of the portal lining. LOD0 uses 30 intrados segments,
     LOD1 uses 9 and LOD2 uses 7.
   - **The gap.** Two neighbours drawn at different LODs (the kit chunks switch at about 150 m and 400 m,
     `kit_instances.rs` bands) meet with a gap between the polylines. The gap is largest near the springings, where
     the coarser chord sags about 0.10 m (LOD0 against LOD1) or 0.16 m (LOD0 against LOD2). The culled lining and
     shell show nothing through it.
   - **Likely impact.** At 150 m that sliver is about one pixel, and it can be seen only through a portal mouth.
   - **Fixes, if it shows.** Draw a tunnel's pieces at one LOD, or add an end-cap annulus between lining and shell at
     each bore end, about 2 x 30 triangles at LOD0.
5. **The portal lining's dark band (owner question).** The open portal keeps its lining slot
   (`road_tunnel_portal_lining`, flat 0.06) from the barrel's end to X_E, so it reads as a dark ring at the
   bore-portal joint: about 1.0 m long at LOD0 and LOD1 and 1.75 m at LOD2 (`bore-seam_bp.jpg`).
   - **The conflict.** "Closed look unchanged through the arch" keeps the band. "No visible seam" wants it gone.
   - **A runtime option that leaves the closed look alone.** Hide or restyle the `_lining` primitive while the
     tunnel is complete, as with `_back`. The bore's lining would then need to start at the barrel's end (x 12.856
     in the portal frame), which is a bore change.
   - **An asset option.** Re-skin that stretch with the bore stone, which changes the closed look seen through the
     arch.

## Runtime edits in this branch (`integration.patch`, checked with `git apply --check` against 170094d)

The patch holds the same two hunks this branch already commits. It is for landing them on a base without this
branch, so it does not apply on top of it. The rail bore agent will probably add its own kit cell. If both land, the
merged count is 45, not 44.

- **kit_scene.rs.** `crates/hex-render/src/kit_scene.rs` gets the bore alone on a meadow at (4, 1) in the kit
  scenario, at the end of `kit_layout`:
  ```rust
      // The open road tunnel's bore (P4 open tunnels, 2026-11-14) alone on a meadow at (4, 1), east of
      // the ocean corner: the tube at the meadow's level with its ends open, so the lining, the lamps'
      // always-on glow and the road on its floor show in kit light.
      v.push(place("meadow_base", 4, 1, 0));
      v.push(place("road_tunnel_bore", 4, 1, 0));
  ```
- **lib.rs.** `crates/hex-render/src/lib.rs` `tests::tile_counts` raises the kit cell count from 43 to 44, in both
  `cells.len()` and `WaterKit.tile_count(Runtime)`. This is that placement's own count, so the two land together.
- **Nothing else.** The pieces need no other runtime or data edit. The bore spawn system, hiding the back slot and the
  corridor are spec sections 2 to 4.
