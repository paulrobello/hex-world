//! Vehicle headlights (P4 open tunnels, owner 2026-11-14: "they will need them for the day night
//! cycle"). Each car and each engine carries a lamp rig built at runtime, so the approved vehicle
//! pieces stay as they are: small unlit lenses at the front and a soft additive beam on the road
//! ahead. No scene lights: the rig is two or three tiny meshes per vehicle.
//!
//! The rig shows when the vehicle's `lamps` level (published by the sim: 1 inside a tunnel,
//! ramping over `hex_sim::LAMP_RAMP` metres at each arch) is above zero, and its beam grows with
//! the level. The day and night cycle (P5) will raise the same level at night.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::cars::{CarKind, CarsView, PlayCar};
use crate::trains::{PlayStock, StockKind, TrainsView};

/// A lamp rig is shown above this level.
const SHOW_ABOVE: f32 = 0.02;

/// What a vehicle's rig is made of: where its lenses sit (forward x, up y, lateral z, metres from
/// the vehicle's origin on the road or rail top), and the beam's near and far width and length.
pub struct Rig {
    pub lenses: &'static [Vec3],
    pub lens_radius: f32,
    /// x of the vehicle's front, where the beam starts.
    pub front: f32,
    pub beam: (f32, f32, f32),
    pub near_alpha: f32,
}

const SEDAN: Rig = Rig {
    lenses: &[Vec3::new(2.10, 0.62, 0.62), Vec3::new(2.10, 0.62, -0.62)],
    lens_radius: 0.09,
    front: 2.14,
    beam: (1.7, 5.2, 11.0),
    near_alpha: 0.3,
};
const VAN: Rig = Rig {
    lenses: &[Vec3::new(2.45, 0.78, 0.74), Vec3::new(2.45, 0.78, -0.74)],
    lens_radius: 0.10,
    front: 2.49,
    beam: (1.9, 5.6, 12.0),
    near_alpha: 0.3,
};
const PICKUP: Rig = Rig {
    lenses: &[Vec3::new(2.36, 0.74, 0.70), Vec3::new(2.36, 0.74, -0.70)],
    lens_radius: 0.10,
    front: 2.40,
    beam: (1.8, 5.4, 11.5),
    near_alpha: 0.3,
};
/// The engine's single smokebox lamp, on the centreline.
const ENGINE: Rig = Rig {
    lenses: &[Vec3::new(4.40, 2.55, 0.0)],
    lens_radius: 0.17,
    front: 4.47,
    beam: (2.2, 7.0, 24.0),
    near_alpha: 0.28,
};

/// The rig of a car body.
pub fn car_rig(kind: CarKind) -> &'static Rig {
    match kind {
        CarKind::Sedan => &SEDAN,
        CarKind::Van => &VAN,
        CarKind::Pickup => &PICKUP,
    }
}

/// The rig of a train vehicle: only the engine carries one.
pub fn stock_rig(kind: StockKind) -> Option<&'static Rig> {
    (kind == StockKind::Engine).then_some(&ENGINE)
}

/// A vehicle that has been given its lamp rig (possibly none), so it is not visited again.
#[derive(Component)]
pub struct Headlights;

/// The parent of a vehicle's lenses and beam: hidden while the lamps are off.
#[derive(Component)]
pub struct HeadlightRig;

/// A rig's beam: scaled along its length by the lamps level.
#[derive(Component)]
pub struct HeadlightBeam;

/// The shared lamp meshes and materials.
#[derive(Resource)]
pub struct HeadlightAssets {
    lens: Handle<StandardMaterial>,
    beam_mat: Handle<StandardMaterial>,
}

impl FromWorld for HeadlightAssets {
    fn from_world(world: &mut World) -> Self {
        let mut mats = world.resource_mut::<Assets<StandardMaterial>>();
        Self {
            lens: mats.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.94, 0.74),
                unlit: true,
                ..default()
            }),
            beam_mat: mats.add(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                alpha_mode: AlphaMode::Add,
                double_sided: true,
                cull_mode: None,
                ..default()
            }),
        }
    }
}

