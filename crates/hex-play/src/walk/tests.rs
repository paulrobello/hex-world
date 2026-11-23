use hex_play_state::locked;
use std::sync::{Mutex, OnceLock};

use hex_assets::parse_manifest;

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use bevy::prelude::*;
use hex_core::piece::{PieceTable, WalkClass};
use hex_core::{Axial, HEX_RADIUS, LEVEL_STEP, TileCatalog, TileDef, rail_end::BUFFER_STOP};

use super::{ground::*, shape::*, tables::*};
use crate::play::scatter::{Instance, layout, prepare};
use crate::{
    boids::plan::cell_at,
    play::{
        PlayTile,
        scatter::{self, Surface, TileScatter, Tri, turn},
    },
    water::tiles::WATER_Y,
};
use hex_render::kit_runtime::MANIFEST;
use hex_render::kit_runtime::piece_rotation;

type Tris = Option<Vec<(Tri, String)>>;

/// Every hero and scatter piece the catalog names, with its scatter group and hero variants.
pub(crate) fn catalog_props() -> std::collections::BTreeSet<String> {
    let c = hex_core::TileCatalog::builtin().unwrap();
    let mut pieces = std::collections::BTreeSet::new();
    for def in c.tiles() {
        let authored = def
            .props
            .hero
            .iter()
            .map(|h| h.piece.as_str())
            .chain(def.props.scatter.iter().map(|r| r.piece.as_str()));
        for piece in authored {
            for member in scatter::group(piece) {
                pieces.insert(member.to_owned());
                pieces.extend(
                    hex_core::hero_variants(member)
                        .iter()
                        .map(|v| (*v).to_owned()),
                );
            }
        }
    }
    pieces
}

/// Every prop piece the catalog places is either an obstacle, deliberately walk-through or a
/// ground prop, and no piece is in two lists. The catalog set misses the pieces `hex_core::boundary`
/// and the rail layout place at run time, which are obstacles.
#[test]
fn every_prop_piece_is_classified() {
    let list = |w| PieceTable::builtin().with_walk(w).collect::<Vec<&str>>();
    let lists: [(&str, Vec<&str>); 3] = [
        ("Obstacle", list(WalkClass::Obstacle)),
        ("WalkThrough", list(WalkClass::WalkThrough)),
        ("Ground", list(WalkClass::Ground)),
    ];
    for (i, (name_a, a)) in lists.iter().enumerate() {
        for (name_b, b) in &lists[i + 1..] {
            let both: Vec<&&str> = a.iter().filter(|p| b.contains(p)).collect();
            assert!(both.is_empty(), "{name_a} and {name_b} both list {both:?}");
        }
    }
    let placed_at_run_time: Vec<&str> = hex_core::boundary::PIECES
        .iter()
        .copied()
        .chain(["rail_buffer_stop"])
        .collect();
    let classified: std::collections::BTreeSet<String> = lists
        .iter()
        .flat_map(|(_, l)| l.iter())
        .filter(|p| !placed_at_run_time.contains(p))
        .map(|p| (*p).to_owned())
        .collect();
    let placed = catalog_props();
    let unclassified: Vec<_> = placed.difference(&classified).collect();
    let stale: Vec<_> = classified.difference(&placed).collect();
    assert!(
        unclassified.is_empty() && stale.is_empty(),
        "placed but in no list: {unclassified:?}; listed but never placed: {stale:?}"
    );
    for piece in placed_at_run_time {
        assert!(is_obstacle(piece), "{piece} blocks the walker");
    }
}

/// The windmill's trunk collider blocks the walker as a building, the sails do not reach it.
#[test]
fn windmill_blocks_as_a_building() {
    assert_eq!(Blocked::Obstacle("windmill".into()).reason(), "building");
    let hull = hull(
        &mesh_tris("windmill", "collider")
            .unwrap()
            .iter()
            .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
            .collect::<Vec<_>>(),
    );
    let reach = hull.iter().map(|p| p.length()).fold(0.0, f32::max);
    assert!((2.5..3.2).contains(&reach), "trunk reach {reach}");
}

/// Owner 2026-10-19 ("fix the labels"): bales are hay, the cow and sheep are animals, the
/// windmill and the bakery are buildings, and walking into a prop reports what a click on it
/// does. Every obstacle must match a row of the table below (a trailing `*` matches a prefix),
/// so a new obstacle piece cannot fall through to `rock` unnoticed: it fails here until a row
/// names its kind.
#[test]
fn every_obstacle_reports_its_own_kind() {
    const KINDS: &[(&str, &str)] = &[
        ("house_*", "building"),
        ("barn_*", "building"),
        ("windmill*", "building"),
        ("bakery_shop*", "building"),
        ("rail_station_shelter", "building"),
        ("tree_*", "tree"),
        ("sapling_*", "tree"),
        ("shrub_*", "tree"),
        ("bush_*", "tree"),
        ("hedge_*", "tree"),
        ("hay_*", "hay"),
        ("animal_*", "animal"),
        ("log_*", "log"),
        ("beach_driftwood*", "log"),
        ("fence_*", "fence"),
        ("wall_*", "fence"),
        ("rail_buffer_stop", "buffer_stop"),
        ("well_*", "well"),
        ("rock_*", "rock"),
        ("river_stones*", "rock"),
        ("boulder_granite*", "rock"),
        ("outcrop_granite*", "rock"),
    ];
    let kind_of = |piece: &str| {
        KINDS.iter().find_map(|&(pattern, kind)| {
            let hit = match pattern.strip_suffix('*') {
                Some(prefix) => piece.starts_with(prefix),
                None => piece == pattern,
            };
            hit.then_some(kind)
        })
    };
    let unlisted: Vec<_> = obstacles()
        .filter(|piece| kind_of(piece).is_none())
        .collect();
    assert!(
        unlisted.is_empty(),
        "obstacles no KINDS row covers: {unlisted:?}"
    );
    for piece in obstacles() {
        let kind = kind_of(piece).unwrap();
        assert_eq!(prop_kind(piece).as_str(), kind, "{piece}");
        assert_eq!(Blocked::Obstacle(piece.into()).reason(), kind, "{piece}");
    }
}

/// `prop_kind` has no default arm, so every piece the walker can hit or a click can pick has to
/// name its label: the obstacles, the centre and footprint parts, the viaduct piers, the
/// boats and the landing stage. Only rocks report `rock`.
#[test]
fn every_collider_piece_has_a_label() {
    let parts = CENTRE_OBSTACLES
        .iter()
        .map(|(_, part, _)| *part)
        .chain(FOOTPRINT_OBSTACLES.iter().map(|(_, part, _)| *part));
    let pieces: Vec<&str> = obstacles()
        .chain(parts)
        .chain(["boat_rowing", "boat_sail", "landing_stage"])
        .collect();
    for piece in pieces {
        let label = prop_kind(piece);
        let is_rock = piece.starts_with("rock_")
            || piece.starts_with("river_stones")
            || piece.starts_with("boulder_granite")
            || piece.starts_with("outcrop_granite")
            || piece == VIADUCT_PIER
            || FOOTPRINT_OBSTACLES
                .iter()
                .any(|(_, part, _)| *part == piece);
        assert_eq!(label == PropLabel::Rock, is_rock, "{piece} is {label:?}");
    }
    assert_eq!(prop_kind("rail_lever"), PropLabel::Lever);
}

/// Only the paddock fence's collider has disjoint parts (one box per run) and only
/// `barn_red_b` declares its own ([`DECLARED_PARTS`], seven), as do the wells (one roof
/// footprint for `well_stone`, seven for `well_stone_b`); every other obstacle is one part, so
/// it keeps the whole collider's hull and body exactly as before.
#[test]
fn only_the_fence_collider_splits_into_parts() {
    for piece in obstacles() {
        let s = shape(piece);
        if piece.starts_with("well_") {
            let n = if piece == "well_stone" { 1 } else { 7 };
            assert_eq!(s.parts.len(), n, "{piece}");
            for p in &s.parts {
                assert_eq!(p.hull.len(), 4, "{:?}", p.hull);
                let (foot, top) = p.body.unwrap();
                assert!((foot + 0.35).abs() < 1e-3 && top >= 0.9, "{foot}..{top}");
            }
        } else if piece == "barn_red_b" {
            assert_eq!(
                s.parts.len(),
                7,
                "the closed barn, two shed end walls, four posts"
            );
            for p in &s.parts {
                assert_eq!(p.hull.len(), 4, "{:?}", p.hull);
                let (foot, top) = p.body.unwrap();
                assert!((foot + 0.35).abs() < 1e-3 && top > 2.6, "{foot}..{top}");
            }
        } else if piece == "fence_paddock" {
            assert_eq!(s.parts.len(), 6, "one part per run");
            for p in &s.parts {
                assert_eq!(p.hull.len(), 4, "a run box: {:?}", p.hull);
                // The box runs from 0.3 m under the run's lowest ground (the meadow relief, at
                // most 0 and level with the ring on most of the paddock) to 5 cm over its
                // tallest post.
                let (foot, top) = p.body.unwrap();
                assert!(
                    foot <= -0.3 + 1e-4 && (1.1..1.5).contains(&top),
                    "{foot}..{top}"
                );
            }
        } else if piece.ends_with("_bend") || piece.ends_with("_junction") {
            // A field boundary's corner: one box per arm (B2), so the walker is not stopped in
            // the wedge between two arms.
            let n = if piece.ends_with("_bend") { 2 } else { 3 };
            assert_eq!(s.parts.len(), n, "{piece}: one part per arm");
            for p in &s.parts {
                assert_eq!(p.hull.len(), 4, "{piece}: an arm box {:?}", p.hull);
                let (foot, top) = p.body.unwrap();
                assert!(
                    foot.abs() < 1e-3 && (1.05..1.7).contains(&top),
                    "{piece}: {foot}..{top}"
                );
            }
        } else {
            assert!(s.parts.is_empty(), "{piece}: {} parts", s.parts.len());
            assert_eq!(s.footprints(), vec![(s.hull.as_slice(), None)], "{piece}");
        }
    }
    // The fallback blocks nothing for a fence, a disc for anything else.
    assert!(PieceShape::fallback("fence_paddock").hull.is_empty());
    assert_eq!(PieceShape::fallback("rock_small").hull.len(), 16);
}

/// The farm's paddock fence (tile frame, rotation 0): the walker passes through the 3 m gate
/// between x 3.5 and 6.5, stops at the north run (z 2.38 to 2.62) a body radius short and
/// reports `fence`, stands inside the paddock, and a click on a run picks the fence.
#[test]
fn the_paddock_fence_blocks_except_at_its_gate() {
    let g = world(&[(0, 0, "farm", 0)]);
    let fences: Vec<&Obstacle> = g.obstacles[&Axial::ORIGIN]
        .iter()
        .filter(|o| o.piece == "fence_paddock")
        .collect();
    assert_eq!(fences.len(), 6, "one obstacle per run");
    let fence_at = |p: Vec2| fences.iter().any(|o| o.hits(p, BODY_RADIUS));
    // Through the gate: no fence part on the line x = 5, z 0 to 5.
    for k in 0..=50 {
        let p = Vec2::new(5.0, k as f32 * 0.1);
        assert!(!fence_at(p), "a fence part at {p}");
    }
    let s = walk(&g, Vec2::new(5.0, 0.0), Vec2::Y, 5.0);
    assert_eq!(s.blocked, None, "through the gate");
    assert!(s.pos.z > 4.9, "through the gate to {}", s.pos);
    // Into the north run from the yard.
    let s = walk(&g, Vec2::new(-5.0, 0.0), Vec2::Y, 5.0);
    assert_eq!(s.blocked, Some(Blocked::Obstacle("fence_paddock".into())));
    assert_eq!(s.blocked.as_ref().map(Blocked::reason), Some("fence"));
    assert!(
        (1.95..2.09).contains(&s.pos.z),
        "stopped at z {} (x {})",
        s.pos.z,
        s.pos.x
    );
    // Inside the paddock.
    let inside = Vec2::new(-5.0, 6.0);
    assert!(!fence_at(inside));
    assert!(g.stand(inside).is_ok(), "{:?}", g.stand(inside));
    // A click on the north run, at rail height from the yard.
    let eye = Vec3::new(-5.0, 0.8, -1.0);
    let (t, piece) = g.pick_prop(ray(eye, Vec3::Z), 1000.0).unwrap();
    assert_eq!(
        (piece, prop_kind(piece).as_str()),
        ("fence_paddock", "fence")
    );
    assert!((3.35..3.45).contains(&t), "hit {t} m out");
    // Through the gate, at rail height, the first fence hit is the far north-east run's
    // (z 11.27 at x 5).
    let eye = Vec3::new(5.0, 0.8, -1.0);
    let (t, piece) = g.pick_prop(ray(eye, Vec3::Z), 1000.0).unwrap();
    assert_eq!(piece, "fence_paddock");
    assert!((11.9..12.3).contains(&t), "hit {t} m out: through the gate");
}

