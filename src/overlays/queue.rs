//! Slide-in queue panel (spec 1l).

use bevy::prelude::*;
use bevy::ui::GlobalZIndex;

use crate::QueueOpen;
use crate::playback::{Playback, fmt_time};
use crate::theme::{self, Fonts, RADIUS_SM, TEXT, Theme};

#[allow(unused_imports)]
use super::widgets::*;

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
                    &track.title,
                    &track.artist,
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
