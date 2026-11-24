//! Waterfall spray and mist (owner, 2026-11-24: "the waterfall needs some kind of mist particles
//! or animation where it hits each level"). Every tier landing of every drawn fall
//! ([`MistLanding`]: each lip below the top, then the plunge pool) gets soft camera-facing
//! puffs that rise, drift out over the lower tile and fade. All falls' puffs are quads of one mesh
//! drawn with one [`FallMistMaterial`]: one draw while any fall stands, none otherwise, and no
//! entity per particle. The CPU builds the mesh only when the landings or the tier change; the
//! vertex shader (`assets/shaders/fall_mist.wesl`) moves each puff as a pure function of the water
//! clock, so `--water_clock fixed` captures stay byte-reproducible, and the thumbnails
//! (`play::NoFalls`) have no landings and no mist. Under [`ReducedMotion`] no mist is drawn, as a rise
//! raises no dust there. The material binds no texture.
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    light::NotShadowCaster,
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use hex_render::{ActiveScenario, Scenario, SceneEntity, config::RenderConfig, post::QualityTier};

use super::{
    WaterClock,
    tiles::{MistLanding, PlayWater},
};
use crate::play::rise::{ReducedMotion, hash01};

pub const MIST_SHADER: &str = "shaders/fall_mist.wesl";

/// Peak opacity of one puff's core (a first value, pending the owner's look).
const MIST_ALPHA: f32 = 0.42;
/// A lip puff's full-grown radius scale (m): small, so each tier's spray stays a band at its foot.
const LIP_SIZE: f32 = 0.95;
/// A plunge-pool puff's radius scale (m): this, plus [`POOL_SIZE_PER_LEVEL`] per level of drop.
/// Owner 2026-11-24: the pool's mist hugs the water at the sheet's foot, not a cloud above it.
const POOL_SIZE: f32 = 1.05;
const POOL_SIZE_PER_LEVEL: f32 = 0.2;
/// How far along the lip (m, either side of its midpoint) the pool's puffs spread: the sheet's
/// 8 m lane less a margin.
const SPREAD: f32 = 3.4;
/// How far along the lip a lip's spray spreads: within the sheet, so none floats off its sides.
const LIP_SPREAD: f32 = 3.2;

/// Mist uniforms: the water clock time and the puffs' peak opacity.
#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct MistParams {
    pub time: f32,
    pub alpha: f32,
    pub _pad0: f32,
    pub _pad1: f32,
}

/// The waterfall mist's soft puffs: lit like the scene (sun, IBL, exposure, fog), blended, out
/// of the prepass (the water reads it as its bed) and the shadow passes. No textures.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct FallMistMaterial {
    #[uniform(100)]
    pub params: MistParams,
}

