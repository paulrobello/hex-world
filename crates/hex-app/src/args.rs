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
    /// `fps=off` starts with the FPS counter hidden (default on; F3 toggles at runtime).
    pub fps_off: bool,
    pub dump_state: Option<String>,
    /// `clock=fixed`: every Bevy clock advances exactly 1/60 s per frame, so the 20 Hz car tick
    /// lands on every third frame and scripted runs replay identically.
    pub clock_fixed: bool,
    /// Native only: render the tile thumbnails and exit (`thumbs::force_flags` sets the rest).
    pub thumbs: bool,
    /// A valid `tier` key was given: it wins over auto-detection (`hex_render::tier`).
    pub tier_explicit: bool,
    /// `gpu_timing=on`: per-pass GPU timestamps in the diag output (`hex_render::gpu_timing`).
    pub gpu_timing: bool,
    /// `photo-export=1|2|4`: once ready, export a photo-mode PNG at that scale, named
    /// `photo-<n>x` (natively `target/shots/photo-<n>x.png`).
    pub photo_export: Option<u32>,
    /// Native only: `.hexworld` file loaded before any script command (PRD §12.2).
    pub load: Option<String>,
    /// Native only: `.hexworld` file written with the world on exit.
    pub save: Option<String>,
    /// `debug=lod,perf,grid,sockets,colliders,lod2`: developer overlays on from the start (never
    /// persisted; `hex_render::debug`).
    pub debug: hex_render::debug::DebugOverlays,
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
    let mut f = Flags {
        scenario: default,
        ..Flags::default()
    };
    let mut warnings = vec![];
    // `tonemap` and `ev100` override the grade's own values whatever the key order.
    let (mut grade, mut tonemap_set, mut ev100_set) = (None, false, false);
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
            "view" => match GateView::parse(v) {
                Some(view) => f.view = view,
                None => warnings.push(format!("unknown view '{v}', using default")),
            },
            "diag" => f.diag = v != "0",
            "exit-after" => match v.parse() {
                Ok(n) => f.exit_after_frames = Some(n),
                Err(_) => warnings.push(format!("bad exit-after '{v}'")),
            },
            "script" => f.script = Some(v.clone()),
            "thumbs" => f.thumbs = v != "0",
            "dump-state" => f.dump_state = Some(v.clone()),
            "load" => f.load = Some(v.clone()),
            "save" => f.save = Some(v.clone()),
            "screenshot-after" => f.screenshot_after_secs = parse_secs(k, v, &mut warnings),
            "exit-after-seconds" => f.exit_after_secs = parse_secs(k, v, &mut warnings),
            "size" => match parse_size(v) {
                Some(s) => f.size = Some(s),
                None => warnings.push(format!("bad size '{v}', expected WxH")),
            },
            "seed" => match v.parse() {
                Ok(n) => f.seed = n,
                Err(_) => warnings.push(format!("bad seed '{v}'")),
            },
            "no-anim" => f.no_anim = v != "0",
            "fps" => match v.as_str() {
                "on" => f.fps_off = false,
                "off" => f.fps_off = true,
                _ => warnings.push(format!("unknown fps '{v}', using default")),
            },
            "layout" => set_or_warn(&mut f.gate.layout, Layout::parse(v), k, v, &mut warnings),
            "post" => set_or_warn(&mut f.gate.post, PostMode::parse(v), k, v, &mut warnings),
            "shadows" => set_or_warn(
                &mut f.gate.shadows,
                hex_render::parse_shadows(v),
                k,
                v,
                &mut warnings,
            ),
            "chunk" => set_or_warn(
                &mut f.gate.chunk,
                hex_render::parse_chunk(v),
                k,
                v,
                &mut warnings,
            ),
            "shadow_casters" => set_or_warn(
                &mut f.gate.shadow_casters,
                ShadowCasters::parse(v),
                k,
                v,
                &mut warnings,
            ),
            "cascades" => set_or_warn(
                &mut f.gate.cascades,
                hex_render::parse_cascades(v),
                k,
                v,
                &mut warnings,
            ),
            "shadow_dist" => set_or_warn(
                &mut f.gate.shadow_dist,
                hex_render::parse_shadow_dist(v),
                k,
                v,
                &mut warnings,
            ),
            "kit_hooks" => set_or_warn(
                &mut f.gate.kit_hooks,
                hex_render::KitHooks::parse(v),
                k,
                v,
                &mut warnings,
            ),
            "kit_src" => set_or_warn(
                &mut f.gate.kit_src,
                hex_render::KitSource::parse(v),
                k,
                v,
                &mut warnings,
            ),
            "tonemap" => {
                let t = hex_render::Tonemap::parse(v);
                tonemap_set |= t.is_some();
                set_or_warn(&mut f.gate.tonemap, t, k, v, &mut warnings);
            }
            "grade" => match hex_render::Grade::parse(v) {
                Some(g) => grade = Some(g),
                None => warnings.push(format!("unknown grade '{v}', using default")),
            },
            "ibl" => set_or_warn(
                &mut f.gate.ibl,
                hex_render::parse_ibl(v),
                k,
                v,
                &mut warnings,
            ),
            "ev100" => {
                let e = hex_render::Ev100::parse(v);
                ev100_set |= e.is_some();
                set_or_warn(&mut f.gate.ev100, e, k, v, &mut warnings);
            }
            "tilt" => set_or_warn(
                &mut f.gate.tilt,
                hex_render::TiltMode::parse(v),
                k,
                v,
                &mut warnings,
            ),
            "tier" => {
                let t = hex_render::post::QualityTier::parse(v);
                f.tier_explicit |= t.is_some();
                set_or_warn(&mut f.gate.tier, t, k, v, &mut warnings);
            }
            "gpu_timing" => set_or_warn(
                &mut f.gpu_timing,
                hex_render::parse_shadows(v),
                k,
                v,
                &mut warnings,
            ),
            "tilt_strength" => set_or_warn(
                &mut f.gate.tilt_strength,
                hex_render::TiltStrength::parse(v),
                k,
                v,
                &mut warnings,
            ),
            "grass" => set_or_warn(
                &mut f.gate.grass,
                hex_render::parse_grass(v),
                k,
                v,
                &mut warnings,
            ),
            "water_clock" => set_or_warn(
                &mut f.gate.water_fixed,
                match v.as_str() {
                    "fixed" => Some(true),
                    "real" => Some(false),
                    _ => None,
                },
                k,
                v,
                &mut warnings,
            ),
            "clock" => set_or_warn(
                &mut f.clock_fixed,
                match v.as_str() {
                    "fixed" => Some(true),
                    "real" => Some(false),
                    _ => None,
                },
                k,
                v,
                &mut warnings,
            ),
            "present" => set_or_warn(&mut f.present, Present::parse(v), k, v, &mut warnings),
            "debug" => {
                let (d, unknown) = hex_render::debug::DebugOverlays::parse_list(v);
                f.debug = d;
                for u in unknown {
                    warnings.push(format!("unknown debug overlay '{u}'"));
                }
            }
            "photo-export" => match v.parse::<u32>() {
                Ok(n) if crate::photo::EXPORT_SCALES.contains(&n) => f.photo_export = Some(n),
                _ => warnings.push(format!("bad photo-export '{v}', expected 1, 2 or 4")),
            },
            _ => {}
        }
    }
    if let Some(g) = grade {
        f.gate.grade = g;
        if !tonemap_set {
            f.gate.tonemap = g.tonemap();
        }
        if !ev100_set {
            f.gate.ev100 = hex_render::Ev100::from_f32(g.ev100());
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
        "gpu_timing",
        "water_clock",
        "clock",
        "grass",
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
        assert!(!f.fps_off && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("fps", "off")]));
        assert!(f.fps_off && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("fps", "off"), ("fps", "on")]));
        assert!(!f.fps_off && w.is_empty());
        let (f, w) = flags_from_pairs(&p(&[("fps", "maybe")]));
        assert!(!f.fps_off);
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
        assert_eq!(f.debug, hex_render::debug::DebugOverlays::default());
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
