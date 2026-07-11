//! Full-screen overlays: first-run import, pause, and the queue panel.
//!
//! These are always fully opaque while shown (unlike the HUD, which fades),
//! so they simply spawn on a state/flag change and despawn when it clears.
//! The world keeps playing behind them, dimmed — "quieter, not hidden".

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::playback::{Playback, fmt_time};
use crate::theme::{self, Fonts, RADIUS_MD, RADIUS_PILL, RADIUS_SM, TEXT, Theme, text_font};
use crate::{AppState, Paused, QueueOpen, WorldIntensity};

// A rounded Node (border_radius is a Node field in Bevy 0.19).
fn rounded(mut node: Node, radius: f32) -> Node {
    node.border_radius = BorderRadius::all(Val::Px(radius));
    node
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    Start,
    Skip,
    Resume,
    BuildWorld,
    KeepWorld,
    PhotoMode,
    Settings,
    Quit,
}

#[derive(Component, Clone, Copy)]
pub struct UiButton {
    pub action: ButtonAction,
    pub primary: bool,
    /// Cached base background so hover can restore it.
    pub base: Color,
}

/// Dispatch button clicks + apply hover/press tint.
#[allow(clippy::type_complexity)]
pub fn handle_buttons(
    mut interactions: Query<(&Interaction, &UiButton, &mut BackgroundColor), Changed<Interaction>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut paused: ResMut<Paused>,
    mut theme: ResMut<Theme>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button, mut bg) in &mut interactions {
        match *interaction {
            Interaction::Pressed => match button.action {
                ButtonAction::Start | ButtonAction::Skip => next_state.set(AppState::InWorld),
                ButtonAction::Resume => paused.0 = false,
                ButtonAction::BuildWorld => {
                    // "same song, a new place" — re-roll the world only.
                    theme.mood = (theme.mood + 1) % theme::MOODS.len();
                }
                ButtonAction::KeepWorld => { /* pin — no-op in this milestone */ }
                ButtonAction::PhotoMode => { /* reserved */ }
                ButtonAction::Settings => { /* settings screen — follow-up */ }
                ButtonAction::Quit => {
                    exit.write(AppExit::Success);
                }
            },
            Interaction::Hovered => {
                bg.0 = lighten(button.base, if button.primary { 0.10 } else { 0.14 });
            }
            Interaction::None => {
                bg.0 = button.base;
            }
        }
    }
}

fn lighten(c: Color, amt: f32) -> Color {
    let s = c.to_srgba();
    Color::srgba(
        (s.red + amt).min(1.0),
        (s.green + amt).min(1.0),
        (s.blue + amt).min(1.0),
        (s.alpha + amt * 0.5).min(1.0),
    )
}

/// Spawn a pill button into `parent`.
fn button(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    label: &str,
    key: Option<&str>,
    action: ButtonAction,
    primary: bool,
    accent: Color,
) {
    let base = if primary {
        accent.with_alpha(0.92)
    } else {
        theme::veil_hud()
    };
    let fg = if primary { theme::BASE } else { TEXT };
    parent
        .spawn((
            Button,
            UiButton {
                action,
                primary,
                base,
            },
            rounded(
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(9.0),
                    padding: UiRect::axes(Val::Px(18.0), Val::Px(11.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                RADIUS_PILL,
            ),
            BackgroundColor(base),
            BorderColor::all(if primary {
                Color::NONE
            } else {
                theme::hairline()
            }),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label.to_string()),
                text_font(fonts.ui_semibold.clone(), 13.5),
                TextColor(fg),
            ));
            if let Some(k) = key {
                b.spawn((
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
                    ),
                    BackgroundColor(if primary {
                        theme::BASE.with_alpha(0.15)
                    } else {
                        theme::hairline()
                    }),
                ))
                .with_children(|c| {
                    c.spawn((
                        Text::new(k.to_string()),
                        text_font(fonts.ui_semibold.clone(), 10.5),
                        TextColor(fg.with_alpha(0.7)),
                    ));
                });
            }
        });
}

fn label_text(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    text: &str,
    size: f32,
    color: Color,
    display: bool,
) {
    parent.spawn((
        Text::new(text.to_string()),
        text_font(
            if display {
                fonts.display.clone()
            } else {
                fonts.ui.clone()
            },
            size,
        ),
        TextColor(color),
    ));
}

// ---------------------------------------------------------------------------
// First run — "Bring your music" (1j)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct FirstRunRoot;

