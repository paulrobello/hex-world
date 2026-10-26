//! Runtime slot coalescing for the instanced settled path (close-camera draw calls, owner A2 Q3).
//!
//! Every manifest slot of a piece builds its own `KitMaterial`, and the instanced path draws one
//! (slot, LOD) per pass, so a house with eight coloured slots costs eight draws in every view.
//! Slots of one piece whose built materials differ only in base colour and roughness merge here,
//! per LOD, into one mesh: each vertex carries its slot's linear colour in COLOR_0 (bevy's
//! `pbr_input_from_standard_material` multiplies it into the base colour; the tileable path takes
//! it as the tint) and, for opaque slots of differing roughness, its roughness in COLOR_0 alpha
//! (`FLAG_VERTEX_ROUGHNESS`). The merged material is the first member's with a white base colour,
//! so every shaded value is the product the separate slots computed.
//!
//! Only the instanced draws use the merged meshes ([`super::chunks::KitFamilies`]). Kit parts,
//! the per-entity rise, and every CPU consumer that reads a slot by name (scatter floors, walk
//! colliders, car ground slots, ghost roles, boats) keep the original meshes.
use std::collections::HashMap;

use bevy::prelude::*;

use super::{KitMaterial, KitPart};
use crate::kit_material::{
    FLAG_ANIMAL, FLAG_BLOSSOM, FLAG_SLOT_COLORS, FLAG_VERTEX_ROUGHNESS, FLAG_WET_BAND,
    FLAG_WET_SEABED,
};

/// Slots whose shader reads their own material colour or animates them per slot: kept apart.
const EXCLUDED_FLAGS: u32 = FLAG_ANIMAL | FLAG_BLOSSOM | FLAG_WET_BAND | FLAG_WET_SEABED;

/// One merged draw of a piece: the member slots' materials in family order, the merged mesh of
/// each LOD the members share, and the merged material.
#[derive(Clone, Debug)]
pub struct CoalescedGroup {
    pub members: Vec<AssetId<KitMaterial>>,
    /// Distinct roughness values among the members: above 1, the group draws as one only through
    /// per-vertex roughness ([`FLAG_VERTEX_ROUGHNESS`]); with roughness in the key it would be this
    /// many draws.
    pub roughness_values: usize,
    pub meshes: [Option<Handle<Mesh>>; 3],
    pub material: Handle<KitMaterial>,
}

/// Every built piece's merged groups, by piece id (a piece with none draws slot by slot), and the
/// `piece/slot: reason` of every slot left out for a reason other than having no partner.
#[derive(Default)]
pub struct KitCoalesced(
    pub HashMap<String, Vec<CoalescedGroup>>,
    pub std::collections::BTreeSet<String>,
);

/// Why a slot is left out of coalescing (the kit-load log counts them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Excluded {
    /// The piece has a rotor: its first body part per LOD stays per-entity under it.
    Rotor,
    /// A mesh already carries COLOR_0 (the river mouth fan's detail weight, wind origins), or a
    /// mesh has not loaded.
    VertexColors,
    /// Animal, blossom or wet-band slot ([`EXCLUDED_FLAGS`]).
    Flags,
    /// Alpha-blended: drawn in the transparent phase, not by the instanced path.
    Blend,
    /// Its primitives are not present at the same LODs, one of them missing LOD1 (the A2 Q4
    /// LOD0-through-LOD1 fallback).
    LodPattern,
    /// Its primitives have different vertex layouts.
    Layout,
    /// No other slot of the piece shares its material.
    Alone,
}

/// The vertex layout signature a merge needs to agree on: topology, indexed or not, and every
/// attribute's id and format.
fn layout_key(mesh: &Mesh) -> String {
    let mut attrs: Vec<String> = mesh
        .attributes()
        .map(|(a, _)| format!("{:?}:{:?}", a.id, a.format))
        .collect();
    attrs.sort();
    format!(
        "{:?}|{}|{}",
        mesh.primitive_topology(),
        mesh.indices().is_some(),
        attrs.join(",")
    )
}

