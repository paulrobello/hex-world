//! Kit asset gate (PRD §11.2 step 7, R7). Every manifest piece is loaded through the `kit://`
//! meshopt rewrite and bevy_gltf, and through the `gltf` crate for exact geometry checks.
//! Runs in `make test` (CI) and `make kit-check`. Fixture manifest: tests/fixtures/kit.
//! Reads committed files only: CI has no Blender or KTX tools.
use ::bevy::{
    asset::{AssetPlugin, AssetServer, Assets, Handle, LoadState},
    gltf::{Gltf, GltfNode, GltfPlugin},
    mesh::MeshPlugin,
    prelude::*,
    world_serialization::WorldSerializationPlugin,
};
use hex_assets::*;
use hex_play::water::tiles::{Coastline, Mouth, coast_height};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The §9.4 hook gate (`tests/hooks/mod.rs`), so `make kit-check` covers it.
mod hooks;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

struct Kit {
    /// Directory served as `kit://` (holds manifest.json).
    root: PathBuf,
    manifest: KitManifest,
    /// Real kit pieces need owner-approved references in provenance (R2, S5). The fixture has none.
    requires_refs: bool,
}

fn kits() -> Vec<Kit> {
    [
        (repo().join("crates/hex-render/tests/fixtures/kit"), false),
        (repo().join("assets/kit"), true),
    ]
    .into_iter()
    .map(|(root, requires_refs)| {
        let json = std::fs::read_to_string(root.join("manifest.json")).unwrap();
        let manifest = parse_manifest(&json).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
        Kit {
            root,
            manifest,
            requires_refs,
        }
    })
    .collect()
}

fn fail(errors: Vec<String>) {
    assert!(errors.is_empty(), "kit gate:\n  {}", errors.join("\n  "));
}

/// The decoded (plain) glTF: document + buffers, via hex-assets' meshopt rewrite.
fn plain(
    kit: &Kit,
    p: &PieceEntry,
) -> Result<(gltf::Document, Vec<gltf::buffer::Data>, u64), String> {
    let path = kit.root.join(&p.glb);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let out = rewrite_glb(&bytes).map_err(|e| format!("{}: {e}", p.id))?;
    let (doc, buffers, _) = gltf::import_slice(&out).map_err(|e| format!("{}: {e}", p.id))?;
    Ok((doc, buffers, bytes.len() as u64))
}

fn mesh_named<'a>(doc: &'a gltf::Document, name: &str) -> Option<gltf::Mesh<'a>> {
    doc.meshes().find(|m| m.name() == Some(name))
}

fn tris(m: &gltf::Mesh) -> usize {
    m.primitives()
        .map(|p| p.indices().map_or(0, |i| i.count()) / 3)
        .sum()
}

fn positions(m: &gltf::Mesh, buffers: &[gltf::buffer::Data]) -> Vec<([f32; 3], [f32; 3])> {
    let mut out = vec![];
    for prim in m.primitives() {
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let pos: Vec<[f32; 3]> = r.read_positions().map(|i| i.collect()).unwrap_or_default();
        let nrm: Vec<[f32; 3]> = r.read_normals().map(|i| i.collect()).unwrap_or_default();
        out.extend(pos.into_iter().zip(nrm));
    }
    out
}

/// Runtime frame: edge k's outward normal is at 60°·k in XZ (axial.rs: edge 0 = +x, edge 1 = +z).
fn dist_to_border(x: f32, z: f32) -> f32 {
    let m = (0..6)
        .map(|k| {
            let a = (60.0 * k as f32).to_radians();
            x * a.cos() + z * a.sin()
        })
        .fold(f32::MIN, f32::max);
    APOTHEM - m
}

/// Cap vertices on the hex border at y = 0 as (x, z), with split-normal duplicates removed.
fn border_ring(v: &[([f32; 3], [f32; 3])]) -> Vec<[f32; 2]> {
    border_ring_at(v, 0.0)
}

/// Heights a base's cap border ring may sit at, one per edge-socket bed profile that is flat
/// (docs/kit/water-contract.md §6): Grass and Forest at y = 0 (`meadow_base`), Ocean at the tile
/// base y = -LEVEL_STEP (`ocean_base`, the ocean bed is the column floor).
const BASE_RING_LEVELS: [f32; 2] = [0.0, -hex_core::LEVEL_STEP];

/// The y of a base's cap border ring: its up-facing border vertices must share one height, and
/// that height must be a contract level. No such vertices gives 0 (the ring check then fails).
fn base_ring_level(id: &str, v: &[([f32; 3], [f32; 3])]) -> Result<f32, String> {
    let ys: Vec<f32> = v
        .iter()
        .filter(|(p, n)| dist_to_border(p[0], p[2]).abs() < 1e-3 && n[1] > 0.5)
        .map(|(p, _)| p[1])
        .collect();
    let Some(&y) = ys.first() else {
        return Ok(0.0);
    };
    if ys.iter().any(|q| (q - y).abs() > 1e-3) {
        return Err(format!("{id}: base border ring is not flat"));
    }
    BASE_RING_LEVELS
        .into_iter()
        .find(|l| (l - y).abs() < 1e-3)
        .ok_or_else(|| format!("{id}: base border ring at y = {y} is not a contract level"))
}

/// Cap vertices on the hex border at y = `level` as (x, z), with split-normal duplicates removed.
fn border_ring_at(v: &[([f32; 3], [f32; 3])], level: f32) -> Vec<[f32; 2]> {
    let mut ring: Vec<[f32; 2]> = vec![];
    for (p, n) in v {
        if dist_to_border(p[0], p[2]).abs() < 1e-3
            && (p[1] - level).abs() < 1e-3
            && n[1] > 0.5
            && !ring.iter().any(|q| near(*q, [p[0], p[2]], RING_TOL))
        {
            ring.push([p[0], p[2]]);
        }
    }
    ring
}

/// Ring positions match within 1e-4 m (gates.md, Seam: border ring).
const RING_TOL: f32 = 1e-4;

fn near(a: [f32; 2], b: [f32; 2], tol: f32) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
}

/// Same point set within `tol`, matched by nearest neighbour (no grid-rounding boundary cases).
fn same_points(a: &[[f32; 2]], b: &[[f32; 2]], tol: f32) -> bool {
    a.len() == b.len()
        && a.iter().all(|p| b.iter().any(|q| near(*p, *q, tol)))
        && b.iter().all(|q| a.iter().any(|p| near(*p, *q, tol)))
}

/// Border-ring errors given each LOD's ring. Every LOD shares LOD0's ring; for a base the ring
/// also has at least 24 vertices and maps onto itself under a 60° rotation.
fn ring_errors(id: &str, rings: &[Vec<[f32; 2]>], base: bool) -> Vec<String> {
    let mut e = vec![];
    let Some(ring) = rings.first() else {
        return e;
    };
    if rings.iter().any(|r| !same_points(r, ring, RING_TOL)) {
        e.push(format!("{id}: border ring differs across LODs"));
    }
    if base {
        if ring.len() < 24 {
            e.push(format!("{id}: border ring has {} vertices", ring.len()));
        }
        let (s, c) = 60f32.to_radians().sin_cos();
        let rot: Vec<[f32; 2]> = ring
            .iter()
            .map(|&[x, z]| [x * c - z * s, x * s + z * c])
            .collect();
        if !same_points(&rot, ring, RING_TOL) {
            e.push(format!(
                "{id}: border ring is not invariant under 60° rotation"
            ));
        }
    }
    e
}

/// Lane-socket errors: a socket's world position must sit on the lane contract for its extras.
fn socket_errors(id: &str, name: &str, extras: &serde_json::Value, got: [f32; 3]) -> Vec<String> {
    let (Some(edge), Some(offset)) = (extras["edge"].as_u64(), extras["offset"].as_f64()) else {
        return vec![format!("{id}: {name} extras lack edge/offset: {extras}")];
    };
    let a = (60.0 * edge as f32).to_radians();
    // hexgeo.lane_frame: Blender tangent t = (-n_y, n_x) = (sin a, cos a), which is runtime
    // (sin a, -cos a) in XZ for the runtime edge normal (cos a, sin a).
    let want = [
        APOTHEM * a.cos() + a.sin() * offset as f32,
        0.0,
        APOTHEM * a.sin() - a.cos() * offset as f32,
    ];
    if (0..3).any(|i| (got[i] - want[i]).abs() > if i == 1 { 0.002 } else { 0.001 }) {
        return vec![format!("{id}: {name} at {got:?}, lane contract {want:?}")];
    }
    vec![]
}

/// One vertex of a tree LOD for the wind gate: runtime position, TEXCOORD_1 (None for a primitive
/// without it), and whether its slot is an alpha-cutoff leaf-card (fringe) slot.
type WindVert = ([f32; 3], Option<[f32; 2]>, bool);

fn wind_verts(p: &PieceEntry, m: &gltf::Mesh, buffers: &[gltf::buffer::Data]) -> Vec<WindVert> {
    let mut out = vec![];
    for prim in m.primitives() {
        let fringe = prim
            .material()
            .name()
            .and_then(|n| p.slots.get(n))
            .is_some_and(|s| s.alpha_cutoff.is_some());
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let pos: Vec<[f32; 3]> = r.read_positions().map(|i| i.collect()).unwrap_or_default();
        match r.read_tex_coords(1) {
            Some(uv) => out.extend(
                pos.into_iter()
                    .zip(uv.into_f32())
                    .map(|(p, w)| (p, Some(w), fringe)),
            ),
            None => out.extend(pos.into_iter().map(|p| (p, None, fringe))),
        }
    }
    out
}

/// One primitive of a LOD for the foliage wind gate: its slot name, the slot's `wind` amplitude
/// (None for a still slot) and its TEXCOORD_1 (None when the primitive has none).
type WindPrim = (String, Option<f32>, Option<Vec<[f32; 2]>>);

fn wind_prims(p: &PieceEntry, m: &gltf::Mesh, buffers: &[gltf::buffer::Data]) -> Vec<WindPrim> {
    m.primitives()
        .map(|prim| {
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let name = prim.material().name().unwrap_or_default().to_string();
            let amp = p.slots.get(&name).and_then(|s| s.wind);
            (
                name,
                amp,
                r.read_tex_coords(1).map(|uv| uv.into_f32().collect()),
            )
        })
        .collect()
}

/// Max wind amplitude (m per unit push) a slot may declare: past it the shader's bend reads as a
/// gale on the strongest gusts (push about 0.8).
const WIND_AMP_MAX: f32 = 1.0;

/// Foliage wind contract (`Slot::wind`, `foliage_wind.wesl`): a wind slot's amplitude is in
/// (0, WIND_AMP_MAX] and the same on every wind slot of the piece (its parts move together); every
/// primitive of a wind slot, in every LOD, carries TEXCOORD_1 with U and V in [0, 1], so
/// `kit-compress` and the LODs kept it; and LOD0's wind slots sway somewhere (U > 0).
fn wind_slot_errors(id: &str, lod: usize, prims: &[WindPrim]) -> Vec<String> {
    let mut e = vec![];
    let amps: Vec<f32> = prims.iter().filter_map(|(_, a, _)| *a).collect();
    if let Some(a) = amps.iter().find(|a| !(**a > 0.0 && **a <= WIND_AMP_MAX)) {
        e.push(format!(
            "{id}: wind amplitude {a} outside (0, {WIND_AMP_MAX}]"
        ));
    }
    if amps.windows(2).any(|w| w[0] != w[1]) {
        e.push(format!("{id}: wind slots differ in amplitude {amps:?}"));
    }
    let mut max_u = 0.0f32;
    for (slot, amp, uv) in prims {
        if amp.is_none() {
            continue;
        }
        let Some(uv) = uv else {
            e.push(format!(
                "{id}: {id}_lod{lod} wind slot {slot} has no TEXCOORD_1 (wind)"
            ));
            continue;
        };
        if uv
            .iter()
            .any(|w| w.iter().any(|x| !(0.0..=1.0).contains(x)))
        {
            e.push(format!("{id}: {id}_lod{lod} {slot} wind UV outside [0, 1]"));
        }
        max_u = uv.iter().fold(max_u, |m, w| m.max(w[0]));
    }
    if lod == 0 && !amps.is_empty() && max_u <= 0.0 {
        e.push(format!(
            "{id}: {id}_lod0 wind slots never sway (U = 0 everywhere)"
        ));
    }
    e
}

/// One primitive of a car LOD for the paint-mask gate: its material name and COLOR_0 (None when
/// the primitive has none).
type PaintPrim = (String, Option<Vec<[f32; 4]>>);

fn paint_prims(m: &gltf::Mesh, buffers: &[gltf::buffer::Data]) -> Vec<PaintPrim> {
    m.primitives()
        .map(|prim| {
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let name = prim.material().name().unwrap_or_default().to_string();
            (name, r.read_colors(0).map(|c| c.into_rgba_f32().collect()))
        })
        .collect()
}

/// Car runtime-recolour contract (PRD §6, `kit/_car.py`): COLOR_0 on every primitive of every LOD,
/// rgb in [0, 1] (the unpainted surface colour), A a binary paint mask (0 or 1). The `<id>_body`
/// slot carries paint, and the `<id>_glass` and `<id>_tyre` slots are never painted.
fn paint_mask_errors(id: &str, lod: &str, prims: &[PaintPrim]) -> Vec<String> {
    let mut e = vec![];
    for (slot, colors) in prims {
        let Some(c) = colors else {
            e.push(format!("{id}: {lod} {slot} has no COLOR_0 (paint mask)"));
            continue;
        };
        if c.iter().any(|v| v.iter().any(|x| !(0.0..=1.0).contains(x))) {
            e.push(format!("{id}: {lod} {slot} COLOR_0 outside [0, 1]"));
        }
        if c.iter().any(|v| v[3] > 1e-3 && v[3] < 1.0 - 1e-3) {
            e.push(format!("{id}: {lod} {slot} paint mask is not binary"));
        }
        let painted = c.iter().filter(|v| v[3] > 0.5).count();
        if slot.ends_with("_body") && painted == 0 {
            e.push(format!("{id}: {lod} {slot} has no painted vertex"));
        }
        if (slot.ends_with("_glass") || slot.ends_with("_tyre")) && painted > 0 {
            e.push(format!("{id}: {lod} {slot} has {painted} painted vertices"));
        }
    }
    e
}

/// Runtime height (the pivot is the trunk base at y = 0) below which wind U must stay near 0.
const WIND_BASE_Y: f32 = 0.5;
/// Max U in that band: the oak measures 0.04 (LOD0/1) and 0.015 (LOD2).
const WIND_BASE_MAX_U: f32 = 0.1;
/// Min median U on the alpha-cutoff fringe cards: the oak measures 0.91-0.92.
const WIND_FRINGE_MIN_MEDIAN_U: f32 = 0.5;

/// Tree wind contract (`project.md`, "Tree wind data"): TEXCOORD_1 on every primitive of every
/// LOD, both channels in [0, 1] (U = sway weight, V = phase), U = 0 on the lowest vertices, U near 0
/// on everything below WIND_BASE_Y (trunk base and roots barely move), and the leaf-card fringe
/// swaying strongly (median U at least WIND_FRINGE_MIN_MEDIAN_U).
fn wind_errors(id: &str, lod: &str, v: &[WindVert]) -> Vec<String> {
    if v.iter().any(|(_, w, _)| w.is_none()) {
        return vec![format!(
            "{id}: {lod} has a primitive without TEXCOORD_1 (wind)"
        )];
    }
    let mut e = vec![];
    let w: Vec<([f32; 3], [f32; 2], bool)> =
        v.iter().map(|(p, w, f)| (*p, w.unwrap(), *f)).collect();
    if let Some((_, bad, _)) = w
        .iter()
        .find(|(_, uv, _)| !uv.iter().all(|c| (0.0..=1.0).contains(c)))
    {
        e.push(format!("{id}: {lod} wind {bad:?} outside [0, 1]"));
    }
    let min_y = w.iter().map(|(p, _, _)| p[1]).fold(f32::INFINITY, f32::min);
    if let Some((_, uv, _)) = w
        .iter()
        .filter(|(p, _, _)| p[1] < min_y + 0.01)
        .find(|(_, uv, _)| uv[0] > 1e-3)
    {
        e.push(format!(
            "{id}: {lod} lowest trunk vertex has wind U {} (want 0)",
            uv[0]
        ));
    }
    if let Some((p, uv, _)) = w
        .iter()
        .filter(|(p, _, _)| p[1] < WIND_BASE_Y)
        .find(|(_, uv, _)| uv[0] > WIND_BASE_MAX_U)
    {
        e.push(format!(
            "{id}: {lod} wind U {} at {p:?}, below y {WIND_BASE_Y} m (trunk base, want <= {WIND_BASE_MAX_U})",
            uv[0]
        ));
    }
    let fringe: Vec<f32> = w
        .iter()
        .filter(|(_, _, f)| *f)
        .map(|(_, uv, _)| uv[0])
        .collect();
    let m = median(&fringe);
    if fringe.is_empty() || m < WIND_FRINGE_MIN_MEDIAN_U {
        e.push(format!(
            "{id}: {lod} leaf-card fringe median wind U {m} < {WIND_FRINGE_MIN_MEDIAN_U} ({} fringe vertices)",
            fringe.len()
        ));
    }
    e
}

/// Minimum median texel density per textured material (D2: trees and props ≥ 256 px/m).
const TEXEL_MIN_PX_PER_M: f32 = 256.0;

/// Per-triangle texel density in px/m: sqrt(uv_area · px² / world_area). `px` is the texture width
/// times any UV0 scale (a tileable set's `tileUvScale`). Degenerate triangles are skipped.
fn face_densities(pos: &[[f32; 3]], uv: &[[f32; 2]], idx: &[u32], px: f32) -> Vec<f32> {
    let sub3 = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    idx.as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| {
            let [a, b, c] = t.map(|i| i as usize);
            let (e1, e2) = (sub3(pos[b], pos[a]), sub3(pos[c], pos[a]));
            let cr = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let world = 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
            let (u1, u2) = (
                [uv[b][0] - uv[a][0], uv[b][1] - uv[a][1]],
                [uv[c][0] - uv[a][0], uv[c][1] - uv[a][1]],
            );
            let uv_area = 0.5 * (u1[0] * u2[1] - u1[1] * u2[0]).abs();
            (world > 1e-9 && uv_area > 1e-12).then(|| (uv_area * px * px / world).sqrt())
        })
        .collect()
}

