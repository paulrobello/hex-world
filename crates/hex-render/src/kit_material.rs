//! Kit material (R6): StandardMaterial + `KitExt`. Tileable layers are sampled in world space in
//! `kit_material.wesl` so rotated/translated neighbours line up, except a slot with `tileUvScale`
//! (tree bark), whose set is sampled at UV0 scaled by StandardMaterial's `uv_transform`; `masks`
//! (raw UV0) carries the §9.4 hooks (R snow, G wetness, B night emissive) and baked AO in A, and a
//! foliage slot takes the per-instance seasonal tint (`crate::hooks`).
//!
//! Sampled-texture budget: a tileable set binds through StandardMaterial's own base-color,
//! normal-map and metallic-roughness slots (the shader samples them at world-space UVs), so the
//! extension adds only `masks`. WebGPU's default per-stage limit is 16: the forward pipeline uses
//! 15 with the IBL and 16 in a view with refracting water (its transmission texture).
use bevy::{
    image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    math::Affine2,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use hex_assets::{Channel, KitManifest, PieceEntry, PieceKind, Slot, TextureFamily, map_file};

use crate::workload::KitHooks;

pub const SHADER: &str = "shaders/kit_material.wesl";
/// Forward vertex stage: Bevy's mesh vertex plus the foliage wind (`foliage_wind.wesl`).
pub const VERTEX_SHADER: &str = "shaders/kit_vertex.wesl";
/// Writes the tileable slots' world-space tiled normal into the normal prepass (SSAO reads it);
/// other slots keep StandardMaterial's prepass. Shares `kit_common.wesl` with `SHADER`.
pub const PREPASS_SHADER: &str = "shaders/kit_prepass.wesl";
pub const FLAG_TILEABLE: u32 = 1;
pub const FLAG_MASKS: u32 = 2;
/// Foliage slot (`hooks::is_foliage`): the shader tints it by the instance's season index.
pub const FLAG_FOLIAGE: u32 = 4;
/// Coast-bed sand (`Slot::wet_band`): the live swash wet band of the look-dev's placeholder bed,
/// on the water surface's clock (`water::tick`).
pub const FLAG_WET_BAND: u32 = 8;
/// Sways as part of a tree (`Slot::wind` on a tree piece): leans from the trunk base by height and
/// flexes its branches by TEXCOORD_1 U (`foliage_wind.wesl`), in the vertex stage of every pass.
pub const FLAG_WIND_TREE: u32 = 16;
/// Sways as a plant (`Slot::wind` on any other piece): bends by TEXCOORD_1 U squared.
pub const FLAG_WIND_PLANT: u32 = 32;

pub type KitMaterial = ExtendedMaterial<StandardMaterial, KitExt>;

#[derive(Clone, Copy, Debug, Default, ShaderType, Reflect)]
pub struct KitParams {
    pub tile_inv_m: f32,
    pub flags: u32,
    pub snow: f32,
    pub wetness: f32,
    pub night: f32,
    /// Water time (s) of the wet band (`WaterClock`); 0 on other slots.
    pub water_time: f32,
    /// Swash amplitude (m) of the wet band (`water::WASH_AMP`); 0 on other slots.
    pub wash_amp: f32,
    /// Wind amplitude (m per unit wind push, `Slot::wind`); 0 on still slots.
    pub wind_amp: f32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct KitExt {
    #[uniform(100)]
    pub params: KitParams,
    #[texture(101)]
    #[sampler(102)]
    pub masks: Option<Handle<Image>>,
}

impl MaterialExtension for KitExt {
    fn vertex_shader() -> ShaderRef {
        VERTEX_SHADER.into()
    }

    /// The same shader as the prepass fragment: one module, so depth, normals, motion vectors
    /// and every shadow cascade displace each vertex with the forward pass's expressions.
    fn prepass_vertex_shader() -> ShaderRef {
        PREPASS_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        PREPASS_SHADER.into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapRequest {
    pub path: String,
    pub srgb: bool,
    pub repeat: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SlotRequests {
    pub masks: Option<MapRequest>,
    /// Tileable albedo, normal, orm and 1 / tile_m.
    pub tile: Option<([MapRequest; 3], f32)>,
    /// The tile is mapped on UV0 scaled by this factor, not in world space (`Slot::tile_uv_scale`).
    pub tile_uv_scale: Option<f32>,
    pub unique: Vec<(Channel, MapRequest)>,
    /// Takes the per-instance seasonal tint (`hooks::is_foliage`).
    pub foliage: bool,
    /// Takes the live wet band (`Slot::wet_band`).
    pub wet_band: bool,
    /// Sways in the wind: (amplitude, tree profile) from `Slot::wind` and the piece kind.
    pub wind: Option<(f32, bool)>,
}

fn req(stem: &str, ch: Channel, family: TextureFamily, repeat: bool) -> MapRequest {
    MapRequest {
        path: map_file(stem, family),
        srgb: ch.is_srgb(),
        repeat,
    }
}

/// Which files a material slot needs and how to load them. Color space comes from the channel,
/// never the file: PNG has no color-space field (Review Focus 3).
///
/// A tileable set occupies StandardMaterial's albedo, normal and ORM bindings, so a slot cannot
/// also take unique maps for those channels: that is an error, not a silent override. Nor can a
/// tileable slot alpha-test: the cutoff forces double-sided shading (the tiled normal has no
/// back-face flip) and the prepass would discard on the tile albedo sampled at UV0.
pub fn slot_requests(
    m: &KitManifest,
    p: &PieceEntry,
    slot: &Slot,
    family: TextureFamily,
) -> Result<SlotRequests, String> {
    let mut out = SlotRequests {
        foliage: crate::hooks::is_foliage(p, slot),
        wet_band: slot.wet_band,
        wind: slot.wind.map(|a| (a, p.kind == PieceKind::Tree)),
        ..default()
    };
    if slot.masks {
        out.masks = p
            .maps
            .get(&Channel::Masks)
            .map(|s| req(s, Channel::Masks, family, false));
    }
    if let Some(set) = slot.tileable.as_ref().and_then(|id| m.materials.get(id)) {
        let get = |ch: Channel| req(&set.maps[&ch], ch, family, true);
        out.tile = Some((
            [
                get(Channel::Albedo),
                get(Channel::Normal),
                get(Channel::Orm),
            ],
            1.0 / set.tile_m,
        ));
        out.tile_uv_scale = slot.tile_uv_scale;
    }
    if slot.unique {
        for ch in [
            Channel::Albedo,
            Channel::Normal,
            Channel::Orm,
            Channel::Emissive,
        ] {
            if let Some(s) = p.maps.get(&ch) {
                out.unique.push((ch, req(s, ch, family, false)));
            }
        }
    }
    if out.tile.is_some() && slot.alpha_cutoff.is_some() {
        return Err(format!(
            "{}: slot uses tileable set {} with alphaCutoff; tileable slots must be opaque",
            p.id,
            slot.tileable.as_deref().unwrap_or_default()
        ));
    }
    if out.tile.is_some()
        && let Some((ch, _)) = out
            .unique
            .iter()
            .find(|(c, _)| matches!(c, Channel::Albedo | Channel::Normal | Channel::Orm))
    {
        return Err(format!(
            "{}: slot uses tileable set {} and a unique {} map; both need the same StandardMaterial binding",
            p.id,
            slot.tileable.as_deref().unwrap_or_default(),
            ch.as_str()
        ));
    }
    Ok(out)
}

pub fn params_for(r: &SlotRequests, hooks: KitHooks) -> KitParams {
    let mut flags = 0;
    if r.tile.is_some() && r.tile_uv_scale.is_none() {
        flags |= FLAG_TILEABLE;
    }
    if r.masks.is_some() {
        flags |= FLAG_MASKS;
    }
    if r.foliage {
        flags |= FLAG_FOLIAGE;
    }
    if r.wet_band {
        flags |= FLAG_WET_BAND;
    }
    match r.wind {
        Some((_, true)) => flags |= FLAG_WIND_TREE,
        Some((_, false)) => flags |= FLAG_WIND_PLANT,
        None => {}
    }
    let [snow, wetness, night] = crate::hooks::surface_hooks(hooks);
    KitParams {
        tile_inv_m: r.tile.as_ref().map_or(0.0, |t| t.1),
        flags,
        snow,
        wetness,
        night,
        wash_amp: if r.wet_band {
            crate::water::WASH_AMP
        } else {
            0.0
        },
        wind_amp: r.wind.map_or(0.0, |(a, _)| a),
        ..default()
    }
}

/// UV0 transform: a UV-mapped tileable set repeats `tile_uv_scale` times per UV unit. Bevy applies
/// `uv_transform` to every StandardMaterial texture (forward and prepass) but not to `in.uv`, so
/// the masks texture keeps sampling the piece atlas at raw UV0. No binding is added.
pub fn uv_transform_for(r: &SlotRequests) -> Affine2 {
    r.tile_uv_scale
        .map_or(Affine2::IDENTITY, |k| Affine2::from_scale(Vec2::splat(k)))
}

/// Loader settings for one map, for every family. Bevy 0.20 uses `is_srgb` for the PNG decode
/// AND to pick the KTX2 view format (`bevy_image` ktx2.rs maps the UNORM and SRGB BC7/ASTC
/// vkFormats alike, then chooses sRGB from `is_srgb`), so it must come from the channel always.
pub fn apply_settings(r: &MapRequest, s: &mut ImageLoaderSettings) {
    s.is_srgb = r.srgb;
    let mut d = ImageSamplerDescriptor::linear();
    d.anisotropy_clamp = 16;
    if r.repeat {
        d.address_mode_u = ImageAddressMode::Repeat;
        d.address_mode_v = ImageAddressMode::Repeat;
    }
    s.sampler = ImageSampler::Descriptor(d);
}

pub fn load_map(server: &AssetServer, r: &MapRequest) -> Handle<Image> {
    let settings = r.clone();
    server
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| apply_settings(&settings, s))
        .load(format!("kit://{}", r.path))
}

pub fn build_material(
    server: &AssetServer,
    slot: &Slot,
    r: &SlotRequests,
    hooks: KitHooks,
) -> KitMaterial {
    let mut base = StandardMaterial {
        perceptual_roughness: 1.0,
        metallic: 1.0, // multiplied by the ORM texture; tileable path overrides in the shader
        ..default()
    };
    for (ch, mr) in &r.unique {
        let h = load_map(server, mr);
        match ch {
            Channel::Albedo => base.base_color_texture = Some(h),
            Channel::Normal => base.normal_map_texture = Some(h),
            Channel::Orm => {
                base.metallic_roughness_texture = Some(h.clone());
                base.occlusion_texture = Some(h);
            }
            Channel::Emissive => {
                base.emissive_texture = Some(h);
                base.emissive = LinearRgba::WHITE;
            }
            Channel::Masks => {}
        }
    }
    if r.unique.iter().all(|(c, _)| *c != Channel::Orm) {
        base.metallic = 0.0;
        base.perceptual_roughness = slot.roughness.unwrap_or(1.0);
    }
    if let Some([cr, cg, cb]) = slot.color {
        base.base_color = Color::linear_rgb(cr, cg, cb);
    }
    if let Some(c) = slot.alpha_cutoff {
        base.alpha_mode = AlphaMode::Mask(c);
        base.double_sided = true;
        base.cull_mode = None;
    }
    // Tileable set: StandardMaterial's own bindings, sampled at world-space UVs by the shader, or
    // at UV0 scaled by `uv_transform` for a UV-mapped set (bark). The ORM goes in
    // metallic_roughness only; AO comes from masks.A, never ORM.R.
    if let Some(([a, n, o], _)) = &r.tile {
        base.base_color_texture = Some(load_map(server, a));
        base.normal_map_texture = Some(load_map(server, n));
        base.metallic_roughness_texture = Some(load_map(server, o));
        if r.tile_uv_scale.is_some() {
            // StandardMaterial multiplies these factors by the tile ORM's G and B.
            base.metallic = 1.0;
            base.perceptual_roughness = 1.0;
            base.uv_transform = uv_transform_for(r);
        }
    }
    KitMaterial {
        base,
        extension: KitExt {
            params: params_for(r, hooks),
            masks: r.masks.as_ref().map(|m| load_map(server, m)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_assets::{TextureFamily, parse_manifest};

    const M: &str = r#"{"version":1,
      "materials":{"grass":{"tileM":2.0,"maps":{"albedo":"s/g.albedo","normal":"s/g.normal","orm":"s/g.orm"}}},
      "pieces":[{"id":"b","kind":"base","glb":"b.glb","provenance":"p.json",
        "slots":{"b_cap":{"tileable":"grass","masks":true},"b_tree":{"unique":true}},
        "maps":{"masks":"b/b.masks","albedo":"b/b.albedo","normal":"b/b.normal","orm":"b/b.orm"}}]}"#;

    const FAMILIES: [TextureFamily; 3] =
        [TextureFamily::Png, TextureFamily::Bc7, TextureFamily::Astc];

    #[test]
    fn png_family_loads_linear_channels_linear() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        let cap = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Png).unwrap();
        assert!(!cap.masks.as_ref().unwrap().srgb);
        let (tile, inv) = cap.tile.unwrap();
        assert_eq!(inv, 0.5);
        assert!(tile[0].srgb && !tile[1].srgb && !tile[2].srgb); // albedo, normal, orm
        assert!(tile.iter().all(|r| r.repeat && r.path.ends_with(".png")));
        let tree = slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Png).unwrap();
        for (ch, r) in &tree.unique {
            assert_eq!(r.srgb, ch.is_srgb(), "{ch:?}");
            assert!(!r.repeat);
        }
    }

    /// Amendment 2 and review M1: what reaches Bevy's image loader, for every family. Bevy's
    /// default is sRGB and 0.20 applies `is_srgb` to KTX2 views too, so normal, ORM and masks
    /// must be forced linear through the settings whatever the file type.
    #[test]
    fn loader_settings_force_color_space_and_addressing_for_every_family() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        // Start every case from Bevy's default (sRGB), so a no-op would fail.
        assert!(
            ImageLoaderSettings::default().is_srgb,
            "Bevy's default changed; revisit amendment 2"
        );
        for family in FAMILIES {
            let cap = slot_requests(&m, p, &p.slots["b_cap"], family).unwrap();
            let tree = slot_requests(&m, p, &p.slots["b_tree"], family).unwrap();
            let mut all: Vec<(Channel, MapRequest)> = tree.unique.clone();
            all.push((Channel::Masks, cap.masks.clone().unwrap()));
            let [ta, tn, to] = cap.tile.clone().unwrap().0;
            all.extend([
                (Channel::Albedo, ta),
                (Channel::Normal, tn),
                (Channel::Orm, to),
            ]);
            let mut linear = 0;
            for (ch, r) in &all {
                assert!(
                    r.path.ends_with(hex_assets::family_suffix(family)),
                    "{family:?} {}",
                    r.path
                );
                let mut s = ImageLoaderSettings::default();
                apply_settings(r, &mut s);
                assert_eq!(s.is_srgb, ch.is_srgb(), "{family:?} {ch:?} {}", r.path);
                linear += usize::from(!s.is_srgb);
                let ImageSampler::Descriptor(d) = &s.sampler else {
                    panic!("{family:?} {ch:?}: sampler not set");
                };
                let want = if r.repeat {
                    ImageAddressMode::Repeat
                } else {
                    ImageAddressMode::ClampToEdge
                };
                assert_eq!(
                    (d.address_mode_u, d.address_mode_v),
                    (want, want),
                    "{family:?} {ch:?}"
                );
            }
            // Unique normal and ORM, masks, tile normal and ORM.
            assert_eq!(linear, 5, "{family:?}");
        }
    }

    #[test]
    fn tileable_and_unique_in_one_slot_is_an_error() {
        let both = M.replace(
            r#""b_tree":{"unique":true}"#,
            r#""b_tree":{"unique":true,"tileable":"grass"}"#,
        );
        let m = parse_manifest(&both).unwrap();
        let p = &m.pieces[0];
        let e = slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).unwrap_err();
        assert!(e.contains("grass") && e.contains("albedo"), "{e}");
        // A unique slot whose piece has only an emissive map does not collide with a tile set.
        let emissive_only = both
            .replace(
                r#","albedo":"b/b.albedo","normal":"b/b.normal","orm":"b/b.orm""#,
                "",
            )
            .replace(
                r#""maps":{"masks":"b/b.masks""#,
                r#""maps":{"masks":"b/b.masks","emissive":"b/b.em""#,
            );
        let m = parse_manifest(&emissive_only).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).unwrap();
        assert!(r.tile.is_some() && r.unique.len() == 1);
    }

    #[test]
    fn tileable_with_alpha_cutoff_is_an_error() {
        let cut = M.replace(
            r#""b_cap":{"tileable":"grass","masks":true}"#,
            r#""b_cap":{"tileable":"grass","masks":true,"alphaCutoff":0.5}"#,
        );
        let m = parse_manifest(&cut).unwrap();
        let p = &m.pieces[0];
        let e = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Bc7).unwrap_err();
        assert!(e.contains("grass") && e.contains("alphaCutoff"), "{e}");
        // Alpha-tested unique slots (leaf cards) stay valid.
        let leaf = M.replace(
            r#""b_tree":{"unique":true}"#,
            r#""b_tree":{"unique":true,"alphaCutoff":0.5}"#,
        );
        let m = parse_manifest(&leaf).unwrap();
        let p = &m.pieces[0];
        assert!(slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).is_ok());
    }

    #[test]
    fn ktx_families_pick_suffix() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Astc).unwrap();
        assert_eq!(r.masks.unwrap().path, "b/b.masks.astc.ktx2");
    }

    #[test]
    fn params_flags() {
        let m = parse_manifest(M).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Bc7).unwrap();
        assert_eq!(
            params_for(&r, KitHooks::None).flags,
            FLAG_TILEABLE | FLAG_MASKS
        );
        assert_eq!(params_for(&r, KitHooks::Snow).snow, 1.0);
    }

    /// A `wetBand` slot takes the live band with the swash amplitude and its tint's wetness; every
    /// other slot leaves it off, so the band never reaches a slot that is not coast sand.
    #[test]
    fn wet_band_slot_flags_and_params() {
        let band = M.replace(
            r#""b_cap":{"tileable":"grass","masks":true}"#,
            r#""b_cap":{"tileable":"grass","masks":true,"wetBand":true}"#,
        );
        let m = parse_manifest(&band).unwrap();
        let p = &m.pieces[0];
        let k = params_for(
            &slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Bc7).unwrap(),
            KitHooks::None,
        );
        assert_eq!(k.flags, FLAG_TILEABLE | FLAG_MASKS | FLAG_WET_BAND);
        assert_eq!((k.wash_amp, k.water_time), (crate::water::WASH_AMP, 0.0));
        let tree = params_for(
            &slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).unwrap(),
            KitHooks::None,
        );
        assert_eq!(tree.flags & FLAG_WET_BAND, 0);
        assert_eq!(tree.wash_amp, 0.0);
        let common = include_str!("kit_common.wesl");
        assert!(common.contains(&format!("const FLAG_WET_BAND: u32 = {FLAG_WET_BAND}u;")));
    }

    /// `kit_normal_basis` (kit_common.wesl) must be transpose(inverse(m)). Its adjugate vectors are
    /// the inverse's rows, so packing them as columns is already the answer; an extra transpose
    /// (2026-10-17 to 10-19) lit every tile not at 0 or 180 degrees as if mirrored about Y.
    #[test]
    fn kit_normal_basis_is_the_inverse_transpose() {
        use bevy::math::{Mat3, Vec3};
        let common = include_str!("kit_common.wesl");
        let body = &common[common.find("fn kit_normal_basis").unwrap()..];
        let body = &body[..body.find("\n}").unwrap()];
        assert!(
            body.contains("return mat3x3(r0 / det, cross(c, a) / det, cross(a, b) / det);")
                && !body.contains("transpose("),
            "kit_normal_basis changed; update the CPU mirror below:\n{body}"
        );
        let basis = |m: Mat3| {
            let (a, b, c) = (m.x_axis, m.y_axis, m.z_axis);
            let r0 = b.cross(c);
            let det = a.dot(r0);
            Mat3::from_cols(r0 / det, c.cross(a) / det, a.cross(b) / det)
        };
        for deg in [60.0_f32, 120.0, 240.0, 300.0] {
            let m = Mat3::from_rotation_y(deg.to_radians())
                * Mat3::from_diagonal(Vec3::new(1.2, 0.8, 1.5));
            let want = m.inverse().transpose();
            let got = basis(m);
            for (g, w) in got.to_cols_array().iter().zip(want.to_cols_array()) {
                assert!((g - w).abs() < 1e-5, "yaw {deg}: {got:?} != {want:?}");
            }
            let n = (got * Vec3::X).normalize();
            assert!(
                n.dot(Mat3::from_rotation_y(deg.to_radians()) * Vec3::X) > 0.99,
                "yaw {deg}: normal not rotated with the piece"
            );
        }
    }

    /// Tree bark: a tileable set on UV0 (uv_transform scale) keeps masks on raw UV0 and must not
    /// take the world-space path (a vertical cylinder has no usable world-space tile frame).
    #[test]
    fn uv_mapped_tileable_slot_skips_world_space_and_scales_uv0() {
        let bark = M.replace(
            r#""b_cap":{"tileable":"grass","masks":true}"#,
            r#""b_cap":{"tileable":"grass","masks":true,"tileUvScale":16.0}"#,
        );
        let m = parse_manifest(&bark).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Bc7).unwrap();
        assert_eq!(r.tile_uv_scale, Some(16.0));
        assert!(r.tile.is_some() && r.masks.is_some());
        assert_eq!(params_for(&r, KitHooks::None).flags, FLAG_MASKS);
        assert_eq!(uv_transform_for(&r), Affine2::from_scale(Vec2::splat(16.0)));
        let world = slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).unwrap();
        assert_eq!(uv_transform_for(&world), Affine2::IDENTITY);
    }

    /// A `wind` slot sways with its amplitude: the tree profile on a tree piece, the plant profile
    /// elsewhere; every other slot stays still. The flags match kit_common.wesl.
    #[test]
    fn wind_slot_flags_follow_the_piece_kind() {
        let windy = M.replace(
            r#""b_tree":{"unique":true}"#,
            r#""b_tree":{"unique":true,"wind":0.4}"#,
        );
        let m = parse_manifest(&windy).unwrap();
        let p = &m.pieces[0];
        let k = params_for(
            &slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).unwrap(),
            KitHooks::None,
        );
        assert_eq!(
            (k.flags & (FLAG_WIND_TREE | FLAG_WIND_PLANT), k.wind_amp),
            (FLAG_WIND_PLANT, 0.4)
        );
        let cap = params_for(
            &slot_requests(&m, p, &p.slots["b_cap"], TextureFamily::Bc7).unwrap(),
            KitHooks::None,
        );
        assert_eq!(
            (cap.flags & (FLAG_WIND_TREE | FLAG_WIND_PLANT), cap.wind_amp),
            (0, 0.0)
        );
        let tree = windy.replace(r#""kind":"base""#, r#""kind":"tree""#);
        let m = parse_manifest(&tree).unwrap();
        let p = &m.pieces[0];
        let r = slot_requests(&m, p, &p.slots["b_tree"], TextureFamily::Bc7).unwrap();
        assert_eq!(
            params_for(&r, KitHooks::None).flags & (FLAG_WIND_TREE | FLAG_WIND_PLANT),
            FLAG_WIND_TREE
        );
        let common = include_str!("kit_common.wesl");
        for (name, v) in [
            ("FLAG_WIND_TREE", FLAG_WIND_TREE),
            ("FLAG_WIND_PLANT", FLAG_WIND_PLANT),
        ] {
            assert!(
                common.contains(&format!("const {name}: u32 = {v}u;")),
                "{name}"
            );
        }
    }

    /// The WGSL struct is eight scalars: the Rust side must stay 32 bytes.
    #[test]
    fn params_are_32_bytes() {
        assert_eq!(KitParams::min_size().get(), 32);
    }
}
