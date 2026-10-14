//! §9.4 hook gate (PRD D7, P2: "every §9.4 hook is present in the MVP assets and shaders").
//! Data-driven from the kit manifests (the gate fixture and `assets/kit`), so a new piece is held
//! to every hook without editing this file, and from the shader directories, so a new shader must
//! be classified here before the gate passes. A module of `kit_gate`, so it runs in `make test` and
//! `make kit-check`.
//!
//! Hooks and where each is checked:
//! - snow, wetness and night emissive: each slot's masks map (authored) or the shader's uniform
//!   default ([`masks_errors`], [`channel_errors`]), and the masks math in each lit kit shader and
//!   in `hooks.wesl` `surface` ([`shader_errors`]);
//! - foliage seasonal tint: [`foliage_errors`] through the runtime `slot_requests`/`params_for`;
//! - time of day: every hooked shader lights through Bevy's view (sun, IBL, exposure) and its post
//!   pass (fog), driven by `hex_render::hooks::atmosphere`;
//! - water freeze: `WaterParams::freeze` on both sides of the uniform ([`struct_errors`]) and in
//!   the water vertex and fragment stages.
//!
//! Exemptions say why a hook is meaningless on a slot or shader. [`KNOWN_MISSING`] lists real gaps
//! awaiting re-authoring: the gate passes with them but fails on any unlisted gap and on any
//! listed entry that no longer applies, so the list only shrinks.
use ::bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, Image, ImageSampler, ImageType},
    reflect::structs::Struct,
};
use hex_assets::*;
use hex_render::{
    KitHooks,
    car_material::CarParams,
    hooks,
    kit_material::{FLAG_FOLIAGE, FLAG_MASKS, KitParams, params_for, slot_requests},
    water::WaterParams,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

struct Kit {
    root: PathBuf,
    manifest: KitManifest,
}

fn kits() -> Vec<Kit> {
    [
        repo().join("crates/hex-render/tests/fixtures/kit"),
        repo().join("assets/kit"),
    ]
    .into_iter()
    .map(|root| {
        let json = std::fs::read_to_string(root.join("manifest.json")).unwrap();
        let manifest = parse_manifest(&json).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
        Kit { root, manifest }
    })
    .collect()
}

fn fail(errors: Vec<String>) {
    assert!(errors.is_empty(), "hooks gate:\n  {}", errors.join("\n  "));
}

// ---------------------------------------------------------------------------------------------
// Snow, wetness and night emissive on every slot (masks map, binding 101).

/// Slots (by slot name, on any piece) where the masks hooks are meaningless, so they run on the
/// shader's uniform default (`hooks.wesl` `NO_MASKS`) and need no authored map.
const MASKS_EXEMPT: &[(&str, &str)] = &[
    (
        "meadow_base_skirt",
        "tile skirt: the O4 cutaway cross-section, not a world surface",
    ),
    (
        "forest_base_skirt",
        "tile skirt: the O4 cutaway cross-section, not a world surface",
    ),
    (
        "beach_skirt",
        "tile skirt: the O4 cutaway cross-section, not a world surface",
    ),
    (
        "river_skirt",
        "tile skirt: the O4 cutaway cross-section, not a world surface",
    ),
];

/// Real gaps, reported for re-authoring: (piece, slot, what is missing). These slots run on the
/// uniform default (full snow on up-facing faces, full porosity, no emitters, no AO) until their
/// piece ships an authored masks map bound to the slot.
const KNOWN_MISSING: &[(&str, &str, &str)] = &[];

fn authored(p: &PieceEntry, slot: &Slot) -> bool {
    slot.masks && p.maps.contains_key(&Channel::Masks)
}

/// Every slot of every piece carries the masks hooks through an authored map, or is listed.
fn masks_errors(
    pieces: &[&PieceEntry],
    exempt: &[(&str, &str)],
    missing: &[(&str, &str, &str)],
) -> Vec<String> {
    let mut e = vec![];
    let mut used_exempt = BTreeSet::new();
    let mut used_missing = BTreeSet::new();
    for p in pieces {
        for (name, slot) in &p.slots {
            let ex = exempt.iter().position(|(s, _)| s == name);
            let mi = missing
                .iter()
                .position(|(pid, s, _)| *pid == p.id && s == name);
            if authored(p, slot) {
                if let Some(i) = ex {
                    e.push(format!(
                        "{}: {name} is exempt ({}) but binds a masks map; drop the exemption",
                        p.id, exempt[i].1
                    ));
                }
                if mi.is_some() {
                    e.push(format!(
                        "{}: {name} is in KNOWN_MISSING but now carries an authored masks map; remove the entry",
                        p.id
                    ));
                }
                continue;
            }
            match (ex, mi) {
                (Some(i), _) => {
                    used_exempt.insert(i);
                }
                (None, Some(i)) => {
                    used_missing.insert(i);
                }
                (None, None) => e.push(format!(
                    "{}: slot {name} carries no snow/wetness/night masks (masks map {}, slot masks {}); \
                     author the map or list the slot with a reason",
                    p.id,
                    if p.maps.contains_key(&Channel::Masks) {
                        "present"
                    } else {
                        "absent"
                    },
                    slot.masks
                )),
            }
        }
    }
    for (i, (s, _)) in exempt.iter().enumerate() {
        if !used_exempt.contains(&i) {
            e.push(format!(
                "MASKS_EXEMPT {s}: no unmasked slot of that name; remove it"
            ));
        }
    }
    for (i, (pid, s, _)) in missing.iter().enumerate() {
        if !used_missing.contains(&i) {
            e.push(format!(
                "KNOWN_MISSING {pid}/{s}: no such unmasked slot; remove it"
            ));
        }
    }
    e
}

#[test]
fn every_slot_carries_the_masks_hooks_or_is_listed() {
    let kits = kits();
    let pieces: Vec<&PieceEntry> = kits.iter().flat_map(|k| &k.manifest.pieces).collect();
    fail(masks_errors(&pieces, MASKS_EXEMPT, KNOWN_MISSING));
}

/// Slot-name suffixes where snow is meaningless: glass sheds it (windows, windscreens).
const SNOW_EXEMPT_SUFFIX: &[(&str, &str)] = &[("_glass", "glass sheds snow")];
/// Slots that sit under water at every sea level, where snow and rain never reach.
const SUBMERGED: &[(&str, &str)] = &[
    ("coast_seabed", "seabed: always under the sea"),
    ("beach_seabed", "seabed: always under the sea"),
    (
        "coast_pebble",
        "ocean_center: pebbles on the ocean floor, under at least 2.5 m of water",
    ),
    (
        "coast_rock",
        "ocean_center: rocks within 0.10 m of the ocean floor, under at least 2.5 m of water",
    ),
    (
        "fish_trout_body",
        "fish_trout: a boid that only swims inside the water volume",
    ),
];
/// A slot named with this suffix is a light-bearing surface (house windows, car lamps): its
/// piece's masks must carry night emission (B) on it.
const EMITTER_SUFFIX: &str = "_glass";

/// One authored slot's masks samples (RGBA in [0, 1]) in one LOD against its channel duties;
/// `lod_b_max` is the night emission over every slot of that LOD.
fn channel_errors(id: &str, slot: &str, samples: &[[f32; 4]], lod_b_max: f32) -> Vec<String> {
    let mut e = vec![];
    if samples.is_empty() {
        return vec![format!(
            "{id}: {slot} has no UV0 triangles to sample the masks at"
        )];
    }
    let max = |c: usize| samples.iter().map(|s| s[c]).fold(0.0f32, f32::max);
    let submerged = SUBMERGED.iter().any(|(s, _)| *s == slot);
    let no_snow = submerged || SNOW_EXEMPT_SUFFIX.iter().any(|(s, _)| slot.ends_with(s));
    if !no_snow && max(0) <= 0.0 {
        e.push(format!(
            "{id}: {slot} masks R (snow accumulation) is 0 everywhere"
        ));
    }
    if !submerged && max(1) <= 0.0 {
        e.push(format!(
            "{id}: {slot} masks G (wetness/porosity) is 0 everywhere"
        ));
    }
    if slot.ends_with(EMITTER_SUFFIX) && lod_b_max <= 0.0 {
        e.push(format!(
            "{id}: {slot} is light-bearing but the piece's masks B (night emission) is 0 everywhere"
        ));
    }
    e
}

fn decode_masks(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let img = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        false,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (img.width(), img.height());
    let data = img
        .data
        .ok_or_else(|| format!("{}: no pixels", path.display()))?;
    if data.len() != (w * h * 4) as usize {
        return Err(format!(
            "{}: masks must be 8-bit RGBA (R snow, G wetness, B night, A AO)",
            path.display()
        ));
    }
    Ok((w, h, data))
}

/// The masks sampled (nearest) at the UV0 centroid of every triangle of each material in LOD
/// `lod` (each LOD has its own islands in the atlas, so a coarse LOD can lose a channel).
fn slot_samples(
    kit: &Kit,
    p: &PieceEntry,
    masks: &(u32, u32, Vec<u8>),
    lod: usize,
) -> Result<Vec<(String, Vec<[f32; 4]>)>, String> {
    let bytes = std::fs::read(kit.root.join(&p.glb)).map_err(|e| format!("{}: {e}", p.id))?;
    let plain = rewrite_glb(&bytes).map_err(|e| format!("{}: {e}", p.id))?;
    let (doc, buffers, _) = gltf::import_slice(&plain).map_err(|e| format!("{}: {e}", p.id))?;
    let mesh_name = format!("{}_lod{lod}", p.id);
    let mesh = doc
        .meshes()
        .find(|m| m.name() == Some(mesh_name.as_str()))
        .ok_or_else(|| format!("{}: mesh {mesh_name} missing", p.id))?;
    let (w, h, px) = masks;
    let mut out: Vec<(String, Vec<[f32; 4]>)> = vec![];
    for prim in mesh.primitives() {
        let name = prim.material().name().unwrap_or_default().to_string();
        let r = prim.reader(|b| Some(&buffers[b.index()]));
        let Some(uv) = r.read_tex_coords(0) else {
            continue;
        };
        let uv: Vec<[f32; 2]> = uv.into_f32().collect();
        let idx: Vec<u32> = r
            .read_indices()
            .map(|i| i.into_u32().collect())
            .unwrap_or_else(|| (0..uv.len() as u32).collect());
        let mut s = vec![];
        for t in idx.as_chunks::<3>().0 {
            let c = t.iter().fold([0.0f32; 2], |a, &i| {
                [
                    a[0] + uv[i as usize][0] / 3.0,
                    a[1] + uv[i as usize][1] / 3.0,
                ]
            });
            let x = ((c[0].rem_euclid(1.0) * *w as f32) as u32).min(w - 1);
            let y = ((c[1].rem_euclid(1.0) * *h as f32) as u32).min(h - 1);
            let o = ((y * w + x) * 4) as usize;
            s.push([0, 1, 2, 3].map(|k| px[o + k] as f32 / 255.0));
        }
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some((_, v)) => v.extend(s),
            None => out.push((name, s)),
        }
    }
    Ok(out)
}

