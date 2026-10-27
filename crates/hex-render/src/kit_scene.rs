//! `scenario=kit`: the P1 family laid out for look-dev, the seam close-up and LOD checks.
//! Pieces come from `assets/kit/manifest.json` (compiled in); missing pieces warn and are skipped.
//! `kit_src=fixture` swaps in the kit gate fixture (served from `assets/kit/p1/fixture/`).
use bevy::{
    gltf::{Gltf, GltfMesh, GltfNode},
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{
    KitManifest, TextureFamily, lod_mesh_name, parse_manifest, part_of, rotor_mesh_name,
    select_family,
};
use hex_core::{Axial, HEX_RADIUS};
use std::collections::HashMap;

use crate::{
    SceneEntity,
    kit_material::{KitMaterial, build_material, slot_requests},
    scene::{Lod, lod_cover, lod_cover_range},
    workload::{GateConfig, KitSource},
};

pub const MANIFEST: &str = include_str!("../../../assets/kit/manifest.json");
/// The gate fixture's manifest; its files are copied to `assets/kit/p1/fixture/` so `kit://`
/// serves them natively and on the web.
pub const FIXTURE_MANIFEST: &str = include_str!("../tests/fixtures/kit/manifest.json");
/// The fixture with its cap on a probe tileable set (`kit_src=fixture_tiled`, smoke only).
pub const FIXTURE_TILED_MANIFEST: &str =
    include_str!("../../../assets/kit/p1/fixture_tile/manifest.json");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub piece: &'static str,
    pub cell: Axial,
    /// Rotation k: edge e maps to edge (e + k) mod 6 (PRD §5.1).
    pub rot: u8,
    /// Offset from the cell center in meters (x, z), applied after rotation.
    pub offset: [f32; 2],
    /// Extra turn about +Y after `rot`, in degrees, the way edges run (90 maps +x to +z): a hero
    /// prop's `yaw_deg` (`data/tiles/*.ron`).
    pub yaw_deg: f32,
    /// Height in meters: props sit on the base's relief at their spot (0 for bases and networks).
    pub y: f32,
}

const fn place(piece: &'static str, q: i32, r: i32, rot: u8) -> Placement {
    Placement {
        piece,
        cell: Axial { q, r },
        rot,
        offset: [0.0, 0.0],
        yaw_deg: 0.0,
        y: 0.0,
    }
}

/// A hero prop at tile-local runtime `at` (x, z) with `yaw_deg`, on a tile placed at rot 0.
const fn hero(piece: &'static str, q: i32, r: i32, at: [f32; 2], yaw_deg: f32) -> Placement {
    Placement {
        offset: at,
        yaw_deg,
        ..place(piece, q, r, 0)
    }
}

