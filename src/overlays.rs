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
use crate::{
    AppState, Comfort, CreditsOpen, LibraryOpen, Paused, QueueOpen, SettingsOpen, WorldIntensity,
};

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
    StartGentle,
    FullIntensity,
    OnboardNext,
    Resume,
    BuildWorld,
    KeepWorld,
    PhotoMode,
    OpenSettings,
    OpenCredits,
    CloseSettings,
    CloseCredits,
    CloseLibrary,
    RestoreComfort,
    ToggleReduceFlashing,
    ToggleGentlerMotion,
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
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn handle_buttons(
    mut interactions: Query<(&Interaction, &UiButton, &mut BackgroundColor), Changed<Interaction>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut paused: ResMut<Paused>,
    mut settings_open: ResMut<SettingsOpen>,
    mut credits_open: ResMut<CreditsOpen>,
    mut library_open: ResMut<LibraryOpen>,
    mut comfort: ResMut<Comfort>,
    mut photo: ResMut<crate::Photo>,
    mut onboarding: ResMut<crate::Onboarding>,
    mut theme: ResMut<Theme>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, button, mut bg) in &mut interactions {
        match *interaction {
            Interaction::Pressed => match button.action {
                ButtonAction::Start | ButtonAction::Skip => next_state.set(AppState::InWorld),
                ButtonAction::StartGentle => {
                    comfort.reduce_flashing = true;
                    comfort.gentler_motion = true;
                    onboarding.step = 1;
                }
                ButtonAction::FullIntensity => onboarding.step = 1,
                ButtonAction::OnboardNext => onboarding.step = onboarding.step.saturating_add(1),
                ButtonAction::Resume => paused.0 = false,
                ButtonAction::BuildWorld => {
                    // "same song, a new place" — re-roll the world only.
                    theme.mood = (theme.mood + 1) % theme::MOODS.len();
                }
                ButtonAction::KeepWorld => { /* pin — no-op in this milestone */ }
                ButtonAction::PhotoMode => {
                    // Clear the chrome (incl. this pause overlay) and capture.
                    photo.request();
                    paused.0 = false;
                    settings_open.0 = false;
                    credits_open.0 = false;
                }
                ButtonAction::OpenSettings => {
                    // Opens over the world; leave pause behind it.
                    paused.0 = false;
                    settings_open.0 = true;
                }
                ButtonAction::OpenCredits => credits_open.0 = true,
                ButtonAction::CloseSettings => settings_open.0 = false,
                ButtonAction::CloseCredits => credits_open.0 = false,
                ButtonAction::CloseLibrary => library_open.0 = false,
                ButtonAction::RestoreComfort => *comfort = Comfort::default(),
                ButtonAction::ToggleReduceFlashing => {
                    comfort.reduce_flashing = !comfort.reduce_flashing;
                }
                ButtonAction::ToggleGentlerMotion => {
                    comfort.gentler_motion = !comfort.gentler_motion;
                }
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

pub fn spawn_first_run(
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    onboarding: Res<crate::Onboarding>,
) {
    build_onboarding(&mut commands, &fonts, &theme, onboarding.step);
}

/// Re-render the onboarding when the step advances.
pub fn refresh_onboarding(
    onboarding: Res<crate::Onboarding>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    existing: Query<Entity, With<FirstRunRoot>>,
) {
    if !onboarding.is_changed() || onboarding.is_added() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn();
    }
    build_onboarding(&mut commands, &fonts, &theme, onboarding.step);
}

/// Build the first-run onboarding for `step` (0 photosensitivity, 1 controls,
/// 2 import) — the photosensitivity-first flow from spec 1p.
fn build_onboarding(commands: &mut Commands, fonts: &Fonts, theme: &Theme, step: u8) {
    let accent = theme.accent();
    commands
        .spawn((
            FirstRunRoot,
            full_screen_center(),
            BackgroundColor(theme::BASE.with_alpha(0.92)),
            GlobalZIndex(100),
        ))
        .with_children(|c| {
            label_text(c, fonts, &format!("{} OF 3", step.min(2) + 1), 11.0, theme::text_muted().with_alpha(0.6), false);
            spacer(c, 18.0);

            match step {
                0 => {
                    label_text(c, fonts, "Worlds that move with your music", 34.0, TEXT, true);
                    spacer(c, 14.0);
                    centered_body(c, fonts, "Some songs make light pulse, flash, or strobe. If flashing bothers\nyou — or you're not sure — start gentle. It still looks beautiful.");
                    spacer(c, 28.0);
                    c.spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Center, ..default() })
                        .with_children(|row| {
                            button(row, fonts, "Start gentle", None, ButtonAction::StartGentle, true, accent);
                            button(row, fonts, "Full intensity", None, ButtonAction::FullIntensity, false, accent);
                        });
                    spacer(c, 14.0);
                    label_text(c, fonts, "Reduced flashing · gentler motion · recommended   ·   change anytime in Settings › Comfort", 11.0, theme::text_muted().with_alpha(0.6), false);
                }
                1 => {
                    label_text(c, fonts, "The controls, both ways", 34.0, TEXT, true);
                    spacer(c, 20.0);
                    c.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(10.0),
                        ..default()
                    })
                    .with_children(|col| {
                        onboard_control_row(col, fonts, "Move", "W A S D", "LS");
                        onboard_control_row(col, fonts, "Look", "Mouse", "RS");
                        onboard_control_row(col, fonts, "Send a pulse", "E", "X");
                        onboard_control_row(col, fonts, "Queue · Pause", "Tab · Esc", "View · B");
                    });
                    spacer(c, 26.0);
                    button(c, fonts, "Next", None, ButtonAction::OnboardNext, true, accent);
                }
                _ => {
                    label_text(c, fonts, "Bring your music", 40.0, TEXT, true);
                    spacer(c, 14.0);
                    centered_body(c, fonts, "Reverie plays the files already on this computer and imagines a\nworld for every song. Point it at a folder — nothing is uploaded, ever.");
                    spacer(c, 30.0);
                    c.spawn(Node { column_gap: Val::Px(12.0), align_items: AlignItems::Center, ..default() })
                        .with_children(|row| {
                            button(row, fonts, "Choose your music folder…", None, ButtonAction::Start, true, accent);
                            button(row, fonts, "Skip for now", None, ButtonAction::Skip, false, accent);
                        });
                    spacer(c, 18.0);
                    label_text(c, fonts, "MP3 · FLAC · WAV · OGG · AIFF", 11.0, theme::text_muted().with_alpha(0.4), false);
                }
            }

            if step < 2 {
                spacer(c, 26.0);
                button(c, fonts, "Skip setup", None, ButtonAction::Skip, false, accent);
            }
        });
}

