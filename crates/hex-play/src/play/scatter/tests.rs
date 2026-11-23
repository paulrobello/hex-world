use hex_play_state::locked;
#[test]
fn portable_atan2_matches_atan2() {
    let mut worst = 0.0f32;
    for i in 0..3600 {
        let a = i as f32 / 3600.0 * std::f32::consts::TAU;
        for r in [0.01f32, 1.0, 37.5] {
            let (y, x) = (a.sin() * r, a.cos() * r);
            worst = worst.max((portable_atan2(y, x) - y.atan2(x)).abs());
        }
    }
    assert!(worst < 5e-5, "worst {worst}");
    assert_eq!(portable_atan2(0.0, 0.0), 0.0);
}

use std::sync::{Mutex, OnceLock};

use hex_assets::parse_manifest;
use hex_core::piece::PieceTable;
use hex_core::{AgentKind, SocketKind, World};

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    f32::consts::TAU,
};

use bevy::prelude::*;
use hex_core::{Axial, HEX_RADIUS, NetworkKind, ScatterRule, TileCatalog, TileDef, cell_seed};

use super::super::{APOTHEM, KitPart};
use super::{bands::*, layout::*, surface::*, sync::*, tables::*};
use crate::water::tiles::Coastline;
use hex_render::kit_runtime::{MANIFEST, piece_rotation};

type Tris = Option<Vec<(Tri, String)>>;

fn catalog() -> TileCatalog {
    TileCatalog::builtin().unwrap()
}

/// The index of `def`'s scatter rule naming `piece` (the pine forest and the meadow lead with
/// their granite outcrop and boulders).
fn rule_of(def: &TileDef, piece: &str) -> u8 {
    def.props
        .scatter
        .iter()
        .position(|r| r.piece.as_str() == piece)
        .unwrap() as u8
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

/// Triangles of mesh `<id>_<mesh>` from the piece's GLB, with slot names (the kit gate's
/// `triangles_with_slot`).
fn glb_mesh_tris(id: &str, mesh: &str) -> Tris {
    let (doc, buffers) = glb(id)?;
    let name = format!("{id}_{mesh}");
    let m = doc.meshes().find(|m| m.name() == Some(name.as_str()))?;
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
}

/// LOD0 triangles, read once per piece for the whole test binary.
fn lod0(id: &str) -> Tris {
    static CACHE: OnceLock<Mutex<HashMap<String, Tris>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(t) = locked(cache).get(id) {
        return t.clone();
    }
    let t = glb_mesh_tris(id, "lod0");
    locked(cache).insert(id.to_owned(), t.clone());
    t
}

fn prepared(def: &TileDef) -> Option<Prepared> {
    prepare(def, &catalog(), &mut |id| lod0(id))
}

/// The massif scatters each ring cell as a meadow (its first rules are meadow's): over 64 seeds,
/// each rule places as many instances per m² of its mask (`Layout::area`) on a ring cell as on a
/// meadow with the same seeds, within three standard errors of the per-tile rounding on both
/// sides plus 3%, and at least 99% of its planned count. Every instance's ground footprint is
/// on the grass cap, off the rock and scree, and the centre cell (the rock) has none.
#[test]
fn ring_scatter_matches_meadow_density() {
    let c = catalog();
    let meadow = c.get(RING_GROUND).unwrap();
    let def = c.get("massif_granite").unwrap();
    let rules = &meadow.props.scatter;
    assert_eq!(&def.props.scatter[..rules.len()], &rules[..]);
    let g = prepared(def).unwrap();
    let mg = prepared(meadow).unwrap();
    assert_eq!(g.ring.len(), 6);
    let seeds = 64u64;
    for (m, at, cell) in &g.ring {
        let n = rules.len();
        let (mut ring, mut base) = (vec![0.0f64; n], vec![0.0f64; n]);
        let (mut placed, mut planned) = (vec![0usize; n], vec![0usize; n]);
        for seed in 0..seeds {
            let seed = ring_seed(cell_seed(1, Axial::new(seed as i32, -(seed as i32))), *m);
            let l = cell_layout(def, seed, cell);
            let ml = cell_layout(meadow, seed, &mg);
            for ri in 0..n {
                planned[ri] += l.planned[ri] as usize;
                placed[ri] += l.placed(ri);
                ring[ri] += l.placed(ri) as f64 / f64::from(l.area[ri]);
                base[ri] += ml.placed(ri) as f64 / f64::from(ml.area[ri]);
            }
            for i in &l.instances {
                let piece = i.piece(def.props.scatter[i.rule as usize].piece.as_str());
                let reach = footprint(piece).ground;
                let p = i.pos.xz();
                for q in std::iter::once(p).chain(ring16().map(|d| p + reach * d)) {
                    let (_, slot) = cell.surface.at(q.x, q.y).unwrap();
                    assert_eq!(slot, MASSIF_CAP, "{m:?} {piece} at {} on {slot}", p + *at);
                }
            }
        }
        println!("ring cell {m:?}: placed {placed:?} of {planned:?}");
        for (ri, r) in rules.iter().enumerate() {
            // A tile plans floor(want + u): Bernoulli on want's fraction, per m² of its mask.
            let se = |want: f64, area: f32| {
                let f = want.fract();
                (f * (1.0 - f) / seeds as f64).sqrt() / f64::from(area)
            };
            let want = f64::from(r.density * CAP_AREA / 100.0);
            let (ra, ma) = (cell.masks[ri].area, mg.masks[ri].area);
            let tol = 3.0
                * (se(want * f64::from(cell.share[ri]), ra).powi(2) + se(want, ma).powi(2)).sqrt()
                + 0.03 * base[ri] / seeds as f64;
            let (ring, base) = (ring[ri] / seeds as f64, base[ri] / seeds as f64);
            assert!(
                (ring - base).abs() <= tol,
                "{m:?} {}: {ring} per m² of mask vs meadow's {base} (tolerance {tol})",
                r.piece.as_str()
            );
            // A drift's quarter of a ring cell's smaller ground may not fit every clump.
            assert!(
                placed[ri] * 100 >= planned[ri] * 99,
                "{m:?} {}: placed {} of {}",
                r.piece.as_str(),
                placed[ri],
                planned[ri]
            );
        }
    }
    let l = layout(def, 7, &g);
    assert!(!l.instances.is_empty());
    assert!(l.instances.iter().all(|i| hex_reach(i.pos.xz()) >= APOTHEM));
    let per_cell: usize = g
        .ring
        .iter()
        .map(|(m, _, cell)| cell_layout(def, ring_seed(7, *m), cell).instances.len())
        .sum();
    assert_eq!(l.instances.len(), per_cell);
}

/// The massif's lower-slope pines (owner A54 Q2 O1): spruces and saplings on the grass cap in
/// [`PINE_BAND`] from its centre, in every ring cell over a few seeds, never on a single tile's
/// meadow-like cap.
#[test]
fn massif_pines_stand_on_the_lower_slopes() {
    let c = catalog();
    let def = c.get("massif_granite").unwrap();
    let g = prepared(def).unwrap();
    let pine = |r: u8| {
        matches!(
            family(def.props.scatter[r as usize].piece.as_str()),
            "tree_forest" | "sapling_spruce"
        )
    };
    let mut cells = HashSet::new();
    let (mut n, mut raised) = (0, 0);
    for seed in 0..16u64 {
        for i in layout(def, cell_seed(3, Axial::new(seed as i32, 0)), &g)
            .instances
            .iter()
            .filter(|i| pine(i.rule))
        {
            let d = i.pos.xz().length();
            assert!((PINE_BAND.0..=PINE_BAND.1).contains(&d), "a pine {d} m out");
            let (k, (_, at, cell)) = g
                .ring
                .iter()
                .enumerate()
                .find(|(_, (_, at, _))| hex_reach(i.pos.xz() - *at) < APOTHEM)
                .unwrap();
            let q = i.pos.xz() - *at;
            let (y, slot) = cell.surface.at(q.x, q.y).unwrap();
            assert_eq!(slot, MASSIF_CAP);
            // On the piece's ground (`stand`: the lowest of its centre and six points 0.3 m
            // round it, so up to a steep flank's drop below the centre), not the tile's level:
            // the flanks (owner A55 Q1 O1) carry most pines metres up.
            assert!(
                i.pos.y <= y + 1e-4 && i.pos.y > y - 0.6,
                "a pine at {} over ground {y}",
                i.pos
            );
            raised += usize::from(i.pos.y > 2.0);
            cells.insert(k);
            n += 1;
        }
    }
    assert_eq!(cells.len(), 6, "{n} pines over {cells:?}");
    assert!(
        2 * raised > n,
        "{raised} of {n} pines stand over 2 m up the flanks"
    );
    assert!(ring_band(c.get("forest").unwrap(), "tree_forest").is_none());
}

/// Spec 2026-11-13 D9: the single-tile hill and peak scatter Meadow's rules (theirs lead with
/// Meadow's, in its order) at a Meadow's density per m² of free ground. Over 64 seeds each rule
/// places as many instances per m² of its mask as a Meadow with the same seeds, within three
/// standard errors of the per-tile rounding on both sides plus 3%, and at least 99% of its
/// planned count. Every instance's ground footprint stands on the grass cap, off the rock and
/// scree.
#[test]
fn landform_scatter_matches_meadow_density() {
    let c = catalog();
    let meadow = c.get(RING_GROUND).unwrap();
    let rules = &meadow.props.scatter;
    let mg = prepared(meadow).unwrap();
    let seeds = 64u64;
    for id in LANDFORMS {
        let def = c.get(id).unwrap();
        assert_eq!(&def.props.scatter[..rules.len()], &rules[..], "{id}");
        let g = prepared(def).unwrap();
        assert_eq!(g.share.len(), def.props.scatter.len(), "{id}");
        let cap = format!("{id}_cap");
        let n = rules.len();
        let (mut hill, mut base) = (vec![0.0f64; n], vec![0.0f64; n]);
        let (mut placed, mut planned) = (vec![0usize; n], vec![0usize; n]);
        for seed in 0..seeds {
            let seed = cell_seed(1, Axial::new(seed as i32, -(seed as i32)));
            let l = layout(def, seed, &g);
            let ml = layout(meadow, seed, &mg);
            for ri in 0..n {
                planned[ri] += l.planned[ri] as usize;
                placed[ri] += l.placed(ri);
                hill[ri] += l.placed(ri) as f64 / f64::from(l.area[ri]);
                base[ri] += ml.placed(ri) as f64 / f64::from(ml.area[ri]);
            }
            for i in &l.instances {
                let piece = i.piece(def.props.scatter[i.rule as usize].piece.as_str());
                let reach = footprint(piece).ground;
                let p = i.pos.xz();
                for q in std::iter::once(p).chain(ring16().map(|d| p + reach * d)) {
                    let (_, slot) = g.surface.at(q.x, q.y).unwrap();
                    if (i.rule as usize) < n {
                        assert_eq!(slot, cap, "{id}: {piece} at {p} on {slot}");
                    } else {
                        assert_eq!(slot, PEAK_CAP, "{id}: {piece} at {p} on {slot}");
                    }
                }
            }
        }
        println!(
            "{id}: share {:?}, placed {placed:?} of {planned:?}",
            &g.share[..n]
        );
        for (ri, r) in rules.iter().enumerate() {
            let se = |want: f64, area: f32| {
                let f = want.fract();
                (f * (1.0 - f) / seeds as f64).sqrt() / f64::from(area)
            };
            let want = f64::from(r.density * CAP_AREA / 100.0);
            let (ha, ma) = (g.masks[ri].area, mg.masks[ri].area);
            // An outcrop's footprint finds no grass on the peak flat and wide enough for it.
            if ha <= 0.0 {
                assert_eq!(g.share[ri], 0.0, "{id} {}", r.piece.as_str());
                assert_eq!(planned[ri], 0, "{id} {}", r.piece.as_str());
                continue;
            }
            let tol = 3.0
                * (se(want * f64::from(g.share[ri]), ha).powi(2) + se(want, ma).powi(2)).sqrt()
                + 0.03 * base[ri] / seeds as f64;
            let (hill, base) = (hill[ri] / seeds as f64, base[ri] / seeds as f64);
            assert!(
                (hill - base).abs() <= tol,
                "{id} {}: {hill} per m² of mask vs meadow's {base} (tolerance {tol})",
                r.piece.as_str()
            );
            assert!(
                placed[ri] * 100 >= planned[ri] * 99,
                "{id} {}: placed {} of {}",
                r.piece.as_str(),
                placed[ri],
                planned[ri]
            );
        }
    }
    // Every other single tile keeps its own densities.
    assert!(prepared(meadow).unwrap().share.is_empty());
}

/// Spec 2026-11-13 D9: the peak's pines stand on its grass cap in [`PEAK_PINE_BAND`] from its
/// centre, on the slope (not the tile's level), and the hill has none.
#[test]
fn peak_pines_stand_on_the_lower_slopes() {
    let c = catalog();
    let def = c.get("peak_granite").unwrap();
    let g = prepared(def).unwrap();
    let pine = |r: u8| {
        matches!(
            family(def.props.scatter[r as usize].piece.as_str()),
            "tree_forest" | "sapling_spruce"
        )
    };
    let (mut n, mut raised) = (0, 0);
    for seed in 0..32u64 {
        for i in layout(def, cell_seed(3, Axial::new(seed as i32, 0)), &g)
            .instances
            .iter()
            .filter(|i| pine(i.rule))
        {
            let d = i.pos.xz().length();
            assert!(
                (PEAK_PINE_BAND.0..=PEAK_PINE_BAND.1).contains(&d),
                "a pine {d} m out"
            );
            let (y, slot) = g.surface.at(i.pos.x, i.pos.z).unwrap();
            assert_eq!(slot, PEAK_CAP);
            assert!(
                i.pos.y <= y + 1e-4 && i.pos.y > y - 0.6,
                "a pine at {} over ground {y}",
                i.pos
            );
            raised += usize::from(i.pos.y > 0.5);
            n += 1;
        }
    }
    assert!(n >= 32, "{n} pines over 32 seeds");
    assert!(2 * raised > n, "{raised} of {n} pines stand on the slope");
    let hill = c.get("hill_meadow").unwrap();
    assert!(
        hill.props
            .scatter
            .iter()
            .all(|r| ring_band(hill, r.piece.as_str()).is_none())
    );
    assert!(ring_band(c.get("meadow").unwrap(), "tree_forest").is_none());
}

/// `Surface::top`, with each triangle's divisor found once in `Surface::new`, answers as a scan
/// of every triangle whose (x, z) box meets the point's 1 m bucket, in order, computing it.
#[test]
fn top_matches_a_scan_of_its_bucket() {
    let c = catalog();
    for id in [
        "meadow",
        "road_junction",
        "rail_road_crossing",
        "river_rail_bridge",
        "beach",
    ] {
        let s = prepared(c.get(id).unwrap()).unwrap().surface;
        let scan = |x: f32, z: f32| {
            if x.abs() >= EXTENT || z.abs() >= EXTENT {
                return None;
            }
            let mut best: Option<(f32, u16)> = None;
            for ([a, b, cc], _, slot) in &s.tris {
                let t = [*a, *b, *cc];
                let lo = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MAX, f32::min));
                let hi = |k: usize| bucket(t.iter().map(|p| p[k]).fold(f32::MIN, f32::max));
                if !(lo(0)..=hi(0)).contains(&bucket(x)) || !(lo(2)..=hi(2)).contains(&bucket(z)) {
                    continue;
                }
                let d = (b[0] - a[0]) * (cc[2] - a[2]) - (cc[0] - a[0]) * (b[2] - a[2]);
                let u = ((x - a[0]) * (cc[2] - a[2]) - (cc[0] - a[0]) * (z - a[2])) / d;
                let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
                if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
                    continue;
                }
                let y = a[1] + u * (b[1] - a[1]) + v * (cc[1] - a[1]);
                if best.is_none_or(|(h, _)| y > h) {
                    best = Some((y, *slot));
                }
            }
            best
        };
        let mut rng = Rng(7);
        for _ in 0..20_000 {
            let (x, z) = (rng.unit() * 34.0 - 17.0, rng.unit() * 34.0 - 17.0);
            assert_eq!(s.top(x, z), scan(x, z), "{id} at ({x}, {z})");
        }
        // Points on and beside the bucket edges and quarter lines.
        for k in 0..=(EXTENT * 8.0) as i32 {
            let v = k as f32 * 0.25 - EXTENT;
            for e in [-1e-4, 0.0, 1e-4] {
                for w in [-7.3, 0.05, 11.9] {
                    assert_eq!(s.top(v + e, w), scan(v + e, w), "{id} at ({}, {w})", v + e);
                    assert_eq!(s.top(w, v + e), scan(w, v + e), "{id} at ({w}, {})", v + e);
                }
            }
        }
    }
}