/// Two road-straight tiles meeting at (0,0) edge 0 / (1,0) edge 3, ringed by meadow at every
/// rotation (Review Focus 1), and the hero tree on (0,1).
pub fn kit_layout() -> Vec<Placement> {
    let mut v = vec![];
    for (q, r) in [(0, 0), (1, 0)] {
        v.push(place("meadow_base", q, r, 0));
        v.push(place("road_straight_center", q, r, 0));
        v.push(place("road_straight_edge", q, r, 0));
        v.push(place("road_straight_edge", q, r, 3));
    }
    for (k, (q, r)) in [(0, 1), (1, -1), (-1, 0), (0, -1), (-1, 1), (2, -1)]
        .into_iter()
        .enumerate()
    {
        v.push(place("meadow_base", q, r, k as u8));
    }
    v.push(Placement {
        offset: [3.0, -2.0],
        ..place("tree_hero_oak", 0, 1, 0)
    });
    // Farm hay bales (P3) on the oak's meadow, lifted onto the meadow relief at their spots.
    for &(piece, offset, yaw_deg, y) in HAY_BALES {
        v.push(Placement {
            offset,
            yaw_deg,
            y,
            ..place(piece, 0, 1, 0)
        });
    }
    // The farm barn (P3, refs A17) on the oak's meadow: its long side along edge 2, its back 0.5 m
    // inside the border and its nearest corner 9.6 m from the oak (crown 8.55 m), on the meadow
    // relief at its centre (`relief()` in scripts/blender/kit/meadow_base.py).
    v.push(Placement {
        offset: [-4.75, 8.227],
        yaw_deg: 30.0,
        y: -0.013,
        ..place("barn_red", 0, 1, 0)
    });
    // The bakery (P3, refs A17) on road tile (1, 0), north of the road and facing it (yaw 90 turns
    // its +x shop front to +z), its LOD0 front (the bread basket, 3.73 m) 0.27 m clear of the road
    // profile (|z| <= 4.6 m), on the meadow relief at its centre (`relief()` in
    // scripts/blender/kit/meadow_base.py).
    v.push(Placement {
        offset: [1.5, -8.6],
        yaw_deg: 90.0,
        y: 0.0,
        ..place("bakery_shop", 1, 0, 0)
    });
    // Its hero variant, the twin-gabled bakehouse bakery_shop_b, across the road on the same tile,
    // its shop front turned to +x toward the kit sun (yaw 0), its +y eave (runtime z 5.17) 0.57 m
    // beyond the road profile (|z| <= 4.6 m) and its eave corners 0.58 m inside the border, on the
    // meadow relief at its centre.
    v.push(Placement {
        offset: [1.5, 9.0],
        yaw_deg: 0.0,
        y: 0.0,
        ..place("bakery_shop_b", 1, 0, 0)
    });
    // Verge bushes (road refs approved 2026-10-13, ref-1's "grass verges with tufts and small
    // bushes") on road tile (0, 0): two on the sunny south verge and one on the north, each beyond
    // the kerb and the verge sink (|z| 4.6) on the meadow relief at its spot (`relief()` in
    // scripts/blender/kit/meadow_base.py at Blender (x, -z)); a shrub_forest beside them and a sedan
    // on the lane for scale.
    for &(piece, offset, yaw_deg, y) in VERGE_BUSHES {
        v.push(Placement {
            offset,
            yaw_deg,
            y,
            ..place(piece, 0, 0, 0)
        });
    }
    // Farm animals (P3) on the oak's meadow, in open sun on the oak's +x/+z side.
    for &(piece, offset, yaw_deg, y) in FARM_ANIMALS {
        v.push(Placement {
            offset,
            yaw_deg,
            y,
            ..place(piece, 0, 1, 0)
        });
    }
    // house_cluster (data/tiles/house_cluster.ron) on the meadow at (-1, 0): its edge-0 road stub
    // meets road tile (0, 0)'s edge-3 stub, so the road runs on into the cluster's driveway.
    v.push(place("house_pad", -1, 0, 0));
    v.push(place("road_straight_edge", -1, 0, 0));
    v.push(hero("house_cottage", -1, 0, [-0.5, -7.0], 90.0));
    v.push(hero("house_cottage", -1, 0, [-0.5, 7.0], 270.0));
    v.push(hero("house_twostorey", -1, 0, [-7.5, 0.0], 0.0));
    // Its well (hero 3) on the lawn east of cottage 0, front to the kit sun; the variants' cluster
    // below shows the open well_stone_b in the same slot. Its apron and every foot run to -0.35 m,
    // under the meadow relief at any base rotation, so it stands at y 0.
    v.push(hero("well_stone", -1, 0, [5.3, -7.7], 60.0));
    // The cottage slots' variants in a second house_cluster on (-1, -1): its edge-0 road stub meets
    // the wide bend's world-edge-3 stub on (0, -1). house_longhouse stands in house 0's slot (yawed
    // 90, front toward the yard) and house_cottage_b in house 1's across the yard (yawed 270), the
    // cross-wing two-storey variant (house_twostorey_c) closes the yard, and sedans are parked at
    // park-0 and park-1 (`PARK` in scripts/blender/kit/house_pad.py), on the gravel.
    v.push(place("meadow_base", -1, -1, 0));
    v.push(place("house_pad", -1, -1, 0));
    v.push(place("road_straight_edge", -1, -1, 0));
    v.push(hero("house_longhouse", -1, -1, [-0.5, -7.0], 90.0));
    v.push(hero("house_cottage_b", -1, -1, [-0.5, 7.0], 270.0));
    v.push(hero("house_twostorey_c", -1, -1, [-7.5, 0.0], 0.0));
    v.push(hero("well_stone_b", -1, -1, [5.3, -7.7], 60.0));
    for z in [-2.6, 2.6] {
        v.push(Placement {
            y: PARK_Y,
            ..hero("car_sedan", -1, -1, [-1.0, z], 180.0)
        });
    }
    // The two-storey variant (house_twostorey_b) in a third house_cluster on (1, -2) at rot 1: its
    // road stub (local edge 0, world edge 1) meets the Y junction's world-edge-4 stub on (1, -1). It
    // stands in house 2's slot, its front turned to world yaw 60 toward the kit sun, between the two
    // cottages, with a sedan parked in front of it at park-2 (`PARK` in house_pad.py).
    v.push(place("meadow_base", 1, -2, 1));
    v.push(place("house_pad", 1, -2, 1));
    v.push(place("road_straight_edge", 1, -2, 1));
    for (piece, at, yaw_deg) in [
        ("house_cottage", [-0.5, -7.0], 90.0),
        ("house_cottage", [-0.5, 7.0], 270.0),
        ("house_twostorey_b", [-7.5, 0.0], 0.0),
    ] {
        v.push(Placement {
            offset: at,
            yaw_deg,
            ..place(piece, 1, -2, 1)
        });
    }
    v.push(Placement {
        offset: [-1.0, 0.0],
        yaw_deg: 180.0,
        y: PARK_Y,
        ..place("car_sedan", 1, -2, 1)
    });
    // Road-family centre pieces on ring cells, each with its edge stubs. The wide bend on (0, -1)
    // turns from world edge 3 (the longhouse cluster's road) to world edge 5 (the twostorey_b
    // cluster's grass on (1, -2)).
    v.push(place("road_bend_120_center", 0, -1, 3));
    v.push(place("road_straight_edge", 0, -1, 3));
    v.push(place("road_straight_edge", 0, -1, 5));
    // The sharp bend on (2,-1), lanes on world edges 2 and 3, with the tile's stubs trimmed at the
    // inner corner (local lanes 0 and 1).
    v.push(place("road_bend_60_center", 2, -1, 2));
    v.push(place("road_bend_60_edge_e0", 2, -1, 2));
    v.push(place("road_bend_60_edge_e1", 2, -1, 3));
    // The Y junction on (1,-1); its edge 0 meets the sharp bend's edge-3 lane.
    v.push(place("road_junction_center", 1, -1, 0));
    for k in [0, 2, 4] {
        v.push(place("road_straight_edge", 1, -1, k));
    }
    // Three ocean tiles meeting at one corner, each at another rotation (seams under rotation), with
    // open edges to the east. Cells stay inside the water look-dev patch, so `water_kit` keeps 12.
    // The seabed rocks and pebbles are play scatter (seabed_rock, seabed_pebbles), not a centre;
    // SEABED shows them on the first ocean tile.
    for (q, r, k) in [(1, 1, 0), (2, 1, 3), (2, 0, 1)] {
        v.push(place("ocean_base", q, r, k));
    }
    for &(piece, at, yaw_deg) in SEABED {
        v.push(Placement {
            y: SEABED_Y,
            ..hero(piece, 1, 1, at, yaw_deg)
        });
    }
    // Meadow props on the meadow at (-1, 1) (base rotation 4), each lifted onto the meadow relief at
    // its spot (`relief()` in scripts/blender/kit/meadow_base.py, evaluated in the base's frame).
    for &(piece, offset, rot, y) in MEADOW_PROPS {
        v.push(Placement {
            offset,
            y,
            ..place(piece, -1, 1, rot)
        });
    }
    // The windmill (P3, refs approved under A17) alone on a meadow at (3, -1), the one water-patch
    // cell the layout left free (so `water_kit` keeps its 12 cells), its sails turned toward the
    // overview camera. Its rotor turns (`spin_rotors`).
    v.push(place("meadow_base", 3, -1, 0));
    v.push(Placement {
        yaw_deg: 120.0,
        y: WINDMILL_Y,
        ..place("windmill", 3, -1, 0)
    });
    // Its hero variant, the smock mill windmill_b, on its own meadow at (4, -1) east of it (outside
    // the water patch, and off the line its sails face, so neither mill hides the other), at the same
    // yaw and cap-centre relief, its rotor turning.
    v.push(place("meadow_base", 4, -1, 0));
    v.push(Placement {
        yaw_deg: 120.0,
        y: WINDMILL_Y,
        ..place("windmill_b", 4, -1, 0)
    });
    // Beach dressing (owner D3, refs approved 2026-10-20) on a straight beach at (5, -1), east of
    // windmill_b and outside the water patch: the three driftwood pieces along the coast on the dry
    // sand, the three marram clumps landward of them and the three pebble clusters at the grass
    // line, as the scatter lays them.
    v.push(place("beach_base", 5, -1, 0));
    for &(piece, at, yaw_deg, y) in BEACH_DRESSING {
        v.push(Placement {
            y,
            ..hero(piece, 5, -1, at, yaw_deg)
        });
    }
    // Rail (P3, refs A17) on two meadows south of the oak: the straight on (0, 2) and the sharp
    // bend on (1, 2) at rot 2 (lanes on world edges 2 and 3), so the bend's edge-3 lane meets the
    // straight's edge-0 lane and its other end runs out to the open rim rather than the ocean.
    v.push(place("meadow_base", 0, 2, 0));
    v.push(place("rail_straight_center", 0, 2, 0));
    // The station (P3, refs A17) beside that straight, in the straight's own frame (same cell and
    // rotation): the platform on the north side (runtime -z), coping 1.66 m off the track centreline
    // and 0.92 m above the rail top, and the shelter standing on its paving (scripts/blender/kit/
    // _station.py: SHELTER_AT, H).
    v.push(place("rail_station_platform", 0, 2, 0));
    v.push(Placement {
        offset: STATION_SHELTER_AT,
        y: STATION_PLATFORM_TOP,
        ..place("rail_station_shelter", 0, 2, 0)
    });
    v.push(place("meadow_base", 1, 2, 0));
    v.push(place("rail_bend_60_center", 1, 2, 2));
    // The train (P3, refs A17) on the rail: the engine and two carriages on the 8 m bend at (1, 2),
    // each with its outer axles on the track centreline and the inner buffers touching, wheels on the
    // rail top. Poses from docs/checkpoints/train/consist.py (the bend's frame, rot 2).
    for (piece, offset, yaw_deg) in TRAIN_ON_BEND {
        v.push(Placement {
            offset,
            yaw_deg,
            y: TRAIN_RAIL_TOP,
            ..place(piece, 1, 2, 2)
        });
    }
    // The level crossing (P3, refs A17) west of the straight on (-1, 2): its rail lane on edge 0
    // meets the straight's edge 3, and its road lanes on edges 1 and 4 carry road stubs.
    v.push(place("meadow_base", -1, 2, 0));
    v.push(place("rail_road_crossing_center", -1, 2, 0));
    v.push(place("road_straight_edge", -1, 2, 1));
    v.push(place("road_straight_edge", -1, 2, 4));
    // The rail junction (P3, refs A17) on (0, 3) at rot 5 (lanes on world edges 5, 0 and 2): its toe
    // meets the rail bend's free end at (1, 2), its straight runs on into a rail straight on (-1, 4)
    // and its 8 m branch into a rail bend on (1, 3), so all three of its edges show a seam.
    v.push(place("meadow_base", 0, 3, 0));
    v.push(place("rail_junction_center", 0, 3, 5));
    v.push(place("meadow_base", -1, 4, 0));
    v.push(place("rail_straight_center", -1, 4, 2));
    v.push(place("meadow_base", 1, 3, 0));
    v.push(place("rail_bend_60_center", 1, 3, 2));
    // The forest floor (P3, refs approved 2026-10-13: "ferns, fallen log") on a forest at (-2, 1),
    // west of the meadow props: spruces at the back, the fallen log in open sun in front, ferns
    // around it and one tucked against a trunk, each on the litter at its spot (`relief()` in
    // scripts/blender/kit/forest_base.py at Blender (x, -z)).
    v.push(place("forest_base", -2, 1, 0));
    for &(piece, at, yaw_deg, y) in FOREST_FLOOR {
        v.push(Placement {
            y,
            ..hero(piece, -2, 1, at, yaw_deg)
        });
    }
    // The forest-floor variants (PRD §6 Variety) on their own forest at (-3, 1), west of the
    // originals' cell so nothing here shades it (the sun is at +x/+z): fern_clump_b and _c and
    // log_fallen_b in open sun, with an original fern and log beside them and one spruce at the back.
    v.push(place("forest_base", -3, 1, 0));
    for &(piece, at, yaw_deg, y) in FOREST_FLOOR_VARIANTS {
        v.push(Placement {
            y,
            ..hero(piece, -3, 1, at, yaw_deg)
        });
    }
    // The scatter oaks (refs approved 2026-10-13: "spruce among oaks") on their own forest at
    // (-2, 2), south of the forest-floor showcase and out of its review cameras and shade, with a
    // spruce behind them for comparison, each on the litter at its spot (`relief()` in
    // scripts/blender/kit/forest_base.py at Blender (x, -z)).
    v.push(place("forest_base", -2, 2, 0));
    for &(piece, at, yaw_deg, y) in OAK_GROVE {
        v.push(Placement {
            y,
            ..hero(piece, -2, 2, at, yaw_deg)
        });
    }
    // The forest undergrowth (refs approved 2026-10-13, ref-1: "undergrowth of ferns, shrubs and
    // saplings") on its own forest at (-3, 2), west of the oak grove: two spruces at the back, the
    // shrubs and saplings in open sun in front with a fern beside them, each on the litter at its
    // spot (`relief()` in scripts/blender/kit/forest_base.py at Blender (x, -z)).
    v.push(place("forest_base", -3, 2, 0));
    for &(piece, at, yaw_deg, y) in UNDERGROWTH {
        v.push(Placement {
            y,
            ..hero(piece, -3, 2, at, yaw_deg)
        });
    }
    // The river rail bridge (P3) on (-2, 5) at rot 1 (rail on its local edges 1 and 4 = world 2 and
    // 5, river on local 0 and 3 = world 1 and 4): the junction's straight on (-1, 4) runs into it
    // across world edge 5 and a rail straight on (-3, 6) carries it on across world edge 2. It sits
    // on river_straight_base with river_edge stubs, between river straights upstream on (-2, 4) and
    // downstream on (-2, 6). The engine and a carriage stand on the deck at the rail top.
    v.push(place("meadow_base", -3, 6, 0));
    v.push(place("rail_straight_center", -3, 6, 2));
    // The wide rail bend (P3, owner 2026-10-20: rail bends like the road set) on (-4, 7) at rot 3
    // (lanes on world edges 3 and 5): its edge-5 lane carries that straight's free end on across
    // world edge 2, and its other end runs out to the open rim.
    v.push(place("meadow_base", -4, 7, 0));
    v.push(place("rail_bend_120_center", -4, 7, 3));
    // The river straights have no centre piece: their bank stones and reeds are runtime scatter
    // (owner, 2026-10-26), shown here as RIVER_BANK on the downstream straight.
    for (q, r, center) in [
        (-2, 4, None),
        (-2, 5, Some("river_rail_bridge_center")),
        (-2, 6, None),
    ] {
        v.push(place("river_straight_base", q, r, 1));
        if let Some(center) = center {
            v.push(place(center, q, r, 1));
        }
        v.push(place("river_edge", q, r, 1));
        v.push(place("river_edge", q, r, 4));
    }
    for &(piece, at, yaw_deg, y) in RIVER_BANK {
        v.push(Placement {
            y,
            ..hero(piece, -2, 6, at, yaw_deg)
        });
    }
    for &(piece, at, yaw_deg, y) in WATER_PROPS {
        v.push(Placement {
            y,
            ..hero(piece, -2, 4, at, yaw_deg)
        });
    }
    // The river fork (owner 2026-10-20) on (-3, 4), west of the river straights: river_fork_base
    // at rot 1 (river on world edges 1 and 3 facing empty cells and 5 meeting the tight
    // bend on (-2, 3); its grass edge 0 meets the straight's grass edge 3 on (-2, 4)) with a river_edge stub on each River
    // edge. The kit scenario spawns no water, so its channel shows dry here; play draws the water.
    v.push(place("river_fork_base", -3, 4, 1));
    for k in [1, 3, 5] {
        v.push(place("river_edge", -3, 4, k));
    }
    // The tight river bend (river_bend_60: lanes on local edges 0 and 1) on (-2, 3) at rot 1 (lanes
    // on world edges 1 and 2): it joins the straight on (-2, 4) across world edge 1 to the fork's
    // edge-5 arm on (-3, 4) across world edge 2. Its own channel-cut base with a river_edge stub at
    // each lane.
    v.push(place("river_bend_60_base", -2, 3, 1));
    v.push(place("river_edge", -2, 3, 1));
    v.push(place("river_edge", -2, 3, 2));
    for (piece, offset) in TRAIN_ON_RAIL_BRIDGE {
        v.push(Placement {
            offset,
            yaw_deg: 60.0,
            y: TRAIN_RAIL_TOP,
            ..place(piece, -2, 5, 1)
        });
    }
    // A buffer stop (owner D2) at the layout's open rail end at local edge 0 of the bend on (1, 3)
    // (rot 2, world edge 2, facing an empty cell), with the engine standing buffered up to it as
    // hex-sim poses it. The straight on (-3, 6) carries on into the wide bend, so it has none.
    for (q, r, stop, engine) in BUFFER_STOPS {
        v.push(Placement {
            offset: [stop[0], stop[1]],
            yaw_deg: stop[2],
            ..place("rail_buffer_stop", q, r, 2)
        });
        v.push(Placement {
            offset: [engine[0], engine[1]],
            yaw_deg: engine[2],
            y: TRAIN_RAIL_TOP,
            ..place("train_engine", q, r, 2)
        });
    }
    // The farm (data/tiles/farm.ron) with its paddock fence (P3) on a meadow at (-1, 5), south-east
    // of the river rail bridge; no lane of a neighbour runs onto it. The heroes stand at y = 0, as
    // play places them; the fence mesh carries the meadow relief itself.
    v.push(place("meadow_base", -1, 5, 0));
    for &(piece, at, yaw_deg) in FARM_HEROES {
        v.push(hero(piece, -1, 5, at, yaw_deg));
    }
    // The farm animal variants (PRD §6 Variety) on a second farm at (0, 5), east of the first: the
    // grazing cow animal_cow_b head to tail with animal_cow, and the lamb animal_sheep_b beside a
    // ewe, each hoof within 2.3 cm of the meadow relief at y = 0 (option O2 proposed for farm.ron),
    // and the grazing sheep animal_sheep_c west of the lamb, its worst hoof 2.8 cm above the relief.
    v.push(place("meadow_base", 0, 5, 0));
    for &(piece, at, yaw_deg) in FARM_VARIANTS {
        v.push(hero(piece, 0, 5, at, yaw_deg));
    }
    // The hay variants (PRD §6 Variety) on a third farm at (1, 5): hay_bale_stack_b in the farm's
    // stack slot and hay_bale_round_b in both round slots, as the hero variant resolver may draw
    // them, each standing on the meadow relief with no contact over it, beside barn_red_b in the
    // barn slot.
    v.push(place("meadow_base", 1, 5, 0));
    for &(piece, at, yaw_deg) in FARM_HAY_VARIANTS {
        v.push(hero(piece, 1, 5, at, yaw_deg));
    }
    // Boats (P3, refs D4), each origin on its water at rest: the landing stage on the downstream
    // river straight (-2, 6) (rot 1: the flow runs along (0.5, 0.866)) at the play water
    // (`hex_play::water::tiles::WATER_Y`), its origin on the bank's waterline 4 m from the centre
    // line and its +x toward it, with a rowing boat on the far half of the channel heading
    // downstream (no water is drawn there: the posts show against the bed); on the ocean tiles of
    // the water patch the sailing boat on (2, 0) and a second rowing boat on (1, 1), on the
    // look-dev water at y = 0 that `water_kit` draws there.
    for &(piece, q, r, at, yaw_deg, y) in BOATS {
        v.push(Placement {
            y,
            ..hero(piece, q, r, at, yaw_deg)
        });
    }
    v
}