/// Owner A31 Q4: field boundaries block the walker, a closed gate blocks, and an edge with no
/// boundary (the farm's edge to the wheat field) lets it through. The review world at seed 1
/// (`play::boundaries::tests::REVIEW`, the farm at rotation 0, so its frame is the world's
/// turned by nothing): every run on the yard side (z < 2, clear of the paddock) stops a walker
/// coming at it square from 3.5 m inside the farm, by the run's own piece.
#[test]
fn walls_hedges_and_the_closed_gate_block_and_an_open_edge_passes() {
    let mut tiles = vec![(0, 0, "meadow", 0)];
    tiles.extend(crate::play::boundaries::tests::REVIEW);
    let g = world(&tiles);
    let c = catalog();
    let defs: HashMap<Axial, (&TileDef, u8)> = tiles
        .iter()
        .map(|&(q, r, id, rot)| (Axial::new(q, r), (c.get(id).unwrap(), rot)))
        .collect();
    let at = |x: Axial| defs.get(&x).copied();
    let farm = centre(1, 0);
    let runs: Vec<_> = crate::play::boundaries::placements(1, Axial::new(1, 0), &at, &|_| 0)
        .into_iter()
        .filter(|p| p.piece.contains("_run") || p.piece == "wall_gate" || p.piece == "hedge_oak")
        .filter(|p| p.at[1] < 2.0)
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for p in &runs {
        let spot = farm + Vec2::from(p.at);
        let inward = (farm - spot).normalize();
        let s = walk(&g, spot + inward * 3.5, -inward, 5.0);
        let piece = match &s.blocked {
            Some(Blocked::Obstacle(x)) => x.as_str(),
            other => panic!("{} at {:?}: {other:?}", p.piece, p.at),
        };
        assert_eq!(piece, p.piece, "the walker meets the run it walks at");
        let half = if p.piece.starts_with("wall") {
            0.35
        } else {
            0.62
        };
        let gap = (s.pos.xz() - spot).dot(inward);
        assert!(
            (half..half + BODY_RADIUS + 0.12).contains(&gap),
            "{}: stopped {gap} m short",
            p.piece
        );
        seen.insert(
            p.piece.split('_').next().unwrap().to_owned()
                + if p.piece == "wall_gate" { "_gate" } else { "" },
        );
    }
    // The yard side holds the gate (its approach clear of the paddock and barn) and walls at seed 1.
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        ["wall", "wall_gate"],
        "{runs:#?}"
    );
    // From the land side, every hedge run stops a walker coming at it square from 1.8 m out
    // (one standing clear of the neighbour's trees).
    let mut hedges = 0;
    for p in crate::play::boundaries::placements(1, Axial::new(1, 0), &at, &|_| 0)
        .iter()
        .filter(|p| p.piece.starts_with("hedge_") && p.piece != "hedge_end")
    {
        let spot = farm + Vec2::from(p.at);
        let out = (spot - farm).normalize();
        let start = spot + out * 1.8;
        if g.stand(start).is_err() {
            continue;
        }
        let s = walk(&g, start, -out, 3.0);
        match &s.blocked {
            Some(Blocked::Obstacle(x)) if x.starts_with("hedge_") => hedges += 1,
            Some(Blocked::Obstacle(x)) if !x.starts_with("wall_") => {}
            other => panic!("{} at {:?}: {other:?}", p.piece, p.at),
        }
    }
    assert!(hedges >= 1, "no hedge run checked");
    assert_eq!(prop_kind("wall_gate").as_str(), "fence");
    assert_eq!(prop_kind("hedge_run").as_str(), "tree");
    // Edge 0 has no boundary (farm and wheat are both fields): the walker crosses it.
    let s = walk(&g, farm + Vec2::new(6.0, 1.5), Vec2::X, 12.0);
    assert!(
        !matches!(&s.blocked, Some(Blocked::Obstacle(p)) if p.starts_with("wall_") || p.starts_with("hedge_")),
        "{:?}",
        s.blocked
    );
    assert!(
        s.pos.x - farm.x > 13.86,
        "crossed into the wheat field: {}",
        s.pos
    );
}

/// A placed windmill tile stops the walker at its tower, from every side and through its turn.
#[test]
fn a_placed_windmill_tile_blocks_at_the_tower() {
    let mut tested = 0;
    for rotation in [0, 3] {
        let g = world(&[(0, 0, "windmill", rotation)]);
        for k in 0..6 {
            let a = (60.0 * k as f32 + 30.0).to_radians();
            let start = Vec2::new(a.cos(), a.sin()) * 8.0;
            if g.stand(start).is_err() {
                continue;
            }
            let s = walk(&g, start, -start, 8.0);
            let at = s.pos.xz().length();
            if s.blocked == Some(Blocked::Obstacle("rock_small".into())) {
                continue;
            }
            assert_eq!(
                s.blocked,
                Some(Blocked::Obstacle("windmill".into())),
                "r{rotation} side {k}: stopped at {at} m"
            );
            assert!((2.4..3.6).contains(&at), "r{rotation} side {k}: {at} m out");
            tested += 1;
        }
    }
    assert!(tested >= 8, "only {tested} approaches reached the tower");
}

/// A placed farm tile stops the walker at the barn's front wall as a building, through the
/// tile's turn, and its animals (at their slots before the herd ticks) and bales block too.
#[test]
fn a_placed_farm_tile_blocks_at_the_barn() {
    assert_eq!(Blocked::Obstacle("barn_red".into()).reason(), "building");
    assert_eq!(prop_kind("barn_red").as_str(), "building");
    let def = catalog().get("farm").unwrap().clone();
    let barn = def
        .props
        .hero
        .iter()
        .find(|h| h.piece.as_str() == "barn_red")
        .unwrap();
    let yaw = Quat::from_rotation_y(-barn.yaw_deg.to_radians());
    let centre = Vec3::new(barn.at[0], 0.0, barn.at[1]);
    // The barn's long front wall faces its local +z, 3.85 m out (collider half depth).
    let mut tested = 0;
    for rotation in [0, 3] {
        let g = world(&[(0, 0, "farm", rotation)]);
        let turn = piece_rotation(rotation);
        let front = (turn * yaw * Vec3::Z).xz();
        for along in [-3.0, 0.0, 3.0] {
            let local = centre + yaw * Vec3::new(along, 0.0, 8.0);
            let start = (turn * local).xz();
            if g.stand(start).is_err() {
                continue;
            }
            let s = walk(&g, start, -front, 8.0);
            if s.blocked == Some(Blocked::Obstacle("rock_small".into())) {
                continue;
            }
            let out = (s.pos.xz() - (turn * centre).xz()).dot(front);
            assert_eq!(
                s.blocked,
                Some(Blocked::Obstacle("barn_red".into())),
                "r{rotation} at {along}: stopped {out} m out"
            );
            assert!(
                (3.8..4.4).contains(&out),
                "r{rotation} at {along}: {out} m out"
            );
            tested += 1;
        }
    }
    assert!(tested >= 4, "only {tested} approaches reached the barn");
    // The cow stands in the pasture: walking at its flank from 4 m stops against it. The
    // grazing cow stands off its other flank.
    let cow = def
        .props
        .hero
        .iter()
        .find(|h| h.piece.as_str() == "animal_cow")
        .unwrap();
    let at = Vec2::from(cow.at);
    let side = (Quat::from_rotation_y(-cow.yaw_deg.to_radians()) * Vec3::X).xz() * 4.0;
    let g = world(&[(0, 0, "farm", 0)]);
    let s = walk(&g, at - side, side, 4.0);
    assert_eq!(s.blocked, Some(Blocked::Obstacle("animal_cow".into())));
    assert_eq!(s.blocked.as_ref().map(Blocked::reason), Some("animal"));
    assert!(s.pos.xz().distance(at) < 1.0, "{}", s.pos.xz().distance(at));
}

/// A farm at (0, 1) draws barn_red_b in the barn slot (`hex_core::hero_piece`). Its collider
/// follows the closed walls (owner A1-Q1, 2026-10-20, [`DECLARED_PARTS`]): from the yard the
/// walker passes the post line into the open shed and stops at its back wall (1.0 m out from
/// the barn's centre, plus the body radius), a walk at a post stops at the post (3.55 m out),
/// and inside the shed the end walls stop it (inner faces 4.9 m from the centre), each as
/// barn_red_b, a building.
#[test]
fn a_farm_drawing_barn_red_b_lets_the_walker_into_its_shed() {
    let def = catalog().get("farm").unwrap().clone();
    let cell = Axial::new(0, 1);
    assert_eq!(hex_core::hero_piece(&def, cell, 0), "barn_red_b");
    let barn = &def.props.hero[0];
    let yaw = Quat::from_rotation_y(-barn.yaw_deg.to_radians());
    let centre_local = Vec3::new(barn.at[0], 0.0, barn.at[1]);
    let blocked_by_barn = |s: &Step, what: &str| {
        assert_eq!(
            s.blocked,
            Some(Blocked::Obstacle("barn_red_b".into())),
            "{what}"
        );
        assert_eq!(s.blocked.as_ref().map(Blocked::reason), Some("building"));
    };
    let mut tested = 0;
    for rotation in [0, 3] {
        let g = world(&[(cell.q, cell.r, "farm", rotation)]);
        let turn = piece_rotation(rotation);
        let at = |x: f32, z: f32| {
            (turn * (centre_local + yaw * Vec3::new(x, 0.0, z))).xz() + centre(cell.q, cell.r)
        };
        let dir = |x: f32, z: f32| (turn * yaw * Vec3::new(x, 0.0, z)).xz();
        let local = |p: Vec2| {
            let d = p - at(0.0, 0.0);
            Vec2::new(d.dot(dir(1.0, 0.0)), d.dot(dir(0.0, 1.0)))
        };
        for (along, stop) in [(-3.0, 1.3), (0.0, 1.3), (3.0, 1.3), (1.633, 3.85)] {
            let start = at(along, 8.0);
            if g.stand(start).is_err() {
                continue;
            }
            let s = walk(&g, start, -dir(0.0, 1.0), 8.0);
            if s.blocked == Some(Blocked::Obstacle("rock_small".into())) {
                continue;
            }
            let out = local(s.pos.xz()).y;
            let what = format!("r{rotation} at {along}: stopped {out} m out");
            blocked_by_barn(&s, &what);
            assert!(
                (stop - 0.1..stop + 0.1).contains(&out),
                "{what}, want {stop}"
            );
            tested += 1;
        }
        for side in [1.0, -1.0] {
            let s = walk(&g, at(0.0, 2.2), dir(side, 0.0), 6.0);
            let x = local(s.pos.xz()).x;
            let what = format!("r{rotation} in the shed toward {side}: stopped at x {x}");
            blocked_by_barn(&s, &what);
            assert!((4.5..4.7).contains(&(x * side)), "{what}");
        }
    }
    assert!(tested >= 6, "only {tested} approaches reached the barn");
}

/// [`DECLARED_PARTS`] are `barn_red_b`'s own walls and posts: every part inside its GLB
/// collider box, the closed barn's yard face on the LOD0 wall at z 1.0, the shed end walls on
/// the LOD0 inner faces at x +-4.9, and each post's corners on its LOD0 trim box.
#[test]
fn barn_red_b_parts_match_its_glb() {
    let collider = mesh_tris("barn_red_b", "collider").unwrap();
    let lod0 = mesh_tris("barn_red_b", "lod0").unwrap();
    let pts = |slot: &str| -> Vec<Vec3> {
        lod0.iter()
            .filter(|(_, s)| s == slot)
            .flat_map(|(t, _)| t.iter().map(|p| Vec3::from(*p)))
            .collect()
    };
    let (lo, hi) = collider
        .iter()
        .flat_map(|(t, _)| t.iter())
        .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
            (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p)))
        });
    let parts = declared_parts("barn_red_b").unwrap();
    for &[x0, x1, z0, z1, top] in parts {
        assert!(
            x0 >= lo.x && x1 <= hi.x && z0 >= lo.z && z1 <= hi.z,
            "{x0}..{x1} {z0}..{z1}"
        );
        assert!(top <= hi.y + 0.4, "{top}");
    }
    let walls = pts("barn_red_b_walls");
    let near = |v: &[Vec3], x: f32, z: f32| {
        v.iter()
            .any(|p| (p.x - x).abs() < 0.01 && (p.z - z).abs() < 0.01)
    };
    let [x0, x1, _, z1, _] = parts[0];
    assert!(
        near(&walls, -4.9, z1) && near(&walls, 4.9, z1),
        "the shed's back wall at z {z1}"
    );
    assert!(x0 <= -5.0 && x1 >= 5.0, "the gables at x +-5.0");
    for &[x0, x1, z0, z1, _] in &parts[1..3] {
        let inner = if x0 > 0.0 { x0 } else { x1 };
        assert!(
            near(&walls, inner, z0) && near(&walls, inner, z1),
            "end wall at x {inner}"
        );
    }
    let trim = pts("barn_red_b_trim");
    for &[x0, x1, z0, z1, _] in &parts[3..] {
        assert!(
            near(&trim, x0, z0) && near(&trim, x1, z1),
            "post {x0}..{x1}"
        );
    }
}

