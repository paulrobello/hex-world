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
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
    let mut ring: Vec<[f32; 2]> = vec![];
    for (p, n) in v {
        if dist_to_border(p[0], p[2]).abs() < 1e-3
            && p[1].abs() < 1e-3
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
    lod0: &gltf::Mesh,
    buffers: &[gltf::buffer::Data],
) -> Vec<String> {
    let mut per: std::collections::BTreeMap<String, Vec<f32>> = Default::default();
    let mut e = vec![];
    for prim in lod0.primitives() {
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
        .map(|i| mesh_named(&doc, &format!("{}_lod{i}", p.id)))
        .collect();
    let Some(lod0) = &lods[0] else {
        return vec![format!("{}: mesh {}_lod0 missing", p.id, p.id)];
    };
    let t0 = tris(lod0);
    if t0 > p.kind.lod0_budget() {
        e.push(format!(
            "{}: LOD0 {t0} tris > {}",
            p.id,
            p.kind.lod0_budget()
        ));
    }
    for (i, max) in [(1, LOD1_MAX_RATIO), (2, LOD2_MAX_RATIO)] {
        match &lods[i] {
            None => e.push(format!("{}: mesh {}_lod{i} missing", p.id, p.id)),
            Some(m) if tris(m) as f32 > max * t0 as f32 => e.push(format!(
                "{}: LOD{i} {} tris > {max} x LOD0 {t0}",
                p.id,
                tris(m)
            )),
            _ => {}
        }
    }
    for m in lods.iter().flatten() {
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
    e.extend(check_texel_density(kit, p, lod0, &buffers));
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
    if !doc
        .nodes()
        .any(|n| n.name() == Some(&format!("{}_collider", p.id)))
    {
        e.push(format!("{}: node {}_collider missing (D14)", p.id, p.id));
    }
    if p.kind.touches_border() {
        let rings: Vec<_> = lods
            .iter()
            .flatten()
            .map(|m| border_ring(&positions(m, &buffers)))
            .collect();
        e.extend(ring_errors(&p.id, &rings, p.kind == PieceKind::Base));
    }
    if p.kind == PieceKind::Base {
        let v = positions(lod0, &buffers);
        if let Some((p_bad, n_bad)) = v.iter().find(|(q, n)| {
            dist_to_border(q[0], q[2]).abs() < 1e-3
                && q[1].abs() < 1e-3
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
    let Ok((doc, buffers, _)) = plain(kit, p) else {
        return vec![];
    };
    (0..3)
        .filter_map(|i| mesh_named(&doc, &format!("{}_lod{i}", p.id)))
        .map(|m| {
            let mut sec: Vec<(i64, i64)> = positions(&m, &buffers)
                .iter()
                .filter(|(q, _)| (q[0] - x).abs() < 1e-3)
                .map(|(q, _)| ((q[1] * 1e4).round() as i64, (q[2] * 1e4).round() as i64))
                .collect();
            sec.sort();
            sec.dedup();
            sec
        })
        .collect()
}

/// Road centre/edge joint. `center[i]` is LOD i's (x = +6, x = -6) sections and
/// `edge[i]` the edge piece's LOD i section at x = +6. The -6 joint meets a rot-3 edge piece, whose
/// section there is the edge section mirrored (z -> -z).
fn joint_errors(
    id: &str,
    center: &[(Vec<(i64, i64)>, Vec<(i64, i64)>)],
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
    for (i, ((pos, neg), want)) in center.iter().zip(edge).enumerate() {
        let mut mirror: Vec<_> = want.iter().map(|&(y, z)| (y, -z)).collect();
        mirror.sort();
        if pos != want {
            e.push(format!(
                "{id}: LOD{i} section at x = +6 differs from the edge piece's"
            ));
        }
        if neg != &mirror {
            e.push(format!(
                "{id}: LOD{i} section at x = -6 differs from the mirrored edge piece's"
            ));
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

/// `lod_sections` at x = `x` after rotating the piece so its edge `k` faces +x (edge 0).
fn lod_sections_at_edge(kit: &Kit, p: &PieceEntry, x: f32, k: u64) -> Vec<Vec<(i64, i64)>> {
    let Ok((doc, buffers, _)) = plain(kit, p) else {
        return vec![];
    };
    let (s, c) = (60.0 * k as f32).to_radians().sin_cos();
    (0..3)
        .filter_map(|i| mesh_named(&doc, &format!("{}_lod{i}", p.id)))
        .map(|m| {
            let mut sec: Vec<(i64, i64)> = positions(&m, &buffers)
                .iter()
                .map(|(q, _)| (q[0] * c + q[2] * s, q[1], -q[0] * s + q[2] * c))
                .filter(|(rx, _, _)| (rx - x).abs() < 1e-3)
                .map(|(_, y, rz)| ((y * 1e4).round() as i64, (rz * 1e4).round() as i64))
                .collect();
            sec.sort();
            sec.dedup();
            sec
        })
        .collect()
}

/// A centre piece that declares `socket-lane-*` nodes joins the network only at those edges (a
/// building pad's driveway, a bend): each declared edge's x = +6 section must equal every edge
/// piece's, in every LOD.
fn lane_joint_errors(kit: &Kit, c: &PieceEntry, edges: &BTreeSet<u64>) -> Vec<String> {
    let mut e = vec![];
    for &k in edges {
        let secs = lod_sections_at_edge(kit, c, 6.0, k);
        if secs.len() != 3 || secs.iter().any(|s| s.is_empty()) {
            e.push(format!(
                "{}: a LOD has no vertices on the edge-{k} joint",
                c.id
            ));
        }
        for edge in kit
            .manifest
            .pieces
            .iter()
            .filter(|p| p.kind == PieceKind::Edge)
        {
            let want = lod_sections(kit, edge, 6.0);
            for (i, (got, want)) in secs.iter().zip(&want).enumerate() {
                if got != want {
                    e.push(format!(
                        "{} / {}: LOD{i} section at the edge-{k} joint differs from the edge piece's",
                        c.id, edge.id
                    ));
                }
            }
        }
    }
    e
}

/// Every centre piece against every edge piece in the same kit, at the x = ±6 joint. A centre that
/// declares lane sockets is checked at those edges instead (`lane_joint_errors`).
fn check_center_edge_joint(kit: &Kit) -> Vec<String> {
    let half = 6.0;
    let mut e = vec![];
    for c in kit
        .manifest
        .pieces
        .iter()
        .filter(|p| p.kind == PieceKind::Center)
    {
        let edges = lane_socket_edges(kit, c);
        if !edges.is_empty() {
            e.extend(lane_joint_errors(kit, c, &edges));
            continue;
        }
        let pos = lod_sections(kit, c, half);
        let neg = lod_sections(kit, c, -half);
        if pos.iter().chain(&neg).any(|s| s.is_empty()) {
            e.push(format!("{}: a LOD has no vertices on x = ±6", c.id));
        }
        let center: Vec<_> = pos.into_iter().zip(neg).collect();
        for edge in kit
            .manifest
            .pieces
            .iter()
            .filter(|p| p.kind == PieceKind::Edge)
        {
            let id = format!("{} / {}", c.id, edge.id);
            e.extend(joint_errors(&id, &center, &lod_sections(kit, edge, half)));
        }
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
    // The provenance must record the class the maps were actually compressed under.
    if let Some(prov) = std::fs::read_to_string(repo().join(&p.provenance))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        && prov["compression"]["budgets"]["class"] != name
    {
        return Err(format!(
            "{}: provenance budget class {} != compress report {name} (re-run kit-provenance)",
            p.id, prov["compression"]["budgets"]["class"]
        ));
    }
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
/// and `generatorDeps` [{path, sha256}] are present, every recorded hash matches the committed file
/// (an edit to a generator or helper needs a rebuild and new provenance), and `generatorDeps` lists
/// exactly the helpers the generator imports.
fn generator_errors(id: &str, v: &serde_json::Value) -> Vec<String> {
    let (Some(gen_rel), Some(gen_sha)) = (
        v["generator"]["path"].as_str(),
        v["generator"]["sha256"].as_str(),
    ) else {
        return vec![format!("{id}: provenance lacks generator {{path, sha256}}")];
    };
    let mut e = vec![];
    match sha(&repo().join(gen_rel)) {
        None => e.push(format!("{id}: generator {gen_rel} missing")),
        Some(h) if h != gen_sha => e.push(format!(
            "{id}: generator {gen_rel} changed since provenance (stale; rebuild)"
        )),
        _ => {}
    }
    let Some(deps) = v["generatorDeps"].as_array() else {
        e.push(format!("{id}: provenance lacks generatorDeps"));
        return e;
    };
    let mut listed = BTreeSet::new();
    for d in deps {
        let rel = d["path"].as_str().unwrap_or_default();
        listed.insert(repo_rel(&repo().join(rel)));
        match sha(&repo().join(rel)) {
            None => e.push(format!("{id}: generator dep {rel} missing")),
            Some(h) if h != d["sha256"].as_str().unwrap_or_default() => e.push(format!(
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
    e
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
            if !g
                .named_meshes
                .contains_key(format!("{id}_lod{lod}").as_str())
            {
                e.push(format!(
                    "{id}: bevy mesh {id}_lod{lod} missing (runtime looks meshes up by name)"
                ));
            }
        }
        for (name, nh) in &g.named_nodes {
            if name.starts_with("socket-lane-") || name.starts_with("anchor-prop-") {
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

#[test]
fn geometry_budgets_lods_sockets_border() {
    let mut e = vec![];
    for k in kits() {
        for p in &k.manifest.pieces {
            e.extend(check_geometry(&k, p));
            e.extend(check_edge_profile(&k, p));
        }
        e.extend(check_center_edge_joint(&k));
    }
    fail(e);
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
    let (_, p) = fixture();
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
    let err = budget_class(&q).unwrap_err();
    assert!(err.contains("provenance budget class"), "{err}");
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
/// recorded and re-hashed, so editing either without a rebuild and new provenance fails the gate,
/// and a provenance that omits an imported helper (or lists one no longer imported) fails too.
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
    let mut v = good.clone();
    v["generator"]["sha256"] = "00".into();
    has(&v, "gen.py changed since provenance (stale; rebuild)");
    let mut v = good.clone();
    v["generatorDeps"][1]["sha256"] = "00".into();
    has(&v, "generator dep");
    has(&v, "_deep.py changed since provenance");
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

/// The centre piece's sections at x = +6 and x = -6 must equal the edge piece's
/// section at x = 6 (the -6 joint meets a rot-3 edge piece, so it compares against the mirror).
#[test]
fn lane_socket_centre_is_checked_at_its_declared_edges() {
    let k = kits()
        .into_iter()
        .find(|k| k.manifest.pieces.iter().any(|p| p.id == "house_pad"))
        .expect("runtime kit holds house_pad");
    let piece = |id: &str| k.manifest.pieces.iter().find(|p| p.id == id).unwrap();
    let pad = piece("house_pad");
    assert_eq!(lane_socket_edges(&k, pad), BTreeSet::from([0]));
    assert!(lane_joint_errors(&k, pad, &BTreeSet::from([0])).is_empty());
    // The pad has no road at edge 3, so the rotated joint check must fail there.
    let e = lane_joint_errors(&k, pad, &BTreeSet::from([3]));
    assert!(e.iter().any(|m| m.contains("edge-3")), "{e:?}");
    // The straight centre, turned so its x = -6 end faces edge 0, still meets the edge section.
    let straight = piece("road_straight_center");
    assert!(lane_joint_errors(&k, straight, &BTreeSet::from([0, 3])).is_empty());
}

#[test]
fn center_sections_must_meet_the_edge_section() {
    let edge = vec![(0, -30_000), (0, 30_000), (800, 5)];
    let mirror = vec![(0, -30_000), (0, 30_000), (800, -5)];
    let ok = (edge.clone(), mirror.clone());
    assert!(
        joint_errors(
            "c",
            &[ok.clone(), ok.clone()],
            &[edge.clone(), edge.clone()]
        )
        .is_empty()
    );
    let moved = vec![(0, -30_000), (0, 30_000), (900, 5)];
    let e = joint_errors(
        "c",
        &[ok.clone(), (moved, mirror.clone())],
        &[edge.clone(), edge.clone()],
    );
    assert!(
        e.iter().any(|m| m.contains("LOD1") && m.contains("x = +6")),
        "{e:?}"
    );
    let e = joint_errors(
        "c",
        &[(edge.clone(), edge.clone())],
        std::slice::from_ref(&edge),
    );
    assert!(
        e.iter().any(|m| m.contains("LOD0") && m.contains("x = -6")),
        "{e:?}"
    );
    let e = joint_errors("c", &[ok.clone(), ok], &[edge]);
    assert!(e.iter().any(|m| m.contains("LOD count")), "{e:?}");
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