fn median(v: &[f32]) -> f32 {
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    let n = s.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        s[n / 2]
    } else {
        0.5 * (s[n / 2 - 1] + s[n / 2])
    }
}

/// Texel-density floor checked per material (owner ruling, hero oak look pass): one pooled median let
/// the many leaf cards hide low bark density.
fn texel_errors(id: &str, material: &str, densities: &[f32]) -> Vec<String> {
    let m = median(densities);
    if m < TEXEL_MIN_PX_PER_M {
        vec![format!(
            "{id}: {material} median texel density {m:.1} px/m < {TEXEL_MIN_PX_PER_M} (per material)"
        )]
    } else {
        vec![]
    }
}

/// PNG width from the IHDR chunk.
fn png_width(path: &Path) -> Result<f32, String> {
    let b = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if b.len() < 24 || &b[12..16] != b"IHDR" {
        return Err(format!("{}: not a PNG", path.display()));
    }
    Ok(u32::from_be_bytes([b[16], b[17], b[18], b[19]]) as f32)
}

/// The texture width (times tileUvScale) a slot samples on UV0, or None for world-space tileable,
/// colour-only and masks-only slots, which have no UV0 texel density to check. A UV0 slot whose
/// albedo is missing is an error, never a silent skip.
fn uv0_texture_px(
    kit: &Kit,
    p: &PieceEntry,
    name: &str,
    slot: &Slot,
) -> Option<Result<f32, String>> {
    let stem = match (&slot.tileable, slot.tile_uv_scale) {
        (Some(set), Some(_)) => kit
            .manifest
            .materials
            .get(set)
            .and_then(|ts| ts.maps.get(&Channel::Albedo))
            .ok_or_else(|| {
                format!(
                    "{}: {name} maps tileable set {set} on UV0, but the set has no albedo",
                    p.id
                )
            }),
        (None, _) if slot.unique => p.maps.get(&Channel::Albedo).ok_or_else(|| {
            format!(
                "{}: unique slot {name} but the piece has no albedo map (texel density unchecked)",
                p.id
            )
        }),
        _ => return None,
    };
    Some(stem.and_then(|stem| {
        let path = kit.root.join(map_file(stem, TextureFamily::Png));
        png_width(&path).map(|w| w * slot.tile_uv_scale.unwrap_or(1.0))
    }))
}

fn check_texel_density(
    kit: &Kit,
    p: &PieceEntry,
    lod0: &[&gltf::Mesh],
    buffers: &[gltf::buffer::Data],
) -> Vec<String> {
    let mut per: std::collections::BTreeMap<String, Vec<f32>> = Default::default();
    let mut e = vec![];
    for prim in lod0.iter().flat_map(|m| m.primitives()) {
        let Some(name) = prim.material().name() else {
            e.push(format!(
                "{}: a LOD0 primitive has no named material (no manifest slot)",
                p.id
            ));
            continue;
        };
        let Some(slot) = p.slots.get(name) else {
            continue; // check_geometry reports the missing slot
        };
        let px = match uv0_texture_px(kit, p, name, slot) {
            None => continue,
            Some(Err(err)) => {
                e.push(err);
                continue;
            }
            Some(Ok(px)) => px,
        };
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let pos: Vec<[f32; 3]> = r.read_positions().map(|i| i.collect()).unwrap_or_default();
        let uv: Vec<[f32; 2]> = r
            .read_tex_coords(0)
            .map(|t| t.into_f32().collect())
            .unwrap_or_default();
        let idx: Vec<u32> = r
            .read_indices()
            .map(|i| i.into_u32().collect())
            .unwrap_or_default();
        if uv.len() != pos.len() {
            e.push(format!(
                "{}: {name} has no TEXCOORD_0 for its texture",
                p.id
            ));
            continue;
        }
        per.entry(name.to_string())
            .or_default()
            .extend(face_densities(&pos, &uv, &idx, px));
    }
    for (name, d) in &per {
        e.extend(texel_errors(&p.id, name, d));
    }
    e
}

type Mat4 = [[f32; 4]; 4];

/// Column-major product a · b (glTF matrices are column-major).
fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [[0.0; 4]; 4];
    for (c, col) in m.iter_mut().enumerate() {
        for (r, v) in col.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    m
}

/// World matrix of every node, composing parents from each scene root down.
fn world_matrices(doc: &gltf::Document) -> Vec<Mat4> {
    const ID: Mat4 = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut world = vec![ID; doc.nodes().count()];
    fn walk(n: gltf::Node, parent: &Mat4, world: &mut [Mat4]) {
        let m = mul(parent, &n.transform().matrix());
        world[n.index()] = m;
        for c in n.children() {
            walk(c, &m, world);
        }
    }
    for s in doc.scenes() {
        for n in s.nodes() {
            walk(n, &ID, &mut world);
        }
    }
    world
}

fn check_geometry(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    let mut e = vec![];
    let (doc, buffers, glb_bytes) = match plain(kit, p) {
        Ok(x) => x,
        Err(err) => return vec![err],
    };
    if glb_bytes > GLB_MAX_BYTES {
        e.push(format!("{}: GLB {glb_bytes} B > {GLB_MAX_BYTES}", p.id));
    }
    if doc.images().count() > 0 || doc.textures().count() > 0 {
        e.push(format!("{}: GLB embeds images (R6)", p.id));
    }
    if doc.extensions_used().any(|x| x == "KHR_mesh_quantization") {
        e.push(format!(
            "{}: KHR_mesh_quantization is unsupported by bevy_gltf (D7)",
            p.id
        ));
    }
    let lods: Vec<_> = (0..3)
        .map(|i| mesh_named(&doc, &lod_mesh_name(&p.id, i)))
        .collect();
    let Some(lod0) = &lods[0] else {
        return vec![format!("{}: mesh {}_lod0 missing", p.id, p.id)];
    };
    let rotors: Vec<_> = (0..3)
        .map(|i| mesh_named(&doc, &rotor_mesh_name(&p.id, i)))
        .collect();
    e.extend(rotor_errors(&p.id, &rotors, &buffers));
    let lod_tris = [0, 1, 2].map(|i| {
        lods[i]
            .as_ref()
            .map(|m| tris(m) + rotors[i].as_ref().map_or(0, tris))
    });
    e.extend(lod_budget_errors(
        &p.id,
        lod_tris,
        lod0_budget(p),
        lod2_max_ratio(kit, p),
    ));
    for m in lods.iter().chain(&rotors).flatten() {
        for prim in m.primitives() {
            if prim.get(&gltf::Semantic::Tangents).is_none() {
                e.push(format!(
                    "{}: {:?} primitive without TANGENT",
                    p.id,
                    m.name()
                ));
            }
            for (sem, acc) in prim.attributes() {
                if acc.data_type() != gltf::accessor::DataType::F32 {
                    e.push(format!("{}: {sem:?} is not FLOAT (D7)", p.id));
                }
            }
            if let Some(name) = prim.material().name()
                && !p.slots.contains_key(name)
            {
                e.push(format!("{}: material {name} has no manifest slot", p.id));
            }
        }
        for (pos, _) in positions(m, &buffers) {
            if !pos.iter().all(|c| c.is_finite()) {
                e.push(format!("{}: non-finite position", p.id));
                break;
            }
        }
    }
    let lod0_parts: Vec<_> = std::iter::once(lod0).chain(&rotors[0]).collect();
    e.extend(check_texel_density(kit, p, &lod0_parts, &buffers));
    if p.kind == PieceKind::Tree {
        for (i, m) in lods.iter().enumerate() {
            if let Some(m) = m {
                e.extend(wind_errors(
                    &p.id,
                    &format!("{}_lod{i}", p.id),
                    &wind_verts(p, m, &buffers),
                ));
            }
        }
    }
    for (i, m) in lods.iter().enumerate() {
        if let Some(m) = m {
            e.extend(wind_slot_errors(&p.id, i, &wind_prims(p, m, &buffers)));
        }
    }
    if p.id.starts_with("car_") || p.id.starts_with("train_") {
        for (i, m) in lods.iter().enumerate() {
            if let Some(m) = m {
                e.extend(paint_mask_errors(
                    &p.id,
                    &format!("{}_lod{i}", p.id),
                    &paint_prims(m, &buffers),
                ));
            }
        }
    }
    if !doc
        .nodes()
        .any(|n| n.name() == Some(&format!("{}_collider", p.id)))
    {
        e.push(format!("{}: node {}_collider missing (D14)", p.id, p.id));
    }
    let mouth = (p.kind == PieceKind::Base)
        .then(|| mouth_of_base(&p.id))
        .flatten();
    if let Some(m) = &mouth {
        e.extend(mouth_base_errors(kit, p, m, &lods, &buffers));
    }
    let coast = (p.kind == PieceKind::Base && mouth.is_none())
        .then(|| coast_of_base(&p.id))
        .flatten();
    if let Some(coast) = &coast {
        let rings: Vec<_> = lods
            .iter()
            .flatten()
            .map(|m| border_ring_any(&positions(m, &buffers)))
            .collect();
        let flat: Vec<Vec<[f32; 2]>> = rings
            .iter()
            .map(|r| r.iter().map(|(q, _)| [q[0], q[2]]).collect())
            .collect();
        e.extend(ring_errors(&p.id, &flat, false));
        for r in &rings {
            e.extend(coast_ring_errors(&p.id, coast, r).0);
        }
        for (i, m) in lods.iter().enumerate() {
            if let Some(m) = m {
                let (_, err) = coast_profile_errors(
                    &format!("{}_lod{i}", p.id),
                    coast,
                    &triangles(m, &buffers),
                );
                e.extend(err);
            }
        }
        let min_y = positions(lod0, &buffers)
            .iter()
            .map(|(p, _)| p[1])
            .fold(f32::MAX, f32::min);
        if (min_y + hex_core::LEVEL_STEP).abs() > 0.005 {
            e.push(format!(
                "{}: skirt bottom {min_y} != -{}",
                p.id,
                hex_core::LEVEL_STEP
            ));
        }
    }
    let level = if p.kind == PieceKind::Base && coast.is_none() && mouth.is_none() {
        base_ring_level(&p.id, &positions(lod0, &buffers)).unwrap_or_else(|err| {
            e.push(err);
            0.0
        })
    } else {
        0.0
    };
    if p.kind.touches_border() && coast.is_none() && mouth.is_none() {
        let rings: Vec<_> = lods
            .iter()
            .flatten()
            .map(|m| border_ring_at(&positions(m, &buffers), level))
            .collect();
        let cuts: BTreeMap<u64, f32> = lane_sockets(kit, p)
            .into_iter()
            .filter_map(|(k, (_, c))| Some((k, c? as f32)))
            .collect();
        let lane_cut = p.kind == PieceKind::Base && !cuts.is_empty();
        e.extend(ring_errors(
            &p.id,
            &rings,
            p.kind == PieceKind::Base && !lane_cut,
        ));
        if lane_cut && let Some(ring) = rings.first() {
            e.extend(cut_ring_errors(&p.id, ring, &cuts));
        }
    }
    if p.kind == PieceKind::Base && coast.is_none() && mouth.is_none() {
        let v = positions(lod0, &buffers);
        if let Some((p_bad, n_bad)) = v.iter().find(|(q, n)| {
            dist_to_border(q[0], q[2]).abs() < 1e-3
                && (q[1] - level).abs() < 1e-3
                && n[1] > 0.5
                && n[1] < 0.999
        }) {
            e.push(format!(
                "{}: cap border normal {n_bad:?} at {p_bad:?} is not +Y (seam shading)",
                p.id
            ));
        }
        let min_y = v.iter().map(|(p, _)| p[1]).fold(f32::MAX, f32::min);
        if (min_y + hex_core::LEVEL_STEP).abs() > 0.005 {
            e.push(format!(
                "{}: skirt bottom {min_y} != -{}",
                p.id,
                hex_core::LEVEL_STEP
            ));
        }
    }
    let world = world_matrices(&doc);
    for node in doc
        .nodes()
        .filter(|n| n.name().is_some_and(|s| s.starts_with("socket-lane-")))
    {
        let extras: serde_json::Value = node
            .extras()
            .as_ref()
            .and_then(|r| serde_json::from_str(r.get()).ok())
            .unwrap_or_default();
        let t = world[node.index()][3];
        e.extend(socket_errors(
            &p.id,
            node.name().unwrap(),
            &extras,
            [t[0], t[1], t[2]],
        ));
    }
    e
}

/// Cross-section errors for one edge piece, given each LOD's (y, z) section at x = APOTHEM.
fn section_errors(id: &str, secs: &[Vec<(i64, i64)>]) -> Vec<String> {
    let Some(sec) = secs.first() else {
        return vec![];
    };
    let mut mirrored: Vec<_> = sec.iter().map(|&(y, z)| (y, -z)).collect();
    mirrored.sort();
    let mut e = vec![];
    if sec.is_empty() {
        e.push(format!("{id}: no vertices on the edge-0 border"));
    } else if sec != &mirrored {
        e.push(format!("{id}: edge cross-section is not mirror-symmetric"));
    }
    if secs.iter().any(|s| s != sec) {
        e.push(format!(
            "{id}: edge cross-section differs across LODs (LOD seam)"
        ));
    }
    e
}

/// Road edge pieces: the cross-section at the edge-0 border is mirror-symmetric about the lane
/// center (z = 0 for offset 0), so a 180°-rotated neighbour's section coincides, and it is the
/// same in every LOD, so neighbours at different LODs meet (Review Focus 2).
fn check_edge_profile(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    if p.kind != PieceKind::Edge {
        return vec![];
    }
    let secs = lod_sections(kit, p, APOTHEM);
    section_errors(&p.id, &secs)
}

/// Each LOD's cross-section at runtime x = `x`: the (y, z) of every vertex on that plane, in 0.1 mm
/// units, sorted and deduplicated.
fn lod_sections(kit: &Kit, p: &PieceEntry, x: f32) -> Vec<Vec<(i64, i64)>> {
    lane_sections(kit, p, x, 0)
}

/// Like [`lod_sections`], after rotating the piece so its edge `lane` lands on edge 0 (the inverse
/// of the runtime placement rotation that takes edge 0 to edge `lane`).
fn lane_sections(kit: &Kit, p: &PieceEntry, x: f32, lane: u8) -> Vec<Vec<(i64, i64)>> {
    let Ok((doc, buffers, _)) = plain(kit, p) else {
        return vec![];
    };
    let (s, c) = (60.0 * lane as f32).to_radians().sin_cos();
    (0..3)
        .filter_map(|i| mesh_named(&doc, &format!("{}_lod{i}", p.id)))
        .map(|m| {
            let mut sec: Vec<(i64, i64)> = positions(&m, &buffers)
                .iter()
                .map(|(q, _)| [q[0] * c + q[2] * s, q[1], -q[0] * s + q[2] * c])
                .filter(|q| (q[0] - x).abs() < 1e-3)
                .map(|q| ((q[1] * 1e4).round() as i64, (q[2] * 1e4).round() as i64))
                .collect();
            sec.sort();
            sec.dedup();
            sec
        })
        .collect()
}

/// The lane edges of a centre piece: the edges with a lane on the tile (`data/tiles`) whose
/// `center` names it. A piece no tile names keeps the straight pair, edges 0 and 3.
fn center_lanes(id: &str) -> Vec<u8> {
    let catalog = hex_core::TileCatalog::builtin().expect("builtin tile catalog");
    catalog
        .tiles()
        .iter()
        .find(|t| t.center.as_ref().is_some_and(|c| c.as_str() == id))
        .map(|t| {
            (0..6u8)
                .filter(|&e| t.edges[e as usize].lane.is_some())
                .collect()
        })
        .unwrap_or_else(|| vec![0, 3])
}

/// Same section within one 0.1 mm unit per coordinate (absorbs the f32 lane rotation).
fn same_section(a: &[(i64, i64)], b: &[(i64, i64)]) -> bool {
    let near = |p: &(i64, i64), q: &(i64, i64)| (p.0 - q.0).abs() <= 1 && (p.1 - q.1).abs() <= 1;
    a.len() == b.len()
        && a.iter().all(|p| b.iter().any(|q| near(p, q)))
        && b.iter().all(|q| a.iter().any(|p| near(p, q)))
}

/// Road centre/edge joint. `center[i][j]` is LOD i's section at lane `lanes[j]`, rotated onto
/// edge 0, and `edge[i]` the edge piece's LOD i section at x = +6. Each lane meets an edge piece
/// rotated onto it, so every lane section must equal the edge section (for lane 3 this is the
/// x = -6 section mirrored, z -> -z).
fn joint_errors(
    id: &str,
    lanes: &[u8],
    center: &[Vec<Vec<(i64, i64)>>],
    edge: &[Vec<(i64, i64)>],
) -> Vec<String> {
    let mut e = vec![];
    if center.len() != edge.len() {
        e.push(format!(
            "{id}: LOD count {} differs from the edge piece's {}",
            center.len(),
            edge.len()
        ));
    }
    for (i, (secs, want)) in center.iter().zip(edge).enumerate() {
        for (lane, sec) in lanes.iter().zip(secs) {
            if !same_section(sec, want) {
                e.push(format!(
                    "{id}: LOD{i} section at lane {lane} differs from the edge piece's"
                ));
            }
        }
    }
    e
}

/// The edges a piece joins a network at, from its `socket-lane-*` nodes' `extras.edge`.
fn lane_socket_edges(kit: &Kit, p: &PieceEntry) -> BTreeSet<u64> {
    let Ok((doc, _, _)) = plain(kit, p) else {
        return BTreeSet::new();
    };
    doc.nodes()
        .filter(|n| n.name().is_some_and(|s| s.starts_with("socket-lane-")))
        .filter_map(|n| {
            let v: serde_json::Value = serde_json::from_str(n.extras().as_ref()?.get()).ok()?;
            v["edge"].as_u64()
        })
        .collect()
}

/// A piece's `socket-lane-*` extras by edge: (network kind, `cut` half-width). A base that
/// declares lanes with a `cut` is lane-cut (the river bases): its border ring leaves out each
/// lane's channel, and it owns the joint with the edge piece there.
fn lane_sockets(kit: &Kit, p: &PieceEntry) -> BTreeMap<u64, (String, Option<f64>)> {
    let Ok((doc, _, _)) = plain(kit, p) else {
        return BTreeMap::new();
    };
    doc.nodes()
        .filter(|n| n.name().is_some_and(|s| s.starts_with("socket-lane-")))
        .filter_map(|n| {
            let v: serde_json::Value = serde_json::from_str(n.extras().as_ref()?.get()).ok()?;
            let kind = v["kind"].as_str().unwrap_or("road").to_string();
            Some((v["edge"].as_u64()?, (kind, v["cut"].as_f64())))
        })
        .collect()
}

