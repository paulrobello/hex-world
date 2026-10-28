//! GPU-instanced settled kit parts (card 01a14675's D1 pivot): settled play-world kit parts draw
//! through one storage buffer of 48-byte records per world rebuild instead of one entity per
//! part. Chunk merging duplicated every part's vertices and transiently exceeded wasm32's 4 GiB
//! during the merge wave; instancing keeps memory at unique-piece size plus 48 B per instance.
//! The draw machinery adapts the gate's `instanced.rs` to real kit materials: slot entities
//! carry the `KitMaterial` binding (group 3) and are binned as `NonMesh` items, group 2 binds
//! the record + id buffers (`hex_render::kit_instances`), and the kit shaders' `KIT_INSTANCED`
//! def replaces the per-mesh uniform with the record.
use std::{
    collections::HashMap,
    ops::Range,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use bevy::{
    asset::{UntypedAssetId, uuid::Uuid},
    camera::primitives::{Aabb, Frustum},
    core_pipeline::{
        core_3d::{
            AlphaMask3d, CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey,
        },
        prepass::{
            AlphaMask3dPrepass, Opaque3dPrepass, OpaqueNoLightmap3dBatchSetKey,
            OpaqueNoLightmap3dBinKey, prepass_target_descriptors,
        },
    },
    ecs::system::{
        SystemParamItem,
        lifetimeless::{Read, SRes},
    },
    material::RenderPhaseType,
    math::Affine3A,
    pbr::{
        ExtractedDirectionalLight, LightEntity, MATERIAL_BIND_GROUP_INDEX, MeshPipeline,
        MeshPipelineKey, PreparedMaterial, PrepassPipeline, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
        Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache, ViewLightEntities,
        alpha_mode_pipeline_key,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        erased_render_asset::ErasedRenderAssets,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        material_bind_groups::MaterialBindGroupAllocators,
        mesh::allocator::{MeshAllocator, MeshSlabs},
        mesh::{MeshVertexBufferLayoutRef, RenderMesh, RenderMeshBufferInfo},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedPhaseItem, BinnedRenderPhaseType, DrawFunctions,
            InputUniformIndex, PhaseItem, RenderCommand, RenderCommandResult, SetItemPipeline,
            TrackedRenderPass, ViewBinnedRenderPhases,
        },
        render_resource::{
            binding_types::{storage_buffer_read_only_sized, uniform_buffer_sized},
            *,
        },
        renderer::{RenderDevice, RenderQueue},
        settings::WgpuFeatures,
        sync_world::MainEntity,
        view::ExtractedView,
    },
    shader::ShaderDefVal,
    shape::ViewFrustum,
};
use bytemuck::{Pod, Zeroable};

use crate::kit_material::{KitMaterial, PREPASS_SHADER, SHADER, VERTEX_SHADER};

/// One settled kit part on the GPU: `world = translation · rotation · scale`. Layout matches
/// `hex_render::kit_instances`' `GpuKitInstance` (48 bytes, translation at 16, tag at 28).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuKitInstance {
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub tag: u32,
    pub scale: [f32; 3],
    pub _pad: u32,
}

/// One slot's mesh metadata (decompress AABB + UV ranges), matching `hex_render::kit_instances`'
/// `KitMeshMeta` (80 bytes). One entry per (slot, LOD) draw, each bound by its own bind group.
/// `lod_range` is written by the render world from [`kit_lod_range`] (callers leave it zero).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct KitGpuMeta {
    pub center: [f32; 3],
    pub _pad0: u32,
    pub half: [f32; 3],
    pub _pad1: u32,
    pub uv_a: [f32; 4],
    pub uv_b: [f32; 4],
    pub lod_range: [f32; 4],
}

/// Bytes of one `KitGpuMeta` binding.
const META_SIZE: u64 = std::mem::size_of::<KitGpuMeta>() as u64;

/// Per-cell culling data (one cell per tile, owner A2 Q3 O6): the cell's hex centre, the
/// world-space bounds of every instanced part in it, the bounds of the parts' origins (the points
/// LOD distances are measured to), and the instance range of each slot it holds, as sparse
/// `(slot, range)` entries in slot order. A tile holds a few of the world's slots, so per-tile
/// cells cull close views tile by tile at the cost of the entries they hold, not of every slot.
#[derive(Clone, Debug)]
pub struct KitChunk {
    pub centre: Vec3,
    pub aabb: Aabb,
    pub origins: Aabb,
    pub ranges: Vec<(u32, Range<u32>)>,
}

/// One draw slot: a piece mesh family × material. `mesh[l]` is the family's LOD-`l` mesh and
/// `key` carries the `StandardMaterialKey` bits the pipeline defs read.
#[derive(Clone, Debug)]
pub struct KitSlot {
    pub mesh: [Option<AssetId<Mesh>>; 3],
    /// Indices per LOD for the census/report.
    pub tris: [u32; 3],
    pub material: AssetId<KitMaterial>,
    /// `StandardMaterialKey` bits (cull mode, normal map, depth bias, ...).
    pub key: u64,
    /// The mesh family, numbered from the sorted piece ids: the slot's draw-order key.
    pub family: u32,
    /// The kit piece the family belongs to (the per-piece draw census; rendering ignores it).
    pub piece: Arc<str>,
}

/// The phase bin key of slot `slot`'s LOD `lod` draw (its band draw when `band`), in
/// family-then-LOD order. Bins sort by key within a pipeline, and at equal depth the later draw
/// takes the pixel (the main pass tests `GreaterEqual` against the depth prepass), so the key must
/// not be the mesh `AssetId`: its index follows the order the GLBs finished loading, which differs
/// between runs.
fn bin_key(family: u32, lod: u8, band: bool, slot: u32) -> UntypedAssetId {
    let bits = (u128::from(family) << 64)
        | (u128::from(lod) << 33)
        | (u128::from(band) << 32)
        | u128::from(slot);
    AssetId::<Mesh>::Uuid {
        uuid: Uuid::from_u128(bits),
    }
    .untyped()
}

/// The whole world's instance data, rebuilt on each settle wave and swapped as one `Arc`.
#[derive(Clone, Debug, Default)]
pub struct KitInstanceSetData {
    pub pieces: Vec<GpuKitInstance>,
    pub slots: Vec<KitSlot>,
    pub chunks: Vec<KitChunk>,
    /// One `KitGpuMeta` per (slot, LOD) draw, in `slot * 3 + lod` order.
    pub meta: Vec<KitGpuMeta>,
    pub version: u64,
    /// Each slot's `(chunk, range)` entries in chunk order, the transpose of the chunks' sparse
    /// ranges ([`KitInstanceSetData::index_slots`]).
    pub by_slot: Vec<Vec<(u32, Range<u32>)>>,
}

impl KitInstanceSetData {
    /// Fills [`Self::by_slot`] from the chunks' ranges; the producer calls it once the chunks
    /// and slots are final.
    pub fn index_slots(&mut self) {
        let mut by_slot = vec![vec![]; self.slots.len()];
        for (ci, c) in self.chunks.iter().enumerate() {
            for (slot, r) in &c.ranges {
                if let Some(list) = by_slot.get_mut(*slot as usize) {
                    list.push((ci as u32, r.clone()));
                }
            }
        }
        self.by_slot = by_slot;
    }
}

/// The main world's instance set, extracted to the render world by `Arc`.
#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct KitInstanceSet(pub Arc<KitInstanceSetData>);

/// One (slot, LOD) draw entity. Carries the `KitMaterial` binding; hex-play spawns them. Each
/// (slot, LOD) has two: the whole draw, and the `band` draw that replaces it in a view where any of
/// its parts lies inside a cross-fade band, whose pipelines dither like a per-entity
/// `VisibilityRange` band.
#[derive(Component, Clone, Copy, Debug, ExtractComponent)]
#[extract_app(RenderApp)]
pub struct KitSlotDraw {
    pub slot: u32,
    pub lod: u8,
    pub band: bool,
}

impl KitSlotDraw {
    /// The (slot, LOD) mesh metadata entry and bind group, shared by the whole and band draws.
    fn meta_index(self) -> usize {
        self.slot as usize * 3 + self.lod as usize
    }

    /// The key of this draw's per-view instance range.
    fn draw_index(self) -> usize {
        draw_index(self.meta_index(), self.band)
    }
}

fn draw_index(meta_index: usize, band: bool) -> usize {
    meta_index * 2 + usize::from(band)
}

/// The debug LOD overlays for the instanced path, which has no per-part entities for the
/// `VisibilityRange` force and the material-swap tint to act on: `force` pins every chunk to one
/// LOD, `tint` paints each draw in its LOD's debug colour. hex-play mirrors its overlays here.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, ExtractResource)]
#[extract_app(RenderApp)]
pub struct KitLodDebug {
    pub force: Option<u8>,
    pub tint: bool,
}

/// One view's instanced kit work in a census frame (`diag.kit_queue`): what the queue culled,
/// binned and drew, counted on the CPU, so a change's work can be compared without timing.
#[derive(Clone, Debug, Default)]
pub struct KitQueueView {
    /// `main` or `cascade<i>`.
    pub view: String,
    /// The screen-aware cascade skip (O5) left the cascade without kit draws.
    pub skipped: bool,
    /// Culling cells (tiles) in the view's frustum.
    pub cells_visible: u32,
    pub draws: u32,
    pub instances: u64,
    /// Vertices the draws fetch (instances times the mesh's vertices).
    pub vertices: u64,
    /// Vertex buffer bytes behind them (vertices times the mesh's interleaved stride).
    pub vertex_bytes: u64,
    /// The share of `vertex_bytes` that is COLOR_0 (16 bytes a vertex on the coalesced meshes).
    pub color_bytes: u64,
    /// Indices drawn (instances times the mesh's indices): three per triangle.
    pub indices: u64,
}

/// The instanced kit's queue census (`diag.kit_queue`).
#[derive(Clone, Debug, Default)]
pub struct KitQueueStats {
    pub slots: u32,
    pub cells: u32,
    /// Group-2 bind groups created by the frame's prepare (one per (slot, LOD) when rebuilt).
    pub bind_groups_created: u32,
    pub views: Vec<KitQueueView>,
}

static QUEUE_STATS_REQUESTED: AtomicBool = AtomicBool::new(false);
static QUEUE_STATS: Mutex<Option<KitQueueStats>> = Mutex::new(None);

/// Asks the next queue for a [`KitQueueStats`] census (the diag census frame does).
pub fn request_queue_stats() {
    QUEUE_STATS_REQUESTED.store(true, Ordering::Relaxed);
}

/// The last census taken, if any.
pub fn queue_stats() -> Option<KitQueueStats> {
    QUEUE_STATS.lock().ok().and_then(|s| s.clone())
}

/// Mesh facts per `draw_index` for the census: vertices, indices, stride, COLOR_0.
type DrawMeshInfo = HashMap<usize, (u64, u64, u64, bool)>;

/// Sums a view's `ranges` into a census row.
fn view_census(view: String, ranges: &[(usize, Range<u32>)], info: &DrawMeshInfo) -> KitQueueView {
    let mut v = KitQueueView {
        view,
        draws: ranges.len() as u32,
        ..default()
    };
    for (draw, r) in ranges {
        let n = u64::from(r.end - r.start);
        let (verts, indices, stride, color) = info.get(draw).copied().unwrap_or_default();
        v.instances += n;
        v.vertices += n * verts;
        v.vertex_bytes += n * verts * stride;
        v.color_bytes += if color { n * verts * 16 } else { 0 };
        v.indices += n * indices;
    }
    v
}

/// LOD `lod`'s band edges `[x, y, z, w]` in metres from the camera, the per-entity
/// `VisibilityRange` of `scene::lod_range` as bevy's `visibility_ranges` entries hold it: fading
/// in over `x..y`, whole over `y..z`, fading out over `z..w`.
pub fn kit_lod_range(lod: usize) -> [f32; 4] {
    let r = crate::scene::lod_range(lod);
    [
        r.start_margin.start,
        r.start_margin.end,
        r.end_margin.start,
        r.end_margin.end,
    ]
}

/// Where a settled part draws: whole at one LOD, or inside the cross-fade band between LOD `l`
/// and `l + 1`, where both draw dithered in camera views. In shadow views a band part casts with
/// LOD `l` alone, whole (owner A15 Q1, 2026-10-21): the outgoing LOD is the one still drawn
/// whole up to the band, and a slot the next LOD drops keeps its shadow until it has faded out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartLod {
    Whole(u8),
    Band(u8),
}

/// The full LOD chain: each LOD's range is drawn by its own mesh.
const FULL_CHAIN: [Option<u8>; 3] = [Some(0), Some(1), Some(2)];

