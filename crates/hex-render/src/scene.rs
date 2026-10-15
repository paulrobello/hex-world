use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use hex_core::{Axial, HEX_RADIUS, hex_prism, spiral};

use crate::{
    ActiveScenario, ActiveView, Scenario,
    chunks::{chunk_tiles, merge_by_material},
    post::{camera_post_bundle, camera_ssao},
    workload::{
        GATE, GateConfig, GateView, Layout, PostMode, ShadowCasters, piece_material, unit_hash,
    },
};

pub const TILE_HEIGHT: f32 = 4.0;

/// Marks everything `setup` spawns so a scenario switch can despawn and respawn the scene.
/// Contract: every world entity (tiles, pieces, vehicles, camera, lights) carries it.
#[derive(Component)]
pub struct SceneEntity;

#[derive(Component)]
pub struct Tile;

/// LOD level of one rendered LOD part: gate pieces and chunk meshes, play kit pieces and pair
/// edges, car parts and the kit scenario's pieces. `diag` counts the visible ones per level and the
/// debug overlays (`crate::debug`) tint and pin them.
#[derive(Component, Clone, Copy)]
pub struct Lod(pub u8);

/// What the current scene actually built, for diagnostics.
#[derive(Resource, Clone, Default)]
pub struct SceneFacts {
    pub tris_lod0_per_tile: usize,
    pub materials: usize,
    /// Logical static pieces (tiles × pieces per tile), whatever the layout.
    pub pieces: usize,
    pub layout: &'static str,
    /// Hexes per chunk, 0 for the entities layout.
    pub chunk_size: u32,
    pub chunks: usize,
}

/// Triangles drawn by this mesh entity, recorded at spawn because merged chunk meshes keep no
/// main-world copy once they reach the render world.
#[derive(Component, Clone, Copy)]
pub struct Tris(pub u32);

/// Parent of one chunk's merged meshes in the chunks layout; `tiles` is how many hexes it holds.
#[derive(Component)]
pub struct Chunk {
    pub tiles: u32,
}

/// A moving agent on a circle: `pos = center + (cos(t·ω + phase), 0, sin(t·ω + phase))·r`.
#[derive(Component)]
pub struct Vehicle {
    center: Vec3,
    radius: f32,
    omega: f32,
    phase: f32,
}

/// LOD switch distances (PRD §10): LOD0 below 150 m, LOD1 to 400 m, LOD2 beyond.
const LOD_NEAR: f32 = 150.0;
const LOD_FAR: f32 = 400.0;
const LOD_BAND: f32 = 5.0;
const LOD_END: f32 = 100_000.0;

pub(crate) fn lod_range(lod: usize) -> VisibilityRange {
    let near = LOD_NEAR - LOD_BAND..LOD_NEAR + LOD_BAND;
    let far = LOD_FAR - LOD_BAND..LOD_FAR + LOD_BAND;
    let (start_margin, end_margin) = match lod {
        0 => (0.0..0.0, near),
        1 => (near, far),
        _ => (far, LOD_END..LOD_END + 1.0),
    };
    VisibilityRange {
        start_margin,
        end_margin,
        use_aabb: false,
    }
}