#[test]
fn authored_masks_carry_snow_wetness_and_night_channels() {
    let mut e = vec![];
    for kit in kits() {
        for p in &kit.manifest.pieces {
            let Some(stem) = p.maps.get(&Channel::Masks) else {
                continue;
            };
            let path = kit.root.join(map_file(stem, TextureFamily::Png));
            let masks = match decode_masks(&path) {
                Ok(m) => m,
                Err(err) => {
                    e.push(err);
                    continue;
                }
            };
            for lod in 0..3 {
                let samples = match slot_samples(&kit, p, &masks, lod) {
                    Ok(s) => s,
                    Err(err) => {
                        e.push(err);
                        continue;
                    }
                };
                let lod_b_max = samples
                    .iter()
                    .flat_map(|(_, v)| v.iter().map(|s| s[2]))
                    .fold(0.0f32, f32::max);
                let id = format!("{} lod{lod}", p.id);
                for (name, slot) in &p.slots {
                    // A slot is held to its channels in every LOD that draws it, and LOD0 must draw it.
                    let s = samples.iter().find(|(n, _)| n == name);
                    if !slot.masks || (lod > 0 && s.is_none()) {
                        continue;
                    }
                    let s = s.map(|(_, v)| v.as_slice()).unwrap_or_default();
                    e.extend(channel_errors(&id, name, s, lod_b_max));
                }
            }
        }
    }
    fail(e);
}