impl PartLod {
    /// Whether the part draws in `lod`'s whole (`band` false) or band draw of a camera view, or
    /// of a shadow view, for a slot whose LOD-`g` range is drawn by its mesh `map[g]`
    /// ([`slot_lod_map`]). A band whose two sides draw the same mesh draws it whole.
    fn draws_in(self, map: [Option<u8>; 3], lod: usize, band: bool, shadow: bool) -> bool {
        let is = |m: Option<u8>| m.is_some_and(|m| m as usize == lod);
        match self {
            PartLod::Whole(g) => is(map[g as usize]) && !band,
            PartLod::Band(g) => {
                let (out, inc) = (map[g as usize], map[g as usize + 1]);
                if out == inc || shadow {
                    is(out) && !band
                } else {
                    (is(out) || is(inc)) && band
                }
            }
        }
    }

    #[cfg(test)]
    fn draws(self, lod: usize, band: bool, shadow: bool) -> bool {
        self.draws_in(FULL_CHAIN, lod, band, shadow)
    }
}

/// Which of a slot's meshes draws each LOD range: a slot with no LOD1 mesh draws its LOD0 mesh
/// there (or its LOD2 mesh with no LOD0 either), as the per-entity path does through
/// `scene::lod_cover` (owner A2 Q4). A slot with no mesh at all (test fixtures) is a full chain.
pub fn slot_lod_map(slot: &KitSlot) -> [Option<u8>; 3] {
    let present = slot.mesh.map(|m| m.is_some());
    if !present.iter().any(|&p| p) {
        return FULL_CHAIN;
    }
    let mut map = [None; 3];
    for lod in (0..3).filter(|&l| present[l]) {
        let c = crate::scene::lod_cover(present, lod);
        for g in c.first..=c.last {
            map[g as usize] = Some(lod as u8);
        }
    }
    map
}

/// The band edges (as [`kit_lod_range`]) slot `slot`'s LOD-`lod` mesh fades over: from its first
/// covered LOD's fade-in to its last one's fade-out.
fn slot_lod_range(slot: Option<&KitSlot>, lod: usize) -> [f32; 4] {
    let Some(map) = slot.map(slot_lod_map) else {
        return kit_lod_range(lod);
    };
    let covered: Vec<usize> = (0..3).filter(|&g| map[g] == Some(lod as u8)).collect();
    match (covered.first(), covered.last()) {
        (Some(&a), Some(&b)) => {
            let (a, b) = (kit_lod_range(a), kit_lod_range(b));
            [a[0], a[1], b[2], b[3]]
        }
        _ => kit_lod_range(lod),
    }
}

/// The LOD of a part whose origin lies `distance` metres from the camera. The last LOD never
/// fades out (the per-entity range ends at `LOD_END`, far past any world).
pub fn kit_part_lod(distance: f32) -> PartLod {
    for lod in 0..2u8 {
        let [_, _, z, w] = kit_lod_range(lod as usize);
        if distance < z {
            return PartLod::Whole(lod);
        }
        if distance < w {
            return PartLod::Band(lod);
        }
    }
    PartLod::Whole(2)
}

/// A chunk's LOD this frame: every part in it at one `PartLod`, or `Split` per part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkLod {
    All(PartLod),
    Split,
}

/// The LOD of chunk `chunk` seen from `eye`: `All` when its nearest and farthest part origins fall
/// in one `PartLod` (the bands are ordered by distance, so everything between does too).
pub fn kit_chunk_lod(chunk: &KitChunk, eye: Vec3) -> ChunkLod {
    let (lo, hi) = (
        Vec3::from(chunk.origins.min()),
        Vec3::from(chunk.origins.max()),
    );
    if lo.cmpgt(hi).any() {
        return ChunkLod::All(PartLod::Whole(0));
    }
    let near = eye.distance(eye.clamp(lo, hi));
    let far = (eye - lo).abs().max((eye - hi).abs()).length();
    let (a, b) = (kit_part_lod(near), kit_part_lod(far));
    if a == b {
        ChunkLod::All(a)
    } else {
        ChunkLod::Split
    }
}

/// Appends the visible instance ids to `ids`, grouped by (slot, LOD), and returns the non-empty
/// draws' ranges into `ids` keyed by `draw_index(slot * 3 + lod, band)`. A (slot, LOD) with any
/// part inside a band draws all its parts in one band draw: a whole part's dither level is 0, so
/// the dithered pipeline draws it whole, and the band costs no draw beyond the neighbouring LOD's.
/// `lods[ci]` is chunk `ci`'s LOD from the main camera and `parts[i]` record `i`'s where its
/// chunk is `Split`; shadow views reuse them, as `VisibilityRange` does, and fold band parts into
/// the whole draws. `cells`, when given, leaves out each cell it marks false (a shadow cascade's
/// per-cell skip, [`cell_shadowed_depth_range`]).
pub fn kit_view_ranges(
    set: &KitInstanceSetData,
    lods: &[ChunkLod],
    parts: &[PartLod],
    frustum: &Frustum,
    shadow: bool,
    cells: Option<&[bool]>,
    ids: &mut Vec<u32>,
) -> Vec<(usize, Range<u32>)> {
    let visible: Vec<bool> = set
        .chunks
        .iter()
        .enumerate()
        .map(|(ci, c)| {
            cells.is_none_or(|m| m.get(ci).copied().unwrap_or(true))
                // Shadow casters in front of a cascade's near plane still cast into it.
                && frustum.intersects_obb(&c.aabb, &Affine3A::IDENTITY, !shadow, true)
        })
        .collect();
    let mut out = vec![];
    for (slot, cells) in set.by_slot.iter().enumerate().take(set.slots.len()) {
        if !cells.iter().any(|(ci, _)| visible[*ci as usize]) {
            continue;
        }
        let map = slot_lod_map(&set.slots[slot]);
        for lod in 0..3usize {
            let first = ids.len() as u32;
            let mut banded = false;
            for band in [false, true] {
                let start = ids.len();
                for (ci, r) in cells {
                    let ci = *ci as usize;
                    if !visible[ci] {
                        continue;
                    }
                    match lods[ci] {
                        ChunkLod::All(p) => {
                            if p.draws_in(map, lod, band, shadow) {
                                ids.extend(r.clone());
                            }
                        }
                        ChunkLod::Split => ids.extend(
                            r.clone()
                                .filter(|&i| parts[i as usize].draws_in(map, lod, band, shadow)),
                        ),
                    }
                }
                banded |= band && ids.len() > start;
            }
            let last = ids.len() as u32;
            if last > first {
                out.push((draw_index(slot * 3 + lod, banded), first..last));
            }
        }
    }
    out
}

/// Every chunk's LOD from `eye` (all pinned to `force` when set, with no band), and the per-part
/// LOD of every record in a `Split` chunk (`parts` is resized to the record count; the entries of
/// other chunks' records are left stale and never read).
pub fn kit_frame_lods(
    set: &KitInstanceSetData,
    eye: Vec3,
    force: Option<u8>,
    parts: &mut Vec<PartLod>,
) -> Vec<ChunkLod> {
    parts.resize(set.pieces.len(), PartLod::Whole(0));
    set.chunks
        .iter()
        .map(|c| {
            if let Some(l) = force {
                return ChunkLod::All(PartLod::Whole(l.min(2)));
            }
            let lod = kit_chunk_lod(c, eye);
            if lod == ChunkLod::Split {
                for (_, r) in &c.ranges {
                    for i in r.clone() {
                        let p = Vec3::from(set.pieces[i as usize].translation);
                        parts[i as usize] = kit_part_lod(eye.distance(p));
                    }
                }
            }
            lod
        })
        .collect()
}

/// Perf ablation (`kit_ablate=cascade_clip,coalesce`): each named close-camera draw cut is turned
/// off, so one build times it against itself (`docs/perf/large-world-o4o5/`). Set once at startup;
/// the default turns nothing off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KitAblate {
    /// O5 off: the cascade skip takes the whole swept kit bounds, not their on-screen part.
    pub cascade_clip: bool,
    /// O4 off: no runtime slot coalescing; every slot draws its own mesh and material.
    pub coalesce: bool,
    /// B2 off: play derives no field boundaries (walls, hedges, their hedgerow oaks), so a draw
    /// census can be taken with and without them on one build.
    pub boundaries: bool,
}

impl KitAblate {
    /// A comma list of `cascade_clip`, `coalesce` and `boundaries`, or `none`.
    pub fn parse(s: &str) -> Option<KitAblate> {
        let mut a = KitAblate::default();
        for w in s.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            match w {
                "cascade_clip" => a.cascade_clip = true,
                "coalesce" => a.coalesce = true,
                "boundaries" => a.boundaries = true,
                "none" => {}
                _ => return None,
            }
        }
        Some(a)
    }
}

static KIT_ABLATE: std::sync::OnceLock<KitAblate> = std::sync::OnceLock::new();

/// Sets the process's [`KitAblate`]; only the first call takes effect.
pub fn set_kit_ablate(a: KitAblate) {
    let _ = KIT_ABLATE.set(a);
}

/// The process's [`KitAblate`] (nothing turned off unless [`set_kit_ablate`] said so).
pub fn kit_ablate() -> KitAblate {
    KIT_ABLATE.get().copied().unwrap_or_default()
}

/// [`shadowed_depth_range`] without the frustum clip (the skip before O5): the view depths of the
/// whole swept, grown bounds, on screen or not.
pub fn swept_depth_range(
    lo: Vec3,
    hi: Vec3,
    dir: Vec3,
    view_from_world: &Mat4,
    margin: f32,
) -> ShadowedDepth {
    if dir.y > -0.05 {
        return ShadowedDepth::Unbounded;
    }
    let (lo, hi) = (lo - Vec3::splat(margin), hi + Vec3::splat(margin));
    let sweep = dir * ((hi.y - lo.y) / -dir.y);
    let (mut near, mut far) = (f32::MAX, f32::MIN);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        for p in [corner, corner + sweep] {
            let depth = -view_from_world.transform_point3(p).z;
            near = near.min(depth);
            far = far.max(depth);
        }
    }
    ShadowedDepth::Range(near, far)
}

/// What of a shadow-casting region a camera sees, for [`shadowed_depth_range`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowedDepth {
    /// The light is so near the horizon that the sweep is unbounded: any cascade may be read.
    Unbounded,
    /// No point the casters can shadow lies inside the camera's frustum: no cascade is read.
    OffScreen,
    /// The view depths (`-z` in the camera's view space) of the points on screen they can shadow.
    Range(f32, f32),
}

/// The view depth range of every point on screen that a caster inside `lo..hi` can shadow, under
/// a directional light shining along `dir`. The shadowed region is the bounds grown by `margin`
/// and swept along `dir` down to their grown floor (light falls downward, so nothing above the
/// bounds or past the sweep is shadowed by them), clipped by the camera frustum of
/// `clip_from_world`: a fragment off screen is never shaded, so it reads no cascade.
pub fn shadowed_depth_range(
    lo: Vec3,
    hi: Vec3,
    dir: Vec3,
    view_from_world: &Mat4,
    clip_from_world: &Mat4,
    margin: f32,
) -> ShadowedDepth {
    if dir.y > -0.05 {
        return ShadowedDepth::Unbounded;
    }
    let (lo, hi) = (lo - Vec3::splat(margin), hi + Vec3::splat(margin));
    let sweep = dir * ((hi.y - lo.y) / -dir.y);
    let planes = frustum_planes(clip_from_world);
    let (mut near, mut far) = (f32::MAX, f32::MIN);
    let mut take = |p: Vec3| {
        let depth = -view_from_world.transform_point3(p).z;
        near = near.min(depth);
        far = far.max(depth);
    };
    // The swept box is the Minkowski sum of the box and the sweep segment. Its boundary lies on
    // the box's faces, their swept copies and the parallelograms each box edge sweeps out, so a
    // vertex of its intersection with the frustum is a vertex of one of those polygons clipped by
    // the frustum, or a frustum corner inside it.
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    const FACES: [[usize; 4]; 6] = [
        [0, 2, 6, 4],
        [1, 3, 7, 5],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 1, 3, 2],
        [4, 5, 7, 6],
    ];
    const EDGES: [[usize; 2]; 12] = [
        [0, 1],
        [2, 3],
        [4, 5],
        [6, 7],
        [0, 2],
        [1, 3],
        [4, 6],
        [5, 7],
        [0, 4],
        [1, 5],
        [2, 6],
        [3, 7],
    ];
    let mut polygons: Vec<Vec<Vec3>> = vec![];
    for f in FACES {
        let face: Vec<Vec3> = f.iter().map(|&i| corner(i)).collect();
        polygons.push(face.iter().map(|&p| p + sweep).collect());
        polygons.push(face);
    }
    for [a, b] in EDGES {
        let (a, b) = (corner(a), corner(b));
        polygons.push(vec![a, b, b + sweep, a + sweep]);
    }
    for poly in polygons {
        for p in planes
            .iter()
            .fold(poly, |poly, &plane| clip_polygon(&poly, plane))
        {
            take(p);
        }
    }
    for p in frustum_corners(&planes) {
        if in_swept_box(p, lo, hi, sweep) {
            take(p);
        }
    }
    if near > far {
        ShadowedDepth::OffScreen
    } else {
        ShadowedDepth::Range(near, far)
    }
}

