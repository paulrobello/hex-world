# P0 Foundations Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the Hex World stack end to end. Bevy 0.20 renders a lit PBR hex world on WebGPU in the browser and natively. 2,000 tiles with the full post stack are measured against the frame budget. A custom compute pass and a custom material run inside Bevy's render graph. A meshopt GLB and a BC7 KTX2 texture load on wasm32. The build deploys to `https://hex-world.pardev.net`. The Bevy-vs-custom-wgpu decision (PRD D2) is recorded with measurements.

**Architecture:** This is a Cargo workspace. `hex-core` holds pure Rust hex math and mesh data, with no Bevy dependency. `hex-assets` holds the meshopt GLB rewrite and the texture-family selection (pure Rust core plus a Bevy asset-source adapter). `hex-render` holds the Bevy render plugins: the scene, the post stack, the custom material and compute boids. `hex-app` is the Bevy app, native and wasm entry, query-param and CLI flags, and the JS bridge. The web build uses Trunk with a plain HTML shell, copying `~/Repos/suck-it-up`. The TypeScript/Vite UI layer from PRD §12.1 is deferred to P2, because P0 has no player UI.

**Tech Stack:**
- Rust 1.99 stable, Bevy `=0.20.0` (wgpu 30), wasm-bindgen, Trunk 0.21.14 and wasm-opt.
- Node 24 with `@gltf-transform/*` for asset scripts. KTX-Software 4.4.2 `ktx` CLI.
- Playwright on GPU Chromium for the smoke and perf tests.
- GitHub Actions and GitHub Pages.

**Spec:** `PRD.md` (repo root), sections 3 (D2, D18, TC1–TC5), 9.1, 9.3, 10, 12, 13, 14, 15/P0 and 18.

## Global Constraints

- Bevy is pinned `=0.20.0` with `default-features = false`. Never enable `webgl2`, which conflicts with `webgpu` and is part of Bevy's `default_platform`.
- WebGPU is required. When there is no `navigator.gpu`, or `requestAdapter()` returns null, the page redirects to `web/unsupported.html`. There is no WebGL2 fallback.
- No wasm threads and no `SharedArrayBuffer`. GitHub Pages cannot set COOP/COEP (TC1).
- `hex-core` must not depend on Bevy. `hex-assets`' core decode functions must not depend on Bevy. Only its `bevy` module may.
- World scale: hex circumradius `R = 16.0` m, pointy-top, axial `(q, r)`, Y-up, meters (PRD §5.1).
- Wasm over the wire: at most 12 MB gzip. Initial download: at most 30 MB (PRD §10).
- Frame budget: 95th percentile frame time at or below 16.6 ms at 1440p, High tier, 2,000 tiles, on the reference Apple M-series machine (PRD §10).
- Profiles: dependencies use `opt-level = "z"` in `wasm-release`, and the workspace crates keep `opt-level = 3` (PRD §9.1).
- Browser tests launch Chromium with `channel: 'chromium'`, `headless: true`, `--use-angle=metal --enable-gpu --ignore-gpu-blocklist`. SwiftShader is used only when `PW_GPU=swiftshader`. Browser tests run locally only, never in CI.
- Commits never carry a co-author trailer. Never push, create the GitHub repo, or touch DNS without the owner's explicit go-ahead in chat (Task 9).
- Do not run `kanban` commands from sub-agents. The orchestrating session owns the board.
- Bevy 0.20 is new, and API names in this plan come from Bevy 0.17–0.19 conventions. Task 1 clones the `v0.20.0` source into `target/ref/bevy` (gitignored). Its `examples/` directory is authoritative. When a name in this plan doesn't compile, find the 0.20 equivalent there and note the rename in the commit message. Do not change the behavior.

## Review Focus

1. **A browser without WebGPU, or with WebGPU but no adapter.** The user sees the unsupported page, not a blank canvas or a wasm panic. Pinned in Task 5: the smoke test runs with `--disable-gpu` and expects the redirect.
2. **An unknown or malformed `?shot=` scenario name.** The app logs a warning and loads the default scenario. It does not panic. Pinned in Task 4 (`Scenario::parse` unit test) and Task 5 (smoke test with `?shot=nope`).
3. **A device that lacks BC texture compression** (iOS and some Android). Texture-family selection falls back to ASTC, then to PNG, and never requests a format the device can't sample. Pinned in Task 8 (`select_family` unit tests).
4. **A plain GLB with no meshopt extension, or a corrupt meshopt GLB.** The asset reader passes the plain file through byte-identical and returns an error, not a panic, on corrupt data. Pinned in Task 8 (`rewrite_glb` tests).
5. **A resized window and a high devicePixelRatio (Retina).** The canvas fills the viewport at device resolution, and TAA, SSAO and DOF survive the resize without validation errors. Pinned in Task 6 (Playwright resize test with no console errors).

---

## File Structure

```
Cargo.toml                         # workspace, profiles, lints
rust-toolchain.toml                # stable + wasm32 target
Makefile                           # fmt, lint, typecheck, test, checkall, checkall-ci, run, shot, build-web, web-size, web-smoke, perf-gate, assets-p0, fetch-tools
.gitignore
LICENSE                            # MIT
README.md
Trunk.toml
index.html                         # Trunk entry; WebGPU gate; window.hexWorld bridge
CNAME                              # hex-world.pardev.net
web/unsupported.html
crates/hex-core/src/lib.rs         # re-exports
crates/hex-core/src/axial.rs       # Axial coords, to_world, neighbors, spiral
crates/hex-core/src/prism.rs       # PrismMesh data (positions/normals/uvs/indices)
crates/hex-assets/src/lib.rs
crates/hex-assets/src/meshopt_glb.rs   # rewrite_glb(): decode EXT_meshopt_compression → plain GLB
crates/hex-assets/src/texture_family.rs# TextureFamily, select_family()
crates/hex-assets/src/bevy.rs          # MeshoptAssetReader + plugin (feature "bevy")
crates/hex-render/src/lib.rs       # HexRenderPlugin
crates/hex-render/src/scene.rs     # lights, env, camera, tile spawning
crates/hex-render/src/post.rs      # QualityTier → Bloom/TAA/SSAO/DOF components
crates/hex-render/src/diag.rs      # FrameStats ring buffer, p50/p95, log of render modes
crates/hex-render/src/boids.rs     # compute boids node + BoidMaterial
crates/hex-render/src/kit_test.rs  # loads the P0 test GLB + KTX2
assets/shaders/boids_compute.wgsl
assets/shaders/boid_material.wgsl
assets/kit/p0/                     # generated test assets (committed runtime derivatives)
crates/hex-app/src/main.rs         # native + wasm entry
crates/hex-app/src/args.rs         # Scenario, Flags parsing (CLI + query string)
crates/hex-app/src/bridge.rs       # wasm-bindgen exports
scripts/fetch-ktx.sh
scripts/assets/build-p0-assets.mjs
scripts/assets/package.json
e2e/package.json
e2e/smoke.mjs
e2e/perf-gate.mjs
.github/workflows/ci.yml
docs/decisions/0001-engine.md
docs/perf/p0-engine-gate.json      # written by perf-gate
```

---

### Task 1: Workspace scaffold and gates

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `Makefile`, `.gitignore`, `LICENSE`, `README.md`, `crates/hex-core/Cargo.toml`, `crates/hex-core/src/lib.rs`

**Interfaces:**
- Produces: the workspace layout, the `make checkall` / `make checkall-ci` targets every later task extends, and the `target/ref/bevy` reference checkout.

- [ ] **Step 1: Create the root `Cargo.toml`**

```toml
[workspace]
resolver = "3"
members = ["crates/hex-core"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
publish = false

[workspace.dependencies]
bevy = { version = "=0.20.0", default-features = false }

[workspace.lints.clippy]
type_complexity = "allow"
too_many_arguments = "allow"

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
debug = false

[profile.release]
panic = "abort"
lto = "thin"
codegen-units = 1
opt-level = 3

[profile.wasm-release]
inherits = "release"
opt-level = "z"
lto = "fat"
strip = "debuginfo"

[profile.wasm-release.package.hex-core]
opt-level = 3
```

- [ ] **Step 2: Create `rust-toolchain.toml`, `.gitignore`, `LICENSE`, `README.md`**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
targets = ["wasm32-unknown-unknown"]
components = ["rustfmt", "clippy"]
```

`.gitignore`:
```
/target
/dist
/tools
node_modules/
e2e/test-results/
*.blend1
.DS_Store
```

`LICENSE`: the standard MIT text with `Copyright (c) 2026 Paul Robello`.

`README.md`:
```markdown
# Hex World

A browser world-building game on hexagon tiles, built with Rust, Bevy and WebGPU.
Play: https://hex-world.pardev.net (requires a WebGPU browser).

See `PRD.md` for the product plan. Build with `make run` (native) or `make build-web`.
```

- [ ] **Step 3: Create the `hex-core` crate skeleton**

`crates/hex-core/Cargo.toml`:
```toml
[package]
name = "hex-core"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true
```

`crates/hex-core/src/lib.rs`:
```rust
//! Hex World core: hex grid math and mesh data. No engine dependencies.
```

- [ ] **Step 4: Create the `Makefile`**

```make
.PHONY: fmt fmt-check lint typecheck test checkall checkall-ci ref-bevy

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

typecheck:
	cargo check --workspace --all-targets
	cargo check -p hex-core --target wasm32-unknown-unknown

test:
	cargo test --workspace

checkall: fmt-check lint typecheck test

checkall-ci: checkall

ref-bevy:
	@test -d target/ref/bevy || git clone --depth 1 --branch v0.20.0 https://github.com/bevyengine/bevy target/ref/bevy
```

Task 4 changes the wasm line to `cargo check -p hex-app --target wasm32-unknown-unknown`, which covers every crate it depends on. Later tasks extend `checkall-ci`.

- [ ] **Step 5: Run the gate and fetch the reference**

Run: `make checkall && make ref-bevy && ls target/ref/bevy/examples/3d | head`
Expected: the gate passes (0 tests), and the example list prints, including `ssao.rs`, `anti_aliasing.rs`, `bloom_3d.rs`, `depth_of_field.rs` and `shadow_caster_receiver.rs`.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml rust-toolchain.toml Makefile .gitignore LICENSE README.md crates/hex-core
git commit -m "chore: scaffold Rust workspace, gates, and MIT license"
```

---

### Task 2: Axial hex grid math (`hex-core`)

**Files:**
- Create: `crates/hex-core/src/axial.rs`
- Modify: `crates/hex-core/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `axial.rs`

**Interfaces:**
- Produces:
  - `pub const HEX_RADIUS: f32 = 16.0;`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] pub struct Axial { pub q: i32, pub r: i32 }`
  - `impl Axial { pub const ORIGIN: Axial; pub fn new(q: i32, r: i32) -> Self; pub fn to_world(self, radius: f32) -> [f32; 2] /* (x, z) */; pub fn neighbor(self, edge: u8) -> Axial; pub fn distance(self, other: Axial) -> i32 }`
  - `pub fn spiral(count: usize) -> Vec<Axial>`: cells in ring order from the origin, exactly `count` long.
  - `pub fn ring_capacity(radius: u32) -> usize`: `1 + 3n(n+1)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_maps_to_world_origin() {
        assert_eq!(Axial::ORIGIN.to_world(HEX_RADIUS), [0.0, 0.0]);
    }

    #[test]
    fn east_neighbor_is_flat_to_flat_away() {
        // Pointy-top: edge 0 faces east; centers are sqrt(3)*R apart.
        let [x, z] = Axial::ORIGIN.neighbor(0).to_world(HEX_RADIUS);
        assert!((x - 3f32.sqrt() * HEX_RADIUS).abs() < 1e-4);
        assert!(z.abs() < 1e-4);
    }

    #[test]
    fn all_six_neighbors_are_distance_one_and_distinct() {
        let n: Vec<Axial> = (0..6).map(|e| Axial::ORIGIN.neighbor(e)).collect();
        for a in &n {
            assert_eq!(Axial::ORIGIN.distance(*a), 1);
        }
        let set: std::collections::HashSet<_> = n.iter().collect();
        assert_eq!(set.len(), 6);
    }

    #[test]
    fn neighbor_edges_are_clockwise_from_east() {
        // Edge 1 is 60° clockwise from east when viewed from +Y looking down (south-east, +z).
        let [_, z] = Axial::ORIGIN.neighbor(1).to_world(HEX_RADIUS);
        assert!(z > 0.0);
    }

    #[test]
    fn opposite_edges_cancel() {
        for e in 0..3u8 {
            let a = Axial::ORIGIN.neighbor(e).neighbor(e + 3);
            assert_eq!(a, Axial::ORIGIN);
        }
    }

    #[test]
    fn ring_capacity_matches_formula() {
        assert_eq!(ring_capacity(0), 1);
        assert_eq!(ring_capacity(1), 7);
        assert_eq!(ring_capacity(25), 1951);
        assert_eq!(ring_capacity(26), 2107);
    }

    #[test]
    fn spiral_is_exact_length_unique_and_ring_ordered() {
        let cells = spiral(2000);
        assert_eq!(cells.len(), 2000);
        let set: std::collections::HashSet<_> = cells.iter().collect();
        assert_eq!(set.len(), 2000);
        assert_eq!(cells[0], Axial::ORIGIN);
        let mut last = 0;
        for c in &cells {
            let d = Axial::ORIGIN.distance(*c);
            assert!(d >= last, "spiral must never move inward");
            last = d;
        }
        assert_eq!(Axial::ORIGIN.distance(cells[1999]), 26);
    }

    #[test]
    fn spiral_zero_is_empty() {
        assert!(spiral(0).is_empty());
    }
}
```