fn rule(piece: &str, density: f32) -> ScatterRule {
    ScatterRule {
        piece: piece.into(),
        density,
        lane_clearance: 0.0,
        drift: false,
        vary: 0.0,
    }
}

/// Owner report 2026-10-19: a river_straight drew its banks bare of grass until a hard reload.
/// A ground piece whose GLB failed for good (its pack ready, the piece never in the kit:
/// `ground_tris` gives it no triangles) must not hold the tile's ground back for the session;
/// a piece still streaming (`None`) still does.
#[test]
fn a_lost_ground_piece_does_not_hold_the_ground_back() {
    let c = catalog();
    let def = c.get("river_straight").unwrap();
    let centre = "river_edge";
    let kit: HashMap<String, Vec<KitPart>> = ground_pieces(def, &c)
        .into_iter()
        .filter(|(id, _)| *id != centre)
        .map(|(id, _)| (id.to_string(), vec![]))
        .collect();
    let pending = PlayScatter::default().ground(def, &c, &kit, &mut |id| {
        if id == centre { None } else { lod0(id) }
    });
    assert!(pending.is_none(), "a streaming centre waits");
    let g = PlayScatter::default()
        .ground(def, &c, &kit, &mut |id| {
            if id == centre { Some(vec![]) } else { lod0(id) }
        })
        .expect("the centre failed for good: the base and edges still make the ground");
    let m = parse_manifest(MANIFEST).unwrap();
    let slots = crate::play::grass::grass_slots(&m, def, &c);
    let blades = crate::play::grass::layout(7, &g.surface, &slots, &[])
        .blades
        .len();
    assert!(blades > 5000, "{blades} blades on the banks");
}

/// A hillside's ground (P4) is the runtime cap over its slope field, not the flat baked
/// meadow cap: scatter, grass and walkers stand on the slope.
#[test]
fn sloped_ground_tris_match_the_field() {
    let c = catalog();
    let def = c.get("hillside_000111").unwrap();
    let kit: HashMap<String, Vec<KitPart>> = ground_pieces(def, &c)
        .into_iter()
        .map(|(id, _)| (id.to_string(), vec![]))
        .collect();
    let g = PlayScatter::default()
        .ground(def, &c, &kit, &mut |id| lod0(id))
        .expect("meadow_base is in the kit");
    let corners = def.corners(0);
    let mut worst = 0.0f32;
    for i in 0..4 {
        for j in 0..4 {
            let (x, z) = (-9.0 + 6.0 * i as f32, -9.0 + 6.0 * j as f32);
            let (y, slot) = g.surface.at(x, z).expect("ground under an interior point");
            let want = hex_core::field_at(def.ground, corners, x, z);
            worst = worst.max((y - want).abs());
            assert!((y - want).abs() < 0.05, "({x}, {z}): {y} vs field {want}");
            assert_eq!(slot, "meadow_base_cap");
        }
    }
    println!("hillside_000111 ground vs field: worst {worst:.4} m");
}

/// Owner report 2026-10-30: the tunnel portal's road ended short of the arch, with grass
/// between its end and the mouth. The road piece listed on its lane-less edge 0 is ground,
/// so from the centre piece's end to the portal face (x = apothem − 2.2) the corridor is road
/// at the cutting's level 0, and no grass or scatter grows there.
#[test]
fn the_portal_road_runs_on_to_the_arch() {
    let c = catalog();
    let def = c.get("road_tunnel_portal").unwrap();
    assert!(ground_pieces(def, &c).contains(&("road_straight_edge", 0)));
    let kit: HashMap<String, Vec<KitPart>> = ground_pieces(def, &c)
        .into_iter()
        .map(|(id, _)| (id.to_string(), vec![]))
        .collect();
    let g = PlayScatter::default()
        .ground(def, &c, &kit, &mut |id| lod0(id))
        .expect("the portal's ground pieces are in the kit");
    for x in [7.5f32, 9.0, 10.5, 11.4] {
        for z in [-2.5f32, 0.0, 2.5] {
            let (y, slot) = g.surface.at(x, z).expect("ground in the corridor");
            assert!(
                matches!(slot, "road_asphalt" | "road_paint"),
                "({x}, {z}): {slot}"
            );
            assert!(y > 0.0 && y < 0.2, "({x}, {z}): {y} m above the corridor");
        }
    }
}

/// Visual standard (P4): a hillside carries the Meadow's scatter list, so over a few seeds
/// each rule places about as many props and the grass about as many blades as on the flat
/// Meadow beside it.
#[test]
fn hillsides_scatter_and_grass_like_the_meadow() {
    let c = catalog();
    let m = parse_manifest(MANIFEST).unwrap();
    let counts = |id: &str| {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let slots = crate::play::grass::grass_slots(&m, def, &c);
        let mut per_rule = vec![0usize; def.props.scatter.len()];
        let mut blades = 0;
        for seed in 0..4u64 {
            let l = layout(def, seed * 7919 + 3, &g);
            for (i, n) in per_rule.iter_mut().enumerate() {
                *n += l.placed(i);
            }
            blades += crate::play::grass::layout(seed, &g.surface, &slots, &[])
                .blades
                .len();
        }
        (def.props.scatter.clone(), per_rule, blades)
    };
    let (rules, meadow, meadow_blades) = counts("meadow");
    let mut rows = vec![];
    for def in c.tiles().iter().filter(|d| d.id.starts_with("hillside_")) {
        let (hill_rules, hill, blades) = counts(&def.id);
        assert_eq!(
            hill_rules.iter().map(|r| &r.piece).collect::<Vec<_>>(),
            rules.iter().map(|r| &r.piece).collect::<Vec<_>>(),
            "{}: the meadow's scatter list",
            def.id
        );
        let ratio = |a: usize, b: usize| a as f32 / b.max(1) as f32;
        let grass = ratio(blades, meadow_blades);
        rows.push(format!("{} grass {grass:.3}", def.id));
        assert!((0.85..1.15).contains(&grass), "{}: grass {grass}", def.id);
        for (i, r) in rules.iter().enumerate() {
            let k = ratio(hill[i], meadow[i]);
            rows.push(format!(
                "  {} {}/{} = {k:.3}",
                r.piece.as_str(),
                hill[i],
                meadow[i]
            ));
            // Rules that place a handful per tile (granite outcrops, boulders) vary by
            // whole props between seeds: compare those by count, the rest by ratio.
            if meadow[i] >= 40 {
                assert!(
                    (0.85..1.15).contains(&k),
                    "{} {}: {k}",
                    def.id,
                    r.piece.as_str()
                );
            } else {
                assert!(
                    hill[i].abs_diff(meadow[i]) <= 6,
                    "{} {}",
                    def.id,
                    r.piece.as_str()
                );
            }
        }
    }
    println!("hillside vs meadow (4 seeds):\n{}", rows.join("\n"));
}

#[test]
fn the_same_seed_gives_the_same_instances() {
    let c = catalog();
    for id in ["forest", "oak", "meadow"] {
        let def = c.get(id).unwrap();
        let g = prepared(def).expect("ground pieces in the kit");
        let a = layout(def, 7, &g);
        assert!(!a.instances.is_empty(), "{id}");
        assert_eq!(a, layout(def, 7, &g), "{id}: same seed");
        // A second preparation of the same ground scatters identically too.
        assert_eq!(a, layout(def, 7, &prepared(def).unwrap()), "{id}");
        let b = layout(def, 8, &g);
        assert_ne!(a.instances, b.instances, "{id}: another seed");
    }
    // Seeded from the world seed and the cell: `PlacedTile::seed`.
    let w = World::new(3);
    let origin = w.tiles()[&Axial::ORIGIN].seed;
    assert_eq!(origin, cell_seed(3, Axial::ORIGIN));
    assert_ne!(
        cell_seed(3, Axial::new(1, 0)),
        cell_seed(4, Axial::new(1, 0))
    );
}