/// A trapezoid on the ground in front of x = 0, `(near, far, length)` wide and long, warm white
/// rising from nothing at the front to `near_alpha` a quarter of the way out, then fading to nothing
/// at the far edge (vertex colours; the material is additive).
pub fn beam_mesh(front: f32, (near, far, length): (f32, f32, f32), near_alpha: f32) -> Mesh {
    const PEAK: f32 = 0.25;
    let warm = |a: f32| [1.0, 0.86, 0.55, a];
    let y = 0.03;
    let mid = near + (far - near) * PEAK;
    let positions = vec![
        [front, y, -near / 2.0],
        [front, y, near / 2.0],
        [front + length * PEAK, y, -mid / 2.0],
        [front + length * PEAK, y, mid / 2.0],
        [front + length, y, -far / 2.0],
        [front + length, y, far / 2.0],
    ];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 6]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![
            warm(0.0),
            warm(0.0),
            warm(near_alpha),
            warm(near_alpha),
            warm(0.0),
            warm(0.0),
        ],
    );
    mesh.insert_indices(Indices::U32(vec![0, 2, 1, 1, 2, 3, 2, 4, 3, 3, 4, 5]));
    mesh
}

pub struct HeadlightsPlugin;

impl Plugin for HeadlightsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeadlightAssets>()
            .add_systems(Update, (attach, update).chain());
    }
}

fn spawn_rig(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    assets: &HeadlightAssets,
    vehicle: Entity,
    rig: &Rig,
) {
    let lens_mesh = meshes.add(Sphere::new(rig.lens_radius).mesh().ico(1).unwrap());
    let beam = meshes.add(beam_mesh(rig.front, rig.beam, rig.near_alpha));
    commands.entity(vehicle).with_children(|p| {
        p.spawn((HeadlightRig, Transform::IDENTITY, Visibility::Hidden))
            .with_children(|r| {
                for at in rig.lenses {
                    r.spawn((
                        Mesh3d(lens_mesh.clone()),
                        MeshMaterial3d(assets.lens.clone()),
                        Transform::from_translation(*at),
                        NotShadowCaster,
                    ));
                }
                r.spawn((
                    HeadlightBeam,
                    Mesh3d(beam),
                    MeshMaterial3d(assets.beam_mat.clone()),
                    // The beam starts at the vehicle's front: scale about it.
                    Transform::from_translation(Vec3::new(rig.front, 0.0, 0.0))
                        .with_scale(Vec3::splat(0.0)),
                    NotShadowCaster,
                ));
            });
    });
}

/// Gives every new car and train vehicle its rig, hidden.
#[allow(clippy::too_many_arguments)]
fn attach(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    assets: Res<HeadlightAssets>,
    cars: Query<(Entity, &PlayCar), Without<Headlights>>,
    stock: Query<(Entity, &PlayStock), Without<Headlights>>,
    car_view: Res<CarsView>,
    train_view: Res<TrainsView>,
) {
    for (e, car) in &cars {
        // A placeholder car or one gone from the view is not given a rig.
        if let Some(p) = car_view.cars.iter().find(|p| p.id == car.0) {
            commands.entity(e).insert(Headlights);
            spawn_rig(&mut commands, &mut meshes, &assets, e, car_rig(p.kind));
        }
    }
    for (e, s) in &stock {
        if let Some(p) = train_view.vehicles.iter().find(|p| p.id == s.0) {
            commands.entity(e).insert(Headlights);
            if let Some(rig) = stock_rig(p.kind) {
                spawn_rig(&mut commands, &mut meshes, &assets, e, rig);
            }
        }
    }
}