/// The wells' [`DECLARED_PARTS`] sit inside their GLB collider (the apron's hull, which grass
/// and scatter keep clear) and hold, within 2 cm, every LOD0 vertex above the apron: the wall,
/// the capstones, posts, windlass, crank, bucket and (`well_stone`) the roof. A rebuild that
/// moves any of them out fails here.
#[test]
fn well_parts_hold_the_well_above_its_apron() {
    for piece in ["well_stone", "well_stone_b"] {
        let collider = mesh_tris(piece, "collider").unwrap();
        let (lo, hi) = collider
            .iter()
            .flat_map(|(t, _)| t.iter())
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
                (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p)))
            });
        let parts = declared_parts(piece).unwrap();
        for &[x0, x1, z0, z1, top] in parts {
            assert!(
                x0 >= lo.x && x1 <= hi.x && z0 >= lo.z && z1 <= hi.z,
                "{piece}: {x0}..{x1} {z0}..{z1}"
            );
            assert!(top <= hi.y + 0.1, "{piece}: {top}");
        }
        for (t, slot) in mesh_tris(piece, "lod0").unwrap() {
            for p in t.iter().filter(|p| p[1] > 0.1) {
                let held = parts.iter().any(|&[x0, x1, z0, z1, _]| {
                    (x0 - 0.02..=x1 + 0.02).contains(&p[0])
                        && (z0 - 0.02..=z1 + 0.02).contains(&p[2])
                });
                assert!(held, "{piece}: {slot} vertex {p:?} outside its parts");
            }
        }
    }
}

/// A house cluster's well stops the walker at its parts and reports `well`: walking in from
/// 2.6 m behind it, the walker stops on the apron (inside its 1.385 m rim), a body radius short
/// of the eaves (`well_stone`, x -0.9) or the wall (`well_stone_b`, x -0.79).
#[test]
fn the_well_stops_the_walker_on_its_apron() {
    let c = catalog();
    let def = c.get("house_cluster").unwrap();
    let slot = def
        .props
        .hero
        .iter()
        .position(|h| h.piece.as_str() == "well_stone")
        .unwrap();
    let h = &def.props.hero[slot];
    let axis = Vec2::new(h.yaw_deg.to_radians().cos(), h.yaw_deg.to_radians().sin());
    for (piece, back) in [("well_stone", 0.9), ("well_stone_b", 0.79)] {
        let cell = (-4..=4)
            .flat_map(|q| (-4..=4).map(move |r| Axial::new(q, r)))
            .find(|&a| hex_core::hero_piece(def, a, slot) == piece)
            .unwrap();
        let g = world(&[(cell.q, cell.r, "house_cluster", 0)]);
        let well = centre(cell.q, cell.r) + Vec2::from(h.at);
        let s = walk(&g, well - axis * 2.6, axis, 2.6);
        assert_eq!(
            s.blocked,
            Some(Blocked::Obstacle(piece.into())),
            "{piece} at {cell:?}"
        );
        assert_eq!(s.blocked.unwrap().reason(), "well");
        let d = s.pos.xz().distance(well);
        assert!(
            d >= back + BODY_RADIUS - 0.06 && d < 1.385,
            "{piece}: stopped {d} m from its centre"
        );
    }
}

/// Owner 2026-10-19 ("add the collider"): a bakery hero blocks the walker at its collider's
/// wall and reports `building`, and photo mode's click picks it. A fixture tile stands in for
/// the bakery tile.
#[test]
fn a_bakery_hero_blocks_the_walker_and_is_picked() {
    let builtin = catalog();
    let mut def = builtin.get("meadow").unwrap().clone();
    def.id = "bakery_fixture".into();
    def.props.scatter.clear();
    def.props.hero = vec![hex_core::HeroProp {
        piece: hex_core::KitRef::from("bakery_shop"),
        at: [2.0, 1.0],
        yaw_deg: 30.0,
        y: 0.0,
    }];
    let mut tiles = builtin.tiles().to_vec();
    tiles.push(def.clone());
    let c = TileCatalog::new(builtin.sockets().clone(), tiles).unwrap();
    let yaw = Quat::from_rotation_y(-def.props.hero[0].yaw_deg.to_radians());
    let centre = Vec3::new(2.0, 0.0, 1.0);
    let b = scatter::ground_box("bakery_shop").unwrap();
    for rotation in [0, 2] {
        let g = world_in(&c, &[(0, 0, "bakery_fixture", rotation)]);
        let turn = piece_rotation(rotation);
        let hull = &g.obstacles[&Axial::new(0, 0)][0];
        assert_eq!(hull.piece, "bakery_shop");
        let local = shape("bakery_shop").hull;
        let lo = local.iter().fold(Vec2::MAX, |a, p| a.min(*p));
        let hi = local.iter().fold(Vec2::MIN, |a, p| a.max(*p));
        assert!(
            lo.distance(Vec2::from(b.min)) < 0.01 && hi.distance(Vec2::from(b.max)) < 0.01,
            "the collider hull {lo}..{hi}"
        );
        assert_eq!(hull.poly.len(), local.len(), "the collider hull, placed");
        // The bakery's +x wall, `b.max[0]` out, from 8 m beyond it.
        let side = (turn * yaw * Vec3::X).xz();
        let at = (turn * centre).xz();
        for along in [-2.0, 0.0, 2.0] {
            let cross = (turn * yaw * Vec3::Z).xz() * along;
            let s = walk(&g, at + side * 8.0 + cross, -side, 8.0);
            assert_eq!(
                s.blocked,
                Some(Blocked::Obstacle("bakery_shop".into())),
                "r{rotation} at {along}"
            );
            assert_eq!(s.blocked.as_ref().map(Blocked::reason), Some("building"));
            let out = (s.pos.xz() - at).dot(side);
            assert!(
                (b.max[0]..b.max[0] + 0.6).contains(&out),
                "r{rotation} at {along}: stopped {out} m out"
            );
        }
        let target = Vec3::new(at.x, 2.0, at.y);
        let eye = target + Vec3::new(side.x, 0.0, side.y) * 30.0;
        let (t, piece) = g.pick_prop(ray(eye, target - eye), 1000.0).unwrap();
        assert_eq!(
            (piece, prop_kind(piece).as_str()),
            ("bakery_shop", "building")
        );
        assert!(
            (30.0 - b.max[0] - 0.1..29.0).contains(&t),
            "r{rotation}: hit {} m out",
            30.0 - t
        );
    }
}

/// The walker crosses a wheat field along and across its rows without being blocked or lifted
/// onto the wheat: it walks on the ploughed earth, within a furrow's depth of the meadow.
#[test]
fn the_walker_walks_through_a_wheat_field() {
    let g = world(&[(0, 0, "wheat_field", 0)]);
    for dir in [Vec2::X, Vec2::Y, Vec2::new(1.0, 1.0)] {
        let from = -dir.normalize() * 11.5;
        let s = walk(&g, from, dir, 23.0);
        assert_eq!(s.blocked, None, "{dir}");
        assert!(
            (s.pos.xz() - (from + dir.normalize() * 23.0)).length() < 0.1,
            "{dir}"
        );
        for k in 0..=46 {
            let p = from + dir.normalize() * (k as f32 * 0.5);
            let y = g.ground(p).unwrap();
            assert!((-0.35..0.3).contains(&y), "{dir} at {p}: ground {y}");
        }
    }
}

fn catalog() -> TileCatalog {
    TileCatalog::builtin().unwrap()
}

fn glb(id: &str) -> Option<(gltf::Document, Vec<gltf::buffer::Data>)> {
    let m = parse_manifest(MANIFEST).unwrap();
    let e = m.pieces.iter().find(|p| p.id == id)?;
    let path = format!("{}/../../assets/kit/{}", env!("CARGO_MANIFEST_DIR"), e.glb);
    let bytes = std::fs::read(path).ok()?;
    let plain = hex_assets::rewrite_glb(&bytes).ok()?;
    let (doc, buffers, _) = gltf::import_slice(&plain).ok()?;
    Some((doc, buffers))
}

/// Triangles of mesh `<id>_<mesh>` with slot names, read once per mesh for the test binary.
pub(crate) fn mesh_tris(id: &str, mesh: &str) -> Tris {
    static CACHE: OnceLock<Mutex<HashMap<String, Tris>>> = OnceLock::new();
    let key = format!("{id}_{mesh}");
    let cache = CACHE.get_or_init(Default::default);
    if let Some(t) = locked(cache).get(&key) {
        return t.clone();
    }
    let read = || -> Tris {
        let (doc, buffers) = glb(id)?;
        let m = doc.meshes().find(|m| m.name() == Some(key.as_str()))?;
        let mut out = vec![];
        for prim in m.primitives() {
            let slot = prim.material().name().unwrap_or_default().to_string();
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let pos: Vec<[f32; 3]> = r.read_positions()?.collect();
            let idx: Vec<u32> = r.read_indices()?.into_u32().collect();
            for t in idx.as_chunks::<3>().0 {
                out.push((
                    [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]],
                    slot.clone(),
                ));
            }
        }
        Some(out)
    };
    let t = read();
    locked(cache).insert(key, t.clone());
    t
}

pub(crate) fn surface(def: &TileDef) -> Option<Arc<Surface>> {
    let all = crate::play::slope_ground::tile_ground_tris(def, &catalog(), &mut |id| {
        mesh_tris(id, "lod0")
    })?;
    Some(Arc::new(Surface::new(all)))
}

fn shape(piece: &str) -> PieceShape {
    let pts: Vec<Vec3> = mesh_tris(piece, "collider")
        .expect("collider mesh")
        .iter()
        .flat_map(|(t, _)| t.iter().map(|p| Vec3::from(*p)))
        .collect();
    PieceShape::from_meshes(piece, &pts, &mesh_tris(piece, "lod0").expect("lod0 mesh"))
}

/// A world of `tiles` with the real kit ground, colliders and seeded scatter.
pub(crate) fn world(tiles: &[(i32, i32, &str, u8)]) -> WalkGround {
    world_in(&catalog(), tiles)
}

pub(crate) fn world_in(c: &TileCatalog, tiles: &[(i32, i32, &str, u8)]) -> WalkGround {
    let at: Vec<_> = tiles.iter().map(|&(q, r, t, k)| (q, r, t, k, 0)).collect();
    world_at(c, &at)
}

/// [`world_in`] with each tile's level: (q, r, id, rotation, level).
pub(crate) fn world_at(c: &TileCatalog, tiles: &[(i32, i32, &str, u8, u8)]) -> WalkGround {
    world_with(c, tiles, &[])
}

/// [`world_at`] with `extra` scatter instances (tile frame) added to the tiles at their cells.
fn world_with(
    c: &TileCatalog,
    tiles: &[(i32, i32, &str, u8, u8)],
    extra: &[(Axial, Instance)],
) -> WalkGround {
    let play: Vec<PlayTile> = tiles
        .iter()
        .map(|&(q, r, tile, rotation, level)| PlayTile {
            cell: Axial::new(q, r),
            tile: tile.into(),
            rotation,
            level,
        })
        .collect();
    let mut scatter = BTreeMap::new();
    for t in &play {
        let def = c.get(&t.tile).unwrap();
        if def.props.scatter.is_empty() {
            continue;
        }
        let prepared = prepare(def, c, &mut |id| mesh_tris(id, "lod0")).unwrap();
        let seed = hex_core::cell_seed(1, t.cell);
        let mut l = layout(def, seed, &prepared);
        l.instances.extend(
            extra
                .iter()
                .filter(|(cell, _)| *cell == t.cell)
                .map(|(_, i)| i.clone()),
        );
        scatter.insert(
            t.cell,
            TileScatter {
                tile: t.tile.clone(),
                seed,
                layout: Arc::new(l),
                fences: Arc::new(vec![]),
                cut: 0,
                drawn: vec![true; def.props.scatter.len()],
                pieces: def
                    .props
                    .scatter
                    .iter()
                    .map(|r| r.piece.as_str().to_owned())
                    .collect(),
            },
        );
    }
    WalkGround::build(
        &play,
        c,
        1,
        &[],
        &mut |d| {
            let s = TileSurface::new(d, c, &mut |id| mesh_tris(id, "lod0"))?;
            let ride = ride_max(d, &s.centre);
            Some((Arc::new(s), ride))
        },
        &mut |p| shape(p),
        &scatter,
    )
}

fn centre(q: i32, r: i32) -> Vec2 {
    Vec2::from(Axial::new(q, r).to_world(HEX_RADIUS))
}

/// Walks from `from` along `dir` for `dist` metres in 5 cm strides, as the controller does.
fn walk(g: &WalkGround, from: Vec2, dir: Vec2, dist: f32) -> Step {
    let mut pos = Vec3::new(from.x, g.ground(from).unwrap(), from.y);
    let mut last = None;
    for _ in 0..(dist / 0.05).round() as usize {
        let s = g.step(pos, dir.normalize() * 0.05);
        pos = s.pos;
        if s.blocked.is_some() {
            last = s.blocked;
        }
    }
    Step { pos, blocked: last }
}