Add `pub mod axial; pub use axial::*;` to `lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p hex-core`
Expected: compile errors (`Axial` not found).

- [ ] **Step 3: Write the implementation (above the tests in `axial.rs`)**

```rust
//! Pointy-top axial hex coordinates. Edge 0 faces east (+x); edges run clockwise viewed from +Y.

pub const HEX_RADIUS: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Axial {
    pub q: i32,
    pub r: i32,
}

/// Axial offsets for edges 0..6: E, SE, SW, W, NW, NE (+z is "south" on screen).
const DIRS: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

impl Axial {
    pub const ORIGIN: Axial = Axial { q: 0, r: 0 };

    pub fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Center of the cell on the XZ plane, in meters.
    pub fn to_world(self, radius: f32) -> [f32; 2] {
        let q = self.q as f32;
        let r = self.r as f32;
        [radius * 3f32.sqrt() * (q + r / 2.0), radius * 1.5 * r]
    }

    pub fn neighbor(self, edge: u8) -> Axial {
        let (dq, dr) = DIRS[(edge % 6) as usize];
        Axial::new(self.q + dq, self.r + dr)
    }

    pub fn distance(self, other: Axial) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        (dq.abs() + dr.abs() + (dq + dr).abs()) / 2
    }
}

pub fn ring_capacity(radius: u32) -> usize {
    let n = radius as usize;
    1 + 3 * n * (n + 1)
}

/// Cells in ring order (center, ring 1, ring 2, …), truncated to `count`.
pub fn spiral(count: usize) -> Vec<Axial> {
    let mut out = Vec::with_capacity(count);
    if count == 0 {
        return out;
    }
    out.push(Axial::ORIGIN);
    let mut radius = 1;
    while out.len() < count {
        // Start at the cell `radius` steps along edge 4 (NW), then walk each side.
        let mut cell = Axial::new(DIRS[4].0 * radius, DIRS[4].1 * radius);
        for side in 0..6u8 {
            for _ in 0..radius {
                if out.len() == count {
                    return out;
                }
                out.push(cell);
                cell = cell.neighbor(side);
            }
        }
        radius += 1;
    }
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p hex-core`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/hex-core
git commit -m "feat(core): axial hex coordinates, neighbors, distance, spiral"
```

---

### Task 3: Hex prism mesh data (`hex-core`)

**Files:**
- Create: `crates/hex-core/src/prism.rs`
- Modify: `crates/hex-core/src/lib.rs`

**Interfaces:**
- Consumes: `HEX_RADIUS` from Task 2.
- Produces: `pub struct PrismMesh { pub positions: Vec<[f32; 3]>, pub normals: Vec<[f32; 3]>, pub uvs: Vec<[f32; 2]>, pub indices: Vec<u32> }` and `pub fn hex_prism(radius: f32, height: f32) -> PrismMesh`. The top cap is at `y = height`, the skirt runs from `y = 0` to `height`, there is no bottom cap, and faces are flat-shaded with counter-clockwise front faces viewed from outside.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tri_normal(m: &PrismMesh, t: usize) -> [f32; 3] {
        let [a, b, c] = [0, 1, 2].map(|k| m.positions[m.indices[t * 3 + k] as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    }

    #[test]
    fn counts_are_top_fan_plus_six_quads() {
        let m = hex_prism(16.0, 4.0);
        assert_eq!(m.positions.len(), 7 + 6 * 4);
        assert_eq!(m.indices.len(), 6 * 3 + 6 * 6);
        assert_eq!(m.normals.len(), m.positions.len());
        assert_eq!(m.uvs.len(), m.positions.len());
    }

    #[test]
    fn top_cap_is_flat_at_height_and_faces_up() {
        let m = hex_prism(16.0, 4.0);
        for i in 0..7 {
            assert_eq!(m.positions[i][1], 4.0);
            assert_eq!(m.normals[i], [0.0, 1.0, 0.0]);
        }
        for t in 0..6 {
            assert!(tri_normal(&m, t)[1] > 0.0, "top tri {t} must wind CCW from above");
        }
    }

    #[test]
    fn corners_lie_on_circumradius() {
        let m = hex_prism(16.0, 4.0);
        for p in &m.positions[1..7] {
            let d = (p[0] * p[0] + p[2] * p[2]).sqrt();
            assert!((d - 16.0).abs() < 1e-4);
        }
    }

    #[test]
    fn side_faces_point_outward() {
        let m = hex_prism(16.0, 4.0);
        for t in 6..18 {
            let n = tri_normal(&m, t);
            let i = m.indices[t * 3] as usize;
            let p = m.positions[i];
            assert!(n[0] * p[0] + n[2] * p[2] > 0.0, "side tri {t} must face outward");
            assert!(n[1].abs() < 1e-4);
        }
    }

    #[test]
    fn indices_in_range() {
        let m = hex_prism(16.0, 4.0);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()));
    }
}
```

Add `pub mod prism; pub use prism::*;` to `lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p hex-core prism`
Expected: compile error (`hex_prism` not found).

- [ ] **Step 3: Write the implementation**

```rust
//! Procedural hex column used as the P0 placeholder tile.

pub struct PrismMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

/// Pointy-top corner `i` (0..6) at angle 30° + 60°·i, clockwise from +x viewed from +Y.
fn corner(radius: f32, i: usize) -> [f32; 2] {
    let a = (30.0 + 60.0 * i as f32).to_radians();
    [radius * a.cos(), radius * a.sin()]
}

pub fn hex_prism(radius: f32, height: f32) -> PrismMesh {
    let mut m = PrismMesh { positions: vec![], normals: vec![], uvs: vec![], indices: vec![] };
    let up = [0.0, 1.0, 0.0];

    m.positions.push([0.0, height, 0.0]);
    m.normals.push(up);
    m.uvs.push([0.5, 0.5]);
    for i in 0..6 {
        let [x, z] = corner(radius, i);
        m.positions.push([x, height, z]);
        m.normals.push(up);
        m.uvs.push([0.5 + x / (2.0 * radius), 0.5 + z / (2.0 * radius)]);
    }
    // +z increases with corner index, so corners run clockwise viewed from +Y;
    // CCW-from-above order is (center, next, current).
    for i in 0..6u32 {
        let cur = 1 + i;
        let next = 1 + (i + 1) % 6;
        m.indices.extend_from_slice(&[0, next, cur]);
    }

    for i in 0..6 {
        let [x0, z0] = corner(radius, i);
        let [x1, z1] = corner(radius, (i + 1) % 6);
        let mid = [(x0 + x1) / 2.0, (z0 + z1) / 2.0];
        let len = (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
        let n = [mid[0] / len, 0.0, mid[1] / len];
        let base = m.positions.len() as u32;
        for (x, y, z, u, v) in [
            (x0, 0.0, z0, 0.0, 1.0),
            (x1, 0.0, z1, 1.0, 1.0),
            (x1, height, z1, 1.0, 0.0),
            (x0, height, z0, 0.0, 0.0),
        ] {
            m.positions.push([x, y, z]);
            m.normals.push(n);
            m.uvs.push([u, v]);
        }
        m.indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    m
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p hex-core`
Expected: all pass. If `side_faces_point_outward` fails, swap the winding of the two side triangles to `[base, base+1, base+2, base, base+2, base+3]`. The test is the authority on winding.

- [ ] **Step 5: Commit**

```bash
git add crates/hex-core
git commit -m "feat(core): procedural hex prism mesh data"
```

---

### Task 4: Native Bevy app with a lit PBR hex and the scenario/shot flags

**Files:**
- Create: `crates/hex-render/Cargo.toml`, `crates/hex-render/src/lib.rs`, `crates/hex-render/src/scene.rs`, `crates/hex-app/Cargo.toml`, `crates/hex-app/src/main.rs`, `crates/hex-app/src/args.rs`
- Modify: root `Cargo.toml` (members, `wasm-release` package overrides), `Makefile` (run, shot, typecheck wasm line)

**Interfaces:**
- Consumes: `hex_core::{hex_prism, spiral, Axial, HEX_RADIUS}`.
- Produces:
  - `hex_render::HexRenderPlugin { pub scenario: Scenario }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum Scenario { #[default] Single, Gate2000 }` (defined in `hex-render`, re-exported by `hex-app`)
  - `impl Scenario { pub fn parse(s: &str) -> Option<Scenario>; pub fn tile_count(self) -> usize }`
  - `hex_app::args::Flags { scenario: Scenario, shot: Option<String>, exit_after_frames: Option<u32>, diag: bool }` and `fn flags_from_pairs(pairs: &[(String, String)]) -> (Flags, Vec<String>)`, which returns the flags plus warnings.
  - A `ShotRequest(Option<String>)` resource. When it is set, after 120 frames the app saves a screenshot to that path (native) and sets the ready flag.

- [ ] **Step 1: Write `crates/hex-render/Cargo.toml` with the confirmed Bevy 0.20 features**

These feature sets were checked against `bevy-0.20.0/Cargo.toml` (`3d_bevy_render` pulls in PBR, core pipeline, anti-aliasing, post-processing, glTF, `ktx2`, `zstd_rust` and `tonemapping_luts`, and does not pull in `webgl2`):

```toml
[package]
name = "hex-render"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
hex-core = { path = "../hex-core" }
bevy = { workspace = true, features = [
    "3d_bevy_render",
    "default_app",
    "std",
    "bevy_winit",
    "multi_threaded",
    "default_font",
    "bluenoise_texture",
] }

[target.'cfg(target_os = "linux")'.dependencies]
bevy = { workspace = true, features = ["x11", "wayland"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
bevy = { workspace = true, features = ["web", "webgpu"] }

[lints]
workspace = true
```

- [ ] **Step 2: Write the failing `Scenario` tests in `hex-render/src/lib.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_and_rejects_unknown() {
        assert_eq!(Scenario::parse("single"), Some(Scenario::Single));
        assert_eq!(Scenario::parse("gate2000"), Some(Scenario::Gate2000));
        assert_eq!(Scenario::parse("nope"), None);
        assert_eq!(Scenario::parse(""), None);
    }

    #[test]
    fn tile_counts() {
        assert_eq!(Scenario::Single.tile_count(), 1);
        assert_eq!(Scenario::Gate2000.tile_count(), 2000);
    }
}
```

Run: `cargo test -p hex-render` (after adding `"crates/hex-render"` to the workspace members).
Expected: compile error.

- [ ] **Step 3: Implement `lib.rs` and `scene.rs`**

`crates/hex-render/src/lib.rs`:
```rust
//! Hex World Bevy render plugins.
use bevy::prelude::*;

mod scene;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scenario {
    #[default]
    Single,
    Gate2000,
}

impl Scenario {
    pub fn parse(s: &str) -> Option<Scenario> {
        match s {
            "single" => Some(Scenario::Single),
            "gate2000" => Some(Scenario::Gate2000),
            _ => None,
        }
    }

    pub fn tile_count(self) -> usize {
        match self {
            Scenario::Single => 1,
            Scenario::Gate2000 => 2000,
        }
    }
}

#[derive(Resource, Clone, Copy)]
pub struct ActiveScenario(pub Scenario);

pub struct HexRenderPlugin {
    pub scenario: Scenario,
}

impl Plugin for HexRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ActiveScenario(self.scenario))
            .add_systems(Startup, scene::setup);
    }
}
```

