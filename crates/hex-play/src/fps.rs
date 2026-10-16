//! On-screen FPS counter: Bevy UI text in the top-right corner, F3 toggles it.
//!
//! The text node is a persistent UI entity, deliberately NOT a `SceneEntity`: the harness
//! `scenario` command despawns every `SceneEntity` and rebuilds the scene, and the counter
//! must survive that. It follows the primary camera, which `scenario` respawns.
use std::collections::VecDeque;

use bevy::prelude::*;

use crate::diag::FrameStats;

/// Length of the rolling window behind `fps` and `frame_ms`.
const WINDOW_MS: f32 = 1000.0;
/// The label is rebuilt at this interval so text layout stays off the per-frame path.
const REFRESH_SECS: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FpsReadout {
    pub fps: f32,
    pub frame_ms: f32,
    pub p95_ms: f32,
}

impl FpsReadout {
    /// `window` is the recent frame times in ms (about 1 s); `stats` supplies the p95.
    pub fn compute(window: &[f32], stats: &FrameStats) -> FpsReadout {
        if window.is_empty() {
            return FpsReadout::default();
        }
        let frame_ms = window.iter().sum::<f32>() / window.len() as f32;
        FpsReadout {
            fps: if frame_ms > 0.0 {
                1000.0 / frame_ms
            } else {
                0.0
            },
            frame_ms,
            p95_ms: stats.p95(),
        }
    }

    /// `60 fps | 16.4 ms | p95 17.1`. ASCII separator: Bevy's `default_font` subset has no U+00B7.
    pub fn label(&self) -> String {
        format!(
            "{:.0} fps | {:.1} ms | p95 {:.1}",
            self.fps, self.frame_ms, self.p95_ms
        )
    }
}

#[derive(Resource)]
struct FpsState {
    window: VecDeque<f32>,
    stats: FrameStats,
    since_refresh: f32,
    visible: bool,
}

#[derive(Component)]
struct FpsText;

/// `visible` is the initial state (`fps=on|off`); F3 flips it at runtime. While hidden no UI
/// node exists and the only per-frame work is pushing the frame time into `FrameStats`.
pub struct FpsPlugin {
    pub visible: bool,
}

impl Plugin for FpsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FpsState {
            window: VecDeque::new(),
            stats: FrameStats::default(),
            since_refresh: REFRESH_SECS,
            visible: self.visible,
        })
        .add_systems(Update, (toggle, update).chain());
    }
}

fn spawn_text(commands: &mut Commands, label: String) {
    commands.spawn((
        FpsText,
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::WHITE),
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            top: px(6),
            right: px(8),
            ..default()
        },
    ));
}

fn toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut st: ResMut<FpsState>,
    nodes: Query<Entity, With<FpsText>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        st.visible = !st.visible;
        if st.visible {
            // Hidden frames leave the 1 s window stale; restart it and refresh at once.
            st.window.clear();
            st.since_refresh = REFRESH_SECS;
        } else {
            nodes.iter().for_each(|e| commands.entity(e).despawn());
        }
    }
}

fn update(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut st: ResMut<FpsState>,
    mut text: Query<&mut Text, With<FpsText>>,
) {
    let ms = time.delta_secs() * 1000.0;
    st.stats.push(ms);
    if !st.visible {
        return;
    }
    st.window.push_back(ms);
    while st.window.len() > 1 && st.window.iter().sum::<f32>() - st.window[0] >= WINDOW_MS {
        st.window.pop_front();
    }
    st.since_refresh += time.delta_secs();
    if st.since_refresh < REFRESH_SECS {
        return;
    }
    st.since_refresh = 0.0;
    let w = st.window.make_contiguous().to_vec();
    let label = FpsReadout::compute(&w, &st.stats).label();
    match text.single_mut() {
        Ok(mut t) => t.0 = label,
        Err(_) => {
            spawn_text(&mut commands, label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_16ms_is_62_5_fps() {
        let mut stats = FrameStats::default();
        let w = vec![16.0_f32; 60];
        w.iter().for_each(|&m| stats.push(m));
        let r = FpsReadout::compute(&w, &stats);
        assert_eq!(r.fps, 62.5);
        assert_eq!(r.frame_ms, 16.0);
        assert_eq!(r.p95_ms, 16.0);
    }

    #[test]
    fn empty_window_is_zero() {
        let r = FpsReadout::compute(&[], &FrameStats::default());
        assert_eq!(r, FpsReadout::default());
        assert_eq!(r.fps, 0.0);
    }

    #[test]
    fn spike_shows_in_p95_not_mean_fps() {
        let mut stats = FrameStats::default();
        for i in 0..100 {
            stats.push(if i < 90 { 16.0 } else { 50.0 });
        }
        let r = FpsReadout::compute(&[16.0; 60], &stats);
        assert_eq!(r.p95_ms, 50.0);
        assert_eq!(r.fps, 62.5);
    }

    #[test]
    fn label_format() {
        let r = FpsReadout {
            fps: 60.2,
            frame_ms: 16.43,
            p95_ms: 17.06,
        };
        assert_eq!(r.label(), "60 fps | 16.4 ms | p95 17.1");
    }
}
