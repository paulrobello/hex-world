pub use hex_render::{GateConfig, GateView, Layout, PostMode, Scenario, ShadowCasters};

/// Native present mode. `NoVsync` is Bevy's `AutoNoVsync` (immediate, else mailbox, else fifo).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Present {
    #[default]
    Fifo,
    Immediate,
    NoVsync,
}

impl Present {
    pub fn parse(s: &str) -> Option<Present> {
        match s {
            "fifo" => Some(Present::Fifo),
            "immediate" => Some(Present::Immediate),
            "novsync" => Some(Present::NoVsync),
            _ => None,
        }
    }
}

fn set_or_warn<T>(slot: &mut T, parsed: Option<T>, k: &str, v: &str, warnings: &mut Vec<String>) {
    match parsed {
        Some(x) => *slot = x,
        None => warnings.push(format!("unknown {k} '{v}', using default")),
    }
}

#[derive(Clone, Debug, Default)]
pub struct Flags {
    /// Gate variant: `layout`, `post`, `shadows`, `chunk`, `shadow_casters`, `cascades`,
    /// `shadow_dist`.
    pub gate: GateConfig,
    /// Native only.
    pub present: Present,
    pub scenario: Scenario,
    /// Gate camera view: `strategic` (default) or `closeup`.
    pub view: GateView,
    pub shot: Option<String>,
    pub exit_after_frames: Option<u32>,
    pub diag: bool,
    /// Native only: command script path, read and parsed before the app starts.
    pub script: Option<String>,
    pub screenshot_after_secs: Option<f32>,
    pub exit_after_secs: Option<f32>,
    /// Native only: fixed window size in physical pixels at scale factor 1.
    pub size: Option<(u32, u32)>,
    pub seed: u64,
    pub no_anim: bool,
    /// `fps=on` starts with the FPS counter shown (default off since the Settings performance
    /// readout, owner 2026-10-19; F3 toggles at runtime).
    pub fps_on: bool,
    pub dump_state: Option<String>,
    /// `clock=fixed`: every Bevy clock advances exactly 1/60 s per frame, so the 20 Hz car tick
    /// lands on every third frame and scripted runs replay identically.
    pub clock_fixed: bool,
    /// Native only: render the tile thumbnails and exit (`thumbs::force_flags` sets the rest).
    pub thumbs: bool,
    /// A valid `tier` key was given: it wins over auto-detection (`hex_play::tier`).
    pub tier_explicit: bool,
    /// `gpu_timing=on`: per-pass GPU timestamps in the diag output (`hex_play::gpu_timing`).
    pub gpu_timing: bool,
    /// `photo-export=1|2|4`: once ready, export a photo-mode PNG at that scale, named
    /// `photo-<n>x` (natively `target/shots/photo-<n>x.png`).
    pub photo_export: Option<u32>,
    /// Native only: `.hexworld` file loaded before any script command (PRD §12.2).
    pub load: Option<String>,
    /// Native only: `.hexworld` file written with the world on exit.
    pub save: Option<String>,
    /// `debug=lod,perf,grid,sockets,colliders,lod2`: developer overlays on from the start (never
    /// persisted; `hex_play::debug`).
    pub debug: hex_play::debug::DebugOverlays,
    /// Native only, review/debug: `water_look=trail=<x>,churn=<g>` overrides the drying-trail
    /// darkening and the river churn foam scale (`hex_render::water::WaterLook`).
    pub water_look: Option<hex_render::water::WaterLook>,
    /// `frame_cap=off|30|60|120` (natively also `--frame-cap 60`): the start frame rate cap
    /// (`crate::frame_cap`); the Settings sheet's stored choice follows as a command.
    pub frame_cap: crate::frame_cap::FrameCap,
    /// Native only, `offscreen=on`: the game camera draws into an image the window's size instead
    /// of the window (`crate::offscreen`), so a frame renders in full while the window is occluded
    /// (a locked or sleeping screen, another window on top, another Space) and presents nothing,
    /// and screenshots capture the image. A capture run turns it on unless the key is given
    /// ([`Flags::capture_default`]).
    pub offscreen: bool,
    /// An `offscreen` key was given: it wins over the capture-run default.
    pub offscreen_explicit: bool,
    /// Perf ablation, `kit_ablate=cascade_clip,coalesce,boundaries`: turns off the named
    /// close-camera draw cuts (O5, O4) to time each against the same build, or the field
    /// boundaries (B2) for a draw census without them (`hex_render::kit_instances::KitAblate`).
    pub kit_ablate: hex_render::kit_instances::KitAblate,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl Flags {
    /// The run takes screenshots: `--thumbs`, `--shot`, `--screenshot-after`, or a script with a
    /// `screenshot` command (`script_shoots`).
    pub fn captures(&self, script_shoots: bool) -> bool {
        self.thumbs || self.shot.is_some() || self.screenshot_after_secs.is_some() || script_shoots
    }

    /// A capture run renders offscreen unless an `offscreen` key was given, so an occluded window
    /// cannot turn its screenshots black. True when this default turned it on.
    pub fn capture_default(&mut self, script_shoots: bool) -> bool {
        let implicit = !self.offscreen_explicit && self.captures(script_shoots);
        if implicit {
            self.offscreen = true;
        }
        implicit
    }
}

/// `"1280x800"` -> `Some((1280, 800))`; zero or malformed sizes are rejected.
pub fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    let (w, h) = (w.parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

fn parse_secs(k: &str, v: &str, warnings: &mut Vec<String>) -> Option<f32> {
    match v.parse::<f32>() {
        Ok(n) if n >= 0.0 && n.is_finite() => Some(n),
        _ => {
            warnings.push(format!("bad {k} '{v}'"));
            None
        }
    }
}

/// The start scenario when no `scenario` or `shot` key names one, or the one named is unknown. The
/// web build opens on the game; the native build stays the dev and capture harness, whose scripts
/// and baselines start from `single`.
pub const fn default_scenario() -> Scenario {
    if cfg!(target_arch = "wasm32") {
        Scenario::Play
    } else {
        Scenario::Single
    }
}

/// Shared by the native CLI (`--shot gate2000`) and the web query string (`?shot=gate2000`).
pub fn flags_from_pairs(pairs: &[(String, String)]) -> (Flags, Vec<String>) {
    flags_from_pairs_or(pairs, default_scenario())
}

/// [`flags_from_pairs`] with an explicit fallback scenario.
pub fn flags_from_pairs_or(pairs: &[(String, String)], default: Scenario) -> (Flags, Vec<String>) {
    let mut state = FlagParser {
        f: Flags {
            scenario: default,
            ..Flags::default()
        },
        warnings: vec![],
        grade: None,
        tonemap_set: false,
        ev100_set: false,
    };
    for (k, v) in pairs {
        if let Some((_, handler)) = FLAG_HANDLERS.iter().find(|(key, _)| *key == k) {
            handler(&mut state, k, v);
        }
    }
    // Explicit tonemap and exposure override the grade regardless of key order.
    if let Some(g) = state.grade {
        state.f.gate.grade = g;
        if !state.tonemap_set {
            state.f.gate.tonemap = g.tonemap();
        }
        if !state.ev100_set {
            state.f.gate.ev100 = hex_render::Ev100::from_f32(g.ev100());
        }
    }
    (state.f, state.warnings)
}

struct FlagParser {
    f: Flags,
    warnings: Vec<String>,
    grade: Option<hex_render::Grade>,
    tonemap_set: bool,
    ev100_set: bool,
}

type FlagHandler = fn(&mut FlagParser, &String, &String);

const FLAG_HANDLERS: &[(&str, FlagHandler)] = &[
    ("shot", |state, k, v| {
        match Scenario::parse(v) {
            Some(s) => state.f.scenario = s,
            None => state
                .warnings
                .push(format!("unknown scenario '{v}', using default")),
        }
        if k == "shot" && Scenario::parse(v).is_some() {
            state.f.shot = Some(v.clone());
        }
    }),
    ("scenario", |state, k, v| {
        match Scenario::parse(v) {
            Some(s) => state.f.scenario = s,
            None => state
                .warnings
                .push(format!("unknown scenario '{v}', using default")),
        }
        if k == "shot" && Scenario::parse(v).is_some() {
            state.f.shot = Some(v.clone());
        }
    }),
    ("view", |state, _k, v| match GateView::parse(v) {
        Some(view) => state.f.view = view,
        None => state
            .warnings
            .push(format!("unknown view '{v}', using default")),
    }),
    ("diag", |state, _k, v| state.f.diag = v != "0"),
    ("exit-after", |state, _k, v| match v.parse() {
        Ok(n) => state.f.exit_after_frames = Some(n),
        Err(_) => state.warnings.push(format!("bad exit-after '{v}'")),
    }),
    ("script", |state, _k, v| state.f.script = Some(v.clone())),
    ("thumbs", |state, _k, v| state.f.thumbs = v != "0"),
    ("dump-state", |state, _k, v| {
        state.f.dump_state = Some(v.clone())
    }),
    ("load", |state, _k, v| state.f.load = Some(v.clone())),
    ("save", |state, _k, v| state.f.save = Some(v.clone())),
    ("screenshot-after", |state, k, v| {
        state.f.screenshot_after_secs = parse_secs(k, v, &mut state.warnings)
    }),
    ("exit-after-seconds", |state, k, v| {
        state.f.exit_after_secs = parse_secs(k, v, &mut state.warnings)
    }),
    ("size", |state, _k, v| match parse_size(v) {
        Some(s) => state.f.size = Some(s),
        None => state.warnings.push(format!("bad size '{v}', expected WxH")),
    }),
    ("seed", |state, _k, v| match v.parse() {
        Ok(n) => state.f.seed = n,
        Err(_) => state.warnings.push(format!("bad seed '{v}'")),
    }),
    ("no-anim", |state, _k, v| state.f.no_anim = v != "0"),
    ("fps", |state, _k, v| match v.as_str() {
        "on" => state.f.fps_on = true,
        "off" => state.f.fps_on = false,
        _ => state
            .warnings
            .push(format!("unknown fps '{v}', using default")),
    }),
    ("layout", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.layout,
            Layout::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("post", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.post,
            PostMode::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("shadows", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.shadows,
            hex_render::parse_shadows(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("chunk", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.chunk,
            hex_render::parse_chunk(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("shadow_casters", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.shadow_casters,
            ShadowCasters::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("cascades", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.cascades,
            hex_render::parse_cascades(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("shadow_dist", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.shadow_dist,
            hex_render::parse_shadow_dist(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("kit_hooks", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.kit_hooks,
            hex_render::KitHooks::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("kit_src", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.kit_src,
            hex_render::KitSource::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("tonemap", |state, k, v| {
        let t = hex_render::Tonemap::parse(v);
        state.tonemap_set |= t.is_some();
        set_or_warn(&mut state.f.gate.tonemap, t, k, v, &mut state.warnings);
    }),
    ("grade", |state, _k, v| match hex_render::Grade::parse(v) {
        Some(g) => state.grade = Some(g),
        None => state
            .warnings
            .push(format!("unknown grade '{v}', using default")),
    }),
    ("ibl", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.ibl,
            hex_render::parse_ibl(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("ev100", |state, k, v| {
        let e = hex_render::Ev100::parse(v);
        state.ev100_set |= e.is_some();
        set_or_warn(&mut state.f.gate.ev100, e, k, v, &mut state.warnings);
    }),
    ("tilt", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.tilt,
            hex_render::TiltMode::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("tier", |state, k, v| {
        let t = hex_render::post::QualityTier::parse(v);
        state.f.tier_explicit |= t.is_some();
        set_or_warn(&mut state.f.gate.tier, t, k, v, &mut state.warnings);
    }),
    ("tier_warm", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.tier_warm,
            hex_render::parse_shadows(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("offscreen", |state, k, v| {
        let on = hex_render::parse_shadows(v);
        state.f.offscreen_explicit |= on.is_some();
        set_or_warn(&mut state.f.offscreen, on, k, v, &mut state.warnings)
    }),
    ("kit_ablate", |state, k, v| {
        set_or_warn(
            &mut state.f.kit_ablate,
            hex_render::kit_instances::KitAblate::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("gpu_timing", |state, k, v| {
        set_or_warn(
            &mut state.f.gpu_timing,
            hex_render::parse_shadows(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("tilt_strength", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.tilt_strength,
            hex_render::TiltStrength::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("grass", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.grass,
            hex_render::parse_grass(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("grass_distance", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.grass_distance,
            hex_render::parse_grass_distance(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    // The hyphenated spelling, natively (`--grass-distance far`).
    ("grass-distance", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.grass_distance,
            hex_render::parse_grass_distance(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("frame_cap", |state, k, v| {
        set_or_warn(
            &mut state.f.frame_cap,
            crate::frame_cap::FrameCap::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    // The hyphenated spelling, natively (`--frame-cap 60`).
    ("frame-cap", |state, k, v| {
        set_or_warn(
            &mut state.f.frame_cap,
            crate::frame_cap::FrameCap::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("water_clock", |state, k, v| {
        set_or_warn(
            &mut state.f.gate.water_fixed,
            match v.as_str() {
                "fixed" => Some(true),
                "real" => Some(false),
                _ => None,
            },
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("clock", |state, k, v| {
        set_or_warn(
            &mut state.f.clock_fixed,
            match v.as_str() {
                "fixed" => Some(true),
                "real" => Some(false),
                _ => None,
            },
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("present", |state, k, v| {
        set_or_warn(
            &mut state.f.present,
            Present::parse(v),
            k,
            v,
            &mut state.warnings,
        )
    }),
    ("debug", |state, _k, v| {
        let (d, unknown) = hex_play::debug::DebugOverlays::parse_list(v);
        state.f.debug = d;
        for u in unknown {
            state.warnings.push(format!("unknown debug overlay '{u}'"));
        }
    }),
    (
        "water_look",
        |state, _k, v| match hex_render::water::WaterLook::parse(v) {
            Some(l) => state.f.water_look = Some(l),
            None => state.warnings.push(format!(
                "bad water_look '{v}', expected trail=<0.3..1>,churn=<0..6>"
            )),
        },
    ),
    ("photo-export", |state, _k, v| match v.parse::<u32>() {
        Ok(n) if crate::photo::EXPORT_SCALES.contains(&n) => state.f.photo_export = Some(n),
        _ => state
            .warnings
            .push(format!("bad photo-export '{v}', expected 1, 2 or 4")),
    }),
];

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
    [
        "shot",
        "scenario",
        "view",
        "diag",
        "exit-after",
        "seed",
        "no-anim",
        "layout",
        "post",
        "shadows",
        "chunk",
        "shadow_casters",
        "cascades",
        "shadow_dist",
        "fps",
        "kit_hooks",
        "kit_src",
        "tonemap",
        "grade",
        "ibl",
        "ev100",
        "tilt",
        "tilt_strength",
        "tier",
        "tier_warm",
        "gpu_timing",
        "kit_ablate",
        "water_clock",
        "clock",
        "grass",
        "grass_distance",
        "frame_cap",
        "photo-export",
        "debug",
    ]
    .iter()
    .filter_map(|k| params.get(k).map(|v| (k.to_string(), v)))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn fps_flag() {
        let (f, w) = flags_from_pairs(&[]);
        assert!(!f.fps_on && w.is_empty(), "hidden by default");
        let (f, w) = flags_from_pairs(&p(&[("fps", "on")]));
        assert!(f.fps_on && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("fps", "on"), ("fps", "off")]));
        assert!(!f.fps_on && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("fps", "maybe")]));
        assert!(!f.fps_on);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn frame_cap_flag() {
        use crate::frame_cap::FrameCap;
        let (f, w) = flags_from_pairs(&[]);
        assert_eq!(f.frame_cap, FrameCap(None), "off by default");
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("frame_cap", "60")]));
        assert_eq!(f.frame_cap, FrameCap(Some(60)));
        assert!(w.is_empty());
        let (f, _) = flags_from_pairs(&p(&[("frame-cap", "120"), ("frame_cap", "off")]));
        assert_eq!(f.frame_cap, FrameCap(None));
        let (f, w) = flags_from_pairs(&p(&[("frame_cap", "75")]));
        assert_eq!(f.frame_cap, FrameCap(None));
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn grass_distance_flag() {
        let (f, w) = flags_from_pairs(&[]);
        assert_eq!(f.gate.grass_distance.hundredths(), 100, "Near by default");
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("grass_distance", "far")]));
        assert_eq!(f.gate.grass_distance.hundredths(), 200);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("grass-distance", "1.5")]));
        assert_eq!(f.gate.grass_distance.hundredths(), 150);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("grass_distance", "farther")]));
        assert_eq!(f.gate.grass_distance.hundredths(), 100);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn kit_scenario_and_hooks() {
        let (f, w) = flags_from_pairs(&p(&[("shot", "kit"), ("kit_hooks", "snow")]));
        assert_eq!(f.scenario, Scenario::Kit);
        assert_eq!(f.gate.kit_hooks, hex_render::KitHooks::Snow);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("kit_hooks", "hail")]));
        assert_eq!(f.gate.kit_hooks, hex_render::KitHooks::None);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn kit_source_parses_or_warns() {
        let (f, w) = flags_from_pairs(&p(&[("scenario", "kit"), ("kit_src", "fixture")]));
        assert_eq!(f.gate.kit_src, hex_render::KitSource::Fixture);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("kit_src", "p0")]));
        assert_eq!(f.gate.kit_src, hex_render::KitSource::Runtime);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn water_keys() {
        let (f, w) = flags_from_pairs(&p(&[
            ("scenario", "water"),
            ("tier", "low"),
            ("water_clock", "fixed"),
        ]));
        assert_eq!(f.scenario, Scenario::Water);
        assert_eq!(f.gate.tier, hex_render::post::QualityTier::Low);
        assert!(f.gate.water_fixed && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("tier", "epic"), ("water_clock", "x")]));
        assert_eq!(f.gate.tier, hex_render::post::QualityTier::High);
        assert!(!f.gate.water_fixed);
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn tier_key_is_explicit_only_when_valid() {
        let (f, _) = flags_from_pairs(&[]);
        assert!(!f.tier_explicit, "no key: auto-detect");
        let (f, _) = flags_from_pairs(&p(&[("tier", "high")]));
        assert!(f.tier_explicit, "even the default value, when named, wins");
        let (f, w) = flags_from_pairs(&p(&[("tier", "epic")]));
        assert!(!f.tier_explicit && w.len() == 1);
    }

    #[test]
    fn debug_key() {
        let (f, w) = flags_from_pairs(&[]);
        assert_eq!(f.debug, hex_play::debug::DebugOverlays::default());
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("debug", "grid,lod,lod1,perf")]));
        assert!(f.debug.grid && f.debug.lod_tint && f.debug.perf && !f.debug.sockets);
        assert_eq!(f.debug.force_lod, Some(1));
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("debug", "sockets,wireframe")]));
        assert!(f.debug.sockets && w.len() == 1, "{w:?}");
    }

    #[test]
    fn gpu_timing_key() {
        let (f, w) = flags_from_pairs(&[]);
        assert!(!f.gpu_timing && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("gpu_timing", "on")]));
        assert!(f.gpu_timing && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("gpu_timing", "yes")]));
        assert!(!f.gpu_timing && w.len() == 1);
    }

    #[test]
    fn offscreen_key() {
        let (f, w) = flags_from_pairs(&[]);
        assert!(!f.offscreen && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("offscreen", "on")]));
        assert!(f.offscreen && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("offscreen", "maybe")]));
        assert!(!f.offscreen && !f.offscreen_explicit && w.len() == 1);
    }

    #[test]
    fn capture_runs_render_offscreen_unless_told_otherwise() {
        let run = |pairs: &[(&str, &str)], script_shoots: bool| {
            let (mut f, _) = flags_from_pairs(&p(pairs));
            let implicit = f.capture_default(script_shoots);
            (f.offscreen, implicit)
        };
        assert_eq!(
            run(&[], false),
            (false, false),
            "a plain run draws to the window"
        );
        assert_eq!(
            run(&[("script", "perf.json")], false),
            (false, false),
            "a script without screenshots (a perf run) keeps the window"
        );
        assert_eq!(run(&[("script", "s.json")], true), (true, true));
        assert_eq!(run(&[("shot", "single")], false), (true, true));
        assert_eq!(run(&[("screenshot-after", "2")], false), (true, true));
        assert_eq!(run(&[("thumbs", "")], false), (true, true));
        assert_eq!(
            run(&[("shot", "single"), ("offscreen", "off")], false),
            (false, false),
            "an explicit off captures the window"
        );
        assert_eq!(
            run(&[("offscreen", "on")], false),
            (true, false),
            "an explicit on is not a capture default (unpaced)"
        );
        assert_eq!(
            run(&[("shot", "single"), ("offscreen", "on")], false),
            (true, false)
        );
    }

    #[test]
    fn tier_warm_flag() {
        let (f, w) = flags_from_pairs(&[]);
        assert!(!f.gate.tier_warm, "off by default (owner A33 O2)");
        let (f, w2) = flags_from_pairs(&p(&[("tier_warm", "on")]));
        assert!(f.gate.tier_warm && w.is_empty() && w2.is_empty());
        let (f, _) = flags_from_pairs(&p(&[("tier_warm", "off")]));
        assert!(!f.gate.tier_warm);
        let (_, w) = flags_from_pairs(&p(&[("tier_warm", "maybe")]));
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn kit_ablate_key() {
        use hex_render::kit_instances::KitAblate;
        let (f, w) = flags_from_pairs(&[]);
        assert!(f.kit_ablate == KitAblate::default() && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("kit_ablate", "coalesce")]));
        assert!(f.kit_ablate.coalesce && !f.kit_ablate.cascade_clip && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("kit_ablate", "cascade_clip,coalesce")]));
        assert!(f.kit_ablate.coalesce && f.kit_ablate.cascade_clip && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("kit_ablate", "grass")]));
        assert!(f.kit_ablate == KitAblate::default() && w.len() == 1);
    }

    #[test]
    fn clock_key() {
        let (f, w) = flags_from_pairs(&[]);
        assert!(!f.clock_fixed && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("clock", "fixed")]));
        assert!(f.clock_fixed && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("clock", "fixed"), ("clock", "sometimes")]));
        assert!(
            f.clock_fixed && w.len() == 1,
            "a bad value keeps the previous one"
        );
    }

    #[test]
    fn grade_sets_tone_map_and_exposure_unless_their_keys_override() {
        let (f, w) = flags_from_pairs(&p(&[("grade", "aces")]));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(f.gate.grade, hex_render::Grade::Aces);
        assert_eq!(f.gate.tonemap, hex_render::Tonemap::AcesFitted);
        assert_eq!(f.gate.ev100.get(), 12.5);
        let (f, _) = flags_from_pairs(&p(&[("ev100", "13"), ("grade", "aces")]));
        assert_eq!(
            (f.gate.tonemap, f.gate.ev100.get()),
            (hex_render::Tonemap::AcesFitted, 13.0)
        );
        let (f, _) = flags_from_pairs(&p(&[("grade", "rich-2"), ("tonemap", "agx")]));
        assert_eq!(
            (f.gate.grade, f.gate.tonemap),
            (hex_render::Grade::Rich2, hex_render::Tonemap::AgX)
        );
        let (f, w) = flags_from_pairs(&p(&[("grade", "vivid")]));
        assert_eq!((f.gate.grade, w.len()), (hex_render::Grade::Rich3, 1));
        assert_eq!(
            (f.gate.tonemap, f.gate.ev100.get()),
            (hex_render::Tonemap::AcesFitted, 12.5)
        );
    }

    #[test]
    fn tonemap_and_ibl_keys() {
        let (f, w) = flags_from_pairs(&p(&[("tonemap", "agx"), ("ibl", "off")]));
        assert_eq!(f.gate.tonemap, hex_render::Tonemap::AgX);
        assert!(!f.gate.ibl && w.is_empty());
    }

    #[test]
    fn ev100_key() {
        let (f, w) = flags_from_pairs(&p(&[("ev100", "13.3")]));
        assert_eq!(f.gate.ev100.get(), 13.3);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("ev100", "dark")]));
        assert_eq!(f.gate.ev100, hex_render::GateConfig::default().ev100);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn tilt_keys() {
        let (f, w) = flags_from_pairs(&p(&[("tilt", "on"), ("tilt_strength", "0.5")]));
        assert_eq!(f.gate.tilt, hex_render::TiltMode::On);
        assert_eq!(f.gate.tilt_strength.get(), 0.5);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("tilt", "maybe"), ("tilt_strength", "2")]));
        assert_eq!(f.gate.tilt, hex_render::TiltMode::Off);
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn defaults() {
        let (f, w) = flags_from_pairs(&[]);
        assert_eq!(f.scenario, Scenario::Single);
        assert!(f.shot.is_none() && !f.diag && w.is_empty());
    }