`crates/hex-render/src/scene.rs`:
```rust
use bevy::{
    asset::RenderAssetUsages,
    light::CascadeShadowConfigBuilder,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use hex_core::{HEX_RADIUS, hex_prism, spiral};

use crate::ActiveScenario;

pub const TILE_HEIGHT: f32 = 4.0;

fn prism_mesh() -> Mesh {
    let p = hex_prism(HEX_RADIUS, TILE_HEIGHT);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs)
        .with_inserted_indices(Indices::U32(p.indices))
}

pub fn setup(
    mut commands: Commands,
    scenario: Res<ActiveScenario>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(prism_mesh());
    // Varied materials keep the gate honest: real scenes never share one material.
    let palette = [
        Color::srgb(0.32, 0.55, 0.24),
        Color::srgb(0.20, 0.42, 0.20),
        Color::srgb(0.78, 0.70, 0.48),
        Color::srgb(0.16, 0.38, 0.62),
    ];
    let mats: Vec<_> = palette
        .iter()
        .map(|c| {
            materials.add(StandardMaterial {
                base_color: *c,
                perceptual_roughness: 0.85,
                ..default()
            })
        })
        .collect();

    for (i, cell) in spiral(scenario.0.tile_count()).into_iter().enumerate() {
        let [x, z] = cell.to_world(HEX_RADIUS);
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(mats[i % mats.len()].clone()),
            Transform::from_xyz(x, 0.0, z),
        ));
    }

    commands.spawn((
        DirectionalLight { illuminance: 30_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(200.0, 400.0, 150.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: 2_000.0,
            first_cascade_far_bound: 120.0,
            ..default()
        }
        .build(),
    ));
    commands.insert_resource(GlobalAmbientLight { brightness: 400.0, ..default() });

    let dist = if scenario.0.tile_count() > 1 { 900.0 } else { 90.0 };
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, dist * 0.7, dist).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
```

If `GlobalAmbientLight`, `CascadeShadowConfigBuilder`'s module path, or `RenderAssetUsages` have moved in 0.20, take the current names from `target/ref/bevy/examples/3d/lighting.rs` and `shadow_caster_receiver.rs`.

- [ ] **Step 4: Run the hex-render tests**

Run: `cargo test -p hex-render`
Expected: 2 passed.

- [ ] **Step 5: Write the failing flag-parsing tests in `hex-app/src/args.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn defaults() {
        let (f, w) = flags_from_pairs(&[]);
        assert_eq!(f.scenario, Scenario::Single);
        assert!(f.shot.is_none() && !f.diag && w.is_empty());
    }

    #[test]
    fn shot_selects_scenario() {
        let (f, w) = flags_from_pairs(&p(&[("shot", "gate2000"), ("diag", "1")]));
        assert_eq!(f.scenario, Scenario::Gate2000);
        assert_eq!(f.shot.as_deref(), Some("gate2000"));
        assert!(f.diag && w.is_empty());
    }

    #[test]
    fn unknown_scenario_warns_and_falls_back() {
        let (f, w) = flags_from_pairs(&p(&[("shot", "nope")]));
        assert_eq!(f.scenario, Scenario::Single);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("nope"));
    }

    #[test]
    fn scenario_key_without_shot() {
        let (f, _) = flags_from_pairs(&p(&[("scenario", "gate2000")]));
        assert_eq!(f.scenario, Scenario::Gate2000);
        assert!(f.shot.is_none());
    }

    #[test]
    fn exit_after_parses_or_warns() {
        let (f, w) = flags_from_pairs(&p(&[("exit-after", "300")]));
        assert_eq!(f.exit_after_frames, Some(300));
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("exit-after", "abc")]));
        assert_eq!(f.exit_after_frames, None);
        assert_eq!(w.len(), 1);
    }
}
```

Expected after Step 6 wiring: the tests fail to compile until Step 7.

- [ ] **Step 6: Create `crates/hex-app/Cargo.toml` and add it to the workspace**

```toml
[package]
name = "hex-app"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[[bin]]
name = "hex-world"
path = "src/main.rs"

[dependencies]
hex-core = { path = "../hex-core" }
hex-render = { path = "../hex-render" }
bevy = { workspace = true }

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["Window", "Location", "UrlSearchParams", "console"] }

[lints]
workspace = true
```

Root `Cargo.toml`: set `members = ["crates/*"]` and add:
```toml
[profile.wasm-release.package.hex-render]
opt-level = 3

[profile.wasm-release.package.hex-app]
opt-level = 3
```

- [ ] **Step 7: Implement `args.rs` and `main.rs`**

`crates/hex-app/src/args.rs`:
```rust
pub use hex_render::Scenario;

#[derive(Clone, Debug, Default)]
pub struct Flags {
    pub scenario: Scenario,
    pub shot: Option<String>,
    pub exit_after_frames: Option<u32>,
    pub diag: bool,
}

/// Shared by the native CLI (`--shot gate2000`) and the web query string (`?shot=gate2000`).
pub fn flags_from_pairs(pairs: &[(String, String)]) -> (Flags, Vec<String>) {
    let mut f = Flags::default();
    let mut warnings = vec![];
    for (k, v) in pairs {
        match k.as_str() {
            "shot" | "scenario" => {
                match Scenario::parse(v) {
                    Some(s) => f.scenario = s,
                    None => warnings.push(format!("unknown scenario '{v}', using default")),
                }
                if k == "shot" && Scenario::parse(v).is_some() {
                    f.shot = Some(v.clone());
                }
            }
            "diag" => f.diag = v != "0",
            "exit-after" => match v.parse() {
                Ok(n) => f.exit_after_frames = Some(n),
                Err(_) => warnings.push(format!("bad exit-after '{v}'")),
            },
            _ => {}
        }
    }
    (f, warnings)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_pairs() -> Vec<(String, String)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = vec![];
    let mut i = 0;
    while i < args.len() {
        if let Some(key) = args[i].strip_prefix("--") {
            let val = args.get(i + 1).filter(|v| !v.starts_with("--")).cloned();
            i += if val.is_some() { 2 } else { 1 };
            out.push((key.to_string(), val.unwrap_or_else(|| "1".into())));
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
pub fn web_pairs() -> Vec<(String, String)> {
    let Some(search) = web_sys::window().and_then(|w| w.location().search().ok()) else {
        return vec![];
    };
    let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) else {
        return vec![];
    };
    ["shot", "scenario", "diag", "exit-after"]
        .iter()
        .filter_map(|k| params.get(k).map(|v| (k.to_string(), v)))
        .collect()
}
```

In the tests module, keep the tests from Step 5 at the bottom of this file.

`crates/hex-app/src/main.rs`:
```rust
use bevy::{prelude::*, render::view::screenshot::{Screenshot, save_to_disk}};
use hex_render::HexRenderPlugin;

mod args;

#[derive(Resource)]
struct RunFlags(args::Flags);

#[derive(Resource, Default)]
struct FrameCount(u32);

const SHOT_FRAME: u32 = 120;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let pairs = args::native_pairs();
    #[cfg(target_arch = "wasm32")]
    let pairs = args::web_pairs();
    let (flags, warnings) = args::flags_from_pairs(&pairs);

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Hex World".into(),
            canvas: Some("#hex-world".into()),
            fit_canvas_to_parent: true,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(HexRenderPlugin { scenario: flags.scenario })
    .insert_resource(RunFlags(flags))
    .init_resource::<FrameCount>()
    .add_systems(Update, frame_hooks);

    for w in warnings {
        warn!("{w}");
    }
    app.run();
}

fn frame_hooks(
    mut commands: Commands,
    flags: Res<RunFlags>,
    mut frames: ResMut<FrameCount>,
    mut exit: MessageWriter<AppExit>,
) {
    frames.0 += 1;
    if frames.0 == SHOT_FRAME {
        if let Some(name) = &flags.0.shot {
            #[cfg(not(target_arch = "wasm32"))]
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("target/shots/{name}.png")));
            #[cfg(target_arch = "wasm32")]
            let _ = (&mut commands, name);
        }
    }
    if flags.0.exit_after_frames.is_some_and(|n| frames.0 >= n) {
        exit.write(AppExit::Success);
    }
}
```

If `MessageWriter<AppExit>` is named `EventWriter` in 0.20, take the current name from `target/ref/bevy/examples/app/` (search for `AppExit`). The wasm ready flag for `?shot=` is wired in Task 5.

- [ ] **Step 8: Add native run/shot targets to the `Makefile` and restore the wasm typecheck**

```make
run:
	cargo run -p hex-app --release -- $(ARGS)

shot:
	mkdir -p target/shots
	rm -f target/shots/single.png
	cargo run -p hex-app --release -- --shot single --exit-after 180
	@test -s target/shots/single.png && echo "wrote target/shots/single.png"
```

Replace the wasm line in `typecheck` with `cargo check -p hex-app --target wasm32-unknown-unknown`.

- [ ] **Step 9: Verify**

Run: `make checkall && make shot`
Expected: the gate passes, and `target/shots/single.png` exists. Open it with the Read tool: it must show a green hex column, lit from one side with a visible shadow side. A black or empty image fails this step.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Makefile crates/hex-render crates/hex-app
git commit -m "feat: native Bevy 0.20 app rendering a lit PBR hex, scenario and shot flags"
```

---

### Task 5: Web build, WebGPU gate, JS bridge and smoke test

**Files:**
- Create: `Trunk.toml`, `index.html`, `CNAME`, `web/unsupported.html`, `crates/hex-app/src/bridge.rs`, `e2e/package.json`, `e2e/smoke.mjs`
- Modify: `crates/hex-app/src/main.rs`, `Makefile`

**Interfaces:**
- Consumes: `Flags` and `FrameCount` from Task 4.
- Produces: the JS API `window.hexWorld = { ready(): boolean, state(): object, diag(): object }`, backed by the wasm exports `hex_world_ready() -> bool`, `hex_world_state_json() -> String` and `hex_world_diag_json() -> String`. Task 6 fills in `diag`. Also produces the make targets `build-web`, `web-size` and `web-smoke`.

- [ ] **Step 1: Create `Trunk.toml`, `CNAME` and `web/unsupported.html`**

`Trunk.toml`:
```toml
[build]
target = "index.html"
dist = "dist"