pub fn spawn_first_run(mut commands: Commands, fonts: Res<Fonts>, theme: Res<Theme>) {
    let accent = theme.accent();
    commands
        .spawn((
            FirstRunRoot,
            full_screen_center(),
            BackgroundColor(theme::BASE.with_alpha(0.72)),
            GlobalZIndex(100),
        ))
        .with_children(|c| {
            label_text(c, &fonts, "R E V E R I E", 15.0, theme::text_muted(), false);
            c.spawn(Node { height: Val::Px(18.0), ..default() });
            label_text(c, &fonts, "Bring your music", 40.0, TEXT, true);
            c.spawn(Node { height: Val::Px(14.0), ..default() });
            c.spawn((
                Text::new(
                    "Reverie plays the files already on this computer and imagines a\nworld for every song. Point it at a folder — nothing is uploaded, ever.",
                ),
                text_font(fonts.ui.clone(), 14.0),
                TextColor(theme::text_muted()),
                TextLayout { justify: Justify::Center, ..default() },
            ));
            c.spawn(Node { height: Val::Px(30.0), ..default() });
            c.spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Center, ..default() })
                .with_children(|row| {
                    button(row, &fonts, "Choose your music folder…", None, ButtonAction::Start, true, accent);
                    button(row, &fonts, "Skip for now", None, ButtonAction::Skip, false, accent);
                });
            c.spawn(Node { height: Val::Px(18.0), ..default() });
            label_text(c, &fonts, "MP3 · FLAC · WAV · OGG · AIFF", 11.0, theme::text_muted().with_alpha(0.4), false);
        });
}

pub fn despawn_first_run(mut commands: Commands, q: Query<Entity, With<FirstRunRoot>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}

// ---------------------------------------------------------------------------
// Pause overlay (1k)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct PauseRoot;
#[derive(Component)]
pub struct IntensityKnob;

