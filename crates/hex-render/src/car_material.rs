//! Car material (PRD §6 runtime recolour): StandardMaterial + `CarExt`, shader
//! `assets/shaders/car_material.wesl`.
//!
//! One model per car type, painted per instance. The model marks its paintable surfaces in COLOR_0.a
//! (no texture) and each car entity carries its paint colour as a [`MeshTag`] (sRGB8 packed
//! 0xBBGGRR). `MeshTag` lives in the per-entity mesh uniform, so it never splits a batch: every car
//! of one type and LOD draws as one instanced batch per slot, whatever its colour, and no material
//! is made per colour. Glass, tyres, lights and trim keep their COLOR_0 colours.
//!
//! Sampled-texture budget: the extension binds only `masks`, exactly as `KitExt` does, so a car
//! pipeline samples the same count as the kit forward pipeline (15 with the IBL, 16 in a view with
//! refracting water), within WebGPU's default 16.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use hex_assets::Slot;

use crate::{
    kit_material::{SlotRequests, load_map},
    workload::KitHooks,
};

pub const SHADER: &str = "shaders/car_material.wesl";
pub const CAR_FLAG_MASKS: u32 = 1;

pub type CarMaterial = ExtendedMaterial<StandardMaterial, CarExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct CarParams {
    pub snow: f32,
    pub wetness: f32,
    pub night: f32,
    pub car_flags: u32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct CarExt {
    #[uniform(100)]
    pub params: CarParams,
    #[texture(101)]
    #[sampler(102)]
    pub masks: Option<Handle<Image>>,
}

impl MaterialExtension for CarExt {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

pub fn params_for(r: &SlotRequests, hooks: KitHooks) -> CarParams {
    let [snow, wetness, night] = crate::hooks::surface_hooks(hooks);
    CarParams {
        snow,
        wetness,
        night,
        car_flags: if r.masks.is_some() { CAR_FLAG_MASKS } else { 0 },
    }
}

/// A colour-only car slot (`<id>_body`, `_glass`, `_tyre`): the slot's colour and roughness, the
/// piece's `masks`. Unique maps are not used by cars.
pub fn build_material(
    server: &AssetServer,
    slot: &Slot,
    r: &SlotRequests,
    hooks: KitHooks,
) -> CarMaterial {
    let [cr, cg, cb] = slot.color.unwrap_or([1.0, 1.0, 1.0]);
    CarMaterial {
        base: StandardMaterial {
            base_color: Color::linear_rgb(cr, cg, cb),
            perceptual_roughness: slot.roughness.unwrap_or(0.5),
            metallic: 0.0,
            ..default()
        },
        extension: CarExt {
            params: params_for(r, hooks),
            masks: r.masks.as_ref().map(|m| load_map(server, m)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WGSL struct is four scalars: the Rust side must stay 16 bytes.
    #[test]
    fn params_are_16_bytes() {
        assert_eq!(CarParams::min_size().get(), 16);
    }

    #[test]
    fn night_hook_and_masks_flag() {
        let r = SlotRequests::default();
        let p = params_for(&r, KitHooks::Night);
        assert_eq!((p.night, p.snow, p.car_flags), (1.0, 0.0, 0));
    }
}