[serve]
addresses = ["127.0.0.1"]
port = 8094
open = false
```

Before using port 8094, check `~/.claude/used_ports.md`. If 8094 is taken, pick a free port, and record `hex-world` with the port there.

`CNAME`:
```
hex-world.pardev.net
```

`web/unsupported.html`: copy `~/Repos/suck-it-up/web/unsupported.html`. Replace "Suck It Up" with "Hex World", set the title to `Hex World needs WebGPU`, and add Safari 26 on iPhone/iPad to the browser list. Keep `<main id="unsupported">`, because the smoke test selects it.

- [ ] **Step 2: Create `index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <meta
      http-equiv="Content-Security-Policy"
      content="default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; connect-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; worker-src 'self' blob:"
    />
    <title>Hex World</title>
    <style>
      html, body { margin: 0; height: 100%; background: #1d2126; overflow: hidden; }
      canvas#hex-world { display: block; width: 100%; height: 100%; outline: none; }
      #diag { position: fixed; top: 8px; left: 8px; padding: 6px 8px; background: rgb(0 0 0 / 0.6);
              color: #e6e6e6; font: 12px/1.4 ui-monospace, monospace; white-space: pre; display: none; }
    </style>
    <script>
      // WebGPU gate before the wasm loads (PRD D2, TC4).
      (function () {
        function bail() { window.location.replace("web/unsupported.html"); }
        if (!("gpu" in navigator) || !navigator.gpu) return bail();
        navigator.gpu.requestAdapter().then(function (a) { if (!a) bail(); }).catch(bail);
      })();
    </script>
    <link data-trunk rel="rust" href="crates/hex-app/Cargo.toml" data-bin="hex-world" data-cargo-profile="wasm-release" data-wasm-opt="z" />
    <link data-trunk rel="copy-dir" href="web" />
    <link data-trunk rel="copy-dir" href="assets" />
    <link data-trunk rel="copy-file" href="CNAME" />
  </head>
  <body>
    <canvas id="hex-world"></canvas>
    <div id="diag"></div>
    <script>
      addEventListener("TrunkApplicationStarted", function () {
        var w = window.wasmBindings;
        if (!w) { console.error("hexWorld: wasmBindings missing"); return; }
        window.hexWorld = {
          ready: function () { return w.hex_world_ready(); },
          state: function () { return JSON.parse(w.hex_world_state_json()); },
          diag: function () { return JSON.parse(w.hex_world_diag_json()); },
        };
        if (new URLSearchParams(location.search).get("diag") === "1") {
          var el = document.getElementById("diag");
          el.style.display = "block";
          setInterval(function () { el.textContent = JSON.stringify(window.hexWorld.diag(), null, 1); }, 500);
        }
      });
    </script>
  </body>
</html>
```

The `assets` copy-dir needs the directory to exist. Create `assets/.gitkeep` now.

- [ ] **Step 3: Implement `bridge.rs`**

```rust
//! wasm-bindgen exports behind window.hexWorld. Shared state is written by Bevy systems.
use std::sync::Mutex;

pub static READY: Mutex<bool> = Mutex::new(false);
pub static STATE_JSON: Mutex<String> = Mutex::new(String::new());
pub static DIAG_JSON: Mutex<String> = Mutex::new(String::new());

#[cfg(target_arch = "wasm32")]
mod exports {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn hex_world_ready() -> bool {
        *super::READY.lock().unwrap()
    }

    #[wasm_bindgen]
    pub fn hex_world_state_json() -> String {
        let s = super::STATE_JSON.lock().unwrap();
        if s.is_empty() { "{}".into() } else { s.clone() }
    }

    #[wasm_bindgen]
    pub fn hex_world_diag_json() -> String {
        let s = super::DIAG_JSON.lock().unwrap();
        if s.is_empty() { "{}".into() } else { s.clone() }
    }
}
```

In `main.rs`, add `mod bridge;`. In `frame_hooks`, at `frames.0 == SHOT_FRAME`, set `*bridge::READY.lock().unwrap() = true;`. Every 30 frames, write the state JSON:
```rust
if frames.0 % 30 == 0 {
    *bridge::STATE_JSON.lock().unwrap() = format!(
        "{{\"scenario\":\"{:?}\",\"frames\":{}}}",
        flags.0.scenario, frames.0
    );
}
```
Remove the wasm-only `let _ = (&mut commands, name);` line, since the ready flag now covers the web shot. Playwright screenshots the canvas. Because `commands` is now used only natively, add `#[cfg_attr(target_arch = "wasm32", allow(unused_mut, unused_variables))]` on `frame_hooks`.

- [ ] **Step 4: Add the web targets to the `Makefile`**

```make
build-web:
	trunk build --release

web-size:
	@rm -rf dist
	@$(MAKE) --no-print-directory build-web
	@wasm=$$(ls dist/*.wasm); raw=$$(wc -c < $$wasm | tr -d ' '); gz=$$(gzip -9 -c $$wasm | wc -c | tr -d ' '); \
	echo "wasm raw=$$raw gzip=$$gz"; \
	test $$gz -le 12582912 || { echo "FAIL: wasm gzip $$gz > 12 MiB budget"; exit 1; }

web-smoke: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/smoke.mjs dist
```

Append `build-web web-size` to `checkall-ci`.

- [ ] **Step 5: Write the smoke test `e2e/smoke.mjs`**

`e2e/package.json`:
```json
{ "private": true, "type": "module", "dependencies": { "playwright": "1.56.1" } }
```
Use the newest Playwright version that `npm view playwright version` reports and that is at least two weeks old. Pin it exactly and commit `package-lock.json`.

`e2e/smoke.mjs`:
```js
// Local-only browser smoke test: GPU Chromium, WebGPU backend, gate redirect, bad scenario fallback.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";

const dist = process.argv[2] ?? "dist";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain" };
const server = createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  try {
    const body = await readFile(join(dist, path.endsWith("/") ? path + "index.html" : path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist"];
const fail = (m) => { console.error("FAIL:", m); process.exitCode = 1; };

async function withPage(args, fn) {
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: [...args, "--enable-unsafe-webgpu"] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  try { await fn(page, errors); } finally { await browser.close(); }
}

// 1. Default scenario becomes ready on the WebGPU backend.
await withPage(gpuArgs, async (page, errors) => {
  await page.goto(base);
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 60_000 });
  const backend = await page.evaluate(async () => (await navigator.gpu.requestAdapter())?.info?.vendor ?? "none");
  if (backend === "none") fail("no WebGPU adapter in GPU run");
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-single.png" });
});

// 2. Unknown scenario falls back to default without errors.
await withPage(gpuArgs, async (page, errors) => {
  await page.goto(base + "?shot=nope");
  await page.waitForFunction(() => window.hexWorld?.ready() !== undefined, null, { timeout: 60_000 });
  const state = await page.evaluate(() => new Promise((r) => setTimeout(() => r(window.hexWorld.state()), 3000)));
  if (state.scenario !== "Single") fail("bad scenario did not fall back: " + JSON.stringify(state));
  if (errors.some((e) => /panic|unreachable/i.test(e))) fail("panic on bad scenario");
});

// 3. No GPU → unsupported page.
await withPage(["--disable-gpu"], async (page) => {
  await page.goto(base);
  await page.waitForURL(/unsupported\.html$/, { timeout: 15_000 }).catch(() => fail("no redirect without GPU"));
  if (!(await page.locator("#unsupported").count())) fail("unsupported page content missing");
});

server.close();
console.log(process.exitCode ? "smoke FAILED" : "smoke OK");
```

Per the vault note on this gate, `--disable-features=WebGPU` still returns an adapter in Chrome, so `--disable-gpu` is the trigger.

- [ ] **Step 6: Verify**

Run: `make checkall && make web-size && make web-smoke`
Expected:
- The gate passes.
- `web-size` prints the wasm sizes under budget. Record them in the commit message.
- `smoke OK`.
- Open `e2e/test-results/smoke-single.png` with the Read tool: it must show the lit hex.

- [ ] **Step 7: Commit**

```bash
git add Trunk.toml index.html CNAME web assets/.gitkeep crates/hex-app e2e/package.json e2e/package-lock.json e2e/smoke.mjs Makefile
git commit -m "feat(web): Trunk WebGPU build, unsupported gate, hexWorld bridge, smoke test"
```

---

### Task 6: Engine-gate scene: real tile workload, full post stack, diagnostics, perf gate

**Files:**
- Create: `crates/hex-render/src/post.rs`, `crates/hex-render/src/diag.rs`, `crates/hex-render/src/workload.rs`, `e2e/perf-gate.mjs`
- Modify: `crates/hex-render/src/lib.rs`, `crates/hex-render/src/scene.rs`, `crates/hex-app/src/main.rs`, `Makefile`

**Interfaces:**
- Consumes: `Scenario::Gate2000`, the `bridge::DIAG_JSON` static (`hex-app`).
- Produces:
  - `pub enum QualityTier { Low, Medium, High, Ultra }`, with `High` as the default.
  - `pub struct FrameStats` with `fn push(&mut self, ms: f32)`, `fn p50(&self) -> f32` and `fn p95(&self) -> f32` over a 600-sample ring.
  - `#[derive(Resource)] pub struct DiagSnapshot { pub json: String }`, updated every 30 frames with `{"p50":..,"p95":..,"tiles":..,"ssao":bool,"gpu_preprocessing":"..","adapter":".."}`. `hex-app` copies it into `bridge::DIAG_JSON`.
  - `pub struct GateWorkload { pub tiles: usize, pub pieces_per_tile: usize, pub materials: usize, pub moving: usize, pub tris_lod0_per_tile: usize }` with `pub const GATE: GateWorkload` and `fn entities(&self) -> usize`. The gate scene spawns exactly this workload, and the perf JSON reports it.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum GateView { Strategic, Closeup }`, selected by `?view=closeup` (default `Strategic`).
  - `make perf-gate`, which writes `docs/perf/p0-engine-gate.json` with both views measured.

The gate must load Bevy with the real tile shape, not 2,000 bare prisms (owner rule: performance is measured at production-scale workload). PRD §6 and §10 define a tile as a base, 6 edge pieces, a center piece and a prop scatter, with LOD1/LOD2 at 50% and 15% of LOD0. The placeholder workload below copies those counts and triangle budgets. Agents come from PRD §7.2 (High tier: 300 vehicles; the 1,000 birds are the Task 7 boids).

| Piece | Count per tile | LOD0 tris | Placeholder mesh |
|---|---|---|---|
| Base | 1 | 1,000 | `hex_prism` with a subdivided cap (rings) |
| Edge pieces | 6 | 600 each | subdivided box strips along each edge |
| Center | 1 | 1,500 | icosphere (subdivision 3), flattened |
| Props | 12 | 800 each | icosphere (subdivision 2) and cylinders |
| Per tile | 20 entities | 15,700 | |

- 2,000 tiles × 20 = 40,000 static entities, plus 300 moving "vehicle" boxes driven along circles every frame.
- 40 distinct `StandardMaterial`s (base color, roughness and metallic varied), assigned by `(tile index × 7 + piece index) % 40`.
- Each piece has LOD0/LOD1/LOD2 meshes (decimated by lowering the subdivision level), switched with Bevy's `VisibilityRange`. Find its 0.20 form in `target/ref/bevy/examples/3d/visibility_range.rs`. LOD switch distances are 150 m and 400 m.
- Two camera views are measured: `Strategic` (whole world, 900 m out, mostly LOD2) and `Closeup` (street level, 40 m above tile `(3, 2)` looking across the world, mostly LOD0 with deep shadow cascades).

- [ ] **Step 0: Write the failing workload test, then implement `workload.rs`**

`crates/hex-render/src/workload.rs`:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateWorkload {
    pub tiles: usize,
    pub pieces_per_tile: usize,
    pub materials: usize,
    pub moving: usize,
    pub tris_lod0_per_tile: usize,
}

pub const GATE: GateWorkload = GateWorkload {
    tiles: 2000,
    pieces_per_tile: 20,
    materials: 40,
    moving: 300,
    tris_lod0_per_tile: 1000 + 6 * 600 + 1500 + 12 * 800,
};

impl GateWorkload {
    pub fn entities(&self) -> usize {
        self.tiles * self.pieces_per_tile + self.moving
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GateView {
    #[default]
    Strategic,
    Closeup,
}

impl GateView {
    pub fn parse(s: &str) -> Option<GateView> {
        match s {
            "strategic" => Some(GateView::Strategic),
            "closeup" => Some(GateView::Closeup),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_matches_prd_tile_shape() {
        assert_eq!(GATE.tiles, 2000);
        assert_eq!(GATE.pieces_per_tile, 1 + 6 + 1 + 12);
        assert_eq!(GATE.tris_lod0_per_tile, 15_700);
        assert_eq!(GATE.entities(), 40_300);
    }

    #[test]
    fn views_parse() {
        assert_eq!(GateView::parse("closeup"), Some(GateView::Closeup));
        assert_eq!(GateView::parse("x"), None);
    }
}
```

Run `cargo test -p hex-render workload` once before writing the implementation (expect a compile error), then once after (expect 2 passed).

For each piece type, add a mesh builder in `scene.rs` that produces the three LODs. In a test, assert that each builder's LOD0 triangle count is within ±10% of the table value, and that LOD1 and LOD2 are at or below 55% and 20% of LOD0. Count triangles as `indices.len() / 3`.

Then in `Scenario::Gate2000`, spawn the workload:
- Each tile gets its 20 pieces as child entities of a `Tile` entity at the tile's world position.
- Spawn 300 `Vehicle` entities, moved every frame by a system: `pos = center + (cos(t·ω), 0, sin(t·ω))·r`, using per-vehicle seeded ω and r.
- `?view=` selects the camera view, and `args.rs` gains a `view` key using `GateView::parse` (add `"view"` to the key list in `web_pairs`, and pass the view to `HexRenderPlugin` as a new `view: GateView` field). Unknown values warn and fall back, exactly like the scenario key. Add a `flags_from_pairs` test for `view=closeup` and `view=bogus`.

- [ ] **Step 1: Write the failing `FrameStats` tests in `diag.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero() {
        let s = FrameStats::default();
        assert_eq!(s.p50(), 0.0);
        assert_eq!(s.p95(), 0.0);
    }

    #[test]
    fn percentiles_over_known_samples() {
        let mut s = FrameStats::default();
        for i in 1..=100 {
            s.push(i as f32);
        }
        assert_eq!(s.p50(), 50.0);
        assert_eq!(s.p95(), 95.0);
    }

    #[test]
    fn ring_keeps_latest_600() {
        let mut s = FrameStats::default();
        for _ in 0..600 {
            s.push(100.0);
        }
        for _ in 0..600 {
            s.push(10.0);
        }
        assert_eq!(s.p95(), 10.0);
    }
}
```

Run: `cargo test -p hex-render diag`
Expected: compile error.

- [ ] **Step 2: Implement `diag.rs`**

```rust
use bevy::prelude::*;

const RING: usize = 600;

#[derive(Default)]
pub struct FrameStats {
    samples: Vec<f32>,
    next: usize,
}

impl FrameStats {
    pub fn push(&mut self, ms: f32) {
        if self.samples.len() < RING {
            self.samples.push(ms);
        } else {
            self.samples[self.next] = ms;
        }
        self.next = (self.next + 1) % RING;
    }

    fn percentile(&self, p: f32) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let idx = ((p / 100.0) * v.len() as f32).ceil() as usize;
        v[idx.clamp(1, v.len()) - 1]
    }

    pub fn p50(&self) -> f32 {
        self.percentile(50.0)
    }

    pub fn p95(&self) -> f32 {
        self.percentile(95.0)
    }
}

#[derive(Resource, Default)]
pub struct DiagSnapshot {
    pub json: String,
}

#[derive(Resource, Default)]
struct DiagState {
    stats: FrameStats,
    frames: u32,
}

/// Facts about the render setup that the engine gate must log (PRD P0).
#[derive(Resource, Default, Clone)]
pub struct RenderModes {
    pub ssao: bool,
    pub gpu_preprocessing: String,
    pub adapter: String,
}

pub struct DiagPlugin;

impl Plugin for DiagPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiagState>()
            .init_resource::<DiagSnapshot>()
            .init_resource::<RenderModes>()
            .add_systems(Update, sample);
    }
}

fn sample(
    time: Res<Time>,
    mut st: ResMut<DiagState>,
    mut snap: ResMut<DiagSnapshot>,
    modes: Res<RenderModes>,
    tiles: Query<(), With<crate::scene::Tile>>,
    meshes: Query<(), With<Mesh3d>>,
) {
    st.frames += 1;
    if st.frames > 60 {
        st.stats.push(time.delta_secs() * 1000.0);
    }
    if st.frames % 30 == 0 {
        snap.json = format!(
            "{{\"p50\":{:.2},\"p95\":{:.2},\"tiles\":{},\"entities\":{},\"ssao\":{},\"gpu_preprocessing\":\"{}\",\"adapter\":\"{}\",\"samples\":{}}}",
            st.stats.p50(), st.stats.p95(), tiles.iter().count(), meshes.iter().count(), modes.ssao,
            modes.gpu_preprocessing, modes.adapter.replace('"', "'"), st.stats.samples.len()
        );
    }
}
```

`RenderModes` is filled in from the render world in Step 4.

- [ ] **Step 3: Implement `post.rs` and attach it to the camera**

```rust
use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    pbr::ScreenSpaceAmbientOcclusion,
    post_process::{bloom::Bloom, dof::DepthOfField},
    prelude::*,
    render::view::Msaa,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QualityTier {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

/// Components for the camera at a tier. TAA requires MSAA off.
pub fn camera_post_bundle(tier: QualityTier) -> impl Bundle {
    let ssao = matches!(tier, QualityTier::High | QualityTier::Ultra);
    (
        Msaa::Off,
        TemporalAntiAliasing::default(),
        Bloom::NATURAL,
        DepthOfField { focal_distance: 900.0, aperture_f_stops: 8.0, ..default() },
        ssao.then(ScreenSpaceAmbientOcclusion::default),
    )
}
```

If `Option<Component>` is not a bundle in 0.20, insert SSAO conditionally with a separate `commands.entity(cam).insert(...)` call instead. Check the module paths (`bevy::anti_alias`, `bevy::post_process`) against `target/ref/bevy/examples/3d/anti_aliasing.rs`, `depth_of_field.rs`, `bloom_3d.rs` and `ssao.rs`. In those examples, find out whether TAA, SSAO or DOF need extra components or plugins (for example a `DepthPrepass`/`NormalPrepass`, or `Hdr`), and add exactly what the examples add.

In `scene.rs`:
- Add `#[derive(Component)] pub struct Tile;` and spawn it on every tile.
- Spawn the camera with `camera_post_bundle(QualityTier::default())` alongside `Camera3d::default()`.
- Add a skybox or environment map only if `Hdr`/IBL is required for the examples to look right. Otherwise keep `GlobalAmbientLight`. IBL look-dev is P1.

- [ ] **Step 4: Populate `RenderModes` and log it once**

Add a system in `lib.rs` that runs once 10 frames after startup. It:
- reads `RenderAdapterInfo` (in the main world in 0.20; if it's render-world only, add an `ExtractResource` copy as `target/ref/bevy/examples/shader/` does),
- checks whether the camera entity has `ScreenSpaceAmbientOcclusion` **and** whether the SSAO pipeline resource exists in the render world (the plugin bails out silently when storage-texture limits are too low, so component presence alone proves nothing). Search for `SsaoPipelines` in `target/ref/bevy/crates/bevy_pbr/src/ssao/mod.rs` and use that resource's presence,
- reads the GPU preprocessing mode from the render world's `GpuPreprocessingSupport` resource (`crates/bevy_render/src/batching/gpu_preprocessing.rs`) and formats it as `"{:?}"`.

Copy the results into `RenderModes` through a render-world → main-world channel. The minimal path is a `static Mutex<Option<RenderModes>>` written by a render-world system in `RenderApp`'s `Render` schedule and read by a main-world system. Log a single `info!("render modes: ssao={} gpu_preprocessing={} adapter={}")` line.

Register `DiagPlugin` and this system in `HexRenderPlugin`. In `hex-app`, every 30 frames copy `DiagSnapshot.json` into `bridge::DIAG_JSON`.

- [ ] **Step 5: Write `e2e/perf-gate.mjs`**

```js
// Engine gate (PRD P0): 2,000 tiles + full post stack at 1440p, GPU Chromium. Local only.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { extname, join } from "node:path";

const dist = process.argv[2] ?? "dist";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".wgsl": "text/plain" };
const server = createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  try {
    const body = await readFile(join(dist, path.endsWith("/") ? path + "index.html" : path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

// Uncapped: a vsync-locked 60 Hz run reports p95 ≈ 16.67 ms and would read as a false failure.
const browser = await chromium.launch({ channel: "chromium", headless: true,
  args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu",
         "--disable-gpu-vsync", "--disable-frame-rate-limit"] });
const errors = [];
const views = {};
let ua = "";
for (const view of ["strategic", "closeup"]) {
  const page = await browser.newPage({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 1 });
  page.on("console", (m) => { if (m.type() === "error") errors.push(`[${view}] ${m.text()}`); });
  await page.goto(base + `?scenario=gate2000&view=${view}`);
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
  await page.waitForTimeout(15_000);
  views[view] = await page.evaluate(() => window.hexWorld.diag());
  if (view === "strategic") {
    // Review Focus 5: resize mid-run must not error.
    await page.setViewportSize({ width: 1600, height: 900 });
    await page.waitForTimeout(2_000);
    await page.setViewportSize({ width: 2560, height: 1440 });
    await page.waitForTimeout(2_000);
  }
  ua = await page.evaluate(() => navigator.userAgent);
  await page.close();
}
await browser.close();
server.close();

const workload = { tiles: 2000, pieces_per_tile: 20, entities: 40300, tris_lod0_per_tile: 15700,
  materials: 40, moving: 300, boids: 1000, cascades: 4, post: ["taa", "ssao", "bloom", "dof"] };
const ok = (d) => d.tiles === 2000 && d.entities >= 40000 && d.p95 <= 16.6;
const result = { date: new Date().toISOString(), browser: "chromium-headless-gpu-uncapped", ua, viewport: "2560x1440",
  budget_p95_ms: 16.6, workload, views, console_errors: errors,
  pass: ok(views.strategic) && ok(views.closeup) && errors.length === 0 };
await mkdir("docs/perf", { recursive: true });
await writeFile("docs/perf/p0-engine-gate.json", JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result, null, 2));
if (!result.pass) process.exitCode = 1;
```

If p50 still sits at exactly 16.6–16.7 ms in both views, the uncapping flags did not take effect. Note `"vsync_suspected": true` in the JSON and report it rather than calling the run a pass or a fail.

`Makefile`:
```make
perf-gate: build-web
	cd e2e && npm ci --no-audit --no-fund
	node e2e/perf-gate.mjs dist
```

- [ ] **Step 6: Verify**

Run: `make checkall && make perf-gate`
Expected:
- The gate passes.
- `docs/perf/p0-engine-gate.json` is written with the workload block, both views reporting `tiles: 2000` and `entities` ≥ 40,000, a non-empty `adapter`, and the `ssao` and `gpu_preprocessing` values.
- Record the pass or fail result. A failed budget does **not** block this task. It is the input to the decision in Task 10.
- A console error or validation error **does** block it: fix it first.
- Also run `make run ARGS="--scenario gate2000"` natively and look at the window, or use `--shot gate2000 --exit-after 300` and read `target/shots/gate2000.png`. TAA, bloom, SSAO darkening between columns, and DOF blur at the edges should be visible.

- [ ] **Step 7: Commit**

```bash
git add crates Makefile e2e/perf-gate.mjs docs/perf/p0-engine-gate.json
git commit -m "feat(render): 2000-tile engine-gate scene, post stack, diagnostics, perf gate"
```

---

### Task 7: Custom compute pass (boids) and custom material inside Bevy's render graph

**Files:**
- Create: `crates/hex-render/src/boids.rs`, `assets/shaders/boids_compute.wgsl`, `assets/shaders/boid_material.wgsl`
- Modify: `crates/hex-render/src/lib.rs`, `crates/hex-render/src/scene.rs`

**Interfaces:**
- Consumes: the `HexRenderPlugin` app.
- Produces:
  - `pub struct BoidsPlugin { pub count: u32 }` (default 1,000).
  - The `ShaderStorageBuffer` asset `BoidBuffer(Handle<ShaderStorageBuffer>)` of `Boid { pos: vec4<f32>, vel: vec4<f32> }`.
  - `BoidMaterial` (a `Material` with `#[storage(0, read_only)] boids`), drawn as one merged mesh of `count` small tetrahedra.
  - `pub fn boid_mesh(count: u32) -> Mesh`: 4 vertices per boid. The vertex shader derives the boid index from `vertex_index / 4u`.

Reference implementations to adapt: `target/ref/bevy/examples/shader/compute_shader_game_of_life.rs` (render-graph compute node and pipeline cache), `examples/shader/storage_buffer.rs` (`ShaderStorageBuffer` shared by a material), and `examples/shader/shader_material.rs`.

- [ ] **Step 1: Write the failing `boid_mesh` test in `boids.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn mesh_has_four_vertices_and_four_tris_per_boid() {
        let m = boid_mesh(10);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("positions missing");
        };
        assert_eq!(p.len(), 40);
        assert_eq!(m.indices().unwrap().len(), 10 * 4 * 3);
    }

    #[test]
    fn initial_boids_are_deterministic_and_inside_bounds() {
        let a = initial_boids(64, 7);
        let b = initial_boids(64, 7);
        assert_eq!(a, b);
        assert!(a.iter().all(|x| x.pos[1] >= 40.0 && x.pos[1] <= 120.0));
    }
}
```

Run: `cargo test -p hex-render boids`
Expected: compile error.

- [ ] **Step 2: Implement the CPU side of `boids.rs`**

```rust
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::{render_resource::{AsBindGroup, ShaderType}, storage::ShaderStorageBuffer},
    shader::ShaderRef,
};

#[derive(Clone, Copy, Debug, PartialEq, ShaderType, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Boid {
    pub pos: [f32; 4],
    pub vel: [f32; 4],
}

/// Deterministic LCG so tests and screenshots are reproducible.
pub fn initial_boids(count: u32, seed: u32) -> Vec<Boid> {
    let mut s = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let mut rnd = move || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s >> 8) as f32 / (1u32 << 24) as f32
    };
    (0..count)
        .map(|_| Boid {
            pos: [rnd() * 800.0 - 400.0, 40.0 + rnd() * 80.0, rnd() * 800.0 - 400.0, 1.0],
            vel: [rnd() * 2.0 - 1.0, 0.0, rnd() * 2.0 - 1.0, 0.0],
        })
        .collect()
}

pub fn boid_mesh(count: u32) -> Mesh {
    let tet = [[0.0, 0.0, 1.2], [-0.6, 0.0, -0.6], [0.6, 0.0, -0.6], [0.0, 0.5, -0.4]];
    let mut pos = Vec::with_capacity(count as usize * 4);
    let mut idx = Vec::with_capacity(count as usize * 12);
    for b in 0..count {
        let base = b * 4;
        pos.extend_from_slice(&tet);
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 3, base + 1,
                                base + 1, base + 3, base + 2, base + 2, base + 3, base]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_indices(Indices::U32(idx))
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BoidMaterial {
    #[storage(0, read_only)]
    pub boids: Handle<ShaderStorageBuffer>,
}

impl Material for BoidMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/boid_material.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/boid_material.wgsl".into()
    }
}

#[derive(Resource, Clone)]
pub struct BoidBuffer(pub Handle<ShaderStorageBuffer>);
```

Add `bytemuck = { version = "1", features = ["derive"] }` to `hex-render`. If `Boid` can't derive both `ShaderType` and `Pod` in 0.20, keep `Pod` and build the buffer with `ShaderStorageBuffer::from(bytemuck::cast_slice(...).to_vec())`. The boid mesh's AABB doesn't match the moved vertices, so add `NoFrustumCulling` to the boid entity.

- [ ] **Step 3: Write the WGSL shaders**

`assets/shaders/boids_compute.wgsl`:
```wgsl
struct Boid { pos: vec4<f32>, vel: vec4<f32> };
@group(0) @binding(0) var<storage, read_write> boids: array<Boid>;
struct Params { count: u32, dt: f32, _pad0: u32, _pad1: u32 };
@group(0) @binding(1) var<uniform> params: Params;

@compute @workgroup_size(64)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params.count) { return; }
    var me = boids[i];
    var center = vec3<f32>(0.0);
    var align = vec3<f32>(0.0);
    var sep = vec3<f32>(0.0);
    var n = 0.0;
    // O(n^2) is fine for the P0 count (1,000); spatial hashing arrives with P2 flocks.
    for (var j = 0u; j < params.count; j++) {
        if (j == i) { continue; }
        let o = boids[j];
        let d = o.pos.xyz - me.pos.xyz;
        let dist = length(d);
        if (dist < 40.0) {
            center += o.pos.xyz; align += o.vel.xyz; n += 1.0;
            if (dist < 8.0) { sep -= d / max(dist, 0.01); }
        }
    }
    var v = me.vel.xyz;
    if (n > 0.0) {
        v += (center / n - me.pos.xyz) * 0.005 + (align / n - v) * 0.05;
    }
    v += sep * 0.1;
    v += vec3<f32>(-me.pos.x, 80.0 - me.pos.y, -me.pos.z) * 0.0005; // stay over the world
    let speed = clamp(length(v), 6.0, 18.0);
    v = normalize(v) * speed;
    me.vel = vec4<f32>(v, 0.0);
    me.pos = vec4<f32>(me.pos.xyz + v * params.dt, 1.0);
    boids[i] = me;
}
```

`assets/shaders/boid_material.wgsl`:
```wgsl
#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_clip}
#import bevy_pbr::view_transformations::position_world_to_clip

struct Boid { pos: vec4<f32>, vel: vec4<f32> };
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage, read> boids: array<Boid>;

struct Vertex {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec3<f32>,
};
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) shade: f32 };

@vertex
fn vertex(v: Vertex) -> VOut {
    let b = boids[v.vertex_index / 4u];
    let fwd = normalize(vec3<f32>(b.vel.x, 0.0, b.vel.z) + vec3<f32>(0.0001, 0.0, 0.0));
    let right = vec3<f32>(fwd.z, 0.0, -fwd.x);
    let world = b.pos.xyz + right * v.position.x + vec3<f32>(0.0, v.position.y, 0.0) + fwd * v.position.z;
    var o: VOut;
    o.clip = position_world_to_clip(world);
    o.shade = 0.6 + v.position.y;
    return o;
}

@fragment
fn fragment(i: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(vec3<f32>(0.12, 0.12, 0.14) * i.shade, 1.0);
}
```

Take the material bind-group macro name (`#{MATERIAL_BIND_GROUP}`) and the import paths from `target/ref/bevy/assets/shaders/storage_buffer.wgsl` and `custom_vertex_attribute.wgsl`. Use whatever they show. Remove any unused imports.

- [ ] **Step 4: Implement the render-graph compute node**

Follow `compute_shader_game_of_life.rs` in structure:
- `BoidsPlugin::build` creates the `ShaderStorageBuffer` from `initial_boids(count, 1)`, with `buffer.buffer_description.usage |= BufferUsages::STORAGE | BufferUsages::COPY_DST`, inserts `BoidBuffer`, spawns `(Mesh3d(boid_mesh(count)), MeshMaterial3d(BoidMaterial { boids }), NoFrustumCulling)`, and adds `MaterialPlugin::<BoidMaterial>::default()`.
- Extract `BoidBuffer` to the render world with `ExtractResourcePlugin`.
- In `RenderApp`: a `BoidsPipeline` resource built in `RenderStartup` (layout: binding 0 storage read_write, binding 1 uniform `Params`), a `Prepare` system that gets the `GpuShaderStorageBuffer` for the handle and builds the bind group plus a `UniformBuffer<Params>` with `dt` from the extracted `Time`, and a `BoidsNode` added to the render graph before `CameraDriverLabel`. The node dispatches `ceil(count / 64)` workgroups once the pipeline cache reports the pipeline ready.
- Register `BoidsPlugin { count: 1000 }` from `HexRenderPlugin` for every scenario.

TC5 requires warming render pipelines behind a loading screen. The P0 gate measures with a 15-second warm-up, so no loading screen is needed yet. Leave it for P2.

- [ ] **Step 5: Verify**

Run:
1. `cargo test -p hex-render`
2. `make checkall`
3. `cargo run -p hex-app --release -- --shot single --exit-after 300`, then Read `target/shots/single.png`. Dark bird shapes must be visible above the tile.
4. `make web-smoke`

Expected: all pass. Run `make web-smoke` again and look at `e2e/test-results/smoke-single.png`: the birds must appear in the web build too, which proves compute plus a custom material on WebGPU. Then re-run `make perf-gate` and commit the updated JSON, since boids now add load.

- [ ] **Step 6: Commit**

```bash
git add crates/hex-render assets/shaders docs/perf/p0-engine-gate.json
git commit -m "feat(render): GPU compute boids node and storage-buffer boid material"
```

---

### Task 8: Asset route: meshopt GLB and BC7 KTX2 + zstd on wasm32

**Files:**
- Create: `scripts/fetch-ktx.sh`, `scripts/assets/package.json`, `scripts/assets/build-p0-assets.mjs`, `crates/hex-assets/Cargo.toml`, `crates/hex-assets/src/lib.rs`, `crates/hex-assets/src/meshopt_glb.rs`, `crates/hex-assets/src/texture_family.rs`, `crates/hex-assets/src/bevy.rs`, `crates/hex-render/src/kit_test.rs`, `crates/hex-assets/tests/fixtures/` (generated)
- Modify: `Makefile`, `crates/hex-render/Cargo.toml`, `crates/hex-render/src/lib.rs`, `crates/hex-app/src/main.rs`

**Interfaces:**
- Produces:
  - `pub fn rewrite_glb(bytes: &[u8]) -> Result<Cow<'_, [u8]>, GlbError>`. It returns `Cow::Borrowed` when the file has no `EXT_meshopt_compression`. Otherwise it returns a plain GLB with every compressed bufferView decoded in place and the extension removed from `extensionsUsed`/`extensionsRequired`.
  - `pub enum GlbError { NotGlb, BadJson(String), Decode(String) }`, which implements `std::error::Error`.
  - `pub enum TextureFamily { Bc7, Astc, Png }` and `pub fn select_family(bc: bool, astc: bool) -> TextureFamily`.
  - `pub fn family_suffix(f: TextureFamily) -> &'static str` returning `"bc7.ktx2"`, `"astc.ktx2"` or `"png"`.
  - `hex_assets::bevy::MeshoptGlbPlugin` (feature `bevy`), which registers asset source `kit://` and decodes `.glb` reads through `rewrite_glb`.
  - Runtime assets in `assets/kit/p0/`: `hex_test.glb` (meshopt, float attributes), `hex_test.bc7.ktx2`, `hex_test.astc.ktx2` and `hex_test.png`.

- [ ] **Step 1: Fetch the KTX tools**

`scripts/fetch-ktx.sh`:
```bash
#!/usr/bin/env bash
# Installs KTX-Software 4.4.2 tools into ./tools (gitignored). macOS arm64 only (local asset builds).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/tools/ktx-4.4.2"
bin="$dest/KTX-Software-4.4.2-Darwin-arm64-tools.pkg/Payload/usr/local/bin"
lib="$dest/KTX-Software-4.4.2-Darwin-arm64-library.pkg/Payload/usr/local/lib"
if [ ! -x "$bin/ktx" ]; then
  mkdir -p "$root/tools"
  pkg="$root/tools/KTX-Software-4.4.2-Darwin-arm64.pkg"
  curl -fL --max-time 120 https://github.com/KhronosGroup/KTX-Software/releases/download/v4.4.2/KTX-Software-4.4.2-Darwin-arm64.pkg -o "$pkg"
  rm -rf "$dest"
  pkgutil --expand-full "$pkg" "$dest"
fi
printf 'export PATH="%s:$PATH"\nexport DYLD_LIBRARY_PATH="%s${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"\n' "$bin" "$lib" > "$root/tools/ktx-env.sh"
. "$root/tools/ktx-env.sh"
ktx --version
```

Run: `bash scripts/fetch-ktx.sh`
Expected: `ktx version: v4.4.2`, and `tools/ktx-env.sh` exists. If the expanded layout differs, adjust `bin` and `lib` to the directories that hold `ktx` and `libktx`.

- [ ] **Step 2: Write the asset build script**

`scripts/assets/package.json`, pinned to versions at least two weeks old (check with `npm view`):
```json
{ "private": true, "type": "module",
  "dependencies": { "@gltf-transform/core": "4.3.0", "@gltf-transform/extensions": "4.3.0",
                    "@gltf-transform/functions": "4.3.0", "meshoptimizer": "0.25.0", "sharp": "0.34.4" } }
```

`scripts/assets/build-p0-assets.mjs` builds a test hex and its texture, then writes:
- `assets/kit/p0/hex_test.png`: a 1024² procedural albedo (a stone/grass noise pattern) made with `sharp` from a raw RGBA buffer,
- `crates/hex-assets/tests/fixtures/hex_test.plain.glb`: an uncompressed reference,
- `assets/kit/p0/hex_test.glb`: the meshopt-compressed version of the same document.

```js
import { Document, NodeIO } from "@gltf-transform/core";
import { EXTMeshoptCompression } from "@gltf-transform/extensions";
import { reorder } from "@gltf-transform/functions";
import { MeshoptEncoder } from "meshoptimizer";
import sharp from "sharp";
import { mkdir } from "node:fs/promises";

const R = 16, H = 4, SEG = 24; // subdivided top cap so the mesh is big enough for compression to matter
const pos = [], nrm = [], uv = [], idx = [];
for (let ring = 0; ring <= SEG; ring++) {
  for (let k = 0; k < 6 * Math.max(ring, 1); k++) {
    const t = ring === 0 ? 0 : k / (6 * ring);
    const a = (30 + 360 * t) * Math.PI / 180;
    const rad = (ring / SEG) * R;
    pos.push(rad * Math.cos(a), H, rad * Math.sin(a)); nrm.push(0, 1, 0);
    uv.push(0.5 + rad * Math.cos(a) / (2 * R), 0.5 + rad * Math.sin(a) / (2 * R));
    if (ring === 0) break;
  }
}
// Triangulate rings: connect ring r-1 and ring r by nearest-angle walking.
const ringStart = (r) => (r === 0 ? 0 : 1 + 3 * (r - 1) * r);
const ringLen = (r) => (r === 0 ? 1 : 6 * r);
for (let r = 1; r <= SEG; r++) {
  const a0 = ringStart(r - 1), n0 = ringLen(r - 1), a1 = ringStart(r), n1 = ringLen(r);
  let i = 0, j = 0;
  while (i < n0 || j < n1) {
    const ti = n0 === 1 ? 1 : (i + 0.5) / n0, tj = (j + 0.5) / n1;
    if (j < n1 && (i >= n0 || tj <= ti)) { idx.push(a0 + (i % n0), a1 + ((j + 1) % n1), a1 + j); j++; }
    else { idx.push(a0 + (i % n0), a0 + ((i + 1) % n0), a1 + (j % n1)); i++; }
  }
}

function build() {
  const doc = new Document();
  const buf = doc.createBuffer();
  const prim = doc.createPrimitive()
    .setAttribute("POSITION", doc.createAccessor().setType("VEC3").setArray(new Float32Array(pos)).setBuffer(buf))
    .setAttribute("NORMAL", doc.createAccessor().setType("VEC3").setArray(new Float32Array(nrm)).setBuffer(buf))
    .setAttribute("TEXCOORD_0", doc.createAccessor().setType("VEC2").setArray(new Float32Array(uv)).setBuffer(buf))
    .setIndices(doc.createAccessor().setType("SCALAR").setArray(new Uint32Array(idx)).setBuffer(buf));
  doc.createScene().addChild(doc.createNode("hex_test").setMesh(doc.createMesh("hex_test").addPrimitive(prim)));
  return doc;
}

await MeshoptEncoder.ready;
await mkdir("assets/kit/p0", { recursive: true });
await mkdir("crates/hex-assets/tests/fixtures", { recursive: true });
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.encoder": MeshoptEncoder });

const plain = build();
await reorder({ encoder: MeshoptEncoder })(plain);
await io.write("crates/hex-assets/tests/fixtures/hex_test.plain.glb", plain);

const packed = build();
await reorder({ encoder: MeshoptEncoder })(packed);
packed.createExtension(EXTMeshoptCompression).setRequired(true)
  .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE });
await io.write("assets/kit/p0/hex_test.glb", packed);
await io.write("crates/hex-assets/tests/fixtures/hex_test.meshopt.glb", packed);

const S = 1024, px = Buffer.alloc(S * S * 4);
for (let y = 0; y < S; y++) for (let x = 0; x < S; x++) {
  const n = (Math.sin(x * 0.05) * Math.cos(y * 0.07) + Math.sin((x + y) * 0.013)) * 0.25 + 0.5;
  const o = (y * S + x) * 4;
  px[o] = 60 + 70 * n; px[o + 1] = 110 + 80 * n; px[o + 2] = 50 + 40 * n; px[o + 3] = 255;
}
await sharp(px, { raw: { width: S, height: S, channels: 4 } }).png().toFile("assets/kit/p0/hex_test.png");
console.log("p0 assets written");
```

Constraint: the meshopt GLB must keep `FLOAT` POSITION, NORMAL and TEXCOORD_0, because `bevy_gltf` lacks `KHR_mesh_quantization` (TC3). Since there is no `quantize()` call, the `QUANTIZE` method only byte-packs attributes. Verify in Step 5's test that `KHR_mesh_quantization` is absent from `extensionsUsed`. If gltf-transform adds it anyway, switch to `EncoderMethod.FILTER` and re-check. If it is still present, report it and stop: quantization handling becomes a P0 finding.

`Makefile`:
```make
fetch-tools:
	bash scripts/fetch-ktx.sh

assets-p0: fetch-tools
	cd scripts/assets && npm ci --no-audit --no-fund
	node scripts/assets/build-p0-assets.mjs
	. tools/ktx-env.sh; \
	ktx create --format R8G8B8A8_SRGB --encode uastc --uastc-quality 2 --zstd 18 --generate-mipmap assets/kit/p0/hex_test.png target/hex_test.uastc.ktx2 && \
	ktx transcode --target bc7 --zstd 18 target/hex_test.uastc.ktx2 assets/kit/p0/hex_test.bc7.ktx2 && \
	ktx transcode --target astc --zstd 18 target/hex_test.uastc.ktx2 assets/kit/p0/hex_test.astc.ktx2 && \
	ktx info assets/kit/p0/hex_test.bc7.ktx2 | head -20
```

The `--uastc-quality` flag name differs between KTX releases. Use what `ktx create --help` prints for 4.4.2. If `--target astc` needs a block size, use the 4x4 default.

Run: `make assets-p0`
Expected: all 4 runtime files plus 2 fixtures exist, and `ktx info` shows `vkFormat: VK_FORMAT_BC7_SRGB_BLOCK` with `supercompressionScheme: KTX_SS_ZSTD`.

- [ ] **Step 3: Write the failing `texture_family` tests**

`crates/hex-assets/Cargo.toml`:
```toml
[package]
name = "hex-assets"
version.workspace = true
edition.workspace = true
license.workspace = true
publish.workspace = true

[features]
bevy = ["dep:bevy"]

[dependencies]
serde_json = "1"
meshoptimizer-rs = "=0.2.1"
bevy = { workspace = true, optional = true, features = ["bevy_asset", "std"] }

[dev-dependencies]
gltf = "1"

[lints]
workspace = true
```

`meshoptimizer-rs` 0.2.1 (MIT, a pure-Rust port of meshoptimizer 1.3, confirmed on crates.io 2026-10-12) is the decoder from PRD §9.3. Its library name may be `meshoptimizer_rs` or something else, and its function names may differ from the ones used below. Read its docs (`cargo doc -p meshoptimizer-rs --open`, or the source in `~/.cargo/registry/src/*/meshoptimizer-rs-0.2.1/`) and adapt the calls in `decode()`.

`texture_family.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_bc7_on_desktop() {
        assert_eq!(select_family(true, false), TextureFamily::Bc7);
        assert_eq!(select_family(true, true), TextureFamily::Bc7);
    }

    #[test]
    fn falls_back_to_astc_then_png() {
        assert_eq!(select_family(false, true), TextureFamily::Astc);
        assert_eq!(select_family(false, false), TextureFamily::Png);
    }

    #[test]
    fn suffixes() {
        assert_eq!(family_suffix(TextureFamily::Bc7), "bc7.ktx2");
        assert_eq!(family_suffix(TextureFamily::Astc), "astc.ktx2");
        assert_eq!(family_suffix(TextureFamily::Png), "png");
    }
}
```

Run: `cargo test -p hex-assets`
Expected: compile error.

- [ ] **Step 4: Implement `texture_family.rs`**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFamily {
    Bc7,
    Astc,
    Png,
}

/// Picks the runtime texture variant from the device's compression features (PRD §9.3).
pub fn select_family(bc: bool, astc: bool) -> TextureFamily {
    if bc {
        TextureFamily::Bc7
    } else if astc {
        TextureFamily::Astc
    } else {
        TextureFamily::Png
    }
}

pub fn family_suffix(f: TextureFamily) -> &'static str {
    match f {
        TextureFamily::Bc7 => "bc7.ktx2",
        TextureFamily::Astc => "astc.ktx2",
        TextureFamily::Png => "png",
    }
}
```

Run: `cargo test -p hex-assets texture_family`. Expected: 3 passed.

- [ ] **Step 5: Write the failing `rewrite_glb` tests**

`crates/hex-assets/tests/meshopt_glb.rs`:
```rust
use hex_assets::{GlbError, rewrite_glb};
use std::borrow::Cow;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn positions(glb: &[u8]) -> Vec<[f32; 3]> {
    let (doc, buffers, _) = gltf::import_slice(glb).expect("valid plain glTF");
    let mesh = doc.meshes().next().unwrap();
    let prim = mesh.primitives().next().unwrap();
    let reader = prim.reader(|b| Some(&buffers[b.index()]));
    reader.read_positions().unwrap().collect()
}

#[test]
fn plain_glb_passes_through_borrowed() {
    let plain = fixture("hex_test.plain.glb");
    let out = rewrite_glb(&plain).unwrap();
    assert!(matches!(out, Cow::Borrowed(_)));
    assert_eq!(&*out, &plain[..]);
}

#[test]
fn meshopt_glb_decodes_to_reference_geometry() {
    let packed = fixture("hex_test.meshopt.glb");
    assert!(String::from_utf8_lossy(&packed).contains("EXT_meshopt_compression"));
    assert!(!String::from_utf8_lossy(&packed).contains("KHR_mesh_quantization"));
    let out = rewrite_glb(&packed).unwrap();
    assert!(matches!(out, Cow::Owned(_)));
    assert!(!String::from_utf8_lossy(&out).contains("EXT_meshopt_compression"));
    assert_eq!(positions(&out), positions(&fixture("hex_test.plain.glb")));
}

#[test]
fn non_glb_is_rejected() {
    assert!(matches!(rewrite_glb(b"not a glb at all"), Err(GlbError::NotGlb)));
}

#[test]
fn corrupt_meshopt_payload_errors_not_panics() {
    let mut packed = fixture("hex_test.meshopt.glb");
    let n = packed.len();
    for b in &mut packed[n - 256..] {
        *b = 0xA5;
    }
    assert!(rewrite_glb(&packed).is_err());
}
```

Run: `cargo test -p hex-assets --test meshopt_glb`
Expected: compile error.

- [ ] **Step 6: Implement `meshopt_glb.rs`**

The GLB layout is a 12-byte header (`glTF`, version 2, total length), then a JSON chunk (length, `JSON`, padded with spaces), then an optional BIN chunk (length, `BIN\0`, padded with zeros).

```rust
use serde_json::Value;
use std::borrow::Cow;

#[derive(Debug)]
pub enum GlbError {
    NotGlb,
    BadJson(String),
    Decode(String),
}

impl std::fmt::Display for GlbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for GlbError {}

const EXT: &str = "EXT_meshopt_compression";

fn u32_at(b: &[u8], o: usize) -> Result<u32, GlbError> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(GlbError::NotGlb)
}