/// The view depths of every point a caster inside one culling cell `lo..hi` can shadow, bounded
/// from outside: the cell grown by `margin` and swept along `dir` down to `floor` (the whole kit's
/// grown floor, since a cell's casters can shadow a lower neighbour), measured at its extreme
/// corners and not clipped by the frustum. It holds every on-screen point the clipped
/// [`shadowed_depth_range`] of the same region would, so a cascade it misses is one that no
/// fragment shadowed by the cell reads.
pub fn cell_shadowed_depth_range(
    lo: Vec3,
    hi: Vec3,
    floor: f32,
    dir: Vec3,
    view_from_world: &Mat4,
    margin: f32,
) -> (f32, f32) {
    // View depth is `-z` in view space, `a . p + b`: linear, so a box's extremes are its centre's
    // depth plus or minus `|a| . half`, and the sweep moves one end by `a . sweep`.
    let row = view_from_world.row(2);
    let (a, b) = (-row.truncate(), -row.w);
    let (lo, hi) = (lo - Vec3::splat(margin), hi + Vec3::splat(margin));
    let (centre, half) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
    let mid = a.dot(centre) + b;
    let reach = a.abs().dot(half);
    let swept = a.dot(dir * ((hi.y - floor) / -dir.y).max(0.0));
    (mid - reach + swept.min(0.0), mid + reach + swept.max(0.0))
}

/// The frustum of `clip_from_world` as inward half-spaces `n . p + d >= 0` (WebGPU clip space,
/// `-w <= x, y <= w`, `0 <= z <= w`). A degenerate plane (the far plane of bevy's infinite
/// reverse-z projection, `z = near`) bounds nothing and is left out.
fn frustum_planes(clip_from_world: &Mat4) -> Vec<Vec4> {
    let r = |i: usize| clip_from_world.row(i);
    [
        r(3) + r(0),
        r(3) - r(0),
        r(3) + r(1),
        r(3) - r(1),
        r(2),
        r(3) - r(2),
    ]
    .into_iter()
    .filter(|p| p.truncate().length() > 1e-6 * p.w.abs().max(1.0))
    .collect()
}

/// Signed distance-like value of `p` against `plane` (positive inside).
fn plane_side(plane: Vec4, p: Vec3) -> f32 {
    plane.truncate().dot(p) + plane.w
}