/// The material with its slot colour and (for opaque slots) roughness taken out: equal keys
/// merge. `Debug` covers every StandardMaterial field, texture handle and `KitExt` parameter.
fn material_key(m: &KitMaterial) -> String {
    let mut n = m.clone();
    let a = n.base.base_color.alpha();
    n.base.base_color = Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, a));
    if matches!(n.base.alpha_mode, AlphaMode::Opaque) {
        n.base.perceptual_roughness = 1.0;
    }
    format!("{n:?}")
}

/// The slot colour a member's vertices carry: linear RGB, and roughness in A when the group's
/// roughness varies (1 otherwise).
fn slot_color(m: &KitMaterial, vary: bool) -> [f32; 4] {
    let c = m.base.base_color.to_linear();
    [
        c.red,
        c.green,
        c.blue,
        if vary {
            m.base.perceptual_roughness
        } else {
            1.0
        },
    ]
}

/// `mesh` with every vertex coloured `color`.
fn coloured(mesh: &Mesh, color: [f32; 4]) -> Mesh {
    let mut m = mesh.clone();
    let n = m.count_vertices();
    m.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![color; n]);
    m
}

/// The merged groups of one built piece (`parts`, as `build_piece` returns them), adding their
/// meshes and materials to the assets, and the slots left out with why.
pub(crate) fn coalesce_piece(
    parts: &[KitPart],
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<KitMaterial>,
) -> (Vec<CoalescedGroup>, Vec<(AssetId<KitMaterial>, Excluded)>) {
    // Families in the order `KitFamilies::build` numbers them: (material, ordinal among that
    // material's primitives in the LOD), first appearance first, with the LODs each is in.
    let mut families: Vec<(AssetId<KitMaterial>, u32, [bool; 3])> = vec![];
    let mut ords: HashMap<(AssetId<KitMaterial>, usize), u32> = HashMap::new();
    let mut part_family: Vec<Option<usize>> = vec![];
    let mut order: Vec<AssetId<KitMaterial>> = vec![];
    for part in parts {
        if part.rotor.is_some() {
            part_family.push(None);
            continue;
        }
        let m = part.material.id();
        let ord = ords.entry((m, part.lod)).or_default();
        let fi = match families.iter().position(|f| f.0 == m && f.1 == *ord) {
            Some(fi) => fi,
            None => {
                families.push((m, *ord, [false; 3]));
                families.len() - 1
            }
        };
        *ord += 1;
        families[fi].2[part.lod.min(2)] = true;
        part_family.push(Some(fi));
        if !order.contains(&m) {
            order.push(m);
        }
    }
    let exclude_all = |why: Excluded| order.iter().map(|&m| (m, why)).collect::<Vec<_>>();
    if parts.iter().any(|p| p.rotor.is_some()) {
        return (vec![], exclude_all(Excluded::Rotor));
    }
    let mesh_of = |p: &KitPart| meshes.get(&p.mesh);
    if parts
        .iter()
        .any(|p| mesh_of(p).is_none_or(|m| m.attribute(Mesh::ATTRIBUTE_COLOR).is_some()))
    {
        return (vec![], exclude_all(Excluded::VertexColors));
    }
    // Group the eligible slots by (material key, LOD pattern, layout).
    let mut excluded = vec![];
    let mut keyed: Vec<(String, Vec<AssetId<KitMaterial>>)> = vec![];
    for &m in &order {
        let Some(mat) = materials.get(m) else {
            continue;
        };
        if mat.extension.params.flags & EXCLUDED_FLAGS != 0 {
            excluded.push((m, Excluded::Flags));
            continue;
        }
        if matches!(
            mat.base.alpha_mode,
            AlphaMode::Blend | AlphaMode::Premultiplied | AlphaMode::Add | AlphaMode::Multiply
        ) {
            excluded.push((m, Excluded::Blend));
            continue;
        }
        let patterns: Vec<[bool; 3]> = families.iter().filter(|f| f.0 == m).map(|f| f.2).collect();
        // A family with a LOD1 mesh draws each LOD's range with that LOD's mesh or not at all
        // (`scene::lod_cover`), so slots like it merge whatever LOD0 or LOD2 they lack: the merged
        // LOD mesh holds the members present there and draws each range as they did. A family
        // without LOD1 draws its LOD0 mesh through the LOD1 range, and merges only with families
        // of the same pattern.
        let lod_key = if patterns.iter().all(|p| p[1]) {
            "chain".to_owned()
        } else if patterns.iter().all(|p| *p == patterns[0]) {
            format!("{:?}", patterns[0])
        } else {
            excluded.push((m, Excluded::LodPattern));
            continue;
        };
        let layouts: Vec<String> = parts
            .iter()
            .filter(|p| p.material.id() == m)
            .filter_map(|p| mesh_of(p).map(layout_key))
            .collect();
        if layouts.iter().any(|l| *l != layouts[0]) {
            excluded.push((m, Excluded::Layout));
            continue;
        }
        let key = format!("{}|{lod_key}|{}", material_key(mat), layouts[0]);
        match keyed.iter_mut().find(|(k, _)| *k == key) {
            Some((_, ms)) => ms.push(m),
            None => keyed.push((key, vec![m])),
        }
    }
    let mut groups = vec![];
    for (_, members) in keyed {
        if members.len() < 2 {
            excluded.extend(members.iter().map(|&m| (m, Excluded::Alone)));
            continue;
        }
        let roughness = |m: &AssetId<KitMaterial>| {
            materials
                .get(*m)
                .map_or(1.0, |x| x.base.perceptual_roughness)
        };
        let mut values: Vec<u32> = members.iter().map(|m| roughness(m).to_bits()).collect();
        values.sort_unstable();
        values.dedup();
        let vary = values.len() > 1;
        let colors: HashMap<AssetId<KitMaterial>, [f32; 4]> = members
            .iter()
            .filter_map(|&m| materials.get(m).map(|x| (m, slot_color(x, vary))))
            .collect();
        let mut merged: [Option<Handle<Mesh>>; 3] = [None, None, None];
        for (lod, slot) in merged.iter_mut().enumerate() {
            // Member primitives of this LOD in family order: one draw keeps their draw order.
            let mut prims: Vec<(usize, &KitPart)> = parts
                .iter()
                .zip(&part_family)
                .filter(|(p, _)| p.lod == lod && members.contains(&p.material.id()))
                .filter_map(|(p, f)| f.map(|f| (f, p)))
                .collect();
            prims.sort_by_key(|(f, _)| *f);
            let mut out: Option<Mesh> = None;
            for (_, p) in prims {
                let Some(src) = meshes.get(&p.mesh) else {
                    continue;
                };
                let c = coloured(src, colors[&p.material.id()]);
                match out.as_mut() {
                    None => out = Some(c),
                    Some(o) => {
                        if let Err(e) = o.merge(&c) {
                            warn!("play: slot coalescing merge failed: {e}");
                        }
                    }
                }
            }
            *slot = out.map(|m| meshes.add(m));
        }
        let Some(first) = materials.get(members[0]) else {
            continue;
        };
        let mut material = first.clone();
        let a = material.base.base_color.alpha();
        material.base.base_color = Color::LinearRgba(LinearRgba::new(1.0, 1.0, 1.0, a));
        material.extension.params.flags |= FLAG_SLOT_COLORS;
        if vary {
            material.base.perceptual_roughness = 1.0;
            material.extension.params.flags |= FLAG_VERTEX_ROUGHNESS;
        }
        groups.push(CoalescedGroup {
            members,
            roughness_values: values.len(),
            meshes: merged,
            material: materials.add(material),
        });
    }
    (groups, excluded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};

    fn tri(x: f32) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; 3])
        .with_inserted_indices(Indices::U16(vec![0, 1, 2]))
    }

    fn mat(rgb: [f32; 3], roughness: f32) -> KitMaterial {
        KitMaterial {
            base: StandardMaterial {
                base_color: Color::linear_rgb(rgb[0], rgb[1], rgb[2]),
                perceptual_roughness: roughness,
                metallic: 0.0,
                ..default()
            },
            extension: default(),
        }
    }

    struct Kit {
        meshes: Assets<Mesh>,
        materials: Assets<KitMaterial>,
    }

    impl Kit {
        fn new() -> Self {
            Self {
                meshes: Assets::default(),
                materials: Assets::default(),
            }
        }

        fn part(&mut self, lod: usize, mesh: Mesh, material: &Handle<KitMaterial>) -> KitPart {
            KitPart {
                lod,
                mesh: self.meshes.add(mesh),
                material: material.clone(),
                slot: String::new(),
                rotor: None,
            }
        }
    }

    fn colors(m: &Mesh) -> Vec<[f32; 4]> {
        match m.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(v)) => v.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// Two coloured slots present at every LOD merge into one draw per LOD: the vertices and
    /// indices are conserved, each slot's vertices carry its colour, and differing roughness
    /// rides in COLOR_0 alpha under a white, roughness-1 material.
    #[test]
    fn coloured_slots_merge_with_their_colour_per_vertex() {
        let mut k = Kit::new();
        let red = k.materials.add(mat([0.8, 0.1, 0.1], 0.6));
        let blue = k.materials.add(mat([0.1, 0.2, 0.9], 0.9));
        let mut parts = vec![];
        for lod in 0..3 {
            parts.push(k.part(lod, tri(0.0), &red));
            parts.push(k.part(lod, tri(5.0), &blue));
        }
        let (groups, excluded) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        assert!(excluded.is_empty(), "{excluded:?}");
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.members, [red.id(), blue.id()]);
        for lod in 0..3 {
            let m = k.meshes.get(g.meshes[lod].as_ref().unwrap()).unwrap();
            assert_eq!(m.count_vertices(), 6);
            assert_eq!(m.indices().unwrap().len(), 6);
            let idx: Vec<usize> = m.indices().unwrap().iter().collect();
            assert_eq!(idx, [0, 1, 2, 3, 4, 5]);
            let c = colors(m);
            assert_eq!(&c[..3], &[[0.8, 0.1, 0.1, 0.6]; 3]);
            assert_eq!(&c[3..], &[[0.1, 0.2, 0.9, 0.9]; 3]);
        }
        let merged = k.materials.get(&g.material).unwrap();
        assert_eq!(
            merged.base.base_color.to_linear(),
            LinearRgba::new(1.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(merged.base.perceptual_roughness, 1.0);
        let flags = merged.extension.params.flags;
        assert_eq!(
            flags & (FLAG_SLOT_COLORS | FLAG_VERTEX_ROUGHNESS),
            FLAG_SLOT_COLORS | FLAG_VERTEX_ROUGHNESS
        );
    }

    /// Equal roughness stays in the material: alpha is 1 and no vertex roughness flag is set.
    #[test]
    fn equal_roughness_stays_in_the_material() {
        let mut k = Kit::new();
        let a = k.materials.add(mat([0.5, 0.5, 0.5], 0.7));
        let b = k.materials.add(mat([0.2, 0.3, 0.4], 0.7));
        let parts = vec![k.part(0, tri(0.0), &a), k.part(0, tri(2.0), &b)];
        let (groups, _) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        let g = &groups[0];
        let m = k.meshes.get(g.meshes[0].as_ref().unwrap()).unwrap();
        assert!(colors(m).iter().all(|c| c[3] == 1.0));
        let merged = k.materials.get(&g.material).unwrap();
        assert_eq!(merged.base.perceptual_roughness, 0.7);
        assert_eq!(merged.extension.params.flags & FLAG_VERTEX_ROUGHNESS, 0);
        assert!(g.meshes[1].is_none() && g.meshes[2].is_none());
    }

    /// Slots that differ in anything but colour and roughness, that miss a LOD the others have,
    /// or that are masked with a different roughness, keep their own draws.
    #[test]
    fn slots_that_differ_otherwise_stay_apart() {
        let mut k = Kit::new();
        let a = k.materials.add(mat([0.5, 0.5, 0.5], 0.7));
        let mut metal = mat([0.2, 0.3, 0.4], 0.7);
        metal.base.metallic = 0.5;
        let b = k.materials.add(metal);
        let c = k.materials.add(mat([0.9, 0.9, 0.1], 0.7));
        let masked = |rgb, r| {
            let mut m = mat(rgb, r);
            m.base.alpha_mode = AlphaMode::Mask(0.5);
            m
        };
        let d = k.materials.add(masked([0.1, 0.6, 0.1], 0.5));
        let e = k.materials.add(masked([0.1, 0.7, 0.1], 0.8));
        let mut parts = vec![
            k.part(0, tri(0.0), &a),
            k.part(0, tri(1.0), &b),
            k.part(0, tri(2.0), &c),
            k.part(0, tri(3.0), &d),
            k.part(0, tri(4.0), &e),
        ];
        // `a` has a LOD1 mesh, `c` does not.
        parts.push(k.part(1, tri(0.0), &a));
        let (groups, excluded) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        assert!(groups.is_empty(), "{groups:?}");
        let mut why: Vec<Excluded> = excluded.iter().map(|(_, w)| *w).collect();
        why.sort();
        assert_eq!(why, [Excluded::Alone; 5]);
    }

    /// Slots with a LOD1 mesh merge whatever LOD2 they lack: the merged LOD2 mesh holds only the
    /// members that have one, so the other slot still draws nothing past 400 m.
    #[test]
    fn slots_with_lod1_merge_across_a_missing_lod2() {
        let mut k = Kit::new();
        let a = k.materials.add(mat([0.5, 0.5, 0.5], 0.7));
        let b = k.materials.add(mat([0.2, 0.3, 0.4], 0.7));
        let mut parts = vec![];
        for lod in 0..3 {
            parts.push(k.part(lod, tri(0.0), &a));
            if lod < 2 {
                parts.push(k.part(lod, tri(5.0), &b));
            }
        }
        let (groups, excluded) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        assert!(excluded.is_empty(), "{excluded:?}");
        let g = &groups[0];
        let verts = |l: usize| {
            k.meshes
                .get(g.meshes[l].as_ref().unwrap())
                .unwrap()
                .count_vertices()
        };
        assert_eq!([verts(0), verts(1), verts(2)], [6, 6, 3]);
    }

    /// A piece with a rotor or a mesh that already carries COLOR_0 is not coalesced at all.
    #[test]
    fn rotor_and_vertex_colour_pieces_are_left_alone() {
        let mut k = Kit::new();
        let a = k.materials.add(mat([0.5, 0.5, 0.5], 0.7));
        let b = k.materials.add(mat([0.2, 0.3, 0.4], 0.7));
        let mut parts = vec![k.part(0, tri(0.0), &a), k.part(0, tri(2.0), &b)];
        let mut rotor = k.part(0, tri(4.0), &a);
        rotor.rotor = Some(Transform::IDENTITY);
        parts.push(rotor);
        let (groups, excluded) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        assert!(groups.is_empty());
        assert!(excluded.iter().all(|(_, w)| *w == Excluded::Rotor));

        let fan = coloured(&tri(0.0), [1.0, 1.0, 1.0, 0.5]);
        let parts = vec![k.part(0, fan, &a), k.part(0, tri(2.0), &b)];
        let (groups, excluded) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        assert!(groups.is_empty());
        assert!(excluded.iter().all(|(_, w)| *w == Excluded::VertexColors));
    }

    /// The merged mesh holds its primitives in family order (first appearance), the order the
    /// separate draws were binned in: a slot's second primitive after the other slot's first.
    #[test]
    fn merged_mesh_keeps_family_order() {
        let mut k = Kit::new();
        let a = k.materials.add(mat([0.5, 0.5, 0.5], 0.7));
        let b = k.materials.add(mat([0.2, 0.3, 0.4], 0.7));
        let parts = vec![
            k.part(0, tri(0.0), &a),
            k.part(0, tri(10.0), &b),
            k.part(0, tri(20.0), &a),
        ];
        let (groups, _) = coalesce_piece(&parts, &mut k.meshes, &mut k.materials);
        let m = k.meshes.get(groups[0].meshes[0].as_ref().unwrap()).unwrap();
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!();
        };
        let xs: Vec<f32> = p.iter().step_by(3).map(|v| v[0]).collect();
        assert_eq!(xs, [0.0, 10.0, 20.0]);
    }
}