impl Material for FallMistMaterial {
    fn vertex_shader() -> ShaderRef {
        MIST_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        MIST_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    /// Sorts after the blended water (Low and Medium) the mist stands over, so the water never
    /// draws over it.
    fn depth_bias(&self) -> f32 {
        1_000.0
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The quads face the camera in the vertex shader, so either winding may face it.
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

/// Puffs at one landing on `tier`: a lip spray, or the plunge pool's cloud, which grows with the
/// fall's `drop`. Low draws the fewest.
pub fn puffs_at(tier: QualityTier, pool: bool, drop: u8) -> u32 {
    let (lip, base, per_level) = match tier {
        QualityTier::Low => (4, 6, 2),
        QualityTier::Medium => (6, 10, 3),
        QualityTier::High => (10, 14, 4),
        QualityTier::Ultra => (14, 18, 6),
    };
    if pool {
        base + per_level * u32::from(drop)
    } else {
        lip
    }
}

/// One puff as the mesh carries it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Puff {
    /// Where it starts (world): on the landing, spread along the lip.
    pub origin: Vec3,
    /// Unit plan direction it drifts in (down the fall, out over the lower tile).
    pub out: Vec2,
    /// Its phase in its life cycle, its seed, its radius scale (m) and 1 in the plunge pool.
    pub phase: f32,
    pub seed: f32,
    pub size: f32,
    pub pool: bool,
}

/// The puffs of every landing on `tier`, seeded by each fall's upper tile and tier, so a fall's
/// mist is the same wherever and whenever it is drawn.
pub fn puffs(landings: &[MistLanding], tier: QualityTier) -> Vec<Puff> {
    let mut out = vec![];
    for l in landings {
        let n = puffs_at(tier, l.pool, l.drop);
        let size = if l.pool {
            POOL_SIZE + POOL_SIZE_PER_LEVEL * f32::from(l.drop)
        } else {
            LIP_SIZE
        };
        let salt = u32::from(l.tier) * 7919;
        let spread = if l.pool { SPREAD } else { LIP_SPREAD };
        for i in 0..n {
            let h = |k: u32| hash01(l.upper, i, salt + k);
            let along = spread * (2.0 * (i as f32 + h(1)) / n as f32 - 1.0);
            // Owner 2026-11-24: the mist stands where the water lands, not out past it.
            let ahead = 0.2 * h(2);
            let p = l.pos.xz() + l.lat * along + l.out * ahead;
            out.push(Puff {
                origin: Vec3::new(p.x, l.pos.y, p.y),
                out: l.out,
                phase: (i as f32 * 0.618_034 + 0.3 * h(3)).fract(),
                seed: h(4),
                size: size * (0.8 + 0.4 * h(5)),
                pool: l.pool,
            });
        }
    }
    out
}

/// One quad per puff: POSITION its origin, NORMAL its drift, UV_0 the corner (±1, ±1), COLOR
/// (phase, seed, size, pool).
pub fn mist_mesh(puffs: &[Puff]) -> Mesh {
    let (mut pos, mut nor, mut uv, mut col, mut idx) = (vec![], vec![], vec![], vec![], vec![]);
    for (i, p) in puffs.iter().enumerate() {
        let base = (i * 4) as u32;
        for corner in [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]] {
            pos.push(p.origin.to_array());
            nor.push([p.out.x, 0.0, p.out.y]);
            uv.push(corner);
            col.push([p.phase, p.seed, p.size, if p.pool { 1.0 } else { 0.0 }]);
        }
        idx.extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nor)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_indices(Indices::U32(idx))
}

/// Marks the mist entity.
#[derive(Component)]
pub struct FallMist;

/// The mist entity, what its mesh was built from, and its material.
#[derive(Resource, Default)]
pub struct PlayMist {
    entity: Option<Entity>,
    built: Option<(Vec<MistLanding>, QualityTier, bool)>,
    material: Option<Handle<FallMistMaterial>>,
    puffs: usize,
}

impl PlayMist {
    /// Puffs the mist draws (0 with no fall).
    pub fn puffs(&self) -> usize {
        self.puffs
    }
}

/// Keeps the one mist entity in step with the drawn falls (none under reduced motion), and hands
/// its material the water clock's time.
#[allow(clippy::too_many_arguments)]
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    water: Option<Res<PlayWater>>,
    config: Res<RenderConfig>,
    reduced: Res<ReducedMotion>,
    clock: Option<Res<WaterClock>>,
    mut mist: ResMut<PlayMist>,
    alive: Query<(), With<FallMist>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FallMistMaterial>>,
) {
    if active.0 != Scenario::Play {
        return;
    }
    // A scenario switch despawns every scene entity: draw afresh.
    if mist.entity.is_some_and(|e| !alive.contains(e)) {
        mist.entity = None;
        mist.built = None;
    }
    let landings = water.as_deref().map_or(&[][..], PlayWater::landings);
    let key = (landings.to_vec(), config.tier, reduced.0);
    if mist.built.as_ref() != Some(&key) {
        if let Some(e) = mist.entity.take() {
            commands.entity(e).try_despawn();
        }
        let ps = if reduced.0 {
            vec![]
        } else {
            puffs(landings, config.tier)
        };
        mist.puffs = ps.len();
        if !ps.is_empty() {
            let material = mist
                .material
                .get_or_insert_with(|| {
                    materials.add(FallMistMaterial {
                        params: MistParams {
                            alpha: MIST_ALPHA,
                            ..default()
                        },
                    })
                })
                .clone();
            let e = commands
                .spawn((
                    FallMist,
                    Mesh3d(meshes.add(mist_mesh(&ps))),
                    MeshMaterial3d(material),
                    Transform::IDENTITY,
                    // The vertex shader moves the puffs off the mesh's resting bounds.
                    NoFrustumCulling,
                    NotShadowCaster,
                    SceneEntity,
                ))
                .id();
            mist.entity = Some(e);
        }
        mist.built = Some(key);
    }
    let t = clock.map_or(0.0, |c| c.t);
    if let Some(h) = &mist.material
        && materials.get(h).is_some_and(|m| m.params.time != t)
        && let Some(mut m) = materials.get_mut(h)
    {
        m.params.time = t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::water::tiles::{PLUNGE_R, fall_landings, fall_tiers};
    use hex_core::{Axial, LEVEL_STEP};

    fn landing(pool: bool, drop: u8, tier: u8) -> MistLanding {
        MistLanding {
            pos: Vec3::new(0.0, -1.0, 0.0),
            out: Vec2::X,
            lat: Vec2::Y,
            pool,
            drop,
            upper: Axial::new(2, -1),
            tier,
        }
    }

    /// Every tier is cheaper on a lower quality tier, Low the cheapest, and the plunge pool's
    /// cloud grows with the drop.
    #[test]
    fn puff_counts_scale_by_tier_and_drop() {
        use QualityTier::*;
        for pool in [false, true] {
            for drop in 1..=3 {
                let n = [Low, Medium, High, Ultra].map(|t| puffs_at(t, pool, drop));
                assert!(
                    n[0] < n[1] && n[1] < n[2] && n[2] <= n[3],
                    "{pool} {drop}: {n:?}"
                );
                assert!(n[0] >= 3);
            }
        }
        for t in [Low, Medium, High, Ultra] {
            assert!(puffs_at(t, true, 1) > puffs_at(t, false, 1));
            assert!(puffs_at(t, true, 3) > puffs_at(t, true, 2));
            assert!(puffs_at(t, true, 2) > puffs_at(t, true, 1));
        }
    }

    /// A 3-level fall's landings: two lips and the pool, the puffs counted per landing, spread
    /// along the lip (a lip's past the sheet's sides), and the pool's bigger.
    #[test]
    fn puffs_follow_the_landings() {
        let ls = [
            landing(false, 3, 0),
            landing(false, 3, 1),
            landing(true, 3, 2),
        ];
        for tier in [QualityTier::Low, QualityTier::High] {
            let ps = puffs(&ls, tier);
            let lip = puffs_at(tier, false, 3) as usize;
            let pool = puffs_at(tier, true, 3) as usize;
            assert_eq!(ps.len(), 2 * lip + pool);
            assert!(ps[..2 * lip].iter().all(|p| !p.pool));
            assert!(ps[2 * lip..].iter().all(|p| p.pool));
            for p in &ps {
                let spread = if p.pool { SPREAD } else { LIP_SPREAD };
                assert!(p.origin.z.abs() <= spread + 1e-4, "{p:?}");
                assert!((0.0..1.0).contains(&p.phase) && (0.0..1.0).contains(&p.seed));
                assert_eq!(p.origin.y, -1.0);
            }
            let max_lip = ps[..2 * lip].iter().map(|p| p.size).fold(0.0, f32::max);
            let min_pool = ps[2 * lip..]
                .iter()
                .map(|p| p.size)
                .fold(f32::MAX, f32::min);
            assert!(min_pool > max_lip);
        }
        // Seeded: the same landings give the same puffs; the two lips differ.
        assert_eq!(puffs(&ls, QualityTier::High), puffs(&ls, QualityTier::High));
        let ps = puffs(&ls, QualityTier::High);
        assert_ne!(ps[0].phase, ps[8].phase);
    }

    /// The mesh is one quad per puff, all in one draw.
    #[test]
    fn the_mist_is_one_mesh_of_quads() {
        let ls = [landing(true, 2, 1)];
        let ps = puffs(&ls, QualityTier::Medium);
        let m = mist_mesh(&ps);
        assert_eq!(m.count_vertices(), ps.len() * 4);
        assert_eq!(m.indices().map(|i| i.len()), Some(ps.len() * 6));
    }

    /// The landings sit where the tiers fall to: one per tier, one level apart, each further
    /// out, and the last in the plunge pool's disc.
    #[test]
    fn landings_step_down_one_level_per_tier() {
        for drop in 1..=3u8 {
            let ls = fall_landings(drop, 0, drop);
            assert_eq!(ls.len(), fall_tiers(drop).len());
            for (i, (p, pool)) in ls.iter().enumerate() {
                assert_eq!(*pool, i + 1 == usize::from(drop));
                let want = crate::water::tiles::water_y(drop) - LEVEL_STEP * (i as f32 + 1.0);
                assert!((p.y - want).abs() < 1e-4);
            }
            // The pool landing is on the lower tile's water, inside the pool's disc.
            let (last, _) = ls[ls.len() - 1];
            assert!((last.y - crate::water::tiles::water_y(0)).abs() < 1e-4);
            let apothem = hex_core::HEX_RADIUS * 0.866_025_4;
            assert!(last.xz().length() - apothem < PLUNGE_R);
            assert!(last.xz().length() > apothem);
        }
    }
}
