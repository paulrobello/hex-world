//! P0 asset-route probe: the meshopt GLB disc with a device-selected KTX2/PNG texture, on top of
//! the single tile. Both files load through the `kit://` source.
use bevy::{
    gltf::GltfAssetLabel,
    prelude::*,
    render::{renderer::RenderDevice, settings::WgpuFeatures},
};
use hex_assets::{family_suffix, select_family};

use crate::SceneEntity;

/// Set by scene setup; the disc spawns once the render device (and so its features) exists,
/// which can be after `Startup` when the renderer initializes asynchronously (web).
#[derive(Resource)]
pub struct KitTestPending;

/// Clears the hex top so the disc does not z-fight with it.
const LIFT: f32 = 0.05;
/// The disc's radius equals the hex circumradius; this keeps it inside the hex's flat edges.
const SCALE: f32 = 0.85;

pub fn spawn_when_ready(
    mut commands: Commands,
    pending: Option<Res<KitTestPending>>,
    device: Option<Res<RenderDevice>>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(_), Some(device)) = (pending, device) else {
        return;
    };
    commands.remove_resource::<KitTestPending>();
    let features = device.features();
    let bc = features.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC);
    let astc = features.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC);
    let family = select_family(bc, astc);
    info!("texture family: {family:?} (bc={bc}, astc={astc})");

    let mesh = asset_server.load(
        GltfAssetLabel::Primitive {
            mesh: 0,
            primitive: 0,
        }
        .from_asset("kit://p0/hex_test.glb"),
    );
    let texture = asset_server.load(format!("kit://p0/hex_test.{}", family_suffix(family)));
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        ..default()
    });
    // The disc's vertices are authored at y = TILE_HEIGHT (4 m), so only the lift is added.
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(0.0, LIFT, 0.0).with_scale(Vec3::new(SCALE, 1.0, SCALE)),
        SceneEntity,
    ));
}
