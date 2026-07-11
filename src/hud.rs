//! The in-world HUD.
//!
//! Implements the "one system, three states" chrome from the spec (1e / 1r):
//! **Visible** during input and for a few seconds after, **Minimized** to a
//! breathing hairline once idle, then **Hidden** entirely — world only. Any
//! input wakes it. The single variable is the accent, sampled from the world.

use bevy::prelude::*;

use crate::playback::{Beat, Playback, fmt_time};
use crate::theme::{self, Fonts, RADIUS_PILL, RADIUS_SM, TEXT, Theme, text_font};
use crate::{CameraMode, Comfort};

// ---------------------------------------------------------------------------
// HUD depth + activity
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HudDepth {
    Visible,
    Minimized,
    Hidden,
}

/// Idle tracking + the two eased global alphas that drive the fades.
#[derive(Resource)]
pub struct HudActivity {
    pub idle: f32,
    pub depth: HudDepth,
    /// Alpha for "full" chrome — 1 only while Visible.
    pub full: f32,
    /// Alpha for "minimal" chrome — 1 while Visible or Minimized.
    pub minimal: f32,
    /// Set true for one wake to force the timers back to zero.
    pub force_visible: bool,
}

impl Default for HudActivity {
    fn default() -> Self {
        Self {
            idle: 0.0,
            depth: HudDepth::Visible,
            full: 1.0,
            minimal: 1.0,
            force_visible: true,
        }
    }
}

impl HudActivity {
    /// Any input calls this — snap awake.
    pub fn wake(&mut self) {
        self.idle = 0.0;
        self.force_visible = true;
    }
}

// ---------------------------------------------------------------------------
// Fade plumbing — Bevy UI has no opacity inheritance, so each coloured node
// carries its intrinsic colour and the group whose global alpha scales it.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum FadeGroup {
    /// Visible only.
    Full,
    /// Visible + Minimized (world name, progress hairline).
    Minimal,
}

#[derive(Clone, Copy)]
pub enum FadeTarget {
    Bg,
    Text,
}

#[derive(Component)]
pub struct Fade {
    pub group: FadeGroup,
    pub base: Color,
    pub target: FadeTarget,
}

/// Marks a node whose `Fade.base` tracks the current accent.
#[derive(Component)]
pub struct AccentTint;

// Dynamic-content markers. `pub(crate)` because they appear in the signatures
// of the `pub` systems below (scheduled from `main`).
#[derive(Component)]
pub(crate) struct WorldNameText;
#[derive(Component)]
pub(crate) struct TrackTitleText;
#[derive(Component)]
pub(crate) struct SectionText;
#[derive(Component)]
pub(crate) struct TimeElapsedText;
#[derive(Component)]
pub(crate) struct TimeTotalText;
#[derive(Component)]
pub(crate) struct NextTrackText;
#[derive(Component)]
pub(crate) struct ProgressFill;
#[derive(Component)]
pub(crate) struct ProgressPlayhead;
#[derive(Component)]
pub(crate) struct ModeTab(CameraMode);
#[derive(Component)]
pub(crate) struct Reticle;

#[derive(Component)]
struct HudRoot;

/// Which corner chip — used to route clicks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Library,
    Queue,
}

#[derive(Component)]
pub(crate) struct HudChip(ChipKind);