/// Flat-shaded triangle soup with outward-facing winding.
#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Builder {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, outward: Vec3) {
        let n = (b - a).cross(c - a);
        let (b, c) = if n.dot(outward) < 0.0 { (c, b) } else { (b, c) };
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for p in [a, b, c] {
            self.indices.push(self.positions.len() as u32);
            self.positions.push(p.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push([p.x, p.z]);
        }
    }

    /// A `nu × nv` grid of quads spanning `u` and `v` from `origin`.
    fn grid(&mut self, origin: Vec3, u: Vec3, v: Vec3, nu: u32, nv: u32, outward: Vec3) {
        let at = |i: u32, j: u32| origin + u * (i as f32 / nu as f32) + v * (j as f32 / nv as f32);
        for i in 0..nu {
            for j in 0..nv {
                let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                self.tri(a, b, c, outward);
                self.tri(a, c, d, outward);
            }
        }
    }

    fn build(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn corner(radius: f32, i: usize) -> Vec3 {
    let a = (30.0 + 60.0 * i as f32).to_radians();
    Vec3::new(radius * a.cos(), 0.0, radius * a.sin())
}

/// Hex prism with a subdivided top cap: `6n²` cap triangles plus `12n` side triangles.
pub fn base_mesh(rings: u32, radius: f32, height: f32) -> Mesh {
    let mut b = Builder::default();
    let top = Vec3::Y * height;
    let n = rings as f32;
    for s in 0..6 {
        let (ca, cb) = (corner(radius, s) + top, corner(radius, (s + 1) % 6) + top);
        let p = |i: u32, j: u32| top + (ca - top) * (i as f32 / n) + (cb - top) * (j as f32 / n);
        for i in 0..rings {
            for j in 0..rings - i {
                b.tri(p(i, j), p(i + 1, j), p(i, j + 1), Vec3::Y);
                if i + j + 1 < rings {
                    b.tri(p(i + 1, j), p(i + 1, j + 1), p(i, j + 1), Vec3::Y);
                }
            }
        }
        let out = ((ca + cb) * 0.5 - top).normalize();
        b.grid(corner(radius, s), cb - ca, top, 1, rings, out);
    }
    b.build()
}

/// Box strip subdivided along its length: `8·segments + 4` triangles.
pub fn edge_mesh(segments: u32) -> Mesh {
    let (l, w, h) = (HEX_RADIUS * 0.7, 1.4, 1.2);
    let o = Vec3::new(-l / 2.0, 0.0, -w / 2.0);
    let (x, y, z) = (Vec3::X * l, Vec3::Y * h, Vec3::Z * w);
    let mut b = Builder::default();
    b.grid(o + y, x, z, segments, 1, Vec3::Y);
    b.grid(o, x, z, segments, 1, -Vec3::Y);
    b.grid(o, x, y, segments, 1, -Vec3::Z);
    b.grid(o + z, x, y, segments, 1, Vec3::Z);
    b.grid(o, z, y, 1, 1, -Vec3::X);
    b.grid(o + x, z, y, 1, 1, Vec3::X);
    b.build()
}

/// Bevy's `ico(n)` has `20·(n+1)²` triangles.
pub fn sphere_mesh(subdivisions: u32, radius: f32) -> Mesh {
    Sphere::new(radius)
        .mesh()
        .ico(subdivisions)
        .expect("icosphere subdivisions within limit")
}

pub fn cylinder_mesh(resolution: u32, segments: u32) -> Mesh {
    Cylinder::new(0.6, 3.0)
        .mesh()
        .resolution(resolution)
        .segments(segments)
        .build()
}

/// Per-LOD parameters: LOD1/LOD2 land at or below ~50% / ~15% of LOD0 (PRD §10).
const BASE_RINGS: [u32; 3] = [12, 8, 4];
const EDGE_SEGMENTS: [u32; 3] = [75, 37, 12];
const CENTER_ICO: [u32; 3] = [8, 5, 2];
const PROP_ICO: [u32; 3] = [5, 3, 1];
const PROP_CYL: [(u32, u32); 3] = [(40, 9), (20, 9), (12, 3)];

pub(crate) struct PieceMeshes {
    base: [Handle<Mesh>; 3],
    edge: [Handle<Mesh>; 3],
    center: [Handle<Mesh>; 3],
    prop_sphere: [Handle<Mesh>; 3],
    prop_cyl: [Handle<Mesh>; 3],
}

impl PieceMeshes {
    /// The kit meshes in `instanced::KINDS` order: base, edge, center, prop sphere, prop cylinder.
    pub(crate) fn kinds(&self) -> [&[Handle<Mesh>; 3]; 5] {
        [
            &self.base,
            &self.edge,
            &self.center,
            &self.prop_sphere,
            &self.prop_cyl,
        ]
    }

    pub(crate) fn new(meshes: &mut Assets<Mesh>) -> Self {
        let mut lods = |f: &dyn Fn(usize) -> Mesh| [0, 1, 2].map(|l| meshes.add(f(l)));
        Self {
            base: lods(&|l| base_mesh(BASE_RINGS[l], HEX_RADIUS * 0.96, TILE_HEIGHT)),
            edge: lods(&|l| edge_mesh(EDGE_SEGMENTS[l])),
            center: lods(&|l| sphere_mesh(CENTER_ICO[l], 3.0)),
            prop_sphere: lods(&|l| sphere_mesh(PROP_ICO[l], 1.0)),
            prop_cyl: lods(&|l| {
                let (r, s) = PROP_CYL[l];
                cylinder_mesh(r, s)
            }),
        }
    }
}

/// Gate material `i`; the instanced layout turns the same values into its palette.
pub(crate) fn gate_material(i: usize) -> StandardMaterial {
    let base_color = Color::hsl(i as f32 * 9.0, 0.45, 0.5);
    let emissive = if i % 10 == 3 {
        LinearRgba::from(base_color) * 12.0
    } else {
        LinearRgba::BLACK
    };
    StandardMaterial {
        base_color,
        perceptual_roughness: 0.3 + 0.65 * (i % 7) as f32 / 6.0,
        metallic: if i.is_multiple_of(5) { 0.8 } else { 0.0 },
        emissive,
        ..default()
    }
}

fn gate_materials(materials: &mut Assets<StandardMaterial>) -> Vec<Handle<StandardMaterial>> {
    (0..GATE.materials)
        .map(|i| materials.add(gate_material(i)))
        .collect()
}

/// Whether piece `pi` (index within its tile) at `lod` casts shadows under `casters`:
/// `large` drops the 12 props (pieces 8..20) and every LOD2 piece.
pub(crate) fn casts_shadow(casters: ShadowCasters, pi: usize, lod: usize) -> bool {
    casters == ShadowCasters::All || (pi < 8 && lod < 2)
}

/// The 20 pieces of tile `ti` in tile-local space: base, 6 edges, center, 12 props.
pub(crate) fn tile_pieces(m: &PieceMeshes, ti: usize) -> Vec<(&[Handle<Mesh>; 3], Transform)> {
    let seed = ti as u64 * 1_000;
    // Varied column heights give SSAO and shadows real crevices to work on.
    let lift = 0.6 + 1.6 * unit_hash(seed);
    let top = TILE_HEIGHT * lift;

    let mut pieces: Vec<(&[Handle<Mesh>; 3], Transform)> = Vec::with_capacity(GATE.pieces_per_tile);
    pieces.push((&m.base, Transform::from_scale(Vec3::new(1.0, lift, 1.0))));
    for e in 0..6 {
        let mid = (corner(HEX_RADIUS, e) + corner(HEX_RADIUS, (e + 1) % 6)) * 0.5 * 0.8;
        let dir = corner(HEX_RADIUS, (e + 1) % 6) - corner(HEX_RADIUS, e);
        pieces.push((
            &m.edge,
            Transform::from_translation(mid + Vec3::Y * top)
                .with_rotation(Quat::from_rotation_y(-dir.z.atan2(dir.x))),
        ));
    }
    pieces.push((
        &m.center,
        Transform::from_translation(Vec3::Y * top).with_scale(Vec3::new(1.0, 0.4, 1.0)),
    ));
    for p in 0..12u64 {
        let a = unit_hash(seed + 10 + p) * std::f32::consts::TAU;
        let r = HEX_RADIUS * (0.3 + 0.35 * unit_hash(seed + 30 + p));
        let pos = Vec3::new(r * a.cos(), top, r * a.sin());
        let s = 0.7 + 0.8 * unit_hash(seed + 50 + p);
        let (mesh, t) = if p % 2 == 0 {
            (
                &m.prop_sphere,
                Transform::from_translation(pos + Vec3::Y * s),
            )
        } else {
            (
                &m.prop_cyl,
                Transform::from_translation(pos + Vec3::Y * 1.5 * s),
            )
        };
        pieces.push((mesh, t.with_scale(Vec3::splat(s))));
    }
    pieces
}

/// LOD0 triangles of one tile as spawned, counted from the mesh assets.
fn tile_lod0_tris(pieces: &[(&[Handle<Mesh>; 3], Transform)], meshes: &Assets<Mesh>) -> usize {
    pieces
        .iter()
        .filter_map(|(lods, _)| meshes.get(&lods[0]))
        .map(|mesh| mesh.indices().map_or(0, |i| i.len() / 3))
        .sum()
}

fn mesh_tris(meshes: &Assets<Mesh>, h: &Handle<Mesh>) -> u32 {
    meshes
        .get(h)
        .map_or(0, |m| m.indices().map_or(0, |i| i.len() / 3)) as u32
}

fn spawn_gate(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: GateConfig,
) {
    let m = PieceMeshes::new(meshes);
    let mats = gate_materials(materials);
    let cells = spiral(GATE.tiles);
    let mut facts = SceneFacts {
        tris_lod0_per_tile: tile_lod0_tris(&tile_pieces(&m, 0), meshes),
        materials: mats.len(),
        pieces: cells.len() * GATE.pieces_per_tile,
        layout: config.layout.name(),
        chunk_size: 0,
        chunks: 0,
    };
    let casters = config.shadow_casters;
    match config.layout {
        Layout::Entities => spawn_gate_entities(commands, meshes, &m, &mats, &cells, casters),
        Layout::Chunks => {
            facts.chunk_size = config.chunk;
            facts.chunks =
                spawn_gate_chunks(commands, meshes, &m, &mats, &cells, config.chunk, casters);
        }
        Layout::Instanced => {
            facts.chunk_size = config.chunk;
            facts.chunks =
                crate::instanced::spawn(commands, meshes, &m, &cells, config.chunk, casters);
        }
    }
    commands.insert_resource(facts);
    spawn_vehicles(commands, meshes, &mats);
}

/// Per chunk and LOD, one merged mesh per material, placed at the chunk centroid so
/// `VisibilityRange` measures distance to the chunk rather than to the world origin.
fn spawn_gate_chunks(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    m: &PieceMeshes,
    mats: &[Handle<StandardMaterial>],
    cells: &[Axial],
    size: u32,
    casters: ShadowCasters,
) -> usize {
    let chunks = chunk_tiles(cells, size);
    for (_, members) in &chunks {
        let centroid = members
            .iter()
            .map(|&ti| {
                let [x, z] = cells[ti].to_world(HEX_RADIUS);
                Vec3::new(x, 0.0, z)
            })
            .sum::<Vec3>()
            / members.len() as f32;
        let mut merged = vec![];
        for lod in 0..3 {
            let mut parts = vec![];
            for &ti in members {
                let [x, z] = cells[ti].to_world(HEX_RADIUS);
                for (pi, (lods, t)) in tile_pieces(m, ti).into_iter().enumerate() {
                    let mesh = meshes.get(&lods[lod]).expect("piece mesh");
                    parts.push((
                        piece_material(ti, pi),
                        mesh,
                        Transform::from_xyz(x, 0.0, z) * t,
                    ));
                }
            }
            merged.push(merge_by_material(&parts, centroid));
        }
        commands
            .spawn((
                Chunk {
                    tiles: members.len() as u32,
                },
                Transform::from_translation(centroid),
                Visibility::default(),
                SceneEntity,
            ))
            .with_children(|chunk| {
                for (lod, by_mat) in merged.into_iter().enumerate() {
                    for (mat, mesh) in by_mat {
                        let tris = mesh.indices().map_or(0, |i| i.len() / 3) as u32;
                        let mut e = chunk.spawn((
                            Mesh3d(meshes.add(mesh)),
                            MeshMaterial3d(mats[mat].clone()),
                            Transform::default(),
                            lod_range(lod),
                            Lod(lod as u8),
                            Tris(tris),
                            SceneEntity,
                        ));
                        // Props are merged with the large pieces here, so `large` drops LOD2 only.
                        if casters == ShadowCasters::Large && lod == 2 {
                            e.insert(NotShadowCaster);
                        }
                    }
                }
            });
    }
    chunks.len()
}

fn spawn_gate_entities(
    commands: &mut Commands,
    meshes: &Assets<Mesh>,
    m: &PieceMeshes,
    mats: &[Handle<StandardMaterial>],
    cells: &[Axial],
    casters: ShadowCasters,
) {
    let ppt = GATE.pieces_per_tile;
    for (ti, cell) in cells.iter().enumerate() {
        let [x, z] = cell.to_world(HEX_RADIUS);
        let pieces = tile_pieces(m, ti);
        debug_assert_eq!(pieces.len(), ppt);

        commands
            .spawn((
                Tile,
                Transform::from_xyz(x, 0.0, z),
                Visibility::default(),
                SceneEntity,
            ))
            .with_children(|tile| {
                for (pi, (lods, t)) in pieces.into_iter().enumerate() {
                    let mat = mats[piece_material(ti, pi)].clone();
                    for (lod, mesh) in lods.iter().enumerate() {
                        let mut e = tile.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(mat.clone()),
                            t,
                            lod_range(lod),
                            Lod(lod as u8),
                            Tris(mesh_tris(meshes, mesh)),
                            SceneEntity,
                        ));
                        if !casts_shadow(casters, pi, lod) {
                            e.insert(NotShadowCaster);
                        }
                    }
                }
            });
    }
}