/// One Sutherland-Hodgman step: the part of convex polygon `poly` inside `plane`.
fn clip_polygon(poly: &[Vec3], plane: Vec4) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let (da, db) = (plane_side(plane, a), plane_side(plane, b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}

/// The frustum's corners: every point where three of `planes` meet that lies inside all of them.
fn frustum_corners(planes: &[Vec4]) -> Vec<Vec3> {
    let mut out = vec![];
    for i in 0..planes.len() {
        for j in i + 1..planes.len() {
            for k in j + 1..planes.len() {
                let (a, b, c) = (planes[i], planes[j], planes[k]);
                let m = Mat3::from_cols(a.truncate(), b.truncate(), c.truncate()).transpose();
                let det = m.determinant();
                let scale = a.truncate().length() * b.truncate().length() * c.truncate().length();
                if det.abs() <= 1e-6 * scale {
                    continue;
                }
                let p = m.inverse() * -Vec3::new(a.w, b.w, c.w);
                let tol = 1e-4 * p.length().max(1.0);
                if planes
                    .iter()
                    .all(|&q| plane_side(q, p) >= -tol * q.truncate().length())
                {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Whether `p` lies in the box `lo..hi` swept along `sweep`: some `t` in `0..=1` puts `p - t *
/// sweep` inside the box.
fn in_swept_box(p: Vec3, lo: Vec3, hi: Vec3, sweep: Vec3) -> bool {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for a in 0..3 {
        let (p, lo, hi, s) = (p[a], lo[a], hi[a], sweep[a]);
        if s.abs() < 1e-9 {
            if p < lo || p > hi {
                return false;
            }
            continue;
        }
        let (u, v) = ((p - hi) / s, (p - lo) / s);
        t0 = t0.max(u.min(v));
        t1 = t1.min(u.max(v));
    }
    t0 <= t1
}

/// Whether a fragment at a view depth inside `range` samples cascade `index`, by bevy_pbr's
/// `fetch_directional_shadow`: a fragment samples the first cascade whose far bound lies past
/// it, and inside the last `overlap` of that cascade it also blends in the next one. So cascade
/// `i` is read from `(1 - overlap) * bounds[i - 1]` (0 for the first) up to `bounds[i]`.
pub fn cascade_sampled(bounds: &[f32], overlap: f32, index: usize, range: (f32, f32)) -> bool {
    let Some(&far) = bounds.get(index) else {
        return true;
    };
    let near = match index {
        0 => f32::MIN,
        i => (1.0 - overlap) * bounds[i - 1],
    };
    range.1 >= near && range.0 < far
}

/// The shadow filter's reach past a caster's edge, in shadow-map texels of the cascade, plus a
/// fixed allowance (m) for the depth and normal biases.
const SHADOW_REACH_TEXELS: f32 = 8.0;
const SHADOW_REACH_M: f32 = 2.0;
// ---------------------------------------------------------------------------------------------
// Render world.

pub struct KitInstancesPlugin;

impl Plugin for KitInstancesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KitLodDebug>().add_plugins((
            ExtractResourcePlugin::<KitInstanceSet>::default(),
            ExtractResourcePlugin::<KitLodDebug>::default(),
            ExtractComponentPlugin::<KitSlotDraw>::default(),
        ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<KitGpu>()
            .init_resource::<SpecializedMeshPipelines<KitMainPipeline>>()
            .init_resource::<SpecializedMeshPipelines<KitPrepassPipeline>>()
            .add_render_command::<Opaque3d, DrawKitMain>()
            .add_render_command::<AlphaMask3d, DrawKitMain>()
            .add_render_command::<Opaque3dPrepass, DrawKitPrepass>()
            .add_render_command::<AlphaMask3dPrepass, DrawKitPrepass>()
            .add_render_command::<Shadow, DrawKitPrepass>()
            .add_systems(
                Render,
                (
                    prepare_kit.in_set(RenderSystems::PrepareResources),
                    prepare_kit_bind_group.in_set(RenderSystems::PrepareBindGroups),
                    // The slot entities carry `MeshMaterial3d` only to resolve their bind group, so
                    // bevy's own queue systems treat a material edited this frame (the wet band,
                    // every frame) as a changed renderable and `remove` it from each phase. Queue
                    // after them, or a removal can follow this frame's `add` and drop the slot.
                    queue_kit
                        .in_set(RenderSystems::Queue)
                        .after(RenderSystems::QueueMeshes),
                ),
            );
    }
}

/// GPU state: the record buffer plus this frame's per-view draw ranges.
#[derive(Resource)]
struct KitGpu {
    scene: Option<Arc<KitInstanceSetData>>,
    pieces: Option<Buffer>,
    meta: Option<Buffer>,
    /// Byte stride between (slot, LOD) meta entries (device alignment).
    meta_stride: u32,
    /// The LOD tint the meta buffer was written with.
    meta_tint: bool,
    ids: RawBufferVec<u32>,
    // (`ids` holds the per-view record ids; it is bound as an instanced vertex buffer, not
    // storage — see kit_instances.wesl for why.)
    /// One static bind group per meta entry (no dynamic offsets).
    bind_groups: Vec<BindGroup>,
    /// (view render entity, `KitSlotDraw::draw_index`) -> instance range into `ids`.
    draws: HashMap<(Entity, usize), Range<u32>>,
    /// Each record's LOD this frame where its chunk is split (kept to reuse the allocation).
    parts: Vec<PartLod>,
    /// Slot draws binned last frame. Bevy's binned phases retain items across frames until
    /// `remove`d, so each queue first unbins these or culled and despawned slots stay binned.
    queued: Vec<MainEntity>,
    /// The (record, metadata) buffers `bind_groups` bind.
    bound: Option<(BufferId, BufferId)>,
    /// Bind groups the last prepare created (the queue census reports it).
    bind_groups_made: u32,
}

impl Default for KitGpu {
    fn default() -> Self {
        Self {
            scene: None,
            pieces: None,
            meta: None,
            meta_stride: 0,
            meta_tint: false,
            ids: RawBufferVec::new(BufferUsages::VERTEX),
            bind_groups: Vec::new(),
            draws: HashMap::default(),
            parts: Vec::new(),
            queued: Vec::new(),
            bound: None,
            bind_groups_made: 0,
        }
    }
}

/// Removes `stale` from every view's `phases`.
fn unqueue<BPI: BinnedPhaseItem>(phases: &mut ViewBinnedRenderPhases<BPI>, stale: &[MainEntity]) {
    for phase in phases.values_mut() {
        for &me in stale {
            phase.remove(me);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Pipelines.

/// The `StandardMaterial` shader defs my pipelines must reproduce (the per-entity path gets them
/// from `StandardMaterial::specialize`); read off the pinned bevy_pbr-0.20.0 `pbr_material.rs`.
const MATERIAL_DEFS: &[(u64, &str)] = &[
    (
        0x000004, // StandardMaterialKey::NORMAL_MAP
        "STANDARD_MATERIAL_NORMAL_MAP",
    ),
    (
        0x000008, // RELIEF_MAPPING
        "RELIEF_MAPPING",
    ),
    (
        0x000010, // DIFFUSE_TRANSMISSION
        "STANDARD_MATERIAL_DIFFUSE_TRANSMISSION",
    ),
    (
        0x000020, // SPECULAR_TRANSMISSION
        "STANDARD_MATERIAL_SPECULAR_TRANSMISSION",
    ),
    (0x000040, "STANDARD_MATERIAL_CLEARCOAT"),
    (0x000080, "STANDARD_MATERIAL_CLEARCOAT_NORMAL_MAP"),
    (0x000100, "STANDARD_MATERIAL_ANISOTROPY"),
    (0x000200, "STANDARD_MATERIAL_BASE_COLOR_UV_B"),
    (0x000400, "STANDARD_MATERIAL_EMISSIVE_UV_B"),
    (0x000800, "STANDARD_MATERIAL_METALLIC_ROUGHNESS_UV_B"),
    (0x001000, "STANDARD_MATERIAL_OCCLUSION_UV_B"),
    (0x002000, "STANDARD_MATERIAL_SPECULAR_TRANSMISSION_UV_B"),
    (0x004000, "STANDARD_MATERIAL_THICKNESS_UV_B"),
    (0x008000, "STANDARD_MATERIAL_DIFFUSE_TRANSMISSION_UV_B"),
    (0x0000_1000 << 4, "STANDARD_MATERIAL_NORMAL_MAP_UV_B"),
    (0x0000_2000 << 4, "STANDARD_MATERIAL_ANISOTROPY_UV_B"),
    (0x0000_4000 << 4, "STANDARD_MATERIAL_CLEARCOAT_UV_B"),
    (
        0x0000_8000 << 4,
        "STANDARD_MATERIAL_CLEARCOAT_ROUGHNESS_UV_B",
    ),
    (0x0001_0000 << 4, "STANDARD_MATERIAL_CLEARCOAT_NORMAL_UV_B"),
    (0x0002_0000 << 4, "STANDARD_MATERIAL_SPECULAR_UV_B"),
    (0x0004_0000 << 4, "STANDARD_MATERIAL_SPECULAR_TINT_UV_B"),
];

fn push_material_defs(defs: &mut Vec<ShaderDefVal>, key: u64, mesh_key: MeshPipelineKey) {
    for &(flags, def) in MATERIAL_DEFS {
        if matches!(
            flags,
            0x000020 | 0x002000 // SPECULAR_TRANSMISSION, SPECULAR_TRANSMISSION_UV
        ) && !mesh_key.contains(MeshPipelineKey::VIEW_TRANSMISSION_TEXTURE)
        {
            continue;
        }
        if key & flags != 0 {
            defs.push(def.into());
        }
    }
}

fn cull_mode(key: u64) -> Option<Face> {
    match key & 0b11 {
        0b01 => Some(Face::Front),
        0b10 => Some(Face::Back),
        _ => None,
    }
}

fn depth_bias_of(key: u64) -> i32 {
    (key >> 32) as u32 as i32
}

/// Pipeline key: the mesh key (view + mesh + material alpha bits) plus the material key bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct KitKey {
    mesh: MeshPipelineKey,
    mat: u64,
}

#[derive(Resource, Clone)]
struct KitMainPipeline {
    mesh: MeshPipeline,
    instances: BindGroupLayoutDescriptor,
    material: BindGroupLayoutDescriptor,
    vertex: Handle<Shader>,
    fragment: Handle<Shader>,
}

impl SpecializedMeshPipeline for KitMainPipeline {
    type Key = KitKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut d = self.mesh.specialize(key.mesh, layout)?;
        push_material_defs(&mut d.vertex.shader_defs, key.mat, key.mesh);
        if let Some(f) = d.fragment.as_mut() {
            push_material_defs(&mut f.shader_defs, key.mat, key.mesh);
            f.shader_defs.push(ShaderDefVal::UInt(
                "MATERIAL_BIND_GROUP".into(),
                MATERIAL_BIND_GROUP_INDEX as u32,
            ));
            f.shader_defs.push("KIT_INSTANCED".into());
            f.shader_defs.push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
        }
        d.vertex.shader_defs.push(ShaderDefVal::UInt(
            "MATERIAL_BIND_GROUP".into(),
            MATERIAL_BIND_GROUP_INDEX as u32,
        ));
        d.vertex.shader_defs.push("KIT_INSTANCED".into());
        d.vertex
            .shader_defs
            .push("VERTEX_OUTPUT_INSTANCE_INDEX".into());
        d.vertex.shader = self.vertex.clone();
        d.vertex.entry_point = Some("vertex".into());
        let f = d.fragment.as_mut().expect("main pass has a fragment");
        f.shader = self.fragment.clone();
        f.entry_point = Some("fragment".into());
        d.layout[2] = self.instances.clone();
        d.layout.insert(3, self.material.clone());
        // The instanced u32 record-id buffer: one u32 per instance, sliced per draw. Vertex
        // attributes carry no instance-index semantics, so the record lookup cannot drift
        // between Metal's absolute `[[instance_id]]` and WebGPU's 0-based `instance_index`.
        let mut record = d.vertex.buffers[0].clone();
        record.array_stride = 4;
        record.step_mode = VertexStepMode::Instance;
        let mut attr = record.attributes[0];
        attr.format = VertexFormat::Uint32;
        attr.offset = 0;
        attr.shader_location = 6;
        record.attributes = vec![attr];
        d.vertex.buffers.push(record);
        d.primitive.cull_mode = cull_mode(key.mat);
        if let Some(ds) = d.depth_stencil.as_mut() {
            ds.bias.constant = depth_bias_of(key.mat);
        }
        d.label = Some("kit_instances_main".into());
        Ok(d)
    }
}
#[derive(Resource, Clone)]
struct KitPrepassPipeline {
    prepass: PrepassPipeline,
    instances: BindGroupLayoutDescriptor,
    material: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    depth_clip_control: bool,
}

impl SpecializedMeshPipeline for KitPrepassPipeline {
    type Key = KitKey;

    /// Mirrors bevy_pbr-0.20.0 `PrepassPipeline::specialize` for an opaque/masked, non-skinned,
    /// non-morphed, non-lightmapped, non-deferred, non-compressed kit mesh, with the mesh-uniform
    /// group replaced by the instance buffers. Pinned upstream, so this cannot drift silently.
    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mesh_key = key.mesh;
        let normal = mesh_key.contains(MeshPipelineKey::NORMAL_PREPASS);
        let motion = mesh_key.contains(MeshPipelineKey::MOTION_VECTOR_PREPASS);
        let depth_only = mesh_key.contains(MeshPipelineKey::DEPTH_PREPASS);
        let ortho_unclipped = mesh_key.contains(MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO);
        let emulate = ortho_unclipped && !self.depth_clip_control;
        let mut defs: Vec<ShaderDefVal> = vec![
            "PREPASS_PIPELINE".into(),
            ShaderDefVal::UInt(
                "MATERIAL_BIND_GROUP".into(),
                MATERIAL_BIND_GROUP_INDEX as u32,
            ),
            "KIT_INSTANCED".into(),
            "VERTEX_OUTPUT_INSTANCE_INDEX".into(),
        ];
        push_material_defs(&mut defs, key.mat, mesh_key);
        let vp = mesh_key.intersection(MeshPipelineKey::VIEW_PROJECTION_RESERVED_BITS);
        if vp == MeshPipelineKey::VIEW_PROJECTION_NONSTANDARD {
            defs.push("VIEW_PROJECTION_NONSTANDARD".into());
        } else if vp == MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE {
            defs.push("VIEW_PROJECTION_PERSPECTIVE".into());
        } else if vp == MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC {
            defs.push("VIEW_PROJECTION_ORTHOGRAPHIC".into());
        }
        if depth_only {
            defs.push("DEPTH_PREPASS".into());
        }
        if mesh_key.contains(MeshPipelineKey::MAY_DISCARD) {
            defs.push("MAY_DISCARD".into());
        }
        // A band draw: the fragment discards the pixels its dither pattern gives the other LOD.
        let dither = mesh_key.contains(MeshPipelineKey::VISIBILITY_RANGE_DITHER);
        if dither {
            defs.push("VISIBILITY_RANGE_DITHER".into());
        }
        if normal {
            defs.push("NORMAL_PREPASS".into());
            defs.push("NORMAL_PREPASS_OR_DEFERRED_PREPASS".into());
        }
        // Attribute-driven defs + the vertex buffer layout, in bevy's location order.
        let mut attributes = vec![];
        if layout.0.contains(Mesh::ATTRIBUTE_POSITION) {
            defs.push("VERTEX_POSITIONS".into());
            attributes.push(Mesh::ATTRIBUTE_POSITION.at_shader_location(0));
        }
        if layout.0.contains(Mesh::ATTRIBUTE_UV_0) {
            defs.push("VERTEX_UVS".into());
            defs.push("VERTEX_UVS_A".into());
            attributes.push(Mesh::ATTRIBUTE_UV_0.at_shader_location(1));
        }
        if layout.0.contains(Mesh::ATTRIBUTE_UV_1) {
            defs.push("VERTEX_UVS".into());
            defs.push("VERTEX_UVS_B".into());
            attributes.push(Mesh::ATTRIBUTE_UV_1.at_shader_location(2));
        }
        if normal && layout.0.contains(Mesh::ATTRIBUTE_NORMAL) {
            defs.push("VERTEX_NORMALS".into());
            attributes.push(Mesh::ATTRIBUTE_NORMAL.at_shader_location(3));
        }
        if normal && layout.0.contains(Mesh::ATTRIBUTE_TANGENT) {
            defs.push("VERTEX_TANGENTS".into());
            attributes.push(Mesh::ATTRIBUTE_TANGENT.at_shader_location(4));
        }
        if layout.0.contains(Mesh::ATTRIBUTE_COLOR) {
            defs.push("VERTEX_COLORS".into());
            attributes.push(Mesh::ATTRIBUTE_COLOR.at_shader_location(7));
        }
        if motion {
            defs.push("MOTION_VECTOR_PREPASS".into());
            defs.push("MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS".into());
        }
        if normal || motion {
            defs.push("PREPASS_FRAGMENT".into());
        }
        if emulate {
            defs.push("UNCLIPPED_DEPTH_ORTHO_EMULATION".into());
            defs.push("PREPASS_FRAGMENT".into());
        }
        let vertex_layout = layout.0.get_layout(&attributes)?;
        let mut targets = prepass_target_descriptors(normal, motion, false);
        if targets.iter().all(Option::is_none) {
            targets.clear();
        }
        let fragment_needed = !targets.is_empty()
            || emulate
            || dither
            || mesh_key.contains(MeshPipelineKey::MAY_DISCARD);
        let fragment = fragment_needed.then(|| FragmentState {
            shader: self.shader.clone(),
            shader_defs: defs.clone(),
            entry_point: Some("fragment".into()),
            targets,
            ..default()
        });
        let d = RenderPipelineDescriptor {
            label: Some("kit_instances_prepass".into()),
            layout: vec![
                if motion {
                    self.prepass.view_layout_motion_vectors.clone()
                } else {
                    self.prepass.view_layout_no_motion_vectors.clone()
                },
                self.prepass.empty_layout.clone(),
                self.instances.clone(),
                self.material.clone(),
            ],
            vertex: VertexState {
                shader: self.shader.clone(),
                shader_defs: defs,
                entry_point: Some("vertex".into()),
                // Instanced u32 record-id buffer; see the main-pass specialize for why.
                buffers: {
                    let mut record = vertex_layout.clone();
                    record.array_stride = 4;
                    record.step_mode = VertexStepMode::Instance;
                    let mut attr = record.attributes[0];
                    attr.format = VertexFormat::Uint32;
                    attr.offset = 0;
                    attr.shader_location = 6;
                    record.attributes = vec![attr];
                    vec![vertex_layout, record]
                },
                ..default()
            },
            fragment,
            primitive: PrimitiveState {
                topology: mesh_key.primitive_topology(),
                strip_index_format: mesh_key.strip_index_format(),
                unclipped_depth: ortho_unclipped && self.depth_clip_control,
                cull_mode: cull_mode(key.mat),
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState::default(),
                bias: DepthBiasState {
                    constant: depth_bias_of(key.mat),
                    ..default()
                },
            }),
            multisample: MultisampleState {
                count: mesh_key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            ..default()
        };
        Ok(d)
    }
}
// ---------------------------------------------------------------------------------------------
// Prepare systems.

/// Builds the pipelines and uploads the record buffer the first time a set arrives (and whenever
/// its `Arc` swaps). Reads the kit material's layout from its first prepared instance.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn prepare_kit(
    mut commands: Commands,
    scene: Option<Res<KitInstanceSet>>,
    mut gpu: ResMut<KitGpu>,
    mesh_pipeline: Option<Res<MeshPipeline>>,
    prepass_pipeline: Option<Res<PrepassPipeline>>,
    server: Res<AssetServer>,
    device: Res<RenderDevice>,
    materials: Option<Res<ErasedRenderAssets<PreparedMaterial>>>,
    debug: Option<Res<KitLodDebug>>,
) {
    let Some(scene) = scene else {
        return;
    };
    let tint = debug.is_some_and(|d| d.tint);
    let changed = gpu.scene.as_ref().is_none_or(|s| !Arc::ptr_eq(s, &scene.0));
    if !changed {
        if tint != gpu.meta_tint && gpu.meta.is_some() {
            gpu.meta = Some(meta_buffer(
                &device,
                &scene.0.meta,
                &scene.0.slots,
                gpu.meta_stride,
                tint,
            ));
            gpu.meta_tint = tint;
        }
        return;
    }
    gpu.scene = Some(scene.0.clone());
    let Some(slot) = scene.0.slots.first() else {
        return;
    };
    let Some(materials) = materials else {
        return;
    };
    let Some(prepared) = materials.get(slot.material.untyped()) else {
        return;
    };
    let instances = BindGroupLayoutDescriptor::new(
        "kit_instances_group2",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::VERTEX_FRAGMENT,
            (
                storage_buffer_read_only_sized(false, None),
                uniform_buffer_sized(false, std::num::NonZeroU64::new(META_SIZE)),
            ),
        ),
    );
    let material_layout = prepared
        .properties
        .material_layout
        .as_ref()
        .expect("kit layout");
    commands.insert_resource(KitMainPipeline {
        mesh: mesh_pipeline.expect("MeshPipeline").clone(),
        instances: instances.clone(),
        material: material_layout.clone(),
        vertex: server.load(VERTEX_SHADER),
        fragment: server.load(SHADER),
    });
    commands.insert_resource(KitPrepassPipeline {
        prepass: prepass_pipeline.expect("PrepassPipeline").clone(),
        instances,
        material: material_layout.clone(),
        shader: server.load(PREPASS_SHADER),
        depth_clip_control: device.features().contains(WgpuFeatures::DEPTH_CLIP_CONTROL),
    });
    // The record buffer is rebuilt whenever the set swaps. The floor keeps the binding at or
    // above the shader's runtime-array minimum on near-empty worlds (wgpu validates the bound
    // size, and this app treats validation errors as fatal).
    let mut records = bytemuck::cast_slice(&scene.0.pieces).to_vec();
    records.resize(records.len().max(256), 0);
    gpu.pieces = Some(device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("kit_instances_records"),
        contents: &records,
        usage: BufferUsages::STORAGE,
    }));
    // One metadata entry per (slot, LOD), padded to the device's uniform offset alignment.
    let align = device.limits().min_uniform_buffer_offset_alignment;
    gpu.meta_stride = (META_SIZE as u32).next_multiple_of(align);
    gpu.meta = Some(meta_buffer(
        &device,
        &scene.0.meta,
        &scene.0.slots,
        gpu.meta_stride,
        tint,
    ));
    gpu.meta_tint = tint;
}

/// The (slot, LOD) metadata entries at `stride`. Entry `slot * 3 + lod` carries the debug LOD
/// tint in its pad words (`_pad0` 1 when tinting, `_pad1` the LOD) and the LOD's band edges, which
/// the band draws dither on.
fn meta_buffer(
    device: &RenderDevice,
    meta: &[KitGpuMeta],
    slots: &[KitSlot],
    stride: u32,
    tint: bool,
) -> Buffer {
    let stride = stride as usize;
    let mut bytes = vec![0u8; stride * meta.len()];
    for (i, m) in meta.iter().enumerate() {
        let mut m = *m;
        m._pad0 = u32::from(tint);
        m._pad1 = (i % 3) as u32;
        m.lod_range = slot_lod_range(slots.get(i / 3), i % 3);
        let src = bytemuck::bytes_of(&m);
        bytes[i * stride..i * stride + src.len()].copy_from_slice(src);
    }
    device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("kit_instances_meta"),
        contents: &bytes,
        usage: BufferUsages::UNIFORM,
    })
}
// ---------------------------------------------------------------------------------------------
// Queue.