// A pill/rounded Node helper (border_radius is a Node field in 0.19).
fn rounded(mut node: Node, radius: f32) -> Node {
    node.border_radius = BorderRadius::all(Val::Px(radius));
    node
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

pub fn setup_hud(
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    playback: Res<Playback>,
) {
    let accent = theme.accent();
    let track = playback.track();

    commands
        .spawn((
            HudRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            // The HUD never eats clicks meant for the world.
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            // --- Top-left corner affordances ---------------------------------
            root.spawn(Node {
                position_type: PositionType::Absolute,
                top: Val::Px(26.0),
                left: Val::Px(30.0),
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|row| {
                chip(row, &fonts, "‹  Library", "L", ChipKind::Library);
                chip(row, &fonts, "Queue", "Tab", ChipKind::Queue);
            });

            // --- Bottom-left now-playing cluster -----------------------------
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(64.0),
                left: Val::Px(30.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|col| {
                col.spawn((
                    WorldNameText,
                    Text::new(theme.current().world_name),
                    text_font(fonts.display.clone(), 30.0),
                    TextColor(TEXT),
                    Fade {
                        group: FadeGroup::Minimal,
                        base: TEXT,
                        target: FadeTarget::Text,
                    },
                ));
                col.spawn((
                    TrackTitleText,
                    Text::new(track.title.clone()),
                    text_font(fonts.ui_medium.clone(), 15.0),
                    TextColor(theme::TEXT_DIM),
                    Fade {
                        group: FadeGroup::Full,
                        base: theme::TEXT_DIM,
                        target: FadeTarget::Text,
                    },
                ));
                col.spawn((
                    SectionText,
                    Text::new(track.section.clone()),
                    text_font(fonts.ui.clone(), 12.0),
                    TextColor(theme::text_muted()),
                    Fade {
                        group: FadeGroup::Full,
                        base: theme::text_muted(),
                        target: FadeTarget::Text,
                    },
                ));
            });

            // --- Bottom-right time + next ------------------------------------
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(64.0),
                right: Val::Px(30.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|col| {
                col.spawn(Node {
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        TimeElapsedText,
                        Text::new(fmt_time(playback.elapsed)),
                        text_font(fonts.ui_medium.clone(), 15.0),
                        TextColor(TEXT),
                        Fade {
                            group: FadeGroup::Full,
                            base: TEXT,
                            target: FadeTarget::Text,
                        },
                    ));
                    row.spawn((
                        TimeTotalText,
                        Text::new(fmt_time(track.duration)),
                        text_font(fonts.ui.clone(), 15.0),
                        TextColor(theme::text_muted()),
                        Fade {
                            group: FadeGroup::Full,
                            base: theme::text_muted(),
                            target: FadeTarget::Text,
                        },
                    ));
                });
                col.spawn((
                    NextTrackText,
                    Text::new(format!(
                        "NEXT   {} — {}",
                        playback.next_track().title,
                        playback.next_track().artist
                    )),
                    text_font(fonts.ui.clone(), 11.5),
                    TextColor(theme::text_muted()),
                    Fade {
                        group: FadeGroup::Full,
                        base: theme::text_muted(),
                        target: FadeTarget::Text,
                    },
                ));
            });

            // --- Bottom-center controls + mode toggle ------------------------
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(30.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-190.0)),
                width: Val::Px(380.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|col| {
                // Mode toggle (Explore | Drift).
                col.spawn((
                    rounded(
                        Node {
                            padding: UiRect::all(Val::Px(4.0)),
                            column_gap: Val::Px(4.0),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        RADIUS_PILL,
                    ),
                    BackgroundColor(theme::veil_hud()),
                    BorderColor::all(theme::hairline()),
                    Fade {
                        group: FadeGroup::Full,
                        base: theme::veil_hud(),
                        target: FadeTarget::Bg,
                    },
                ))
                .with_children(|tabs| {
                    mode_tab(tabs, &fonts, "Explore", CameraMode::Explore, accent, true);
                    mode_tab(tabs, &fonts, "Drift", CameraMode::Drift, accent, false);
                });
                // Control hint line.
                col.spawn((
                    Text::new("W A S D  move   ·   E  pulse   ·   Tab  queue   ·   Esc  pause"),
                    text_font(fonts.ui.clone(), 11.5),
                    TextColor(theme::text_muted()),
                    Fade {
                        group: FadeGroup::Full,
                        base: theme::text_muted(),
                        target: FadeTarget::Text,
                    },
                ));
            });

            // --- Progress bar (beat-reactive) --------------------------------
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(0.0),
                    left: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Px(3.0),
                    ..default()
                },
                BackgroundColor(theme::hairline()),
                Fade {
                    group: FadeGroup::Minimal,
                    base: theme::hairline(),
                    target: FadeTarget::Bg,
                },
            ))
            .with_children(|bar| {
                // Fill.
                bar.spawn((
                    ProgressFill,
                    AccentTint,
                    Node {
                        width: Val::Percent(playback.fraction() * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(accent),
                    Fade {
                        group: FadeGroup::Minimal,
                        base: accent,
                        target: FadeTarget::Bg,
                    },
                ));
                // Section notches.
                for f in &playback.sections {
                    bar.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(f * 100.0),
                            bottom: Val::Px(0.0),
                            width: Val::Px(1.0),
                            height: Val::Px(7.0),
                            ..default()
                        },
                        BackgroundColor(theme::hairline().with_alpha(0.4)),
                        Fade {
                            group: FadeGroup::Minimal,
                            base: theme::hairline().with_alpha(0.4),
                            target: FadeTarget::Bg,
                        },
                    ));
                }
                // Playhead.
                bar.spawn((
                    ProgressPlayhead,
                    AccentTint,
                    rounded(
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(playback.fraction() * 100.0),
                            bottom: Val::Px(-3.0),
                            width: Val::Px(9.0),
                            height: Val::Px(9.0),
                            margin: UiRect::left(Val::Px(-4.5)),
                            ..default()
                        },
                        RADIUS_PILL,
                    ),
                    BackgroundColor(accent),
                    Fade {
                        group: FadeGroup::Minimal,
                        base: accent,
                        target: FadeTarget::Bg,
                    },
                ));
            });

            // --- Explore reticle (survives into Hidden) ----------------------
            root.spawn((
                Reticle,
                rounded(
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Percent(50.0),
                        width: Val::Px(5.0),
                        height: Val::Px(5.0),
                        margin: UiRect::new(Val::Px(-2.5), Val::ZERO, Val::Px(-2.5), Val::ZERO),
                        ..default()
                    },
                    RADIUS_PILL,
                ),
                BackgroundColor(TEXT.with_alpha(0.5)),
            ));
        });
}

