//! Named look presets ("grades", owner 2026-10-14): the tone mapper, exposure, colour grading and
//! backdrop every kit, water and play camera reads. `--grade`, `?grade=` and the `set_grade` command
//! pick one; a live switch rewrites the cameras and the clear colour without rebuilding the scene.
//! Measurements: `docs/checkpoints/grades/`.
use bevy::{
    camera::Exposure,
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
    render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection},
};

use crate::workload::{Ev100, GateConfig, Tonemap};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Grade {
    /// AcesFitted at EV100 12.5 with Bevy's default clear colour: the look before 2026-10-14.
    Aces,
    /// TonyMcMapface at EV100 12.9 (owner, 2026-10-14, Q4): the default until `rich-3` replaced it.
    Tony,
    Rich1,
    Rich2,
    /// The default (owner, 2026-10-14, after the grade comparison).
    #[default]
    Rich3,
}

/// Bevy's default `ClearColor`, which read about 15/255 under ACES.
const ACES_BACKDROP: Color = Color::srgb_u8(43, 44, 47);

/// Shadow lift of `rich-3`, added before the ACES curve. Fitted ACES outputs black below about
/// 0.0033 linear and the shaded soil walls sit near 0.0045 at EV100 12.5; 0.013 puts them at
/// about 0.017, which the curve maps to about 8/255.
const RICH3_LIFT: f32 = 0.013;

/// Midtone pivot of `rich-2`'s contrast: about the grass at EV100 12.9.
const RICH2_PIVOT: f32 = 0.18;
/// `rich-2`'s exponent is 1 / 0.926 = 1.08: the walls (about 0.0034 before the curve) fall to
/// about 73%, about 9/255 under TonyMcMapface.
const RICH2_GAMMA: f32 = 0.926;

impl Grade {
    pub const ALL: [Grade; 5] = [
        Grade::Aces,
        Grade::Tony,
        Grade::Rich1,
        Grade::Rich2,
        Grade::Rich3,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Grade::Aces => "aces",
            Grade::Tony => "tony",
            Grade::Rich1 => "rich-1",
            Grade::Rich2 => "rich-2",
            Grade::Rich3 => "rich-3",
        }
    }

    pub fn parse(s: &str) -> Option<Grade> {
        Grade::ALL.into_iter().find(|g| g.name() == s)
    }

    /// One line for the Settings sheet and the comparison page.
    pub fn detail(self) -> &'static str {
        match self {
            Grade::Aces => "The first look: deep colour, but shaded walls go black",
            Grade::Tony => "Every wall readable, softer colour",
            Grade::Rich1 => "Tony with a fifth more colour",
            Grade::Rich2 => "Tony with ACES-like contrast and colour, walls still readable",
            Grade::Rich3 => "ACES colour with its shadows lifted out of black",
        }
    }

    pub fn tonemap(self) -> Tonemap {
        match self {
            Grade::Aces | Grade::Rich3 => Tonemap::AcesFitted,
            Grade::Tony | Grade::Rich1 | Grade::Rich2 => Tonemap::TonyMcMapface,
        }
    }

    pub fn ev100(self) -> f32 {
        match self {
            Grade::Aces | Grade::Rich3 => 12.5,
            Grade::Tony | Grade::Rich1 | Grade::Rich2 => crate::lookdev::KIT_EV100,
        }
    }

    /// Bevy grades before the tone curve, on exposed linear values, except `post_saturation`.
    /// Section contrast pivots at 0.5 linear, so any value above 1 drives the walls (about 0.004)
    /// negative; no preset uses it. Bevy blends the shadow/midtone boundary into the highlights
    /// section, so shadows and highlights always carry the same values here.
    pub fn grading(self) -> ColorGrading {
        let section = ColorGradingSection::default();
        match self {
            Grade::Aces | Grade::Tony => ColorGrading::default(),
            Grade::Rich1 => ColorGrading::with_identical_sections(
                ColorGradingGlobal {
                    post_saturation: 1.2,
                    ..default()
                },
                section,
            ),
            // Log contrast about RICH2_PIVOT: (c * gain)^(1 / gamma) leaves the pivot in place,
            // darkens what lies below it and brightens what lies above.
            Grade::Rich2 => ColorGrading::with_identical_sections(
                ColorGradingGlobal {
                    post_saturation: 1.12,
                    ..default()
                },
                ColorGradingSection {
                    gamma: RICH2_GAMMA,
                    gain: RICH2_PIVOT.powf(RICH2_GAMMA - 1.0),
                    ..section
                },
            ),
            Grade::Rich3 => {
                let lifted = ColorGradingSection {
                    lift: RICH3_LIFT,
                    ..section
                };
                ColorGrading {
                    global: ColorGradingGlobal::default(),
                    shadows: lifted,
                    midtones: section,
                    highlights: lifted,
                }
            }
        }
    }

    /// The clear colour, tone-mapped like the scene: each value reads about 15/255 under its grade.
    pub fn backdrop(self) -> Color {
        match self {
            Grade::Aces => ACES_BACKDROP,
            Grade::Tony | Grade::Rich1 => crate::lookdev::BACKDROP,
            // `lookdev::BACKDROP` before rich-2's contrast curve.
            Grade::Rich2 => Color::linear_rgb(0.0046, 0.0047, 0.0058),
            Grade::Rich3 => Color::linear_rgb(0.0110, 0.0118, 0.0142),
        }
    }
}