/// Every MVP tile, every rotation: rules that declare no scatter get one rule per scatter piece,
/// so roads, rivers, beaches, the ocean and the house pad are exercised too. Each instance's
/// centre and footprint ring, turned with the tile, stand on its piece's floor in the tile's
/// turned ground, never on an excluded slot, clear of heroes, lanes and the border. A
/// footprint tile's instances are read on the cell they stand in, in that cell's frame.
#[test]
fn no_instance_on_an_excluded_slot_for_every_mvp_tile_and_rotation() {
    let c = catalog();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let pending = hex_core::PendingPieces::builtin().unwrap();
    let mut awaiting = 0;
    for base in c.tiles() {
        // A tile built ahead of its centre piece (data/pending-pieces.ron) has no ground yet.
        if base
            .center
            .as_ref()
            .is_some_and(|k| pending.contains(k.as_str()))
        {
            awaiting += 1;
            continue;
        }
        let mut def = base.clone();
        if def.props.scatter.is_empty() {
            def.props.scatter = vec![
                rule("tree_forest", 2.0),
                rule("rock_small", 1.0),
                rule("wildflower_clump", 3.0),
            ];
        }
        let g = prepared(&def).unwrap_or_else(|| panic!("{}: ground pieces missing", def.id));
        let local: Vec<(Tri, String)> =
            crate::play::slope_ground::tile_ground_tris(&def, &c, &mut |id| lod0(id)).unwrap();
        let lanes = lanes(&def);
        // Each cell's ground in its own frame, with its centre's offset from the tile's.
        let cells: Vec<(Vec2, Vec<(Tri, String)>)> = if def.is_mega() {
            hex_core::footprint::members(Axial::ORIGIN)
                .map(|m| {
                    let at = Vec2::from(m.to_world(HEX_RADIUS));
                    (at, member_tris(&local, at))
                })
                .to_vec()
        } else {
            vec![(Vec2::ZERO, local.clone())]
        };
        for k in 0..6u8 {
            let rot = piece_rotation(k);
            let worlds: Vec<(Vec2, Surface)> = cells
                .iter()
                .map(|(at, tris)| {
                    let turned = tris
                        .iter()
                        .map(|(t, s)| (t.map(|p| (rot * Vec3::from(p)).to_array()), s.clone()));
                    (*at, Surface::new(turned))
                })
                .collect();
            for seed in 0..4u64 {
                let l = layout(&def, seed * 7919 + u64::from(k), &g);
                *counts.entry(def.id.clone()).or_default() += l.instances.len();
                for tile_inst in &l.instances {
                    let (at, world) = worlds
                        .iter()
                        .min_by(|a, b| {
                            let reach = |o: Vec2| hex_reach(tile_inst.pos.xz() - o);
                            reach(a.0).total_cmp(&reach(b.0))
                        })
                        .unwrap();
                    let inst = &Instance {
                        pos: tile_inst.pos - Vec3::new(at.x, 0.0, at.y),
                        ..tile_inst.clone()
                    };
                    let r = &def.props.scatter[inst.rule as usize];
                    let piece = r.piece.as_str();
                    let fp = footprint(inst.piece(piece));
                    let floors = floor_slots(inst.piece(piece));
                    let mut probes = vec![Vec3::ZERO];
                    probes.extend(
                        ring16()
                            .iter()
                            .step_by(2)
                            .map(|d| Vec3::new(d.x, 0.0, d.y) * fp.ground),
                    );
                    for off in probes {
                        let p = rot * (inst.pos + off);
                        let (h, slot) = world.at(p.x, p.z).unwrap_or_else(|| {
                            panic!("{} k{k}: {piece} over no ground at {p}", def.id)
                        });
                        assert!(
                            floor_ok(floors, slot),
                            "{} k{k} seed {seed}: {piece} at {} over {slot}",
                            def.id,
                            inst.pos
                        );
                        // A water prop stands on the water, not the bed
                        // (`water_props_stand_on_the_water`).
                        if off == Vec3::ZERO && !water_prop(piece) {
                            // It stands at the lowest point of its 0.3 m ring (`stand`),
                            // which on a P4 slope may lie more than 0.1 m below its centre.
                            let ring_low = (0..6u8)
                                .filter_map(|j| {
                                    let d = hex_dir(j);
                                    let q = rot * (inst.pos + Vec3::new(d.x, 0.0, d.y) * 0.3);
                                    world.at(q.x, q.z).map(|(y, _)| y)
                                })
                                .fold(h - 0.1, f32::min);
                            assert!(
                                inst.pos.y <= h + 1e-4 && inst.pos.y > ring_low - 1e-3,
                                "{} {piece}: y {} vs surface {h}",
                                def.id,
                                inst.pos.y
                            );
                        }
                    }
                    let p = inst.pos.xz();
                    assert!(hex_reach(p) <= border_reach(fp) + 0.2, "{} border", def.id);
                    // Every piece a hero slot may draw at any cell is cleared, a fence by
                    // its line.
                    for line in fence_lines(&def) {
                        let need = fence_clearance(fp.ground * inst.scale);
                        assert!(
                            polyline_distance(p, &line) >= need - 1e-4,
                            "{} fence",
                            def.id
                        );
                    }
                    for (i, h) in def.props.hero.iter().enumerate() {
                        for piece in def.hero_options(i).filter(|p| fence(p).is_none()) {
                            let need = hero_clearance(footprint(piece), fp);
                            assert!(
                                p.distance(Vec2::from(h.at)) >= need - 0.2,
                                "{} hero {piece}",
                                def.id
                            );
                        }
                    }
                    for (n, (line, half)) in def.networks.iter().zip(&lanes) {
                        if n.kind == NetworkKind::River
                            && (bank_band(piece).is_some() || water_prop(piece))
                        {
                            continue;
                        }
                        let need = lane_clearance(r, *half, fp);
                        assert!(polyline_distance(p, line) >= need - 0.2, "{} lane", def.id);
                    }
                }
            }
        }
    }
    assert_eq!(
        counts.len() + awaiting,
        c.tiles().len(),
        "every inventory tile ran or awaits its piece"
    );
    for id in [
        "forest",
        "oak",
        "windmill",
        "farm",
        "wheat_field",
        "meadow",
        "hill_meadow",
        "peak_granite",
        "road_straight",
        "house_cluster",
        "beach",
        "beach_inner",
        "beach_outer",
        "river_mouth",
        "river_mouth_l",
        "river_straight",
        "river_bend",
        "river_bend_60",
        "river_fork",
        "ocean",
    ] {
        assert!(counts[id] > 0, "{id} scatters: {counts:?}");
    }
}

/// Density is per 100 m² of the hex cap: the mean count over cells matches it within three
/// standard errors of the per-tile rounding (plus 3%), every
/// tile places its whole planned count, and same-piece neighbours keep the rule's spacing: the
/// blue-noise spacing over its ground (a drift's share of it), never under two ground reaches
/// ([`DRIFT_PACK`] in a drift).
#[test]
fn density_is_per_100_m2_of_cap_and_spacing_is_blue_noise() {
    let c = catalog();
    for id in ["forest", "oak", "meadow"] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let seeds = 64u64;
        let mut sums = vec![0usize; def.props.scatter.len()];
        for seed in 0..seeds {
            let l = layout(
                def,
                cell_seed(1, Axial::new(seed as i32, -(seed as i32))),
                &g,
            );
            for (ri, r) in def.props.scatter.iter().enumerate() {
                let want = r.density * CAP_AREA / 100.0;
                let n = l.placed(ri);
                assert_eq!(
                    n as u32,
                    l.planned[ri],
                    "{id} {}: shortfall",
                    r.piece.as_str()
                );
                assert!((n as f32 - want).abs() < 1.0, "{id}: {n} vs {want}");
                sums[ri] += n;
                let fp = group_footprint(r.piece.as_str());
                let pts: Vec<Vec2> = l
                    .instances
                    .iter()
                    .filter(|i| i.rule as usize == ri)
                    .map(|i| i.pos.xz())
                    .collect();
                let share = if r.drift { DRIFT_SHARE } else { 1.0 };
                let even = BLUE_NOISE * (share * l.area[ri] / n.max(1) as f32).sqrt();
                let pack = if r.drift { DRIFT_PACK } else { 2.0 };
                let want = even.max(pack * fp.ground).max(CROWN_SPACING * fp.crown);
                assert!(
                    (l.spacing[ri] - want).abs() < 0.05,
                    "{id}: spacing {} vs {want}",
                    l.spacing[ri]
                );
                let spacing = l.spacing[ri];
                for (a, p) in pts.iter().enumerate() {
                    for q in &pts[a + 1..] {
                        assert!(p.distance(*q) >= spacing - 1e-4, "{id}: too close");
                    }
                }
            }
        }
        for (ri, r) in def.props.scatter.iter().enumerate() {
            // A tile plans floor(want + u), so its count is Bernoulli on want's fraction: the
            // tolerance is three standard errors of the 64-seed mean (plus 3% of the density),
            // relative to how sparse the rule is, so a rare rule that places half or none fails.
            let want = r.density * CAP_AREA / 100.0;
            let f = want.fract();
            let se = (f * (1.0 - f) / seeds as f32).sqrt() * 100.0 / CAP_AREA;
            let tol = 3.0 * se + 0.03 * r.density;
            let mean = sums[ri] as f32 / seeds as f32 / CAP_AREA * 100.0;
            assert!(
                (mean - r.density).abs() <= tol,
                "{id} {}: {mean} per 100 m² vs {} (tolerance {tol})",
                r.piece.as_str(),
                r.density
            );
            if want * seeds as f32 >= 3.0 {
                assert!(sums[ri] > 0, "{id} {}: never placed", r.piece.as_str());
            }
        }
    }
}

/// The nearest-neighbour distances of a forest's trees vary: Poisson-disc, not a lattice.
#[test]
fn spacing_is_not_a_grid() {
    let c = catalog();
    let def = c.get("forest").unwrap();
    let l = layout(def, 11, &prepared(def).unwrap());
    let trees: Vec<Vec2> = l
        .instances
        .iter()
        .filter(|i| i.rule == rule_of(def, "tree_forest"))
        .map(|i| i.pos.xz())
        .collect();
    let nn: Vec<f32> = trees
        .iter()
        .enumerate()
        .map(|(a, p)| {
            trees
                .iter()
                .enumerate()
                .filter(|(b, _)| *b != a)
                .map(|(_, q)| p.distance(*q))
                .fold(f32::MAX, f32::min)
        })
        .collect();
    let mean = nn.iter().sum::<f32>() / nn.len() as f32;
    let sd = (nn.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / nn.len() as f32).sqrt();
    // A lattice spreads ~0%; dense Poisson-disc packing (20 spruces on the litter) still spreads
    // several per cent.
    assert!(
        sd > 0.05 * mean,
        "nearest-neighbour spread {sd} of mean {mean}"
    );
    let yaws: HashSet<u32> = l.instances.iter().map(|i| i.yaw.to_bits()).collect();
    assert_eq!(
        yaws.len(),
        l.instances.len(),
        "every instance has its own yaw"
    );
    assert!(l.instances.iter().all(|i| (0.85..=1.15).contains(&i.scale)));
}

/// Undo and redo (and bulldoze then undo) bring a tile back with the seed it had, so it
/// scatters identically.
#[test]
fn undo_and_redo_restore_an_identical_scatter() {
    let c = catalog();
    let def = c.get("forest").unwrap();
    let g = prepared(def).unwrap();
    let mut w = World::new(42);
    let cell = Axial::new(1, 0);
    let k = w
        .auto_orient(&c, cell, def)
        .expect("a forest fits next to the meadow");
    w.place(&c, cell, "forest", k).unwrap();
    let scatter = |w: &World| {
        let t = &w.tiles()[&cell];
        assert_eq!(t.seed, cell_seed(w.seed(), cell));
        layout(c.get(&t.def).unwrap(), cell_seed(w.seed(), cell), &g)
    };
    let first = scatter(&w);
    assert!(w.undo(&c));
    assert!(!w.tiles().contains_key(&cell));
    assert!(w.redo(&c));
    assert_eq!(scatter(&w), first, "redo");
    w.bulldoze(&c, cell).unwrap();
    assert!(w.undo(&c));
    assert_eq!(scatter(&w), first, "undo of a bulldoze");
}

/// PRD §6 Variety: a rule's instances draw every piece of its group over a few tiles, the same
/// seed draws the same mix, and every variant is a known piece of its family with a footprint.
#[test]
fn instances_draw_every_variant_of_their_rule() {
    let c = catalog();
    for (id, rule) in [("forest", 0), ("forest", 1), ("meadow", 0), ("meadow", 2)] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let piece = def.props.scatter[rule].piece.as_str();
        let mut seen = std::collections::BTreeSet::new();
        // 24 seeds: the forest's outcrop rule places one on only a few tiles in 12.
        for seed in 0..24u64 {
            let l = layout(def, seed, &g);
            assert_eq!(l, layout(def, seed, &g), "{id}: same seed");
            for i in l.instances.iter().filter(|i| i.rule as usize == rule) {
                seen.insert(i.piece(piece).to_owned());
            }
        }
        let want: std::collections::BTreeSet<String> = group(piece).map(str::to_owned).collect();
        assert_eq!(seen, want, "{id} {piece}");
    }
    for def in &hex_core::piece::PieceTable::builtin().pieces {
        let id = def.id.as_str();
        for v in &def.variants {
            assert_eq!(family(v), id);
            assert!(
                PieceTable::builtin()
                    .get(v)
                    .is_some_and(|p| p.footprint.is_some()),
                "{v} has no footprint"
            );
            assert_eq!(floor_slots(v), floor_slots(id));
        }
    }
}