#[test]
fn hull_is_convex_and_counter_clockwise() {
    let pts = [
        Vec2::new(0.0, 0.0),
        Vec2::new(2.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(2.0, 2.0),
        Vec2::new(0.0, 2.0),
    ];
    let h = hull(&pts);
    assert_eq!(h.len(), 4, "{h:?}");
    let o = Obstacle {
        piece: "rock_small".into(),
        poly: h,
    };
    assert!(o.hits(Vec2::new(1.0, 1.0), 0.0), "inside");
    assert!(
        o.hits(Vec2::new(2.2, 1.0), 0.3),
        "within the radius of an edge"
    );
    assert!(!o.hits(Vec2::new(2.4, 1.0), 0.3));
}

#[test]
fn ground_follows_the_meadow_road_beach_and_river_banks() {
    let g = world(&[
        (0, 0, "meadow", 0),
        (1, 0, "road_straight", 0),
        (0, 1, "beach", 0),
        (-1, 0, "river_straight", 0),
    ]);
    // Meadow: the cap stands at the border ring's height, give or take its relief.
    let m = g.ground(centre(0, 0) + Vec2::new(3.0, 2.0)).unwrap();
    assert!(m.abs() < 0.4, "meadow cap at {m}");
    // Road: the crown on the lane centre line, near the cap.
    let road = g.ground(centre(1, 0)).unwrap();
    assert!(road.abs() < 0.4, "road crown at {road}");
    // Beach: the sand falls toward the sea, from grass level to the waterline and below.
    let beach = (-12..=12)
        .filter_map(|i| {
            let p = centre(0, 1) + Vec2::new(0.0, i as f32);
            g.ground(p).ok().map(|h| (i, h))
        })
        .collect::<Vec<_>>();
    let lo = beach.iter().map(|b| b.1).fold(f32::MAX, f32::min);
    let hi = beach.iter().map(|b| b.1).fold(f32::MIN, f32::max);
    assert!(hi - lo > 0.8, "a beach slope: {beach:?}");
    // River: the banks fall from the cap toward the channel, and the deep channel is a ford
    // with the feet at wading depth.
    let c = centre(-1, 0);
    let bank_top = g.ground(c + Vec2::new(0.0, 10.0)).unwrap();
    let bank_low = g.ground(c + Vec2::new(0.0, 5.0)).unwrap();
    assert!(bank_top - bank_low > 0.3, "{bank_top} then {bank_low}");
    assert!(
        g.water_depth(c) > MAX_WADE,
        "the channel is deeper than a wade"
    );
    assert_eq!(g.ground(c), Ok(WATER_Y - MAX_WADE), "the channel centre");
}

/// Walk blocks at the piece the cell draws: a cluster at (1, 0) draws house_cottage_b in house
/// 1's slot (`hex_core::hero_piece`), at (-3, 0) the longhouse, at the origin the cottage; the
/// origin and (1, 0) draw house_twostorey_c in house 2's slot, (-1, 1) house_twostorey_b, and
/// (2, -1) keeps house_twostorey.
#[test]
fn walk_obstacles_are_the_drawn_hero_variant() {
    let pieces = |q, r| -> Vec<String> {
        let g = world(&[(q, r, "house_cluster", 0)]);
        g.obstacles[&Axial::new(q, r)]
            .iter()
            .filter(|o| o.piece.starts_with("house_"))
            .map(|o| o.piece.clone())
            .collect()
    };
    assert_eq!(
        pieces(0, 0),
        ["house_cottage", "house_cottage", "house_twostorey_c"]
    );
    // house_cottage_b's collider and its two porch posts (`PIECE_POSTS`).
    assert_eq!(
        pieces(1, 0),
        [
            "house_cottage",
            "house_cottage_b",
            "house_cottage_b",
            "house_cottage_b",
            "house_twostorey_c"
        ]
    );
    assert_eq!(
        pieces(-1, 1),
        ["house_cottage", "house_longhouse", "house_twostorey_b"]
    );
    assert_eq!(
        pieces(-3, 0),
        ["house_cottage", "house_longhouse", "house_twostorey"]
    );
    assert_eq!(
        pieces(2, -1),
        ["house_cottage", "house_cottage", "house_twostorey"]
    );
    // House 1 sits at (-0.5, 7.0): walk at its yard front from the pad.
    for (q, r, piece) in [(1, 0, "house_cottage_b"), (-3, 0, "house_longhouse")] {
        let g = world(&[(q, r, "house_cluster", 0)]);
        let c = centre(q, r);
        let start = c + Vec2::new(-0.5, 1.5);
        let s = walk(&g, start, Vec2::new(0.0, 6.0), 8.0);
        assert_eq!(s.blocked, Some(Blocked::Obstacle(piece.into())));
        assert_eq!(s.blocked.unwrap().reason(), "building");
    }
}

/// Each `PIECE_POSTS` square stands outside its piece's collider and holds the LOD0 timber
/// posts there: every timber vertex outside the collider below the porch's tie beam (the posts'
/// buried feet) lies in a square, and every square holds some. A rebuild that moves the posts
/// fails here.
#[test]
fn piece_posts_cover_the_posts_outside_the_collider() {
    let pieces: Vec<&str> = PIECE_POSTS.iter().map(|(p, _)| *p).collect();
    for &piece in &pieces {
        assert!(is_obstacle(piece), "{piece}");
        let s = shape(piece);
        let collider = Obstacle {
            piece: piece.into(),
            poly: s.hull.clone(),
        };
        let squares: Vec<Obstacle> = PIECE_POSTS
            .iter()
            .filter(|(p, _)| *p == piece)
            .map(|(_, c)| Obstacle {
                piece: piece.into(),
                poly: hull(&c.map(Vec2::from)),
            })
            .collect();
        let posts: Vec<Vec2> = mesh_tris(piece, "lod0")
            .unwrap()
            .iter()
            .filter(|(_, slot)| slot.ends_with("_timber"))
            .flat_map(|(t, _)| t.iter().copied())
            .filter(|p| p[1] < 1.8)
            .map(|p| Vec2::new(p[0], p[2]))
            .filter(|p| !collider.hits(*p, 0.0))
            .collect();
        assert!(!posts.is_empty(), "{piece}: no timber outside the collider");
        for p in &posts {
            assert!(
                squares.iter().any(|q| q.hits(*p, 0.005)),
                "{piece}: post vertex {p} outside every square"
            );
        }
        for q in &squares {
            assert!(posts.iter().any(|p| q.hits(*p, 0.0)), "{q:?} holds no post");
            assert!(
                q.poly.iter().all(|c| !collider.hits(*c, 0.0)),
                "{q:?} is inside the collider"
            );
        }
    }
}

/// Owner Q65: house_cottage_b's porch posts stop the walker. The cluster at (1, 0) draws it in
/// house 1's slot at (-0.5, 7.0), yaw 270, door toward the tile centre: the posts stand at
/// x -1.04 and 0.04, z 4.11 (0.12 m square), the front wall's collider beyond them.
#[test]
fn the_cottage_b_porch_posts_block_the_walker() {
    let g = world(&[(1, 0, "house_cluster", 0)]);
    let c = centre(1, 0);
    // At the east post from the pad: stopped a body radius short of its face (z 4.05).
    let s = walk(&g, c + Vec2::new(0.04, 1.5), Vec2::Y, 4.0);
    assert_eq!(s.blocked, Some(Blocked::Obstacle("house_cottage_b".into())));
    assert_eq!(s.blocked.as_ref().map(Blocked::reason), Some("building"));
    let z = s.pos.z - c.y;
    assert!((3.70..3.80).contains(&z), "stopped at z {z}");
    // From the side, under the porch roof's edge: stopped at the post's east face (x 0.10).
    let s = walk(&g, c + Vec2::new(1.5, 4.11), -Vec2::X, 3.0);
    assert_eq!(s.blocked, Some(Blocked::Obstacle("house_cottage_b".into())));
    let x = s.pos.x - c.x;
    assert!((0.35..0.45).contains(&x), "stopped at x {x}");
    // Between the posts onto the doorstep: past the posts' line, stopped by the front wall.
    let s = walk(&g, c + Vec2::new(-0.5, 1.5), Vec2::Y, 4.0);
    assert_eq!(s.blocked, Some(Blocked::Obstacle("house_cottage_b".into())));
    let z = s.pos.z - c.y;
    assert!(z > 4.17, "under the porch to z {z}");
}

#[test]
fn a_house_wall_stops_the_walker_on_a_turned_tile() {
    // The origin draws house_twostorey_c in house 2's slot, (-1, 1) house_twostorey_b, (2, -1)
    // house_twostorey.
    for (q, r, piece) in [
        (0, 0, "house_twostorey_c"),
        (-1, 1, "house_twostorey_b"),
        (2, -1, "house_twostorey"),
    ] {
        for rotation in [0u8, 2] {
            let g = world(&[(q, r, "house_cluster", rotation)]);
            let c = centre(q, r);
            // The two-storey house stands at (-7.5, 0) in the tile frame: walk at it from the
            // tile centre, through the tile's turn.
            let house = Vec2::from({
                let p = turn(rotation, [-7.5, 0.0, 0.0]);
                [p[0], p[2]]
            });
            let start = c + house * (2.0 / 7.5);
            assert!(
                g.stand(start).is_ok(),
                "{piece} r{rotation}: start on the pad"
            );
            let s = walk(&g, start, c + house - start, 8.0);
            assert_eq!(
                s.blocked,
                Some(Blocked::Obstacle(piece.into())),
                "r{rotation}"
            );
            let gone = s.pos.xz().distance(start);
            assert!(
                gone > 0.5 && gone < 5.0,
                "{piece} r{rotation}: stopped {gone} m out, short of the house centre"
            );
            assert!(
                g.obstacle(s.pos.xz()).is_none(),
                "{piece} r{rotation}: never inside"
            );
        }
    }
}

#[test]
fn a_tree_trunk_stops_the_walker() {
    let g = world(&[(0, 0, "forest", 0), (1, 0, "meadow", 0)]);
    let trees: Vec<&Obstacle> = g.obstacles[&Axial::ORIGIN]
        .iter()
        .filter(|o| o.piece.starts_with("tree_forest"))
        .collect();
    assert!(trees.len() > 10, "{} spruces", trees.len());
    // Walk at a trunk from 3 m away along a line no other obstacle crosses.
    let mut tested = 0;
    for t in trees {
        let c = t.poly.iter().copied().sum::<Vec2>() / t.poly.len() as f32;
        let reach = t.poly.iter().map(|p| p.distance(c)).fold(0.0, f32::max);
        assert!(reach < 1.0, "a trunk, not a crown: {reach} m");
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            let start = c + 3.0 * Vec2::new(a.cos(), a.sin());
            if g.stand(start).is_err() {
                continue;
            }
            let s = walk(&g, start, c - start, 3.0);
            if s.blocked != Some(Blocked::Obstacle(t.piece.clone())) {
                continue;
            }
            let d = s.pos.xz().distance(c);
            assert!(
                d > BODY_RADIUS && d < reach + BODY_RADIUS + 0.15,
                "stopped {d} m from the trunk centre (reach {reach})"
            );
            tested += 1;
            break;
        }
        if tested >= 3 {
            break;
        }
    }
    assert!(tested >= 3, "three trunks walked into");
}