pub fn sync_pause_overlay(
    paused: Res<Paused>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    playback: Res<Playback>,
    intensity: Res<WorldIntensity>,
    existing: Query<Entity, With<PauseRoot>>,
) {
    if !paused.is_changed() {
        return;
    }
    if paused.0 && existing.is_empty() {
        spawn_pause(&mut commands, &fonts, &theme, &playback, intensity.0);
    } else if !paused.0 {
        for e in &existing {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_pause(
    commands: &mut Commands,
    fonts: &Fonts,
    theme: &Theme,
    playback: &Playback,
    intensity: f32,
) {
    let accent = theme.accent();
    let track = playback.track();
    commands
        .spawn((
            PauseRoot,
            full_screen_center(),
            BackgroundColor(theme::veil_panel()),
            GlobalZIndex(90),
        ))
        .with_children(|c| {
            // Card.
            c.spawn((
                rounded(
                    Node {
                        width: Val::Px(460.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(34.0)),
                        row_gap: Val::Px(6.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    RADIUS_MD,
                ),
                BackgroundColor(Color::srgba(0.06, 0.06, 0.09, 0.75)),
                BorderColor::all(theme::hairline()),
            ))
            .with_children(|card| {
                label_text(
                    card,
                    fonts,
                    &format!("{} · PAUSED", theme.current().world_name),
                    12.5,
                    theme::text_muted(),
                    false,
                );
                card.spawn(Node {
                    height: Val::Px(4.0),
                    ..default()
                });
                label_text(card, fonts, track.title, 30.0, TEXT, true);
                label_text(
                    card,
                    fonts,
                    &format!(
                        "{} · {} of {}",
                        track.section,
                        fmt_time(playback.elapsed),
                        fmt_time(track.duration)
                    ),
                    12.5,
                    theme::text_muted(),
                    false,
                );
                card.spawn(Node {
                    height: Val::Px(22.0),
                    ..default()
                });

                // Primary + secondary actions.
                card.spawn(Node {
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|row| {
                    button(
                        row,
                        fonts,
                        "Resume",
                        Some("Esc"),
                        ButtonAction::Resume,
                        true,
                        accent,
                    );
                    button(
                        row,
                        fonts,
                        "Build a different world",
                        Some("R"),
                        ButtonAction::BuildWorld,
                        false,
                        accent,
                    );
                    button(
                        row,
                        fonts,
                        "Keep this world",
                        None,
                        ButtonAction::KeepWorld,
                        false,
                        accent,
                    );
                });

                card.spawn(Node {
                    height: Val::Px(24.0),
                    ..default()
                });

                // World intensity slider.
                card.spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|hdr| {
                        label_text(hdr, fonts, "World intensity", 12.5, TEXT, false);
                        label_text(hdr, fonts, "← →", 12.5, theme::text_muted(), false);
                    });
                    // Track + knob.
                    col.spawn((
                        rounded(
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(4.0),
                                ..default()
                            },
                            RADIUS_PILL,
                        ),
                        BackgroundColor(theme::hairline()),
                    ))
                    .with_children(|tk| {
                        tk.spawn((
                            IntensityKnob,
                            rounded(
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: Val::Percent(intensity * 100.0),
                                    top: Val::Px(-5.0),
                                    width: Val::Px(14.0),
                                    height: Val::Px(14.0),
                                    margin: UiRect::left(Val::Px(-7.0)),
                                    ..default()
                                },
                                RADIUS_PILL,
                            ),
                            BackgroundColor(accent),
                        ));
                    });
                    col.spawn(Node {
                        justify_content: JustifyContent::SpaceBetween,
                        width: Val::Percent(100.0),
                        ..default()
                    })
                    .with_children(|lbl| {
                        label_text(lbl, fonts, "calm", 11.0, theme::text_muted(), false);
                        label_text(lbl, fonts, "lively", 11.0, theme::text_muted(), false);
                        label_text(lbl, fonts, "intense", 11.0, theme::text_muted(), false);
                    });
                    label_text(
                        col,
                        fonts,
                        "also caps flashing & camera motion — see Comfort in Settings",
                        10.5,
                        theme::text_muted().with_alpha(0.6),
                        false,
                    );
                });

                card.spawn(Node {
                    height: Val::Px(24.0),
                    ..default()
                });

                // Footer actions.
                card.spawn(Node {
                    column_gap: Val::Px(10.0),
                    ..default()
                })
                .with_children(|row| {
                    button(
                        row,
                        fonts,
                        "Photo mode",
                        Some("P"),
                        ButtonAction::PhotoMode,
                        false,
                        accent,
                    );
                    button(
                        row,
                        fonts,
                        "Settings",
                        None,
                        ButtonAction::Settings,
                        false,
                        accent,
                    );
                    button(
                        row,
                        fonts,
                        "Quit Reverie",
                        Some("Q"),
                        ButtonAction::Quit,
                        false,
                        accent,
                    );
                });
                card.spawn(Node {
                    height: Val::Px(12.0),
                    ..default()
                });
                label_text(
                    card,
                    fonts,
                    "time holds its breath while you're here",
                    11.0,
                    theme::text_muted().with_alpha(0.6),
                    false,
                );
            });
        });
}

/// Live-update the knob position while paused.
pub fn update_intensity_knob(
    intensity: Res<WorldIntensity>,
    mut q: Query<&mut Node, With<IntensityKnob>>,
) {
    if !intensity.is_changed() {
        return;
    }
    for mut n in &mut q {
        n.left = Val::Percent(intensity.0 * 100.0);
    }
}

// ---------------------------------------------------------------------------
// Queue panel (1l)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct QueueRoot;

pub fn sync_queue_overlay(
    queue_open: Res<QueueOpen>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    playback: Res<Playback>,
    existing: Query<Entity, With<QueueRoot>>,
) {
    if !queue_open.is_changed() {
        return;
    }
    if queue_open.0 && existing.is_empty() {
        spawn_queue(&mut commands, &fonts, &theme, &playback);
    } else if !queue_open.0 {
        for e in &existing {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_queue(commands: &mut Commands, fonts: &Fonts, theme: &Theme, playback: &Playback) {
    let accent = theme.accent();
    let total: f32 = playback.queue.iter().map(|t| t.duration).sum();
    commands
        .spawn((
            QueueRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                width: Val::Px(380.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(26.0)),
                row_gap: Val::Px(6.0),
                border: UiRect::left(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme::veil_panel()),
            BorderColor::all(theme::hairline()),
            GlobalZIndex(80),
        ))
        .with_children(|panel| {
            // Header.
            panel
                .spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Baseline,
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|hdr| {
                    label_text(hdr, fonts, "Queue", 22.0, TEXT, true);
                    label_text(hdr, fonts, "Clear", 12.0, theme::text_muted(), false);
                });
            label_text(
                panel,
                fonts,
                &format!(
                    "{} songs · {} min",
                    playback.queue.len(),
                    (total / 60.0).round() as i32
                ),
                11.5,
                theme::text_muted(),
                false,
            );
            panel.spawn(Node {
                height: Val::Px(14.0),
                ..default()
            });

            for (i, track) in playback.queue.iter().enumerate() {
                let tag = match i {
                    0 => Some(("NOW", accent)),
                    1 => Some(("NEXT", theme::text_muted())),
                    _ => None,
                };
                queue_row(
                    panel,
                    fonts,
                    tag,
                    track.title,
                    track.artist,
                    fmt_time(track.duration),
                    i == 0,
                );
            }

            panel.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            label_text(
                panel,
                fonts,
                "drag to reorder · the next world is prepared quietly",
                11.0,
                theme::text_muted().with_alpha(0.6),
                false,
            );
        });
}

fn queue_row(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    tag: Option<(&str, Color)>,
    title: &str,
    artist: &str,
    dur: String,
    current: bool,
) {
    parent
        .spawn((
            rounded(
                Node {
                    width: Val::Percent(100.0),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(9.0)),
                    ..default()
                },
                RADIUS_SM,
            ),
            BackgroundColor(if current {
                theme::hairline().with_alpha(0.08)
            } else {
                Color::NONE
            }),
        ))
        .with_children(|row| {
            // Tag column.
            row.spawn(Node {
                width: Val::Px(42.0),
                ..default()
            })
            .with_children(|c| {
                if let Some((t, col)) = tag {
                    label_text(c, fonts, t, 9.5, col, false);
                }
            });
            // Title + artist.
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|col| {
                label_text(col, fonts, title, 14.0, TEXT, false);
                label_text(col, fonts, artist, 11.5, theme::text_muted(), false);
            });
            label_text(row, fonts, &dur, 12.0, theme::text_muted(), false);
        });
}

// ---------------------------------------------------------------------------
// Shared layout helpers
// ---------------------------------------------------------------------------

fn full_screen_center() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}