/// A small corner chip: `label` + a keycap.
fn chip(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    label: &str,
    key: &str,
    kind: ChipKind,
) {
    parent
        .spawn((
            HudChip(kind),
            Button,
            rounded(
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(7.0),
                    padding: UiRect::axes(Val::Px(11.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                RADIUS_SM,
            ),
            BackgroundColor(theme::veil_hud()),
            BorderColor::all(theme::hairline()),
            Fade {
                group: FadeGroup::Full,
                base: theme::veil_hud(),
                target: FadeTarget::Bg,
            },
        ))
        .with_children(|c| {
            c.spawn((
                Text::new(label.to_string()),
                text_font(fonts.ui_medium.clone(), 12.5),
                TextColor(TEXT),
                Fade {
                    group: FadeGroup::Full,
                    base: TEXT,
                    target: FadeTarget::Text,
                },
            ));
            c.spawn((
                keycap_node(),
                BackgroundColor(theme::hairline()),
                Fade {
                    group: FadeGroup::Full,
                    base: theme::hairline(),
                    target: FadeTarget::Bg,
                },
            ))
            .with_children(|k| {
                k.spawn((
                    Text::new(key.to_string()),
                    text_font(fonts.ui_semibold.clone(), 10.5),
                    TextColor(theme::text_muted()),
                    Fade {
                        group: FadeGroup::Full,
                        base: theme::text_muted(),
                        target: FadeTarget::Text,
                    },
                ));
            });
        });
}

fn keycap_node() -> Node {
    rounded(
        Node {
            min_width: Val::Px(18.0),
            height: Val::Px(18.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            padding: UiRect::horizontal(Val::Px(5.0)),
            ..default()
        },
        5.0,
    )
}

fn mode_tab(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    label: &str,
    mode: CameraMode,
    accent: Color,
    active: bool,
) {
    let bg = if active {
        accent.with_alpha(0.18)
    } else {
        Color::NONE
    };
    parent
        .spawn((
            ModeTab(mode),
            Button, // clickable — switches camera feel (see `mode_tab_clicks`)
            rounded(
                Node {
                    padding: UiRect::axes(Val::Px(16.0), Val::Px(7.0)),
                    ..default()
                },
                RADIUS_PILL,
            ),
            BackgroundColor(bg),
            Fade {
                group: FadeGroup::Full,
                base: bg,
                target: FadeTarget::Bg,
            },
        ))
        .with_children(|t| {
            t.spawn((
                Text::new(label.to_string()),
                text_font(fonts.ui_semibold.clone(), 12.5),
                TextColor(if active { TEXT } else { theme::text_muted() }),
                Fade {
                    group: FadeGroup::Full,
                    base: if active { TEXT } else { theme::text_muted() },
                    target: FadeTarget::Text,
                },
            ));
        });
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Grow the idle timer and resolve the HUD depth from it.
pub fn hud_depth(time: Res<Time>, mode: Res<CameraMode>, mut activity: ResMut<HudActivity>) {
    if activity.force_visible {
        activity.idle = 0.0;
        activity.force_visible = false;
    } else {
        activity.idle += time.delta_secs();
    }

    let hidden_after = match *mode {
        CameraMode::Explore => 12.0,
        CameraMode::Drift => 8.0,
    };
    activity.depth = if activity.idle < 4.0 {
        HudDepth::Visible
    } else if activity.idle < hidden_after {
        HudDepth::Minimized
    } else {
        HudDepth::Hidden
    };

    // Ease the two global alphas. Rising (wake) is fast (~180ms); falling
    // (sleep) is slower (~600ms).
    let dt = time.delta_secs();
    let target_full = matches!(activity.depth, HudDepth::Visible) as u8 as f32;
    let target_min = matches!(activity.depth, HudDepth::Visible | HudDepth::Minimized) as u8 as f32;
    activity.full = ease(activity.full, target_full, dt);
    activity.minimal = ease(activity.minimal, target_min, dt);
}

fn ease(current: f32, target: f32, dt: f32) -> f32 {
    let rate = if target > current { 10.0 } else { 3.0 }; // ~180ms vs ~600ms
    let step = (target - current) * (dt * rate).min(1.0);
    (current + step).clamp(0.0, 1.0)
}

/// Apply the eased alphas to every faded node.
#[allow(clippy::type_complexity)]
pub fn apply_hud_alpha(
    activity: Res<HudActivity>,
    mut q: Query<(
        &Fade,
        Option<&mut BackgroundColor>,
        Option<&mut TextColor>,
        Option<&mut BorderColor>,
    )>,
) {
    for (fade, bg, text, border) in &mut q {
        let g = match fade.group {
            FadeGroup::Full => activity.full,
            FadeGroup::Minimal => activity.minimal,
        };
        let col = fade.base.with_alpha(fade.base.alpha() * g);
        match fade.target {
            FadeTarget::Bg => {
                if let Some(mut bg) = bg {
                    bg.0 = col;
                }
            }
            FadeTarget::Text => {
                if let Some(mut t) = text {
                    t.0 = col;
                }
            }
        }
        // HUD borders are all hairline; fade them with the same group so the
        // chrome disappears completely (no ghost outlines when Hidden).
        if let Some(mut b) = border {
            let h = theme::hairline();
            *b = BorderColor::all(h.with_alpha(h.alpha() * g));
        }
    }
}

/// Keep accent-tinted nodes in sync with the current world's accent.
pub fn update_hud_accent(theme: Res<Theme>, mut q: Query<&mut Fade, With<AccentTint>>) {
    if !theme.is_changed() {
        return;
    }
    let accent = theme.accent();
    for mut fade in &mut q {
        fade.base = accent.with_alpha(fade.base.alpha());
    }
}

/// Sync dynamic text + progress geometry to the transport.
#[allow(clippy::type_complexity)]
pub fn update_hud_content(
    playback: Res<Playback>,
    beat: Res<Beat>,
    comfort: Res<Comfort>,
    theme: Res<Theme>,
    mut sets: ParamSet<(
        Query<&mut Text, With<WorldNameText>>,
        Query<&mut Text, With<TrackTitleText>>,
        Query<&mut Text, With<SectionText>>,
        Query<&mut Text, With<TimeElapsedText>>,
        Query<&mut Text, With<TimeTotalText>>,
        Query<&mut Text, With<NextTrackText>>,
    )>,
    mut fill_q: Query<&mut Node, (With<ProgressFill>, Without<ProgressPlayhead>)>,
    mut head_q: Query<(&mut Node, &mut Transform), With<ProgressPlayhead>>,
) {
    let track = playback.track();
    if let Ok(mut t) = sets.p0().single_mut() {
        *t = Text::new(theme.current().world_name);
    }
    if let Ok(mut t) = sets.p1().single_mut() {
        *t = Text::new(track.title.clone());
    }
    if let Ok(mut t) = sets.p2().single_mut() {
        *t = Text::new(track.section.clone());
    }
    if let Ok(mut t) = sets.p3().single_mut() {
        *t = Text::new(fmt_time(playback.elapsed));
    }
    if let Ok(mut t) = sets.p4().single_mut() {
        *t = Text::new(fmt_time(track.duration));
    }
    if let Ok(mut t) = sets.p5().single_mut() {
        *t = Text::new(format!(
            "NEXT   {} — {}",
            playback.next_track().title,
            playback.next_track().artist
        ));
    }

    let frac = playback.fraction() * 100.0;
    if let Ok(mut n) = fill_q.single_mut() {
        n.width = Val::Percent(frac);
    }
    if let Ok((mut n, mut tf)) = head_q.single_mut() {
        n.left = Val::Percent(frac);
        // Beat pulse: scale 1→1.5, capped when reduce-flashing is on.
        let pulse = if comfort.reduce_flashing {
            0.0
        } else {
            beat.pulse
        };
        tf.scale = Vec3::splat(1.0 + pulse * 0.5);
    }
}

/// Highlight the active camera-mode tab.
pub fn update_mode_tabs(
    mode: Res<CameraMode>,
    theme: Res<Theme>,
    mut q: Query<(&ModeTab, &mut Fade, &Children)>,
    mut text_q: Query<&mut Fade, (With<Text>, Without<ModeTab>)>,
) {
    if !mode.is_changed() && !theme.is_changed() {
        return;
    }
    let accent = theme.accent();
    for (tab, mut fade, children) in &mut q {
        let active = tab.0 == *mode;
        fade.base = if active {
            accent.with_alpha(0.18)
        } else {
            Color::NONE
        };
        for child in children {
            if let Ok(mut tfade) = text_q.get_mut(*child) {
                tfade.base = if active { TEXT } else { theme::text_muted() };
            }
        }
    }
}

/// Show the reticle only in Explore.
pub fn update_reticle(mode: Res<CameraMode>, mut q: Query<&mut Visibility, With<Reticle>>) {
    if !mode.is_changed() {
        return;
    }
    for mut v in &mut q {
        *v = if matches!(*mode, CameraMode::Explore) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Clicking an Explore/Drift tab switches the camera feel (in addition to `F`).
pub fn mode_tab_clicks(
    mut mode: ResMut<CameraMode>,
    q: Query<(&ModeTab, &Interaction), Changed<Interaction>>,
) {
    for (tab, interaction) in &q {
        if *interaction == Interaction::Pressed {
            *mode = tab.0;
        }
    }
}

/// Clicking a corner chip toggles its panel (Library / Queue).
pub fn chip_clicks(
    mut stack: ResMut<crate::OverlayStack>,
    mut queue_open: ResMut<crate::QueueOpen>,
    q: Query<(&HudChip, &Interaction), Changed<Interaction>>,
) {
    for (chip, interaction) in &q {
        if *interaction == Interaction::Pressed {
            match chip.0 {
                ChipKind::Library => stack.toggle(crate::Overlay::Library),
                ChipKind::Queue => queue_open.0 = !queue_open.0,
            }
        }
    }
}