/// Weighted groups: every `data/pieces.ron` `variant_weights` and `verge_variant_weights` list names a group and weighs each of its
/// pieces above 0 (a new variant without a weight would never be drawn), and `bush_verge` is
/// the most common verge bush, about half the picks (owner, 2026-10-20).
#[test]
fn weighted_groups_draw_by_their_weights() {
    let lists = hex_core::piece::PieceTable::builtin()
        .pieces
        .iter()
        .flat_map(|p| {
            [&p.variant_weights, &p.verge_variant_weights]
                .into_iter()
                .flatten()
                .map(move |w| (p.id.as_str(), w))
        });
    for (id, weights) in lists {
        assert_eq!(weights.len(), group(id).count(), "{id}: a weight per piece");
        assert!(weights.iter().all(|&w| w > 0), "{id}: {weights:?}");
    }
    let c = catalog();
    let meadow = c.get("meadow").unwrap();
    let road = c.get("road_straight").unwrap();
    let mut counts = [0u32; 3];
    for seed in 0..64u64 {
        for n in 0..64 {
            counts[usize::from(pick_variant(road, "bush_verge", seed, 3, n))] += 1;
        }
    }
    let total = counts.iter().sum::<u32>() as f32;
    let share = counts.map(|c| c as f32 / total);
    assert!((0.46..0.54).contains(&share[0]), "{share:?}");
    assert!(share[1] > 0.2 && share[2] > 0.2, "{share:?}");
    assert!(share[0] > share[1] + 0.15 && share[0] > share[2] + 0.15);
    let poppy_share = |def: &TileDef| {
        let poppies = (0..64u64)
            .flat_map(|seed| {
                (0..64).map(move |n| pick_variant(def, "wildflower_single", seed, 0, n))
            })
            .filter(|&v| variant_piece("wildflower_single", v) == "wildflower_single_e")
            .count();
        poppies as f32 / 4096.0
    };
    let share = poppy_share(meadow);
    assert!((0.08..0.12).contains(&share), "meadow poppy share {share}");
    let share = poppy_share(road);
    assert!((0.22..0.28).contains(&share), "verge poppy share {share}");
}

/// More poppies on the road and rail verges (owner A9-Q4, 2026-10-20): every tile with a road
/// or rail lane and single wildflowers draws them by its `verge_variant_weights`, 1 poppy in 4;
/// every other tile keeps the meadow's 1 in 10, and the meadow draws exactly the picks it drew
/// before per-tile weights (a digest of 4096 picks under the old weights `[9, 9, 9, 9, 4]`).
#[test]
fn verges_draw_more_poppies_and_the_meadow_is_unchanged() {
    let c = catalog();
    let verges: Vec<&str> = c
        .tiles()
        .iter()
        .filter(|d| {
            d.props
                .scatter
                .iter()
                .any(|r| r.piece.as_str() == "wildflower_single")
        })
        .filter(|d| verge_tile(d))
        .map(|d| d.id.as_str())
        .collect();
    for id in [
        "road_straight",
        "road_bend_60",
        "road_bend_120",
        "road_junction",
        "road_ramp",
        "road_tunnel_portal",
        "rail_tunnel_portal",
        "rail_viaduct_l1",
        "rail_viaduct_l2",
        "rail_viaduct_l3",
        "rail_straight",
        "rail_bend_60",
        "rail_bend_120",
        "rail_junction",
        "rail_road_crossing",
        "rail_station",
    ] {
        assert!(verges.contains(&id), "{id} is a verge tile: {verges:?}");
    }
    assert_eq!(verges.len(), 16, "{verges:?}");
    for d in c.tiles() {
        let want = if verge_tile(d) {
            [3, 3, 3, 3, 4]
        } else {
            [9, 9, 9, 9, 4]
        };
        assert_eq!(
            variant_weights(d, "wildflower_single"),
            Some(&want[..]),
            "{}",
            d.id
        );
        assert_eq!(
            variant_weights(d, "bush_verge"),
            Some(&[2, 1, 1][..]),
            "{}",
            d.id
        );
    }
    let meadow = c.get("meadow").unwrap();
    let mut digest = 0u64;
    for seed in 0..64u64 {
        for n in 0..64 {
            let v = pick_variant(meadow, "wildflower_single", seed, 0, n);
            digest = digest.wrapping_mul(31).wrapping_add(u64::from(v));
        }
    }
    assert_eq!(
        digest, 8_349_994_037_374_194_218,
        "the meadow's picks moved"
    );
}

/// A `vary` rule (the meadow's single wildflowers) stands where its seeded noise is high more
/// often than its ground does on average, so the field's density varies gently, never in a grid.
#[test]
fn single_flowers_thin_by_their_seeded_noise() {
    let c = catalog();
    let def = c.get("meadow").unwrap();
    let (ri, r) = def
        .props
        .scatter
        .iter()
        .enumerate()
        .find(|(_, r)| r.piece.as_str() == "wildflower_single")
        .unwrap();
    assert!(r.vary > 0.0 && !r.drift);
    let g = prepared(def).unwrap();
    let (mut at, mut ground) = (0.0, 0.0);
    for seed in 0..16u64 {
        let l = layout(def, seed, &g);
        let pts: Vec<Vec2> = l
            .instances
            .iter()
            .filter(|i| i.rule as usize == ri)
            .map(|i| i.pos.xz())
            .collect();
        assert_eq!(pts.len() as u32, l.planned[ri]);
        at += pts.iter().map(|p| vary_noise(seed, *p)).sum::<f32>() / pts.len() as f32;
        let cells = &g.masks[ri].cells;
        ground += cells
            .iter()
            .map(|&k| {
                let k = k as usize;
                vary_noise(
                    seed,
                    Vec2::new(cell_centre(k % CELLS), cell_centre(k / CELLS)),
                )
            })
            .sum::<f32>()
            / cells.len() as f32;
    }
    let (at, ground) = (at / 16.0, ground / 16.0);
    assert!(
        at > ground + 0.03,
        "noise at flowers {at} vs ground {ground}"
    );
}

/// The footprint table matches the kit: ground reach is the collider's, crown reach the widest
/// LOD's (within 0.1 m, never under).
/// The wheat field's ground (scatter, grass and the walker all stand on it) is its ploughed
/// earth inside the field and the meadow cap on the grass margin; the wheat cards are never
/// ground (`data/pieces.ron` `not_ground`), and the meadow cap never pokes through the earth.
#[test]
fn the_wheat_field_ground_is_its_earth_not_its_wheat() {
    let def = catalog().get("wheat_field").unwrap().clone();
    let g = prepared(&def).expect("wheat_field ground in the kit");
    let field = |p: Vec2| (0..3).map(|e| p.dot(hex_dir(e)).abs()).fold(0.0, f32::max);
    let (mut earth, mut cap) = (0, 0);
    for j in -60..=60 {
        for i in -60..=60 {
            let p = Vec2::new(i as f32 * 0.25, j as f32 * 0.25);
            let reach = field(p);
            let Some((_, slot)) = g.surface.at(p.x, p.y) else {
                continue;
            };
            assert_ne!(slot, "wheat_field_wheat", "wheat is ground at {p}");
            if reach < 10.3 {
                assert_eq!(slot, "wheat_field_soil", "at {p}");
                earth += 1;
            } else if reach > 10.9 && reach < 13.5 {
                assert_eq!(slot, "meadow_base_cap", "at {p}");
                cap += 1;
            }
        }
    }
    assert!(earth > 4000 && cap > 1500, "{earth} {cap}");
    // LOD1 and LOD2 draw from 150 m and 400 m, base and centre switching together (one
    // origin): the meadow cap never pokes through the field's earth there either.
    for lod in ["lod1", "lod2"] {
        let mut all = vec![];
        for (id, k) in ground_pieces(&def, &catalog()) {
            for (t, s) in glb_mesh_tris(id, lod).unwrap() {
                all.push((t.map(|p| turn(k, p)), s));
            }
        }
        let s = Surface::new(all);
        for j in -40..=40 {
            for i in -40..=40 {
                let p = Vec2::new(i as f32 * 0.25, j as f32 * 0.25);
                if field(p) < 10.3 {
                    let (_, slot) = s.at(p.x, p.y).unwrap();
                    assert_eq!(slot, "wheat_field_soil", "{lod} at {p}");
                }
            }
        }
    }
    // The wildflowers and rocks stand on the margin only.
    let l = layout(&def, 1, &g);
    assert!(!l.instances.is_empty());
    for inst in &l.instances {
        assert!(field(inst.pos.xz()) > 10.8, "{:?}", inst.pos);
    }
}

/// Variants whose collider disc reaches past the authored piece's (the twin-gabled bakehouse's
/// corner is 4.91 m out, the authored bakery's 4.68 m) while their ground box still nests in
/// the authored one's. A slot's masks take the widest of its options
/// ([`hero_footprint`]), so scatter clears the wider reach.
const WIDER_THAN_AUTHORED: &[&str] = &["bakery_shop_b"];

/// A hero variant stands inside the authored piece's ground box and reach, so the clearances
/// sized for the authored piece (hex-sim's parking boxes, the scatter masks) hold for it. A
/// round piece (a tree, no ground box) keeps inside its ground and crown reach.
#[test]
fn hero_variants_fit_inside_the_authored_piece() {
    for def in &hex_core::piece::PieceTable::builtin().pieces {
        let piece = def.id.as_str();
        for v in &def.hero_variants {
            let v = v.as_str();
            // A building's ground box nests in the authored one's; a disc piece (the oak, the
            // hay) has no box, only its reach.
            assert_eq!(ground_box(piece).is_some(), ground_box(v).is_some(), "{v}");
            if let (Some(b), Some(vb)) = (ground_box(piece), ground_box(v)) {
                assert!(
                    vb.min[0] >= b.min[0]
                        && vb.min[1] >= b.min[1]
                        && vb.max[0] <= b.max[0]
                        && vb.max[1] <= b.max[1],
                    "{v} {vb:?} outside {piece} {b:?}"
                );
            }
            if WIDER_THAN_AUTHORED.contains(&v) {
                continue;
            }
            assert!(footprint(v).ground <= footprint(piece).ground, "{v}");
            assert!(footprint(v).crown <= footprint(piece).crown, "{v}");
        }
    }
}

/// Every table lookup (`footprint`, `ground_box`, `fence`) falls back silently for a piece
/// without a row, and every table test iterates the table, so a missing row is invisible.
/// This iterates the catalog instead: each hero and scatter piece, with its scatter group and
/// hero variants, has a `footprint` or a `fence` in `data/pieces.ron` or is an [`is_ground_prop`].
#[test]
fn every_prop_piece_has_a_footprint_row() {
    let missing: Vec<String> = crate::walk::tests::catalog_props()
        .into_iter()
        .filter(|p| {
            !PieceTable::builtin()
                .get(p)
                .is_some_and(|d| d.footprint.is_some() || d.fence.is_some())
                && !is_ground_prop(p)
        })
        .collect();
    assert!(
        missing.is_empty(),
        "pieces the catalog places with no footprint or fence and not a ground prop: {missing:?}"
    );
}

/// Owner D3 (2026-10-20): on every coast tile the driftwood and marram clumps stand on the dry
/// sand inside their [`beach_band`], their whole footprint [`BEACH_BORDER`] inside the border
/// and clear of the river mouth's channel; every planned instance is placed, the driftwood lies
/// along the coast and the clumps thin seaward. Prints the mean count per tile.
#[test]
fn beach_dressing_stays_on_its_dry_sand_band() {
    let c = catalog();
    for id in [
        "beach",
        "beach_inner",
        "beach_outer",
        "river_mouth",
        "river_mouth_l",
    ] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let coast = Coastline::of(def, 0).unwrap();
        let lanes = lanes(def);
        let (mut near, mut far) = (0usize, 0usize);
        let mut per_rule = vec![0usize; def.props.scatter.len()];
        let mut area = vec![];
        let mut short = vec![];
        let seeds = 24u64;
        for seed in 0..seeds {
            let l = layout(def, cell_seed(7, Axial::new(seed as i32, 0)), &g);
            area.clone_from(&l.area);
            for (ri, r) in def.props.scatter.iter().enumerate() {
                let piece = r.piece.as_str();
                if bank_band(piece).is_some() {
                    // The mouths' bank props: `bank_props_stand_in_their_band`.
                    continue;
                }
                assert!(beach_band(piece).is_some(), "{id}: {piece}");
                if l.placed(ri) as u32 != l.planned[ri] {
                    short.push(format!(
                        "{id} {piece}: {} of {}",
                        l.placed(ri),
                        l.planned[ri]
                    ));
                }
                per_rule[ri] += l.placed(ri);
            }
            for inst in &l.instances {
                let r = &def.props.scatter[inst.rule as usize];
                let piece = r.piece.as_str();
                let Some(band) = beach_band(piece) else {
                    continue;
                };
                let fp = group_footprint(piece);
                let p = inst.pos.xz();
                let s = coast.seaward(p);
                assert!(
                    s >= band.near - 0.2 && s <= band.far + 0.2,
                    "{id}: {piece} at s {s}"
                );
                assert!(
                    s + fp.ground <= DAMP_TOP_S + 0.2,
                    "{id}: {piece} footprint on the damp band at s {s}"
                );
                assert!(
                    hex_reach(p) <= APOTHEM - fp.ground - BEACH_BORDER + 0.2,
                    "{id}: {piece} border"
                );
                for (line, half) in &lanes {
                    let need = lane_clearance(r, *half, fp);
                    assert!(
                        polyline_distance(p, line) >= need - 0.2,
                        "{id}: {piece} lane"
                    );
                }
                if band.along {
                    let t = match coast {
                        Coastline::Straight { sea } => sea.perp(),
                        Coastline::Arc { centre, .. } => (p - centre).perp(),
                    }
                    .normalize();
                    let axis = Vec2::new(inst.yaw.cos(), -inst.yaw.sin());
                    assert!(
                        axis.dot(t).abs() >= DRIFT_YAW_JITTER.cos() - 1e-3,
                        "{id}: {piece} lies across the coast"
                    );
                } else if s < 0.5 * (band.near + band.far) {
                    near += 1;
                } else {
                    far += 1;
                }
            }
        }
        assert!(
            near > far,
            "{id}: clumps thin seaward ({near} near, {far} far)"
        );
        let mean: Vec<f32> = per_rule.iter().map(|&n| n as f32 / seeds as f32).collect();
        let pieces: Vec<&str> = def.props.scatter.iter().map(|r| r.piece.as_str()).collect();
        println!(
            "{id}: mean per tile {mean:?} for {pieces:?} on {area:?} m² ({near} near, {far} far)"
        );
        assert!(short.is_empty(), "shortfall: {short:?}");
    }
}