/// The play water at rest (`hex_play::water::tiles::WATER_Y`).
const PLAY_WATER_Y: f32 = -crate::water::LAND;

/// (piece, q, r, at, yaw, y) of the boats showcase.
const BOATS: &[(&str, i32, i32, [f32; 2], f32, f32)] = &[
    ("landing_stage", -2, 6, [3.464, -2.0], 150.0, PLAY_WATER_Y),
    ("boat_rowing", -2, 6, [-1.299, 0.75], 60.0, PLAY_WATER_Y),
    ("boat_sail", 2, 0, [0.0, 0.0], 200.0, 0.0),
    ("boat_rowing", 1, 1, [-3.0, 2.0], 30.0, 0.0),
];

/// (piece, at, yaw) of the hay-variants showcase on (1, 5): the farm's fence, the catslide barn
/// barn_red_b in the barn slot (barn_red stands in the same slot next door on (0, 5)), and the
/// variants in the farm's three hay slots (data/tiles/farm.ron).
const FARM_HAY_VARIANTS: &[(&str, [f32; 2], f32)] = &[
    ("fence_paddock", [0.0, 0.0], 0.0),
    ("barn_red_b", [4.75, -8.227], 30.0),
    ("hay_bale_stack_b", [8.9, -2.4], 30.0),
    ("hay_bale_round_b", [7.5, -0.4], 15.0),
    ("hay_bale_round_b", [10.4, -1.4], 30.0),
];

/// (piece, at, yaw) of the farm-variants showcase on (0, 5): the farm's barn, fence and ewes, its cow
/// moved beside the grazing cow, the lamb, and the grazing sheep animal_sheep_c west of the lamb.
const FARM_VARIANTS: &[(&str, [f32; 2], f32)] = &[
    ("fence_paddock", [0.0, 0.0], 0.0),
    ("barn_red", [4.75, -8.227], 30.0),
    ("animal_cow", [0.5, 8.125], 135.0),
    ("animal_cow_b", [-0.5, 8.75], 315.0),
    ("animal_sheep", [-0.5, 10.75], 45.0),
    ("animal_sheep", [-2.25, 11.25], 345.0),
    ("animal_sheep_b", [-1.25, 9.625], 30.0),
    ("animal_sheep_c", [-2.25, 9.625], 210.0),
];

/// (piece, at, yaw) of the farm tile's heroes (data/tiles/farm.ron) plus the paddock fence at the
/// tile centre.
const FARM_HEROES: &[(&str, [f32; 2], f32)] = &[
    ("fence_paddock", [0.0, 0.0], 0.0),
    ("barn_red", [4.75, -8.227], 30.0),
    ("hay_bale_stack", [8.9, -2.4], 30.0),
    ("hay_bale_round", [7.5, -0.4], 15.0),
    ("hay_bale_round", [10.4, -1.4], 30.0),
    ("animal_cow", [0.25, 8.25], 255.0),
    ("animal_sheep", [-0.5, 10.75], 45.0),
    ("animal_sheep", [-2.25, 11.25], 345.0),
];