/// The network an edge piece carries (`road`, `river`), from its lane socket.
fn edge_kind(kit: &Kit, p: &PieceEntry) -> Option<String> {
    lane_sockets(kit, p).into_values().next().map(|(k, _)| k)
}

/// Where a centre meets an edge piece: the edge piece's inner end, its smallest runtime x on LOD0
/// (6 for the road stub, APOTHEM - 1 for the river mouth).
fn joint_x(kit: &Kit, edge: &PieceEntry) -> f32 {
    let Ok((doc, buffers, _)) = plain(kit, edge) else {
        return 6.0;
    };
    mesh_named(&doc, &format!("{}_lod0", edge.id))
        .map(|m| {
            positions(&m, &buffers)
                .iter()
                .map(|(q, _)| q[0])
                .fold(f32::MAX, f32::min)
        })
        .unwrap_or(6.0)
}

/// The tile whose `center` names `id`.
fn center_tile(id: &str) -> Option<hex_core::TileDef> {
    let catalog = hex_core::TileCatalog::builtin().expect("builtin tile catalog");
    catalog
        .tiles()
        .iter()
        .find(|t| t.center.as_ref().is_some_and(|c| c.as_str() == id))
        .cloned()
}

/// The network kind of each lane of the tile whose `center` names `id` (`road`, `river`).
fn tile_lane_kinds(id: &str) -> BTreeMap<u8, String> {
    center_tile(id)
        .map(|t| {
            (0..6u8)
                .filter(|&e| t.edges[e as usize].lane.is_some())
                .map(|e| (e, format!("{:?}", t.edges[e as usize].kind).to_lowercase()))
                .collect()
        })
        .unwrap_or_default()
}

/// LOD2 ceiling of a lane-cut base (gates.md, LOD floor for border pieces): its ring and one-row
/// skirt fix about 300 tris, and the river bend's arc needs 16 rows to stay inside the water
/// contract tolerance at LOD2 (owner, 2026-10-13).
const LANE_CUT_LOD2_MAX_RATIO: f32 = 0.20;

/// LOD0 ceiling of the river dressing props, whose reed clumps and cattails are modelled for close-up
/// and eye-level views: owner approval 2026-10-14, "Raise to 2,500". Every other prop keeps
/// `PieceKind::Prop`'s 1,500, and the LOD1/LOD2 ratio ceilings are unchanged.
/// (river_road_bridge_center carries the same clumps as a `center` under its kind's 4,000.)
const RIVER_DRESSING_LOD0_MAX: usize = 2_500;
const RIVER_DRESSING_PROPS: [&str; 5] = [
    "river_straight_center",
    "river_bend_center",
    "river_spring_center",
    "river_mouth_center",
    "river_mouth_l_center",
];

/// A piece's LOD0 triangle ceiling: its kind's, or `RIVER_DRESSING_LOD0_MAX` for a river dressing prop.
fn lod0_budget(p: &PieceEntry) -> usize {
    if p.kind == PieceKind::Prop && RIVER_DRESSING_PROPS.contains(&p.id.as_str()) {
        RIVER_DRESSING_LOD0_MAX
    } else {
        p.kind.lod0_budget()
    }
}

/// LOD triangle budget errors. `lod_tris[i]` is LOD i's body plus its rotor (`None`: the LOD mesh is
/// missing).
fn lod_budget_errors(
    id: &str,
    lod_tris: [Option<usize>; 3],
    lod0_max: usize,
    lod2_max_ratio: f32,
) -> Vec<String> {
    let mut e = vec![];
    let Some(t0) = lod_tris[0] else {
        return vec![format!("{id}: mesh {id}_lod0 missing")];
    };
    if t0 > lod0_max {
        e.push(format!("{id}: LOD0 {t0} tris > {lod0_max}"));
    }
    for (i, max) in [(1, LOD1_MAX_RATIO), (2, lod2_max_ratio)] {
        match lod_tris[i] {
            None => e.push(format!("{id}: mesh {id}_lod{i} missing")),
            Some(t) if t as f32 > max * t0 as f32 => {
                e.push(format!("{id}: LOD{i} {t} tris > {max} x LOD0 {t0}"))
            }
            _ => {}
        }
    }
    e
}

/// Largest distance (m) a rotor mesh's vertex centroid may sit off its local X axis: further off,
/// it wobbles as it turns.
const ROTOR_AXLE_TOL: f32 = 0.01;

/// A rotor is in every LOD or none, and each is centred on its local X axis.
fn rotor_errors(
    id: &str,
    rotors: &[Option<gltf::Mesh>],
    buffers: &[gltf::buffer::Data],
) -> Vec<String> {
    if rotors.iter().all(Option::is_none) {
        return vec![];
    }
    let mut e = vec![];
    for (i, r) in rotors.iter().enumerate() {
        match r {
            None => e.push(format!(
                "{id}: mesh {} missing (a rotor is in every LOD)",
                rotor_mesh_name(id, i)
            )),
            Some(m) => {
                let pos: Vec<[f32; 3]> =
                    positions(m, buffers).into_iter().map(|(p, _)| p).collect();
                let off = axle_offset(&pos);
                if off > ROTOR_AXLE_TOL {
                    e.push(format!(
                        "{id}: {} is {off:.3} m off its local X axis (> {ROTOR_AXLE_TOL})",
                        rotor_mesh_name(id, i)
                    ));
                }
            }
        }
    }
    e
}

/// Distance of the vertex centroid from the X axis, in the YZ plane.
fn axle_offset(pos: &[[f32; 3]]) -> f32 {
    let n = pos.len().max(1) as f32;
    let (y, z) = pos
        .iter()
        .fold((0.0, 0.0), |(y, z), p| (y + p[1] / n, z + p[2] / n));
    y.hypot(z)
}

/// LOD2's ratio ceiling: `LANE_CUT_LOD2_MAX_RATIO` for a lane-cut base, `LOD2_MAX_RATIO` otherwise.
fn lod2_max_ratio(kit: &Kit, p: &PieceEntry) -> f32 {
    if p.kind == PieceKind::Base && !cut_lanes(kit, p).is_empty() {
        LANE_CUT_LOD2_MAX_RATIO
    } else if p.kind == PieceKind::Base
        && matches!(coast_of_base(&p.id), Some(Coastline::Arc { .. }))
    {
        COAST_ARC_LOD2_MAX_RATIO
    } else {
        LOD2_MAX_RATIO
    }
}

/// The lanes a lane-cut base cuts (its lane sockets with a `cut`).
fn cut_lanes(kit: &Kit, base: &PieceEntry) -> Vec<u8> {
    lane_sockets(kit, base)
        .into_iter()
        .filter(|(_, (_, cut))| cut.is_some())
        .map(|(k, _)| k as u8)
        .collect()
}

/// The lanes the base of the tile whose `center` names `id` cuts: the lane-cut base owns the
/// joint there, so the centre is not checked at those lanes.
fn base_cut_lanes(kit: &Kit, id: &str) -> BTreeSet<u8> {
    let Some(t) = center_tile(id) else {
        return BTreeSet::new();
    };
    kit.manifest
        .pieces
        .iter()
        .find(|p| p.id == t.base.as_str() && p.kind == PieceKind::Base)
        .map(|b| cut_lanes(kit, b).into_iter().collect())
        .unwrap_or_default()
}

/// A piece against every edge piece of the same network kind at each of `lanes`: the section at
/// the joint (the edge piece's inner end, `joint_x`) on the lane, rotated onto edge 0, must equal
/// the edge piece's section there in every LOD. A lane's kind comes from the piece's own lane
/// socket, else from the tile that names it as `center`, else it is a road.
fn lane_joint_errors(kit: &Kit, c: &PieceEntry, lanes: &[u8]) -> Vec<String> {
    let declared = lane_sockets(kit, c);
    let tile = tile_lane_kinds(&c.id);
    let mut e = vec![];
    for &k in lanes {
        let kind = declared
            .get(&(k as u64))
            .map(|(s, _)| s.clone())
            .or_else(|| tile.get(&k).cloned())
            .unwrap_or_else(|| "road".to_string());
        for edge in
            kit.manifest.pieces.iter().filter(|p| {
                p.kind == PieceKind::Edge && edge_kind(kit, p).as_deref() == Some(&kind)
            })
        {
            let x = joint_x(kit, edge);
            let secs = lane_sections(kit, c, x, k);
            if secs.len() != 3 || secs.iter().any(Vec::is_empty) {
                e.push(format!(
                    "{}: a LOD has no vertices at the lane {k} joint (x = {x})",
                    c.id
                ));
            }
            let center: Vec<Vec<_>> = secs.into_iter().map(|s| vec![s]).collect();
            let id = format!("{} / {}", c.id, edge.id);
            e.extend(joint_errors(
                &id,
                &[k],
                &center,
                &lod_sections(kit, edge, x),
            ));
        }
    }
    e
}

/// Runtime border-ring lattice (`hexgeo.border_ring(16)`): (point, edge, offset from the edge
/// midpoint along the edge).
fn lattice_ring() -> Vec<([f32; 2], u64, f32)> {
    let r = 16.0f32;
    let corner = |i: i32| {
        let a = (30.0 + 60.0 * i as f32).to_radians();
        [r * a.cos(), r * a.sin()]
    };
    let mut out = vec![];
    for k in 0..6 {
        let (a, b) = (corner(k - 1), corner(k));
        for s in 0..16 {
            let t = s as f32 / 16.0;
            out.push((
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
                k as u64,
                (t - 0.5) * r,
            ));
        }
    }
    out
}

/// A lane-cut base's ring: at least 24 vertices; every lattice vertex present except inside a cut
/// lane's channel (|offset| < cut); nothing else on the ring but the two bank-top points of each
/// cut (|offset| = cut).
fn cut_ring_errors(id: &str, ring: &[[f32; 2]], cuts: &BTreeMap<u64, f32>) -> Vec<String> {
    let mut e = vec![];
    if ring.len() < 24 {
        e.push(format!("{id}: border ring has {} vertices", ring.len()));
    }
    let lattice = lattice_ring();
    let cut = |k: u64, o: f32| cuts.get(&k).is_some_and(|&c| o.abs() < c - 1e-3);
    for (p, k, o) in &lattice {
        if !cut(*k, *o) && !ring.iter().any(|q| near(*p, *q, RING_TOL)) {
            e.push(format!("{id}: lattice ring vertex {p:?} missing"));
        }
    }
    let banks: Vec<[f32; 2]> = cuts
        .iter()
        .flat_map(|(&k, &c)| {
            let a = (60.0 * k as f32).to_radians();
            let m = [APOTHEM * a.cos(), APOTHEM * a.sin()];
            [-c, c].map(|o| [m[0] + a.sin() * o, m[1] - a.cos() * o])
        })
        .collect();
    for q in ring {
        if !lattice.iter().any(|(p, _, _)| near(*p, *q, RING_TOL))
            && !banks.iter().any(|b| near(*b, *q, RING_TOL))
        {
            e.push(format!(
                "{id}: ring vertex {q:?} is neither lattice nor bank top"
            ));
        }
    }
    e
}

/// The coastline (rotation 0) of the coast tile whose `base` names `id`: a coast base is the bed
/// itself, held to the engine's analytic profile (docs/kit/water-contract.md §3, §5).
fn coast_of_base(id: &str) -> Option<Coastline> {
    let catalog = hex_core::TileCatalog::builtin().expect("builtin tile catalog");
    catalog
        .tiles()
        .iter()
        .filter(|t| t.base.as_str() == id)
        .find_map(|t| Coastline::of(t, 0))
}

/// The mouth (rotation 0) of the river-mouth tile whose `base` names `id`: the base is the bed,
/// held to the engine's analytic mouth profile (water contract §3a).
fn mouth_of_base(id: &str) -> Option<Mouth> {
    let catalog = hex_core::TileCatalog::builtin().expect("builtin tile catalog");
    catalog
        .tiles()
        .iter()
        .filter(|t| t.base.as_str() == id)
        .find_map(|t| Mouth::of(t, 0))
}

/// Analytic mouth bed normal at runtime (x, z), by central differences.
fn mouth_normal(m: &Mouth, x: f32, z: f32) -> Vec3 {
    let e = 1e-3;
    let h = |x: f32, z: f32| m.height(Vec2::new(x, z));
    Vec3::new(
        -(h(x + e, z) - h(x - e, z)) / (2.0 * e),
        1.0,
        -(h(x, z + e) - h(x, z - e)) / (2.0 * e),
    )
    .normalize()
}

/// A river-mouth base (water contract §3a, §6): the ring identical across LODs; every lattice
/// vertex present except inside the River cut, and nothing else on the ring but the cut's bank
/// tops and beach-edge samples; every ring vertex on the mouth bed within 1 mm, with the analytic
/// normal off the River edge; every LOD's up-facing faces on the bed (0.03 m where the water is
/// under 2.5 m deep or the bed is below y = -1.0, 0.10 m elsewhere, upward only on the sea floor);
/// the skirt down to the base.
fn mouth_base_errors(
    kit: &Kit,
    p: &PieceEntry,
    m: &Mouth,
    lods: &[Option<gltf::Mesh>],
    buffers: &[gltf::buffer::Data],
) -> Vec<String> {
    let id = &p.id;
    let mut e = vec![];
    let rings: Vec<_> = lods
        .iter()
        .flatten()
        .map(|g| border_ring_any(&positions(g, buffers)))
        .collect();
    let flat: Vec<Vec<[f32; 2]>> = rings
        .iter()
        .map(|r| r.iter().map(|(q, _)| [q[0], q[2]]).collect())
        .collect();
    e.extend(ring_errors(id, &flat, false));
    let cuts: BTreeMap<u64, f32> = lane_sockets(kit, p)
        .into_iter()
        .filter_map(|(k, (_, c))| Some((k, c? as f32)))
        .collect();
    if cuts.is_empty() {
        e.push(format!("{id}: no cut lane socket on its River edge"));
    }
    let lattice = lattice_ring();
    let in_cut = |k: u64, o: f32| cuts.get(&k).is_some_and(|&c| o.abs() < c - 1e-3);
    let banks: Vec<[f32; 2]> = cuts
        .iter()
        .flat_map(|(&k, &c)| {
            let a = (60.0 * k as f32).to_radians();
            let mid = [APOTHEM * a.cos(), APOTHEM * a.sin()];
            [-c, c].map(|o| [mid[0] + a.sin() * o, mid[1] - a.cos() * o])
        })
        .collect();
    let edge_of = |q: [f32; 2]| {
        (0..6u64)
            .max_by(|&a, &b| {
                let d = |k: u64| {
                    let t = (60.0 * k as f32).to_radians();
                    q[0] * t.cos() + q[1] * t.sin()
                };
                d(a).total_cmp(&d(b))
            })
            .unwrap()
    };
    if let Some(ring) = rings.first() {
        for (q, k, o) in &lattice {
            if !in_cut(*k, *o) && !ring.iter().any(|(r, _)| near(*q, [r[0], r[2]], RING_TOL)) {
                e.push(format!("{id}: lattice ring vertex {q:?} missing"));
            }
        }
        for (q, n) in ring {
            let xz = Vec2::new(q[0], q[2]);
            let want = m.height(xz);
            if (q[1] - want).abs() > 1e-3 {
                e.push(format!(
                    "{id}: ring vertex {q:?} off the mouth bed ({want:.4})"
                ));
            }
            let flat = [q[0], q[2]];
            let k = edge_of(flat);
            if !cuts.contains_key(&k) {
                let a = mouth_normal(m, q[0], q[2]);
                if (Vec3::from(*n) - a).abs().max_element() > 1e-2 {
                    e.push(format!(
                        "{id}: ring normal {n:?} at {q:?} is not the bed normal {a} (seam shading)"
                    ));
                }
            }
            let lattice_pt = lattice.iter().any(|(l, _, _)| near(*l, flat, RING_TOL));
            let bank = banks.iter().any(|b| near(*b, flat, RING_TOL));
            let beach = m.coast.seaward(xz).abs() < 8.0 - 1e-3 && !cuts.contains_key(&k);
            if !lattice_pt && !bank && !beach {
                e.push(format!(
                    "{id}: ring vertex {q:?} is neither lattice, bank top nor on a beach edge"
                ));
            }
        }
    }
    for (i, g) in lods.iter().enumerate() {
        let Some(g) = g else { continue };
        let mut n_err = 0;
        for tri in triangles(g, buffers) {
            let [a, b, c] = tri.map(Vec3::from);
            if (b - a).cross(c - a).normalize_or_zero().y < 0.2 {
                continue;
            }
            const N: usize = 4;
            for u in 0..=N {
                for v in 0..=N - u {
                    let q = a + (b - a) * (u as f32 / N as f32) + (c - a) * (v as f32 / N as f32);
                    let xz = Vec2::new(q.x, q.z);
                    let want = m.height(xz);
                    let s = m.coast.seaward(xz);
                    let tol = if want < -1.0 && want > -3.9 {
                        COAST_TOL_NEAR
                    } else {
                        COAST_TOL_FAR
                    };
                    let err = q.y - want;
                    let below = s >= 8.0 && err < -1e-3;
                    if (err.abs() > tol || below) && n_err < 5 {
                        n_err += 1;
                        e.push(format!(
                            "{id}_lod{i}: bed at {q} (s = {s:.3}) is {err:.4} m off the mouth bed ({want:.4})"
                        ));
                    }
                }
            }
        }
    }
    if let Some(Some(g)) = lods.first() {
        let min_y = positions(g, buffers)
            .iter()
            .map(|(q, _)| q[1])
            .fold(f32::MAX, f32::min);
        if (min_y + hex_core::LEVEL_STEP).abs() > 0.005 {
            e.push(format!(
                "{id}: skirt bottom {min_y} != -{}",
                hex_core::LEVEL_STEP
            ));
        }
    }
    e
}

/// LOD2 ceiling of the coast-corner bases (headland, bay), the lane-cut ceiling extended: the
/// ring and one-row skirt fix about 256 tris, and the arc rows on the smoothstep drop (slope up to
/// 0.67 m/m) need about 300 more for the chord sag to stay inside the 0.03 m tolerance at LOD2
/// (`beach_outer_base` 562 / 3,148 = 17.9%, `beach_inner_base` 590 / 3,244 = 18.2%).
const COAST_ARC_LOD2_MAX_RATIO: f32 = 0.20;