// ---------------------------------------------------------------------------------------------
// Foliage seasonal tint (per-instance MeshTag on a FLAG_FOLIAGE slot).

/// Opaque plant slots outside the alpha-tested and tree rules (slot name, what it is). Each must
/// carry the manifest's `"foliage": true`, and a slot that carries it must be listed here, so the
/// flag is checked against this list and never against itself.
const PLANTS: &[(&str, &str)] = &[
    ("house_pad_hedge", "the house pads' clipped hedges"),
    ("beach_dunegrass", "dune-grass tufts on the beach tiles"),
    ("river_reed", "reed blades on the river banks"),
    ("river_cattail", "cattail seed heads on the reeds"),
];

/// The requirement, stated independently of `hooks::is_foliage`: every alpha-tested slot, every
/// non-bark slot of a tree and every slot in [`PLANTS`] is foliage, and every tree has foliage.
fn foliage_errors(m: &KitManifest) -> Vec<String> {
    foliage_errors_with(m, PLANTS, &mut BTreeSet::new())
}

/// [`foliage_errors`] against `plants`, adding the indices of the entries it matched to `used`.
fn foliage_errors_with(
    m: &KitManifest,
    plants: &[(&str, &str)],
    used: &mut BTreeSet<usize>,
) -> Vec<String> {
    let mut e = vec![];
    for p in &m.pieces {
        let mut any = false;
        for (name, slot) in &p.slots {
            let plant = plants.iter().position(|(s, _)| s == name);
            if let Some(i) = plant {
                used.insert(i);
            }
            let want = slot.alpha_cutoff.is_some()
                || (p.kind == PieceKind::Tree && slot.tileable.as_deref() != Some("bark"))
                || plant.is_some();
            let got = match slot_requests(m, p, slot, TextureFamily::Png) {
                Ok(r) => params_for(&r, KitHooks::None).flags & FLAG_FOLIAGE != 0,
                Err(err) => {
                    e.push(err);
                    continue;
                }
            };
            any |= got;
            if want && !got {
                e.push(format!(
                    "{}: {name} is foliage but its material does not take the seasonal tint (FLAG_FOLIAGE)",
                    p.id
                ));
            }
            if got && !want {
                e.push(format!(
                    "{}: {name} takes the seasonal tint but is not foliage",
                    p.id
                ));
            }
        }
        if p.kind == PieceKind::Tree && !any {
            e.push(format!(
                "{}: tree has no foliage slot for the seasonal tint",
                p.id
            ));
        }
    }
    e
}