/// (piece, offset, yaw, relief height) of the scatter-oak showcase on the forest at (-2, 2).
const OAK_GROVE: &[(&str, [f32; 2], f32, f32)] = &[
    ("tree_oak", [-4.0, 1.0], 0.0, -0.006),
    ("tree_oak_b", [5.0, 2.0], 70.0, 0.0),
    ("tree_oak_c", [0.0, 10.0], 40.0, 0.0),
    ("tree_forest", [0.5, -6.5], 150.0, 0.0),
];

/// (piece, offset, yaw, relief height) of the forest undergrowth on the forest at (-3, 2), each on the
/// litter at its spot (`relief()` in scripts/blender/kit/forest_base.py at Blender (x, -z)).
const UNDERGROWTH: &[(&str, [f32; 2], f32, f32)] = &[
    ("tree_forest", [-6.5, -5.5], 0.0, -0.013),
    ("tree_forest_c", [-1.5, -6.5], 80.0, 0.0),
    ("shrub_forest", [2.5, -1.0], 0.0, -0.134),
    ("shrub_forest", [-4.5, -1.5], 120.0, -0.015),
    ("shrub_forest", [4.0, 3.5], 240.0, -0.035),
    ("shrub_forest_b", [6.5, 0.5], 30.0, -0.011),
    ("shrub_forest_c", [8.0, -2.5], 200.0, -0.002),
    ("sapling_spruce_b", [0.0, 2.0], 0.0, -0.094),
    ("sapling_spruce", [-2.5, 1.5], 140.0, -0.028),
    ("sapling_spruce_c", [5.0, -4.0], 260.0, -0.089),
    ("fern_clump", [-1.0, 5.5], 30.0, 0.0),
    ("fern_clump", [2.5, 6.0], 200.0, 0.0),
];

/// house_pad's gravel top (`YARD_Z` in scripts/blender/kit/house_pad.py), where a parked car's wheels
/// stand.
const PARK_Y: f32 = 0.05;

/// (piece, offset) of the engine on the river rail bridge's arch and a carriage coupled behind it, in
/// the bridge's frame (rot 1): the track runs along runtime (0.5, 0.866) there (`yaw_deg` 60), the
/// carriage 8.04 m back (half lengths 4.47 m + 3.57 m, buffers touching).
const TRAIN_ON_RAIL_BRIDGE: [(&str, [f32; 2]); 2] = [
    ("train_engine", [0.0, 0.0]),
    ("train_carriage", [-4.02, -6.963]),
];

/// (piece, offset, yaw, relief height) of the forest-floor showcase on the forest at (-2, 1), with a
/// parked sedan beside the log for scale.
const FOREST_FLOOR: &[(&str, [f32; 2], f32, f32)] = &[
    ("tree_forest", [-6.5, -5.0], 0.0, -0.013),
    ("tree_forest_b", [0.5, -7.5], 40.0, -0.004),
    ("tree_forest_c", [-8.5, 2.5], 110.0, -0.012),
    ("tree_forest", [4.5, -3.0], 200.0, -0.024),
    ("log_fallen", [1.5, 3.0], -25.0, -0.007),
    ("fern_clump", [-1.5, 1.5], 0.0, -0.068),
    ("fern_clump", [0.0, 5.5], 60.0, -0.001),
    ("fern_clump", [4.5, 1.0], 130.0, 0.0),
    ("fern_clump", [-5.5, -2.5], 200.0, 0.0),
    ("fern_clump", [-3.8, -5.6], 250.0, -0.016),
    ("fern_clump", [2.4, -6.0], 310.0, -0.008),
    ("fern_clump", [5.3, -2.6], 20.0, -0.002),
    ("fern_clump", [-4.5, 4.5], 90.0, -0.006),
    ("car_sedan", [7.0, 4.0], -25.0, 0.0),
];

/// (piece, offset, yaw, relief height) of the forest-floor variants on the forest at (-3, 1), each on
/// the litter at its spot (`relief()` in scripts/blender/kit/forest_base.py at Blender (x, -z)).
const FOREST_FLOOR_VARIANTS: &[(&str, [f32; 2], f32, f32)] = &[
    ("tree_forest", [-7.5, -5.0], 30.0, -0.004),
    // In the spruce's shade (the sun is at +x/+z), so its albedo is reviewed under the canopy.
    ("log_fallen_c", [-8.5, -6.0], 130.0, -0.003),
    ("log_fallen", [-2.0, -3.0], -10.0, -0.001),
    ("log_fallen_b", [0.5, 2.0], 20.0, -0.077),
    ("fern_clump", [4.0, -2.5], 0.0, -0.037),
    ("fern_clump_b", [2.8, 0.2], 45.0, -0.046),
    ("fern_clump_b", [-3.2, 4.0], 160.0, -0.026),
    ("fern_clump_b", [4.2, 4.8], 270.0, -0.001),
    ("fern_clump_c", [-3.8, 0.4], 0.0, -0.008),
    ("fern_clump_c", [1.0, 6.2], 120.0, -0.012),
];

/// The rail top above the cap (`RAIL_TOP` in scripts/blender/kit/_rail.py), where a train's wheels stand.
const TRAIN_RAIL_TOP: f32 = 0.46;

/// (q, r, stop [x, z, yaw], engine [x, z, yaw]) at the open local edge 0 of a rail bend, rot 2, in the rail piece's frame: the stop's buffer faces 3.5 m in along the track,
/// and the engine's front buffers there with its axles on the track
/// (docs/checkpoints/rail_buffer_stop/poses.py, runtime-notes.md). The stop stands at cap level.
const BUFFER_STOPS: [(i32, i32, [f32; 3], [f32; 3]); 1] =
    [(1, 3, [10.467, 0.753, -17.05], [7.35, 3.788, -57.08])];

/// (piece, offset, yaw) of the train consist on the rail bend at (1, 2), rot 2: the engine, a
/// carriage and the brake third last (`hex_sim::rail::CONSIST`; the two carriage designs share one
/// length and wheelbase, so the poses hold).
const TRAIN_ON_BEND: [(&str, [f32; 2], f32); 3] = [
    ("train_engine", [9.490, 1.602], -34.3),
    ("train_carriage", [6.041, 8.814], -95.9),
    ("train_carriage_b", [9.072, 15.713], -120.0),
];

/// The station platform's paving above the cap (`H` in scripts/blender/kit/_station.py: rail top
/// 0.46 m + 0.92 m), where the shelter stands.
const STATION_PLATFORM_TOP: f32 = 1.38;

/// The shelter's pivot in the platform's runtime frame (x, z): `SHELTER_AT` (Blender (-0.3, 5.0)).
const STATION_SHELTER_AT: [f32; 2] = [-0.3, -5.0];

/// The meadow relief at the cap's centre (`relief(0, 0)` in scripts/blender/kit/meadow_base.py), where
/// the windmill stands.
const WINDMILL_Y: f32 = -0.185;

/// (piece, offset, yaw, relief height) of the hay bales on the oak's meadow (0, 1), base rotation 0.
const HAY_BALES: &[(&str, [f32; 2], f32, f32)] = &[
    ("hay_bale_round", [-2.0, 2.5], 20.0, -0.084),
    ("hay_bale_stack", [-9.5, 1.0], -15.0, 0.0),
    ("hay_bale_round", [-8.0, -4.5], 75.0, 0.0),
];

/// (piece, offset, yaw, height) of the verge bushes on road tile (0, 0), base rotation 0, with a
/// shrub_forest beside them and a sedan on the lane (`cars::ROAD_SURFACE_Y`) for comparison; the
/// lighter bush_verge_b beside the south pair and the flowering bush_verge_c east of them, all in
/// open sun.
const VERGE_BUSHES: &[(&str, [f32; 2], f32, f32)] = &[
    ("bush_verge", [-5.0, 5.0], 0.0, -0.008),
    ("bush_verge", [3.5, 5.0], 140.0, 0.0),
    ("bush_verge", [0.5, -5.0], 250.0, 0.0),
    ("bush_verge_b", [1.9, 5.2], 30.0, 0.0),
    ("bush_verge_c", [5.8, 5.6], 75.0, -0.006),
    ("shrub_forest", [-1.8, 6.3], 60.0, 0.0),
    ("car_sedan", [0.5, 1.5], 0.0, 0.11),
];

/// (piece, offset, yaw, relief height) of the farm animals on the oak's meadow (0, 1), base rotation 0.
const FARM_ANIMALS: &[(&str, [f32; 2], f32, f32)] = &[
    ("animal_cow", [7.0, 2.5], 70.0, 0.0),
    ("animal_sheep", [3.5, 7.0], -30.0, 0.0),
    ("animal_sheep", [2.0, 9.0], 140.0, 0.0),
];