/// Decodes EXT_meshopt_compression bufferViews into a plain GLB that bevy_gltf can load (TC3).
pub fn rewrite_glb(bytes: &[u8]) -> Result<Cow<'_, [u8]>, GlbError> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" || u32_at(bytes, 4)? != 2 {
        return Err(GlbError::NotGlb);
    }
    let json_len = u32_at(bytes, 12)? as usize;
    if &bytes[16..20] != b"JSON" || bytes.len() < 20 + json_len {
        return Err(GlbError::NotGlb);
    }
    let json_bytes = &bytes[20..20 + json_len];
    if !json_bytes.windows(EXT.len()).any(|w| w == EXT.as_bytes()) {
        return Ok(Cow::Borrowed(bytes));
    }
    let mut json: Value = serde_json::from_slice(json_bytes).map_err(|e| GlbError::BadJson(e.to_string()))?;
    let bin_off = 20 + json_len;
    let bin = if bytes.len() >= bin_off + 8 && &bytes[bin_off + 4..bin_off + 8] == b"BIN\0" {
        let n = u32_at(bytes, bin_off)? as usize;
        bytes.get(bin_off + 8..bin_off + 8 + n).ok_or(GlbError::NotGlb)?
    } else {
        &[][..]
    };

    let mut out_bin: Vec<u8> = Vec::with_capacity(bin.len() * 4);
    let views = json["bufferViews"].as_array_mut().ok_or(GlbError::BadJson("no bufferViews".into()))?;
    for view in views.iter_mut() {
        let data: Vec<u8> = if let Some(ext) = view.get("extensions").and_then(|e| e.get(EXT)).cloned() {
            let off = ext["byteOffset"].as_u64().unwrap_or(0) as usize;
            let len = ext["byteLength"].as_u64().ok_or(GlbError::BadJson("ext byteLength".into()))? as usize;
            let stride = ext["byteStride"].as_u64().ok_or(GlbError::BadJson("ext byteStride".into()))? as usize;
            let count = ext["count"].as_u64().ok_or(GlbError::BadJson("ext count".into()))? as usize;
            let mode = ext["mode"].as_str().unwrap_or("ATTRIBUTES");
            let filter = ext["filter"].as_str().unwrap_or("NONE");
            let src = bin.get(off..off + len).ok_or(GlbError::Decode("ext range".into()))?;
            let decoded = decode(src, count, stride, mode, filter)?;
            if let Some(obj) = view.as_object_mut() {
                obj.remove("extensions");
            }
            decoded
        } else {
            let off = view["byteOffset"].as_u64().unwrap_or(0) as usize;
            let len = view["byteLength"].as_u64().unwrap_or(0) as usize;
            bin.get(off..off + len).ok_or(GlbError::Decode("view range".into()))?.to_vec()
        };
        while out_bin.len() % 4 != 0 {
            out_bin.push(0);
        }
        view["buffer"] = 0.into();
        view["byteOffset"] = out_bin.len().into();
        view["byteLength"] = data.len().into();
        out_bin.extend_from_slice(&data);
    }
    // Fallback buffers (EXT_meshopt_compression may declare a placeholder buffer) are dropped.
    json["buffers"] = serde_json::json!([{ "byteLength": out_bin.len() }]);
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(list) = json[key].as_array_mut() {
            list.retain(|v| v != EXT);
        }
    }
    Ok(Cow::Owned(write_glb(&json, &out_bin)?))
}