/// Owner 2026-10-21: "the beaches have too much going on, every piece does not need to have all
/// scatter items, 0 to 6 is plenty". Every beach-family tile draws 0 to [`BEACH_MAX`] dressing
/// pieces, the same for the same seed, each standing exactly where the full dart set put it,
/// and the count varies over seeds. Prints how often each count occurs.
#[test]
fn beach_tiles_draw_zero_to_six_dressing_pieces() {
    let c = catalog();
    let mut family: Vec<&str> = c
        .tiles()
        .iter()
        .filter(|d| beach_family(d))
        .map(|d| d.id.as_str())
        .collect();
    family.sort_unstable();
    assert_eq!(
        family,
        [
            "beach",
            "beach_inner",
            "beach_outer",
            "river_mouth",
            "river_mouth_l"
        ]
    );
    for id in family {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let mut seen = [0u32; BEACH_MAX + 1];
        for s in 0..64 {
            let seed = cell_seed(11, Axial::new(s, -s / 2));
            let l = layout(def, seed, &g);
            let n = l.instances.len();
            assert!(n <= BEACH_MAX, "{id} seed {seed}: {n} pieces");
            seen[n] += 1;
            assert_eq!(l, layout(def, seed, &g), "{id}: same seed, same beach");
            let all = every_dart(def, seed, &g);
            assert_eq!(n, beach_count(seed).min(all.instances.len()), "{id}");
            for inst in &l.instances {
                assert!(
                    all.instances.iter().any(|a| Instance {
                        order: 0,
                        ..inst.clone()
                    } == *a),
                    "{id}: a kept piece moved"
                );
            }
            for ri in 0..def.props.scatter.len() {
                assert_eq!(l.planned[ri], l.placed(ri) as u32, "{id}: planned is kept");
            }
        }
        println!("{id}: tiles per count 0..=6 over 64 seeds {seen:?}");
        assert!(
            seen.iter().filter(|&&k| k > 0).count() >= 3,
            "{id}: the count varies {seen:?}"
        );
    }
}

/// Owner 2026-10-26: "riverbank scatter is too concentrated and repetitive, change scatter to
/// 0-4 things", and "scatter type items like rocks and reeds should not be built into the
/// tile". Every river channel tile (with no centre piece), both river bridges (whose centre is
/// only the deck) and the spring (granite and lilies) is a [`river_family`] tile and draws 0 to [`RIVER_MAX`] bank props, the same for the same seed, each standing where the full dart
/// set put it. Over seeds every count 0 to 4 occurs, both kinds and at least four of the five
/// pieces turn up, and no two tiles with props share a layout. Prints the count histogram.
#[test]
fn river_tiles_draw_zero_to_four_bank_props() {
    let c = catalog();
    let mut family: Vec<&str> = c
        .tiles()
        .iter()
        .filter(|d| river_family(d))
        .map(|d| d.id.as_str())
        .collect();
    family.sort_unstable();
    assert_eq!(
        family,
        [
            "river_bend",
            "river_bend_60",
            "river_fork",
            "river_rail_bridge",
            "river_road_bridge",
            "river_spring",
            "river_straight"
        ]
    );
    for id in family {
        let def = c.get(id).unwrap();
        assert!(
            def.center.is_none() || id.ends_with("_bridge") || id == "river_spring",
            "{id}: the banks are scatter, not a centre"
        );
        let g = prepared(def).unwrap();
        let mut seen = [0u32; RIVER_MAX + 1];
        let mut pieces = HashSet::new();
        let mut layouts = HashSet::new();
        let mut with_props = 0;
        for s in 0..64 {
            let seed = cell_seed(13, Axial::new(s, -s / 2));
            let full = layout(def, seed, &g);
            assert_eq!(full, layout(def, seed, &g), "{id}: same seed, same bank");
            let l = bank_only(def, &full);
            let n = l.instances.len();
            assert!(n <= RIVER_MAX, "{id} seed {seed}: {n} props");
            seen[n] += 1;
            let all = bank_only(def, &every_dart(def, seed, &g));
            assert!(
                all.instances.len() >= RIVER_MAX,
                "{id} seed {seed}: room for 4"
            );
            assert_eq!(n, river_count(seed).min(all.instances.len()), "{id}");
            for inst in &l.instances {
                assert!(
                    all.instances.iter().any(|a| Instance {
                        order: 0,
                        ..inst.clone()
                    } == *a),
                    "{id}: a kept prop moved"
                );
                let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
                pieces.insert(piece.to_owned());
            }
            if n > 0 {
                with_props += 1;
                let key: Vec<[i32; 3]> = l
                    .instances
                    .iter()
                    .map(|i| (i.pos * 100.0).round().as_ivec3().to_array())
                    .collect();
                layouts.insert(key);
            }
        }
        println!("{id}: tiles per count 0..=4 over 64 seeds {seen:?}, pieces {pieces:?}");
        assert!(
            seen.iter().all(|&k| k > 0),
            "{id}: every count occurs {seen:?}"
        );
        assert_eq!(layouts.len(), with_props, "{id}: no two banks alike");
        assert!(pieces.len() >= 4, "{id}: the variants vary {pieces:?}");
        assert!(
            pieces.iter().any(|p| p.starts_with("river_stones"))
                && pieces.iter().any(|p| p.starts_with("river_reeds")),
            "{id}: stones and reeds {pieces:?}"
        );
    }
}

/// Owner A22 Q2 (2026-10-26): reeds and stone clusters about half and half (the densities
/// were 0.75 stones to 0.6 reeds, a 43% reed share over 1024 seeds). Over 64 seeds every
/// [`river_family`] tile's kept bank props are 35% to 65% reed clumps, and over 1024 seeds 45%
/// to 55%. Prints each tile's share and its histogram of reeds per tile.
#[test]
fn river_tiles_draw_reeds_and_stones_about_half_and_half() {
    let c = catalog();
    let mut ids: Vec<&str> = c
        .tiles()
        .iter()
        .filter(|d| river_family(d))
        .map(|d| d.id.as_str())
        .collect();
    ids.sort_unstable();
    for id in ids {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        for (seeds, band) in [(64, 0.35..=0.65), (1024, 0.45..=0.55)] {
            let (mut reeds, mut stones) = (0u32, 0u32);
            let mut reeds_per_tile = [0u32; RIVER_MAX + 1];
            for s in 0..seeds {
                let seed = cell_seed(13, Axial::new(s, -s / 2));
                let l = layout(def, seed, &g);
                let mut r = 0;
                for inst in &l.instances {
                    let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                    match family(inst.piece(rule)) {
                        "river_reeds" => r += 1,
                        "river_stones" => stones += 1,
                        "lily_cluster" | "reed_bed" => {}
                        other => panic!("{id}: {other} is not a bank prop"),
                    }
                }
                reeds += r;
                reeds_per_tile[r as usize] += 1;
            }
            let share = f64::from(reeds) / f64::from(reeds + stones);
            println!(
                "{id} over {seeds} seeds: {reeds} reeds, {stones} stones, reed share {:.0}%, tiles per reed count 0..=4 {reeds_per_tile:?}",
                share * 100.0
            );
            assert!(
                band.contains(&share),
                "{id} over {seeds} seeds: reed share {share:.2} ({reeds} reeds, {stones} stones)"
            );
        }
    }
}

/// The mixed scatter world (`docs/checkpoints/scatter/scatter-mixed.json`, seed 1) draws every
/// water prop, as `check.mjs` asserts: the bend at -3,2 keeps a lily_cluster, the spring at -4,3
/// a lily_cluster_c, and the bend at 0,3 (below the spring at 0,2) a lily_cluster_b and a reed bed.
#[test]
fn the_mixed_world_draws_every_water_prop() {
    let c = catalog();
    for (id, q, r, want) in [
        ("river_bend", -3, 2, &["lily_cluster", "lily_cluster"][..]),
        ("river_spring", -4, 3, &["lily_cluster_c"][..]),
        ("river_bend", 0, 3, &["lily_cluster_b", "reed_bed"][..]),
    ] {
        let def = c.get(id).unwrap();
        let l = layout(def, cell_seed(1, Axial::new(q, r)), &prepared(def).unwrap());
        let water: Vec<&str> = l
            .instances
            .iter()
            .map(|i| i.piece(def.props.scatter[i.rule as usize].piece.as_str()))
            .filter(|p| water_prop(p))
            .collect();
        assert_eq!(water, want, "{id} at {q},{r}");
    }
}

/// `l` without its water props ([`water_prop`]).
fn bank_only(def: &TileDef, l: &Layout) -> Layout {
    let mut out = l.clone();
    out.instances
        .retain(|i| !water_prop(def.props.scatter[i.rule as usize].piece.as_str()));
    out
}

/// An instance with its rise rank cleared, for comparing places across layouts.
fn placed_as(i: &Instance) -> Instance {
    Instance {
        order: 0,
        ..i.clone()
    }
}

/// Owner A26 (2026-10-27): the lily clusters and reed beds are listed by the spring and the 120°
/// bend alone (not the straights, forks, mouths, bridges, beaches or ocean), after their bank
/// props, and adding them leaves every bank prop where it stood and the bank cap as it was: over
/// 256 seeds each tile's bank props, kept and darted, equal the same tile's without the water
/// rules, instance for instance.
#[test]
fn water_props_leave_the_bank_props_as_they_were() {
    let c = catalog();
    let mut listed: Vec<&str> = c
        .tiles()
        .iter()
        .filter(|d| d.props.scatter.iter().any(|r| water_prop(r.piece.as_str())))
        .map(|d| d.id.as_str())
        .collect();
    listed.sort_unstable();
    assert_eq!(listed, ["river_bend", "river_spring"]);
    for id in listed {
        let def = c.get(id).unwrap();
        let first = def
            .props
            .scatter
            .iter()
            .position(|r| water_prop(r.piece.as_str()))
            .unwrap();
        assert!(
            def.props.scatter[first..]
                .iter()
                .all(|r| water_prop(r.piece.as_str())),
            "{id}: water props after the bank props"
        );
        assert!(river_family(def), "{id}: still capped as a riverbank");
        let mut dry = def.clone();
        dry.props.scatter.truncate(first);
        let (g, gd) = (prepared(def).unwrap(), prepared(&dry).unwrap());
        for s in 0..256 {
            let seed = cell_seed(29, Axial::new(s, 1 - s));
            for (with, without) in [
                (
                    bank_only(def, &layout(def, seed, &g)),
                    layout(&dry, seed, &gd),
                ),
                (
                    bank_only(def, &every_dart(def, seed, &g)),
                    every_dart(&dry, seed, &gd),
                ),
            ] {
                let a: Vec<Instance> = with.instances.iter().map(placed_as).collect();
                let b: Vec<Instance> = without.instances.iter().map(placed_as).collect();
                assert_eq!(a, b, "{id} seed {seed}: the bank props moved");
                assert_eq!(with.planned[..first], without.planned[..], "{id}");
            }
        }
    }
}

