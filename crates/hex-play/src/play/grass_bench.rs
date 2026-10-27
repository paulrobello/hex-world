//! Native timing of one tile's grass build (`grass::layout` and `GrassInstances::new`, the span
//! `state().grass.last_build_ms` times) on every catalog tile that grows grass, with its real LOD0
//! ground and scatter exclusions. Run with
//! `cargo test --release -p hex-play grass_bench -- --ignored --nocapture`.
use std::{collections::HashMap, time::Instant};

use bevy::prelude::*;
use hex_core::{Axial, TileCatalog, cell_seed};

use super::{
    grass::{Clear, GrassInstances, GrassLayout, exclusions, grass_slots, layout},
    scatter::{self, Surface},
};
use crate::{kit_scene::MANIFEST, walk::tests::mesh_tris};

/// One tile build's inputs, as `grass::sync` gathers them before it starts the clock.
struct Case {
    name: String,
    seed: u64,
    surface: Surface,
    slots: HashMap<String, [f32; 3]>,
    excluded: Vec<Clear>,
}

/// Every catalog tile with grass ground at three cells of a world of seed 1.
fn cases() -> Vec<Case> {
    let c = TileCatalog::builtin().unwrap();
    let m = hex_assets::parse_manifest(MANIFEST).unwrap();
    let mut out = vec![];
    for def in c.tiles() {
        let slots = grass_slots(&m, def, &c);
        if slots.is_empty() {
            continue;
        }
        for cell in [Axial::new(0, 0), Axial::new(3, -1), Axial::new(-5, 2)] {
            let seed = cell_seed(1, cell);
            let Some(prepared) = scatter::prepare(def, &c, &mut |id| mesh_tris(id, "lod0")) else {
                continue;
            };
            let pieces: Vec<String> = def
                .props
                .scatter
                .iter()
                .map(|r| r.piece.as_str().to_owned())
                .collect();
            let instances = if pieces.is_empty() {
                vec![]
            } else {
                scatter::layout(def, seed, &prepared).instances
            };
            let excluded = exclusions(def, cell, &instances, &pieces);
            out.push(Case {
                name: format!("{}@{},{}", def.id, cell.q, cell.r),
                seed,
                surface: prepared.surface,
                slots: slots.clone(),
                excluded,
            });
        }
    }
    out
}

/// FNV-1a over a build's every output bit: the layout's fingerprint and area, and the GPU records,
/// their order and bounds.
fn digest(l: &GrassLayout, i: Option<&GrassInstances>) -> u64 {
    let mut h = l.fingerprint() ^ u64::from(l.area_m2.to_bits());
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01B3);
        }
    };
    for b in &l.blades {
        for c in b.color {
            eat(&c.to_bits().to_le_bytes());
        }
    }
    if let Some(i) = i {
        eat(bytemuck::cast_slice(&i.blades));
        for v in [i.aabb.center, i.aabb.half_extents] {
            for x in v.to_array() {
                eat(&x.to_bits().to_le_bytes());
            }
        }
    }
    h
}

/// The build as of 041ad32, verbatim: the old arm of `grass_ab`, kept so the native A/B in
/// `docs/perf/grass-build/README.md` reruns in one command.
mod reference {
    use std::collections::HashMap;

    use bevy::prelude::*;
    use hex_core::HEX_RADIUS;

    use super::super::{
        APOTHEM,
        grass::{
            Blade, Clear, DENSITY, GRASS_ALBEDO_MEAN, GpuBlade, GrassLayout, LUSH, WATER_MARGIN,
            jitter, style,
        },
        scatter::{self, Rng, Surface},
    };

    const SALT: u64 = 0x6A55_B1AD_E5C0_FFEE;
    const SCALE: [f32; 2] = [0.7, 1.4];
    const HUE_JITTER: f32 = 0.04;
    const LIGHT_JITTER: f32 = 0.05;

    fn lerp(r: [f32; 2], t: f32) -> f32 {
        r[0] + (r[1] - r[0]) * t
    }