/// The coast profile's tolerances (contract §3): 0.03 m where the water is under 2.5 m deep and on
/// the beach above it (s < 7.36), 0.10 m beyond, upward only on the ocean floor (s >= 8).
const COAST_SHALLOW_S: f32 = 7.36;
const COAST_TOL_NEAR: f32 = 0.03;
const COAST_TOL_FAR: f32 = 0.10;

/// Analytic bed normal at runtime (x, z): normalize(-h'(s) grad s, 1).
fn coast_normal(coast: &Coastline, x: f32, z: f32) -> Vec3 {
    let e = 1e-3;
    let s = |x: f32, z: f32| coast.seaward(Vec2::new(x, z));
    let g = Vec2::new(
        (s(x + e, z) - s(x - e, z)) / (2.0 * e),
        (s(x, z + e) - s(x, z - e)) / (2.0 * e),
    );
    let s0 = s(x, z);
    let dh = (coast_height(s0 + e) - coast_height(s0 - e)) / (2.0 * e);
    Vec3::new(-dh * g.x, 1.0, -dh * g.y).normalize()
}

/// Up-facing cap vertices on the hex border at any height (x, y, z) with their normals, split-normal
/// duplicates removed.
fn border_ring_any(v: &[([f32; 3], [f32; 3])]) -> Vec<([f32; 3], [f32; 3])> {
    let mut ring: Vec<([f32; 3], [f32; 3])> = vec![];
    for (p, n) in v {
        if dist_to_border(p[0], p[2]).abs() < 1e-3
            && n[1] > 0.5
            && !ring
                .iter()
                .any(|(q, _)| near([q[0], q[2]], [p[0], p[2]], RING_TOL))
        {
            ring.push((*p, *n));
        }
    }
    ring
}

/// A coast base's ring: every `border_ring(16)` lattice vertex present; any other vertex only on a
/// beach edge (|s| < 8), with both beach edges carrying the same s samples (so every legal beach
/// seam meets vertex for vertex); every vertex on the profile within 1 mm and its normal the
/// analytic bed normal (both tiles of a seam shade alike). Returns the extra s samples.
fn coast_ring_errors(
    id: &str,
    coast: &Coastline,
    ring: &[([f32; 3], [f32; 3])],
) -> (Vec<String>, Vec<f32>) {
    let mut e = vec![];
    let lattice = lattice_ring();
    for (p, _, _) in &lattice {
        if !ring.iter().any(|(q, _)| near(*p, [q[0], q[2]], RING_TOL)) {
            e.push(format!("{id}: lattice ring vertex {p:?} missing"));
        }
    }
    let mut extra: BTreeMap<u64, Vec<f32>> = BTreeMap::new();
    for (q, n) in ring {
        let s = coast.seaward(Vec2::new(q[0], q[2]));
        let want = coast_height(s);
        if (q[1] - want).abs() > 1e-3 {
            e.push(format!(
                "{id}: ring vertex {q:?} at s = {s:.3} off the coast profile ({want:.4})"
            ));
        }
        let a = coast_normal(coast, q[0], q[2]);
        if (Vec3::from(*n) - a).abs().max_element() > 1e-2 {
            e.push(format!(
                "{id}: ring normal {n:?} at {q:?} is not the bed normal {a} (seam shading)"
            ));
        }
        if lattice
            .iter()
            .any(|(p, _, _)| near(*p, [q[0], q[2]], RING_TOL))
        {
            continue;
        }
        if s.abs() >= 8.0 - 1e-3 {
            e.push(format!(
                "{id}: ring vertex {q:?} is neither lattice nor on a beach edge (s = {s:.3})"
            ));
            continue;
        }
        let k = (0..6u64)
            .max_by(|&a, &b| {
                let d = |k: u64| {
                    let t = (60.0 * k as f32).to_radians();
                    q[0] * t.cos() + q[2] * t.sin()
                };
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        extra.entry(k).or_default().push(s);
    }
    let mut sets: Vec<Vec<f32>> = extra.into_values().collect();
    for s in &mut sets {
        s.sort_by(f32::total_cmp);
    }
    let same = |a: &[f32], b: &[f32]| {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    };
    if sets.len() > 2 || sets.windows(2).any(|w| !same(&w[0], &w[1])) {
        e.push(format!(
            "{id}: beach edges carry different extra ring samples {sets:?}"
        ));
    }
    (e, sets.into_iter().next().unwrap_or_default())
}

/// Triangles of a mesh, runtime positions.
fn triangles(m: &gltf::Mesh, buffers: &[gltf::buffer::Data]) -> Vec<[[f32; 3]; 3]> {
    let mut out = vec![];
    for prim in m.primitives() {
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let pos: Vec<[f32; 3]> = r.read_positions().map(|i| i.collect()).unwrap_or_default();
        let Some(idx) = r.read_indices() else {
            continue;
        };
        let idx: Vec<u32> = idx.into_u32().collect();
        for t in idx.as_chunks::<3>().0 {
            out.push([0, 1, 2].map(|i| pos[t[i] as usize]));
        }
    }
    out
}

/// The coast profile error of one LOD's up-facing faces, sampled at vertices, edge midpoints and
/// interior points: (dry s < 0, wet 0 <= s < 7.36, deep s >= 7.36) maxima and the errors over the
/// contract tolerance.
fn coast_profile_errors(
    id: &str,
    coast: &Coastline,
    tris: &[[[f32; 3]; 3]],
) -> ([f32; 3], Vec<String>) {
    let mut max = [0f32; 3];
    let mut e = vec![];
    const N: usize = 4;
    for t in tris {
        let [a, b, c] = t.map(Vec3::from);
        if (b - a).cross(c - a).normalize_or_zero().y < 0.2 {
            continue;
        }
        for i in 0..=N {
            for j in 0..=N - i {
                let q = a + (b - a) * (i as f32 / N as f32) + (c - a) * (j as f32 / N as f32);
                let s = coast.seaward(Vec2::new(q.x, q.z));
                let want = coast_height(s);
                let err = (q.y - want).abs();
                let (k, tol) = if s < 0.0 {
                    (0, COAST_TOL_NEAR)
                } else if s < COAST_SHALLOW_S {
                    (1, COAST_TOL_NEAR)
                } else {
                    (2, COAST_TOL_FAR)
                };
                max[k] = max[k].max(err);
                let below = s >= 8.0 && q.y < want - 1e-3;
                if (err > tol || below) && e.len() < 5 {
                    e.push(format!(
                        "{id}: bed at {q} (s = {s:.3}) is {:.4} m off the coast profile ({want:.4})",
                        q.y - want
                    ));
                }
            }
        }
    }
    (max, e)
}

/// The lane edges a centre piece is checked at: the tile's (`center_lanes`), or, while no tile
/// names the piece yet (the P3 rail pieces ahead of their Rail socket), the edges its own
/// `socket-lane-*` nodes declare. A piece with neither keeps the straight pair, edges 0 and 3.
fn joint_lanes(kit: &Kit, c: &PieceEntry) -> Vec<u8> {
    let declared = lane_socket_edges(kit, c);
    if center_tile(&c.id).is_none() && !declared.is_empty() {
        declared.iter().map(|&k| k as u8).collect()
    } else {
        center_lanes(&c.id)
    }
}

/// Every centre piece against every edge piece of the same network kind, at each of its lane
/// edges (`joint_lanes`: the tile that names it as `center`, else its declared lane sockets) that
/// the tile's base does not cut. A centre that declares `socket-lane-*` nodes (house_pad's
/// driveway) must declare exactly those lane edges. Every lane-cut base is checked at its cut lanes.
fn check_center_edge_joint(kit: &Kit) -> Vec<String> {
    let mut e = vec![];
    for c in &kit.manifest.pieces {
        if c.kind == PieceKind::Base {
            e.extend(lane_joint_errors(kit, c, &cut_lanes(kit, c)));
            continue;
        }
        if c.kind != PieceKind::Center {
            continue;
        }
        let owned = base_cut_lanes(kit, &c.id);
        let lanes: Vec<u8> = joint_lanes(kit, c)
            .into_iter()
            .filter(|k| !owned.contains(k))
            .collect();
        let declared = lane_socket_edges(kit, c);
        let want: BTreeSet<u64> = lanes.iter().map(|&k| k as u64).collect();
        if !declared.is_empty() && declared != want {
            e.push(format!(
                "{}: socket-lane edges {declared:?} differ from the tile's lane edges {want:?}",
                c.id
            ));
        }
        e.extend(lane_joint_errors(kit, c, &lanes));
    }
    e
}

fn piece_stems(p: &PieceEntry) -> Vec<(Channel, String)> {
    p.maps.iter().map(|(c, s)| (*c, s.clone())).collect()
}

fn tileable_sets(p: &PieceEntry) -> BTreeSet<String> {
    p.slots
        .values()
        .filter_map(|s| s.tileable.clone())
        .collect()
}

/// The piece's KTX2 budget class: `budgets.class` in the `compress-report.json` beside its
/// provenance (owner ruling 2026-10-13). The report's `ktxMax` must match this crate's value.
fn budget_class(p: &PieceEntry) -> Result<BudgetClass, String> {
    let path = repo()
        .join(&p.provenance)
        .with_file_name("compress-report.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("{}: compress report {}: {e}", p.id, path.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: compress report: {e}", p.id))?;
    let name = v["budgets"]["class"].as_str().unwrap_or_default();
    let class = BudgetClass::parse(name).ok_or_else(|| {
        format!(
            "{}: unknown budget class {name:?} in {}",
            p.id,
            path.display()
        )
    })?;
    if v["budgets"]["ktxMax"].as_u64() != Some(class.ktx_max()) {
        return Err(format!(
            "{}: compress report ktxMax {} != {} for class {name} (re-run kit-compress)",
            p.id,
            v["budgets"]["ktxMax"],
            class.ktx_max()
        ));
    }
    class_allowed(&p.id, p.kind, class)?;
    Ok(class)
}

/// A piece cannot declare itself into a bigger budget: `hero` is for hero and tree pieces only,
/// and `tileable` belongs to material sets, never to a piece.
fn class_allowed(id: &str, kind: PieceKind, class: BudgetClass) -> Result<(), String> {
    match (class, kind) {
        (BudgetClass::Piece, _) | (BudgetClass::Hero, PieceKind::Hero | PieceKind::Tree) => Ok(()),
        (BudgetClass::Hero, _) => Err(format!(
            "{id}: budget class hero is only for hero or tree pieces, not {kind:?}"
        )),
        (BudgetClass::Tileable, _) => Err(format!(
            "{id}: budget class tileable is for material sets, not pieces"
        )),
    }
}

/// Repo-relative paths every piece's provenance must hash: the GLB and each of its own maps in
/// BC7, ASTC and PNG. Tileable-set maps belong to the set, not the piece.
fn required_derived(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    let rel = |f: String| {
        kit.root
            .join(f)
            .strip_prefix(repo())
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut out = vec![rel(p.glb.clone())];
    for stem in p.maps.values() {
        for fam in [TextureFamily::Bc7, TextureFamily::Astc, TextureFamily::Png] {
            out.push(rel(map_file(stem, fam)));
        }
    }
    out
}

/// One map stem in every family: present, KTX2 header color space / mips / zstd, PNG fallback.
/// Returns the KTX2 bytes per family (BC7, ASTC).
fn check_stem(kit: &Kit, id: &str, ch: Channel, stem: &str, e: &mut Vec<String>) -> [u64; 2] {
    let mut bytes_by_family = [0u64; 2];
    for (fi, fam) in [TextureFamily::Bc7, TextureFamily::Astc]
        .into_iter()
        .enumerate()
    {
        let path = kit.root.join(map_file(stem, fam));
        let Ok(bytes) = std::fs::read(&path) else {
            e.push(format!("{id}: missing {}", path.display()));
            continue;
        };
        bytes_by_family[fi] = bytes.len() as u64;
        let Some(info) = ktx2_info(&bytes) else {
            e.push(format!("{id}: {} is not KTX2", path.display()));
            continue;
        };
        if ktx2_is_srgb(info.vk_format) != Some(ch.is_srgb()) {
            e.push(format!(
                "{id}: {} vkFormat {} wrong color space for {} (R6)",
                path.display(),
                info.vk_format,
                ch.as_str()
            ));
        }
        if info.levels != full_mip_count(info.width, info.height)
            || info.width.max(info.height) > 2048
        {
            e.push(format!(
                "{id}: {} has {} levels at {}x{}",
                path.display(),
                info.levels,
                info.width,
                info.height
            ));
        }
        if info.supercompression != SUPERCOMPRESSION_ZSTD {
            e.push(format!(
                "{id}: {} is not zstd-supercompressed",
                path.display()
            ));
        }
    }
    let png = kit.root.join(map_file(stem, TextureFamily::Png));
    if !png.exists() {
        e.push(format!("{id}: missing PNG fallback {}", png.display()));
    }
    bytes_by_family
}

/// Textures with an explicit per-family limit for the piece's own maps (tests inject small ones).
fn texture_errors(kit: &Kit, p: &PieceEntry, piece_max: u64) -> Vec<String> {
    let mut e = vec![];
    let mut piece = [0u64; 2];
    for (ch, stem) in piece_stems(p) {
        let b = check_stem(kit, &p.id, ch, &stem, &mut e);
        piece = [piece[0] + b[0], piece[1] + b[1]];
    }
    for (fam, bytes) in ["Bc7", "Astc"].iter().zip(piece) {
        if bytes > piece_max {
            e.push(format!("{}: {fam} maps {bytes} B > {piece_max} (D3)", p.id));
        }
    }
    let set_max = BudgetClass::Tileable.ktx_max();
    for set in tileable_sets(p) {
        let mut sum = [0u64; 2];
        for (ch, stem) in &kit.manifest.materials[&set].maps {
            let b = check_stem(kit, &p.id, *ch, stem, &mut e);
            sum = [sum[0] + b[0], sum[1] + b[1]];
        }
        for (fam, bytes) in ["Bc7", "Astc"].iter().zip(sum) {
            if bytes > set_max {
                e.push(format!(
                    "{}: tileable set {set} {fam} {bytes} B > {set_max} (D3)",
                    p.id
                ));
            }
        }
    }
    e.sort();
    e.dedup();
    e
}

/// Every tileable set in `manifest.materials`, whether or not a piece slot references it, checked
/// per family against `set_max`, so `kit-material-provenance` never stamps an unchecked set.
fn material_texture_errors(kit: &Kit, set_max: u64) -> Vec<String> {
    let mut e = vec![];
    for (set, ts) in &kit.manifest.materials {
        let id = format!("material {set}");
        let mut sum = [0u64; 2];
        for (ch, stem) in &ts.maps {
            let b = check_stem(kit, &id, *ch, stem, &mut e);
            sum = [sum[0] + b[0], sum[1] + b[1]];
        }
        for (fam, bytes) in ["Bc7", "Astc"].iter().zip(sum) {
            if bytes > set_max {
                e.push(format!(
                    "{id}: tileable set {set} {fam} {bytes} B > {set_max} (D3)"
                ));
            }
        }
    }
    e
}

fn check_textures(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    match budget_class(p) {
        Ok(class) => texture_errors(kit, p, class.ktx_max()),
        Err(err) => vec![err],
    }
}

fn sha(path: &Path) -> Option<String> {
    let b = std::fs::read(path).ok()?;
    Some(
        Sha256::digest(&b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect(),
    )
}

/// Every recorded encode, PNG included, carries the channel's transfer (R6). The PNG file has no
/// color space of its own, so this record is what the runtime's `is_srgb` must follow.
fn encode_errors(id: &str, encodes: &serde_json::Value) -> Vec<String> {
    let mut e = vec![];
    for t in encodes.as_array().into_iter().flatten() {
        let (ch, fam) = (
            t["channel"].as_str().unwrap_or_default(),
            t["family"].as_str().unwrap_or_default(),
        );
        let Some(channel) = Channel::parse(ch) else {
            e.push(format!("{id}: encode with unknown channel {ch:?}"));
            continue;
        };
        let want = if channel.is_srgb() { "srgb" } else { "linear" };
        if t["transfer"] != want {
            e.push(format!(
                "{id}: {ch} {fam} transfer {} != {want} (R6)",
                t["transfer"]
            ));
        }
        if fam != "png"
            && t["vkFormat"]
                .as_u64()
                .and_then(|vk| ktx2_is_srgb(vk as u32))
                != Some(channel.is_srgb())
        {
            e.push(format!(
                "{id}: {ch} {fam} vkFormat {} wrong color space (R6)",
                t["vkFormat"]
            ));
        }
    }
    e
}

/// The `_*.py` helpers beside each file named by any line that starts, after indentation, with
/// `import _x` or `from _x import` (nested imports included, relative ones not),
/// followed transitively from `generator`: the gate's mirror of `lib.export.generator_deps`, so a
/// provenance cannot omit a helper the generator uses. `lib/` is outside this scope (ADR 0002).
fn scanned_generator_deps(generator: &Path) -> Result<BTreeSet<String>, String> {
    let (mut seen, mut todo) = (BTreeSet::new(), vec![generator.to_path_buf()]);
    while let Some(src) = todo.pop() {
        let text = std::fs::read_to_string(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        for line in text.lines().map(str::trim_start) {
            let names: Vec<&str> = if let Some(rest) = line.strip_prefix("import ") {
                rest.split(',')
                    .filter_map(|n| n.split_whitespace().next())
                    .collect()
            } else if let Some(rest) = line.strip_prefix("from ") {
                rest.split_whitespace().next().into_iter().collect()
            } else {
                continue;
            };
            for name in names {
                let module = name.split('.').next().unwrap_or_default();
                let dep = src.with_file_name(format!("{module}.py"));
                if module.starts_with('_') && dep.is_file() && seen.insert(repo_rel(&dep)) {
                    todo.push(dep);
                }
            }
        }
    }
    Ok(seen)
}

/// Generator provenance, mandatory for every piece and set: `generator` {path, sha256}
/// and `generatorDeps` [{path, sha256}] are present, the files exist, and `generatorDeps` lists
/// exactly the helpers the generator imports. A generator or helper edited since provenance is
/// printed as a warning, not an error: owner ruling 2026-10-19, "full edits are allowed even if it
/// breaks hash" (the piece is rebuilt and re-provenanced on its next revision).
fn generator_errors(id: &str, v: &serde_json::Value) -> Vec<String> {
    let (e, stale) = generator_check(id, v);
    for s in stale {
        eprintln!("warning: {s}");
    }
    e
}

/// `generator_errors`' (errors, stale-hash warnings).
fn generator_check(id: &str, v: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let (Some(gen_rel), Some(gen_sha)) = (
        v["generator"]["path"].as_str(),
        v["generator"]["sha256"].as_str(),
    ) else {
        return (
            vec![format!("{id}: provenance lacks generator {{path, sha256}}")],
            vec![],
        );
    };
    let (mut e, mut stale) = (vec![], vec![]);
    match sha(&repo().join(gen_rel)) {
        None => e.push(format!("{id}: generator {gen_rel} missing")),
        Some(h) if h != gen_sha => stale.push(format!(
            "{id}: generator {gen_rel} changed since provenance (stale; rebuild)"
        )),
        _ => {}
    }
    let Some(deps) = v["generatorDeps"].as_array() else {
        e.push(format!("{id}: provenance lacks generatorDeps"));
        return (e, stale);
    };
    let mut listed = BTreeSet::new();
    for d in deps {
        let rel = d["path"].as_str().unwrap_or_default();
        listed.insert(repo_rel(&repo().join(rel)));
        match sha(&repo().join(rel)) {
            None => e.push(format!("{id}: generator dep {rel} missing")),
            Some(h) if h != d["sha256"].as_str().unwrap_or_default() => stale.push(format!(
                "{id}: generator dep {rel} changed since provenance (stale; rebuild)"
            )),
            _ => {}
        }
    }
    match scanned_generator_deps(&repo().join(gen_rel)) {
        Err(err) => e.push(format!("{id}: {err}")),
        Ok(want) => {
            for d in want.difference(&listed) {
                e.push(format!("{id}: {gen_rel} imports {d}, not in generatorDeps"));
            }
            for d in listed.difference(&want) {
                e.push(format!(
                    "{id}: generatorDeps lists {d}, which {gen_rel} does not import"
                ));
            }
        }
    }
    (e, stale)
}

/// `route` is scripted or interactive. An interactive piece references its high-poly `.blend` in
/// the local store by SHA-256 (R1), and that hash must be the one its committed bake report baked
/// from (`bake.blend.sha256`), which CI can check without the gitignored file.
fn route_errors(id: &str, v: &serde_json::Value) -> Vec<String> {
    match v["route"].as_str() {
        Some("scripted") => vec![],
        Some("interactive") => {
            let b = &v["blendRef"];
            let key = b["key"].as_str().unwrap_or_default();
            let digest = b["sha256"].as_str().unwrap_or_default();
            let mut e = vec![];
            if b["store"] != "local:assets-source/blend"
                || key.is_empty()
                || key.starts_with('/')
                || key.split('/').any(|c| c == "..")
                || digest.len() != 64
                || !digest.bytes().all(|c| c.is_ascii_hexdigit())
                || b["bytes"].as_u64().unwrap_or(0) == 0
            {
                e.push(format!(
                    "{id}: interactive route needs blendRef {{store: \"local:assets-source/blend\", key (store-relative), sha256, bytes}} (R1), got {b}"
                ));
            } else if v["bake"]["blend"]["sha256"].as_str() != Some(digest) {
                e.push(format!(
                    "{id}: blendRef.sha256 differs from the bake report's blend.sha256 (baked from another .blend)"
                ));
            }
            e
        }
        other => vec![format!(
            "{id}: route {other:?} is not scripted or interactive"
        )],
    }
}

fn check_provenance(kit: &Kit, p: &PieceEntry) -> Vec<String> {
    let path = repo().join(&p.provenance);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![format!(
            "{}: provenance missing at {}",
            p.id,
            path.display()
        )];
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(err) => return vec![format!("{}: provenance JSON: {err}", p.id)],
    };
    let mut e = vec![];
    for key in [
        "pieceId",
        "route",
        "authoringAgent",
        "blender",
        "toolVersions",
        "bake",
        "lods",
        "collider",
        "sockets",
        "pivot",
        "compression",
        "derivedFiles",
        "license",
        "validation",
    ] {
        if v.get(key).is_none() {
            e.push(format!("{}: provenance lacks {key}", p.id));
        }
    }
    if v["pieceId"] != p.id.as_str() {
        e.push(format!("{}: provenance pieceId is {}", p.id, v["pieceId"]));
    }
    e.extend(generator_errors(&p.id, &v));
    e.extend(route_errors(&p.id, &v));
    if v["authoringAgent"]["model"] != "claude-opus-5-5" {
        e.push(format!(
            "{}: authoringAgent.model must be claude-opus-5-5 (R3)",
            p.id
        ));
    }
    if v["validation"]["pass"] != true {
        e.push(format!("{}: validation.pass is not true", p.id));
    }
    e.extend(encode_errors(&p.id, &v["compression"]["textureEncodes"]));
    // The provenance must record the class the maps were actually compressed under. A provenance
    // check, so `make kit-provenance` (which skips them until it writes) never needs an early write.
    let report = path.with_file_name("compress-report.json");
    if let Some(r) = std::fs::read_to_string(&report)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        && v["compression"]["budgets"]["class"] != r["budgets"]["class"]
    {
        e.push(format!(
            "{}: provenance budget class {} != compress report {} (re-run kit-provenance)",
            p.id, v["compression"]["budgets"]["class"], r["budgets"]["class"]
        ));
    }
    let listed: BTreeSet<&str> = v["derivedFiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["path"].as_str())
        .collect();
    for want in required_derived(kit, p) {
        if !listed.contains(want.as_str()) {
            e.push(format!("{}: {want} not in provenance derivedFiles", p.id));
        }
    }
    for f in v["derivedFiles"].as_array().into_iter().flatten() {
        let rel = f["path"].as_str().unwrap_or_default();
        match sha(&repo().join(rel)) {
            None => e.push(format!("{}: derived file {rel} missing", p.id)),
            Some(h) if h != f["sha256"].as_str().unwrap_or_default() => {
                e.push(format!("{}: {rel} changed since provenance (stale)", p.id))
            }
            _ => {}
        }
    }
    let refs = v["references"].as_array().cloned().unwrap_or_default();
    if kit.requires_refs && refs.is_empty() {
        e.push(format!(
            "{}: provenance has no references (R2: run kit-provenance with REFS=)",
            p.id
        ));
    }
    for r in &refs {
        let rel = r["path"].as_str().unwrap_or_default();
        if sha(&repo().join(rel)).as_deref() != r["sha256"].as_str() {
            e.push(format!("{}: reference {rel} missing or changed", p.id));
        }
    }
    let approved: BTreeSet<&str> = v["approvals"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["gate"] == "reference")
        .flat_map(|a| a["approved"].as_array().into_iter().flatten())
        .filter_map(|x| x.as_str())
        .collect();
    if !refs.is_empty()
        && !refs
            .iter()
            .any(|r| approved.contains(r["path"].as_str().unwrap_or_default()))
    {
        e.push(format!(
            "{}: no owner reference approval covers its references (R2, S5)",
            p.id
        ));
    }
    e
}

/// Provenance of one tileable set, written by `kit-provenance.mjs <set> --material` at
/// `assets-source/kit/materials/<set>/provenance.json`.
/// Assumes the single repo kit (`assets/kit`): the path ignores `Kit::root`, so a second kit or
/// the fixture listing a tileable set would be checked against the repo's set provenance.
fn material_provenance_path(set: &str) -> PathBuf {
    repo().join(format!("assets-source/kit/materials/{set}/provenance.json"))
}

/// A set's provenance: present, complete, validated, transfers per channel (R6), and every map
/// stem in BC7, ASTC and PNG listed in `derivedFiles` with a hash that matches the shipped file.
/// The generator and its helpers are re-hashed as for pieces (`generator_errors`).
fn check_material_provenance(kit: &Kit, set: &str, path: &Path) -> Vec<String> {
    let id = format!("material {set}");
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![format!("{id}: provenance missing at {}", path.display())];
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(err) => return vec![format!("{id}: provenance JSON: {err}")],
    };
    let mut e = vec![];
    for key in [
        "setId",
        "generator",
        "authoringAgent",
        "blender",
        "toolVersions",
        "bake",
        "bakeReport",
        "compressReport",
        "compression",
        "derivedFiles",
        "license",
        "validation",
    ] {
        if v.get(key).is_none() {
            e.push(format!("{id}: provenance lacks {key}"));
        }
    }
    if v["setId"] != set {
        e.push(format!("{id}: provenance setId is {}", v["setId"]));
    }
    e.extend(generator_errors(&id, &v));
    if v["authoringAgent"]["model"] != "claude-opus-5-5" {
        e.push(format!(
            "{id}: authoringAgent.model must be claude-opus-5-5 (R3)"
        ));
    }
    if v["validation"]["pass"] != true {
        e.push(format!("{id}: validation.pass is not true"));
    }
    e.extend(encode_errors(&id, &v["compression"]["textureEncodes"]));
    let listed: BTreeSet<&str> = v["derivedFiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| f["path"].as_str())
        .collect();
    if let Some(ts) = kit.manifest.materials.get(set) {
        for stem in ts.maps.values() {
            for fam in [TextureFamily::Bc7, TextureFamily::Astc, TextureFamily::Png] {
                let want = repo_rel(&kit.root.join(map_file(stem, fam)));
                if !listed.contains(want.as_str()) {
                    e.push(format!("{id}: {want} not in provenance derivedFiles"));
                }
            }
        }
    }
    for f in v["derivedFiles"].as_array().into_iter().flatten() {
        let rel = f["path"].as_str().unwrap_or_default();
        match sha(&repo().join(rel)) {
            None => e.push(format!("{id}: derived file {rel} missing")),
            Some(h) if h != f["sha256"].as_str().unwrap_or_default() => {
                e.push(format!("{id}: {rel} changed since provenance (stale)"))
            }
            _ => {}
        }
    }
    e
}

/// Repo-relative path, or the absolute path for files outside the repo (scratch kits in tests).
fn repo_rel(p: &Path) -> String {
    p.strip_prefix(repo())
        .unwrap_or(p)
        .to_string_lossy()
        .into_owned()
}

/// Loads every piece through `kit://` (MeshoptGlbReader) and bevy_gltf, then checks names/extras.
fn check_bevy(kit: &Kit) -> Vec<String> {
    if kit.manifest.pieces.is_empty() {
        return vec![];
    }
    let mut app = App::new();
    hex_assets::bevy::register_kit_source(&mut app, kit.root.to_string_lossy().into_owned());
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        WorldSerializationPlugin,
        MeshPlugin,
        GltfPlugin::default(),
    ));
    app.finish();
    app.cleanup();
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<(String, Handle<Gltf>)> = kit
        .manifest
        .pieces
        .iter()
        .map(|p| (p.id.clone(), server.load(format!("kit://{}", p.glb))))
        .collect();
    let mut e = vec![];
    for _ in 0..10_000 {
        app.update();
        let states: Vec<_> = handles
            .iter()
            .map(|(_, h)| server.get_load_state(h.id()))
            .collect();
        if states
            .iter()
            .all(|s| matches!(s, Some(LoadState::Loaded) | Some(LoadState::Failed(_))))
        {
            break;
        }
    }
    let gltfs = app.world().resource::<Assets<Gltf>>();
    let nodes = app.world().resource::<Assets<GltfNode>>();
    for ((id, h), p) in handles.iter().zip(&kit.manifest.pieces) {
        match server.get_load_state(h.id()) {
            Some(LoadState::Failed(err)) => {
                e.push(format!("{id}: bevy_gltf load failed: {err}"));
                continue;
            }
            Some(LoadState::Loaded) => {}
            other => {
                e.push(format!("{id}: not loaded ({other:?})"));
                continue;
            }
        }
        let g = gltfs.get(h).unwrap();
        for want in [
            id.clone(),
            format!("{id}_lod0"),
            format!("{id}_lod1"),
            format!("{id}_lod2"),
            format!("{id}_collider"),
        ] {
            if !g.named_nodes.contains_key(want.as_str()) {
                e.push(format!("{id}: bevy node {want} missing"));
            }
        }
        for lod in 0..3 {
            if !g.named_meshes.contains_key(lod_mesh_name(id, lod).as_str()) {
                e.push(format!(
                    "{id}: bevy mesh {id}_lod{lod} missing (runtime looks meshes up by name)"
                ));
            }
        }
        // A rotor (PRD §7.2): the runtime spins node `<id>_rotor_lod<n>` (its transform, then local
        // X) and draws its mesh, so each node must carry that mesh and say what it is.
        if (0..3).any(|lod| {
            g.named_nodes
                .contains_key(rotor_mesh_name(id, lod).as_str())
        }) {
            for lod in 0..3 {
                let name = rotor_mesh_name(id, lod);
                let Some(n) = g.named_nodes.get(name.as_str()).and_then(|h| nodes.get(h)) else {
                    e.push(format!(
                        "{id}: bevy node {name} missing (a rotor is in every LOD)"
                    ));
                    continue;
                };
                if n.mesh.is_none() || n.mesh.as_ref() != g.named_meshes.get(name.as_str()) {
                    e.push(format!("{id}: bevy node {name} does not carry mesh {name}"));
                }
                let kind = n
                    .extras
                    .as_ref()
                    .and_then(|x| serde_json::from_str::<serde_json::Value>(&x.value).ok())
                    .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_owned));
                if kind.as_deref() != Some("rotor") {
                    e.push(format!(
                        "{id}: {name} extras.kind is {kind:?}, not \"rotor\""
                    ));
                }
            }
        }
        for (name, nh) in &g.named_nodes {
            if name.starts_with("socket-lane-")
                || name.starts_with("anchor-prop-")
                || name.starts_with("anchor-park-")
            {
                let ok = nodes
                    .get(nh)
                    .and_then(|n| n.extras.as_ref())
                    .and_then(|x| serde_json::from_str::<serde_json::Value>(&x.value).ok())
                    .is_some_and(|v| v.get("kind").is_some());
                if !ok {
                    e.push(format!("{id}: {name} has no extras.kind through bevy_gltf"));
                }
            }
        }
        if p.kind == PieceKind::Edge && !g.named_nodes.keys().any(|k| k.starts_with("socket-lane-"))
        {
            e.push(format!("{id}: edge piece without socket-lane-*"));
        }
    }
    e
}