/// Each tile keeps a seeded 0 to [`WATER_MAX`] water props ([`water_count`], a stream of its
/// own), each standing where the full dart set put it; over seeds every count occurs and the
/// three lily clusters and the reed bed all turn up. Prints placed against planned darts per
/// rule and the kept histogram.
#[test]
fn water_props_keep_their_own_zero_to_two() {
    let c = catalog();
    for id in ["river_spring", "river_bend"] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let rules: Vec<usize> = (0..def.props.scatter.len())
            .filter(|&r| water_prop(def.props.scatter[r].piece.as_str()))
            .collect();
        let mut seen = [0u32; WATER_MAX + 1];
        let mut pieces = HashSet::new();
        let (mut planned, mut darted) = (vec![0u32; rules.len()], vec![0usize; rules.len()]);
        let seeds = 256;
        for s in 0..seeds {
            let seed = cell_seed(31, Axial::new(s, -s));
            let l = layout(def, seed, &g);
            let all = every_dart(def, seed, &g);
            let water: Vec<&Instance> = l
                .instances
                .iter()
                .filter(|i| rules.contains(&(i.rule as usize)))
                .collect();
            let room: usize = rules.iter().map(|&r| all.placed(r)).sum();
            assert_eq!(water.len(), water_count(seed).min(room), "{id} seed {seed}");
            seen[water.len()] += 1;
            for inst in water {
                assert!(
                    all.instances.iter().any(|a| placed_as(inst) == *a),
                    "{id}: a kept water prop moved"
                );
                let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                pieces.insert(inst.piece(rule).to_owned());
            }
            for (k, &r) in rules.iter().enumerate() {
                planned[k] += all.planned[r];
                darted[k] += all.placed(r);
            }
        }
        let names: Vec<&str> = rules
            .iter()
            .map(|&r| def.props.scatter[r].piece.as_str())
            .collect();
        println!(
            "{id} over {seeds} seeds: rules {names:?} planned {planned:?} placed {darted:?}, \
             tiles per kept count 0..=2 {seen:?}, pieces {pieces:?}"
        );
        assert!(seen.iter().all(|&k| k > 0), "{id}: every count {seen:?}");
        for p in [
            "lily_cluster",
            "lily_cluster_b",
            "lily_cluster_c",
            "reed_bed",
        ] {
            assert!(pieces.contains(p), "{id}: {p} turns up {pieces:?}");
        }
    }
}

/// Every water prop stands where approval A26 puts it, at its largest footprint and scale: a lily
/// cluster's pads at [`LILY_Y`], its every rim, half-rim and centre point over the bed under the
/// water at rest (4.0 m from a river centreline less its radius) with the bed topmost, so no
/// built-in spring pad lies under it; a reed bed at [`WATER_REST`] over a bed 0.4 to 0.8 m deep,
/// its -z up the slope; both [`WATER_BORDER`] inside the border and, on the spring, within
/// [`SPRING_REACH`] of the node.
#[test]
fn water_props_stand_on_the_water() {
    let c = catalog();
    for id in ["river_spring", "river_bend"] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let springs = spring_nodes(def);
        assert_eq!(springs.len(), usize::from(id == "river_spring"), "{id}");
        let pads: Vec<Vec2> = g
            .surface
            .tris
            .iter()
            .filter(|(_, _, s)| g.surface.slots[*s as usize] == "river_lily")
            .flat_map(|(t, _, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
            .collect();
        assert_eq!(
            pads.is_empty(),
            id != "river_spring",
            "{id}: the spring's own pads"
        );
        let (mut lilies, mut reeds) = (0, 0);
        for s in 0..128 {
            let all = every_dart(def, cell_seed(37, Axial::new(s, 3)), &g);
            for inst in &all.instances {
                let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                if !water_prop(rule) {
                    continue;
                }
                let piece = inst.piece(rule);
                let r = footprint(piece).ground * inst.scale;
                let p = inst.pos.xz();
                assert!(
                    hex_reach(p) + r <= APOTHEM - WATER_BORDER + 0.02,
                    "{id}: {piece} at {p} within 1.5 m of the edge"
                );
                let (h, slot) = g.surface.at(p.x, p.y).unwrap();
                if lily(rule) {
                    lilies += 1;
                    assert_eq!(inst.pos.y, LILY_Y, "{id}: pads on the water");
                    let mut probes = vec![p];
                    for d in ring16() {
                        probes.push(p + r * d);
                        probes.push(p + 0.5 * r * d);
                    }
                    for q in probes {
                        let (y, s) = g.surface.at(q.x, q.y).unwrap();
                        assert!(
                            s == WATER_FLOOR && y < WATER_REST,
                            "{id}: {piece} at {p} reaches {s} at {y}"
                        );
                    }
                    for &v in &pads {
                        assert!(
                            v.distance(p) > 0.9 * r,
                            "{id}: {piece} at {p} over a built-in pad at {v}"
                        );
                    }
                    for n in &springs {
                        assert!(n.distance(p) + r <= SPRING_REACH + 1e-3, "{id}: spring");
                    }
                } else {
                    reeds += 1;
                    assert_eq!(inst.pos.y, WATER_REST, "{id}: reed bed at the water");
                    assert!(
                        (REED_BED_BAND.0..=REED_BED_BAND.1).contains(&h),
                        "{id}: reed bed over {slot} at {h}"
                    );
                    let up = Vec2::new(
                        g.surface.at(p.x + 0.5, p.y).unwrap().0
                            - g.surface.at(p.x - 0.5, p.y).unwrap().0,
                        g.surface.at(p.x, p.y + 0.5).unwrap().0
                            - g.surface.at(p.x, p.y - 0.5).unwrap().0,
                    )
                    .normalize();
                    let minus_z = Vec2::new(-inst.yaw.sin(), -inst.yaw.cos());
                    assert!(
                        minus_z.dot(up) >= DRIFT_YAW_JITTER.cos() - 1e-3,
                        "{id}: reed bed faces the bank"
                    );
                    let q = inst.transform().transform_point(Vec3::new(0.0, 0.0, -1.0));
                    let along = (q.xz() - p).dot(up) / inst.scale;
                    assert!(along > DRIFT_YAW_JITTER.cos() - 1e-3, "{id}: -z is uphill");
                    for n in &springs {
                        assert!(n.distance(p) <= SPRING_REACH + 1e-3, "{id}: spring");
                    }
                }
            }
        }
        println!("{id}: {lilies} lily clusters and {reeds} reed beds darted over 128 seeds");
        assert!(lilies > 64, "{id}: {lilies} lily clusters");
        assert!(reeds > 0, "{id}: {reeds} reed beds");
    }
}

/// Every bank prop stands on the bank of its band ([`bank_band`]: the surface height under its
/// centre, which is how far from the centreline it stands), on the [`BANK_FLOORS`], at that
/// height, and a stone cluster lies along the bank's contour within [`DRIFT_YAW_JITTER`].
#[test]
fn bank_props_stand_in_their_band() {
    let c = catalog();
    for id in [
        "river_straight",
        "river_bend",
        "river_bend_60",
        "river_fork",
        "river_mouth",
        "river_mouth_l",
        "river_road_bridge",
        "river_rail_bridge",
        "river_spring",
    ] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let mut n = 0;
        for s in 0..32 {
            let all = every_dart(def, cell_seed(17, Axial::new(s, 0)), &g);
            for inst in &all.instances {
                let rule = def.props.scatter[inst.rule as usize].piece.as_str();
                let Some((low, high)) = bank_band(rule) else {
                    continue;
                };
                n += 1;
                let p = inst.pos.xz();
                let (h, slot) = g.surface.at(p.x, p.y).unwrap();
                assert!(BANK_FLOORS.contains(&slot), "{id}: {rule} on {slot}");
                assert!(
                    (inst.pos.y - h).abs() < 1e-4,
                    "{id}: {rule} at the bank's height"
                );
                assert!(
                    h >= low && h <= high,
                    "{id}: {rule} at height {h}, band {low}..{high}"
                );
                if rule == "river_stones" {
                    let t = contour(&g.surface, p).unwrap().normalize();
                    let axis = Vec2::new(inst.yaw.cos(), -inst.yaw.sin());
                    assert!(
                        axis.dot(t).abs() >= DRIFT_YAW_JITTER.cos() - 1e-3,
                        "{id}: stones across the bank"
                    );
                }
            }
        }
        assert!(n > 32, "{id}: {n} bank props over 32 seeds");
    }
}

/// Owner 2026-10-26 ("0 to 6 is plenty" stands): the beach pebble clusters count inside a coast
/// tile's [`BEACH_MAX`], drawn like any other dressing piece. Prints the mean kept per tile of
/// each piece, and the share of tiles that keep a pebble cluster.
#[test]
fn beach_pebbles_count_inside_the_beach_cap() {
    let c = catalog();
    for id in ["beach", "beach_inner", "beach_outer"] {
        let def = c.get(id).unwrap();
        let g = prepared(def).unwrap();
        let rule = def
            .props
            .scatter
            .iter()
            .position(|r| r.piece.as_str() == "beach_pebbles")
            .unwrap_or_else(|| panic!("{id}: a beach_pebbles rule"));
        // The pebbles come last among the beach dressing; only the inland granite boulders
        // (owner A25), band-limited on a coast, follow them.
        let inland = def
            .props
            .scatter
            .iter()
            .filter(|r| family(r.piece.as_str()) == "boulder_granite")
            .count();
        assert_eq!(
            rule + 1 + inland,
            def.props.scatter.len(),
            "{id}: pebbles last"
        );
        let seeds = 256;
        let mut kept = vec![0usize; def.props.scatter.len()];
        let (mut with_pebbles, mut darts) = (0, 0);
        for s in 0..seeds {
            let seed = cell_seed(13, Axial::new(s, 2 - s));
            let l = layout(def, seed, &g);
            assert!(l.instances.len() <= BEACH_MAX, "{id}");
            for (ri, k) in kept.iter_mut().enumerate() {
                *k += l.placed(ri);
            }
            with_pebbles += usize::from(l.placed(rule) > 0);
            darts += every_dart(def, seed, &g).placed(rule);
        }
        let mean: Vec<f32> = kept.iter().map(|&n| n as f32 / seeds as f32).collect();
        println!(
            "{id}: mean kept per tile {mean:?}, a pebble cluster on {with_pebbles} of {seeds} \
             tiles, {} clusters darted per tile",
            darts as f32 / seeds as f32
        );
        assert!(with_pebbles > 0, "{id}: some tile keeps a pebble cluster");
    }
}

/// Owner 2026-10-26: the ocean's rocks and pebble clusters are scatter on its floor, standing
/// at the bed's contract level, so the floor stays within its 0.10 m upward allowance (water
/// contract §4): every seabed instance's top (its piece's highest vertex in any LOD, times its
/// scale) stays within [`SEABED_ALLOWANCE`] of the bed, it never stands above the floor, and
/// its footprint keeps [`SEABED_BORDER`] inside the border. Every rule places its whole count.
#[test]
fn seabed_stones_stay_within_the_ocean_floor_allowance() {
    let c = catalog();
    let def = c.get("ocean").unwrap();
    let g = prepared(def).unwrap();
    let top = |id: &str| {
        (0..3)
            .flat_map(|l| glb_mesh_tris(id, &format!("lod{l}")).unwrap())
            .flat_map(|(t, _)| t)
            .map(|p| p[1])
            .fold(f32::MIN, f32::max)
    };
    let mut per_rule = vec![0usize; def.props.scatter.len()];
    let mut highest = 0.0f32;
    let seeds = 32;
    for s in 0..seeds {
        let l = layout(def, cell_seed(5, Axial::new(s, 1 - s)), &g);
        for inst in &l.instances {
            let piece = inst.piece(def.props.scatter[inst.rule as usize].piece.as_str());
            assert!(seabed(piece), "{piece}");
            assert_eq!(inst.pos.y, SEABED_Y, "{piece}");
            let p = inst.pos.xz();
            let (floor, slot) = g.surface.at(p.x, p.y).unwrap();
            assert_eq!(slot, SEABED_FLOOR);
            assert!(inst.pos.y <= floor + 1e-4, "{piece} above the floor");
            let rise = inst.pos.y + inst.scale * top(piece) - SEABED_Y;
            highest = highest.max(rise);
            assert!(
                rise <= SEABED_ALLOWANCE,
                "{piece}: top {rise} m over the bed"
            );
            assert!(
                hex_reach(p) + footprint(piece).ground * inst.scale
                    <= APOTHEM - SEABED_BORDER + 1e-3,
                "{piece} within {SEABED_BORDER} m of the border"
            );
            per_rule[inst.rule as usize] += 1;
        }
        for ri in 0..def.props.scatter.len() {
            assert_eq!(l.placed(ri) as u32, l.planned[ri], "ocean: shortfall");
        }
    }
    let mean: Vec<f32> = per_rule.iter().map(|&n| n as f32 / seeds as f32).collect();
    println!("ocean: mean per tile {mean:?}, highest top {highest} m over the bed");
    assert!(per_rule.iter().all(|&n| n > 0), "{per_rule:?}");
}

/// The pieces with a `data/pieces.ron` footprint, in table order.
fn footprint_rows() -> impl Iterator<Item = (&'static str, Footprint)> {
    PieceTable::builtin()
        .pieces
        .iter()
        .filter_map(|p| Some((p.id.as_str(), p.footprint?)))
}