fn decode(src: &[u8], count: usize, stride: usize, mode: &str, filter: &str) -> Result<Vec<u8>, GlbError> {
    // meshoptimizer-rs API: adapt names to the crate's docs (decode_vertex_buffer / decode_index_buffer /
    // decode_index_sequence and the octahedral/quaternion/exponential filters).
    let mut out = vec![0u8; count * stride];
    let r = match mode {
        "ATTRIBUTES" => meshoptimizer_rs::decode_vertex_buffer(&mut out, count, stride, src),
        "TRIANGLES" => meshoptimizer_rs::decode_index_buffer(&mut out, count, stride, src),
        "INDICES" => meshoptimizer_rs::decode_index_sequence(&mut out, count, stride, src),
        m => return Err(GlbError::Decode(format!("unknown mode {m}"))),
    };
    r.map_err(|e| GlbError::Decode(format!("{e:?}")))?;
    match filter {
        "NONE" => {}
        f => return Err(GlbError::Decode(format!("filter {f} not supported in P0"))),
    }
    Ok(out)
}

fn write_glb(json: &Value, bin: &[u8]) -> Result<Vec<u8>, GlbError> {
    let mut j = serde_json::to_vec(json).map_err(|e| GlbError::BadJson(e.to_string()))?;
    while j.len() % 4 != 0 {
        j.push(b' ');
    }
    let mut b = bin.to_vec();
    while b.len() % 4 != 0 {
        b.push(0);
    }
    let total = 12 + 8 + j.len() + 8 + b.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(j.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&j);
    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&b);
    Ok(out)
}
```

`lib.rs`:
```rust
//! Hex World asset loading: meshopt GLB rewrite and texture-family selection.
mod meshopt_glb;
mod texture_family;
#[cfg(feature = "bevy")]
pub mod bevy;

