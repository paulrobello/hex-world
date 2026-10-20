//! PRD §9.4 hooks for the atmosphere phases (D7). Every hook is plumbed through to the shaders and
//! is inert at the MVP values: noon, no snow, dry, summer tint, open water. The asset gate
//! (`tests/hooks/mod.rs`, part of `kit_gate`) holds every manifest piece and every kit and water
//! shader to them.
//!
//! | Hook | Channel |
//! |---|---|
//! | Emissive, night-only intensity | masks.B × `night` uniform (`hooks.wesl` `surface`) |
//! | Snow accumulation + up-facing coverage | masks.R × `snow` × smoothstep(N.y) |
//! | Wetness/porosity | masks.G × `wetness` |
//! | Foliage seasonal tint index | per-instance `MeshTag` on a `FLAG_FOLIAGE` slot |
//! | Seasonal blossom | unique ORM.A (1 - blossom) on a `FLAG_BLOSSOM` slot, hidden by that tag outside summer |
//! | Time-of-day block (sun, sky, fog, exposure) | [`TimeOfDay`] → [`atmosphere`], noon in the MVP |
//! | Water freeze | `WaterParams::freeze` |
//!
//! None adds a sampled texture: masks is binding 101 of `KitExt`/`CarExt`, the rest are uniforms or
//! the mesh uniform's tag. `--kit_hooks snow|wet|night|dusk|freeze` drives them for verification
//! shots (docs/checkpoints/hooks/).
use bevy::{
    camera::Exposure,
    light::EnvironmentMapLight,
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};
use hex_assets::{PieceEntry, PieceKind, Slot};

use crate::{
    ActiveScenario, Scenario,
    lighting::{IBL_INTENSITY, KIT_SUN_DIR},
    water::WaterMaterial,
    workload::{GateConfig, KitHooks},
};

/// The MVP's fixed hour.
pub const NOON: f32 = 12.0;
/// `kit_hooks=dusk`: the sun about 3 degrees above the horizon.
pub const DUSK: f32 = 17.75;
/// The noon sun (lx), as `scene::setup` spawns it.
pub const NOON_SUN_LUX: f32 = 30_000.0;
/// Season index of an untagged instance: summer, the MVP look (`hooks.wesl` `season_tint`).
pub const SEASON_SUMMER: u32 = 0;
/// The autumn indices `season_tint` knows (`kit_hooks=autumn` tags foliage with these).
pub const SEASON_AUTUMN: [u32; 2] = [1, 2];
/// The tileable set of tree bark: the one tree slot that is not foliage.
pub const BARK_SET: &str = "bark";

/// The time-of-day block: one hour value drives the sun, the sky (IBL), fog, exposure and the
/// night factor of the emissive hook. Fixed at [`NOON`] in the MVP.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct TimeOfDay {
    pub hour: f32,
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self { hour: NOON }
    }
}

impl TimeOfDay {
    pub fn for_hooks(h: KitHooks) -> TimeOfDay {
        TimeOfDay {
            hour: if h == KitHooks::Dusk { DUSK } else { NOON },
        }
    }

    pub fn is_noon(self) -> bool {
        self.hour == NOON
    }
}

/// What the time-of-day block drives. `fog` is `(colour, visibility in m)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// Unit vector toward the sun.
    pub sun_dir: Vec3,
    pub sun_lux: f32,
    pub sun_color: Color,
    /// IBL intensity (the sky).
    pub sky: f32,
    pub fog: Option<(Color, f32)>,
    /// Added to the camera's EV100.
    pub ev100_offset: f32,
    /// 0 by day, 1 at night: scales the emissive hook (masks.B).
    pub night: f32,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The atmosphere at `hour`. Noon returns the look-dev values exactly (no curve is evaluated), so