/// The scenarios whose camera carries the look-dev components.
pub fn graded(s: crate::Scenario) -> bool {
    s == crate::Scenario::Kit || s.has_water() || s == crate::Scenario::Play
}

/// Switches the grade live: `GateConfig` (so a rebuild and the time of day keep it), every 3D
/// camera's tone mapper, exposure and grading, and the clear colour. Exposure moves by the change
/// in base EV, which keeps photo-mode and time-of-day offsets; that change is returned.
pub fn apply(world: &mut World, grade: Grade) -> f32 {
    let before = world.resource::<GateConfig>().ev100.get();
    {
        let mut c = world.resource_mut::<GateConfig>();
        c.grade = grade;
        c.tonemap = grade.tonemap();
        c.ev100 = Ev100::from_f32(grade.ev100());
    }
    let after = world.resource::<GateConfig>().ev100.get();
    if !graded(world.resource::<crate::ActiveScenario>().0) {
        return 0.0;
    }
    world.insert_resource(ClearColor(grade.backdrop()));
    let cams: Vec<Entity> = world
        .query_filtered::<Entity, With<Camera3d>>()
        .iter(world)
        .collect();
    for e in cams {
        let mut ent = world.entity_mut(e);
        ent.insert((
            crate::lookdev::tonemapping(grade.tonemap()),
            grade.grading(),
        ));
        if let Some(mut x) = ent.get_mut::<Exposure>() {
            x.ev100 += after - before;
        }
    }
    info!("grade: {}", grade.name());
    after - before
}

/// The grade's camera components, inserted by `scene::setup`.
pub fn camera_bundle(config: &GateConfig) -> (Tonemapping, Exposure, ColorGrading) {
    (
        crate::lookdev::tonemapping(config.tonemap),
        crate::lookdev::exposure(config.ev100),
        config.grade.grading(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_rich3_is_the_default() {
        for g in Grade::ALL {
            assert_eq!(Grade::parse(g.name()), Some(g));
            assert!(!g.detail().is_empty());
        }
        assert_eq!(Grade::parse("filmic"), None);
        assert_eq!(Grade::default(), Grade::Rich3);
        assert_eq!(GateConfig::default().grade, Grade::default());
    }

    #[test]
    fn rich3_is_the_shipped_look_and_tony_and_aces_the_earlier_ones() {
        let d = GateConfig::default();
        assert_eq!(Grade::Rich3.tonemap(), d.tonemap);
        assert_eq!(Grade::Rich3.ev100(), d.ev100.get());
        assert_eq!(
            (Grade::Rich3.tonemap(), Grade::Rich3.ev100()),
            (Tonemap::AcesFitted, 12.5)
        );
        assert_eq!(Grade::Rich3.grading().shadows.lift, RICH3_LIFT);
        assert_eq!(Grade::Rich3.grading().midtones.lift, 0.0);
        assert_eq!(
            (Grade::Tony.tonemap(), Grade::Tony.ev100()),
            (Tonemap::TonyMcMapface, crate::lookdev::KIT_EV100)
        );
        assert_eq!(Grade::Tony.grading(), ColorGrading::default());
        assert_eq!(Grade::Tony.backdrop(), crate::lookdev::BACKDROP);
        assert_eq!(
            (Grade::Aces.tonemap(), Grade::Aces.ev100()),
            (Tonemap::AcesFitted, 12.5)
        );
        assert_eq!(Grade::Aces.grading(), ColorGrading::default());
    }

    #[test]
    fn no_grade_uses_section_contrast_and_outer_sections_match() {
        for g in Grade::ALL {
            let c = g.grading();
            for s in [c.shadows, c.midtones, c.highlights] {
                assert_eq!(s.contrast, 1.0, "{}", g.name());
            }
            assert_eq!(c.shadows, c.highlights, "{}", g.name());
        }
    }
}