/// Binds the record + id buffers at group 2.
fn prepare_kit_bind_group(
    mut gpu: ResMut<KitGpu>,
    main: Option<Res<KitMainPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
) {
    let (Some(pipeline), Some(pieces), Some(meta)) = (main, gpu.pieces.as_ref(), gpu.meta.as_ref())
    else {
        return;
    };
    // The bind groups bind only the record and metadata buffers, so they hold until either is
    // recreated (a new instance set or a tint change). Rebuilt every frame, they cost one
    // `createBindGroup` per (slot, LOD) per frame, a call into the browser's GPU process on the web.
    let key = (pieces.id(), meta.id());
    if gpu.bound == Some(key) && !gpu.bind_groups.is_empty() {
        gpu.bind_groups_made = 0;
        return;
    }
    let layout = pipeline_cache.get_bind_group_layout(&pipeline.instances);
    let entries = |offset: u64| {
        [
            BindGroupEntry {
                binding: 0,
                resource: pieces.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: meta,
                    offset,
                    size: std::num::NonZeroU64::new(META_SIZE),
                }),
            },
        ]
    };
    let count = (meta.size() / gpu.meta_stride as u64) as usize;
    gpu.bind_groups = (0..count)
        .map(|i| {
            device.create_bind_group(
                "kit_instances_group2",
                &layout,
                &entries(i as u64 * gpu.meta_stride as u64),
            )
        })
        .collect();
    gpu.bound = Some(key);
    gpu.bind_groups_made = gpu.bind_groups.len() as u32;
}
// ---------------------------------------------------------------------------------------------
// Queue.