/// Centered muted body paragraph used across onboarding steps.
fn centered_body(parent: &mut ChildSpawnerCommands<'_>, fonts: &Fonts, text: &str) {
    parent.spawn((
        Text::new(text.to_string()),
        text_font(fonts.ui.clone(), 14.0),
        TextColor(theme::text_muted()),
        TextLayout {
            justify: Justify::Center,
            ..default()
        },
    ));
}

/// One "action — keyboard — gamepad" row on the controls step.
fn onboard_control_row(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    action: &str,
    keyboard: &str,
    gamepad: &str,
) {
    parent
        .spawn(Node {
            width: Val::Px(440.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|row| {
            label_text(row, fonts, action, 14.0, TEXT, false);
            row.spawn(Node {
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|keys| {
                key_chip(keys, fonts, keyboard);
                key_chip(keys, fonts, gamepad);
            });
        });
}

/// A small key/gamepad chip.
fn key_chip(parent: &mut ChildSpawnerCommands<'_>, fonts: &Fonts, text: &str) {
    parent
        .spawn((
            rounded(
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                    ..default()
                },
                RADIUS_SM,
            ),
            BackgroundColor(theme::hairline()),
        ))
        .with_children(|c| {
            label_text(c, fonts, text, 12.0, TEXT, false);
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
                        ButtonAction::OpenSettings,
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
// Settings overlay (1n) — the Comfort group, with working toggles
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct SettingsRoot;

/// Which comfort field a toggle controls (for live visual updates).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ComfortField {
    ReduceFlashing,
    GentlerMotion,
}

/// On the toggle track (a `Button`).
#[derive(Component)]
pub struct ComfortToggle(pub ComfortField);
/// On the toggle knob.
#[derive(Component)]
pub struct ComfortKnob(pub ComfortField);

fn comfort_get(comfort: &Comfort, field: ComfortField) -> bool {
    match field {
        ComfortField::ReduceFlashing => comfort.reduce_flashing,
        ComfortField::GentlerMotion => comfort.gentler_motion,
    }
}

fn switch_bg(on: bool, accent: Color) -> Color {
    if on {
        accent.with_alpha(0.9)
    } else {
        theme::hairline()
    }
}

fn knob_left(on: bool) -> Val {
    Val::Px(if on { 22.0 } else { 4.0 })
}

pub fn sync_settings_overlay(
    open: Res<SettingsOpen>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    comfort: Res<Comfort>,
    existing: Query<Entity, With<SettingsRoot>>,
) {
    if !open.is_changed() {
        return;
    }
    if open.0 && existing.is_empty() {
        spawn_settings(&mut commands, &fonts, &theme, &comfort);
    } else if !open.0 {
        for e in &existing {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_settings(commands: &mut Commands, fonts: &Fonts, theme: &Theme, comfort: &Comfort) {
    let accent = theme.accent();
    commands
        .spawn((
            SettingsRoot,
            full_screen_center(),
            BackgroundColor(theme::veil_panel()),
            GlobalZIndex(95),
        ))
        .with_children(|c| {
            c.spawn((
                rounded(
                    Node {
                        width: Val::Px(640.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(34.0)),
                        row_gap: Val::Px(4.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    RADIUS_MD,
                ),
                BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.98)),
                BorderColor::all(theme::hairline()),
            ))
            .with_children(|card| {
                label_text(card, fonts, "SETTINGS", 13.0, theme::text_muted(), false);
                spacer(card, 16.0);

                // Section nav — Comfort is the active group; About & Credits opens Credits.
                card.spawn(Node {
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|nav| {
                    for (name, active) in [
                        ("Graphics", false),
                        ("Comfort", true),
                        ("Sound", false),
                        ("Worlds", false),
                        ("Storage", false),
                    ] {
                        nav_tab(nav, fonts, name, active, accent);
                    }
                    button(
                        nav,
                        fonts,
                        "About & Credits",
                        None,
                        ButtonAction::OpenCredits,
                        false,
                        accent,
                    );
                });

                spacer(card, 22.0);
                label_text(card, fonts, "Comfort", 20.0, TEXT, true);
                label_text(
                    card,
                    fonts,
                    "Reverie should feel good to be in. These apply instantly.",
                    12.5,
                    theme::text_muted(),
                    false,
                );
                spacer(card, 12.0);

                comfort_toggle(
                    card,
                    fonts,
                    "Reduce flashing",
                    "Caps strobing and beat-flash effects across every world.",
                    ComfortField::ReduceFlashing,
                    ButtonAction::ToggleReduceFlashing,
                    comfort.reduce_flashing,
                    accent,
                );
                hairline_row(card);
                comfort_toggle(
                    card,
                    fonts,
                    "Gentler world motion",
                    "The world sways less; scene changes take their time.",
                    ComfortField::GentlerMotion,
                    ButtonAction::ToggleGentlerMotion,
                    comfort.gentler_motion,
                    accent,
                );
                hairline_row(card);
                static_row(
                    card,
                    fonts,
                    "Camera bob while walking",
                    "Off keeps the camera perfectly level",
                    "Off",
                );
                hairline_row(card);
                static_row(
                    card,
                    fonts,
                    "Field of view",
                    "Wider can ease motion sickness",
                    "90°",
                );
                hairline_row(card);
                static_row(
                    card,
                    fonts,
                    "Interface size",
                    "TV is made for across-the-room",
                    "Comfortable",
                );

                spacer(card, 24.0);
                card.spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                })
                .with_children(|row| {
                    button(
                        row,
                        fonts,
                        "Restore comfort defaults",
                        None,
                        ButtonAction::RestoreComfort,
                        false,
                        accent,
                    );
                    button(
                        row,
                        fonts,
                        "Done",
                        Some("Esc"),
                        ButtonAction::CloseSettings,
                        true,
                        accent,
                    );
                });
            });
        });
}

/// A comfort on/off row: label + description on the left, a switch on the right.
#[allow(clippy::too_many_arguments)]
fn comfort_toggle(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    label: &str,
    desc: &str,
    field: ComfortField,
    action: ButtonAction,
    on: bool,
    accent: Color,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(16.0),
            padding: UiRect::vertical(Val::Px(10.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(3.0),
                ..default()
            })
            .with_children(|c| {
                label_text(c, fonts, label, 14.0, TEXT, false);
                label_text(c, fonts, desc, 11.5, theme::text_muted(), false);
            });
            // The switch is a button; its visual is kept in sync by
            // `update_comfort_toggles`.
            let bg = switch_bg(on, accent);
            row.spawn((
                Button,
                UiButton {
                    action,
                    primary: false,
                    base: bg,
                },
                ComfortToggle(field),
                rounded(
                    Node {
                        width: Val::Px(42.0),
                        height: Val::Px(24.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    RADIUS_PILL,
                ),
                BackgroundColor(bg),
                BorderColor::all(theme::hairline()),
            ))
            .with_children(|s| {
                s.spawn((
                    ComfortKnob(field),
                    rounded(
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(3.0),
                            left: knob_left(on),
                            width: Val::Px(16.0),
                            height: Val::Px(16.0),
                            ..default()
                        },
                        RADIUS_PILL,
                    ),
                    BackgroundColor(TEXT),
                ));
            });
        });
}

/// Keep comfort switches in sync with the `Comfort` resource.
pub fn update_comfort_toggles(
    comfort: Res<Comfort>,
    theme: Res<Theme>,
    mut tracks: Query<(&ComfortToggle, &mut BackgroundColor, &mut UiButton)>,
    mut knobs: Query<(&ComfortKnob, &mut Node)>,
) {
    if !comfort.is_changed() && !theme.is_changed() {
        return;
    }
    let accent = theme.accent();
    for (toggle, mut bg, mut btn) in &mut tracks {
        let c = switch_bg(comfort_get(&comfort, toggle.0), accent);
        bg.0 = c;
        btn.base = c;
    }
    for (knob, mut node) in &mut knobs {
        node.left = knob_left(comfort_get(&comfort, knob.0));
    }
}

// ---------------------------------------------------------------------------
// Credits & Licenses overlay (1o)
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct CreditsRoot;

pub fn sync_credits_overlay(
    open: Res<CreditsOpen>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    existing: Query<Entity, With<CreditsRoot>>,
) {
    if !open.is_changed() {
        return;
    }
    if open.0 && existing.is_empty() {
        spawn_credits(&mut commands, &fonts, &theme);
    } else if !open.0 {
        for e in &existing {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_credits(commands: &mut Commands, fonts: &Fonts, theme: &Theme) {
    let accent = theme.accent();
    commands
        .spawn((
            CreditsRoot,
            full_screen_center(),
            BackgroundColor(theme::veil_panel()),
            GlobalZIndex(96),
        ))
        .with_children(|c| {
            c.spawn((
                rounded(
                    Node {
                        width: Val::Px(680.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(34.0)),
                        row_gap: Val::Px(4.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    RADIUS_MD,
                ),
                BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.98)),
                BorderColor::all(theme::hairline()),
            ))
            .with_children(|card| {
                card.spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|h| {
                    h.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(3.0),
                        ..default()
                    })
                    .with_children(|t| {
                        label_text(t, fonts, "Credits & Licenses", 22.0, TEXT, true);
                        label_text(
                            t,
                            fonts,
                            "Everyone whose work is in these worlds.   412 assets · 38 packages",
                            11.5,
                            theme::text_muted(),
                            false,
                        );
                    });
                    button(
                        h,
                        fonts,
                        "Done",
                        Some("Esc"),
                        ButtonAction::CloseCredits,
                        true,
                        accent,
                    );
                });

                spacer(card, 14.0);
                card.spawn(Node {
                    column_gap: Val::Px(8.0),
                    row_gap: Val::Px(8.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|f| {
                    for (name, active) in [
                        ("All", true),
                        ("3D assets", false),
                        ("Sounds", false),
                        ("Open-source software", false),
                    ] {
                        nav_tab(f, fonts, name, active, accent);
                    }
                });

                spacer(card, 16.0);
                section_label(card, fonts, "3D ASSETS · 412");
                credit_row(
                    card,
                    fonts,
                    "Windswept Pines — tree set",
                    "14 models · appears in calm forest worlds",
                    "Mira Kovanen",
                    "CC BY 4.0",
                    accent,
                );
                credit_row(
                    card,
                    fonts,
                    "Basalt Monoliths",
                    "6 models · hero objects, low-valence worlds",
                    "Studio Merek",
                    "CC BY-SA 4.0",
                    accent,
                );
                credit_row(
                    card,
                    fonts,
                    "Drift Grass Vol. 2",
                    "ground cover · appears in most worlds",
                    "T. Okabe",
                    "CC0",
                    accent,
                );
                credit_row(
                    card,
                    fonts,
                    "Glass Crystal Kit",
                    "reactive objects · 22 variants",
                    "Anna Reyes",
                    "Licensed",
                    accent,
                );

                spacer(card, 14.0);
                section_label(card, fonts, "OPEN-SOURCE SOFTWARE · 38");
                credit_row(
                    card,
                    fonts,
                    "Bevy Engine",
                    "the engine Reverie runs on",
                    "Bevy contributors",
                    "MIT / Apache-2.0",
                    accent,
                );
                credit_row(
                    card,
                    fonts,
                    "Symphonia",
                    "audio decoding",
                    "Philip Deljanov",
                    "MPL-2.0",
                    accent,
                );
            });
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

/// A fixed-height vertical spacer.
fn spacer(parent: &mut ChildSpawnerCommands<'_>, h: f32) {
    parent.spawn(Node {
        height: Val::Px(h),
        ..default()
    });
}

/// A full-width hairline divider.
fn hairline_row(parent: &mut ChildSpawnerCommands<'_>) {
    parent.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(1.0),
            ..default()
        },
        BackgroundColor(theme::hairline().with_alpha(0.5)),
    ));
}

/// A non-interactive nav/filter pill (active = accent-tinted).
fn nav_tab(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    label: &str,
    active: bool,
    accent: Color,
) {
    parent
        .spawn((
            rounded(
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                    ..default()
                },
                RADIUS_PILL,
            ),
            BackgroundColor(if active {
                accent.with_alpha(0.18)
            } else {
                theme::veil_hud()
            }),
        ))
        .with_children(|t| {
            label_text(
                t,
                fonts,
                label,
                12.5,
                if active { TEXT } else { theme::text_muted() },
                false,
            );
        });
}

/// A display-only settings row: label + description on the left, value on the right.
fn static_row(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    label: &str,
    desc: &str,
    value: &str,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(16.0),
            padding: UiRect::vertical(Val::Px(10.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(3.0),
                ..default()
            })
            .with_children(|c| {
                label_text(c, fonts, label, 14.0, TEXT, false);
                label_text(c, fonts, desc, 11.5, theme::text_muted(), false);
            });
            label_text(row, fonts, value, 13.0, theme::text_muted(), false);
        });
}

/// An uppercase section header inside the credits list.
fn section_label(parent: &mut ChildSpawnerCommands<'_>, fonts: &Fonts, text: &str) {
    parent.spawn((
        Text::new(text.to_string()),
        text_font(fonts.ui_semibold.clone(), 10.5),
        TextColor(theme::text_muted().with_alpha(0.7)),
        Node {
            margin: UiRect::bottom(Val::Px(6.0)),
            ..default()
        },
    ));
}

/// One attribution row: title/detail, author, license chip, and a link.
fn credit_row(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    title: &str,
    detail: &str,
    author: &str,
    license: &str,
    accent: Color,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(14.0),
            padding: UiRect::vertical(Val::Px(8.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|c| {
                label_text(c, fonts, title, 13.5, TEXT, false);
                label_text(c, fonts, detail, 11.0, theme::text_muted(), false);
            });
            label_text(row, fonts, author, 12.0, theme::text_muted(), false);
            row.spawn((
                rounded(
                    Node {
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                        ..default()
                    },
                    RADIUS_SM,
                ),
                BackgroundColor(theme::hairline()),
            ))
            .with_children(|chip| {
                label_text(chip, fonts, license, 10.5, theme::TEXT_DIM, false);
            });
            label_text(
                row,
                fonts,
                "source & license ↗",
                10.5,
                accent.with_alpha(0.9),
                false,
            );
        });
}

// ---------------------------------------------------------------------------
// Library / Home overlay (1i) — pick a world to jump into
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct LibraryRoot;

/// A world card in the library; carries the mood index it selects.
#[derive(Component)]
pub struct WorldCard(pub usize);

pub fn sync_library_overlay(
    open: Res<LibraryOpen>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    existing: Query<Entity, With<LibraryRoot>>,
) {
    if !open.is_changed() {
        return;
    }
    if open.0 && existing.is_empty() {
        spawn_library(&mut commands, &fonts, &theme);
    } else if !open.0 {
        for e in &existing {
            commands.entity(e).despawn();
        }
    }
}

fn spawn_library(commands: &mut Commands, fonts: &Fonts, theme: &Theme) {
    let accent = theme.accent();
    let current = theme.mood % theme::MOODS.len();
    commands
        .spawn((
            LibraryRoot,
            full_screen_center(),
            BackgroundColor(theme::veil_panel()),
            GlobalZIndex(85),
        ))
        .with_children(|c| {
            c.spawn((
                rounded(
                    Node {
                        width: Val::Px(720.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(34.0)),
                        row_gap: Val::Px(6.0),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    RADIUS_MD,
                ),
                BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.96)),
                BorderColor::all(theme::hairline()),
            ))
            .with_children(|card| {
                card.spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|h| {
                    h.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(3.0),
                        ..default()
                    })
                    .with_children(|t| {
                        label_text(t, fonts, "Library", 24.0, TEXT, true);
                        label_text(
                            t,
                            fonts,
                            "Every world Reverie has imagined for you",
                            11.5,
                            theme::text_muted(),
                            false,
                        );
                    });
                    button(
                        h,
                        fonts,
                        "Done",
                        Some("L"),
                        ButtonAction::CloseLibrary,
                        true,
                        accent,
                    );
                });

                spacer(card, 18.0);
                card.spawn(Node {
                    width: Val::Percent(100.0),
                    column_gap: Val::Px(16.0),
                    row_gap: Val::Px(16.0),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|grid| {
                    for (i, mood) in theme::MOODS.iter().enumerate() {
                        world_card(grid, fonts, i, mood, i == current);
                    }
                });
            });
        });
}