/// (piece, at, yaw, height) of the beach dressing showcase on the straight beach at (5, -1). The
/// tile's coast runs along (-0.5, 0.866) in (x, z) with s = 0.866 x + 0.5 z seaward (`Coastline`),
/// so each spot is (s, u) along it and its height is the coast profile there (`coast_height(s)`,
/// -1.4 + 1.4 t (2 - t) with t = s / -8). The driftwood lies along the coast (yaw 120 or 300). The
/// pebble clusters stand 0.8-1 m seaward of the grass line (s = -8), inside their band
/// (`beach_band`: s -7.5 to -6), between the marram clumps.
const BEACH_DRESSING: &[(&str, [f32; 2], f32, f32)] = &[
    ("beach_driftwood", [-0.964, -6.33], 120.0, -0.35),
    ("beach_driftwood_b", [-3.79, -1.03], 300.0, -0.386),
    ("beach_driftwood_c", [-6.64, 3.1], 120.0, -0.316),
    ("beach_dunegrass", [-4.13, -5.85], 0.0, -0.049),
    ("beach_dunegrass_b", [-6.45, -0.84], 70.0, -0.088),
    ("beach_dunegrass_c", [-7.89, 0.06], 200.0, -0.032),
    ("beach_pebbles", [-2.812, -9.129], 30.0, -0.022),
    ("beach_pebbles_b", [-6.235, -3.6], 150.0, -0.014),
    ("beach_pebbles_c", [-9.562, 2.562], 260.0, -0.022),
];

/// Height of the seabed dressing: the ocean bed's contract level (`scatter::SEABED_Y`, one level
/// step under the land), under the floor's relief.
const SEABED_Y: f32 = -hex_core::LEVEL_STEP;

/// (piece, at, yaw) of the seabed dressing (owner 2026-10-26) on the ocean at (1, 1), rot 0: two
/// rocks and two pebble clusters well inside the 2 m border the scatter keeps (`SEABED_BORDER`).
const SEABED: &[(&str, [f32; 2], f32)] = &[
    ("seabed_rock", [-3.0, 2.5], 40.0),
    ("seabed_rock", [4.5, -2.0], 200.0),
    ("seabed_pebbles", [1.0, 4.5], 110.0),
    ("seabed_pebbles", [-4.0, -3.5], 290.0),
];

/// (piece, at, yaw, height) of the riverbank scatter props (owner 2026-10-26) on the downstream river
/// straight at (-2, 6), rot 1: the channel runs along t = (0.5, 0.866) in (x, z) with the bank
/// normal n = (-0.866, 0.5), so each spot is u t + v n, its height `_river.profile_z(v)`; the stone
/// clusters lie along the bank (yaw 60 or 240), straddling the waterline (|v| 4.0-4.1), and the reed
/// clumps stand on the bank above it (|v| 5.2-5.4).
const RIVER_BANK: &[(&str, [f32; 2], f32, f32)] = &[
    ("river_stones", [-5.964, -2.33], 60.0, -1.399),
    ("river_stones_b", [5.051, 0.548], 240.0, -1.295),
    ("river_stones_c", [0.449, 8.978], 60.0, -1.295),
    ("river_reeds", [-5.003, 1.734], 0.0, -0.572),
    ("river_reeds_b", [7.676, 2.496], 0.0, -0.462),
];

/// (piece, at, yaw, height) of the water-surface scatter props (B3, owner A26, 2026-10-27) on the
/// upstream river straight at (-2, 4), rot 1, in RIVER_BANK's frame (u t + v n): the lily clusters
/// float at the water at rest plus 0.01 m (-1.39, as the spring's pads) within 4.0 m of the centre
/// line less their reach, and the reed bed stands at the water at rest (-1.4) 3.3 m out on the +n
/// bank, its local +x along -t and its Blender +y (runtime -z) up the bank (yaw 240). The kit
/// scenario draws no water, so the channel shows dry here; play draws the water.
const WATER_PROPS: &[(&str, [f32; 2], f32, f32)] = &[
    ("lily_cluster", [-0.864, -4.697], 30.0, -1.39),
    ("lily_cluster_b", [-1.145, 1.816], 150.0, -1.39),
    ("lily_cluster_c", [4.299, 4.446], 270.0, -1.39),
    ("reed_bed", [-4.608, -1.381], 240.0, -1.4),
];

/// (piece, offset, rotation, relief height) of the meadow props in the kit layout.
const MEADOW_PROPS: &[(&str, [f32; 2], u8, f32)] = &[
    ("rock_small", [-6.0, 5.0], 0, 0.0),
    ("rock_small", [5.0, 6.0], 2, 0.0),
    ("rock_small", [-2.5, -7.0], 4, -0.011),
    ("wildflower_clump", [-4.8, 4.2], 1, -0.036),
    ("wildflower_clump", [-6.9, 6.4], 3, 0.0),
    ("wildflower_clump", [-5.0, 6.6], 5, 0.0),
    ("wildflower_clump", [4.0, 7.2], 0, 0.0),
    ("wildflower_clump", [6.2, 4.6], 2, 0.0),
    ("wildflower_clump", [-3.6, -6.0], 1, 0.0),
    ("wildflower_clump", [-1.4, -8.0], 3, 0.0),
    ("wildflower_clump", [-7.4, 3.4], 4, 0.0),
    ("wildflower_clump", [0.0, 5.5], 2, -0.024),
    ("wildflower_clump", [-2.0, 3.0], 0, -0.003),
    ("wildflower_clump", [-8.5, 0.5], 5, -0.041),
    ("wildflower_clump", [2.0, 8.5], 1, -0.004),
    ("wildflower_single_e", [2.5, 3.0], 0, 0.0),
];

/// The gate fixture on the same seam as the road tiles, plus one rotated neighbour.
pub fn fixture_layout() -> Vec<Placement> {
    vec![
        place("fixture", 0, 0, 0),
        place("fixture", 1, 0, 3),
        place("fixture", 0, 1, 1),
    ]
}

/// The manifest and layout a `kit_src` selects.
pub fn source(src: KitSource) -> (&'static str, Vec<Placement>) {
    match src {
        KitSource::Runtime => (MANIFEST, kit_layout()),
        KitSource::Fixture => (FIXTURE_MANIFEST, fixture_layout()),
        KitSource::FixtureTiled => (FIXTURE_TILED_MANIFEST, fixture_layout()),
    }
}

pub fn piece_rotation(rot: u8) -> Quat {
    // Edges run clockwise viewed from +Y: +x -> +z is a negative rotation about +Y.
    Quat::from_rotation_y(-(60.0 * (rot % 6) as f32).to_radians())
}

/// The piece's full rotation: its edge rotation `rot`, then its hero `yaw_deg` (same sense).
pub fn placement_rotation(p: &Placement) -> Quat {
    piece_rotation(p.rot) * Quat::from_rotation_y(-p.yaw_deg.to_radians())
}

pub fn resolve_placements(m: &KitManifest, layout: &[Placement]) -> (Vec<Placement>, Vec<String>) {
    let mut warn = vec![];
    let ok = layout
        .iter()
        .filter(|p| {
            let found = m.pieces.iter().any(|e| e.id == p.piece);
            if !found && !warn.iter().any(|w: &String| w.contains(p.piece)) {
                warn.push(format!("kit: piece {} not in manifest, skipped", p.piece));
            }
            found
        })
        .copied()
        .collect();
    (ok, warn)
}

#[derive(Resource)]
pub struct KitPending;

#[derive(Resource)]
pub struct KitLoading {
    manifest: KitManifest,
    items: Vec<(Placement, Handle<Gltf>)>,
    family: TextureFamily,
}

pub fn start_kit(
    mut commands: Commands,
    pending: Option<Res<KitPending>>,
    device: Option<Res<RenderDevice>>,
    server: Res<AssetServer>,
    config: Res<GateConfig>,
) {
    let (Some(_), Some(device)) = (pending, device) else {
        return;
    };
    commands.remove_resource::<KitPending>();
    let f = device.features();
    let family = select_family(
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC),
        f.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC),
    );
    let (json, layout) = source(config.kit_src);
    let manifest = match parse_manifest(json) {
        Ok(m) => m,
        Err(e) => {
            warn!("kit: {e}");
            return;
        }
    };
    let (placements, warnings) = resolve_placements(&manifest, &layout);
    for w in warnings {
        warn!("{w}");
    }
    let items = placements
        .into_iter()
        .filter_map(|p| {
            let e = manifest.pieces.iter().find(|e| e.id == p.piece)?;
            Some((p, server.load(format!("kit://{}", e.glb))))
        })
        .collect();
    info!("kit: texture family {family:?}");
    commands.insert_resource(KitLoading {
        manifest,
        items,
        family,
    });
}

/// Rotor speed (PRD §7.2: windmill sails turn as a rigid transform): a steady, slow 4 rpm.
pub const ROTOR_RPM: f64 = 4.0;

/// A spinning part of a kit instance: the rotor node's rest transform in its parent's frame (the
/// world here: placement, then the node's own transform; in play the hero's body part, so the node's
/// own transform) and its phase. It turns about its local +X axis.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct KitRotor {
    pub base: Transform,
    pub phase: f32,
}