/// Queues the slot draws: culls chunks per view, writes the per-view id ranges, and bins every
/// non-empty (slot, LOD) into the view's opaque or alpha-mask phase, its prepass, and the shadow
/// cascades. Masked slots go to `AlphaMask3d`/`AlphaMask3dPrepass`, as `RenderPhaseType` decides.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn queue_kit(
    groups: Query<(Entity, &MainEntity, &KitSlotDraw)>,
    scene: Option<Res<KitInstanceSet>>,
    main: Option<Res<KitMainPipeline>>,
    prepass: Option<Res<KitPrepassPipeline>>,
    mut main_pipelines: ResMut<SpecializedMeshPipelines<KitMainPipeline>>,
    mut prepass_pipelines: ResMut<SpecializedMeshPipelines<KitPrepassPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    view_key_cache: Res<ViewKeyCache>,
    (opaque_draw, mask_draw, prepass_draw, mask_prepass_draw, shadow_draw): (
        Res<DrawFunctions<Opaque3d>>,
        Res<DrawFunctions<AlphaMask3d>>,
        Res<DrawFunctions<Opaque3dPrepass>>,
        Res<DrawFunctions<AlphaMask3dPrepass>>,
        Res<DrawFunctions<Shadow>>,
    ),
    (
        mut opaque_phases,
        mut mask_phases,
        mut prepass_phases,
        mut mask_prepass_phases,
        mut shadow_phases,
    ): (
        ResMut<ViewBinnedRenderPhases<Opaque3d>>,
        ResMut<ViewBinnedRenderPhases<AlphaMask3d>>,
        ResMut<ViewBinnedRenderPhases<Opaque3dPrepass>>,
        ResMut<ViewBinnedRenderPhases<AlphaMask3dPrepass>>,
        ResMut<ViewBinnedRenderPhases<Shadow>>,
    ),
    cameras: Query<
        (Entity, &ExtractedView, &Msaa, Option<&ViewLightEntities>),
        With<ExtractedCamera>,
    >,
    (lights, sun): (
        Query<(Entity, &ExtractedView, &LightEntity)>,
        Query<&ExtractedDirectionalLight>,
    ),
    (device, queue, debug): (
        Res<RenderDevice>,
        Res<RenderQueue>,
        Option<Res<KitLodDebug>>,
    ),
    mut gpu: ResMut<KitGpu>,
) {
    let stale = std::mem::take(&mut gpu.queued);
    unqueue(&mut opaque_phases, &stale);
    unqueue(&mut mask_phases, &stale);
    unqueue(&mut prepass_phases, &stale);
    unqueue(&mut mask_prepass_phases, &stale);
    unqueue(&mut shadow_phases, &stale);
    let (Some(scene), Some(main), Some(prepass)) = (scene, main, prepass) else {
        return;
    };
    let draw_main = opaque_draw.read().id::<DrawKitMain>();
    let draw_mask = mask_draw.read().id::<DrawKitMain>();
    // Draw-function ids index each phase's own registry: an `Opaque3dPrepass` id queued into
    // `AlphaMask3dPrepass` names some other function there, and the masked slots never draw.
    let draw_prepass = prepass_draw.read().id::<DrawKitPrepass>();
    let draw_mask_prepass = mask_prepass_draw.read().id::<DrawKitPrepass>();
    let draw_shadow = shadow_draw.read().id::<DrawKitPrepass>();
    // One queue-side record per (slot, LOD) whole or band draw: the render mesh for the
    // key/topology, the material's pipeline key bits, and the cross-fade dither for band draws.
    struct Draw {
        entity: Entity,
        main_entity: MainEntity,
        slot_idx: usize,
        dither: MeshPipelineKey,
        mesh: AssetId<Mesh>,
        bin: UntypedAssetId,
        topo: MeshPipelineKey,
        bits: MeshPipelineKey,
        key: u64,
        mask: bool,
    }
    let mut draws: Vec<Draw> = vec![];
    for (e, me, g) in &groups {
        let Some(slot) = scene.0.slots.get(g.slot as usize) else {
            continue;
        };
        let Some(mesh) = slot.mesh[g.lod as usize] else {
            continue;
        };
        let Some(mesh) = render_meshes.get(mesh) else {
            continue;
        };
        let Some(prepared) = materials.get(slot.material.untyped()) else {
            continue;
        };
        // As bevy's `queue_material_meshes` does: the alpha mode's key (MAY_DISCARD for Mask, so
        // the alpha-cutoff foliage discards) is added per draw, not carried in the material bits.
        let mut bits: MeshPipelineKey = prepared.properties.mesh_pipeline_key_bits.downcast();
        bits.insert(alpha_mode_pipeline_key(
            prepared.properties.alpha_mode,
            &Msaa::Off,
        ));
        draws.push(Draw {
            entity: e,
            main_entity: *me,
            slot_idx: g.draw_index(),
            dither: if g.band {
                MeshPipelineKey::VISIBILITY_RANGE_DITHER
            } else {
                MeshPipelineKey::NONE
            },
            mesh: slot.mesh[g.lod as usize].expect("checked above"),
            bin: bin_key(slot.family, g.lod, g.band, g.slot),
            topo: MeshPipelineKey::from_primitive_topology_and_strip_index(
                mesh.primitive_topology(),
                mesh.index_format(),
            ),
            bits,
            key: slot.key,
            mask: matches!(
                prepared.properties.render_phase_type,
                RenderPhaseType::AlphaMask
            ),
        });
    }
    if draws.is_empty() {
        return;
    }
    let census = QUEUE_STATS_REQUESTED.swap(false, Ordering::Relaxed);
    let mut stats = KitQueueStats::default();
    let mut info = DrawMeshInfo::default();
    if census {
        for dr in &draws {
            let Some(m) = render_meshes.get(dr.mesh) else {
                continue;
            };
            let indices = match &m.buffer_info {
                RenderMeshBufferInfo::Indexed { count, .. } => u64::from(*count),
                RenderMeshBufferInfo::NonIndexed => u64::from(m.vertex_count),
            };
            info.insert(
                dr.slot_idx,
                (
                    u64::from(m.vertex_count),
                    indices,
                    m.layout.0.layout().array_stride,
                    m.layout.0.contains(Mesh::ATTRIBUTE_COLOR),
                ),
            );
        }
    }
    let d = scene.0.clone();
    gpu.draws.clear();
    gpu.ids.clear();
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    // LOD is per part from the main camera's distance to its origin, in every view, as bevy's
    // `VisibilityRange` resolves it per entity (shadow views measure from their camera too). A
    // forced debug LOD pins every chunk, with no band, as it pins every per-entity part's range.
    let eye = camera.1.world_from_view.translation();
    let force = debug.and_then(|d| d.force);
    let mut parts = std::mem::take(&mut gpu.parts);
    let lods = kit_frame_lods(&d, eye, force, &mut parts);
    // Shadow cascades whose shadow map no fragment on screen that the kit can shadow reads: the
    // kit draws nothing into them, which leaves every shaded pixel as it was (a strategic
    // camera's world lies past the first cascades and short of the last; a close camera looking
    // down sees none of the world nearer than the first cascade's far bound).
    let mut unsampled: Vec<Entity> = vec![];
    let ablate_clip = kit_ablate().cascade_clip;
    // Per read cascade, the cells whose shadowed region reaches a view depth the cascade is read
    // at: the same test per tile cell, so a cascade over part of the world takes only that part's
    // casters, and a slot whose parts all lie elsewhere draws nothing into it.
    let mut cell_masks: HashMap<Entity, Vec<bool>> = HashMap::default();
    let bounds = d
        .chunks
        .iter()
        .filter(|c| c.aabb.half_extents.cmpge(Vec3A::ZERO).all())
        .map(|c| (c.aabb.min(), c.aabb.max()))
        .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)));
    for (ve, view, _, cascade_views) in &cameras {
        let (Some((lo, hi)), Some(cascade_views)) = (bounds, cascade_views) else {
            continue;
        };
        let view_from_world = view.world_from_view.to_matrix().inverse();
        let clip_from_world = view.clip_from_view * view_from_world;
        for &cascade_view in &cascade_views.lights {
            let Ok((
                _,
                _,
                LightEntity::Directional {
                    light_entity,
                    cascade_index,
                },
            )) = lights.get(cascade_view)
            else {
                continue;
            };
            // Volumetric lighting samples the cascades along each view ray, not by surface depth.
            let Ok(light) = sun.get(*light_entity) else {
                continue;
            };
            if light.volumetric {
                continue;
            }
            let Some(cascade) = light.cascades.get(&ve).and_then(|c| c.get(*cascade_index)) else {
                continue;
            };
            let margin = SHADOW_REACH_M + SHADOW_REACH_TEXELS * cascade.texel_size;
            let dir = light.transform.forward().as_vec3();
            let range = if ablate_clip {
                swept_depth_range(lo.into(), hi.into(), dir, &view_from_world, margin)
            } else {
                shadowed_depth_range(
                    lo.into(),
                    hi.into(),
                    dir,
                    &view_from_world,
                    &clip_from_world,
                    margin,
                )
            };
            let range = match range {
                ShadowedDepth::Unbounded => continue,
                ShadowedDepth::OffScreen => {
                    unsampled.push(cascade_view);
                    continue;
                }
                ShadowedDepth::Range(near, far) => (near, far),
            };
            let config = &light.cascade_shadow_config;
            if !cascade_sampled(
                &config.bounds,
                config.overlap_proportion,
                *cascade_index,
                range,
            ) {
                unsampled.push(cascade_view);
                continue;
            }
            let floor = lo.y - margin;
            let mask = d
                .chunks
                .iter()
                .map(|c| {
                    if c.aabb.half_extents.cmplt(Vec3A::ZERO).any() {
                        return true;
                    }
                    let (near, far) = cell_shadowed_depth_range(
                        c.aabb.min().into(),
                        c.aabb.max().into(),
                        floor,
                        dir,
                        &view_from_world,
                        margin,
                    );
                    let (near, far) = (near.max(range.0), far.min(range.1));
                    near <= far
                        && cascade_sampled(
                            &config.bounds,
                            config.overlap_proportion,
                            *cascade_index,
                            (near, far),
                        )
                })
                .collect();
            cell_masks.insert(cascade_view, mask);
        }
    }
    for (ve, view, msaa, _) in &cameras {
        let Some(&view_key) = view_key_cache.get(&view.retained_view_entity) else {
            continue;
        };
        // Band draws dither on their own LOD band (the `KitSlotDraw` key), not on visibility ranges.
        let view_key = view_key - MeshPipelineKey::VISIBILITY_RANGE_DITHER;
        let clip_from_world = view.clip_from_view * view.world_from_view.to_matrix().inverse();
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let ranges = kit_view_ranges(
            &d,
            &lods,
            &parts,
            &frustum,
            false,
            None,
            gpu.ids.values_mut(),
        );
        for (slot_idx, r) in &ranges {
            gpu.draws.insert((ve, *slot_idx), r.clone());
        }
        if census {
            let mut v = view_census("main".into(), &ranges, &info);
            v.cells_visible = d
                .chunks
                .iter()
                .filter(|c| frustum.intersects_obb(&c.aabb, &Affine3A::IDENTITY, true, true))
                .count() as u32;
            stats.views.push(v);
        }
        // Main pass: opaque and alpha-mask phases.
        if let Some(phase) = opaque_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if !dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = main_pipelines.specialize(
                        &pipeline_cache,
                        &main,
                        KitKey {
                            mesh: view_key | dr.topo | dr.bits | dr.dither,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
                    ) else {
                        continue;
                    };
                    phase.add(
                        Opaque3dBatchSetKey {
                            pipeline,
                            draw_function: draw_main,
                            material_bind_group_index: None,
                            slabs: MeshSlabs::default(),
                            lightmap_slab: None,
                        },
                        Opaque3dBinKey { asset_id: dr.bin },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
        if let Some(phase) = mask_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = main_pipelines.specialize(
                        &pipeline_cache,
                        &main,
                        KitKey {
                            mesh: view_key | dr.topo | dr.bits | dr.dither,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
                    ) else {
                        continue;
                    };
                    phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            pipeline,
                            draw_function: draw_mask,
                            material_bind_group_index: None,
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey { asset_id: dr.bin },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
        // Prepass phases for cameras (normals/motion/depth).
        if let Some(phase) = prepass_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if !dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = prepass_pipelines.specialize(
                        &pipeline_cache,
                        &prepass,
                        KitKey {
                            mesh: MeshPipelineKey::from_msaa_samples(msaa.samples())
                                | (view_key & MeshPipelineKey::ALL_PREPASS_BITS)
                                | dr.topo
                                | dr.bits
                                | dr.dither,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
                    ) else {
                        continue;
                    };
                    phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            pipeline,
                            draw_function: draw_prepass,
                            material_bind_group_index: None,
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey { asset_id: dr.bin },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
        // Alpha-mask prepass: the same prepass phase machinery, mask phase only.
        if let Some(phase) = mask_prepass_phases.get_mut(&view.retained_view_entity) {
            for dr in &draws {
                if dr.mask && gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                    let Ok(pipeline) = prepass_pipelines.specialize(
                        &pipeline_cache,
                        &prepass,
                        KitKey {
                            mesh: MeshPipelineKey::from_msaa_samples(msaa.samples())
                                | (view_key & MeshPipelineKey::ALL_PREPASS_BITS)
                                | dr.topo
                                | dr.bits
                                | dr.dither,
                            mat: dr.key,
                        },
                        &render_meshes.get(dr.mesh).expect("mesh").layout,
                    ) else {
                        continue;
                    };
                    phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            pipeline,
                            draw_function: draw_mask_prepass,
                            material_bind_group_index: None,
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey { asset_id: dr.bin },
                        (dr.entity, dr.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            }
        }
    }
    for (ve, view, light) in &lights {
        let LightEntity::Directional { cascade_index, .. } = light else {
            continue;
        };
        if unsampled.contains(&ve) {
            if census {
                stats.views.push(KitQueueView {
                    view: format!("cascade{cascade_index}"),
                    skipped: true,
                    ..default()
                });
            }
            continue;
        }
        let Some(phase) = shadow_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        // Shadow views cull in their own frustum; cascade LODs follow the main camera's.
        let clip_from_world = view.clip_from_view * view.world_from_view.to_matrix().inverse();
        let frustum = Frustum(ViewFrustum::from_clip_from_world(&clip_from_world));
        let mask = cell_masks.get(&ve).map(Vec::as_slice);
        let ranges = kit_view_ranges(
            &d,
            &lods,
            &parts,
            &frustum,
            true,
            mask,
            gpu.ids.values_mut(),
        );
        for (slot_idx, r) in &ranges {
            gpu.draws.insert((ve, *slot_idx), r.clone());
        }
        if census {
            let mut v = view_census(format!("cascade{cascade_index}"), &ranges, &info);
            v.cells_visible = d
                .chunks
                .iter()
                .enumerate()
                .filter(|(ci, c)| {
                    mask.is_none_or(|m| m.get(*ci).copied().unwrap_or(true))
                        && frustum.intersects_obb(&c.aabb, &Affine3A::IDENTITY, false, true)
                })
                .count() as u32;
            stats.views.push(v);
        }
        let key = MeshPipelineKey::DEPTH_PREPASS | MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO;
        for dr in &draws {
            if !gpu.draws.contains_key(&(ve, dr.slot_idx)) {
                continue;
            }
            let Ok(pipeline) = prepass_pipelines.specialize(
                &pipeline_cache,
                &prepass,
                KitKey {
                    mesh: key | dr.topo | dr.bits,
                    mat: dr.key,
                },
                &render_meshes.get(dr.mesh).expect("mesh").layout,
            ) else {
                continue;
            };
            phase.add(
                ShadowBatchSetKey {
                    pipeline,
                    draw_function: draw_shadow,
                    material_bind_group_index: None,
                    slabs: MeshSlabs::default(),
                },
                ShadowBinKey { asset_id: dr.bin },
                (dr.entity, dr.main_entity),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
    }
    gpu.queued = draws.iter().map(|dr| dr.main_entity).collect();
    gpu.parts = parts;
    if census {
        stats.slots = d.slots.len() as u32;
        stats.cells = d.chunks.len() as u32;
        stats.bind_groups_created = gpu.bind_groups_made;
        if let Ok(mut s) = QUEUE_STATS.lock() {
            *s = Some(stats);
        }
    }
    if gpu.ids.is_empty() {
        gpu.ids.push(0);
    }
    gpu.ids.write_buffer(&device, &queue);
}
// ---------------------------------------------------------------------------------------------
// Draw commands.

type DrawKitMain = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetKitInstancesBindGroup,
    SetKitMaterialBindGroup,
    DrawKitInstances,
);

type DrawKitPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetKitInstancesBindGroup,
    SetKitMaterialBindGroup,
    DrawKitInstances,
);

/// Binds the slot's kit material at the material bind group. Bevy's `SetMaterialBindGroup` looks
/// the material up in `RenderMaterialInstances`, which extraction only fills for `Mesh3d`
/// entities; the kit draw slots have no `Mesh3d`, so it would Skip and — tuples abort on Skip —
/// silently drop the whole draw.
struct SetKitMaterialBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetKitMaterialBindGroup {
    type Param = (
        SRes<KitGpu>,
        SRes<ErasedRenderAssets<PreparedMaterial>>,
        SRes<MaterialBindGroupAllocators>,
    );
    type ViewQuery = ();
    type ItemQuery = Read<KitSlotDraw>;

    fn render<'w>(
        _item: &P,
        _view: (),
        group: Option<&'w KitSlotDraw>,
        (gpu, materials, allocators): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (Some(group), Some(scene)) = (group, gpu.into_inner().scene.as_ref()) else {
            return RenderCommandResult::Skip;
        };
        let Some(slot) = scene.slots.get(group.slot as usize) else {
            return RenderCommandResult::Skip;
        };
        let (materials, allocators) = (materials.into_inner(), allocators.into_inner());
        let Some(prepared) = materials.get(slot.material.untyped()) else {
            return RenderCommandResult::Skip;
        };
        let Some(bind_group) = allocators
            .get(&slot.material.untyped().type_id())
            .and_then(|allocator| allocator.get(prepared.binding.group))
            .and_then(|bind_group| bind_group.bind_group())
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(MATERIAL_BIND_GROUP_INDEX, bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// Binds the record + id buffers and the draw's metadata entry at group 2.
struct SetKitInstancesBindGroup;

impl<P: PhaseItem> RenderCommand<P> for SetKitInstancesBindGroup {
    type Param = SRes<KitGpu>;
    type ViewQuery = ();
    type ItemQuery = Read<KitSlotDraw>;

    fn render<'w>(
        _item: &P,
        _view: (),
        group: Option<&'w KitSlotDraw>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(group) = group else {
            return RenderCommandResult::Skip;
        };
        let gpu = gpu.into_inner();
        let Some(bind_group) = gpu.bind_groups.get(group.meta_index()) else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(2, bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// Binds the slot's mesh at group 2's vertex slots and draws the view's instance range of it.
struct DrawKitInstances;

impl<P: PhaseItem> RenderCommand<P> for DrawKitInstances {
    type Param = (
        SRes<KitGpu>,
        SRes<RenderAssets<RenderMesh>>,
        SRes<MeshAllocator>,
    );
    type ViewQuery = Entity;
    type ItemQuery = Read<KitSlotDraw>;

    fn render<'w>(
        _item: &P,
        view: Entity,
        group: Option<&'w KitSlotDraw>,
        (gpu, meshes, allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let (gpu, meshes, allocator) = (
            gpu.into_inner(),
            meshes.into_inner(),
            allocator.into_inner(),
        );
        let (Some(group), Some(scene)) = (group, gpu.scene.as_ref()) else {
            return RenderCommandResult::Skip;
        };
        let Some(range) = gpu.draws.get(&(view, group.draw_index())) else {
            return RenderCommandResult::Skip;
        };
        let instances = range.end - range.start;
        let id = scene.slots[group.slot as usize].mesh[group.lod as usize].expect("slotted mesh");
        let (Some(mesh), Some(vertices)) = (meshes.get(id), allocator.mesh_vertex_slice(&id))
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_vertex_buffer(0, vertices.buffer.slice(..));
        // The record ids for this draw as an instanced attribute: one u32 per instance,
        // starting at 0 for every draw (WebGPU's instance_index is 0-based per draw; Metal's
        // [[instance_id]] is absolute — attributes sidestep the divergence).
        pass.set_vertex_buffer(
            1,
            gpu.ids
                .buffer()
                .expect("ids buffer")
                .slice((range.start * 4) as u64..(range.end * 4) as u64),
        );
        match &mesh.buffer_info {
            RenderMeshBufferInfo::Indexed {
                index_format,
                count,
            } => {
                let Some(indices) = allocator.mesh_index_slice(&id) else {
                    return RenderCommandResult::Skip;
                };
                pass.set_index_buffer(indices.buffer.slice(..), *index_format);
                pass.draw_indexed(
                    indices.range.start..indices.range.start + count,
                    vertices.range.start as i32,
                    0..instances,
                );
            }
            RenderMeshBufferInfo::NonIndexed => {
                pass.draw(vertices.range, 0..instances);
            }
        }
        RenderCommandResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draw order within a pipeline follows the family, then the LOD, whatever slot index or
    /// mesh load order a run produced, and no two (slot, LOD, band) draws share a bin.
    #[test]
    fn bins_order_by_family_then_lod() {
        assert!(bin_key(3, 0, true, 9) < bin_key(4, 0, false, 0));
        assert!(bin_key(3, 0, true, 9) < bin_key(3, 1, false, 0));
        assert!(bin_key(3, 0, false, 9) < bin_key(3, 0, true, 0));
        assert!(bin_key(3, 1, false, 0) < bin_key(3, 1, false, 1));
        let keys: std::collections::HashSet<_> = (0..4u32)
            .flat_map(|f| {
                (0..3u8).flat_map(move |l| [false, true].map(move |b| bin_key(f, l, b, f)))
            })
            .collect();
        assert_eq!(keys.len(), 24);
    }

    /// Binned phases keep last frame's items: a slot culled from a cascade (or despawned by a
    /// rebuild) stays binned, and drawn as a skipped item, until it is removed from every view.
    #[test]
    fn unqueue_removes_last_frames_slots_from_every_view() {
        use bevy::render::batching::gpu_preprocessing::GpuPreprocessingMode;
        use bevy::{
            asset::AssetIndex, material::labels::DrawFunctionId, render::view::RetainedViewEntity,
        };
        let mut phases = ViewBinnedRenderPhases::<Shadow>::default();
        let views = [0, 1].map(|i| {
            RetainedViewEntity::new(
                MainEntity::from(Entity::from_raw_u32(100 + i).unwrap()),
                None,
                i,
            )
        });
        let slots = [1, 2].map(|i| {
            let e = Entity::from_raw_u32(i).unwrap();
            (e, MainEntity::from(e))
        });
        for v in views {
            phases.prepare_for_new_frame(v, GpuPreprocessingMode::Culling);
            let phase = phases.get_mut(&v).unwrap();
            for (i, s) in slots.into_iter().enumerate() {
                phase.add(
                    ShadowBatchSetKey {
                        pipeline: CachedRenderPipelineId::INVALID,
                        draw_function: DrawFunctionId(0),
                        material_bind_group_index: None,
                        slabs: MeshSlabs::default(),
                    },
                    ShadowBinKey {
                        asset_id: AssetId::<Mesh>::from(AssetIndex::from_bits(i as u64)).untyped(),
                    },
                    s,
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
        let count = |phases: &ViewBinnedRenderPhases<Shadow>| -> Vec<usize> {
            views
                .iter()
                .map(|v| {
                    phases[v]
                        .non_mesh_items
                        .values()
                        .map(|b| b.entities.len())
                        .sum()
                })
                .collect()
        };
        assert_eq!(count(&phases), [2, 2]);
        unqueue(&mut phases, &[slots[0].1]);
        assert_eq!(count(&phases), [1, 1]);
        unqueue(&mut phases, &[slots[1].1]);
        assert_eq!(count(&phases), [0, 0]);
        assert!(views.iter().all(|v| phases[v].is_empty()));
    }

    fn view_from_world_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        Transform::from_translation(eye)
            .looking_at(target, up)
            .to_matrix()
            .inverse()
    }

    /// `(view_from_world, clip_from_world)` of bevy's default perspective camera (45 degrees
    /// vertical, near 0.1 m, infinite reverse-z) at `eye` looking at `target`, `aspect` wide.
    fn camera_at(eye: Vec3, target: Vec3, fov_deg: f32, aspect: f32) -> (Mat4, Mat4) {
        let view_from_world = view_from_world_at(eye, target, Vec3::Y);
        use bevy::camera::CameraProjection;
        let clip_from_view = PerspectiveProjection {
            fov: fov_deg.to_radians(),
            aspect_ratio: aspect,
            near: 0.1,
            ..default()
        }
        .get_clip_from_view();
        (view_from_world, clip_from_view * view_from_world)
    }

    /// The 20-tile scatter world's kit bounds (`draws.json`), loosely.
    const WORLD: (Vec3, Vec3) = (Vec3::new(-40.0, -3.0, -60.0), Vec3::new(80.0, 20.0, 60.0));

    /// Play's cascades: 4 out to 2,000 m, the first ending at 25 m.
    fn play_cascades() -> bevy::light::CascadeShadowConfig {
        bevy::light::CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: 2000.0,
            first_cascade_far_bound: crate::lighting::KIT_FIRST_CASCADE_FAR_BOUND,
            ..default()
        }
        .build()
    }

    /// Which of play's cascades a 1600x1000 camera at `eye` looking at `target` reads for the
    /// world's kit under the kit sun.
    fn cascades_read(eye: Vec3, target: Vec3) -> Vec<bool> {
        let (view, clip) = camera_at(eye, target, 45.0, 1.6);
        let dir = -crate::lighting::KIT_SUN_DIR.normalize();
        let ShadowedDepth::Range(near, far) =
            shadowed_depth_range(WORLD.0, WORLD.1, dir, &view, &clip, 4.0)
        else {
            panic!("the world is on screen under a high sun");
        };
        let config = play_cascades();
        (0..4)
            .map(|i| cascade_sampled(&config.bounds, config.overlap_proportion, i, (near, far)))
            .collect()
    }

    /// The scatter checkpoint's strategic camera over its 20-tile world under the kit sun: the
    /// world's view depths lie past cascade 1 and short of cascade 3, so neither is read.
    #[test]
    fn strategic_camera_reads_only_the_cascades_over_the_world() {
        let read = cascades_read(Vec3::new(20.0, 160.0, 150.0), Vec3::new(20.0, 0.0, -5.0));
        assert_eq!(read, [false, false, true, false]);
    }

    /// The forest-cluster camera (`draws.json`) looks down from 58 m: the world under and behind
    /// it is off screen, and the nearest shadowed point on screen lies past the first cascade's
    /// reach, so cascade 0 is not read. Unclipped, the world's sweep reaches under the camera.
    #[test]
    fn close_camera_looking_down_skips_the_first_cascade() {
        let (eye, target) = (Vec3::new(46.0, 58.0, 48.0), Vec3::new(46.0, 0.0, -12.0));
        assert_eq!(cascades_read(eye, target), [false, true, true, false]);
        // A frustum wide enough to see straight down puts the world back under the camera.
        let (view, clip) = camera_at(eye, target, 170.0, 1.6);
        let dir = -crate::lighting::KIT_SUN_DIR.normalize();
        let ShadowedDepth::Range(near, _) =
            shadowed_depth_range(WORLD.0, WORLD.1, dir, &view, &clip, 4.0)
        else {
            panic!("on screen");
        };
        assert!(near < 20.0, "{near}");
    }

    /// The meadow camera (`draws.json`) sits 13 m over the world: its near cascade is read.
    #[test]
    fn low_camera_over_the_world_reads_the_first_cascade() {
        let read = cascades_read(Vec3::new(6.0, 13.0, 44.0), Vec3::new(12.0, 0.0, 22.0));
        assert!(read[0] && read[1], "{read:?}");
    }

    /// A camera inside the world (walk mode) reads its near cascades.
    #[test]
    fn camera_in_the_world_reads_the_near_cascades() {
        let read = cascades_read(Vec3::new(0.0, 1.7, 0.0), Vec3::new(10.0, 1.7, 0.0));
        assert!(read[0] && read[1], "{read:?}");
    }

    /// Casters whose whole shadowed region lies behind the camera put nothing on screen: no
    /// cascade is read.
    #[test]
    fn shadowed_region_behind_the_camera_is_off_screen() {
        let (view, clip) = camera_at(Vec3::ZERO, Vec3::NEG_Z, 45.0, 1.6);
        let (lo, hi) = (Vec3::new(-5.0, -1.0, 10.0), Vec3::new(5.0, 3.0, 20.0));
        let got = shadowed_depth_range(lo, hi, Vec3::NEG_Y, &view, &clip, 0.0);
        assert_eq!(got, ShadowedDepth::OffScreen);
        // Off to the side, past the frustum's edge, is off screen too.
        let (lo, hi) = (Vec3::new(40.0, -1.0, -20.0), Vec3::new(50.0, 3.0, -10.0));
        let got = shadowed_depth_range(lo, hi, Vec3::NEG_Y, &view, &clip, 0.0);
        assert_eq!(got, ShadowedDepth::OffScreen);
    }

    /// A box the camera stands inside reaches the near plane; one that fills the view from the
    /// side is clipped to what the frustum holds of it.
    #[test]
    fn shadowed_range_is_clipped_to_the_frustum() {
        let (view, clip) = camera_at(Vec3::ZERO, Vec3::NEG_Z, 90.0, 1.0);
        let around = shadowed_depth_range(
            Vec3::splat(-5.0),
            Vec3::splat(5.0),
            Vec3::NEG_Y,
            &view,
            &clip,
            0.0,
        );
        let ShadowedDepth::Range(near, far) = around else {
            panic!("{around:?}");
        };
        assert!(
            (near - 0.1).abs() < 1e-3 && (far - 5.0).abs() < 1e-3,
            "{near} {far}"
        );
        // A wall from x = -100 to 100 at z = -10: with a 90 degree view only |x| <= 10 of it is on
        // screen, all at depth 10 (the unclipped range would be the same here, but its far end
        // for a wall slanting away is not).
        let slant = shadowed_depth_range(
            Vec3::new(-100.0, -1.0, -10.0),
            Vec3::new(100.0, 1.0, -10.0),
            Vec3::NEG_Y,
            &view,
            &clip,
            0.0,
        );
        let ShadowedDepth::Range(near, far) = slant else {
            panic!("{slant:?}");
        };
        assert!(
            (near - 10.0).abs() < 1e-3 && (far - 10.0).abs() < 1e-3,
            "{near} {far}"
        );
        // A floor running away from the camera: the frustum's bottom plane ends it nowhere, so
        // its far end is the box's.
        let floor = shadowed_depth_range(
            Vec3::new(-1.0, -2.0, -50.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::NEG_Y,
            &view,
            &clip,
            0.0,
        );
        let ShadowedDepth::Range(near, far) = floor else {
            panic!("{floor:?}");
        };
        assert!(
            (near - 1.0).abs() < 1e-3 && (far - 50.0).abs() < 1e-3,
            "{near} {far}"
        );
    }

    /// A cell's unclipped range holds the clipped range of the same shadowed region, for cells
    /// on and off screen, near and far, under the kit sun and a slanting one.
    #[test]
    fn cell_range_holds_the_clipped_range() {
        let (view, clip) = camera_at(
            Vec3::new(12.0, 172.0, 165.0),
            Vec3::new(12.0, 0.0, -2.0),
            45.0,
            1.6,
        );
        let suns = [
            -crate::lighting::KIT_SUN_DIR.normalize(),
            Vec3::new(0.7, -0.3, -0.2).normalize(),
        ];
        for dir in suns {
            for x in [-120.0f32, -40.0, 0.0, 40.0, 120.0] {
                for z in [-160.0f32, -60.0, 0.0, 60.0, 140.0] {
                    let (lo, hi) = (Vec3::new(x, -3.0, z), Vec3::new(x + 28.0, 20.0, z + 32.0));
                    let (near, far) =
                        cell_shadowed_depth_range(lo, hi, lo.y - 4.0, dir, &view, 4.0);
                    if let ShadowedDepth::Range(n, f) =
                        shadowed_depth_range(lo, hi, dir, &view, &clip, 4.0)
                    {
                        assert!(
                            near <= n + 1e-3 && f <= far + 1e-3,
                            "{x} {z}: {n}..{f} in {near}..{far}"
                        );
                    }
                }
            }
        }
    }

    /// From the mixed world's overview pose, a tile near the camera shadows nothing cascade 3 is
    /// read at, while one at the world's far edge does; sweeping to a lower floor only widens it.
    #[test]
    fn far_cascade_takes_only_the_far_cells() {
        let (view, _) = camera_at(
            Vec3::new(12.0, 172.0, 165.0),
            Vec3::new(12.0, 0.0, -2.0),
            45.0,
            1.6,
        );
        let dir = -crate::lighting::KIT_SUN_DIR.normalize();
        let config = play_cascades();
        let reads = |z: f32, floor: f32| {
            let (lo, hi) = (Vec3::new(0.0, -3.0, z), Vec3::new(28.0, 20.0, z + 32.0));
            let r = cell_shadowed_depth_range(lo, hi, floor, dir, &view, 4.0);
            (0..4)
                .map(|i| cascade_sampled(&config.bounds, config.overlap_proportion, i, r))
                .collect::<Vec<_>>()
        };
        assert_eq!(reads(60.0, -7.0), [false, false, true, false]);
        assert!(reads(-260.0, -7.0)[3]);
        let (lo, hi) = (Vec3::new(0.0, -3.0, 0.0), Vec3::new(28.0, 20.0, 32.0));
        let (a, b) = cell_shadowed_depth_range(lo, hi, -7.0, dir, &view, 4.0);
        let (c, e) = cell_shadowed_depth_range(lo, hi, -40.0, dir, &view, 4.0);
        assert!(c <= a && b <= e, "{a}..{b} in {c}..{e}");
    }

    /// A cascade is read from the overlap band before its near bound, and not at its far bound.
    #[test]
    fn cascade_sampled_follows_bevys_selection_and_blend() {
        let bounds = [25.0, 108.0, 464.0];
        // 0.8 * 25 = 20: a fragment at 21 m blends cascade 1 into cascade 0.
        assert!(cascade_sampled(&bounds, 0.2, 1, (21.0, 21.0)));
        assert!(!cascade_sampled(&bounds, 0.2, 1, (19.0, 19.9)));
        assert!(!cascade_sampled(&bounds, 0.2, 0, (25.0, 30.0)));
        assert!(cascade_sampled(&bounds, 0.2, 0, (-5.0, 0.0)));
        // Past the last bound no cascade is read; an unknown cascade is kept.
        assert!(!cascade_sampled(&bounds, 0.2, 2, (464.0, 900.0)));
        assert!(cascade_sampled(&bounds, 0.2, 3, (0.0, 1.0)));
    }

    /// The sweep follows the light down to the floor, so a low sun reaches farther; a sun at the
    /// horizon gives no bound and nothing is skipped.
    #[test]
    fn shadowed_range_sweeps_along_the_light() {
        let (view, clip) = camera_at(Vec3::ZERO, Vec3::NEG_Z, 90.0, 1.0);
        let (lo, hi) = (Vec3::new(-1.0, 0.0, -11.0), Vec3::new(1.0, 10.0, -9.0));
        let high = shadowed_depth_range(lo, hi, Vec3::NEG_Y, &view, &clip, 0.0);
        assert_eq!(high, ShadowedDepth::Range(9.0, 11.0));
        let low = shadowed_depth_range(
            lo,
            hi,
            Vec3::new(0.0, -1.0, -1.0).normalize(),
            &view,
            &clip,
            0.0,
        );
        let ShadowedDepth::Range(_, far) = low else {
            panic!("{low:?}");
        };
        assert!((far - 21.0).abs() < 1e-4, "{low:?}");
        assert_eq!(
            shadowed_depth_range(lo, hi, Vec3::NEG_Z, &view, &clip, 0.0),
            ShadowedDepth::Unbounded
        );
    }

    /// A part is whole at one LOD outside the 10 m bands around 150 m and 400 m and in both
    /// neighbours' band draws inside them; the last LOD never fades out.
    #[test]
    fn part_lod_cross_fades_over_the_bands_at_150_and_400_m() {
        use PartLod::{Band, Whole};
        let at = kit_part_lod;
        assert_eq!([at(0.0), at(140.0), at(144.99)], [Whole(0); 3]);
        assert_eq!([at(145.0), at(150.0), at(154.99)], [Band(0); 3]);
        assert_eq!([at(155.0), at(160.0), at(394.99)], [Whole(1); 3]);
        assert_eq!([at(395.0), at(400.0), at(404.99)], [Band(1); 3]);
        assert_eq!([at(405.0), at(1.0e6)], [Whole(2); 2]);
        // The band is the per-entity VisibilityRange's: contiguous, LOD l fading out where l + 1
        // fades in.
        for l in 0..2 {
            let (a, b) = (kit_lod_range(l), kit_lod_range(l + 1));
            assert_eq!([a[2], a[3]], [b[0], b[1]]);
            assert_eq!(a[3] - a[2], 10.0);
        }
        // In a camera view a band part draws in both LODs' band draws only; in a shadow view in
        // the outgoing LOD's whole draw alone (owner A15 Q1).
        let draws = |p: PartLod, shadow: bool| -> Vec<bool> {
            (0..3)
                .flat_map(|l| [false, true].map(|b| p.draws(l, b, shadow)))
                .collect()
        };
        assert_eq!(
            draws(Band(0), false),
            [false, true, false, true, false, false]
        );
        assert_eq!(
            draws(Band(0), true),
            [true, false, false, false, false, false]
        );
        assert_eq!(
            draws(Band(1), true),
            [false, false, true, false, false, false]
        );
        assert_eq!(
            draws(Whole(1), false),
            [false, false, true, false, false, false]
        );
        assert_eq!(draws(Whole(1), true), draws(Whole(1), false));
    }

    fn piece_at(x: f32) -> GpuKitInstance {
        GpuKitInstance {
            rotation: Quat::IDENTITY.to_array(),
            translation: [x, 0.0, 0.0],
            tag: 0,
            scale: [1.0; 3],
            _pad: 0,
        }
    }

    /// One slot, one chunk of parts at `xs` along +x.
    fn row(xs: &[f32]) -> KitInstanceSetData {
        let lo = xs.iter().copied().fold(f32::MAX, f32::min);
        let hi = xs.iter().copied().fold(f32::MIN, f32::max);
        let mut set = KitInstanceSetData {
            pieces: xs.iter().map(|&x| piece_at(x)).collect(),
            slots: vec![KitSlot {
                mesh: [None, None, None],
                tris: [12; 3],
                material: AssetId::<KitMaterial>::default().untyped().typed(),
                key: 0,
                family: 0,
                piece: "test".into(),
            }],
            chunks: vec![KitChunk {
                centre: Vec3::new((lo + hi) / 2.0, 0.0, 0.0),
                aabb: Aabb::from_min_max(
                    Vec3::new(lo - 2.0, -1.0, -2.0),
                    Vec3::new(hi + 2.0, 1.0, 2.0),
                ),
                origins: Aabb::from_min_max(Vec3::new(lo, 0.0, 0.0), Vec3::new(hi, 0.0, 0.0)),
                ranges: vec![(
                    0,
                    Range {
                        start: 0,
                        end: xs.len() as u32,
                    },
                )],
            }],
            meta: vec![],
            version: 1,
            by_slot: vec![],
        };
        set.index_slots();
        set
    }

    /// The (draw, records) lists a view of `set` from `eye` draws.
    fn drawn(
        set: &KitInstanceSetData,
        eye: Vec3,
        force: Option<u8>,
        shadow: bool,
    ) -> Vec<(usize, Vec<u32>)> {
        let mut parts = vec![];
        let lods = kit_frame_lods(set, eye, force, &mut parts);
        let mut ids = vec![];
        kit_view_ranges(
            set,
            &lods,
            &parts,
            &enclosing_frustum(),
            shadow,
            None,
            &mut ids,
        )
        .into_iter()
        .map(|(k, r)| (k, ids[r.start as usize..r.end as usize].to_vec()))
        .collect()
    }

    /// A slot with no LOD1 (or LOD2) mesh draws its LOD0 mesh through the LOD1 range (owner A2
    /// Q4): whole across the 150 m band, where both sides are the same mesh, and fading out over
    /// the 400 m band, in camera and shadow views alike; its LOD0 metadata carries that range.
    #[test]
    fn a_slot_without_lod1_draws_lod0_through_the_lod1_range() {
        let mut set = row(&[140.0, 150.0, 200.0, 400.0, 500.0]);
        set.slots[0].mesh = [Some(AssetId::default()), None, None];
        assert_eq!(slot_lod_map(&set.slots[0]), [Some(0), Some(0), None]);
        let eye = Vec3::ZERO;
        assert_eq!(
            drawn(&set, eye, None, false),
            vec![(draw_index(0, true), vec![0, 1, 2, 3])]
        );
        assert_eq!(
            drawn(&set, eye, None, true),
            vec![(draw_index(0, false), vec![0, 1, 2, 3])]
        );
        assert_eq!(
            drawn(&row(&[200.0]), eye, None, false),
            vec![(draw_index(1, false), vec![0])],
            "a full chain draws LOD1 there"
        );
        assert_eq!(
            slot_lod_range(set.slots.first(), 0),
            [0.0, 0.0, 395.0, 405.0]
        );
        assert_eq!(
            slot_lod_range(row(&[0.0]).slots.first(), 1),
            kit_lod_range(1)
        );
        assert_eq!(
            drawn(&set, eye, Some(1), false),
            vec![(draw_index(0, false), vec![0, 1, 2, 3, 4])]
        );
    }

    /// A chunk straddling the 150 m band splits per part: the part short of it draws at LOD0, the
    /// part past it at LOD1 and the part in it at both, each LOD in one band (dithered) draw since
    /// it holds a band part. Shadow views draw the band part in the outgoing LOD0's whole draw
    /// alone (owner A15 Q1), and a forced LOD pins all of them with no band.
    #[test]
    fn chunk_across_the_band_draws_each_part_at_its_own_lod() {
        let set = row(&[140.0, 150.0, 160.0]);
        let eye = Vec3::ZERO;
        assert_eq!(kit_chunk_lod(&set.chunks[0], eye), ChunkLod::Split);
        let whole0 = draw_index(0, false);
        let band0 = draw_index(0, true);
        let whole1 = draw_index(1, false);
        let band1 = draw_index(1, true);
        assert_eq!(
            drawn(&set, eye, None, false),
            vec![(band0, vec![0, 1]), (band1, vec![2, 1])]
        );
        // Past the band on both sides: two whole draws, no dither.
        assert_eq!(
            drawn(&row(&[140.0, 160.0]), eye, None, false),
            vec![(whole0, vec![0]), (whole1, vec![1])]
        );
        assert_eq!(
            drawn(&set, eye, None, true),
            vec![(whole0, vec![0, 1]), (whole1, vec![2])]
        );
        assert_eq!(
            drawn(&set, eye, Some(1), false),
            vec![(whole1, vec![0, 1, 2])]
        );
        // Seen from 100 m nearer, the whole chunk is short of the band: one LOD, no split.
        let near = Vec3::new(100.0, 0.0, 0.0);
        assert_eq!(
            kit_chunk_lod(&set.chunks[0], near),
            ChunkLod::All(PartLod::Whole(0))
        );
        assert_eq!(
            drawn(&set, near, None, false),
            vec![(whole0, vec![0, 1, 2])]
        );
        // A chunk wholly inside the band draws all of it in both band draws.
        let inside = row(&[146.0, 154.0]);
        assert_eq!(
            kit_chunk_lod(&inside.chunks[0], eye),
            ChunkLod::All(PartLod::Band(0))
        );
        assert_eq!(
            drawn(&inside, eye, None, false),
            vec![(band0, vec![0, 1]), (band1, vec![0, 1])]
        );
    }

    /// The meta entry and the draw key stay apart: the whole and band draws of one (slot, LOD)
    /// share its bind group and keep their own instance ranges.
    #[test]
    fn band_draws_share_the_meta_entry_and_keep_their_own_range() {
        let whole = KitSlotDraw {
            slot: 4,
            lod: 2,
            band: false,
        };
        let band = KitSlotDraw {
            band: true,
            ..whole
        };
        assert_eq!([whole.meta_index(), band.meta_index()], [14, 14]);
        assert_eq!([whole.draw_index(), band.draw_index()], [28, 29]);
        assert_eq!(META_SIZE, 80);
    }

    /// One chunk, one slot, two instances: visible views draw both exactly once; a frustum
    /// missing the chunk draws nothing; shadow views use the same ranges.
    #[test]
    fn view_ranges_draw_each_visible_instance_once() {
        let set = row(&[0.0, 1.0]);
        let lods = [ChunkLod::All(PartLod::Whole(0))];
        let (mut ids, mut seen) = (vec![], vec![]);
        let enclosing = enclosing_frustum();
        let ranges = kit_view_ranges(&set, &lods, &[], &enclosing, false, None, &mut ids);
        assert_eq!(ranges, vec![(0, 0..2)]);
        assert_eq!(ids.len(), 2);
        kit_view_ranges(&set, &lods, &[], &enclosing, true, None, &mut seen);
        assert_eq!(seen.len(), 2);
        let (mut ids, mut miss) = (vec![], vec![]);
        let other = frustum_around(Vec3::new(1_000_000.0, 0.0, 0.0));
        assert!(kit_view_ranges(&set, &lods, &[], &other, false, None, &mut ids).is_empty());
        assert!(ids.is_empty());
        kit_view_ranges(&set, &lods, &[], &other, true, None, &mut miss);
        assert!(miss.is_empty());
    }

    /// Per-tile cells hold sparse slot ranges: a view that sees one cell draws only the slots that
    /// cell holds, with only its records, and a view over both draws each slot once across them.
    #[test]
    fn cells_cull_their_own_sparse_slot_ranges() {
        let mut set = row(&[0.0, 1.0, 2.0, 100.0, 101.0]);
        set.slots.push(set.slots[0].clone());
        let cell = |x: f32, ranges: Vec<(u32, Range<u32>)>| KitChunk {
            centre: Vec3::new(x, 0.0, 0.0),
            aabb: Aabb::from_min_max(Vec3::new(x - 3.0, -1.0, -2.0), Vec3::new(x + 3.0, 1.0, 2.0)),
            origins: Aabb::from_min_max(Vec3::new(x, 0.0, 0.0), Vec3::new(x + 2.0, 0.0, 0.0)),
            ranges,
        };
        // Slot 0 has records 0..2 in cell 0; slot 1 has record 2 in cell 0 and 3..5 in cell 1.
        set.chunks = vec![
            cell(0.0, vec![(0, 0..2), (1, 2..3)]),
            cell(100.0, vec![(1, 3..5)]),
        ];
        set.index_slots();
        assert_eq!(
            set.by_slot,
            vec![vec![(0, 0..2)], vec![(0, 2..3), (1, 3..5)]]
        );
        let lods = [ChunkLod::All(PartLod::Whole(0)); 2];
        // An orthographic box 10 m either side of `x`.
        let around = |x: f32| {
            let clip = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
                * Mat4::from_scale(Vec3::new(0.1, 0.1, 0.05))
                * Mat4::from_translation(Vec3::new(-x, 0.0, 0.0));
            Frustum(ViewFrustum::from_clip_from_world(&clip))
        };
        let view = |f: &Frustum| {
            let mut ids = vec![];
            let ranges = kit_view_ranges(&set, &lods, &[], f, false, None, &mut ids);
            ranges
                .into_iter()
                .map(|(d, r)| (d, ids[r.start as usize..r.end as usize].to_vec()))
                .collect::<Vec<_>>()
        };
        let (slot0, slot1) = (draw_index(0, false), draw_index(3, false));
        assert_eq!(view(&around(0.0)), [(slot0, vec![0, 1]), (slot1, vec![2])]);
        assert_eq!(view(&around(100.0)), [(slot1, vec![3, 4])]);
        assert_eq!(
            view(&enclosing_frustum()),
            [(slot0, vec![0, 1]), (slot1, vec![2, 3, 4])]
        );
    }

    fn enclosing_frustum() -> Frustum {
        // Orthographic box of ±5 km with clip z in 0..1, enclosing the chunk.
        let clip = Mat4::from_translation(Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(1.0 / 5_000.0, 1.0 / 5_000.0, 1.0 / 10_000.0));
        Frustum(ViewFrustum::from_clip_from_world(&clip))
    }

    fn frustum_around(centre: Vec3) -> Frustum {
        let clip = Mat4::from_translation(centre + Vec3::new(0.0, 0.0, 0.5))
            * Mat4::from_scale(Vec3::new(1.0 / 10.0, 1.0 / 10.0, 1.0 / 20.0));
        Frustum(ViewFrustum::from_clip_from_world(&clip))
    }
}