    pub fn layout(
        seed: u64,
        surface: &Surface,
        slots: &HashMap<String, [f32; 3]>,
        excluded: &[Clear],
    ) -> GrassLayout {
        let mut rng = Rng(seed ^ SALT);
        let step = 1.0 / (DENSITY * LUSH).sqrt();
        let n = (2.0 * HEX_RADIUS / step).ceil() as i32;
        let water = crate::water::tiles::WATER_Y + WATER_MARGIN;
        let mut out = GrassLayout::default();
        let mut ground = 0u32;
        for j in 0..n {
            for i in 0..n {
                let r = [(); 11].map(|_| rng.unit());
                let p = Vec2::new(
                    -HEX_RADIUS + (i as f32 + r[0]) * step,
                    -HEX_RADIUS + (j as f32 + r[1]) * step,
                );
                if scatter::hex_reach(p) >= APOTHEM || excluded.iter().any(|c| c.contains(p)) {
                    continue;
                }
                let Some((y, slot)) = surface.at(p.x, p.y) else {
                    continue;
                };
                let Some(tint) = slots.get(slot) else {
                    continue;
                };
                if y < water {
                    continue;
                }
                ground += 1;
                let st = style(slot);
                if r[2] >= st.density {
                    continue;
                }
                let scale = lerp(SCALE, r[10]);
                let base = [0, 1, 2].map(|k| tint[k] * GRASS_ALBEDO_MEAN[k]);
                out.blades.push(Blade {
                    root: Vec3::new(p.x, y, p.y),
                    yaw_deg: ((r[3] * 360.0) as u16).min(359),
                    rank: 0.9 * r[4],
                    height: lerp(st.height, r[5]) * scale,
                    width: lerp(st.width, r[8]) * scale,
                    color: jitter(
                        base,
                        HUE_JITTER * (2.0 * r[6] - 1.0),
                        LIGHT_JITTER * (2.0 * r[7] - 1.0),
                    ),
                    keep: r[9],
                });
            }
        }
        out.area_m2 = ground as f32 * step * step;
        out
    }

    /// `GrassInstances::new`'s records and keeps.
    pub fn instances(layout: &GrassLayout) -> (Vec<GpuBlade>, Vec<f32>) {
        let mut order: Vec<&Blade> = layout.blades.iter().collect();
        order.sort_by(|a, b| a.keep.total_cmp(&b.keep));
        let unorm = |c: f32| (c.clamp(0.0, 1.0) * 65535.0).round() as u16;
        let blades = order
            .iter()
            .map(|b| GpuBlade {
                root: b.root.to_array(),
                yaw_rank: f32::from(b.yaw_deg) + b.rank,
                size: [b.height, b.width],
                color: [unorm(b.color[0]), unorm(b.color[1]), unorm(b.color[2]), 0],
            })
            .collect();
        (blades, order.iter().map(|b| b.keep).collect())
    }
}