#[test]
fn manifests_parse() {
    assert!(
        !kits()[0].manifest.pieces.is_empty(),
        "fixture manifest must hold the fixture piece"
    );
}

/// Streaming packs (PRD §10, §15 P2): the runtime manifest names a pack on every piece and set
/// (the parser's core default is for the gate fixtures only), core holds everything the start tile
/// draws, and a hero-class piece (full-detail maps) never loads before the first interaction.
#[test]
fn packs_declared_core_draws_the_start_tile_and_heroes_stream() {
    let mut e = vec![];
    let path = repo().join("assets/kit/manifest.json");
    let raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for (id, s) in raw["materials"].as_object().unwrap() {
        if !s["pack"].is_string() {
            e.push(format!("material set {id}: no \"pack\""));
        }
    }
    for p in raw["pieces"].as_array().unwrap() {
        if !p["pack"].is_string() {
            e.push(format!("piece {}: no \"pack\"", p["id"]));
        }
    }
    let kit = kits().pop().unwrap();
    let m = &kit.manifest;
    let catalog = hex_core::TileCatalog::builtin().unwrap();
    let start = catalog.get(hex_core::START_TILE).unwrap();
    for r in start.kit_refs(catalog.sockets()) {
        // Scatter props are not drawn in play yet (play.rs spawns `props.hero` only).
        if start.props.scatter.iter().any(|s| &s.piece == r) {
            continue;
        }
        match m.piece_pack(r.as_str()) {
            Some(CORE_PACK) | None => {}
            Some(k) => e.push(format!(
                "start tile {} draws {} from pack {k}; it must be core",
                start.id,
                r.as_str()
            )),
        }
    }
    if m.packs().first() != Some(&CORE_PACK) {
        e.push(format!("no core pack: {:?}", m.packs()));
    }
    for p in &m.pieces {
        match budget_class(p) {
            Ok(BudgetClass::Hero) if p.pack() == CORE_PACK => e.push(format!(
                "{}: budget class hero in pack core; hero assets stream after the first interaction",
                p.id
            )),
            Ok(_) => {}
            Err(err) => e.push(err),
        }
    }
    fail(e);
}

#[test]
fn geometry_budgets_lods_sockets_border() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_geometry(&k, p));
            e.extend(check_edge_profile(&k, p));
        }
        e.extend(check_center_edge_joint(&k));
        e.extend(check_coast_seams(&k));
    }
    fail(e);
}

/// Every coast base carries the same extra beach-edge ring samples, so each legal beach seam
/// (straight, headland and bay in any pairing) meets vertex for vertex (contract §6).
fn check_coast_seams(kit: &Kit) -> Vec<String> {
    let mut seen: Vec<(String, Vec<f32>)> = vec![];
    for p in &kit.manifest.pieces {
        let Some(coast) = (p.kind == PieceKind::Base)
            .then(|| coast_of_base(&p.id))
            .flatten()
        else {
            continue;
        };
        let Ok((doc, buffers, _)) = plain(kit, p) else {
            continue;
        };
        let Some(m) = mesh_named(&doc, &format!("{}_lod0", p.id)) else {
            continue;
        };
        let ring = border_ring_any(&positions(&m, &buffers));
        seen.push((p.id.clone(), coast_ring_errors(&p.id, &coast, &ring).1));
    }
    let mut e = vec![];
    if let Some((first, s0)) = seen.first() {
        for (id, s) in &seen[1..] {
            if s.len() != s0.len() || s.iter().zip(s0).any(|(a, b)| (a - b).abs() > 1e-3) {
                e.push(format!(
                    "{id}: beach-edge ring samples {s:?} differ from {first}'s {s0:?}"
                ));
            }
        }
    }
    e
}