/// the MVP look cannot drift. Other hours are a first pass for the day/night phase: the sun runs
/// an arc from 06:00 to 18:00 through the noon direction, warming and dimming toward the horizon.
pub fn atmosphere(hour: f32) -> Atmosphere {
    let noon = KIT_SUN_DIR.normalize();
    if hour == NOON {
        return Atmosphere {
            sun_dir: noon,
            sun_lux: NOON_SUN_LUX,
            sun_color: Color::WHITE,
            sky: IBL_INTENSITY,
            fog: None,
            ev100_offset: 0.0,
            night: 0.0,
        };
    }
    let t = ((hour - 6.0) / 12.0).clamp(-0.25, 1.25);
    let elev = noon.y.asin() * (std::f32::consts::PI * t).sin();
    let az = noon.z.atan2(noon.x) + (t - 0.5) * 2.0;
    let sun_dir = Vec3::new(elev.cos() * az.cos(), elev.sin(), elev.cos() * az.sin());
    let s = elev.sin();
    let day = smoothstep(-0.05, 0.35, s);
    let warm = 1.0 - smoothstep(0.05, 0.4, s);
    let sun_color = Color::linear_rgb(1.0, 1.0 - 0.45 * warm, 1.0 - 0.72 * warm);
    Atmosphere {
        sun_dir,
        sun_lux: NOON_SUN_LUX * day,
        sun_color,
        sky: IBL_INTENSITY * (0.08 + 0.92 * day),
        fog: (warm > 0.0).then(|| {
            (
                Color::linear_rgba(0.9, 0.55, 0.32, 1.0),
                6000.0 - 3500.0 * warm,
            )
        }),
        ev100_offset: -2.5 * (1.0 - day),
        night: 1.0 - smoothstep(-0.05, 0.15, s),
    }
}

/// The surface hook uniforms (snow, wetness, night) for a `kit_hooks` value: all 0 by default.
pub fn surface_hooks(h: KitHooks) -> [f32; 3] {
    match h {
        KitHooks::Snow => [1.0, 0.0, 0.0],
        KitHooks::Wet => [0.0, 1.0, 0.0],
        KitHooks::Night => [0.0, 0.0, 1.0],
        KitHooks::Dusk => [0.0, 0.0, atmosphere(DUSK).night],
        KitHooks::None | KitHooks::Freeze | KitHooks::Autumn => [0.0; 3],
    }
}

/// The water freeze value for a `kit_hooks` value: 0 (open water) by default.
pub fn water_freeze(h: KitHooks) -> f32 {
    if h == KitHooks::Freeze { 1.0 } else { 0.0 }
}

/// A foliage slot takes the per-instance seasonal tint: every alpha-tested slot (leaf cards,
/// needles, flower clumps), every slot of a tree except its bark, and every slot the manifest marks
/// `foliage` (opaque plants on other pieces: dune grass, reeds, hedges).
pub fn is_foliage(p: &PieceEntry, slot: &Slot) -> bool {
    slot.foliage
        || slot.alpha_cutoff.is_some()
        || (p.kind == PieceKind::Tree && slot.tileable.as_deref() != Some(BARK_SET))
}

/// Scenarios lit by the look-dev sun, IBL and exposure (`scene::setup`); P0 scenarios keep theirs.
fn lookdev(s: Scenario) -> bool {
    matches!(
        s,
        Scenario::Kit | Scenario::Water | Scenario::WaterKit | Scenario::Play
    )
}