/// The pieces with a `data/pieces.ron` fence, in table order.
fn fence_rows() -> impl Iterator<Item = (&'static str, Fence)> {
    PieceTable::builtin()
        .pieces
        .iter()
        .filter_map(|p| Some((p.id.as_str(), fence(&p.id)?)))
}

/// The pieces with a `data/pieces.ron` ground box, in table order.
fn ground_box_rows() -> impl Iterator<Item = (&'static str, GroundBox)> {
    PieceTable::builtin()
        .pieces
        .iter()
        .filter_map(|p| Some((p.id.as_str(), p.ground_box?)))
}

#[test]
fn footprints_match_the_kit() {
    let reach = |tris: &[(Tri, String)]| {
        tris.iter()
            .flat_map(|(t, _)| t.iter())
            .map(|p| Vec2::new(p[0], p[2]).length())
            .fold(0.0, f32::max)
    };
    for (id, f) in footprint_rows() {
        let ground = reach(&glb_mesh_tris(id, "collider").unwrap());
        assert!(
            f.ground >= ground && f.ground <= ground + 0.1,
            "{id} ground {ground}"
        );
        if f.crown > 0.0 {
            let crown = (0..3)
                .map(|l| reach(&glb_mesh_tris(id, &format!("lod{l}")).unwrap()))
                .fold(0.0, f32::max);
            assert!(
                f.crown >= crown && f.crown <= crown + 0.1,
                "{id} crown {crown}"
            );
        }
    }
}

/// Each `data/pieces.ron` `fence` is its GLB: the collider has one disjoint box per run, each centred
/// on its run, and LOD0 has one post per entry, a square of that half width (its foot, buried
/// below y -0.3, leaning at most 0.022 rad) centred on the entry. A fence has no `footprint`
/// entry.
#[test]
fn fences_match_the_kit() {
    for (id, f) in fence_rows() {
        assert!(
            footprint_rows().all(|(p, _)| p != id),
            "{id} has a footprint"
        );
        let runs = f.line.len() - 1;
        let walk = crate::walk::PieceShape::from_meshes(
            id,
            &glb_mesh_tris(id, "collider")
                .unwrap()
                .iter()
                .flat_map(|(t, _)| t.iter().map(|p| Vec3::from(*p)))
                .collect::<Vec<_>>(),
            &[],
        );
        assert_eq!(walk.parts.len(), runs, "{id}: one collider part per run");
        for w in f.line.windows(2) {
            let (a, b) = (Vec2::from(w[0]), Vec2::from(w[1]));
            let part = walk.parts.iter().find(|p| {
                let c = p.hull.iter().copied().sum::<Vec2>() / p.hull.len() as f32;
                polyline_distance(c, &[a, b]) < 0.2
            });
            assert!(part.is_some(), "{id}: no collider part on run {a}..{b}");
        }
        // The post feet: LOD0 corners at or below y -0.3 (0.3 m under the meadow relief, which
        // is at most 0), grouped by the nearest entry.
        let feet: Vec<Vec2> = glb_mesh_tris(id, "lod0")
            .unwrap()
            .iter()
            .flat_map(|(t, _)| t.iter())
            .filter(|p| p[1] <= -0.3 + 1e-4)
            .map(|p| Vec2::new(p[0], p[2]))
            .collect();
        let mut per: Vec<Vec<Vec2>> = vec![vec![]; f.posts.len()];
        for q in &feet {
            let (k, d) = f
                .posts
                .iter()
                .enumerate()
                .map(|(k, (at, _))| (k, q.distance(Vec2::from(*at))))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert!(d < 0.2, "{id}: a buried corner {q} off every post");
            per[k].push(*q);
        }
        for ((at, half), pts) in f.posts.iter().zip(&mut per) {
            // Each corner once (triangles repeat them).
            pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
            pts.dedup_by(|a, b| a.distance(*b) < 1e-3);
            assert_eq!(
                pts.len(),
                4,
                "{id}: post at {at:?} is not a square: {pts:?}"
            );
            let c = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
            // The foot is 0.3 m (DROP) below the ground, so a lean moves it under 1 cm.
            assert!(
                c.distance(Vec2::from(*at)) < 0.02,
                "{id} post {at:?} at {c}: {pts:?}"
            );
            let r = pts.iter().map(|p| p.distance(c)).fold(0.0, f32::max);
            assert!(
                (r - half * std::f32::consts::SQRT_2).abs() < 0.01,
                "{id} post {at:?}: corner reach {r}, half {half}"
            );
        }
    }
}

/// The paddock fence moves no farm scatter that stands clear of its line: with and without the
/// fence hero, a seed lays out the same instances but those on the line, and none of those
/// left stands within the line's clearance. The fence drops some, so it does matter.
#[test]
fn the_farm_fence_moves_no_scatter_off_its_line() {
    let c = catalog();
    let fenced = c.get("farm").unwrap().clone();
    let mut open = fenced.clone();
    open.props
        .hero
        .retain(|h| fence(h.piece.as_str()).is_none());
    assert_eq!(open.props.hero.len() + 1, fenced.props.hero.len());
    let (gf, go) = (prepared(&fenced).unwrap(), prepared(&open).unwrap());
    for (f, o) in gf.masks.iter().zip(&go.masks) {
        assert_eq!(
            (&f.cells, f.area),
            (&o.cells, o.area),
            "the fence leaves the masks alone"
        );
    }
    let lines = fence_lines(&fenced);
    let mut dropped = 0;
    for seed in 0..16u64 {
        let seed = cell_seed(1, Axial::new(seed as i32, 3 - seed as i32));
        let (a, b) = (layout(&fenced, seed, &gf), layout(&open, seed, &go));
        let clear = |i: &Instance| {
            let piece = i.piece(fenced.props.scatter[i.rule as usize].piece.as_str());
            let need = fence_clearance(footprint(piece).ground * i.scale);
            lines
                .iter()
                .all(|l| polyline_distance(i.pos.xz(), l) >= need)
        };
        let kept: Vec<&Instance> = b.instances.iter().filter(|i| clear(i)).collect();
        dropped += b.instances.len() - kept.len();
        assert_eq!(a.instances.len(), kept.len(), "seed {seed}");
        for (x, y) in a.instances.iter().zip(kept) {
            assert_eq!(
                (x.rule, x.pos, x.yaw, x.scale, x.variant),
                (y.rule, y.pos, y.yaw, y.scale, y.variant),
                "seed {seed}"
            );
        }
        assert_eq!(a.planned, b.planned);
    }
    assert!(dropped > 0, "no seed put a flower or rock on the line");
}

/// Each `data/pieces.ron` `ground_box` is its collider's bounds (to 1 cm) and the collider fills
/// them (a rectangle with at most chamfered corners), so a box clears no more than the piece.
#[test]
fn ground_boxes_match_the_kit() {
    for (id, b) in ground_box_rows() {
        let pts: Vec<Vec2> = glb_mesh_tris(id, "collider")
            .unwrap()
            .iter()
            .flat_map(|(t, _)| t.iter().map(|p| Vec2::new(p[0], p[2])))
            .collect();
        let lo = pts.iter().fold(Vec2::MAX, |a, p| a.min(*p));
        let hi = pts.iter().fold(Vec2::MIN, |a, p| a.max(*p));
        assert!(
            lo.distance(Vec2::from(b.min)) < 0.01 && hi.distance(Vec2::from(b.max)) < 0.01,
            "{id} collider bounds {lo}..{hi}"
        );
        let h = crate::walk::hull(&pts);
        let area = (0..h.len())
            .map(|i| h[i].perp_dot(h[(i + 1) % h.len()]))
            .sum::<f32>()
            / 2.0;
        let size = hi - lo;
        assert!(
            area >= 0.95 * size.x * size.y,
            "{id} fills {area} of its {size} box"
        );
    }
}

/// An [`is_ground_prop`] hero joins the ground in the tile frame, so every one in the catalog
/// stands at the origin at a whole hex turn, unlifted.
#[test]
fn ground_props_stand_at_the_origin_on_a_hex_turn() {
    let c = catalog();
    let mut seen = 0;
    for def in c.tiles() {
        for h in &def.props.hero {
            if is_ground_prop(h.piece.as_str()) {
                seen += 1;
                assert_eq!((h.at, h.y), ([0.0, 0.0], 0.0), "{}", def.id);
                assert_eq!(h.yaw_deg.rem_euclid(60.0), 0.0, "{}", def.id);
            }
        }
    }
    assert!(seen > 0, "the station tile carries its platform");
}

/// Station criterion 3 (scatter): the platform is the station tile's ground, so its
/// wildflowers keep off its whole footprint (its `ground_box`, ramps included) for
/// any seed: the paving is not their floor.
#[test]
fn station_scatter_keeps_off_the_platform() {
    let c = catalog();
    let def = c.get("rail_station").unwrap();
    assert!(
        ground_pieces(def, &c).contains(&("rail_station_platform", 0)),
        "the platform is ground"
    );
    let b = ground_box("rail_station_platform").unwrap();
    let g = prepared(def).unwrap();
    let mut n = 0;
    for seed in 0..16u64 {
        for inst in &layout(def, seed, &g).instances {
            n += 1;
            let fp = footprint(inst.piece(def.props.scatter[inst.rule as usize].piece.as_str()));
            let p = inst.pos.xz();
            assert!(
                p.x < b.min[0] - fp.ground
                    || p.x > b.max[0] + fp.ground
                    || p.y < b.min[1] - fp.ground
                    || p.y > b.max[1] + fp.ground,
                "seed {seed}: a flower at {p} on the platform"
            );
        }
    }
    assert!(n > 100, "{n} flowers over 16 seeds");
}

/// The sqrt-built directions and turns agree with the trig and `piece_rotation` the renderer
/// uses, so the exact layout frame is the drawn one.
#[test]
fn exact_directions_match_trig_and_piece_rotation() {
    for k in 0..6u8 {
        let a = (60.0 * f32::from(k)).to_radians();
        assert!(
            hex_dir(k).distance(Vec2::new(a.cos(), a.sin())) < 1e-6,
            "edge {k}"
        );
        let p = [3.0, 1.5, -7.0];
        let want = piece_rotation(k) * Vec3::from(p);
        assert!(Vec3::from(turn(k, p)).distance(want) < 1e-5, "turn {k}");
    }
    for (k, d) in ring16().iter().enumerate() {
        let a = TAU * k as f32 / 16.0;
        assert!(d.distance(Vec2::new(a.cos(), a.sin())) < 1e-6, "ring {k}");
    }
}

/// Spruces scattered round a hero oak leave its canopy clear, so its perches stay open. No
/// shipped tile pairs them since the split (U3), so the pine forest is given the oak tile's oak.
#[test]
fn spruces_clear_the_oak_canopy() {
    let c = catalog();
    let oak = c.get("oak").unwrap();
    assert!(oak.agents.iter().any(|a| a.agent == AgentKind::Bird));
    let mut def = c.get("forest").unwrap().clone();
    def.props.hero = oak.props.hero.clone();
    let g = prepared(&def).unwrap();
    for seed in 0..16 {
        for i in layout(&def, seed, &g)
            .instances
            .iter()
            .filter(|i| i.rule == rule_of(&def, "tree_forest"))
        {
            assert!(
                i.pos.xz().length() >= HERO_CROWN * 7.1 + 0.5 - 0.2,
                "{}",
                i.pos
            );
        }
    }
}

/// The pine forest's undergrowth (shrubs and saplings, forest ref-1) comes after its other rules:
/// the earlier rules' instances keep their spots, and the shrubs and saplings stand on the
/// litter with every planned instance placed.
#[test]
fn forest_undergrowth_keeps_the_earlier_spots() {
    let c = catalog();
    let pine = c.get("forest").unwrap();
    let n = pine.props.scatter.len();
    assert_eq!(pine.props.scatter[n - 2].piece.as_str(), "shrub_forest");
    assert_eq!(pine.props.scatter[n - 1].piece.as_str(), "sapling_spruce");
    let mut before = pine.clone();
    before.props.scatter.truncate(n - 2);
    let g = prepared(pine).unwrap();
    let (mut planned, mut placed) = ([0u32; 2], [0usize; 2]);
    for seed in 0..32 {
        let l = layout(pine, seed, &g);
        let old = layout(&before, seed, &g);
        let kept: Vec<_> = l
            .instances
            .iter()
            .filter(|i| usize::from(i.rule) < n - 2)
            .collect();
        assert_eq!(kept.len(), old.instances.len(), "seed {seed}");
        for (a, b) in kept.iter().zip(&old.instances) {
            assert_eq!((a.rule, a.pos), (b.rule, b.pos), "seed {seed}");
        }
        for k in 0..2 {
            planned[k] += l.planned[n - 2 + k];
            placed[k] += l.placed(n - 2 + k);
        }
        for i in l.instances.iter().filter(|i| usize::from(i.rule) >= n - 2) {
            let (_, slot) = g.surface.at(i.pos.x, i.pos.z).unwrap();
            assert_eq!(slot, "forest_base_floor", "undergrowth on the litter");
        }
    }
    assert_eq!(
        placed[0], planned[0] as usize,
        "shrubs {placed:?} of {planned:?}"
    );
    assert_eq!(
        placed[1], planned[1] as usize,
        "saplings {placed:?} of {planned:?}"
    );
    assert!(planned[0] > 32 && planned[1] > 32, "{planned:?}");
}