#[test]
fn coast_bases_are_held_to_the_coast_profile() {
    let catalog = hex_core::TileCatalog::builtin().unwrap();
    let beach = Coastline::of(catalog.get("beach").unwrap(), 0).unwrap();
    assert!(matches!(
        coast_of_base("beach_outer_base"),
        Some(Coastline::Arc {
            land_inside: true,
            ..
        })
    ));
    assert!(coast_of_base("meadow_base").is_none());
    // The river mouths' bases are held to the mouth bed (`mouth_base_errors`), and their beach
    // edges still join `check_coast_seams`.
    for id in ["river_mouth_base", "river_mouth_l_base"] {
        let m = mouth_of_base(id).unwrap_or_else(|| panic!("{id}: no mouth"));
        assert_eq!(Some(m.coast), coast_of_base(id), "{id}");
        assert!((m.height(Vec2::ZERO) - (-1.9773)).abs() < 2e-3, "{id}");
    }
    assert!(
        mouth_of_base("beach_base").is_none() && mouth_of_base("river_straight_base").is_none()
    );
    // A ring point on the profile passes; one 5 cm off, or with a +Y normal on the slope, fails.
    let s = |x: f32, z: f32| beach.seaward(Vec2::new(x, z));
    let mid = [
        APOTHEM * (120f32).to_radians().cos(),
        APOTHEM * (120f32).to_radians().sin(),
    ];
    let dir = Vec2::new(-(120f32).to_radians().sin(), (120f32).to_radians().cos());
    let at = |o: f32| {
        let p = Vec2::from(mid) + dir * o;
        let n = coast_normal(&beach, p.x, p.y);
        ([p.x, coast_height(s(p.x, p.y)), p.y], n.to_array())
    };
    let good: Vec<_> = (-4..=4).map(|i| at(i as f32 * 0.5)).collect();
    let errs = |r: &[([f32; 3], [f32; 3])]| coast_ring_errors("b", &beach, r).0;
    assert!(
        errs(&good)
            .iter()
            .all(|m| m.contains("lattice ring vertex")),
        "{:?}",
        errs(&good)
    );
    let mut raised = good.clone();
    raised[2].0[1] += 0.05;
    assert!(
        errs(&raised)
            .iter()
            .any(|m| m.contains("off the coast profile"))
    );
    let mut flat = good.clone();
    flat[2].1 = [0.0, 1.0, 0.0];
    assert!(errs(&flat).iter().any(|m| m.contains("bed normal")));
    // A face on the drop 5 cm off the profile fails; on the profile it passes.
    let tri = |dy: f32| {
        let q = |x: f32, z: f32| [x, coast_height(s(x, z)) + dy, z];
        // Seaward is +x (cos 30, sin 30); a small triangle around s = 5.5.
        let c = Vec2::new(30f32.to_radians().cos(), 30f32.to_radians().sin()) * 5.5;
        [q(c.x, c.y), q(c.x, c.y + 0.08), q(c.x + 0.08, c.y)]
    };
    let ok = coast_profile_errors("t", &beach, &[tri(0.0)]);
    assert!(ok.1.is_empty(), "{:?}", ok.1);
    let off = coast_profile_errors("t", &beach, &[tri(0.05)]);
    assert!(!off.1.is_empty() && off.0[1] > 0.04);
}

#[test]
fn ktx2_color_space_mips_and_budgets() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_textures(&k, p));
        }
        e.extend(material_texture_errors(&k, BudgetClass::Tileable.ktx_max()));
    }
    fail(e);
}

#[test]
fn provenance_complete_and_hashes_match() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_provenance(&k, p));
        }
    }
    fail(e);
}

/// Every tileable set a manifest lists has complete provenance whose hashes match the shipped files.
#[test]
fn material_provenance_complete_and_hashes_match() {
    let mut e = vec![];
    for k in kits() {
        for set in k.manifest.materials.keys() {
            e.extend(check_material_provenance(
                &k,
                set,
                &material_provenance_path(set),
            ));
        }
    }
    fail(e);
}

#[test]
fn bevy_gltf_reimport_names_and_extras() {
    let mut e = vec![];
    for k in kits() {
        e.extend(check_bevy(&k));
    }
    fail(e);
}

/// S33: the JS compress step and this gate use the same numbers (`scripts/assets/kit-lib.mjs`).
#[test]
fn budgets_match_kit_lib() {
    let js = std::fs::read_to_string(repo().join("scripts/assets/kit-lib.mjs")).unwrap();
    let object = |name: &str| -> String {
        let start = js
            .find(&format!("export const {name} = {{"))
            .unwrap_or_else(|| panic!("{name} not found in kit-lib.mjs"));
        let body = &js[start..];
        body[body.find('{').unwrap()..body.find(';').unwrap()].replace('_', "")
    };
    let num = |src: &str, key: &str| -> u64 {
        let at = src
            .find(&format!("{key}:"))
            .unwrap_or_else(|| panic!("{key} missing in {src}"));
        src[at + key.len() + 1..]
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap()
    };
    let budgets = object("KTX_BUDGETS");
    for (class, bytes) in KTX_BUDGETS {
        assert_eq!(
            num(&budgets, class.as_str()),
            bytes,
            "KTX_BUDGETS.{}",
            class.as_str()
        );
    }
    assert_eq!(
        budgets.matches(':').count(),
        KTX_BUDGETS.len(),
        "kit-lib.mjs KTX_BUDGETS has a class the gate lacks: {budgets}"
    );
    let glb = js
        .split("export const GLB_MAX = ")
        .nth(1)
        .unwrap()
        .replace('_', "");
    assert_eq!(num(&format!("x:{glb}"), "x"), GLB_MAX_BYTES);
    let vk = object("VK");
    let (bc7, astc) = vk.split_at(vk.find("astc").unwrap());
    assert_eq!(
        (num(bc7, "srgb"), num(bc7, "linear")),
        (VK_BC7_SRGB as u64, VK_BC7_UNORM as u64)
    );
    assert_eq!(
        (num(astc, "srgb"), num(astc, "linear")),
        (VK_ASTC_4X4_SRGB as u64, VK_ASTC_4X4_UNORM as u64)
    );
}

/// Generators size their LODs from `scripts/blender/lib/budgets.py`; it must carry the gate's numbers.
#[test]
fn budgets_match_blender_lib() {
    let py = std::fs::read_to_string(repo().join("scripts/blender/lib/budgets.py")).unwrap();
    let value = |name: &str| -> String {
        let line = py
            .lines()
            .find(|l| l.starts_with(&format!("{name} = ")))
            .unwrap_or_else(|| panic!("{name} not found in budgets.py"));
        line[name.len() + 3..].trim().to_string()
    };
    let float = |name: &str| value(name).parse::<f32>().unwrap();
    assert_eq!(float("LOD1_MAX_RATIO"), LOD1_MAX_RATIO);
    assert_eq!(float("LOD2_MAX_RATIO"), LOD2_MAX_RATIO);
    assert_eq!(float("LANE_CUT_LOD2_MAX_RATIO"), LANE_CUT_LOD2_MAX_RATIO);
    assert_eq!(float("COAST_ARC_LOD2_MAX_RATIO"), COAST_ARC_LOD2_MAX_RATIO);
    assert_eq!(
        value("RIVER_DRESSING_LOD0_MAX").parse::<usize>().unwrap(),
        RIVER_DRESSING_LOD0_MAX
    );
    let lod0 = value("LOD0_MAX");
    for (name, kind) in [
        ("base", PieceKind::Base),
        ("edge", PieceKind::Edge),
        ("center", PieceKind::Center),
        ("hero", PieceKind::Hero),
        ("prop", PieceKind::Prop),
        ("tree", PieceKind::Tree),
    ] {
        let at = lod0
            .find(&format!("\"{name}\": "))
            .unwrap_or_else(|| panic!("LOD0_MAX lacks {name}: {lod0}"));
        let n: usize = lod0[at + name.len() + 4..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap();
        assert_eq!(n, kind.lod0_budget(), "LOD0_MAX[{name}]");
    }
    assert_eq!(
        lod0.matches(':').count(),
        6,
        "LOD0_MAX has a kind the gate lacks: {lod0}"
    );
    let start = py.find("RIVER_DRESSING_PROPS = (").unwrap();
    let props = &py[start..start + py[start..].find(')').unwrap()];
    let ids: Vec<&str> = props.split('"').skip(1).step_by(2).collect();
    assert_eq!(ids, RIVER_DRESSING_PROPS);
}

// Negative cases: the gate must catch these (Review Focus 3 and 4).
fn fixture() -> (Kit, PieceEntry) {
    let k = kits().remove(0);
    let p = k.manifest.pieces[0].clone();
    (k, p)
}

/// A scratch directory removed when the guard drops.
struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!("kitgate-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

#[test]
fn gate_catches_wrong_color_space() {
    let (k, p) = fixture();
    let stem = &p.maps[&Channel::Masks];
    let tmp = temp("color");
    std::fs::create_dir_all(tmp.join(Path::new(stem).parent().unwrap())).unwrap();
    for fam in [TextureFamily::Bc7, TextureFamily::Astc, TextureFamily::Png] {
        let mut b = std::fs::read(k.root.join(map_file(stem, fam))).unwrap();
        if fam == TextureFamily::Bc7 {
            b[12..16].copy_from_slice(&VK_BC7_SRGB.to_le_bytes());
        }
        std::fs::write(tmp.join(map_file(stem, fam)), b).unwrap();
    }
    let bad = Kit {
        root: tmp.to_path_buf(),
        manifest: k.manifest.clone(),
        requires_refs: false,
    };
    let e = check_textures(&bad, &p);
    assert!(
        e.iter().any(|m| m.contains("wrong color space for masks")),
        "{e:?}"
    );
}

#[test]
fn gate_catches_png_recorded_as_srgb() {
    let encodes = serde_json::json!([
        {"channel": "masks", "family": "png", "transfer": "srgb"},
        {"channel": "albedo", "family": "png", "transfer": "srgb"},
        {"channel": "normal", "family": "bc7", "transfer": "linear", "vkFormat": VK_BC7_SRGB}
    ]);
    let e = encode_errors("x", &encodes);
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(e[0].contains("masks png transfer"), "{e:?}");
    assert!(e[1].contains("normal bc7 vkFormat"), "{e:?}");
}

#[test]
fn gate_catches_budget_overrun_and_stale_budget_class() {
    let (k, p) = fixture();
    let e = texture_errors(&k, &p, 100);
    assert!(
        e.iter()
            .any(|m| m.contains("Bc7 maps") && m.contains("> 100")),
        "{e:?}"
    );
    let tmp = temp("class");
    let src = repo()
        .join(&p.provenance)
        .with_file_name("compress-report.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src).unwrap()).unwrap();
    v["budgets"]["ktxMax"] = 1_000_000.into();
    std::fs::write(tmp.join("compress-report.json"), v.to_string()).unwrap();
    let mut q = p.clone();
    q.provenance = tmp.join("provenance.json").to_string_lossy().into_owned();
    assert!(
        budget_class(&q)
            .unwrap_err()
            .contains("ktxMax 1000000 != 1500000")
    );
    v["budgets"]["class"] = "huge".into();
    std::fs::write(tmp.join("compress-report.json"), v.to_string()).unwrap();
    assert!(
        budget_class(&q)
            .unwrap_err()
            .contains("unknown budget class")
    );
}

#[test]
fn gate_rejects_self_declared_budget_class() {
    let (k, p) = fixture();
    let tmp = temp("selfclass");
    let src = repo()
        .join(&p.provenance)
        .with_file_name("compress-report.json");
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(src).unwrap()).unwrap();
    let mut q = p.clone();
    q.provenance = tmp.join("provenance.json").to_string_lossy().into_owned();
    let write = |class: BudgetClass| {
        let mut v = report.clone();
        v["budgets"]["class"] = class.as_str().into();
        v["budgets"]["ktxMax"] = class.ktx_max().into();
        std::fs::write(tmp.join("compress-report.json"), v.to_string()).unwrap();
    };
    write(BudgetClass::Hero);
    let err = budget_class(&q).unwrap_err();
    assert!(
        err.contains("hero is only for hero or tree pieces"),
        "{err}"
    );
    q.kind = PieceKind::Tree;
    assert_eq!(budget_class(&q), Ok(BudgetClass::Hero));
    write(BudgetClass::Tileable);
    let err = budget_class(&q).unwrap_err();
    assert!(err.contains("tileable is for material sets"), "{err}");
    // Report and provenance disagree: the maps were re-compressed after provenance was written.
    q.kind = PieceKind::Base;
    write(BudgetClass::Piece);
    let mut prov: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo().join(&p.provenance)).unwrap())
            .unwrap();
    prov["compression"]["budgets"]["class"] = "hero".into();
    std::fs::write(tmp.join("provenance.json"), prov.to_string()).unwrap();
    assert_eq!(
        budget_class(&q),
        Ok(BudgetClass::Piece),
        "a provenance check only"
    );
    let e = check_provenance(&k, &q);
    assert!(
        e.iter().any(|m| m.contains("provenance budget class")),
        "{e:?}"
    );
}

#[test]
fn gate_requires_every_derived_file_in_provenance() {
    let (k, mut p) = fixture();
    let dir = temp("derived");
    let tmp = dir.join("provenance.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(repo().join(&p.provenance)).unwrap())
            .unwrap();
    v["derivedFiles"]
        .as_array_mut()
        .unwrap()
        .retain(|f| !f["path"].as_str().unwrap().ends_with(".bc7.ktx2"));
    std::fs::write(&tmp, v.to_string()).unwrap();
    p.provenance = tmp.to_string_lossy().into_owned();
    let e = check_provenance(&k, &p);
    assert!(
        e.iter()
            .any(|m| m.contains("fixture.masks.bc7.ktx2 not in provenance derivedFiles")),
        "{e:?}"
    );
}

/// The `border_ring(4)` hex: 24 points, corners at 30° + 60°·i, radius 16 (hexgeo).
fn hex_ring() -> Vec<[f32; 2]> {
    let corner = |i: usize| {
        let a = (30.0 + 60.0 * i as f32).to_radians();
        [16.0 * a.cos(), 16.0 * a.sin()]
    };
    (0..6)
        .flat_map(|k| {
            let (a, b) = (corner(k), corner(k + 1));
            (0..4).map(move |s| {
                let t = s as f32 / 4.0;
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
            })
        })
        .collect()
}

#[test]
fn ring_must_match_across_lods_and_rotate_onto_itself() {
    let ring = hex_ring();
    assert!(ring_errors("r", &[ring.clone(), ring.clone(), ring.clone()], true).is_empty());
    let mut moved = ring.clone();
    moved[5][0] += 3e-4;
    let e = ring_errors("r", &[ring.clone(), ring.clone(), moved.clone()], false);
    assert_eq!(e, vec!["r: border ring differs across LODs".to_string()]);
    let e = ring_errors("r", &[moved.clone(), moved.clone(), moved], true);
    assert!(
        e.iter()
            .any(|m| m.contains("not invariant under 60° rotation")),
        "{e:?}"
    );
    let e = ring_errors("r", &[ring[..12].to_vec()], true);
    assert!(
        e.iter().any(|m| m.contains("border ring has 12 vertices")),
        "{e:?}"
    );
}

#[test]
fn base_ring_level_must_be_flat_and_a_contract_level() {
    let at = |y: f32| -> Vec<([f32; 3], [f32; 3])> {
        hex_ring()
            .into_iter()
            .map(|[x, z]| ([x, y, z], [0.0, 1.0, 0.0]))
            .collect()
    };
    assert_eq!(base_ring_level("b", &at(0.0)), Ok(0.0));
    let ocean = at(-hex_core::LEVEL_STEP);
    assert_eq!(base_ring_level("b", &ocean), Ok(-hex_core::LEVEL_STEP));
    assert_eq!(border_ring_at(&ocean, -hex_core::LEVEL_STEP).len(), 24);
    assert!(border_ring(&ocean).is_empty());
    let e = base_ring_level("b", &at(-1.4)).unwrap_err();
    assert!(e.contains("not a contract level"), "{e}");
    let mut tilted = at(-hex_core::LEVEL_STEP);
    tilted[3].0[1] += 0.01;
    let e = base_ring_level("b", &tilted).unwrap_err();
    assert!(e.contains("not flat"), "{e}");
}

#[test]
fn socket_must_sit_on_its_lane_contract() {
    let (edge, off) = (2u64, 1.5f32);
    let a = (60.0 * edge as f32).to_radians();
    let at = [
        APOTHEM * a.cos() + a.sin() * off,
        0.0,
        APOTHEM * a.sin() - a.cos() * off,
    ];
    let extras = serde_json::json!({"kind": "road", "edge": edge, "offset": off});
    assert!(socket_errors("s", "socket-lane-2", &extras, at).is_empty());
    let moved = [at[0], at[1], at[2] + 0.01];
    assert!(socket_errors("s", "socket-lane-2", &extras, moved)[0].contains("lane contract"));
    let mirrored = [
        APOTHEM * a.cos() - a.sin() * off,
        0.0,
        APOTHEM * a.sin() + a.cos() * off,
    ];
    assert!(!socket_errors("s", "socket-lane-2", &extras, mirrored).is_empty());
    assert!(socket_errors("s", "x", &serde_json::json!({}), at)[0].contains("extras lack"));
}

#[test]
fn world_transform_composes_parents() {
    let t = |x: f32, y: f32, z: f32| -> Mat4 {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [x, y, z, 1.0],
        ]
    };
    let (s, c) = 90f32.to_radians().sin_cos();
    let rot_y: Mat4 = [
        [c, 0.0, -s, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [s, 0.0, c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let w = mul(&mul(&t(10.0, 0.0, 0.0), &rot_y), &t(1.0, 2.0, 0.0));
    let p = w[3];
    assert!(
        (p[0] - 10.0).abs() < 1e-5 && (p[1] - 2.0).abs() < 1e-5 && (p[2] + 1.0).abs() < 1e-5,
        "{p:?}"
    );
}

#[test]
fn gate_catches_missing_file_and_stale_provenance() {
    let (k, mut p) = fixture();
    p.glb = "p1/fixture/nope.glb".into();
    assert!(
        check_geometry(&k, &p)
            .iter()
            .any(|m| m.contains("nope.glb"))
    );
    let (_, mut p) = fixture();
    let dir = temp("prov");
    let tmp = dir.join("provenance.json");
    let text = std::fs::read_to_string(repo().join(&p.provenance)).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    v["derivedFiles"][0]["sha256"] = "00".into();
    std::fs::write(&tmp, v.to_string()).unwrap();
    p.provenance = tmp.to_string_lossy().into_owned();
    assert!(check_provenance(&k, &p).iter().any(|m| m.contains("stale")));
}

/// The generator and every `_*.py` helper it imports (transitively) are
/// recorded and re-hashed: editing either without a rebuild warns (owner ruling 2026-10-19), and a
/// provenance that omits an imported helper (or lists one no longer imported) fails the gate.
#[test]
fn gate_requires_current_generator_and_complete_deps() {
    let dir = temp("gendep");
    let (generator, helper, nested) = (
        dir.join("gen.py"),
        dir.join("_help.py"),
        dir.join("_deep.py"),
    );
    std::fs::write(
        &generator,
        "import bpy, math\nfrom _help import thing  # noqa\n",
    )
    .unwrap();
    std::fs::write(&helper, "def thing():\n    import _deep\n").unwrap();
    std::fs::write(&nested, "x = 1\n").unwrap();
    let rec =
        |p: &Path| serde_json::json!({"path": p.to_string_lossy(), "sha256": sha(p).unwrap()});
    let good = serde_json::json!({"generator": rec(&generator), "generatorDeps": [rec(&helper), rec(&nested)]});
    assert_eq!(generator_errors("g", &good), Vec::<String>::new());
    let has = |v: &serde_json::Value, want: &str| {
        let e = generator_errors("g", v);
        assert!(e.iter().any(|m| m.contains(want)), "want {want:?} in {e:?}");
    };
    // An edit since provenance warns and never fails (owner ruling 2026-10-19).
    let warns = |v: &serde_json::Value, want: &str| {
        let (e, stale) = generator_check("g", v);
        assert_eq!(e, Vec::<String>::new(), "a stale hash must not be an error");
        assert!(
            stale.iter().any(|m| m.contains(want)),
            "want {want:?} in {stale:?}"
        );
    };
    let mut v = good.clone();
    v["generator"]["sha256"] = "00".into();
    warns(&v, "gen.py changed since provenance (stale; rebuild)");
    let mut v = good.clone();
    v["generatorDeps"][1]["sha256"] = "00".into();
    warns(&v, "_deep.py changed since provenance");
    let mut v = good.clone();
    v.as_object_mut().unwrap().remove("generatorDeps");
    has(&v, "provenance lacks generatorDeps");
    let mut v = good.clone();
    v.as_object_mut().unwrap().remove("generator");
    has(&v, "provenance lacks generator {path, sha256}");
    let mut v = good.clone();
    v["generatorDeps"] = serde_json::json!([rec(&helper)]);
    has(&v, "_deep.py, not in generatorDeps");
    let extra = dir.join("_unused.py");
    std::fs::write(&extra, "y = 2\n").unwrap();
    let mut v = good.clone();
    v["generatorDeps"].as_array_mut().unwrap().push(rec(&extra));
    has(&v, "_unused.py, which");
    std::fs::remove_file(&extra).unwrap();
    has(&v, "_unused.py missing");
}

#[test]
fn interactive_route_requires_a_blend_ref_matching_the_bake() {
    let digest = "ab".repeat(32);
    let good = serde_json::json!({
        "route": "interactive",
        "blendRef": {"store": "local:assets-source/blend", "key": "t/t_high.blend", "sha256": digest, "bytes": 10},
        "bake": {"blend": {"sha256": digest}}
    });
    assert_eq!(route_errors("t", &good), Vec::<String>::new());
    let has = |v: &serde_json::Value, want: &str| {
        let e = route_errors("t", v);
        assert!(e.iter().any(|m| m.contains(want)), "want {want:?} in {e:?}");
    };
    let mut v = good.clone();
    v.as_object_mut().unwrap().remove("blendRef");
    has(&v, "interactive route needs blendRef");
    let mut v = good.clone();
    v["blendRef"]["key"] = "../../Users/x/t_high.blend".into();
    has(&v, "interactive route needs blendRef");
    let mut v = good.clone();
    v["bake"]["blend"]["sha256"] = "cd".repeat(32).into();
    has(&v, "differs from the bake report");
    let mut v = good.clone();
    v["route"] = "sculpted".into();
    has(&v, "is not scripted or interactive");
    assert!(route_errors("t", &serde_json::json!({"route": "scripted"})).is_empty());
}

#[test]
fn gate_requires_approved_references_for_real_pieces() {
    let (mut k, mut p) = fixture();
    k.requires_refs = true;
    let dir = temp("refs");
    let tmp = dir.join("provenance.json");
    let text = std::fs::read_to_string(repo().join(&p.provenance)).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
    std::fs::write(&tmp, v.to_string()).unwrap();
    p.provenance = tmp.to_string_lossy().into_owned();
    assert!(
        check_provenance(&k, &p)
            .iter()
            .any(|m| m.contains("no references"))
    );
    let refs: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("assets-source/refs/meadow_base/refs.json")).unwrap(),
    )
    .unwrap();
    v["references"] = refs;
    std::fs::write(&tmp, v.to_string()).unwrap();
    assert!(
        check_provenance(&k, &p)
            .iter()
            .any(|m| m.contains("no owner reference approval"))
    );
    let approval: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("assets-source/refs/meadow_base/approval.json"))
            .unwrap(),
    )
    .unwrap();
    v["approvals"] = serde_json::json!([approval]);
    std::fs::write(&tmp, v.to_string()).unwrap();
    assert_eq!(check_provenance(&k, &p), Vec::<String>::new());
}