/// A [`PLANTS`] entry that matched no slot in any manifest no longer applies.
fn stale_plants(plants: &[(&str, &str)], used: &BTreeSet<usize>) -> Vec<String> {
    plants
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(_, (s, _))| format!("PLANTS {s}: no slot of that name; remove it"))
        .collect()
}

#[test]
fn foliage_slots_take_the_per_instance_season_tint() {
    let mut e = vec![];
    let mut used = BTreeSet::new();
    for kit in kits() {
        e.extend(foliage_errors_with(&kit.manifest, PLANTS, &mut used));
    }
    e.extend(stale_plants(PLANTS, &used));
    fail(e);
}

// ---------------------------------------------------------------------------------------------
// MVP values: every hook neutral in every material the manifests build.

#[test]
fn mvp_values_are_neutral_for_every_slot() {
    let mut e = vec![];
    for kit in kits() {
        let m = &kit.manifest;
        for p in &m.pieces {
            for (name, slot) in &p.slots {
                let Ok(r) = slot_requests(m, p, slot, TextureFamily::Png) else {
                    continue;
                };
                let k = params_for(&r, KitHooks::None);
                if [k.snow, k.wetness, k.night] != [0.0; 3] {
                    e.push(format!("{}: {name} kit hooks not neutral: {k:?}", p.id));
                }
                if (k.flags & FLAG_MASKS != 0) != r.masks.is_some() {
                    e.push(format!(
                        "{}: {name} FLAG_MASKS disagrees with its masks map",
                        p.id
                    ));
                }
                let c = hex_render::car_material::params_for(&r, KitHooks::None);
                if [c.snow, c.wetness, c.night] != [0.0; 3] {
                    e.push(format!("{}: {name} car hooks not neutral: {c:?}", p.id));
                }
            }
        }
    }
    assert!(hex_render::GateConfig::default().kit_hooks == KitHooks::None);
    assert!(hooks::TimeOfDay::default().is_noon());
    assert_eq!(hooks::atmosphere(hooks::NOON).night, 0.0);
    assert_eq!(hooks::atmosphere(hooks::NOON).fog, None);
    assert_eq!(WaterParams::default().freeze, 0.0);
    let open = hex_render::water::WaterKey::for_tier(hex_render::post::QualityTier::High);
    assert!(
        !open.freeze,
        "open water must not compile the freeze variant"
    );
    fail(e);
}

// ---------------------------------------------------------------------------------------------
// Shaders: every kit and water shader is hooked or exempt with a reason.

#[derive(Clone, Copy, Debug, PartialEq)]
enum Need {
    /// Snow (up-facing), wetness and night emission on the sampled masks, and its AO.
    Masks,
    /// The uniform default for a slot with no masks map, only while a hook is on.
    MasksDefault,
    /// The per-instance seasonal tint on foliage slots.
    Foliage,
    /// Lit by the view's sun, IBL and exposure, and fogged by the post-lighting pass, which the
    /// time-of-day block drives (`hooks::atmosphere`).
    TimeOfDay,
    /// `water.freeze` in both stages.
    Freeze,
    /// The hook library itself.
    Library,
}

/// Shaders the kit and water draw with, and the hooks each must carry (paths from the repo root).
const HOOKED: &[(&str, &[Need])] = &[
    (
        "assets/shaders/kit_material.wesl",
        &[
            Need::Masks,
            Need::MasksDefault,
            Need::Foliage,
            Need::TimeOfDay,
        ],
    ),
    (
        "assets/shaders/car_material.wesl",
        &[Need::Masks, Need::MasksDefault, Need::TimeOfDay],
    ),
    (
        "assets/shaders/water.wesl",
        &[Need::Freeze, Need::TimeOfDay],
    ),
    ("crates/hex-render/src/hooks.wesl", &[Need::Library]),
];