/// Verge bushes (road ref-1's "grass verges with tufts and small bushes") come last on the four
/// road tiles without parking: the wildflowers keep their spots, every planned bush is placed,
/// and each bush's widest reach over its LODs (its leaves, wider than its collider; LOD2's
/// scaled-up cards reach furthest), taken per variant, at its scale stays beyond the kerb
/// (3.25 m from the centreline), never over asphalt, kerb or paint, with its centre inside the
/// verge band, on every seed; every variant is drawn.
#[test]
fn verge_bushes_line_the_road_beyond_the_kerb() {
    const KERB_OUT: f32 = 3.25;
    let leaves: std::collections::BTreeMap<&str, f32> = group("bush_verge")
        .map(|v| {
            let r = (0..3)
                .flat_map(|l| glb_mesh_tris(v, &format!("lod{l}")).unwrap())
                .flat_map(|(t, _)| t)
                .map(|p| Vec2::new(p[0], p[2]).length())
                .fold(0.0, f32::max);
            (v, r)
        })
        .collect();
    assert!(
        (0.55..0.8).contains(&leaves["bush_verge"]),
        "widest LOD reach {leaves:?}"
    );
    assert!(
        (0.4..0.6).contains(&leaves["bush_verge_b"]),
        "widest LOD reach {leaves:?}"
    );
    assert!(
        (0.5..0.7).contains(&leaves["bush_verge_c"]),
        "widest LOD reach {leaves:?}"
    );
    let mut drawn = std::collections::BTreeSet::new();
    let c = catalog();
    for id in [
        "road_straight",
        "road_bend_60",
        "road_bend_120",
        "road_junction",
    ] {
        let def = c.get(id).unwrap();
        let n = def.props.scatter.len();
        assert_eq!(
            def.props.scatter[n - 1].piece.as_str(),
            "bush_verge",
            "{id}"
        );
        let mut before = def.clone();
        before.props.scatter.truncate(n - 1);
        let g = prepared(def).unwrap();
        let lanes = lanes(def);
        let (mut planned, mut placed) = (0u32, 0usize);
        for seed in 0..32u64 {
            let l = layout(def, seed, &g);
            let old = layout(&before, seed, &g);
            let kept: Vec<_> = l
                .instances
                .iter()
                .filter(|i| usize::from(i.rule) < n - 1)
                .collect();
            assert_eq!(kept.len(), old.instances.len(), "{id} seed {seed}");
            for (a, b) in kept.iter().zip(&old.instances) {
                assert_eq!((a.rule, a.pos), (b.rule, b.pos), "{id} seed {seed}");
            }
            planned += l.planned[n - 1];
            placed += l.placed(n - 1);
            for i in l.instances.iter().filter(|i| usize::from(i.rule) == n - 1) {
                let p = i.pos.xz();
                let piece = i.piece("bush_verge");
                drawn.insert(piece);
                let reach = leaves[piece] * i.scale;
                let d = lanes
                    .iter()
                    .map(|(line, _)| polyline_distance(p, line))
                    .fold(f32::MAX, f32::min);
                assert!(
                    d >= KERB_OUT + reach,
                    "{id} seed {seed}: bush at {p} {d} m out"
                );
                // The band is tested on raster cell centres; a dart lands within half a cell.
                let jitter = RASTER * std::f32::consts::FRAC_1_SQRT_2;
                assert!(
                    d <= VERGE_BAND + jitter,
                    "{id} seed {seed}: bush at {p} {d} m out"
                );
                for q in std::iter::once(p).chain(ring16().iter().map(|r| p + reach * *r)) {
                    let (_, slot) = g.surface.at(q.x, q.y).unwrap();
                    assert!(
                        !["road_asphalt", "road_kerb", "road_paint"].contains(&slot),
                        "{id} seed {seed}: bush at {p} over {slot}"
                    );
                }
                let (_, slot) = g.surface.at(p.x, p.y).unwrap();
                assert_eq!(slot, "meadow_base_cap", "{id} seed {seed}: bush at {p}");
            }
        }
        assert_eq!(
            placed, planned as usize,
            "{id}: bushes {placed} of {planned}"
        );
        assert!(planned > 32, "{id}: {planned}");
    }
    assert_eq!(drawn.len(), leaves.len(), "variants drawn {drawn:?}");
}

/// No tile with parking carries verge bushes: hex-sim's parking sweep (tests/parking.rs) checks
/// only each tile's declared obstacle boxes, so a scattered bush could stand in a car's reverse
/// swing across the verge unseen. A parking tile needs its own bush slots cleared against the
/// sweep first.
#[test]
fn no_verge_bushes_on_parking_tiles() {
    for def in catalog().tiles() {
        if !def.parking.is_empty() {
            assert!(
                def.props
                    .scatter
                    .iter()
                    .all(|r| verge_band(r.piece.as_str()).is_none()),
                "{} has parking and verge bushes",
                def.id
            );
        }
    }
}

/// The split (U3): the pine forest is spruces and rocks with no hero and no bird roost; the oak
/// tile is the hero oak, rocks and a few flowers on forest_base's litter, and the roost. Both
/// have Forest on every edge.
#[test]
fn pine_and_oak_tiles_split_the_forest() {
    let c = catalog();
    let pine = c.get("forest").unwrap();
    let oak = c.get("oak").unwrap();
    for d in [pine, oak] {
        assert_eq!(d.base.as_str(), "forest_base", "{}", d.id);
        assert!(
            d.edges.iter().all(|e| e.kind == SocketKind::Forest),
            "{}",
            d.id
        );
    }
    let pieces = |d: &TileDef| -> Vec<String> {
        d.props
            .scatter
            .iter()
            .map(|r| r.piece.as_str().to_owned())
            .collect()
    };
    assert_eq!(
        pieces(pine),
        [
            "outcrop_granite",
            "boulder_granite",
            "log_fallen",
            "tree_forest",
            "rock_small",
            "fern_clump",
            "shrub_forest",
            "sapling_spruce"
        ]
    );
    assert!(pine.props.hero.is_empty() && pine.agents.is_empty());
    assert_eq!(
        pieces(oak),
        [
            "rock_small",
            "wildflower_clump",
            "tree_oak",
            "boulder_granite"
        ]
    );
    assert_eq!(oak.props.hero.len(), 1);
    assert_eq!(oak.props.hero[0].piece.as_str(), "tree_hero_oak");
    let g = prepared(oak).unwrap();
    let mut flowers = 0;
    for seed in 0..32 {
        let l = layout(oak, seed, &g);
        assert_eq!(l.placed(0), l.planned[0] as usize, "rocks");
        assert_eq!(l.placed(1), l.planned[1] as usize, "flowers");
        flowers += l.placed(1);
        for i in l.instances.iter().filter(|i| i.rule == 1) {
            let (_, slot) = g.surface.at(i.pos.x, i.pos.z).unwrap();
            assert_eq!(slot, "forest_base_floor", "a flower on the litter");
        }
    }
    let mean = flowers as f32 / 32.0;
    assert!((4.0..=8.0).contains(&mean), "a few flowers: {mean}");
}

/// Drift rules cluster: over 4 m quadrats of the ground, the meadow's flowers have an index of
/// dispersion (variance over mean count) well above 1, where the same rule spread evenly stays
/// below it. The drifts follow the tile's seed, not a fixed pattern.
#[test]
fn meadow_flowers_grow_in_clustered_drifts() {
    let c = catalog();
    let meadow = c.get("meadow").unwrap();
    let fl = usize::from(rule_of(meadow, "wildflower_clump"));
    assert!(meadow.props.scatter[fl].drift, "the meadow's flowers drift");
    let rocks = usize::from(rule_of(meadow, "rock_small"));
    assert!(!meadow.props.scatter[rocks].drift, "its rocks do not");
    let mut even = meadow.clone();
    even.props.scatter[fl].drift = false;
    let g = prepared(meadow).unwrap();
    let quad = |x: f32, z: f32| ((x / 4.0).floor() as i32, (z / 4.0).floor() as i32);
    let dispersion = |def: &TileDef, seed: u64| {
        let l = layout(def, seed, &g);
        let mut quads: BTreeMap<(i32, i32), u32> = BTreeMap::new();
        for &k in &g.masks[fl].cells {
            let k = k as usize;
            quads
                .entry(quad(cell_centre(k % CELLS), cell_centre(k / CELLS)))
                .or_default();
        }
        for i in l.instances.iter().filter(|i| usize::from(i.rule) == fl) {
            *quads.entry(quad(i.pos.x, i.pos.z)).or_default() += 1;
        }
        let n = quads.len() as f32;
        let mean = quads.values().map(|&v| v as f32).sum::<f32>() / n;
        let var = quads
            .values()
            .map(|&v| (v as f32 - mean).powi(2))
            .sum::<f32>()
            / n;
        var / mean
    };
    let seeds: Vec<u64> = (0..16)
        .map(|k| cell_seed(5, Axial::new(k, 1 - k)))
        .collect();
    let drift = seeds.iter().map(|&s| dispersion(meadow, s)).sum::<f32>() / 16.0;
    let spread = seeds.iter().map(|&s| dispersion(&even, s)).sum::<f32>() / 16.0;
    assert!(
        drift > 1.6,
        "drifts cluster: dispersion {drift} (even {spread})"
    );
    assert!(spread < 1.0, "an even spread does not: dispersion {spread}");
    let (a, b) = (seeds[0], seeds[1]);
    let p = Vec2::new(3.0, -4.0);
    assert_ne!(drift_noise(a, p), drift_noise(b, p), "seeded per tile");
    for k in 0..200 {
        let q = Vec2::new(k as f32 * 0.37 - 30.0, 17.0 - k as f32 * 0.21);
        assert!((0.0..1.0).contains(&drift_noise(a, q)), "{q}");
    }
}

/// Scatter keeps clear of a farm's boundary toward a land footprint's ring cell: the fence
/// map reads the ring cell (2, 0) as the footprint meadow (`covering`), its only land
/// neighbour.
#[test]
fn the_fence_map_reads_a_ring_cell_as_its_footprint() {
    let c = crate::play::tests::footprint_fixtures();
    let tiles: HashMap<Axial, (crate::play::PlayTile, Entity)> =
        [((3, 0), "meadow"), ((1, 0), "farm")]
            .into_iter()
            .map(|((q, r), id)| {
                let cell = Axial::new(q, r);
                let t = crate::play::PlayTile {
                    cell,
                    tile: id.into(),
                    rotation: 0,
                    level: 0,
                };
                (cell, (t, Entity::PLACEHOLDER))
            })
            .collect();
    let map = fence_map(1, &tiles, &c);
    assert!(
        map.get(&Axial::new(1, 0)).is_some_and(|s| !s.is_empty()),
        "the farm has no boundary toward the ring cell: {map:?}"
    );
}

/// The footprint's own fence map entry, which its ring scatter and ring grass keep clear of,
/// carries the farm's wall on the ring cell (2, 0)'s outer edge, in the anchor's frame: about
/// three apothems from the anchor, toward the farm.
#[test]
fn the_fence_map_gives_a_footprint_its_ring_cells_walls() {
    let c = crate::play::tests::footprint_fixtures();
    for rotation in [0u8, 2] {
        let tiles: HashMap<Axial, (crate::play::PlayTile, Entity)> =
            [((3, 0), "meadow", rotation), ((1, 0), "farm", 0)]
                .into_iter()
                .map(|((q, r), id, rotation)| {
                    let cell = Axial::new(q, r);
                    let t = crate::play::PlayTile {
                        cell,
                        tile: id.into(),
                        rotation,
                        level: 0,
                    };
                    (cell, (t, Entity::PLACEHOLDER))
                })
                .collect();
        let map = fence_map(1, &tiles, &c);
        let segs = map
            .get(&Axial::new(3, 0))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("r{rotation}: no wall in the footprint's entry: {map:?}"));
        // The farm lies west of the anchor in the world; the anchor's frame turns it back.
        let toward = turn((6 - rotation) % 6, [-1.0, 0.0, 0.0]);
        for s in segs {
            let mid = (Vec2::from(s.a) + Vec2::from(s.b)) / 2.0;
            assert!(
                (mid.length() - 3.0 * APOTHEM).abs() < HEX_RADIUS,
                "r{rotation}: {s:?} is not on the ring cell's outer edge"
            );
            assert!(
                mid.normalize().dot(Vec2::new(toward[0], toward[2])) > 0.8,
                "r{rotation}: {s:?} does not face the farm"
            );
        }
    }
}