/// Drives the sun, sky, fog and exposure from [`TimeOfDay`]. Does nothing at noon: the look-dev
/// values `scene::setup` spawns are the noon row of [`atmosphere`] (asserted in the tests).
fn apply_time_of_day(
    tod: Res<TimeOfDay>,
    scenario: Res<ActiveScenario>,
    config: Res<GateConfig>,
    mut commands: Commands,
    mut suns: Query<(&mut DirectionalLight, &mut Transform)>,
    mut cams: Query<(Entity, &mut Exposure, Option<&mut EnvironmentMapLight>), With<Camera3d>>,
) {
    if tod.is_noon() || !lookdev(scenario.0) {
        return;
    }
    let a = atmosphere(tod.hour);
    for (mut light, mut t) in &mut suns {
        light.illuminance = a.sun_lux;
        light.color = a.sun_color;
        *t = Transform::from_translation(a.sun_dir * 500.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    for (e, mut exposure, env) in &mut cams {
        exposure.ev100 = config.ev100.get() + a.ev100_offset;
        if let Some(mut env) = env {
            env.intensity = a.sky;
        }
        match a.fog {
            Some((color, visibility)) => {
                commands.entity(e).insert(DistanceFog {
                    color,
                    falloff: FogFalloff::from_visibility(visibility),
                    ..default()
                });
            }
            None => {
                commands.entity(e).remove::<DistanceFog>();
            }
        }
    }
}

/// Sets a water material's freeze: the uniform, and the `WATER_FREEZE` variant that reads it (off
/// at 0, so open water keeps the approved shader).
pub fn set_freeze(m: &mut WaterMaterial, f: f32) {
    m.params.freeze = f;
    m.key.freeze = f > 0.0;
}

/// `kit_hooks=freeze`: freezes every water material. Runs only for that verification variant.
fn apply_freeze(config: Res<GateConfig>, mut water: ResMut<Assets<WaterMaterial>>) {
    let f = water_freeze(config.kit_hooks);
    let ids: Vec<_> = water
        .iter()
        .filter(|(_, m)| m.params.freeze != f)
        .map(|(id, _)| id)
        .collect();
    for id in ids {
        if let Some(mut m) = water.get_mut(id) {
            set_freeze(&mut m, f);
        }
    }
}

/// `kit_hooks=autumn`: tags each untagged foliage instance with an autumn index. Runs only for
/// that verification variant; MVP instances stay untagged (summer).
fn apply_autumn(
    mut commands: Commands,
    kit: Res<Assets<crate::kit_material::KitMaterial>>,
    untagged: Query<
        (Entity, &MeshMaterial3d<crate::kit_material::KitMaterial>),
        Without<bevy::mesh::MeshTag>,
    >,
) {
    for (e, m) in &untagged {
        let Some(mat) = kit.get(&m.0) else {
            continue;
        };
        if mat.extension.params.flags & crate::kit_material::FLAG_FOLIAGE != 0 {
            let i = (crate::workload::unit_hash(e.to_bits()) * 2.0) as usize;
            commands
                .entity(e)
                .insert(bevy::mesh::MeshTag::new(SEASON_AUTUMN[i.min(1)]));
        }
    }
}

pub struct HooksPlugin {
    pub hooks: KitHooks,
}

impl Plugin for HooksPlugin {
    fn build(&self, app: &mut App) {
        let hooks = self.hooks;
        app.insert_resource(TimeOfDay::for_hooks(hooks))
            .add_systems(Update, apply_time_of_day)
            .add_systems(
                Update,
                apply_freeze.run_if(move || hooks == KitHooks::Freeze),
            )
            .add_systems(
                Update,
                apply_autumn.run_if(move || hooks == KitHooks::Autumn),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The noon row is the look-dev rig `scene::setup` spawns, bit for bit.
    #[test]
    fn noon_is_the_lookdev_rig() {
        let a = atmosphere(NOON);
        assert_eq!(a.sun_dir, KIT_SUN_DIR.normalize());
        let shine = crate::lighting::sun_transform().forward().as_vec3();
        assert!(shine.dot(-a.sun_dir) > 0.9999);
        assert_eq!(a.sun_lux, NOON_SUN_LUX);
        assert_eq!(a.sun_color, Color::WHITE);
        assert_eq!(a.sky, IBL_INTENSITY);
        assert_eq!(a.fog, None);
        assert_eq!(a.ev100_offset, 0.0);
        assert_eq!(a.night, 0.0);
        assert!(TimeOfDay::default().is_noon());
        assert!(TimeOfDay::for_hooks(KitHooks::None).is_noon());
    }

    /// The MVP default drives every hook to its neutral value.
    #[test]
    fn mvp_defaults_are_neutral() {
        assert_eq!(GateConfig::default().kit_hooks, KitHooks::None);
        assert_eq!(surface_hooks(KitHooks::None), [0.0; 3]);
        assert_eq!(water_freeze(KitHooks::None), 0.0);
        assert_eq!(crate::water::WaterParams::default().freeze, 0.0);
        let open = crate::water::WaterKey::for_tier(crate::post::QualityTier::High);
        assert!(!open.freeze);
        assert!(!format!("{:?}", open.defs()).contains("WATER_FREEZE"));
        let frozen = crate::water::WaterKey {
            freeze: true,
            ..open
        };
        assert!(format!("{:?}", frozen.defs()).contains("WATER_FREEZE"));
    }

    #[test]
    fn dusk_is_low_warm_dim_and_partly_night() {
        let a = atmosphere(DUSK);
        assert!(a.sun_dir.y > 0.0 && a.sun_dir.y < 0.1, "{:?}", a.sun_dir);
        assert!(a.sun_lux < NOON_SUN_LUX / 3.0);
        assert!(a.fog.is_some() && a.ev100_offset < 0.0);
        assert!(a.night > 0.2 && a.night < 0.8, "{}", a.night);
        assert_eq!(surface_hooks(KitHooks::Dusk)[2], a.night);
        assert!(!TimeOfDay::for_hooks(KitHooks::Dusk).is_noon());
        assert_eq!(water_freeze(KitHooks::Freeze), 1.0);
    }
}