#[test]
fn water_and_the_map_edge_stop_the_walker() {
    let g = world(&[(0, 0, "meadow", 0), (0, 1, "beach", 0), (1, 1, "ocean", 0)]);
    // The map edge: walk east off the lone meadow row, on a line no scattered rock crosses.
    let s = [0.0, 2.0, -2.0, 4.0, -4.0]
        .into_iter()
        .map(|dz| walk(&g, centre(0, 0) + Vec2::new(0.0, dz), Vec2::X, 30.0))
        .find(|s| !matches!(s.blocked, Some(Blocked::Obstacle(_))))
        .expect("a line east that no rock crosses");
    assert_eq!(s.blocked, Some(Blocked::Edge));
    let apothem = HEX_RADIUS * 3f32.sqrt() / 2.0;
    let x = s.pos.x - centre(0, 0).x;
    assert!(
        x > apothem - BODY_RADIUS - 0.2 && x < apothem,
        "stopped at the edge: {x}"
    );
    // The beach: walk out to sea until the water is too deep, with the feet in shallow water.
    let beach = centre(0, 1);
    let (seaward, _) = (0..12)
        .map(|k| {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            let d = Vec2::new(a.cos(), a.sin());
            (d, g.water_depth(beach + 12.0 * d))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let start = beach - 6.0 * seaward;
    let s = walk(&g, start, seaward, 40.0);
    assert_eq!(s.blocked, Some(Blocked::Water));
    let depth = g.water_depth(s.pos.xz());
    assert!(depth <= MAX_WADE, "wading at {depth} m");
    assert!(g.water_depth(s.pos.xz() + seaward) > depth, "deeper ahead");
    // The ocean is never ground.
    assert_eq!(g.ground(centre(1, 1)), Err(Blocked::Water));
}

#[test]
fn the_walker_fords_a_river_from_bank_to_bank() {
    let g = world(&[(0, 0, "river_straight", 1)]);
    let c = centre(0, 0);
    let across = (0..12)
        .map(|k| {
            let a = k as f32 * std::f32::consts::TAU / 12.0;
            Vec2::new(a.cos(), a.sin())
        })
        .find(|d| g.stand(c + 11.0 * *d).is_ok() && g.stand(c - 11.0 * *d).is_ok())
        .expect("a line across, bank to bank");
    let s = walk(&g, c + 11.0 * across, -across, 22.0);
    assert_eq!(s.blocked, None, "nothing stops the crossing");
    assert!(
        s.pos.xz().distance(c - 11.0 * across) < 0.01,
        "on the far bank: {}",
        s.pos
    );
    // Every stride across holds the feet at most a wade under the surface.
    let mut pos = Vec3::new(c.x + 11.0 * across.x, 0.0, c.y + 11.0 * across.y);
    pos.y = g.ground(pos.xz()).unwrap();
    for _ in 0..440 {
        pos = g.step(pos, -across * 0.05).pos;
        assert!(WATER_Y - pos.y <= MAX_WADE + 1e-4, "feet at {}", pos.y);
    }
}

#[test]
fn the_sea_still_stops_the_walker() {
    let g = world(&[(0, 0, "beach", 0), (1, 0, "ocean", 0)]);
    assert_eq!(g.ground(centre(1, 0)), Err(Blocked::Water));
}

/// The distance (m) from a hex's centre to its edges.
const APOTHEM: f32 = HEX_RADIUS * 0.866_025_4;

/// A standable point at x = `x` from the centre of the tile at (q, r): the first of a few
/// offsets across the line that no scatter obstacle covers.
fn standable(g: &WalkGround, q: i32, r: i32, x: f32) -> Vec3 {
    let c = centre(q, r);
    [0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0]
        .into_iter()
        .find_map(|z| {
            let p = c + Vec2::new(x, z);
            g.stand(p).ok().map(|h| Vec3::new(p.x, h, p.y))
        })
        .expect("a standable point")
}

/// Elevation (P4): a meadow one level up is ground 4 m higher than the same meadow on the
/// ground, and a walker stands on its top.
#[test]
fn walker_stands_on_a_raised_meadow() {
    let g = world_at(
        &catalog(),
        &[(0, 0, "meadow", 0, 0), (1, 0, "meadow", 0, 1)],
    );
    let low = g.ground(centre(0, 0)).unwrap();
    let high = g.ground(centre(1, 0)).unwrap();
    assert!((high - low - LEVEL_STEP).abs() < 1e-4, "{low} vs {high}");
    let feet = standable(&g, 1, 0, 0.0);
    assert!((feet.y - LEVEL_STEP).abs() < 0.3, "stands at {feet}");
}

/// A cliff (a level-0 meadow beside a level-1 meadow) is a wall: the step up onto the raised
/// tile is refused as steep and the walker stays on its own tile. Walking off the raised tile
/// the other way is a drop, which [`WalkGround::step`] allows (the walker falls).
#[test]
fn cliff_blocks_a_step() {
    let g = world_at(
        &catalog(),
        &[(0, 0, "meadow", 0, 0), (1, 0, "meadow", 0, 1)],
    );
    let from = standable(&g, 0, 0, APOTHEM - 0.6);
    let s = g.step(from, Vec2::new(1.2, 0.0));
    assert_eq!(s.blocked, Some(Blocked::Steep), "{s:?}");
    assert!(s.pos.x < centre(0, 0).x + APOTHEM, "stays below: {s:?}");
    let top = standable(&g, 1, 0, -APOTHEM + 0.6);
    let down = g.step(top, Vec2::new(-1.2, 0.0));
    assert_eq!(down.blocked, None, "the drop is allowed: {down:?}");
    assert!(down.pos.y < top.y - 3.0, "{down:?}");
}

/// The road ramp is walkable: from the level-0 road west of edge 3, along the ramp's road and
/// onto the level-1 road east of edge 0, no 0.25 m stride climbs more than [`MAX_STEP`] and
/// nothing refuses the walk.
#[test]
fn hillside_is_walkable() {
    let g = world_at(
        &catalog(),
        &[
            (-1, 0, "road_straight", 0, 0),
            (0, 0, "road_ramp", 0, 0),
            (1, 0, "road_straight", 0, 1),
        ],
    );
    let (x0, x1) = (centre(-1, 0).x, centre(1, 0).x);
    let mut pos = Vec3::new(x0, g.stand(Vec2::new(x0, 0.0)).unwrap(), 0.0);
    let start = pos.y;
    let mut steepest = 0.0f32;
    while pos.x < x1 - 0.25 {
        let s = g.step(pos, Vec2::new(0.25, 0.0));
        assert_eq!(s.blocked, None, "blocked at {pos}: {s:?}");
        assert!(s.pos.y - pos.y <= MAX_STEP, "{pos} -> {}", s.pos);
        steepest = steepest.max(s.pos.y - pos.y);
        pos = s.pos;
    }
    println!("ramp walk: steepest 0.25 m rise {steepest:.3} m");
    assert!(
        (pos.y - start - LEVEL_STEP).abs() < 0.05,
        "the ramp climbs one level: {start} -> {}",
        pos.y
    );
}

/// A tunnel (P4): the portal's cutting runs level to the portal's face, 2.2 m inside edge 0,
/// and the portal (a ground hero) stands there as a 4 m wall. The walker walking up the cutting
/// is stopped by the face's step and never stands in the arch; the cover meadow beyond edge 0
/// stands one level up and the walker on it stands on its top.
#[test]
fn walker_cannot_enter_the_tunnel() {
    let g = world_at(
        &catalog(),
        &[(0, 0, "road_tunnel_portal", 0, 0), (1, 0, "meadow", 0, 1)],
    );
    let edge = centre(0, 0).x + APOTHEM;
    let face = edge - 2.2;
    let floor = g.ground(Vec2::new(face - 0.5, 0.0)).unwrap();
    let wall = g.ground(Vec2::new(face + 0.5, 0.0)).unwrap();
    assert!(wall - floor > MAX_STEP, "face step {floor} -> {wall}");
    let s = walk(&g, centre(0, 0), Vec2::X, 30.0);
    assert_eq!(s.blocked, Some(Blocked::Steep), "{s:?}");
    assert!(s.pos.x < face, "stopped short of the face: {s:?}");
    let on_top = g.ground(centre(1, 0)).unwrap();
    assert!((on_top - LEVEL_STEP).abs() < 0.3, "cover top {on_top}");
}

/// Open tunnels (P4): in a complete road tunnel the walker passes the portal's face and walks
/// the bore's floor to the far portal's face on the portal level, never the cover's top; a
/// car's wheels and a train in the cover cells stand on that floor too. Beside the corridor the
/// cover's top still blocks.
#[test]
fn the_walker_walks_a_complete_tunnel_on_its_floor() {
    use hex_core::{NetworkKind, Tunnel};
    let c = catalog();
    let mut g = world_at(
        &c,
        &[
            (0, 0, "road_tunnel_portal", 0, 0),
            (1, 0, "meadow", 0, 1),
            (2, 0, "meadow", 0, 1),
            (3, 0, "road_tunnel_portal", 3, 0),
        ],
    );
    let at = |q| centre(q, 0);
    let start = Vec2::new(at(0).x - 3.0, 0.0);
    assert_eq!(
        walk(&g, start, Vec2::X, 40.0).blocked,
        Some(Blocked::Steep),
        "a lone portal still stops the walker at its face"
    );
    g.set_tunnels(&[Tunnel {
        a: Axial::new(0, 0),
        b: Axial::new(3, 0),
        dir: 0,
        cover: vec![Axial::new(1, 0), Axial::new(2, 0)],
        level: 0,
        kind: NetworkKind::Road,
    }]);
    let s = walk(&g, start, Vec2::X, 60.0);
    let far_face = at(3).x - (APOTHEM - 2.2) + 1.0;
    assert!(s.pos.x > at(2).x, "walked past the first cover cell: {s:?}");
    assert!(s.pos.x <= far_face + 0.5, "stops at the far face: {s:?}");
    for q in 0..=3 {
        let y = g.ground(Vec2::new(at(q).x, 0.0)).unwrap();
        assert!(
            y.abs() < 0.3,
            "cell {q}: the floor is on the portal level, not {y}"
        );
    }
    // Off the axis the cover's top is the ground again, a wall for the walker.
    let beside = g.ground(Vec2::new(at(1).x, 5.0)).unwrap();
    assert!(
        (beside - LEVEL_STEP).abs() < 0.3,
        "beside the bore {beside}"
    );
    // Cars and trains in the cover cells ride the floor.
    let car = g.ride_car(Vec2::new(at(1).x, 0.0), 0.11, Vec2::new(at(1).x, 0.0));
    assert!((car - 0.11).abs() < 1e-3, "{car}");
    assert_eq!(g.level_y(Vec2::new(at(2).x, 0.0)), 0.0);
    assert!((g.level_y(Vec2::new(at(2).x, 6.0)) - LEVEL_STEP).abs() < 1e-3);
}

/// A rail tunnel (P4): the rail portal's face stands 6 m inside edge 0 as a wall the walker
/// cannot climb, and the walker on the cover meadow beyond stands on its top.
#[test]
fn walker_cannot_enter_the_rail_tunnel() {
    let g = world_at(
        &catalog(),
        &[(0, 0, "rail_tunnel_portal", 0, 0), (1, 0, "meadow", 0, 1)],
    );
    let edge = centre(0, 0).x + APOTHEM;
    let face = edge - 6.0;
    let floor = g.ground(Vec2::new(face - 0.5, 0.0)).unwrap();
    let wall = g.ground(Vec2::new(face + 0.5, 0.0)).unwrap();
    assert!(wall - floor > MAX_STEP, "face step {floor} -> {wall}");
    let s = walk(&g, centre(0, 0), Vec2::X, 30.0);
    assert_eq!(s.blocked, Some(Blocked::Steep), "{s:?}");
    assert!(s.pos.x < face, "stopped short of the face: {s:?}");
    let on_top = g.ground(centre(1, 0)).unwrap();
    assert!((on_top - LEVEL_STEP).abs() < 0.3, "cover top {on_top}");
}

fn ray(o: Vec3, d: Vec3) -> Ray3d {
    Ray3d::new(o, Dir3::new(d).unwrap())
}

#[test]
fn a_ray_enters_an_extruded_footprint_from_the_front() {
    let square = [
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(-1.0, 1.0),
    ];
    let toward = ray(Vec3::new(0.0, 2.0, 10.0), Vec3::NEG_Z);
    assert_eq!(
        ray_prism(toward, &square, 0.0, 5.0),
        Some(9.0),
        "the near wall"
    );
    let cw: Vec<Vec2> = square.iter().rev().copied().collect();
    assert_eq!(
        ray_prism(toward, &cw, 0.0, 5.0),
        Some(9.0),
        "either winding"
    );
    assert_eq!(ray_prism(toward, &square, 0.0, 1.5), None, "over its top");
    let down = ray(Vec3::new(0.5, 9.0, 0.0), Vec3::NEG_Y);
    assert_eq!(
        ray_prism(down, &square, 0.0, 5.0),
        Some(4.0),
        "onto the roof"
    );
    assert_eq!(
        ray_prism(
            ray(Vec3::new(3.0, 2.0, 10.0), Vec3::NEG_Z),
            &square,
            0.0,
            5.0
        ),
        None
    );
    assert_eq!(
        ray_prism(
            ray(Vec3::new(0.0, 2.0, 0.0), Vec3::NEG_Z),
            &square,
            0.0,
            5.0
        ),
        None,
        "from inside"
    );
    assert_eq!(
        ray_prism(ray(Vec3::new(0.0, 2.0, 10.0), Vec3::Z), &square, 0.0, 5.0),
        None,
        "behind"
    );
}

#[test]
fn a_crown_is_hit_on_its_widest_band_and_missed_beside_it() {
    // A cone of foliage: 4 m wide at 2 m, a point at 12 m.
    let step = std::f32::consts::TAU / 16.0;
    let tri = |a: f32| -> Tri {
        let (s0, c0) = a.sin_cos();
        let (s1, c1) = (a + step).sin_cos();
        [
            [4.0 * c0, 2.0, 4.0 * s0],
            [4.0 * c1, 2.0, 4.0 * s1],
            [0.0, 12.0, 0.0],
        ]
    };
    let tris: Vec<Tri> = (0..16)
        .map(|i| tri(i as f32 * std::f32::consts::TAU / 16.0))
        .collect();
    let c = Crown::fit(&tris).unwrap();
    assert!((c.y0 - 2.0).abs() < 1e-4 && (c.band * CROWN_BANDS as f32 - 10.0).abs() < 1e-3);
    let widest = |k: usize| c.radii[k].iter().copied().fold(0.0, f32::max);
    assert!(
        widest(0) > widest(CROWN_BANDS - 1) + 2.0,
        "a cone narrows: {:?}",
        c.radii
    );
    let low = c.hit(ray(Vec3::new(0.0, 2.5, 30.0), Vec3::NEG_Z)).unwrap();
    assert!(low > 25.0 && low < 27.0, "the wide skirt at 2.5 m: {low}");
    let high = c.hit(ray(Vec3::new(0.0, 11.0, 30.0), Vec3::NEG_Z)).unwrap();
    assert!(high > low + 2.0, "the narrow top is farther: {high}");
    assert!(
        c.hit(ray(Vec3::new(6.0, 5.0, 30.0), Vec3::NEG_Z)).is_none(),
        "beside it"
    );
    assert!(
        c.hit(ray(Vec3::new(0.0, 13.0, 30.0), Vec3::NEG_Z))
            .is_none(),
        "over it"
    );
}

#[test]
fn a_click_picks_the_nearest_drawn_prop_and_its_kind() {
    let g = world(&[(0, 0, "oak", 0), (1, 0, "house_cluster", 0)]);
    assert!(g.props.iter().any(|p| p.piece.starts_with("tree_hero_oak")));
    assert!(
        g.props.iter().all(|p| p.piece != "wildflower_clump"),
        "flowers are not pickable"
    );
    let oak = centre(0, 0);
    // Level at eye height from 40 m south, up at the crown's middle (above the horizon).
    let eye = Vec3::new(oak.x, 1.7, oak.y + 40.0);
    let mid = Vec3::new(oak.x, 8.7, oak.y);
    let (t, piece) = g.pick_prop(ray(eye, mid - eye), 1000.0).unwrap();
    // The hero oak, or a scattered oak standing in front of it.
    assert!(
        [
            "tree_hero_oak",
            "tree_hero_oak_b",
            "tree_hero_oak_c",
            "tree_oak",
            "tree_oak_b",
            "tree_oak_c"
        ]
        .contains(&piece),
        "{piece}"
    );
    assert_eq!(prop_kind(piece).as_str(), "tree");
    assert!(t > 25.0 && t < 40.0, "the crown's near side: {t}");
    // Beside the tree the ray meets nothing here (the ground is the caller's).
    let beside = Vec3::new(oak.x + 14.0, 8.7, oak.y);
    assert!(g.pick_prop(ray(eye, beside - eye), 1000.0).is_none());
    // The house is a building, and stands in front of whatever is behind it.
    let house = g.obstacles[&Axial::new(1, 0)]
        .iter()
        .find(|o| o.piece.starts_with("house"))
        .unwrap();
    let foot = house.poly.iter().copied().sum::<Vec2>() / house.poly.len() as f32;
    let from = Vec3::new(foot.x, 3.0, foot.y + 30.0);
    let (t, piece) = g.pick_prop(ray(from, Vec3::NEG_Z), 1000.0).unwrap();
    assert_eq!(prop_kind(piece).as_str(), "building");
    assert!(t < 30.0, "{t}");
}

/// A ray into the oak's crown or trunk lands on its LOD0 mesh; the volume, which never
/// starts behind the mesh, only decides that the tree was hit.
#[test]
fn a_click_on_the_oak_lands_on_its_mesh() {
    let tris = mesh_tris("tree_hero_oak", "lod0").unwrap();
    let s = Arc::new(shape("tree_hero_oak"));
    let turned = Transform::from_xyz(5.0, 0.2, -3.0)
        .with_rotation(Quat::from_rotation_y(0.7))
        .with_scale(Vec3::splat(1.1));
    for (h, d) in [(8.7, 40.0), (6.0, 25.0), (10.5, 60.0), (2.0, 30.0)] {
        let r = ray(Vec3::new(0.0, 1.7, d), Vec3::new(0.0, h - 1.7, -d));
        let mesh = tris
            .iter()
            .filter_map(|(t, _)| ray_triangle(r, t))
            .reduce(f32::min)
            .unwrap();
        let prop = PickProp::new("tree_hero_oak", &s, &Transform::IDENTITY).unwrap();
        let volume = s
            .body
            .and_then(|(a, b)| ray_prism(r, &s.hull, a, b))
            .into_iter()
            .chain(s.crown.and_then(|c| c.hit(r)))
            .reduce(f32::min)
            .unwrap();
        assert!(
            volume <= mesh + 1e-3,
            "the volume wraps the mesh: {volume} vs {mesh}"
        );
        assert!(
            (prop.hit(r).unwrap() - mesh).abs() < 1e-3,
            "at {h} m from {d} m"
        );
        // Placed, turned and scaled: the same ray in the tree's frame, in world metres.
        let placed = PickProp::new("tree_hero_oak", &s, &turned).unwrap();
        let world = Ray3d::new(
            turned.transform_point(r.origin),
            Dir3::new(turned.rotation * *r.direction).unwrap(),
        );
        let got = placed.hit(world).unwrap();
        assert!(
            (got - mesh * 1.1).abs() < 1e-2,
            "placed: {got} vs {}",
            mesh * 1.1
        );
    }
}

/// `make shots-photo`'s tree click (photo-09): level at eye height 40 m south of the oak tile
/// at (0, 3), whose hero slot draws the forked oak, a click at (0.58, 0.22) of a 45° 1280x800
/// frame meets the forked oak's LOD0 mesh (its tall east lobe) at 35.8 m of view depth. The
/// piece comes from the tile's variant pick, so a reshuffled variant fails here.
#[test]
fn the_tree_click_shot_meets_the_forked_oak_mesh_at_35_8_m() {
    let oak = catalog().get("oak").unwrap().clone();
    let piece = hex_core::hero_piece(&oak, Axial::new(0, 3), 0);
    assert_eq!(piece, "tree_hero_oak_b");
    let tris = mesh_tris(piece, "lod0").unwrap();
    let tan = 22.5f32.to_radians().tan();
    let d = Vec3::new(
        (0.58 - 0.5) * 2.0 * tan * 1280.0 / 800.0,
        (0.5 - 0.22) * 2.0 * tan,
        -1.0,
    );
    let r = ray(Vec3::new(0.0, 1.7, 40.0), d);
    let t = tris
        .iter()
        .filter_map(|(t, _)| ray_triangle(r, t))
        .reduce(f32::min)
        .unwrap();
    let s = Arc::new(shape(piece));
    let pick = PickProp::new(piece, &s, &Transform::IDENTITY)
        .unwrap()
        .hit(r)
        .unwrap();
    assert!(
        (pick - t).abs() < 1e-3,
        "the click lands on the mesh: {pick} vs {t}"
    );
    let depth = t / d.length();
    assert!((depth - 35.8).abs() < 0.05, "{depth}");
}

#[test]
fn a_gap_in_the_leaves_still_hits_the_crown() {
    let step = std::f32::consts::TAU / 16.0;
    let cone: Vec<Tri> = (0..16)
        .map(|i| {
            let (s0, c0) = (i as f32 * step).sin_cos();
            let (s1, c1) = ((i + 1) as f32 * step).sin_cos();
            [
                [4.0 * c0, 2.0, 4.0 * s0],
                [4.0 * c1, 2.0, 4.0 * s1],
                [0.0, 12.0, 0.0],
            ]
        })
        .collect();
    // Only the cone's west (-x) half is mesh; its east half is a gap the crown still covers.
    let shape = Arc::new(PieceShape {
        crown: Crown::fit(&cone),
        tris: cone
            .iter()
            .copied()
            .filter(|t| t[0][0] + t[1][0] < 0.0)
            .collect(),
        ..default()
    });
    let prop = PickProp::new("tree_forest", &shape, &Transform::IDENTITY).unwrap();
    let east = ray(Vec3::new(2.0, 4.0, 30.0), Vec3::NEG_Z);
    let volume = shape.crown.unwrap().hit(east).unwrap();
    assert_eq!(
        prop.hit(east),
        Some(volume),
        "through the gap only: the crown"
    );
    assert!(volume > 26.0 && volume < 29.0, "{volume}");
    let west = ray(Vec3::new(-2.0, 4.0, 30.0), Vec3::NEG_Z);
    let leaves = prop.hit(west).unwrap();
    assert!(
        leaves > volume - 0.5 && leaves < 30.0,
        "the west leaves: {leaves}"
    );
}

#[test]
fn the_visible_top_is_the_ground_or_the_water_surface() {
    let g = world(&[(0, 0, "meadow", 0), (-1, 0, "river_straight", 0)]);
    let m = centre(0, 0) + Vec2::new(3.0, 2.0);
    assert_eq!(g.top(m), g.ground(m).ok(), "dry ground is its own top");
    let c = centre(-1, 0);
    assert_eq!(
        g.ground(c),
        Ok(WATER_Y - MAX_WADE),
        "a ford under the surface"
    );
    assert_eq!(
        g.top(c),
        Some(WATER_Y),
        "the channel shows its water surface"
    );
    assert_eq!(g.top(Vec2::new(500.0, 0.0)), None, "off the map");
    assert!(!g.is_empty() && WalkGround::default().is_empty());
}

#[test]
fn the_entry_point_is_the_nearest_standable_ground() {
    let g = world(&[(0, 0, "oak", 0)]);
    // The oak's trunk stands on the centre.
    assert!(g.stand(Vec2::ZERO).is_err());
    let p = g.nearest_stand(Vec2::ZERO).unwrap();
    assert!(p.xz().length() < 4.0, "beside the trunk: {p}");
    // A focus off the map comes back to the tile.
    let far = g.nearest_stand(Vec2::new(200.0, 0.0)).unwrap();
    assert!(g.tile_at(far.xz()).is_some());
}

/// The junction's point lever footprint covers the lever's LOD0 geometry (the `rail_lever`
/// slot) and the base timber under it, in the tile frame.
#[test]
fn the_lever_footprint_covers_the_lever_geometry() {
    let tris = mesh_tris("rail_junction_center", "lod0").expect("junction lod0");
    let lever: Vec<Vec2> = tris
        .iter()
        .filter(|(_, s)| s == "rail_lever")
        .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
        .collect();
    assert!(!lever.is_empty(), "the junction draws a rail_lever slot");
    let (_, _, corners) = CENTRE_OBSTACLES[0];
    let (lo, hi) = (Vec2::from(corners[0]), Vec2::from(corners[2]));
    for p in &lever {
        assert!(
            p.x >= lo.x - 0.05 && p.x <= hi.x + 0.05 && p.y >= lo.y - 0.05 && p.y <= hi.y + 0.05,
            "lever vertex {p} outside the footprint {lo}..{hi}"
        );
    }
}

/// A rail viaduct's deck is not ground and its piers block (owner RL-Q3): on each height the
/// walker stands on the meadow under all three arches, not on the deck; each abutment and pier
/// row blocks it; and every LOD0 stone vertex below head height (1.8 m) lies in a row.
#[test]
fn the_viaduct_piers_block_and_the_arches_pass() {
    let c = catalog();
    for n in 1..=3 {
        let id = format!("rail_viaduct_l{n}");
        let g = world_in(&c, &[(0, 0, id.as_str(), 0)]);
        for x in [-7.0, 0.0, 7.0] {
            let h = g
                .stand(Vec2::new(x, 0.0))
                .unwrap_or_else(|b| panic!("{id}: blocked under the arch at x {x}: {b:?}"));
            assert!(h.abs() < 0.3, "{id}: stands at {h} under the arch at x {x}");
        }
        let rows: Vec<[[f32; 2]; 4]> = CENTRE_OBSTACLES
            .iter()
            .filter(|(centre, part, _)| *centre == id && *part == VIADUCT_PIER)
            .map(|(_, _, corners)| *corners)
            .collect();
        assert_eq!(rows.len(), 4, "{id}: two abutments and two piers");
        for r in &rows {
            // Off the centreline: an open rail end's buffer stop stands on it.
            let p = Vec2::new((r[0][0] + r[2][0]) / 2.0, 2.4);
            assert_eq!(
                g.stand(p),
                Err(Blocked::Obstacle(VIADUCT_PIER.into())),
                "{id}: {p}"
            );
        }
        let tris = mesh_tris(&id, "lod0").expect("viaduct lod0");
        let low: Vec<[f32; 3]> = tris
            .iter()
            .filter(|(_, s)| s.ends_with("_stone"))
            .flat_map(|(t, _)| t.iter().copied())
            .filter(|p| p[1] < 1.8)
            .collect();
        assert!(!low.is_empty(), "{id}: stone below head height");
        for p in &low {
            assert!(
                rows.iter().any(|r| p[0] >= r[0][0] - 0.05
                    && p[0] <= r[2][0] + 0.05
                    && p[2] >= r[0][1] - 0.05
                    && p[2] <= r[2][1] + 0.05),
                "{id}: stone vertex {p:?} outside every pier row"
            );
        }
    }
}

/// The wide rail bend's track path is its GLB's track: every LOD0 rail-head vertex of
/// `rail_bend_120_center` stands a rail centre (0.84 m) off the `rail_bend_120` tile's network
/// polyline, within the head's half width, on both sides of it; and the polyline is the bake
/// report's arc (24 m about tile (13.856, 24), 25.133 m long) that trains run on.
#[test]
fn the_wide_rail_bend_path_runs_between_its_rails() {
    let report: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../assets-source/kit/rail_bend_120_center/bake-report.json"
    ))
    .unwrap();
    let path = &report["trackPath"];
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let (radius, arc) = (f(&path["radiusM"]), f(&path["arcLengthM"]));
    let rail = f(&report["section"]["railCentreM"]);
    // Blender (x, y) is the tile frame's (x, -z).
    let c = Vec2::new(f(&path["centre"][0]), -f(&path["centre"][1]));
    assert!((radius - 24.0).abs() < 1e-3 && (arc - 25.133).abs() < 1e-3);
    let def = catalog().get("rail_bend_120").unwrap().clone();
    let (line, _) = scatter::lanes(&def).remove(0);
    for p in &line {
        assert!(
            (p.distance(c) - radius).abs() < 0.002,
            "path point {p} off the arc"
        );
    }
    let length: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    assert!((length - arc).abs() < 0.01, "path {length} m, arc {arc} m");
    let tris = mesh_tris("rail_bend_120_center", "lod0").expect("wide bend lod0");
    let mut sides = [0usize; 2];
    for (t, _) in tris.iter().filter(|(_, s)| s == "rail_head") {
        for p in t.iter().map(|p| Vec2::new(p[0], p[2])) {
            let d = scatter::polyline_distance(p, &line);
            assert!(
                (d - rail).abs() < 0.045,
                "rail head at {p} stands {d} m off the path"
            );
            sides[usize::from(p.distance(c) > radius)] += 1;
        }
    }
    assert!(sides[0] > 0 && sides[1] > 0, "rail heads by side {sides:?}");
}

