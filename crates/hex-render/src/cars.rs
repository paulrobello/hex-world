//! Cars in `scenario=play` (PRD §7.2). `hex-app` runs the simulation and writes the interpolated
//! poses into [`CarsView`]; this module keeps one entity per car id in step with it.
//!
//! Every car shares one placeholder mesh and material ([`CarAssets`]), so Bevy draws them as one
//! instanced batch. A car kit piece replaces the two handles in [`CarAssets`] and nothing else.
use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::{ActiveScenario, Scenario, SceneEntity};

/// Placeholder car size in metres: length along +x (the driving direction), height, width.
pub const CAR_SIZE: Vec3 = Vec3::new(4.2, 1.5, 1.8);

/// Road crown height: `BED_TOP` 0.08 + `CROWN` 0.03 in `scripts/blender/kit/_road.py`.
pub const ROAD_SURFACE_Y: f32 = 0.11;

/// One car's pose between the last two sim ticks, in world XZ metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarPose {
    /// The sim's car id; ids are never reused.
    pub id: u64,
    pub pos: [f32; 2],
    /// Heading in radians from +x toward +z (`hex_sim::Pose::yaw`).
    pub yaw: f32,
}

/// The cars to draw, written by `hex-app` every frame while any car exists.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CarsView {
    pub cars: Vec<CarPose>,
}

/// A drawn car, tagged with its sim id.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayCar(pub u64);

/// The mesh and material every car is drawn with.
#[derive(Resource, Clone)]
pub struct CarAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

impl FromWorld for CarAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(car_mesh());
        // Vertex colours carry the body, roof and tyre colours.
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.55,
                ..default()
            });
        Self { mesh, material }
    }
}

fn part(size: Vec3, center: Vec3, color: Color) -> Mesh {
    let c = color.to_linear();
    let mesh =
        Mesh::from(Cuboid::from_size(size)).transformed_by(Transform::from_translation(center));
    let n = mesh.count_vertices();
    mesh.with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[c.red, c.green, c.blue, 1.0]; n],
    )
}

/// A low-poly placeholder car facing +x with its wheels on y = 0: a neutral body, a darker cabin
/// and roof, and four dark wheels, [`CAR_SIZE`] overall.
pub fn car_mesh() -> Mesh {
    let [len, height, width] = CAR_SIZE.to_array();
    let body = Color::srgb(0.80, 0.78, 0.74);
    let roof = Color::srgb(0.20, 0.22, 0.26);
    let tyre = Color::srgb(0.06, 0.06, 0.07);
    let (wheel, sill, belt) = (0.62, 0.28, 0.90);
    let mut mesh = part(
        Vec3::new(len, belt - sill, width),
        Vec3::new(0.0, (sill + belt) / 2.0, 0.0),
        body,
    );
    let cabin = part(
        Vec3::new(len * 0.55, height - belt, width * 0.9),
        Vec3::new(-0.25, (belt + height) / 2.0, 0.0),
        roof,
    );
    mesh.merge(&cabin).expect("same attributes");
    for x in [-1.35, 1.35] {
        for z in [-1.0, 1.0] {
            let w = part(
                Vec3::new(wheel, wheel, 0.28),
                Vec3::new(x, wheel / 2.0, z * (width / 2.0 - 0.14)),
                tyre,
            );
            mesh.merge(&w).expect("same attributes");
        }
    }
    mesh
}

/// A car standing on the road crown at `p`, its +x along the heading.
pub fn car_transform(p: &CarPose) -> Transform {
    // Bevy turns +x toward -z for a positive angle about +Y, so the yaw (+x toward +z) is negated.
    Transform::from_xyz(p.pos[0], ROAD_SURFACE_Y, p.pos[1])
        .with_rotation(Quat::from_rotation_y(-p.yaw))
}

/// Spawns, moves and despawns car entities to match [`CarsView`] by car id. Outside the play
/// scenario every car entity goes. Cars are `SceneEntity`s, so a scenario switch also removes
/// them, and this respawns whatever the view still holds.
pub fn sync(
    mut commands: Commands,
    active: Res<ActiveScenario>,
    view: Res<CarsView>,
    assets: Res<CarAssets>,
    mut cars: Query<(Entity, &PlayCar, &mut Transform)>,
) {
    let play = active.0 == Scenario::Play;
    let want: HashMap<u64, &CarPose> = if play {
        view.cars.iter().map(|c| (c.id, c)).collect()
    } else {
        HashMap::new()
    };
    if want.is_empty() && cars.is_empty() {
        return;
    }
    let mut have = HashSet::with_capacity(want.len());
    for (e, car, mut t) in &mut cars {
        match want.get(&car.0) {
            Some(p) if have.insert(car.0) => *t = car_transform(p),
            _ => commands.entity(e).try_despawn(),
        }
    }
    for (id, p) in want {
        if !have.contains(&id) {
            commands.spawn((
                PlayCar(id),
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                car_transform(p),
                SceneEntity,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn car_mesh_has_the_placeholder_size_and_sits_on_the_ground() {
        let m = car_mesh();
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions missing");
        };
        let (lo, hi) = p.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), v| {
            (lo.min(Vec3::from(*v)), hi.max(Vec3::from(*v)))
        });
        assert!((hi - lo - CAR_SIZE).abs().max_element() < 1e-4, "{lo} {hi}");
        assert!(lo.y.abs() < 1e-6 && (lo.x + hi.x).abs() < 1e-6 && (lo.z + hi.z).abs() < 1e-6);
        assert!(m.attribute(Mesh::ATTRIBUTE_COLOR).is_some());
    }

    #[test]
    fn transform_faces_the_heading() {
        for (yaw, dir) in [
            (0.0, Vec3::X),
            (std::f32::consts::FRAC_PI_2, Vec3::Z),
            (std::f32::consts::PI, -Vec3::X),
        ] {
            let t = car_transform(&CarPose {
                id: 0,
                pos: [3.0, -2.0],
                yaw,
            });
            assert!((t.rotation * Vec3::X - dir).length() < 1e-5, "yaw {yaw}");
            assert_eq!(t.translation, Vec3::new(3.0, ROAD_SURFACE_Y, -2.0));
        }
    }
}
