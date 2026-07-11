//! Library/Home — world-mood cards (spec 1i).

use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::theme::{self, Fonts, RADIUS_MD, RADIUS_PILL, RADIUS_SM, TEXT, Theme};
use crate::{Overlay, OverlayStack};

use super::actions::ButtonAction;
#[allow(unused_imports)]
use super::widgets::*;

#[derive(Component)]
pub struct LibraryRoot;

/// A world card in the library; carries the mood index it selects.
#[derive(Component)]
pub struct WorldCard(pub usize);

pub fn sync_library_overlay(
    stack: Res<OverlayStack>,
    mut commands: Commands,
    fonts: Res<Fonts>,
    theme: Res<Theme>,
    existing: Query<Entity, With<LibraryRoot>>,
    mut last: Local<bool>,
) {
    let open = stack.is_open(Overlay::Library);
    if open == *last {
        return;
    }
    *last = open;
    if open && existing.is_empty() {
        spawn_library(&mut commands, &fonts, &theme);
    } else if !open {
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
    mut stack: ResMut<OverlayStack>,
    q: Query<(&WorldCard, &Interaction), Changed<Interaction>>,
) {
    for (card, interaction) in &q {
        if *interaction == Interaction::Pressed {
            theme.mood = card.0;
            stack.close(Overlay::Library);
        }
    }
}