/// Criterion 3 (walker): the junction lever blocks the walker at every rotation, reported as a
/// lever, while the track itself is walkable: across the straight, the bend and the junction's
/// legs the walker never meets a step it cannot climb.
#[test]
fn the_junction_lever_blocks_the_walker_and_the_track_is_ground() {
    for rotation in 0..6u8 {
        let g = world(&[(0, 0, "rail_junction", rotation)]);
        let turn = Quat::from_rotation_y(-(f32::from(rotation)) * std::f32::consts::FRAC_PI_3);
        let at = |x: f32, z: f32| (turn * Vec3::new(x, 0.0, z)).xz();
        // From 3 m off the lever, walk at it.
        let lever = at(13.07, -2.6);
        let from = at(13.07, -6.0);
        let s = walk(&g, from, lever - from, 4.0);
        let b = s.blocked.expect("the lever stops the walker");
        assert_eq!(b.piece(), Some("rail_lever"), "r{rotation}: {b:?}");
        assert_eq!(b.reason(), "lever");
        assert!(
            s.pos.xz().distance(lever) > 0.3,
            "r{rotation}: walked into the lever"
        );
    }
    let g = world(&[
        (0, 0, "rail_straight", 0),
        (1, 0, "rail_bend_60", 0),
        (-1, 0, "rail_junction", 0),
        (0, 1, "rail_bend_120", 0),
    ]);
    // Across the straight, both bends' arcs (the wide bend's at its middle, from 4 m inside to
    // 4 m outside) and the junction's two legs past the lever.
    for (start, dir, dist) in [
        (centre(0, 0) + Vec2::new(0.0, -6.0), Vec2::Y, 12.0),
        (
            centre(1, 0) + Vec2::new(3.0, 1.0),
            Vec2::new(1.0, 0.857),
            8.0,
        ),
        (
            centre(0, 1) + Vec2::new(3.856, 6.679),
            Vec2::new(-0.5, -0.866),
            8.0,
        ),
        (centre(-1, 0) + Vec2::new(-4.0, -6.0), Vec2::Y, 12.0),
        (
            centre(-1, 0) + Vec2::new(9.0, 6.0),
            Vec2::new(-1.0, -0.4),
            7.0,
        ),
    ] {
        let s = walk(&g, start, dir, dist);
        assert!(
            s.blocked.is_none(),
            "from {start} along {dir}: blocked by {:?} at {}",
            s.blocked,
            s.pos
        );
    }
}