/// Shaders where the hooks are meaningless, with the reason.
const SHADER_EXEMPT: &[(&str, &str)] = &[
    (
        "assets/shaders/kit_prepass.wesl",
        "depth/normal prepass of the kit material: the hooks change colour, not depth or normals",
    ),
    (
        "assets/shaders/water_terrain.wesl",
        "placeholder bed, drawn only until a tile's centre piece is in the kit (water contract)",
    ),
    (
        "assets/shaders/instanced_piece.wesl",
        "P0 perf-gate layout of synthetic pieces, not an MVP asset path",
    ),
    (
        "assets/shaders/boid_material.wesl",
        "unlit flat boid silhouette (boids stream); takes the hooks when it is lit",
    ),
    (
        "assets/shaders/boids_compute.wgsl",
        "compute pass: no surface",
    ),
    (
        "assets/shaders/affordance.wesl",
        "placement UI overlay, not a world surface",
    ),
    (
        "assets/shaders/ghost.wesl",
        "see-through placement preview, not a world surface",
    ),
    ("assets/shaders/tilt_shift.wesl", "post pass: no surface"),
    (
        "crates/hex-render/src/kit_common.wesl",
        "library: tiling frame and the kit uniform; its flags are checked with kit_material",
    ),
    (
        "crates/hex-render/src/water_common.wesl",
        "library: wave functions, no surface",
    ),
];

/// Source without `//` comments, so a hook named only in a comment does not count.
fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|l| l.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

fn need_errors(path: &str, src: &str, need: Need) -> Vec<String> {
    let code = strip_comments(src);
    let has = |s: &str| code.contains(s);
    let mut e = vec![];
    let mut want = |ok: bool, what: &str| {
        if !ok {
            e.push(format!("{path}: missing {need:?} hook ({what})"));
        }
    };
    match need {
        Need::Masks => {
            for (needle, what) in [
                ("textureSample(", "the masks sample"),
                ("pbr_input.diffuse_occlusion *= m.a", "masks.A baked AO"),
                (
                    "smoothstep(0.55, 0.85, pbr_input.N.y)",
                    "up-facing snow coverage",
                ),
                (".snow *", "masks.R × the snow uniform"),
                ("m.g * ", "masks.G wetness/porosity"),
                (".wetness", "the wetness uniform"),
                ("m.b * ", "masks.B night-only emission"),
                (".night * 4.0", "the night uniform"),
            ] {
                want(has(needle), what);
            }
        }
        Need::MasksDefault => want(
            has("hooks::surface(pbr_input, hooks::NO_MASKS,") && has("hooks::any_on("),
            "hooks::NO_MASKS for a slot without a masks map, gated by hooks::any_on",
        ),
        Need::Foliage => want(
            has("FLAG_FOLIAGE") && has("get_tag(in.instance_index)") && has("hooks::season_tint("),
            "the instance's MeshTag through hooks::season_tint on FLAG_FOLIAGE slots",
        ),
        Need::TimeOfDay => want(
            has("apply_pbr_lighting(") && has("main_pass_post_lighting_processing("),
            "Bevy's view lighting (sun, IBL, exposure) and post-lighting fog",
        ),
        Need::Freeze => {
            let (vs, fs) = code.split_once("@fragment").unwrap_or((code.as_str(), ""));
            want(
                vs.contains("p = mix(p, rest, water.freeze);"),
                "water.freeze in the vertex stage (ice holds the surface at rest)",
            );
            want(
                fs.contains("ice = water.freeze") && fs.contains("ICE_ALBEDO, ice"),
                "water.freeze in the fragment stage (the ice sheet)",
            );
            want(
                code.matches("@if(WATER_FREEZE)").count() >= 4,
                "every freeze path behind the WATER_FREEZE variant, so open water is unchanged",
            );
        }
        Need::Library => {
            for (needle, what) in [
                ("pbr_input.diffuse_occlusion *= m.a", "masks.A baked AO"),
                ("m.r * snow", "masks.R snow accumulation"),
                (
                    "smoothstep(0.55, 0.85, pbr_input.N.y)",
                    "up-facing snow coverage",
                ),
                ("m.g * wetness", "masks.G wetness/porosity"),
                ("m.b * night", "masks.B night-only emission"),
                (
                    "const NO_MASKS: vec4<f32> = vec4(1.0, 1.0, 0.0, 1.0);",
                    "NO_MASKS: snow and porosity on, no emitters, no AO",
                ),
                (
                    "snow > 0.0 || wetness > 0.0 || night > 0.0",
                    "hooks::any_on",
                ),
                ("fn season_tint(index: u32)", "the season tint table"),
            ] {
                want(has(needle), what);
            }
            let default_arm = code.split("default:").nth(1).is_some_and(|s| {
                s.trim_start().starts_with("{") && s.contains("return vec3(1.0);")
            });
            want(default_arm, "season 0 (summer) multiplies by exactly 1");
        }
    }
    e
}