/// The rotor angle (radians, in [0, 2π)) at `elapsed` seconds of the app clock: `--clock fixed`
/// steps that clock by exactly 1/60 s a frame, so a shot at frame N always sees the same angle.
pub fn rotor_angle(elapsed: f64, phase: f32) -> f32 {
    let w = ROTOR_RPM * std::f64::consts::TAU / 60.0;
    (elapsed * w + f64::from(phase)).rem_euclid(std::f64::consts::TAU) as f32
}

/// A per-instance phase from where the rotor stands (its hub, to the centimetre), so neighbouring
/// windmills do not turn in step. Integer hashing keeps native and web identical.
pub fn rotor_phase(at: Vec3) -> f32 {
    let q = |v: f32| (v * 100.0).round() as i32 as u32;
    let mut h = q(at.x).wrapping_mul(0x9E37_79B1) ^ q(at.z).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

pub fn spin_rotors(time: Res<Time>, mut rotors: Query<(&KitRotor, &mut Transform)>) {
    let t = time.elapsed_secs_f64();
    for (r, mut tf) in &mut rotors {
        *tf = r.base * Transform::from_rotation(Quat::from_rotation_x(rotor_angle(t, r.phase)));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn finish_kit(
    mut commands: Commands,
    loading: Option<Res<KitLoading>>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_nodes: Res<Assets<GltfNode>>,
    mut materials: ResMut<Assets<KitMaterial>>,
    config: Res<GateConfig>,
) {
    let Some(loading) = loading else {
        return;
    };
    // A failed dependency leaves the root `Loaded` but never "loaded with dependencies", so it
    // counts as failed here; otherwise the wait would never end (review M4).
    let failed = |h: &Handle<Gltf>| {
        server.load_state(h).is_failed() || server.recursive_dependency_load_state(h).is_failed()
    };
    if loading
        .items
        .iter()
        .any(|(_, h)| !server.is_loaded_with_dependencies(h) && !failed(h))
    {
        return;
    }
    commands.remove_resource::<KitLoading>();
    let manifest = &loading.manifest;
    let mut cache: HashMap<(String, String), Option<Handle<KitMaterial>>> = HashMap::new();
    let mut spawned = 0;
    for (p, h) in &loading.items {
        if failed(h) {
            warn!(
                "kit: {} or one of its dependencies failed to load, skipped",
                p.piece
            );
            continue;
        }
        let Some(g) = gltfs.get(h) else {
            warn!("kit: {} failed to load, skipped", p.piece);
            continue;
        };
        let Some(entry) = manifest.pieces.iter().find(|e| e.id == p.piece) else {
            continue;
        };
        let [x, z] = p.cell.to_world(HEX_RADIUS);
        let rot = piece_rotation(p.rot);
        let pos = Vec3::new(x, p.y, z) + rot * Vec3::new(p.offset[0], 0.0, p.offset[1]);
        let rot = placement_rotation(p);
        let place = Transform::from_translation(pos).with_rotation(rot);
        // Which LOD body meshes carry each material slot, for the missing-LOD1 fallback.
        let mut present: HashMap<String, [bool; 3]> = HashMap::new();
        for lod in 0..3usize {
            let Some(gm) = g
                .named_meshes
                .get(lod_mesh_name(p.piece, lod).as_str())
                .and_then(|mh| gltf_meshes.get(mh))
            else {
                continue;
            };
            for prim in &gm.primitives {
                let name = prim.material.as_ref().and_then(|mh| {
                    g.named_materials
                        .iter()
                        .find(|(_, v)| *v == mh)
                        .map(|(k, _)| k.to_string())
                });
                if let Some(name) = name {
                    present.entry(name).or_default()[lod] = true;
                }
            }
        }
        for lod in 0..3usize {
            let Some(mh) = g.named_meshes.get(lod_mesh_name(p.piece, lod).as_str()) else {
                warn!("kit: {} has no {}_lod{lod}", p.piece, p.piece);
                continue;
            };
            let Some(gm) = gltf_meshes.get(mh) else {
                continue;
            };
            let mut parts = vec![(gm, place, None)];
            // A rotor node draws at its own transform under the placement and turns about local X.
            if let Some(node) = g
                .named_nodes
                .get(rotor_mesh_name(p.piece, lod).as_str())
                .and_then(|h| gltf_nodes.get(h))
                && let Some(rm) = node.mesh.as_ref().and_then(|h| gltf_meshes.get(h))
            {
                let base = place * node.transform;
                let phase = rotor_phase(base.translation);
                parts.push((rm, base, Some(KitRotor { base, phase })));
            }
            // Animatable parts (a boat's oars and sails) draw at rest at their node transforms; the
            // runtime that owns the piece moves them.
            for (name, h) in &g.named_nodes {
                if part_of(p.piece, lod, name).is_none() {
                    continue;
                }
                if let Some(node) = gltf_nodes.get(h)
                    && let Some(pm) = node.mesh.as_ref().and_then(|h| gltf_meshes.get(h))
                {
                    parts.push((pm, place * node.transform, None));
                }
            }
            for (prim, t, rotor) in parts
                .into_iter()
                .flat_map(|(gm, t, r)| gm.primitives.iter().map(move |prim| (prim, t, r)))
            {
                let slot_name = prim
                    .material
                    .as_ref()
                    .and_then(|mh| {
                        g.named_materials
                            .iter()
                            .find(|(_, v)| *v == mh)
                            .map(|(k, _)| k.to_string())
                    })
                    .unwrap_or_default();
                let Some(slot) = entry.slots.get(&slot_name) else {
                    warn!("kit: {} material {slot_name} has no manifest slot", p.piece);
                    continue;
                };
                let mat = cache
                    .entry((p.piece.to_string(), slot_name.clone()))
                    .or_insert_with(
                        || match slot_requests(manifest, entry, slot, loading.family) {
                            Ok(r) => Some(materials.add(build_material(
                                &server,
                                slot,
                                &r,
                                config.kit_hooks,
                            ))),
                            Err(e) => {
                                warn!("kit: {e}, slot {slot_name} skipped");
                                None
                            }
                        },
                    )
                    .clone();
                let Some(mat) = mat else {
                    continue;
                };
                let cover = match (&rotor, present.get(&slot_name)) {
                    (None, Some(have)) => lod_cover(*have, lod),
                    _ => lod_cover([true; 3], lod),
                };
                let mut e = commands.spawn((
                    Mesh3d(prim.mesh.clone()),
                    MeshMaterial3d(mat),
                    t,
                    lod_cover_range(cover),
                    Lod(lod as u8),
                    SceneEntity,
                ));
                if cover.first as usize != lod || cover.last as usize != lod {
                    e.insert(cover);
                }
                if let Some(r) = rotor {
                    e.insert(r);
                }
                spawned += 1;
            }
        }
    }
    info!(
        "kit: spawned {spawned} meshes, {} materials",
        cache.values().flatten().count()
    );
}

/// Distinct hex cells a kit source lays out (`state().tiles` for the kit scenario).
pub fn cell_count(src: KitSource) -> usize {
    let mut cells: Vec<_> = source(src).1.iter().map(|p| (p.cell.q, p.cell.r)).collect();
    cells.sort();
    cells.dedup();
    cells.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_assets::parse_manifest;

    #[test]
    fn rotation_maps_edge0_to_edge_k() {
        // Edge k's outward direction in XZ is at 60°·k (axial.rs: edge 1 = +z).
        for k in 0..6u8 {
            let v = piece_rotation(k) * Vec3::X;
            let a = (60.0 * k as f32).to_radians();
            assert!(
                (v - Vec3::new(a.cos(), 0.0, a.sin())).length() < 1e-5,
                "k={k} {v}"
            );
        }
    }

    #[test]
    fn hero_yaw_turns_the_way_edges_run() {
        // data/tiles yaw_deg: 90 maps +x to +z, as edge 1 does; it composes after the tile's rot.
        let p = |rot, yaw_deg| Placement {
            yaw_deg,
            ..place("house_cottage", 0, 0, rot)
        };
        let x = |p: Placement| placement_rotation(&p) * Vec3::X;
        assert!((x(p(0, 90.0)) - Vec3::Z).length() < 1e-5);
        assert!((x(p(0, 60.0)) - piece_rotation(1) * Vec3::X).length() < 1e-5);
        assert!((x(p(1, 60.0)) - piece_rotation(2) * Vec3::X).length() < 1e-5);
    }

    #[test]
    fn house_cluster_continues_the_road() {
        // The cluster's edge-0 road stub meets road tile (0, 0)'s edge-3 stub.
        let l = kit_layout();
        let pad = l.iter().find(|p| p.piece == "house_pad").unwrap();
        assert_eq!(pad.cell.neighbor(0), Axial { q: 0, r: 0 });
        assert!(
            l.iter()
                .any(|p| p.piece == "road_straight_edge" && p.cell == pad.cell && p.rot == 0)
        );
    }

    #[test]
    fn longhouse_cluster_meets_the_wide_bend() {
        // The second cluster's edge-0 stub meets the wide bend's world-edge-3 stub on (0, -1), and the
        // longhouse stands in a cottage slot of data/tiles/house_cluster.ron.
        let l = kit_layout();
        let lh = l.iter().find(|p| p.piece == "house_longhouse").unwrap();
        assert!(
            l.iter()
                .any(|p| p.piece == "house_pad" && p.cell == lh.cell && p.rot == 0)
        );
        assert!(
            l.iter()
                .any(|p| p.piece == "road_straight_edge" && p.cell == lh.cell && p.rot == 0)
        );
        let bend = lh.cell.neighbor(0);
        assert!(
            l.iter()
                .any(|p| p.piece == "road_bend_120_center" && p.cell == bend)
        );
        assert!(
            l.iter()
                .any(|p| p.piece == "road_straight_edge" && p.cell == bend && p.rot == 3)
        );
        assert_eq!((lh.offset, lh.yaw_deg), ([-0.5, -7.0], 90.0));
        let cb = l.iter().find(|p| p.piece == "house_cottage_b").unwrap();
        assert_eq!(
            (cb.cell, cb.offset, cb.yaw_deg),
            (lh.cell, [-0.5, 7.0], 270.0)
        );
    }

    #[test]
    fn twostorey_b_cluster_meets_the_junction() {
        // The third cluster's road stub (local edge 0, turned by rot) meets the Y junction's stub on
        // that world edge's far side, and house_twostorey_b stands in house 2's slot of
        // data/tiles/house_cluster.ron.
        let l = kit_layout();
        let h = l.iter().find(|p| p.piece == "house_twostorey_b").unwrap();
        assert_eq!((h.offset, h.yaw_deg), ([-7.5, 0.0], 0.0));
        assert!(
            l.iter()
                .any(|p| p.piece == "house_pad" && p.cell == h.cell && p.rot == h.rot)
        );
        assert!(
            l.iter()
                .any(|p| p.piece == "road_straight_edge" && p.cell == h.cell && p.rot == h.rot)
        );
        let j = h.cell.neighbor(h.rot);
        assert!(
            l.iter()
                .any(|p| p.piece == "road_junction_center" && p.cell == j)
        );
        assert!(
            l.iter().any(|p| p.piece == "road_straight_edge"
                && p.cell == j
                && p.rot == (h.rot + 3) % 6)
        );
    }

    #[test]
    fn rotor_turns_steadily_on_the_app_clock() {
        use std::f32::consts::{FRAC_PI_4, TAU};
        let period = 60.0 / ROTOR_RPM;
        assert!(rotor_angle(0.0, 0.0).abs() < 1e-6);
        assert!((rotor_angle(period / 8.0, 0.0) - FRAC_PI_4).abs() < 1e-5);
        let a = rotor_angle(period * 1000.0 + 1.0, 0.5);
        assert!(
            (a - rotor_angle(1.0, 0.5)).abs() < 1e-4,
            "no drift over long runs"
        );
        // 60 fixed frames (1 s) at 4 rpm turn the sails 24°.
        assert!((rotor_angle(1.0, 0.0) - 24f32.to_radians()).abs() < 1e-5);
        let p = rotor_phase(Vec3::new(69.3, 12.0, -24.0));
        assert!((0.0..TAU).contains(&p));
        assert_eq!(p, rotor_phase(Vec3::new(69.3, 12.0, -24.0)));
        assert_ne!(p, rotor_phase(Vec3::new(41.6, 12.0, -24.0)));
    }

    #[test]
    fn windmill_stands_on_its_own_meadow() {
        let l = kit_layout();
        let w = l.iter().find(|p| p.piece == "windmill").unwrap();
        assert!(
            l.iter()
                .any(|p| p.piece == "meadow_base" && p.cell == w.cell)
        );
        assert_eq!(l.iter().filter(|p| p.cell == w.cell).count(), 2);
        assert!(
            crate::water::layout().iter().any(|(c, _)| *c == w.cell),
            "inside the water look-dev patch"
        );
    }

    #[test]
    fn road_tiles_meet_at_edge0() {
        let l = kit_layout();
        let roads: Vec<_> = l
            .iter()
            .filter(|p| p.piece == "road_straight_center")
            .collect();
        assert_eq!(roads.len(), 2);
        assert_eq!(roads[0].cell.neighbor(0), roads[1].cell);
    }

    #[test]
    fn rail_straight_meets_the_bend() {
        let l = kit_layout();
        let at = |id: &str| *l.iter().find(|p| p.piece == id).unwrap();
        let (s, b) = (at("rail_straight_center"), at("rail_bend_60_center"));
        // The straight's lanes are local edges 0 and 3, the bend's 0 and 1, turned by rot.
        assert_eq!(s.cell.neighbor(s.rot), b.cell);
        assert_eq!((1 + b.rot) % 6, (s.rot + 3) % 6);
    }

    #[test]
    fn wide_rail_bend_continues_the_bridge_line() {
        let l = kit_layout();
        let b = *l
            .iter()
            .find(|p| p.piece == "rail_bend_120_center")
            .unwrap();
        let s = *l
            .iter()
            .find(|p| p.piece == "rail_straight_center" && p.cell == Axial::new(-3, 6))
            .unwrap();
        // The bend's lanes are local edges 0 and 2, the straight's 0 and 3, turned by rot.
        let world = |p: &Placement, local: &[u8]| -> Vec<u8> {
            local.iter().map(|e| (e + p.rot) % 6).collect()
        };
        assert!(
            world(&b, &[0, 2]).into_iter().any(
                |e| b.cell.neighbor(e) == s.cell && world(&s, &[0, 3]).contains(&((e + 3) % 6))
            ),
            "the wide bend meets no lane of the straight on (-3, 6)"
        );
    }

    #[test]
    fn rail_crossing_continues_the_straight() {
        let l = kit_layout();
        let at = |id: &str| *l.iter().find(|p| p.piece == id).unwrap();
        let (s, x) = (at("rail_straight_center"), at("rail_road_crossing_center"));
        // The crossing's rail lane on its local edge 0 meets the straight's edge-3 lane.
        assert_eq!(x.cell.neighbor(x.rot), s.cell);
        assert_eq!(s.cell.neighbor((s.rot + 3) % 6), x.cell);
        // Road stubs on the crossing's road lanes (local edges 1 and 4).
        for k in [1, 4] {
            let e = (k + x.rot) % 6;
            assert!(
                l.iter()
                    .any(|p| p.piece == "road_straight_edge" && p.cell == x.cell && p.rot == e)
            );
        }
    }

    #[test]
    fn rail_junction_meets_rail_on_all_three_lanes() {
        let l = kit_layout();
        let j = *l
            .iter()
            .find(|p| p.piece == "rail_junction_center")
            .unwrap();
        // The junction's lanes are local edges 0 (toe), 1 (branch) and 3 (straight), turned by rot.
        let rail_lanes = |p: &Placement| -> Vec<u8> {
            let local: &[u8] = match p.piece {
                "rail_straight_center" => &[0, 3],
                "rail_bend_60_center" => &[0, 1],
                "rail_junction_center" => &[0, 1, 3],
                _ => &[],
            };
            local.iter().map(|e| (e + p.rot) % 6).collect()
        };
        for e in rail_lanes(&j) {
            let n = j.cell.neighbor(e);
            let back = (e + 3) % 6;
            assert!(
                l.iter()
                    .any(|p| p.cell == n && rail_lanes(p).contains(&back)),
                "junction lane on world edge {e} meets no rail lane at {n:?}"
            );
        }
    }

    #[test]
    fn buffer_stops_stand_at_open_rail_ends() {
        let l = kit_layout();
        let stops: Vec<_> = l.iter().filter(|p| p.piece == "rail_buffer_stop").collect();
        assert_eq!(stops.len(), 1);
        for s in stops {
            // The stop stands at its rail piece's local edge 0, which faces a cell with no rail.
            let rail = l
                .iter()
                .find(|p| {
                    p.cell == s.cell
                        && p.rot == s.rot
                        && matches!(p.piece, "rail_straight_center" | "rail_bend_60_center")
                })
                .unwrap_or_else(|| panic!("no rail piece under the stop at {:?}", s.cell));
            let beyond = rail.cell.neighbor(rail.rot % 6);
            assert!(
                !l.iter()
                    .any(|p| p.cell == beyond && p.piece.starts_with("rail_")),
                "the stop at {:?} faces rail at {beyond:?}",
                s.cell
            );
            assert!(
                l.iter()
                    .any(|p| p.piece == "train_engine" && p.cell == s.cell),
                "no engine at the stop at {:?}",
                s.cell
            );
        }
    }

    #[test]
    fn rail_bridge_meets_rail_and_river_on_its_lanes() {
        let l = kit_layout();
        let bridge = *l
            .iter()
            .find(|p| p.piece == "river_rail_bridge_center")
            .unwrap();
        // Rail lanes are the bridge's local edges 1 and 4: a rail straight meets each.
        for k in [1, 4] {
            let e = (k + bridge.rot) % 6;
            let n = bridge.cell.neighbor(e);
            assert!(
                l.iter().any(|p| p.piece == "rail_straight_center"
                    && p.cell == n
                    && [p.rot % 6, (p.rot + 3) % 6].contains(&((e + 3) % 6))),
                "bridge rail lane on world edge {e} meets no rail straight at {n:?}"
            );
        }
        // River lanes are local 0 and 3: the bridge and its upstream and downstream neighbours each
        // carry river_straight_base and river_edge stubs at the same rotation.
        for c in [
            bridge.cell,
            bridge.cell.neighbor(bridge.rot),
            bridge.cell.neighbor((bridge.rot + 3) % 6),
        ] {
            assert!(
                l.iter().any(|p| p.piece == "river_straight_base"
                    && p.cell == c
                    && p.rot == bridge.rot)
            );
            for k in [0, 3] {
                let e = (k + bridge.rot) % 6;
                assert!(
                    l.iter()
                        .any(|p| p.piece == "river_edge" && p.cell == c && p.rot == e)
                );
            }
        }
        // No river cell shares its cell with another base.
        for c in [
            bridge.cell,
            bridge.cell.neighbor(bridge.rot),
            bridge.cell.neighbor((bridge.rot + 3) % 6),
        ] {
            assert!(
                !l.iter()
                    .any(|p| p.cell == c && matches!(p.piece, "meadow_base" | "forest_base"))
            );
        }
    }

    /// The tight river bend's lanes are its local edges 0 and 1, each with a river_edge stub; one
    /// meets the river straight upstream of the rail bridge on that straight's lane, the other a
    /// river arm of the fork, and its cell holds no other base.
    #[test]
    fn river_bend_60_joins_the_straight_to_the_fork() {
        let l = kit_layout();
        let bend = *l.iter().find(|p| p.piece == "river_bend_60_base").unwrap();
        for k in [0, 1] {
            let e = (k + bend.rot) % 6;
            assert!(
                l.iter()
                    .any(|p| p.piece == "river_edge" && p.cell == bend.cell && p.rot == e),
                "no river_edge on world edge {e}"
            );
        }
        let met = [0u8, 1].into_iter().any(|k| {
            let e = (k + bend.rot) % 6;
            let n = bend.cell.neighbor(e);
            l.iter().any(|p| {
                p.piece == "river_straight_base"
                    && p.cell == n
                    && [p.rot % 6, (p.rot + 3) % 6].contains(&((e + 3) % 6))
            })
        });
        assert!(met, "the bend meets no river straight on its lanes");
        let fork = [0u8, 1].into_iter().any(|k| {
            let e = (k + bend.rot) % 6;
            let n = bend.cell.neighbor(e);
            l.iter().any(|p| {
                p.piece == "river_fork_base"
                    && p.cell == n
                    && [0u8, 2, 4]
                        .map(|j| (j + p.rot) % 6)
                        .contains(&((e + 3) % 6))
            })
        });
        assert!(fork, "the bend meets no fork arm on its lanes");
        assert!(l.iter().all(|p| p.cell != bend.cell
            || !matches!(
                p.piece,
                "meadow_base" | "forest_base" | "river_straight_base"
            )));
    }

    /// Owner A24 Q4: the showcase shows the beach and ocean props and the riverbank props, each
    /// on its own tile (the beach, the first ocean tile, the downstream river straight).
    #[test]
    fn showcase_places_the_coast_and_riverbank_props() {
        let l = kit_layout();
        let on = |piece: &str, cell: (i32, i32)| {
            l.iter()
                .any(|p| p.piece == piece && (p.cell.q, p.cell.r) == cell)
        };
        for piece in ["beach_pebbles", "beach_pebbles_b", "beach_pebbles_c"] {
            assert!(on(piece, (5, -1)) && on("beach_base", (5, -1)), "{piece}");
        }
        for piece in ["seabed_rock", "seabed_pebbles"] {
            assert!(on(piece, (1, 1)) && on("ocean_base", (1, 1)), "{piece}");
        }
        for piece in [
            "river_stones",
            "river_stones_b",
            "river_stones_c",
            "river_reeds",
            "river_reeds_b",
        ] {
            assert!(
                on(piece, (-2, 6)) && on("river_straight_base", (-2, 6)),
                "{piece}"
            );
        }
        for piece in [
            "lily_cluster",
            "lily_cluster_b",
            "lily_cluster_c",
            "reed_bed",
        ] {
            assert!(
                on(piece, (-2, 4)) && on("river_straight_base", (-2, 4)),
                "{piece}"
            );
        }
        let m = parse_manifest(MANIFEST).unwrap();
        let (_, warn) = resolve_placements(&m, &l);
        assert!(warn.is_empty(), "{warn:?}");
    }

    #[test]
    fn missing_piece_is_skipped_not_panic() {
        let m = parse_manifest(r#"{"version":1,"materials":{},"pieces":[]}"#).unwrap();
        let (ok, warn) = resolve_placements(&m, &kit_layout());
        assert!(ok.is_empty());
        assert!(warn.iter().any(|w| w.contains("meadow_base")));
    }

    #[test]
    fn fixture_source_resolves_every_placement() {
        let (json, layout) = source(KitSource::Fixture);
        let m = parse_manifest(json).unwrap();
        let (ok, warn) = resolve_placements(&m, &layout);
        assert!(warn.is_empty(), "{warn:?}");
        assert_eq!(ok.len(), layout.len());
        // Two fixtures share edge 0 of (0,0); the third sits on a rotated neighbour.
        assert_eq!(layout[0].cell.neighbor(0), layout[1].cell);
        assert!(layout.iter().any(|p| p.rot != 0));
    }

    #[test]
    fn fixture_tiled_source_binds_a_tileable_set() {
        let (json, layout) = source(KitSource::FixtureTiled);
        let m = parse_manifest(json).unwrap();
        let (ok, warn) = resolve_placements(&m, &layout);
        assert!(warn.is_empty() && ok.len() == layout.len(), "{warn:?}");
        let p = &m.pieces[0];
        let r = crate::kit_material::slot_requests(
            &m,
            p,
            &p.slots["fixture_cap"],
            hex_assets::TextureFamily::Png,
        )
        .unwrap();
        assert!(r.tile.is_some() && r.masks.is_some());
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/kit");
        use hex_assets::TextureFamily::{Astc, Bc7, Png};
        for f in [Png, Bc7, Astc] {
            let r = crate::kit_material::slot_requests(&m, p, &p.slots["fixture_cap"], f).unwrap();
            for mr in r.tile.unwrap().0 {
                assert!(repo.join(&mr.path).is_file(), "{} missing", mr.path);
            }
        }
    }

    #[test]
    fn runtime_source_is_the_kit_layout() {
        let (json, layout) = source(KitSource::Runtime);
        assert!(parse_manifest(json).is_ok());
        assert_eq!(layout, kit_layout());
    }

    /// The fixture files under `assets/kit/p1/fixture/` (served by `kit://`) are copies of the
    /// gate fixture and must not drift from the files `kit_gate` checks.
    #[test]
    fn served_fixture_matches_the_gate_fixture() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let gate = repo.join("crates/hex-render/tests/fixtures/kit/p1/fixture");
        let served = repo.join("assets/kit/p1/fixture");
        let mut n = 0;
        for e in std::fs::read_dir(&gate).unwrap() {
            let name = e.unwrap().file_name();
            let a = std::fs::read(gate.join(&name)).unwrap();
            let b = std::fs::read(served.join(&name))
                .unwrap_or_else(|e| panic!("assets/kit/p1/fixture/{name:?}: {e}"));
            assert!(a == b, "{name:?} differs from the gate fixture");
            n += 1;
        }
        assert_eq!(n, std::fs::read_dir(&served).unwrap().count());
    }

    /// The probe tileable set under `assets/kit/p1/fixture_tile/` ships in `dist/` (Trunk copies
    /// all of `assets/`, and web-smoke serves `kit_src=fixture_tiled` from it). Its provenance is
    /// the gate fixture's: every probe map is a byte copy of a `fixture.masks` file that the
    /// fixture's provenance lists in `derivedFiles` (and `kit_gate` hash-checks), and the folder
    /// holds nothing else besides its manifest.
    #[test]
    fn probe_set_is_a_copy_of_the_fixture_masks() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let gate = "crates/hex-render/tests/fixtures/kit/p1/fixture";
        let probe = repo.join("assets/kit/p1/fixture_tile");
        let prov: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                repo.join("crates/hex-render/tests/fixtures/kit/source/fixture/provenance.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let derived: Vec<&str> = prov["derivedFiles"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["path"].as_str())
            .collect();
        let mut want = std::collections::BTreeSet::from(["manifest.json".to_string()]);
        for family in ["bc7.ktx2", "astc.ktx2", "png"] {
            let src = format!("{gate}/fixture.masks.{family}");
            assert!(
                derived.contains(&src.as_str()),
                "{src} not in fixture provenance"
            );
            let bytes = std::fs::read(repo.join(&src)).unwrap();
            for channel in ["albedo", "normal", "orm"] {
                let name = format!("probe.{channel}.{family}");
                let copy = std::fs::read(probe.join(&name)).unwrap();
                assert!(copy == bytes, "{name} is not a copy of {src}");
                want.insert(name);
            }
        }
        let have: std::collections::BTreeSet<String> = std::fs::read_dir(&probe)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            have, want,
            "assets/kit/p1/fixture_tile holds unexpected files"
        );
    }
}