/// The station at the origin turned `rotation`, with a rail straight on each track edge, and
/// the map from the station's tile frame to world XZ.
fn station(rotation: u8) -> (WalkGround, impl Fn(f32, f32) -> Vec2) {
    let [a, b] = [rotation, (rotation + 3) % 6].map(|e| Axial::ORIGIN.neighbor(e));
    let g = world(&[
        (0, 0, "rail_station", rotation),
        (a.q, a.r, "rail_straight", rotation),
        (b.q, b.r, "rail_straight", rotation),
    ]);
    let turn = Quat::from_rotation_y(-(f32::from(rotation)) * std::f32::consts::FRAC_PI_3);
    (g, move |x: f32, z: f32| (turn * Vec3::new(x, 0.0, z)).xz())
}

/// Station criterion 1: the platform is walker ground. The walker stands on the paving at
/// 1.38 m, and walks in from each neighbouring straight, up the ramp, onto the level paving
/// (clear of the lamps, tubs, board and shelter); every ramp toe is a step it can climb, the
/// meadow at the border sitting within 0.03 m of the cap.
#[test]
fn the_walker_stands_on_the_platform_and_climbs_both_ramps() {
    let def = catalog().get("rail_station").unwrap().clone();
    let surface = surface(&def).unwrap();
    assert_eq!(surface.at(0.0, -4.5).unwrap().1, "rail_station_paving");
    for z in (0..50).map(|i| -6.75 + 0.1 * i as f32) {
        for x in [13.3f32, -13.3] {
            let toe = surface.at(x * 0.999, z).unwrap().0;
            let meadow = surface.at(x * 1.002, z).unwrap().0;
            assert!(
                toe - meadow <= MAX_STEP,
                "toe at ({x}, {z}): {toe} over {meadow}"
            );
        }
    }
    for rotation in [0, 2, 5] {
        let (g, at) = station(rotation);
        let paving = g.ground(at(0.0, -4.5)).unwrap();
        assert!(
            (paving - 1.38).abs() < 0.02,
            "r{rotation}: paving at {paving}"
        );
        for side in [1.0f32, -1.0] {
            let from = at(16.5 * side, -4.5);
            let to = at(6.0 * side, -4.5);
            let s = walk(&g, from, to - from, 10.5);
            assert!(
                s.blocked.is_none() && s.pos.xz().distance(to) < 0.05,
                "r{rotation} side {side}: blocked by {:?} at {}",
                s.blocked,
                s.pos
            );
            assert!(
                (s.pos.y - 1.38).abs() < 0.02,
                "r{rotation} side {side}: feet at {}",
                s.pos.y
            );
        }
    }
}

/// Station criterion 2: the shelter (on the paving, `HeroProp::y`) blocks the walker as a
/// building at every rotation, and photo mode's click picks it at the height it stands.
#[test]
fn the_station_shelter_blocks_as_a_building_and_is_picked() {
    for rotation in [0, 1, 3] {
        let (g, at) = station(rotation);
        let from = at(-2.8, -2.4);
        let s = walk(&g, from, at(-2.8, -6.0) - from, 2.0);
        let b = s.blocked.expect("the shelter stops the walker");
        assert_eq!(
            b.piece(),
            Some("rail_station_shelter"),
            "r{rotation}: {b:?}"
        );
        assert_eq!(b.reason(), "building");
        assert!(
            (s.pos.y - 1.38).abs() < 0.02,
            "r{rotation}: stopped on the paving"
        );
        // Level at paving eye height from across the track, at the room's front wall (z -3.6).
        let eye = at(-2.8, 6.0);
        let wall = at(-2.8, -5.0);
        let o = Vec3::new(eye.x, 1.38 + 1.7, eye.y);
        let (t, piece) = g
            .pick_prop(ray(o, Vec3::new(wall.x, o.y, wall.y) - o), 100.0)
            .expect("the shelter is pickable");
        assert_eq!(piece, "rail_station_shelter", "r{rotation}");
        assert_eq!(prop_kind(piece).as_str(), "building");
        assert!(t > 9.0 && t < 10.5, "r{rotation}: front wall at {t}");
        // Above its ridge the ray meets nothing.
        let high = Vec3::new(eye.x, 1.38 + 9.0, eye.y);
        assert!(
            g.pick_prop(ray(high, Vec3::new(wall.x, high.y, wall.y) - high), 100.0)
                .is_none(),
            "r{rotation}: picked above the roof"
        );
    }
}

/// The topmost upward triangle's height over tile-frame (x, z) among `tris`, by brute force
/// over every triangle (no bucket grid, no extent), as [`Surface::top`] reads one.
fn brute_top(tris: &[(Tri, String)], x: f32, z: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for ([a, b, c], name) in tris {
        if scatter::is_not_ground(name) {
            continue;
        }
        let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
        if d.abs() < 1e-9 {
            continue;
        }
        let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
        let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
        if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
            continue;
        }
        let y = a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]);
        best = Some(best.map_or(y, |h: f32| h.max(y)));
    }
    best
}

/// The massif's ground under world `p` for an anchor at the origin turned `k` at level 1: the
/// piece's LOD0 ground in the tile frame, read by [`brute_top`].
fn massif_ground(tris: &[(Tri, String)], k: u8, p: Vec2) -> f32 {
    let l = turn((6 - k % 6) % 6, [p.x, 0.0, p.y]);
    LEVEL_STEP + brute_top(tris, l[0], l[2]).unwrap_or_else(|| panic!("no ground at {p}"))
}

fn massif_tris(c: &TileCatalog) -> Vec<(Tri, String)> {
    let def = c.get("massif_granite").unwrap();
    crate::play::slope_ground::tile_ground_tris(def, c, &mut |id| mesh_tris(id, "lod0")).unwrap()
}

/// Mega tiles (A52): every cell a massif covers is ground and its tile is the massif, and the
/// ground there is the piece's own, turned with the tile: on the ring cells the grass slopes
/// rise metres above the massif's level toward the rock, not a flat floor at it. The cell past
/// its ring is still the map edge.
#[test]
fn member_cells_are_ground_not_void() {
    let c = catalog();
    let tris = massif_tris(&c);
    for k in [0u8, 1, 2, 5] {
        let g = world_at(&c, &[(0, 0, "massif_granite", k, 1)]);
        let mut raised = 0;
        for m in hex_core::footprint::members(Axial::ORIGIN) {
            let at = centre(m.q, m.r);
            assert_eq!(g.tile_at(at), Some((m, "massif_granite")), "{m:?}");
            for b in 0..12 {
                let dir = Vec2::from_angle(b as f32 * std::f32::consts::FRAC_PI_6);
                for r in [0.0, 3.0, 7.0, 11.0] {
                    let p = at + dir * r;
                    if cell_at(p) != m {
                        continue;
                    }
                    let want = massif_ground(&tris, k, p);
                    let got = g.ground(p).unwrap();
                    assert!(
                        (got - want).abs() < 0.02,
                        "k{k} {m:?} at {p}: ground {got}, the piece's {want}"
                    );
                    if m != Axial::ORIGIN && want > LEVEL_STEP + 1.0 {
                        raised += 1;
                    }
                }
            }
        }
        assert!(raised > 20, "k{k}: only {raised} ring points on the slopes");
    }
    let g = world_at(&c, &[(0, 0, "massif_granite", 0, 1)]);
    assert!(
        g.ground(Vec2::ZERO).unwrap() > LEVEL_STEP + 20.0,
        "the summit"
    );
    assert_eq!(g.ground(centre(2, 0)), Err(Blocked::Edge));
}

/// The massif without its ring scatter, whose boulders and pines would stop a walk short of the
/// rock.
fn bare_massif_catalog() -> TileCatalog {
    let base = catalog();
    let tiles = base.tiles().iter().cloned().map(|mut t| {
        if t.is_mega() {
            t.props.scatter.clear();
        }
        t
    });
    TileCatalog::new(base.sockets().clone(), tiles.collect()).unwrap()
}

/// Mega tiles (A52): the walker stands on a massif's ring on the piece's ground, climbs its
/// grass slopes toward the rock, and never enters the rock's foot: aimed past the centre it
/// slides along the foot, aimed at the dead centre it stops at it.
#[test]
fn walker_walks_around_a_massif() {
    let c = bare_massif_catalog();
    let tris = massif_tris(&c);
    let rock: Vec<Obstacle> = {
        let g = world_at(&c, &[(0, 0, "massif_granite", 0, 1)]);
        assert_eq!(g.obstacles[&Axial::ORIGIN].len(), MASSIF_ROCK_FOOT.len());
        g.obstacles[&Axial::ORIGIN].clone()
    };
    for k in [0u8, 1, 2, 5] {
        let g = world_at(&c, &[(0, 0, "massif_granite", k, 1)]);
        for d in 0..6u8 {
            let m = Axial::ORIGIN.neighbor(d);
            let p = centre(m.q, m.r);
            let y = g
                .stand(p)
                .unwrap_or_else(|b| panic!("k{k} ring cell {d}: {b:?}"));
            let want = massif_ground(&tris, k, p);
            assert!(
                (y - want).abs() < 0.02,
                "k{k} ring cell {d} at {y}, the piece's {want}"
            );
        }
        assert_eq!(
            g.stand(Vec2::ZERO),
            Err(Blocked::Obstacle("massif_granite".into()))
        );
    }
    let g = world_at(&c, &[(0, 0, "massif_granite", 0, 1)]);
    let ring = centre(-1, 0);
    let start = ring + ring.normalize() * 4.0;
    // West of the anchor the foot between two spines (`MASSIF_ROCK_FOOT`) faces a walk aimed
    // a little north of the centre obliquely, so it slides north along that face until the
    // next spine's notch stops it; one aimed at the centre stops where it meets the foot.
    for (bias, slides) in [(Vec2::new(0.0, 0.2), true), (Vec2::ZERO, false)] {
        let aim = (-start).normalize() + bias;
        let mut pos = Vec3::new(start.x, g.ground(start).unwrap(), start.y);
        let mut blocked = None;
        let mut contact = None;
        let mut top = pos.y;
        for _ in 0..1200 {
            let s = g.step(pos, aim.normalize() * 0.05);
            pos = s.pos;
            if contact.is_none() && s.blocked.is_some() {
                contact = Some(pos.xz());
            }
            blocked = s.blocked.or(blocked);
            top = top.max(pos.y);
            assert!(
                rock.iter().all(|o| !o.hits(pos.xz(), BODY_RADIUS - 1e-2)),
                "inside the rock's foot at {pos}"
            );
            let want = massif_ground(&tris, 0, pos.xz());
            assert!(
                (pos.y - want).abs() < 0.02,
                "feet at {pos}, the ground {want}"
            );
        }
        assert!(top > LEVEL_STEP + 2.0, "climbed the slopes: top {top}");
        assert_eq!(blocked, Some(Blocked::Obstacle("massif_granite".into())));
        let contact = contact.expect("met the foot");
        if slides {
            let slid = (pos.xz() - contact).length();
            assert!(slid > 2.0, "slid along the foot: {contact} to {pos}");
        } else {
            assert!(pos.xz().length() < ring.length(), "reached the foot: {pos}");
        }
    }
}