/// The A/B: per case, old and new builds timed alternately in every round, with equal output.
#[test]
#[ignore = "timing benchmark: run with --release -- --ignored --nocapture"]
fn grass_ab() {
    const ROUNDS: usize = 41;
    let cases = cases();
    let mut old = vec![vec![]; cases.len()];
    let mut new = vec![vec![]; cases.len()];
    for round in 0..ROUNDS {
        for (k, c) in cases.iter().enumerate() {
            let run_old = |t: &mut Vec<f64>| {
                let a = Instant::now();
                let l = reference::layout(c.seed, &c.surface, &c.slots, &c.excluded);
                let i = reference::instances(&l);
                t.push(a.elapsed().as_secs_f64() * 1000.0);
                (l, i)
            };
            let run_new = |t: &mut Vec<f64>| {
                let a = Instant::now();
                let out = build(c);
                t.push(a.elapsed().as_secs_f64() * 1000.0);
                out
            };
            if round % 2 == 0 {
                std::hint::black_box(run_old(&mut old[k]));
                std::hint::black_box(run_new(&mut new[k]));
            } else {
                std::hint::black_box(run_new(&mut new[k]));
                std::hint::black_box(run_old(&mut old[k]));
            }
        }
    }
    let (mut worst_old, mut worst_new, mut ratios) = (0f64, 0f64, vec![]);
    for (k, c) in cases.iter().enumerate() {
        let lo = reference::layout(c.seed, &c.surface, &c.slots, &c.excluded);
        let (bo, ko) = reference::instances(&lo);
        let (ln, inew) = build(c);
        assert_eq!(lo, ln, "{}: layout differs", c.name);
        if let Some(i) = &inew {
            assert_eq!(bo, i.blades, "{}: instances differ", c.name);
            assert_eq!(ko.len(), i.blades.len());
        } else {
            assert!(bo.is_empty());
        }
        let (mo, mn) = (median(&mut old[k]), median(&mut new[k]));
        worst_old = worst_old.max(mo);
        worst_new = worst_new.max(mn);
        ratios.push(mn / mo);
        println!(
            "{:<28} old {mo:.3} ms  new {mn:.3} ms  ratio {:.3}",
            c.name,
            mn / mo
        );
    }
    let r = median(&mut ratios);
    let meadow = cases
        .iter()
        .find(|c| c.name.starts_with("meadow@"))
        .unwrap();
    let l = build(meadow).0;
    let mut fp = vec![];
    for _ in 0..ROUNDS {
        let a = Instant::now();
        std::hint::black_box(l.fingerprint());
        fp.push(a.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "{} fingerprint ({} blades): median {:.3} ms",
        meadow.name,
        l.blades.len(),
        median(&mut fp)
    );
    println!(
        "{} cases, {ROUNDS} rounds: worst median old {worst_old:.3} ms new {worst_new:.3} ms, median ratio {r:.3}",
        cases.len()
    );
}

fn build(c: &Case) -> (GrassLayout, Option<GrassInstances>) {
    let l = layout(c.seed, &c.surface, &c.slots, &c.excluded);
    let i = GrassInstances::new(&l);
    (l, i)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Per tile: the median build time over `ROUNDS` rounds (cases interleaved round by round so host
/// load spreads over all of them), the blade count and the output digest.
#[test]
#[ignore = "timing benchmark: run with --release -- --ignored --nocapture"]
fn grass_bench() {
    const ROUNDS: usize = 41;
    let cases = cases();
    let mut times = vec![vec![]; cases.len()];
    for _ in 0..ROUNDS {
        for (k, c) in cases.iter().enumerate() {
            let t = Instant::now();
            let out = build(c);
            times[k].push(t.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(out);
        }
    }
    let mut all = 0u64;
    let mut worst: f64 = 0.0;
    let mut sum = 0.0;
    for (c, t) in cases.iter().zip(&mut times) {
        let (l, i) = build(c);
        let d = digest(&l, i.as_ref());
        all = (all ^ d).wrapping_mul(0x0100_0000_01B3);
        let m = median(t);
        worst = worst.max(m);
        sum += m;
        println!(
            "{:<28} {:>6} blades {:>5} clears  median {m:.3} ms  digest {d:016x}",
            c.name,
            l.blades.len(),
            c.excluded.len()
        );
    }
    println!(
        "{} builds, {ROUNDS} rounds: worst median {worst:.3} ms, mean median {:.3} ms, digest {all:016x}",
        cases.len(),
        sum / cases.len() as f64
    );
    // Once per definition and session, outside `last_build_ms`: its prepared ground (shared with
    // scatter) and, per tile, the report's fingerprint.
    let c = TileCatalog::builtin().unwrap();
    for id in ["meadow", "road_junction", "rail_road_crossing", "beach"] {
        let def = c.get(id).unwrap();
        let mut t = vec![];
        for _ in 0..11 {
            let a = Instant::now();
            std::hint::black_box(scatter::prepare(def, &c, &mut |id| mesh_tris(id, "lod0")));
            t.push(a.elapsed().as_secs_f64() * 1000.0);
        }
        println!("{id} prepare: median {:.3} ms", median(&mut t));
    }
    let meadow = cases
        .iter()
        .find(|c| c.name.starts_with("meadow@"))
        .unwrap();
    let l = build(meadow).0;
    let mut fp = vec![];
    for _ in 0..ROUNDS {
        let a = Instant::now();
        std::hint::black_box(l.fingerprint());
        fp.push(a.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "{} fingerprint ({} blades): median {:.3} ms",
        meadow.name,
        l.blades.len(),
        median(&mut fp)
    );
}