fn spawn_vehicles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &[Handle<StandardMaterial>],
) {
    let vehicle = meshes.add(Cuboid::new(3.0, 1.5, 1.5));
    for v in 0..GATE.moving {
        let seed = 9_000_000 + v as u64 * 8;
        let a = unit_hash(seed) * std::f32::consts::TAU;
        let d = 550.0 * unit_hash(seed + 1).sqrt();
        commands.spawn((
            Vehicle {
                center: Vec3::new(d * a.cos(), 14.0, d * a.sin()),
                radius: 20.0 + 120.0 * unit_hash(seed + 2),
                omega: (0.1 + 0.4 * unit_hash(seed + 3)) * if v % 2 == 0 { 1.0 } else { -1.0 },
                phase: unit_hash(seed + 4) * std::f32::consts::TAU,
            },
            Mesh3d(vehicle.clone()),
            MeshMaterial3d(mats[(v * 7) % GATE.materials].clone()),
            Transform::default(),
            Tris(mesh_tris(meshes, &vehicle)),
            SceneEntity,
        ));
    }
}

pub fn move_vehicles(time: Res<Time>, mut q: Query<(&Vehicle, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (v, mut tr) in &mut q {
        let a = t * v.omega + v.phase;
        tr.translation = v.center + Vec3::new(a.cos(), 0.0, a.sin()) * v.radius;
        tr.rotation = Quat::from_rotation_y(-a - v.omega.signum() * std::f32::consts::FRAC_PI_2);
    }
}

fn spawn_single(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let p = hex_prism(HEX_RADIUS, TILE_HEIGHT);
    commands.insert_resource(SceneFacts {
        tris_lod0_per_tile: p.indices.len() / 3,
        materials: 1,
        pieces: 1,
        layout: Layout::Entities.name(),
        chunk_size: 0,
        chunks: 0,
    });
    let mesh = meshes.add(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs)
        .with_inserted_indices(Indices::U32(p.indices)),
    );
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.55, 0.24),
        perceptual_roughness: 0.85,
        ..default()
    });
    commands.spawn((
        Tile,
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::default(),
        SceneEntity,
    ));
}