/// Real pieces through the one joint check: house_pad at its single lane (declared by socket and
/// by its tile), the straight at 0 and 3, the wide bend at 0 and 2, and wrong edges failing.
#[test]
fn lane_socket_centre_is_checked_at_its_declared_edges() {
    let k = kits()
        .into_iter()
        .find(|k| k.manifest.pieces.iter().any(|p| p.id == "house_pad"))
        .expect("runtime kit holds house_pad");
    let piece = |id: &str| k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
    let pad = piece("house_pad");
    assert_eq!(lane_socket_edges(&k, pad), BTreeSet::from([0]));
    // The tile catalog (house_cluster) gives the pad the same single lane its sockets declare.
    assert_eq!(center_lanes("house_pad"), vec![0]);
    assert!(lane_joint_errors(&k, pad, &[0]).is_empty());
    // The pad has no road at edge 3, so the rotated joint check must fail there.
    let e = lane_joint_errors(&k, pad, &[3]);
    assert!(e.iter().any(|m| m.contains("lane 3")), "{e:?}");
    // The straight centre, turned so its x = -6 end faces edge 0, still meets the edge section.
    let straight = piece("road_straight_center");
    assert!(lane_joint_errors(&k, straight, &[0, 3]).is_empty());
    // A turning piece checked at the wrong edge fails: the wide bend's lanes are 0 and 2, not 1.
    let bend = piece("road_bend_120_center");
    assert!(lane_joint_errors(&k, bend, &center_lanes(&bend.id)).is_empty());
    let e = lane_joint_errors(&k, bend, &[0, 1]);
    assert!(e.iter().any(|m| m.contains("lane 1")), "{e:?}");
    assert!(check_center_edge_joint(&k).is_empty());
}

/// The centre piece's section at every lane, rotated onto edge 0, must equal the edge piece's
/// section at x = 6 (within 0.1 mm), for the straight pair and for turning pieces alike.
#[test]
fn center_sections_must_meet_the_edge_section() {
    let edge = vec![(0, -30_000), (0, 30_000), (800, 5)];
    let ok = vec![edge.clone(), edge.clone()];
    assert!(
        joint_errors(
            "c",
            &[0, 3],
            &[ok.clone(), ok.clone()],
            &[edge.clone(), edge.clone()]
        )
        .is_empty()
    );
    // f32 rotation noise of one 0.1 mm unit is absorbed.
    let noisy = vec![(0, -30_001), (1, 30_000), (800, 4)];
    assert!(
        joint_errors(
            "c",
            &[0, 2],
            &[vec![edge.clone(), noisy]],
            std::slice::from_ref(&edge)
        )
        .is_empty()
    );
    let moved = vec![(0, -30_000), (0, 30_000), (900, 5)];
    let e = joint_errors(
        "c",
        &[0, 3],
        &[ok.clone(), vec![moved, edge.clone()]],
        &[edge.clone(), edge.clone()],
    );
    assert!(
        e.iter().any(|m| m.contains("LOD1") && m.contains("lane 0")),
        "{e:?}"
    );
    // The straight -6 joint: a section that is not the mirrored edge section fails at lane 3.
    let mirror = vec![(0, -30_000), (0, 30_000), (800, -5)];
    let e = joint_errors(
        "c",
        &[0, 3],
        &[vec![edge.clone(), mirror]],
        std::slice::from_ref(&edge),
    );
    assert!(
        e.iter().any(|m| m.contains("LOD0") && m.contains("lane 3")),
        "{e:?}"
    );
    let e = joint_errors("c", &[0, 3], &[ok.clone(), ok], &[edge]);
    assert!(e.iter().any(|m| m.contains("LOD count")), "{e:?}");
}

/// A lane-cut base keeps every lattice ring vertex outside its cut lanes, and adds only the two
/// bank-top points of each cut.
#[test]
fn lane_cut_base_ring_leaves_out_only_its_lanes() {
    let cut = 6.83f32;
    let cuts = BTreeMap::from([(0u64, cut), (3u64, cut)]);
    let bank = |k: u64, o: f32| {
        let a = (60.0 * k as f32).to_radians();
        [
            APOTHEM * a.cos() + a.sin() * o,
            APOTHEM * a.sin() - a.cos() * o,
        ]
    };
    let mut ring: Vec<[f32; 2]> = lattice_ring()
        .into_iter()
        .filter(|(_, k, o)| !(cuts.contains_key(k) && o.abs() < cut))
        .map(|(p, _, _)| p)
        .collect();
    for k in [0, 3] {
        ring.push(bank(k, cut));
        ring.push(bank(k, -cut));
    }
    assert_eq!(cut_ring_errors("b", &ring, &cuts), Vec::<String>::new());
    // A lattice vertex missing outside the cut fails, and so does a stray vertex inside it.
    let e = cut_ring_errors("b", &ring[1..], &cuts);
    assert!(e.iter().any(|m| m.contains("missing")), "{e:?}");
    let mut stray = ring.clone();
    stray.push(bank(0, 2.5));
    let e = cut_ring_errors("b", &stray, &cuts);
    assert!(e.iter().any(|m| m.contains("neither lattice")), "{e:?}");
}

/// Only a lane-cut base gets the wider LOD2 ceiling; every other piece keeps 15%, and the river
/// bend base's LOD2 sits between the two (so the general ratio alone would fail it).
#[test]
fn lane_cut_bases_get_the_wider_lod2_ceiling() {
    let k = kits()
        .into_iter()
        .find(|k| k.manifest.pieces.iter().any(|p| p.id == "river_bend_base"))
        .expect("runtime kit holds river_bend_base");
    let piece = |id: &str| k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
    for id in ["river_bend_base", "river_straight_base"] {
        assert_eq!(
            lod2_max_ratio(&k, piece(id)),
            LANE_CUT_LOD2_MAX_RATIO,
            "{id}"
        );
    }
    for id in [
        "meadow_base",
        "river_edge",
        "road_straight_center",
        "river_bend_center",
    ] {
        assert_eq!(lod2_max_ratio(&k, piece(id)), LOD2_MAX_RATIO, "{id}");
    }
    let bend = piece("river_bend_base");
    let (doc, _, _) = plain(&k, bend).unwrap();
    let t = |i: usize| tris(&mesh_named(&doc, &format!("river_bend_base_lod{i}")).unwrap()) as f32;
    let ratio = t(2) / t(0);
    assert!(
        ratio > LOD2_MAX_RATIO && ratio <= LANE_CUT_LOD2_MAX_RATIO,
        "{ratio}"
    );
}

/// The 2,500-tri LOD0 ceiling reaches the river dressing props only: every listed id is a prop in
/// the runtime kit, other props keep 1,500 and other kinds keep their own.
#[test]
fn river_dressing_lod0_budget_is_scoped_to_its_props() {
    let k = kits()
        .into_iter()
        .find(|k| {
            k.manifest
                .pieces
                .iter()
                .any(|p| p.id == "river_straight_center")
        })
        .expect("runtime kit holds river_straight_center");
    let piece = |id: &str| k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
    for id in RIVER_DRESSING_PROPS {
        assert_eq!(piece(id).kind, PieceKind::Prop, "{id}");
        assert_eq!(lod0_budget(piece(id)), RIVER_DRESSING_LOD0_MAX, "{id}");
    }
    for p in &k.manifest.pieces {
        if !RIVER_DRESSING_PROPS.contains(&p.id.as_str()) {
            assert_eq!(lod0_budget(p), p.kind.lod0_budget(), "{}", p.id);
        }
    }
    assert_eq!(lod0_budget(piece("ocean_center")), 1_500);
    assert_eq!(lod0_budget(piece("river_road_bridge_center")), 4_000);
}

/// A rotor's triangles count toward its LOD (PRD §7.2): the windmill's LODs are its body plus its
/// rotor, so a LOD0 ceiling that the body alone meets fails once the rotor counts, and a rotor
/// that drags a coarse LOD over its ratio fails too.
#[test]
fn rotor_tris_count_toward_the_lod_budget() {
    let k = kits().pop().unwrap();
    let p = k
        .manifest
        .pieces
        .iter()
        .find(|p| p.id == "windmill")
        .expect("runtime kit holds the windmill");
    let (doc, buffers, _) = plain(&k, p).unwrap();
    let body = |i| tris(&mesh_named(&doc, &lod_mesh_name(&p.id, i)).unwrap());
    let rotors: Vec<_> = (0..3)
        .map(|i| mesh_named(&doc, &rotor_mesh_name(&p.id, i)))
        .collect();
    let rotor = |i: usize| tris(rotors[i].as_ref().unwrap());
    assert!(rotor(0) > 0 && rotor(1) > 0 && rotor(2) > 0);
    assert_eq!(rotor_errors(&p.id, &rotors, &buffers), Vec::<String>::new());
    let t = [0, 1, 2].map(|i| Some(body(i) + rotor(i)));
    assert_eq!(
        lod_budget_errors(&p.id, t, lod0_budget(p), LOD2_MAX_RATIO),
        Vec::<String>::new()
    );
    let e = lod_budget_errors(&p.id, t, body(0), LOD2_MAX_RATIO);
    assert!(e.iter().any(|m| m.contains("LOD0")), "{e:?}");
    let heavy = [t[0], t[1], Some(body(2) + t[0].unwrap())];
    let e = lod_budget_errors(&p.id, heavy, lod0_budget(p), LOD2_MAX_RATIO);
    assert!(e.iter().any(|m| m.contains("LOD2")), "{e:?}");
    // A rotor in only some LODs, or off its axle, fails.
    let partial = [rotors[0].clone(), None, rotors[2].clone()];
    assert!(
        rotor_errors(&p.id, &partial, &buffers)
            .iter()
            .any(|m| m.contains("windmill_rotor_lod1 missing"))
    );
    assert!(
        axle_offset(&[
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [3.0, 0.0, 2.0],
            [-1.0, 0.0, -2.0]
        ]) < 1e-6
    );
    assert!(axle_offset(&[[0.0, 1.0, 0.0], [0.0, -0.9, 0.0]]) > ROTOR_AXLE_TOL);
}

/// Joints are scoped by network kind: the river pieces meet river_edge at its inner end, the road
/// centres meet only the road edge pieces, and `same_section` absorbs one 0.1 mm unit.
#[test]
fn joints_are_scoped_by_network_kind() {
    assert!(same_section(&[(0, 5), (10, 7)], &[(1, 4), (10, 8)]));
    assert!(!same_section(&[(0, 5)], &[(2, 5)]));
    let k = kits()
        .into_iter()
        .find(|k| k.manifest.pieces.iter().any(|p| p.id == "river_edge"))
        .expect("runtime kit holds river_edge");
    let piece = |id: &str| k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
    assert_eq!(edge_kind(&k, piece("river_edge")).as_deref(), Some("river"));
    assert_eq!(
        edge_kind(&k, piece("road_straight_edge")).as_deref(),
        Some("road")
    );
    assert!((joint_x(&k, piece("road_straight_edge")) - 6.0).abs() < 1e-4);
    assert!((joint_x(&k, piece("river_edge")) - (APOTHEM - 1.0)).abs() < 1e-3);
    assert_eq!(check_center_edge_joint(&k), Vec::<String>::new());
}

#[test]
fn center_lanes_come_from_the_tiles() {
    assert_eq!(center_lanes("road_straight_center"), vec![0, 3]);
    assert_eq!(center_lanes("road_bend_120_center"), vec![0, 2]);
    assert_eq!(center_lanes("road_bend_60_center"), vec![0, 1]);
    assert_eq!(center_lanes("road_junction_center"), vec![0, 2, 4]);
    assert_eq!(center_lanes("not_a_piece"), vec![0, 3]);
}

/// A lane-socketed centre that no tile names yet (the rail bend, ahead of its Rail socket and
/// tile) is checked at the edges its own sockets declare, not the [0, 3] fallback, which would
/// fail its lane-1 socket. Tile-named centres keep the tile's lanes.
#[test]
fn untiled_lane_socket_centre_is_checked_at_its_declared_edges() {
    let k = kits()
        .into_iter()
        .find(|k| {
            k.manifest
                .pieces
                .iter()
                .any(|p| p.id == "rail_bend_60_center")
        })
        .expect("runtime kit holds rail_bend_60_center");
    let piece = |id: &str| k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
    let bend = piece("rail_bend_60_center");
    assert!(center_tile(&bend.id).is_none());
    assert_eq!(center_lanes(&bend.id), vec![0, 3]);
    assert_eq!(lane_socket_edges(&k, bend), BTreeSet::from([0, 1]));
    assert_eq!(joint_lanes(&k, bend), vec![0, 1]);
    assert_eq!(joint_lanes(&k, piece("rail_straight_center")), vec![0, 3]);
    // Tile-named centres: the tile decides, declared sockets or not.
    assert_eq!(joint_lanes(&k, piece("house_pad")), vec![0]);
    assert_eq!(joint_lanes(&k, piece("road_bend_60_center")), vec![0, 1]);
    assert_eq!(check_center_edge_joint(&k), Vec::<String>::new());
}

#[test]
fn edge_section_must_be_symmetric_and_lod_invariant() {
    let sym = vec![(0, -30_000), (0, 30_000), (800, 0)];
    assert!(section_errors("e", &[sym.clone(), sym.clone(), sym.clone()]).is_empty());
    let skew = vec![(0, -30_000), (0, 31_000), (800, 0)];
    assert!(section_errors("e", &[skew])[0].contains("not mirror-symmetric"));
    let lod2 = vec![(0, -30_000), (0, 30_000)];
    assert!(section_errors("e", &[sym.clone(), sym, lod2])[0].contains("differs across LODs"));
    assert!(section_errors("e", &[vec![]])[0].contains("no vertices"));
}