/// Form pass 3 (A55 Q1 O1): the massif's grass flanks rise from the rim to a rock foot 7-11 m
/// up within a ring cell's depth, and the walker climbs them. The steepest grass or scree
/// triangle stays under the walker's limit, a [`MAX_STEP`] rise per [`SUBSTEP`] stride
/// (gradient 4, 76 degrees), and walks in from the rim on twelve bearings (each ring cell's
/// middle and each notch between two ring cells) climb without a step refused and stop at the
/// rock's foot more than 7 m up.
#[test]
fn walker_climbs_the_massif_flanks_to_the_rock_foot() {
    let c = bare_massif_catalog();
    let tris = massif_tris(&c);
    let steepest = tris
        .iter()
        .filter(|(_, s)| s.ends_with("_cap") || s.ends_with("_scree"))
        .map(|([a, b, c], _)| {
            let u = Vec3::from(*b) - Vec3::from(*a);
            let v = Vec3::from(*c) - Vec3::from(*a);
            let n = u.cross(v);
            n.xz().length() / n.y.abs().max(1e-9)
        })
        .fold(0.0f32, f32::max);
    assert!(
        steepest < 1.5,
        "steepest ground {steepest} (the walker's limit is {})",
        MAX_STEP / SUBSTEP
    );
    let g = world_at(&c, &[(0, 0, "massif_granite", 0, 1)]);
    let apothem = HEX_RADIUS * 3f32.sqrt() / 2.0;
    let ring: Vec<Vec2> = (0..6u8)
        .map(|d| {
            let m = Axial::ORIGIN.neighbor(d);
            centre(m.q, m.r).normalize()
        })
        .collect();
    let mut walks: Vec<(Vec2, f32)> = ring.iter().map(|&d| (d, 3.0 * apothem)).collect();
    walks.extend((0..6).map(|i| ((ring[i] + ring[(i + 1) % 6]).normalize(), 2.0 * HEX_RADIUS)));
    for (dir, rim) in walks {
        let start = dir * (rim - 0.6);
        let mut pos = Vec3::new(start.x, g.ground(start).unwrap(), start.y);
        assert!(pos.y - LEVEL_STEP < 0.3, "{dir}: the rim at {pos}");
        let mut foot = None;
        for _ in 0..1200 {
            let s = g.step(pos, -dir * 0.05);
            assert_ne!(
                s.blocked,
                Some(Blocked::Steep),
                "{dir}: refused a step at {pos}"
            );
            pos = s.pos;
            if s.blocked == Some(Blocked::Obstacle("massif_granite".into())) {
                foot = Some(pos);
                break;
            }
        }
        let foot = foot.unwrap_or_else(|| panic!("{dir}: never reached the rock, at {pos}"));
        assert!(
            foot.y - LEVEL_STEP > 7.0,
            "{dir}: the rock's foot at {foot}, {} m up",
            foot.y - LEVEL_STEP
        );
    }
}

/// The single-tile landforms without their scatter, whose boulders and pines would stop a walk
/// short of the rock.
fn bare_landform_catalog() -> TileCatalog {
    let base = catalog();
    let tiles = base.tiles().iter().cloned().map(|mut t| {
        if crate::play::scatter::LANDFORMS.contains(&t.id.as_str()) {
            t.props.scatter.clear();
        }
        t
    });
    TileCatalog::new(base.sockets().clone(), tiles.collect()).unwrap()
}

/// A single-tile landform is a tile that is its own base piece (`hill_meadow` stands on the
/// `hill_meadow` piece). Every one in the catalog is in `LANDFORMS`, so a new hill or peak
/// cannot skip the landform ground, scatter share and rock footprint rows.
#[test]
fn every_single_tile_landform_is_listed() {
    let c = catalog();
    let landforms: Vec<&str> = c
        .tiles()
        .iter()
        .filter(|t| t.footprint.is_none() && t.base.as_str() == t.id)
        .map(|t| t.id.as_str())
        .collect();
    let mut listed = crate::play::scatter::LANDFORMS.to_vec();
    listed.sort_unstable();
    let mut found = landforms.clone();
    found.sort_unstable();
    assert_eq!(found, listed);
}

/// Spec 2026-11-13 D13: a landform at level 1 ringed by level-1 meadows is ground across its hex
/// at the piece's own LOD0 height, turned with the tile, and at the meadows' level along its
/// border, rising under 0.3 m within 0.6 m of it (the massif's rim bar: the grass is flat there and
/// the LOD0 triangles ease it in). Its rock (`FOOTPRINT_OBSTACLES`) blocks the walker. The
/// steepest grass or scree triangle stays under gradient 2, half the walker's limit (measured 1.26
/// on the hill, 1.53 on the peak), and walks in from the meadows on twelve bearings climb without
/// a refused step to the rock: a slab more than 4 m up on the hill (the lowest measures 4.6 m), the
/// peak's rock more than 1 m up (its spine tips come down to the grass 10 m out, 1.1 m up).
#[test]
fn walker_climbs_the_landforms_to_their_rock() {
    let c = bare_landform_catalog();
    let apothem = HEX_RADIUS * 3f32.sqrt() / 2.0;
    for id in crate::play::scatter::LANDFORMS {
        let lowest_foot = if id == "hill_meadow" { 4.0 } else { 1.0 };
        let def = c.get(id).unwrap();
        let tris =
            crate::play::slope_ground::tile_ground_tris(def, &c, &mut |p| mesh_tris(p, "lod0"))
                .unwrap();
        let steepest = tris
            .iter()
            .filter(|(_, s)| s.ends_with("_cap") || s.ends_with("_scree"))
            .map(|([a, b, d], _)| {
                let u = Vec3::from(*b) - Vec3::from(*a);
                let v = Vec3::from(*d) - Vec3::from(*a);
                let n = u.cross(v);
                n.xz().length() / n.y.abs().max(1e-9)
            })
            .fold(0.0f32, f32::max);
        assert!(
            steepest < 2.0,
            "{id}: steepest ground {steepest} (the walker's limit is {})",
            MAX_STEP / SUBSTEP
        );
        let parts = FOOTPRINT_OBSTACLES
            .iter()
            .find(|(t, _, _)| *t == id)
            .map(|(_, _, p)| *p)
            .unwrap();
        let mut tiles: Vec<(i32, i32, &str, u8, u8)> = vec![(0, 0, id, 0, 1)];
        for d in 0..6u8 {
            let m = Axial::ORIGIN.neighbor(d);
            tiles.push((m.q, m.r, "meadow", 0, 1));
        }
        for k in [0u8, 1, 5] {
            tiles[0].3 = k;
            let g = world_at(&c, &tiles);
            assert_eq!(g.obstacles[&Axial::ORIGIN].len(), parts.len(), "{id} k{k}");
            for b in 0..12 {
                let dir = Vec2::from_angle(b as f32 * std::f32::consts::FRAC_PI_6);
                for r in [0.0, 3.0, 6.0, 9.0, 12.0] {
                    let p = dir * r;
                    let want = massif_ground(&tris, k, p);
                    let got = g.ground(p).unwrap();
                    assert!(
                        (got - want).abs() < 0.02,
                        "{id} k{k} at {p}: ground {got}, the piece's {want}"
                    );
                }
            }
            for d in 0..6u8 {
                let m = Axial::ORIGIN.neighbor(d);
                let n = centre(m.q, m.r).normalize();
                for s in [-6.0, -3.0, 0.0, 3.0, 6.0] {
                    for (inset, most) in [(0.05, 0.05), (0.6, 0.3)] {
                        let p = n * (apothem - inset) + n.perp() * s * (1.0 - inset / apothem);
                        let y = g.ground(p).unwrap() - LEVEL_STEP;
                        assert!(
                            y.abs() < most,
                            "{id} k{k}: {inset} m inside edge {d} at {p}, {y} m up"
                        );
                    }
                }
            }
            for part in parts {
                let n = part.len() as f32;
                let mid = part.iter().fold(Vec2::ZERO, |a, p| a + Vec2::from(*p)) / n;
                let w = turn(k, [mid.x, 0.0, mid.y]);
                assert_eq!(
                    g.stand(Vec2::new(w[0], w[2])),
                    Err(Blocked::Obstacle(id.into())),
                    "{id} k{k}: the rock at {mid}"
                );
            }
        }
        tiles[0].3 = 0;
        let g = world_at(&c, &tiles);
        for b in 0..12 {
            let dir = Vec2::from_angle(b as f32 * std::f32::consts::FRAC_PI_6);
            let start = dir * (HEX_RADIUS + 2.0);
            let mut pos = Vec3::new(start.x, g.ground(start).unwrap(), start.y);
            let mut rock = None;
            for _ in 0..640 {
                let s = g.step(pos, -dir * 0.05);
                assert_ne!(
                    s.blocked,
                    Some(Blocked::Steep),
                    "{id} {dir}: refused a step at {pos}"
                );
                pos = s.pos;
                if s.blocked == Some(Blocked::Obstacle(id.into())) {
                    rock = Some(pos);
                    break;
                }
            }
            let foot = rock.unwrap_or_else(|| panic!("{id} {dir}: never reached the rock"));
            assert!(
                foot.y - LEVEL_STEP > lowest_foot,
                "{id} {dir}: the rock's foot at {foot}"
            );
        }
    }
}

/// The massif's ring scatter blocks the walker where it is drawn, in every ring cell, and a
/// body touching a ring prop from any side (an outer neighbour's cell included) meets it: a
/// boulder added half a metre inside ring cell (1, 0)'s outer edge reaches across it.
#[test]
fn ring_scatter_blocks_the_walker() {
    let c = catalog();
    let def = c.get("massif_granite").unwrap();
    let rule = def
        .props
        .scatter
        .iter()
        .position(|r| r.piece.as_str() == "boulder_granite")
        .unwrap() as u8;
    let edge = Instance {
        rule,
        pos: Vec3::new(1.5 * 3f32.sqrt() * HEX_RADIUS - 0.5, 0.0, 0.0),
        yaw: 0.0,
        scale: 1.0,
        order: 0,
        variant: 0,
    };
    for k in [0u8, 3] {
        let g = world_with(
            &c,
            &[(0, 0, "massif_granite", k, 1)],
            &[(Axial::ORIGIN, edge.clone())],
        );
        let outside = g
            .obstacles
            .values()
            .flatten()
            .flat_map(|o| o.poly.iter())
            .filter(|v| v.length() > 1.5 * 3f32.sqrt() * HEX_RADIUS)
            .count();
        assert!(outside > 0, "k{k}: no ring prop reaches past the ring");
        let props: Vec<&Obstacle> = g
            .obstacles
            .values()
            .flatten()
            .filter(|o| o.piece != "massif_granite")
            .collect();
        let mut cells = std::collections::BTreeSet::new();
        for o in &props {
            let mid = o.poly.iter().copied().sum::<Vec2>() / o.poly.len() as f32;
            let cell = cell_at(mid);
            assert_ne!(cell, Axial::ORIGIN, "k{k}: {} on the rock cell", o.piece);
            cells.insert(cell);
            for v in &o.poly {
                let p = *v + (*v - mid).normalize_or_zero() * (BODY_RADIUS * 0.5);
                assert!(
                    g.obstacle(p).is_some(),
                    "k{k}: a body at {p} ({:?}) misses the {} there",
                    cell_at(p),
                    o.piece
                );
            }
        }
        assert_eq!(cells.len(), 6, "k{k}: props on every ring cell: {cells:?}");
    }
}

/// A rail end facing a massif's ring cell is open (the massif carries no rail) and keeps its
/// buffer stop, as an end facing an empty cell does.
#[test]
fn a_rail_end_at_a_massif_member_keeps_its_stop() {
    let g = world_at(
        &catalog(),
        &[
            (0, 0, "massif_granite", 0, 0),
            (2, 0, "rail_straight", 0, 0),
        ],
    );
    let stops = g
        .obstacles
        .get(&Axial::new(2, 0))
        .map_or(0, |o| o.iter().filter(|o| o.piece == BUFFER_STOP).count());
    assert_eq!(stops, 2, "both open ends keep their stops");
}

/// The walk ground's field boundaries read a footprint's ring cell as the footprint: a farm
/// whose only land neighbour is the footprint meadow's ring cell (2, 0) is walled there.
#[test]
fn walk_fences_read_a_ring_cell_as_its_footprint() {
    let c = crate::play::tests::footprint_fixtures();
    let g = world_in(&c, &[(3, 0, "meadow", 0), (1, 0, "farm", 0)]);
    let fences = g.obstacles.get(&Axial::new(1, 0)).map_or(0, |o| {
        o.iter()
            .filter(|o| hex_core::boundary::PIECES.contains(&o.piece.as_str()))
            .count()
    });
    assert!(fences > 0, "no boundary toward the ring cell");
}