fn shader_files() -> Vec<String> {
    let mut out = vec![];
    for dir in ["assets/shaders", "crates/hex-render/src"] {
        for entry in std::fs::read_dir(repo().join(dir)).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            if name.ends_with(".wesl") || name.ends_with(".wgsl") {
                out.push(format!("{dir}/{name}"));
            }
        }
    }
    out.sort();
    out
}

fn shader_errors(
    files: &[String],
    read: &dyn Fn(&str) -> String,
    hooked: &[(&str, &[Need])],
    exempt: &[(&str, &str)],
) -> Vec<String> {
    let mut e = vec![];
    for f in files {
        let h = hooked.iter().find(|(p, _)| p == f);
        let x = exempt.iter().find(|(p, _)| p == f);
        match (h, x) {
            (Some(_), Some(_)) => e.push(format!("{f}: both hooked and exempt")),
            (None, None) => e.push(format!(
                "{f}: unclassified shader; add it to HOOKED with its hooks or to SHADER_EXEMPT with a reason"
            )),
            (Some((_, needs)), None) => {
                let src = read(f);
                for n in needs.iter() {
                    e.extend(need_errors(f, &src, *n));
                }
            }
            (None, Some(_)) => {}
        }
    }
    for (p, _) in hooked.iter().map(|(p, n)| (p, n)) {
        if !files.iter().any(|f| f == p) {
            e.push(format!("HOOKED {p}: no such shader"));
        }
    }
    for (p, _) in exempt {
        if !files.iter().any(|f| f == p) {
            e.push(format!("SHADER_EXEMPT {p}: no such shader; remove it"));
        }
    }
    e
}

fn read_repo(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap()
}

#[test]
fn kit_and_water_shaders_carry_their_hooks() {
    fail(shader_errors(
        &shader_files(),
        &read_repo,
        HOOKED,
        SHADER_EXEMPT,
    ));
}

// ---------------------------------------------------------------------------------------------
// Uniform blocks: the WGSL structs match their Rust ShaderType field for field, and carry the
// hook fields.

fn wgsl_fields(src: &str, name: &str) -> Option<Vec<String>> {
    let code = strip_comments(src);
    let body = code
        .split(&format!("struct {name} {{"))
        .nth(1)?
        .split('}')
        .next()?;
    Some(
        body.split(',')
            .filter_map(|f| f.split(':').next())
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .collect(),
    )
}

fn rust_fields(s: &dyn Struct) -> Vec<String> {
    (0..s.field_len())
        .map(|i| s.name_at(i).unwrap().to_string())
        .collect()
}

fn struct_errors(
    path: &str,
    src: &str,
    name: &str,
    rust: &[String],
    hook_fields: &[&str],
) -> Vec<String> {
    let mut e = vec![];
    match wgsl_fields(src, name) {
        None => e.push(format!("{path}: struct {name} not found")),
        Some(w) if w != rust => e.push(format!(
            "{path}: struct {name} fields {w:?} differ from the Rust ShaderType {rust:?}"
        )),
        Some(_) => {}
    }
    for f in hook_fields {
        if !rust.iter().any(|r| r == f) {
            e.push(format!("{name}: hook field {f} missing"));
        }
    }
    e
}

#[test]
fn hook_uniform_blocks_match_their_shaders() {
    let mut e = vec![];
    for (path, name, rust, hook_fields) in [
        (
            "crates/hex-render/src/kit_common.wesl",
            "KitParams",
            rust_fields(&KitParams::default()),
            &["snow", "wetness", "night", "flags"][..],
        ),
        (
            "assets/shaders/car_material.wesl",
            "CarParams",
            rust_fields(&CarParams::default()),
            &["snow", "wetness", "night"][..],
        ),
        (
            "assets/shaders/water.wesl",
            "WaterParams",
            rust_fields(&WaterParams::default()),
            &["freeze"][..],
        ),
    ] {
        e.extend(struct_errors(
            path,
            &read_repo(path),
            name,
            &rust,
            hook_fields,
        ));
    }
    // The foliage flag has one value on both sides.
    let common = strip_comments(&read_repo("crates/hex-render/src/kit_common.wesl"));
    if !common.contains(&format!("const FLAG_FOLIAGE: u32 = {FLAG_FOLIAGE}u;")) {
        e.push(format!(
            "kit_common.wesl: FLAG_FOLIAGE must be {FLAG_FOLIAGE}u as in kit_material.rs"
        ));
    }
    fail(e);
}

// ---------------------------------------------------------------------------------------------
// The gate catches each missing hook (a check that cannot fail proves nothing).

fn runtime_manifest() -> KitManifest {
    kits().pop().unwrap().manifest
}