/// Closeup: 40 m above tile (3, 2), looking across the world.
pub fn gate_camera(view: GateView) -> Transform {
    match view {
        GateView::Strategic => {
            Transform::from_xyz(0.0, 630.0, 900.0).looking_at(Vec3::ZERO, Vec3::Y)
        }
        GateView::Closeup => {
            let [x, z] = Axial { q: 3, r: 2 }.to_world(HEX_RADIUS);
            Transform::from_xyz(x, 40.0, z).looking_at(Vec3::new(-400.0, 0.0, -300.0), Vec3::Y)
        }
    }
}

pub fn setup(
    mut commands: Commands,
    scenario: Res<ActiveScenario>,
    view: Res<ActiveView>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Option<Res<GateConfig>>,
    server: Res<AssetServer>,
    (mut images, mut water_mats, mut terrain_mats): (
        ResMut<Assets<Image>>,
        ResMut<Assets<crate::water::WaterMaterial>>,
        ResMut<Assets<crate::water::TerrainMaterial>>,
    ),
) {
    let config = config.map(|c| *c).unwrap_or_default();
    // Look-dev applies to the kit, water and play scenarios only: single/gate2000 keep the P0 camera, so
    // the P0 perf baseline and the instanced pipeline's mesh-view bind-group layout are unchanged.
    let water = scenario.0.has_water();
    let kit = scenario.0 == Scenario::Kit || water || scenario.0 == Scenario::Play;
    if water {
        commands.insert_resource(crate::water::WaterClock {
            t: 0.0,
            fixed: config.water_fixed,
        });
    } else {
        commands.remove_resource::<crate::water::WaterClock>();
    }
    let camera = match scenario.0 {
        Scenario::Single => {
            commands.remove_resource::<crate::instanced::InstancedScene>();
            commands.remove_resource::<crate::kit_scene::KitPending>();
            commands.remove_resource::<crate::kit_scene::KitLoading>();
            spawn_single(&mut commands, &mut meshes, &mut materials);
            commands.insert_resource(crate::kit_test::KitTestPending);
            Transform::from_xyz(0.0, 63.0, 90.0).looking_at(Vec3::ZERO, Vec3::Y)
        }
        Scenario::Gate2000 => {
            commands.remove_resource::<crate::kit_test::KitTestPending>();
            commands.remove_resource::<crate::kit_scene::KitPending>();
            commands.remove_resource::<crate::kit_scene::KitLoading>();
            spawn_gate(&mut commands, &mut meshes, &mut materials, config);
            gate_camera(view.0)
        }
        Scenario::Kit => {
            commands.remove_resource::<crate::instanced::InstancedScene>();
            commands.remove_resource::<crate::kit_test::KitTestPending>();
            commands.insert_resource(crate::kit_scene::KitPending);
            Transform::from_xyz(14.0, 38.0, 62.0).looking_at(Vec3::new(14.0, 0.0, 0.0), Vec3::Y)
        }
        Scenario::Water | Scenario::WaterKit => {
            commands.remove_resource::<crate::instanced::InstancedScene>();
            commands.remove_resource::<crate::kit_test::KitTestPending>();
            if scenario.0 == Scenario::WaterKit {
                commands.insert_resource(crate::kit_scene::KitPending);
            } else {
                commands.remove_resource::<crate::kit_scene::KitPending>();
                commands.remove_resource::<crate::kit_scene::KitLoading>();
            }
            crate::water::spawn(
                &mut commands,
                &mut meshes,
                &mut images,
                &mut water_mats,
                &mut terrain_mats,
                config.tier,
            );
            Transform::from_xyz(-8.0, 34.0, 62.0).looking_at(Vec3::new(14.0, 0.0, 0.0), Vec3::Y)
        }
        Scenario::Play => {
            commands.remove_resource::<crate::instanced::InstancedScene>();
            commands.remove_resource::<crate::kit_test::KitTestPending>();
            commands.remove_resource::<crate::kit_scene::KitPending>();
            commands.remove_resource::<crate::kit_scene::KitLoading>();
            crate::play::setup(&mut commands, &mut meshes, &mut materials)
        }
    };

    commands.spawn((
        DirectionalLight {
            illuminance: 30_000.0,
            shadow_maps_enabled: config.shadows,
            ..default()
        },
        if kit {
            crate::lookdev::sun_transform()
        } else {
            Transform::from_xyz(200.0, 400.0, 150.0).looking_at(Vec3::ZERO, Vec3::Y)
        },
        CascadeShadowConfigBuilder {
            num_cascades: config.cascades as usize,
            maximum_distance: config.shadow_dist as f32,
            first_cascade_far_bound: crate::lookdev::first_cascade_far_bound(kit),
            ..default()
        }
        .build(),
        SceneEntity,
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: if kit && config.ibl { 0.0 } else { 400.0 },
        ..default()
    });

    let tier = config.tier;
    let tilt = crate::tilt_shift::settings_for(&config, kit, tier);
    commands.insert_resource(tilt);
    commands.insert_resource(crate::tilt_shift::TiltTierDefault(tilt));
    commands.insert_resource(ClearColor(if kit {
        config.grade.backdrop()
    } else {
        crate::lookdev::BACKDROP
    }));
    let mut cam = commands.spawn((Camera3d::default(), camera, Msaa::Off, SceneEntity));
    if water {
        crate::water::insert_camera(&mut cam, tier);
    }
    if config.post == PostMode::Full {
        cam.insert(camera_post_bundle(tier));
        // The gate's far-focus DOF blurs every close-up; the water look-dev judges surface detail.
        if water {
            cam.remove::<bevy::post_process::dof::DepthOfField>();
        }
        if let Some(ssao) = camera_ssao(tier) {
            cam.insert(ssao);
        }
    }
    if kit {
        cam.insert(crate::grade::camera_bundle(&config));
        if config.ibl {
            cam.insert(crate::lookdev::environment(&server));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(m: &Mesh) -> usize {
        m.indices().map_or(0, |i| i.len() / 3)
    }

    fn check(name: &str, lods: [Mesh; 3], target: usize) {
        let [l0, l1, l2] = lods.map(|m| tris(&m));
        let t = target as f32;
        assert!(
            (l0 as f32 - t).abs() <= 0.1 * t,
            "{name} LOD0 {l0} not within 10% of {target}"
        );
        assert!(
            l1 as f32 <= 0.55 * l0 as f32,
            "{name} LOD1 {l1} > 55% of {l0}"
        );
        assert!(
            l2 as f32 <= 0.20 * l0 as f32,
            "{name} LOD2 {l2} > 20% of {l0}"
        );
    }

    #[test]
    fn piece_lods_meet_budgets() {
        check(
            "base",
            BASE_RINGS.map(|n| base_mesh(n, HEX_RADIUS, TILE_HEIGHT)),
            1000,
        );
        check("edge", EDGE_SEGMENTS.map(edge_mesh), 600);
        check("center", CENTER_ICO.map(|n| sphere_mesh(n, 3.0)), 1500);
        check("prop sphere", PROP_ICO.map(|n| sphere_mesh(n, 1.0)), 800);
        check(
            "prop cylinder",
            PROP_CYL.map(|(r, s)| cylinder_mesh(r, s)),
            800,
        );
    }

    #[test]
    fn base_mesh_normals_point_outward() {
        let m = base_mesh(4, HEX_RADIUS, TILE_HEIGHT);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) =
            m.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals missing");
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
            m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        for (n, p) in n.iter().zip(p) {
            let (n, p) = (Vec3::from(*n), Vec3::from(*p));
            let centroid = Vec3::Y * TILE_HEIGHT / 2.0;
            assert!(n.dot(p - centroid) > 0.0, "inward normal at {p}");
        }
    }

    #[test]
    fn tile_lod0_tris_counts_what_spawns() {
        let mut assets = Assets::<Mesh>::default();
        let m = PieceMeshes::new(&mut assets);
        // 1,008 base + 6×604 edges + 1,620 center + 6×720 spheres + 6×796 cylinders.
        assert_eq!(tile_lod0_tris(&tile_pieces(&m, 0), &assets), 15_348);
    }

    #[test]
    fn lod_ranges_are_contiguous() {
        assert_eq!(lod_range(0).end_margin, lod_range(1).start_margin);
        assert_eq!(lod_range(1).end_margin, lod_range(2).start_margin);
    }
}