pub use meshopt_glb::{GlbError, rewrite_glb};
pub use texture_family::{TextureFamily, family_suffix, select_family};
```

Run: `cargo test -p hex-assets`
Expected: every test passes, including `meshopt_glb_decodes_to_reference_geometry`. That test is the proof for the pure-Rust decoder. **If `meshoptimizer-rs` fails it, or does not exist:** stop. Record the failure in the commit message and in `docs/decisions/0001-engine.md` (Task 10 creates the file; create it early if needed). Then switch `decode()` to the alternative route: under `cfg(target_arch = "wasm32")`, call the official `meshopt_decoder.js` through a wasm-bindgen extern, and under native use the `meshopt` crate (C FFI) behind `cfg(not(target_arch = "wasm32"))`. Make the test pass natively.

- [ ] **Step 7: Implement the Bevy asset source adapter in `hex-assets/src/bevy.rs`**

Register an asset source named `kit` whose reader wraps the platform default reader for the `assets/kit` directory. On `read()` of a path ending in `.glb`, read all bytes, call `rewrite_glb`, and return a `VecReader` over the result. Map `GlbError` to `AssetReaderError::Io` with the message. Everything else is delegated unchanged. Use `target/ref/bevy/examples/asset/custom_asset_reader.rs` as the template: it shows `AssetSource::build().with_reader(...)` and `AssetSourceId`. Register with `app.register_asset_source("kit", ...)` **before** `DefaultPlugins` (the example shows the order). `MeshoptGlbPlugin` is therefore applied in `hex-app` before `DefaultPlugins`.

- [ ] **Step 8: Load the test asset in `hex-render/src/kit_test.rs`**

- Add `hex-assets = { path = "../hex-assets", features = ["bevy"] }` to `hex-render`.
- At startup, read `RenderDevice::features()`: bc = `Features::TEXTURE_COMPRESSION_BC`, astc = `Features::TEXTURE_COMPRESSION_ASTC`. Call `select_family`, log the choice, and load `kit://p0/hex_test.glb#Mesh0/Primitive0` plus `kit://p0/hex_test.{suffix}`. Note that the texture loads through `kit://` too and passes through the reader unchanged.
- Spawn the mesh with a `StandardMaterial { base_color_texture: Some(tex), perceptual_roughness: 0.9, .. }` at axial `(0,0)`, raised to sit on top of the hex column. Do this in `Scenario::Single` only.
- The `ktx2` feature is already on through `3d_api`. KTX2 requests with BC7 need the device feature, which WebGPU exposes when the adapter supports it and Bevy requests the adapter's features (PRD §18).