#[test]
fn gate_catches_an_unmasked_slot_and_stale_lists() {
    let mut m = runtime_manifest();
    let p = m.pieces.iter_mut().find(|p| p.id == "meadow_base").unwrap();
    p.slots.get_mut("meadow_base_cap").unwrap().masks = false;
    let pieces: Vec<&PieceEntry> = m.pieces.iter().collect();
    let e = masks_errors(&pieces, MASKS_EXEMPT, KNOWN_MISSING);
    assert!(
        e.len() == 1 && e[0].contains("meadow_base: slot meadow_base_cap carries no"),
        "{e:?}"
    );
    // A new piece with no masks map is caught without editing the gate.
    let mut m = runtime_manifest();
    let mut fresh = m.pieces[0].clone();
    fresh.id = "new_piece".into();
    fresh.maps.clear();
    m.pieces.push(fresh);
    let pieces: Vec<&PieceEntry> = m.pieces.iter().collect();
    let e = masks_errors(&pieces, MASKS_EXEMPT, KNOWN_MISSING);
    assert!(e.iter().any(|s| s.contains("new_piece: slot")), "{e:?}");
    // A KNOWN_MISSING entry that no longer applies fails, as does an unused exemption.
    let m = runtime_manifest();
    let pieces: Vec<&PieceEntry> = m.pieces.iter().collect();
    let mut stale = KNOWN_MISSING.to_vec();
    stale.push(("meadow_base", "meadow_base_cap", "test"));
    let e = masks_errors(&pieces, MASKS_EXEMPT, &stale);
    assert!(e.iter().any(|s| s.contains("remove the entry")), "{e:?}");
    let mut ex = MASKS_EXEMPT.to_vec();
    ex.push(("no_such_slot", "test"));
    let e = masks_errors(&pieces, &ex, KNOWN_MISSING);
    assert!(
        e.iter().any(|s| s.contains("MASKS_EXEMPT no_such_slot")),
        "{e:?}"
    );
}

#[test]
fn gate_catches_empty_mask_channels() {
    let blank = [[0.0, 0.0, 0.0, 1.0]; 3];
    let e = channel_errors("p", "p_walls", &blank, 0.0);
    assert!(
        e.iter().any(|s| s.contains("R (snow")) && e.iter().any(|s| s.contains("G (wetness")),
        "{e:?}"
    );
    // Glass is exempt from snow but must carry night emission.
    let e = channel_errors("p", "p_glass", &[[0.0, 0.5, 0.0, 1.0]], 0.0);
    assert!(e.len() == 1 && e[0].contains("night emission"), "{e:?}");
    assert!(channel_errors("p", "p_glass", &[[0.0, 0.5, 0.0, 1.0]], 1.0).is_empty());
    // A submerged seabed needs neither.
    assert!(channel_errors("p", "coast_seabed", &blank, 0.0).is_empty());
}

#[test]
fn gate_catches_foliage_without_the_season_tint() {
    let mut m = runtime_manifest();
    // A tree whose only slot is bark has nothing to tint.
    let oak = m
        .pieces
        .iter_mut()
        .find(|p| p.id == "tree_hero_oak")
        .unwrap();
    oak.slots.retain(|n, _| n.ends_with("_bark"));
    let e = foliage_errors(&m);
    assert!(
        e.iter()
            .any(|s| s.contains("tree_hero_oak: tree has no foliage slot")),
        "{e:?}"
    );
    // A listed plant without the manifest flag, a flagged slot that is not a listed plant, and a
    // stale list entry each fail.
    let mut m = runtime_manifest();
    let pad = m.pieces.iter_mut().find(|p| p.id == "house_pad").unwrap();
    pad.slots.get_mut("house_pad_hedge").unwrap().foliage = false;
    pad.slots.get_mut("house_pad_gravel").unwrap().foliage = true;
    let e = foliage_errors(&m);
    assert!(
        e.iter().any(|s| s.contains(
            "house_pad: house_pad_hedge is foliage but its material does not take the seasonal tint"
        )) && e.iter().any(|s| s
            .contains("house_pad: house_pad_gravel takes the seasonal tint but is not foliage")),
        "{e:?}"
    );
    let mut plants = PLANTS.to_vec();
    plants.push(("no_such_plant", "test"));
    let mut used = BTreeSet::new();
    let mut e = foliage_errors_with(&runtime_manifest(), &plants, &mut used);
    e.extend(stale_plants(&plants, &used));
    assert!(
        e.len() == 1 && e[0].contains("PLANTS no_such_plant"),
        "{e:?}"
    );
}