/// Shows each rig at its vehicle's lamps level and grows the beam with it.
fn update(
    car_view: Res<CarsView>,
    train_view: Res<TrainsView>,
    cars: Query<&PlayCar>,
    stock: Query<&PlayStock>,
    mut rigs: Query<(&mut Visibility, &ChildOf, &Children), With<HeadlightRig>>,
    mut beams: Query<&mut Transform, With<HeadlightBeam>>,
) {
    if rigs.is_empty() {
        return;
    }
    let lamps: HashMap<(bool, u64), f32> = car_view
        .cars
        .iter()
        .map(|c| ((true, c.id), c.lamps))
        .chain(train_view.vehicles.iter().map(|v| ((false, v.id), v.lamps)))
        .collect();
    for (mut visibility, parent, children) in &mut rigs {
        let level = if let Ok(c) = cars.get(parent.parent()) {
            lamps.get(&(true, c.0))
        } else if let Ok(s) = stock.get(parent.parent()) {
            lamps.get(&(false, s.0))
        } else {
            None
        }
        .copied()
        .unwrap_or(0.0);
        let want = if level > SHOW_ABOVE {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(want);
        for child in children.iter() {
            if let Ok(mut t) = beams.get_mut(child) {
                t.scale = Vec3::splat(level.clamp(0.0, 1.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cars::CarPose;
    use crate::trains::StockPose;

    /// Every lens sits inside its vehicle's bake-report extent (forward x, lateral half width,
    /// height), and the beam starts at the front.
    #[test]
    fn lenses_sit_on_the_vehicles_front() {
        let cases: [(&Rig, f32, f32, f32); 4] = [
            (&SEDAN, 2.142, 0.90, 1.42),
            (&VAN, 2.49, 1.08, 1.95),
            (&PICKUP, 2.405, 0.97, 1.78),
            (&ENGINE, 4.47, 1.42, 4.2),
        ];
        for (rig, front, half, high) in cases {
            assert!((rig.front - front).abs() < 0.01, "{front}");
            for l in rig.lenses {
                assert!(l.x <= rig.front && l.x > rig.front - 0.2, "{l}");
                assert!(l.z.abs() <= half && l.y > 0.0 && l.y < high, "{l}");
            }
        }
        assert!(stock_rig(StockKind::Carriage).is_none());
        assert!(stock_rig(StockKind::Engine).is_some());
    }

    #[test]
    fn the_beam_fades_to_nothing_ahead_of_the_front() {
        let m = beam_mesh(2.0, (1.7, 5.2, 11.0), 0.3);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
            m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        assert_eq!(p[0][0], 2.0);
        assert_eq!(p[4][0], 13.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) =
            m.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours")
        };
        assert_eq!((c[0][3], c[2][3], c[4][3]), (0.0, 0.3, 0.0));
    }

    /// A car with the lamps up shows its rig and its beam grows with the level; at zero the rig
    /// hides again. A carriage gets no rig at all.
    #[test]
    fn the_rig_follows_the_lamps_level() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<HeadlightAssets>()
            .insert_resource(CarsView {
                cars: vec![CarPose {
                    id: 7,
                    kind: CarKind::Sedan,
                    pos: [0.0, 0.0],
                    yaw: 0.0,
                    opacity: 1.0,
                    lamps: 1.0,
                }],
            })
            .insert_resource(TrainsView {
                vehicles: vec![StockPose {
                    id: 3,
                    train: 0,
                    kind: StockKind::Carriage,
                    pos: [0.0, 0.0],
                    yaw: 0.0,
                    lamps: 1.0,
                }],
            })
            .add_systems(Update, (attach, update).chain());
        app.world_mut().spawn(PlayCar(7));
        app.world_mut().spawn(PlayStock(3));
        app.update();
        app.update();
        let rig = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<(&Visibility, &Children), With<HeadlightRig>>();
            let rows: Vec<_> = q.iter(app.world()).map(|(v, c)| (*v, c.len())).collect();
            assert_eq!(rows.len(), 1, "one car rig, no carriage rig");
            rows[0]
        };
        let beam = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&Transform, With<HeadlightBeam>>();
            q.single(app.world()).unwrap().scale.x
        };
        assert_eq!(
            rig(&mut app),
            (Visibility::Inherited, 3),
            "two lenses and a beam"
        );
        assert_eq!(beam(&mut app), 1.0);
        app.world_mut().resource_mut::<CarsView>().cars[0].lamps = 0.5;
        app.update();
        assert_eq!(beam(&mut app), 0.5);
        app.world_mut().resource_mut::<CarsView>().cars[0].lamps = 0.0;
        app.update();
        assert_eq!(rig(&mut app).0, Visibility::Hidden);
    }
}