/// One selectable world card, showing a palette swatch + name.
fn world_card(
    parent: &mut ChildSpawnerCommands<'_>,
    fonts: &Fonts,
    idx: usize,
    mood: &theme::WorldMood,
    current: bool,
) {
    parent
        .spawn((
            Button,
            WorldCard(idx),
            rounded(
                Node {
                    width: Val::Px(200.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                RADIUS_MD,
            ),
            BackgroundColor(if current {
                mood.accent.with_alpha(0.10)
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.03)
            }),
            BorderColor::all(if current {
                mood.accent.with_alpha(0.5)
            } else {
                theme::hairline()
            }),
        ))
        .with_children(|card| {
            // Palette swatch: sky over a ground band, with the accent dot.
            let mut swatch = rounded(
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(96.0),
                    ..default()
                },
                RADIUS_SM,
            );
            swatch.overflow = Overflow::clip();
            card.spawn((swatch, BackgroundColor(mood.sky_top)))
                .with_children(|s| {
                    s.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            bottom: Val::Px(0.0),
                            left: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Px(34.0),
                            ..default()
                        },
                        BackgroundColor(mood.ground),
                    ));
                    s.spawn((
                        rounded(
                            Node {
                                position_type: PositionType::Absolute,
                                top: Val::Px(10.0),
                                left: Val::Px(10.0),
                                width: Val::Px(16.0),
                                height: Val::Px(16.0),
                                ..default()
                            },
                            RADIUS_PILL,
                        ),
                        BackgroundColor(mood.accent),
                    ));
                });
            label_text(card, fonts, mood.world_name, 16.0, TEXT, true);
            label_text(
                card,
                fonts,
                if current {
                    "Playing now"
                } else {
                    "Jump in ›"
                },
                11.0,
                if current {
                    mood.accent.with_alpha(0.9)
                } else {
                    theme::text_muted()
                },
                false,
            );
        });
}

/// Clicking a world card jumps to that world and closes the library.
pub fn handle_world_cards(
    mut theme: ResMut<Theme>,
    mut library_open: ResMut<LibraryOpen>,
    q: Query<(&WorldCard, &Interaction), Changed<Interaction>>,
) {
    for (card, interaction) in &q {
        if *interaction == Interaction::Pressed {
            theme.mood = card.0;
            library_open.0 = false;
        }
    }
}