#[test]
fn gate_catches_shaders_without_their_hooks() {
    let files = shader_files();
    let cut = |path: &'static str, from: &'static str, to: &'static str| {
        move |f: &str| {
            let s = read_repo(f);
            if f == path {
                assert!(s.contains(from), "{path}: {from} not found");
                s.replace(from, to)
            } else {
                s
            }
        }
    };
    let cases: [(&str, &str, &str, &str); 6] = [
        (
            "assets/shaders/kit_material.wesl",
            "c0 * m.b * kit_common::kit.night * 4.0",
            "vec3(0.0)",
            "Masks",
        ),
        (
            "assets/shaders/kit_material.wesl",
            "hooks::season_tint(season)",
            "vec3(1.0)",
            "Foliage",
        ),
        (
            "assets/shaders/car_material.wesl",
            "hooks::NO_MASKS",
            "vec4(1.0)",
            "MasksDefault",
        ),
        (
            "assets/shaders/water.wesl",
            "p = mix(p, rest, water.freeze);",
            "",
            "Freeze",
        ),
        (
            "crates/hex-render/src/hooks.wesl",
            "m.b * night",
            "m.b",
            "Library",
        ),
        (
            "crates/hex-render/src/hooks.wesl",
            "return vec3(1.0);",
            "return vec3(0.9);",
            "Library",
        ),
    ];
    for (path, from, to, need) in cases {
        let e = shader_errors(&files, &cut(path, from, to), HOOKED, SHADER_EXEMPT);
        assert!(
            e.iter()
                .any(|s| s.starts_with(path) && s.contains(&format!("missing {need} hook"))),
            "removing {from:?} from {path}: {e:?}"
        );
    }
    // A hook named only in a comment does not count.
    let commented = |f: &str| {
        let s = read_repo(f);
        if f == "assets/shaders/water.wesl" {
            s.replace("p = mix(p, rest, water.freeze);", "// water.freeze")
                .replace(
                    "if (water.freeze > 0.0) {\n        p",
                    "if (true) {\n        p",
                )
        } else {
            s
        }
    };
    let e = shader_errors(&files, &commented, HOOKED, SHADER_EXEMPT);
    assert!(
        e.iter()
            .any(|s| s.contains("water.freeze in the vertex stage")),
        "{e:?}"
    );
    // A new shader must be classified.
    let mut more = files.clone();
    more.push("assets/shaders/new_surface.wesl".into());
    let e = shader_errors(
        &more,
        &|f: &str| {
            if f.ends_with("new_surface.wesl") {
                String::new()
            } else {
                read_repo(f)
            }
        },
        HOOKED,
        SHADER_EXEMPT,
    );
    assert!(
        e.iter()
            .any(|s| s.contains("new_surface.wesl: unclassified shader")),
        "{e:?}"
    );
}

#[test]
fn gate_catches_a_uniform_block_without_its_hook_field() {
    let src = read_repo("assets/shaders/water.wesl").replace("    freeze: f32,\n", "");
    let rust = rust_fields(&WaterParams::default());
    let e = struct_errors("water.wesl", &src, "WaterParams", &rust, &["freeze"]);
    assert!(
        e.iter().any(|s| s.contains("differ from the Rust")),
        "{e:?}"
    );
    let mut no_freeze = rust.clone();
    no_freeze.retain(|f| f != "freeze");
    let e = struct_errors("water.wesl", &src, "WaterParams", &no_freeze, &["freeze"]);
    assert!(
        e.iter().any(|s| s.contains("hook field freeze missing")),
        "{e:?}"
    );
}

/// The hook-by-slot matrix for review: `cargo test -p hex-render --test kit_gate print_hook_matrix
/// -- --ignored --nocapture`. Masks: `authored`, `exempt` or `MISSING`; R/G/B: the slot's max snow, wetness and
/// night-emission samples; tint: takes the foliage season tint.
#[test]
#[ignore]
fn print_hook_matrix() {
    let kit = kits().pop().unwrap();
    let m = &kit.manifest;
    println!("piece | slot | masks | R snow | G wet | B night | tint");
    for p in &m.pieces {
        let samples = p.maps.get(&Channel::Masks).and_then(|stem| {
            let masks = decode_masks(&kit.root.join(map_file(stem, TextureFamily::Png))).ok()?;
            slot_samples(&kit, p, &masks, 0).ok()
        });
        for (name, slot) in &p.slots {
            let state = if authored(p, slot) {
                "authored"
            } else if MASKS_EXEMPT.iter().any(|(s, _)| s == name) {
                "exempt"
            } else {
                "MISSING"
            };
            let max = |c: usize| {
                samples
                    .as_ref()
                    .and_then(|v| v.iter().find(|(n, _)| n == name))
                    .filter(|_| slot.masks)
                    .map_or("-".to_string(), |(_, v)| {
                        format!("{:.2}", v.iter().map(|s| s[c]).fold(0.0f32, f32::max))
                    })
            };
            let tint = slot_requests(m, p, slot, TextureFamily::Png)
                .map(|r| params_for(&r, KitHooks::None).flags & FLAG_FOLIAGE != 0)
                .unwrap_or(false);
            println!(
                "{} | {name} | {state} | {} | {} | {} | {}",
                p.id,
                max(0),
                max(1),
                max(2),
                if tint { "yes" } else { "-" }
            );
        }
    }
}