    #[test]
    fn web_default_is_play_and_others_stay_reachable() {
        let web = Scenario::Play;
        let (f, w) = flags_from_pairs_or(&p(&[("fps", "off")]), web);
        assert_eq!(f.scenario, Scenario::Play);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs_or(&p(&[("shot", "nope")]), web);
        assert_eq!(
            f.scenario,
            Scenario::Play,
            "an unknown name falls back to the default"
        );
        assert_eq!(w.len(), 1);
        for (name, s) in [
            ("single", Scenario::Single),
            ("gate2000", Scenario::Gate2000),
            ("kit", Scenario::Kit),
            ("water", Scenario::Water),
            ("water_kit", Scenario::WaterKit),
            ("play", Scenario::Play),
        ] {
            let (f, w) = flags_from_pairs_or(&p(&[("scenario", name)]), web);
            assert_eq!(f.scenario, s, "?scenario={name}");
            assert!(w.is_empty());
        }
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
    fn view_parses_or_warns_and_falls_back() {
        let (f, w) = flags_from_pairs(&p(&[("view", "closeup")]));
        assert_eq!(f.view, GateView::Closeup);
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("view", "bogus")]));
        assert_eq!(f.view, GateView::Strategic);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("bogus"));
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

    #[test]
    fn size_flag_parses() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size("19x"), None);
    }

    #[test]
    fn size_flag_parses_or_warns() {
        let (f, w) = flags_from_pairs(&p(&[("size", "1280x800")]));
        assert_eq!(f.size, Some((1280, 800)));
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("size", "0x5")]));
        assert_eq!(f.size, None);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn seconds_flags_parse_or_warn() {
        let (f, w) = flags_from_pairs(&p(&[
            ("screenshot-after", "2.5"),
            ("exit-after-seconds", "4"),
        ]));
        assert_eq!(f.screenshot_after_secs, Some(2.5));
        assert_eq!(f.exit_after_secs, Some(4.0));
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[
            ("exit-after-seconds", "-1"),
            ("screenshot-after", "x"),
        ]));
        assert_eq!(f.exit_after_secs, None);
        assert_eq!(f.screenshot_after_secs, None);
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn gate_config_keys_parse() {
        let (f, w) = flags_from_pairs(&p(&[
            ("layout", "chunks"),
            ("post", "none"),
            ("shadows", "off"),
            ("chunk", "19"),
            ("present", "novsync"),
        ]));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(f.gate.layout, Layout::Chunks);
        assert_eq!(f.gate.post, PostMode::None);
        assert!(!f.gate.shadows);
        assert_eq!(f.gate.chunk, 19);
        assert_eq!(f.present, Present::NoVsync);
    }

    #[test]
    fn shadow_tuning_and_instanced_keys_parse() {
        let (f, w) = flags_from_pairs(&p(&[
            ("layout", "instanced"),
            ("shadow_casters", "large"),
            ("cascades", "3"),
            ("shadow_dist", "800"),
        ]));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(f.gate.layout, Layout::Instanced);
        assert_eq!(f.gate.shadow_casters, ShadowCasters::Large);
        assert_eq!(f.gate.cascades, 3);
        assert_eq!(f.gate.shadow_dist, 800);
        let (f, w) = flags_from_pairs(&p(&[
            ("shadow_casters", "few"),
            ("cascades", "9"),
            ("shadow_dist", "50"),
        ]));
        assert_eq!(w.len(), 3, "{w:?}");
        assert_eq!(f.gate, GateConfig::default());
    }

    #[test]
    fn gate_config_defaults() {
        let (f, _) = flags_from_pairs(&[]);
        assert_eq!(f.gate.layout, Layout::Entities);
        assert_eq!(f.gate.post, PostMode::Full);
        assert!(f.gate.shadows);
        assert_eq!(f.gate.chunk, 7);
        assert_eq!(f.present, Present::Fifo);
    }

    #[test]
    fn gate_config_bad_values_warn_and_fall_back() {
        let (f, w) = flags_from_pairs(&p(&[
            ("layout", "blob"),
            ("post", "half"),
            ("shadows", "maybe"),
            ("chunk", "8"),
            ("present", "fast"),
        ]));
        assert_eq!(w.len(), 5, "{w:?}");
        for (warning, bad) in w.iter().zip(["blob", "half", "maybe", "8", "fast"]) {
            assert!(warning.contains(bad), "{warning}");
        }
        assert_eq!(f.gate, GateConfig::default());
        assert_eq!(f.present, Present::Fifo);
        let (f, w) = flags_from_pairs(&p(&[
            ("layout", "entities"),
            ("shadows", "on"),
            ("chunk", "7"),
        ]));
        assert!(w.is_empty());
        assert_eq!(f.gate, GateConfig::default());
    }

    #[test]
    fn seed_no_anim_script_dump_state() {
        let (f, w) = flags_from_pairs(&p(&[
            ("seed", "42"),
            ("no-anim", "1"),
            ("script", "s.json"),
            ("dump-state", "out.json"),
        ]));
        assert_eq!(f.seed, 42);
        assert!(f.no_anim);
        assert_eq!(f.script.as_deref(), Some("s.json"));
        assert_eq!(f.dump_state.as_deref(), Some("out.json"));
        assert!(w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("seed", "-3")]));
        assert_eq!(f.seed, 0);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn photo_export_takes_one_two_or_four() {
        let (f, w) = flags_from_pairs(&p(&[("photo-export", "4")]));
        assert_eq!((f.photo_export, w.len()), (Some(4), 0));
        let (f, w) = flags_from_pairs(&p(&[("photo-export", "3")]));
        assert_eq!((f.photo_export, w.len()), (None, 1));
    }
}