#[test]
fn ibl_cubemaps_are_valid() {
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo().join("assets-source/lookdev/lookdev.json")).unwrap(),
    )
    .unwrap();
    for (name, size) in [("noon.specular.ktx2", 256), ("noon.diffuse.ktx2", 32)] {
        let p = repo().join("assets/kit/p1/ibl").join(name);
        let b = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let i = ktx2_info(&b).expect("KTX2");
        assert_eq!(
            (i.vk_format, i.faces, i.width, i.height, i.supercompression),
            (VK_R16G16B16A16_SFLOAT, 6, size, size, 0),
            "{name}"
        );
        assert_eq!(
            sha(&p).as_deref(),
            meta["ibl"]["files"][name].as_str(),
            "{name}: SHA-256 differs from assets-source/lookdev/lookdev.json (re-run make lookdev-ibl)"
        );
        assert_eq!(
            i.levels,
            if name.contains("specular") {
                full_mip_count(i.width, i.height)
            } else {
                1
            }
        );
    }
}

/// A synthetic one-map tileable set in a scratch kit, with provenance that matches it.
fn scratch_material(tag: &str) -> (TempDir, Kit, serde_json::Value) {
    let dir = temp(tag);
    let json = r#"{ "version": 1, "pieces": [],
        "materials": { "ts": { "tileM": 2.0, "maps": { "albedo": "p1/materials/ts/ts.albedo" } } } }"#;
    std::fs::create_dir_all(dir.join("p1/materials/ts")).unwrap();
    let mut derived = vec![];
    let mut encodes = vec![];
    for (fam, family, vk) in [
        (TextureFamily::Bc7, "bc7", Some(VK_BC7_SRGB)),
        (TextureFamily::Astc, "astc", Some(VK_ASTC_4X4_SRGB)),
        (TextureFamily::Png, "png", None),
    ] {
        let path = dir.join(map_file("p1/materials/ts/ts.albedo", fam));
        std::fs::write(&path, format!("{family}-bytes")).unwrap();
        derived.push(serde_json::json!({"path": path.to_string_lossy(), "sha256": sha(&path)}));
        encodes.push(serde_json::json!({"channel": "albedo", "family": family, "transfer": "srgb", "vkFormat": vk}));
    }
    let generator = dir.join("mat_ts.py");
    std::fs::write(&generator, "import bpy\n").unwrap();
    let prov = serde_json::json!({
        "setId": "ts", "kind": "tileable", "route": "scripted",
        "generator": {"path": generator.to_string_lossy(), "sha256": sha(&generator)},
        "generatorDeps": [],
        "authoringAgent": {"harness": "claude-code", "model": "claude-opus-5-5"},
        "blender": "5.2.2", "toolVersions": {}, "bake": {"tileM": 2.0},
        "bakeReport": {"sha256": "cd"}, "compressReport": {"sha256": "ef"},
        "compression": {"budgets": {"class": "tileable"}, "textureEncodes": encodes},
        "derivedFiles": derived, "license": "MIT", "validation": {"pass": true}
    });
    let kit = Kit {
        root: dir.to_path_buf(),
        manifest: parse_manifest(json).unwrap(),
        requires_refs: true,
    };
    (dir, kit, prov)
}

#[test]
fn gate_catches_stale_or_incomplete_material_provenance() {
    let (dir, kit, good) = scratch_material("matprov");
    let path = dir.join("provenance.json");
    let check = |v: &serde_json::Value| {
        std::fs::write(&path, v.to_string()).unwrap();
        check_material_provenance(&kit, "ts", &path)
    };
    let has = |e: Vec<String>, want: &str| {
        assert!(e.iter().any(|m| m.contains(want)), "want {want:?} in {e:?}");
    };
    assert_eq!(check(&good), Vec::<String>::new());
    let mut v = good.clone();
    v["derivedFiles"][0]["sha256"] = "00".into();
    has(check(&v), "changed since provenance (stale)");
    let mut v = good.clone();
    v["derivedFiles"].as_array_mut().unwrap().remove(1);
    has(
        check(&v),
        "ts.albedo.astc.ktx2 not in provenance derivedFiles",
    );
    let mut v = good.clone();
    v["validation"]["pass"] = false.into();
    has(check(&v), "validation.pass is not true");
    let mut v = good.clone();
    v["setId"] = "other".into();
    has(check(&v), "provenance setId");
    let mut v = good.clone();
    v.as_object_mut().unwrap().remove("generator");
    has(check(&v), "provenance lacks generator");
    let mut v = good.clone();
    v["compression"]["textureEncodes"][2]["transfer"] = "linear".into();
    has(check(&v), "albedo png transfer");
    std::fs::remove_file(&path).unwrap();
    has(
        check_material_provenance(&kit, "ts", &path),
        "provenance missing",
    );
}

#[test]
fn gate_budget_checks_unreferenced_material_sets() {
    // The scratch kit lists set "ts" but has no pieces, so no slot references it.
    let (_dir, kit, _) = scratch_material("matbudget");
    assert!(kit.manifest.pieces.is_empty());
    let e = material_texture_errors(&kit, 1);
    assert!(
        e.iter()
            .any(|m| m.contains("tileable set ts Bc7") && m.contains("> 1")),
        "{e:?}"
    );
    assert!(e.iter().any(|m| m.contains("is not KTX2")), "{e:?}");
}

/// One LOD of a tree: (position, TEXCOORD_1, fringe) per vertex, None where a primitive lacks it.
fn tree_lod(min_u: f32) -> Vec<WindVert> {
    vec![
        ([0.0, -0.2, 0.0], Some([min_u, 0.3]), false),
        ([0.5, -0.2, 0.1], Some([0.0, 0.7]), false),
        ([0.3, 0.4, 0.0], Some([0.04, 0.1]), false),
        ([0.0, 6.0, 0.0], Some([0.6, 0.2]), false),
        ([3.0, 11.0, 1.0], Some([1.0, 0.95]), true),
        ([2.0, 9.0, 3.0], Some([0.8, 0.45]), true),
    ]
}

#[test]
fn car_paint_mask_contract() {
    let ok = || -> Vec<PaintPrim> {
        vec![
            (
                "c_body".into(),
                Some(vec![[1.0, 1.0, 1.0, 1.0], [0.1, 0.1, 0.1, 0.0]]),
            ),
            ("c_glass".into(), Some(vec![[0.1, 0.1, 0.2, 0.0]])),
            ("c_tyre".into(), Some(vec![[0.05, 0.05, 0.05, 0.0]])),
        ]
    };
    assert_eq!(
        paint_mask_errors("c", "c_lod0", &ok()),
        Vec::<String>::new()
    );
    let mut missing = ok();
    missing[1].1 = None;
    assert_eq!(
        paint_mask_errors("c", "c_lod1", &missing),
        vec!["c: c_lod1 c_glass has no COLOR_0 (paint mask)".to_string()]
    );
    let mut soft = ok();
    soft[0].1 = Some(vec![[1.0, 1.0, 1.0, 0.5]]);
    let e = paint_mask_errors("c", "c_lod0", &soft);
    assert!(e.iter().any(|m| m.contains("not binary")), "{e:?}");
    assert!(e.iter().any(|m| m.contains("no painted vertex")), "{e:?}");
    let mut painted_glass = ok();
    painted_glass[1].1 = Some(vec![[1.0, 1.0, 1.0, 1.0]]);
    let e = paint_mask_errors("c", "c_lod2", &painted_glass);
    assert!(
        e.iter().any(|m| m.contains("c_glass has 1 painted")),
        "{e:?}"
    );
}

#[test]
fn tree_wind_uv_contract() {
    assert_eq!(
        wind_errors("t", "t_lod0", &tree_lod(0.0)),
        Vec::<String>::new()
    );
    let mut missing = tree_lod(0.0);
    missing[3].1 = None;
    assert_eq!(
        wind_errors("t", "t_lod1", &missing),
        vec!["t: t_lod1 has a primitive without TEXCOORD_1 (wind)".to_string()]
    );
    let mut out = tree_lod(0.0);
    out[4].1 = Some([1.2, 0.5]);
    let e = wind_errors("t", "t_lod0", &out);
    assert!(e.iter().any(|m| m.contains("outside [0, 1]")), "{e:?}");
    let e = wind_errors("t", "t_lod2", &tree_lod(0.3));
    assert!(
        e.iter()
            .any(|m| m.contains("lowest trunk vertex has wind U 0.3")),
        "{e:?}"
    );
    // A trunk base swaying: a vertex 0.4 m up with U 0.3.
    let mut base = tree_lod(0.0);
    base[2].1 = Some([0.3, 0.1]);
    let e = wind_errors("t", "t_lod0", &base);
    assert!(
        e.iter().any(|m| m.contains("below y 0.5 m (trunk base")),
        "{e:?}"
    );
    // A stiff canopy fringe, and a tree with no fringe cards at all.
    let mut stiff = tree_lod(0.0);
    stiff[4].1 = Some([0.1, 0.5]);
    stiff[5].1 = Some([0.2, 0.5]);
    let e = wind_errors("t", "t_lod0", &stiff);
    assert!(
        e.iter().any(|m| m.contains("fringe median wind U")),
        "{e:?}"
    );
    let bare: Vec<WindVert> = tree_lod(0.0)
        .into_iter()
        .map(|(p, w, _)| (p, w, false))
        .collect();
    let e = wind_errors("t", "t_lod0", &bare);
    assert!(e.iter().any(|m| m.contains("(0 fringe vertices)")), "{e:?}");
}

#[test]
fn wind_slots_carry_their_attribute() {
    let prim =
        |slot: &str, amp: Option<f32>, uv: Option<Vec<[f32; 2]>>| (slot.to_string(), amp, uv);
    let ok = [
        prim("reed", Some(0.5), Some(vec![[0.0, 0.3], [0.8, 0.3]])),
        prim("stone", Some(0.5), Some(vec![[0.0, 1.0]])),
        prim("lily", None, None),
    ];
    assert_eq!(wind_slot_errors("r", 0, &ok), Vec::<String>::new());
    let missing = [prim("reed", Some(0.5), None)];
    assert_eq!(
        wind_slot_errors("r", 1, &missing),
        vec!["r: r_lod1 wind slot reed has no TEXCOORD_1 (wind)".to_string()]
    );
    let out = [prim("reed", Some(0.5), Some(vec![[1.2, 0.0]]))];
    assert!(wind_slot_errors("r", 0, &out)[0].contains("outside [0, 1]"));
    let still = [prim("reed", Some(0.5), Some(vec![[0.0, 0.5]]))];
    assert!(wind_slot_errors("r", 0, &still)[0].contains("never sway"));
    assert!(wind_slot_errors("r", 2, &still).is_empty());
    let mixed = [
        prim("reed", Some(0.5), Some(vec![[0.5, 0.5]])),
        prim("stone", Some(0.4), Some(vec![[0.0, 0.5]])),
    ];
    assert!(wind_slot_errors("r", 0, &mixed)[0].contains("differ in amplitude"));
    let gale = [prim("reed", Some(2.0), Some(vec![[0.5, 0.5]]))];
    assert!(wind_slot_errors("r", 0, &gale)[0].contains("outside (0, 1]"));
}

/// Two 1 m² quads (four triangles) whose UV0 covers `uv_side`² each.
fn quads(uv_side: f32) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let pos = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let uv = vec![
        [0.0, 0.0],
        [uv_side, 0.0],
        [uv_side, uv_side],
        [0.0, uv_side],
    ];
    (pos, uv, vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3])
}

#[test]
fn texel_density_is_checked_per_material() {
    // 1/16 of a 1024 px texture over 1 m: 256 px/m on every face.
    let (pos, uv, idx) = quads(0.25);
    let d = face_densities(&pos, &uv, &idx, 1024.0);
    assert_eq!(d.len(), 4);
    assert!(d.iter().all(|x| (x - 256.0).abs() < 1e-3), "{d:?}");
    assert!(texel_errors("t", "t_leaves", &d).is_empty());
    // A tileable set on UV0 counts through its tileUvScale: same UVs at scale 0.5 halve the density.
    let low = face_densities(&pos, &uv, &idx, 0.5 * 1024.0);
    let e = texel_errors("t", "t_bark", &low);
    assert_eq!(
        e,
        vec!["t: t_bark median texel density 128.0 px/m < 256 (per material)".to_string()]
    );
    assert_eq!(median(&[3.0, 1.0, 2.0, 100.0]), 2.5);
    // A UV0 slot without an albedo to measure against is an error, not a skip.
    let (k, mut p) = fixture();
    p.maps.remove(&Channel::Albedo);
    let unique = Slot {
        unique: true,
        ..Default::default()
    };
    let err = uv0_texture_px(&k, &p, "fixture_x", &unique)
        .unwrap()
        .unwrap_err();
    assert!(
        err.contains("unique slot fixture_x but the piece has no albedo"),
        "{err}"
    );
    assert!(uv0_texture_px(&k, &p, "fixture_cap", &p.slots["fixture_cap"]).is_none());
}

/// A mesh's triangles in the runtime XZ plane: ([x, y, z] corners, slot name).
fn triangles_with_slot(
    m: &gltf::Mesh,
    buffers: &[gltf::buffer::Data],
) -> Vec<([[f32; 3]; 3], String)> {
    let mut out = vec![];
    for prim in m.primitives() {
        let name = prim.material().name().unwrap_or_default().to_string();
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let pos: Vec<[f32; 3]> = r.read_positions().map(|i| i.collect()).unwrap_or_default();
        let idx: Vec<u32> = r
            .read_indices()
            .map(|i| i.into_u32().collect())
            .unwrap_or_default();
        for &[a, b, c] in idx.as_chunks::<3>().0 {
            out.push((
                [pos[a as usize], pos[b as usize], pos[c as usize]],
                name.clone(),
            ));
        }
    }
    out
}

/// Height and slot of the topmost upward triangle over runtime (x, z), if any.
fn surface_at(tris: &[([[f32; 3]; 3], String)], x: f32, z: f32) -> Option<(f32, &str)> {
    let mut best: Option<(f32, &str)> = None;
    for (t, name) in tris {
        let [a, b, c] = *t;
        let d = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
        if d.abs() < 1e-9 {
            continue; // vertical (skirt) or degenerate
        }
        let u = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / d;
        let v = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / d;
        if u < -1e-5 || v < -1e-5 || u + v > 1.0 + 1e-5 {
            continue;
        }
        let y = a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]);
        if best.is_none_or(|(h, _)| y > h) {
            best = Some((y, name.as_str()));
        }
    }
    best
}

/// forest_forest_edge_e<k> over forest_base (scripts/blender/kit/_forest_edge.py): variant k sits at
/// the base's local edge k, so over its slice core (inside both corner rays, before the inner dive at
/// 5.5 m) it must clear that wedge's cap, in the same LOD, wherever the cap shows its grass rim or
/// mossy band, and float no more than 3 cm over its litter floor (no hump). Each variant sees only
/// its own wedge: the cap is rotated by the runtime placement of edge k.
fn forest_edge_clearance(kit: &Kit, k: u8) -> Vec<String> {
    let id = format!("forest_forest_edge_e{k}");
    let find = |id: &str| kit.manifest.pieces.iter().find(|p| p.id == id);
    let (Some(edge), Some(base)) = (find(&id), find("forest_base")) else {
        return vec![format!("{id} or forest_base missing from the manifest")];
    };
    let (Ok((ed, eb, _)), Ok((bd, bb, _))) = (plain(kit, edge), plain(kit, base)) else {
        return vec![format!("{id} / forest_base: GLB did not load")];
    };
    let grass = |s: &str| s == "forest_base_rim" || s == "forest_base_edge";
    let tan30 = 30f32.to_radians().tan();
    let (s, c) = (60.0 * k as f32).to_radians().sin_cos();
    let mut e = vec![];
    for lod in 0..3 {
        let (Some(em), Some(bm)) = (
            mesh_named(&ed, &format!("{id}_lod{lod}")),
            mesh_named(&bd, &format!("forest_base_lod{lod}")),
        ) else {
            e.push(format!("{id} / forest_base: LOD{lod} mesh missing"));
            continue;
        };
        let (et, bt) = (triangles_with_slot(&em, &eb), triangles_with_slot(&bm, &bb));
        let (mut worst, mut float) = (f32::MAX, f32::MIN);
        for i in 0..55 {
            let x = APOTHEM - 0.05 - 0.1 * i as f32;
            for j in -90..=90 {
                let z = 0.1 * j as f32;
                if z.abs() > x * tan30 - 0.02 {
                    continue;
                }
                let Some((over, _)) = surface_at(&et, x, z) else {
                    e.push(format!("{id} LOD{lod}: hole at ({x}, {z})"));
                    continue;
                };
                let Some((h, slot)) = surface_at(&bt, x * c - z * s, x * s + z * c) else {
                    continue;
                };
                if grass(slot) {
                    worst = worst.min(over - h);
                } else {
                    float = float.max(over - h);
                }
            }
        }
        if worst == f32::MAX {
            e.push(format!("{id} LOD{lod}: no forest_base rim under the slice"));
        } else if worst < 0.002 {
            e.push(format!(
                "{id} LOD{lod}: {worst:.4} m over forest_base's rim (needs >= 0.002)"
            ));
        }
        if float > 0.03 {
            e.push(format!(
                "{id} LOD{lod}: floats {float:.4} m over forest_base's floor (hump; at most 0.03)"
            ));
        }
    }
    e
}

/// Each forest edge variant hides its own wedge's grass rim in every LOD with no hump over the
/// floor, and every variant has the same border section (the 17 edge-0 ring points at one height),
/// so a neighbour meets it whatever its own variant is.
#[test]
fn forest_edge_covers_the_base_rim() {
    let k = kits()
        .into_iter()
        .find(|k| {
            k.manifest
                .pieces
                .iter()
                .any(|p| p.id == "forest_forest_edge_e0")
        })
        .expect("runtime kit holds forest_forest_edge_e0");
    let mut e = vec![];
    let mut first: Option<Vec<(i64, i64)>> = None;
    for v in 0..6u8 {
        e.extend(forest_edge_clearance(&k, v));
        let id = format!("forest_forest_edge_e{v}");
        let p = k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
        assert_eq!(edge_kind(&k, p).as_deref(), Some("forest"), "{id}");
        for sec in lod_sections(&k, p, APOTHEM) {
            assert_eq!(sec.len(), 17, "{id} border section {sec:?}");
            assert!(sec.iter().all(|&(y, _)| y == sec[0].0), "{id} {sec:?}");
            let want = first.get_or_insert_with(|| sec.clone());
            assert!(
                same_section(&sec, want),
                "{id}: border section differs from e0's"
            );
        }
    }
    fail(e);
    assert_eq!(check_center_edge_joint(&k), Vec::<String>::new());
}