- [ ] **Step 9: Verify natively and on the web**

Run:
1. `make checkall`
2. `make shot`, then Read `target/shots/single.png`. The textured disc must sit on the hex, with the procedural noise pattern visible.
3. `make web-smoke`, then Read `e2e/test-results/smoke-single.png`. The same texture must appear.
4. `grep -i "texture family" ` in the smoke console output. Add a `console.log` passthrough in `smoke.mjs` if needed so the log line shows `Bc7` on the Mac.

Measure and note in the commit message: the sizes of `hex_test.glb` vs `hex_test.plain.glb`, `hex_test.bc7.ktx2` vs `.png`, and the load time from the network panel (`page.on("requestfinished")` timing for the two files in the smoke test).

- [ ] **Step 10: Commit**

```bash
git add scripts Makefile crates/hex-assets crates/hex-render crates/hex-app assets/kit/p0
git commit -m "feat(assets): meshopt GLB rewrite reader, KTX2 BC7/ASTC family selection, P0 test asset"
```

---

### Task 9: GitHub Pages deploy at `hex-world.pardev.net`

**Files:**
- Create: `.github/workflows/ci.yml`
- Modify: `Makefile` (`checkall-ci` complete), `README.md`

**Interfaces:**
- Consumes: `make checkall`, `make build-web`, `make web-size`, `CNAME`.
- Produces: an Actions workflow that gates and deploys `dist/` to Pages on push to `main`.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

Copy `~/Repos/suck-it-up/.github/workflows/ci.yml`, with these changes:
- Drop the LFS steps (no LFS, A4).
- Keep the `check`, `web` and `deploy` jobs. The `check` job runs `make fmt-check lint typecheck test`. Apt deps stay for the Linux build of Bevy.
- `TRUNK_VERSION: v0.21.14`.
- Before pinning, check that every `uses:` ref exists: `gh api repos/actions/checkout/git/ref/tags/v7`, and likewise for `actions/cache`, `Swatinem/rust-cache@v2`, `actions/upload-pages-artifact@v5` and `actions/deploy-pages@v5`. Use the newest major that resolves.

- [ ] **Step 2: Verify the CI-parity gate locally**

Run: `make checkall-ci`
Expected: passes, with `web-size` under budget.

- [ ] **Step 3: Commit**

```bash
git add .github Makefile README.md
git commit -m "ci: gate and deploy to GitHub Pages"
```

- [ ] **Step 4: OWNER GATE: repo, push, Pages and DNS**

Stop and ask the owner in chat for explicit approval of each outward action:
1. `gh repo create paulrobello/hex-world --public --source . --remote origin`
2. `git push -u origin main`
3. In the repo settings, set Pages source to "GitHub Actions" and the custom domain to `hex-world.pardev.net`: `gh api -X POST repos/paulrobello/hex-world/pages -f build_type=workflow`, then `gh api -X PUT repos/paulrobello/hex-world/pages -f cname=hex-world.pardev.net`.
4. Create the Cloudflare DNS record `CNAME hex-world → paulrobello.github.io` (proxy **off**) per `~/.claude/guides/infrastructure.md` and the `par-infra` skill.
5. After the certificate is issued: `gh api -X PUT repos/paulrobello/hex-world/pages -F https_enforced=true`.

Do not run any of these without that approval.

- [ ] **Step 5: Verify the live site**

Once deployed, run `make web-smoke` against the live URL. Add `SMOKE_BASE` env support to `smoke.mjs`: if it is set, skip the local server and use that base URL. Run `SMOKE_BASE=https://hex-world.pardev.net/ node e2e/smoke.mjs`.
Expected: `smoke OK`. Then record in `docs/perf/p0-deploy.json`:
- the wasm transfer size (the response `content-length` with gzip, from `page.on("response")`),
- total transferred bytes,
- time from navigation to `hexWorld.ready()`,
- `curl -sI https://hex-world.pardev.net | head -5` (HTTP 200 and the certificate valid).

Commit the JSON with `git commit -m "docs(perf): live deploy size and time-to-interactive"`.

---

### Task 10: Engine gate decision

**Files:**
- Create: `docs/decisions/0001-engine.md`
- Modify: `PRD.md` (D2, §9.3, Q3, Q6, §18), `docs/perf/p0-engine-gate.json`

**Interfaces:**
- Consumes: the outputs of Tasks 6–9.
- Produces: the recorded decision Bevy vs custom wgpu (Q6) and the asset route (Q3).

- [ ] **Step 1: Collect browser measurements**

- Chrome: `make perf-gate`. This is already the JSON from Task 6/7, re-run on the final build.
- Safari 26 (macOS): for both `view=strategic` and `view=closeup`, open `http://127.0.0.1:<trunk port>/?scenario=gate2000&view=<view>&diag=1` from `trunk serve --release`, or the live URL, in a 2560×1440 window. Wait 15 s and read p50, p95, ssao and gpu_preprocessing from the overlay. Use the computer-use tools if the agent drives it. Otherwise the owner reads the numbers out. Append them to the JSON under `"safari"`.
- iOS 26 Safari smoke test: the owner opens the live URL on the iPhone and confirms the hex renders, or that a WebGPU-off device shows the unsupported page. Record the answer.

- [ ] **Step 2: Write `docs/decisions/0001-engine.md`**

```markdown
# 0001 — Engine: Bevy 0.20 vs custom wgpu

Date: <YYYY-MM-DD>  Status: Accepted

## Measurements
| Check | Chrome | Safari | Pass? |
|---|---|---|---|
| p95 frame time, gate workload (40,300 entities, 15.7k tris/tile LOD0), strategic view | … ms | … ms | ≤16.6 |
| p95 frame time, gate workload, close-up view | … ms | … ms | ≤16.6 |
| SSAO active | … | … | yes |
| GPU preprocessing mode | … | … | — |
| TAA / bloom / DOF visible, no validation errors | … | … | yes |
| Compute boids + custom material render | … | … | yes |
| meshopt GLB + BC7 KTX2 load on wasm32 | … | … | yes |
| wasm gzip wire size / time to interactive | … | — | ≤12 MB |
| iOS 26 smoke | … | | loads |

## Decision
<Bevy stays | switch to custom wgpu 30 + standalone bevy_ecs>, because <the failing or passing rows>.

## Asset route (Q3)
<meshoptimizer-rs pure Rust | meshopt_decoder.js via wasm-bindgen>; textures <BC7/ASTC KTX2 + zstd>.

## Consequences
<what P1/P2 must account for: e.g., custom SSAO node if Bevy SSAO is off in Safari>.
```

Fill in every cell from the measurement files. Leave nothing as `…`.

- [ ] **Step 3: Update `PRD.md`**

- D2: replace "The P0 spike is the decision gate…" with the outcome and a link to `docs/decisions/0001-engine.md`.
- §9.3: mark the chosen route as decided.
- Q3 and Q6: mark them resolved, with the decision link.
- §18: append one line with the measured numbers.

- [ ] **Step 4: Run the gate and commit**

Run: `make checkall-ci`
Expected: passes.

```bash
git add docs/decisions PRD.md docs/perf
git commit -m "docs: record P0 engine gate decision and asset route"
```
