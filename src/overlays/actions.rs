//! Button actions: the dispatcher broadcasts presses as [`UiAction`]
//! messages; focused handlers consume them (ARCHITECTURE R3).

use bevy::app::AppExit;
use bevy::prelude::*;

use crate::playback::Playback;
use crate::theme::{self, Theme};
use crate::{AppState, Comfort, Overlay, OverlayStack, Paused, QueueOpen};

#[allow(unused_imports)]
use super::widgets::*;

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

/// A pressed button's action, broadcast to the focused handlers below —
/// each takes only the state it touches (ARCHITECTURE R3; replaces the old
/// 14-parameter `handle_buttons`).
#[derive(Message)]
pub struct UiAction(pub ButtonAction);

/// Hover/press tint + broadcast presses as [`UiAction`] messages.
pub fn dispatch_buttons(
    mut interactions: Query<(&Interaction, &UiButton, &mut BackgroundColor), Changed<Interaction>>,
    mut actions: MessageWriter<UiAction>,
) {
    for (interaction, button, mut bg) in &mut interactions {
        match *interaction {
            Interaction::Pressed => {
                actions.write(UiAction(button.action));
            }
            Interaction::Hovered => {
                bg.0 = lighten(button.base, if button.primary { 0.10 } else { 0.14 });
            }
            Interaction::None => {
                bg.0 = button.base;
            }
        }
    }
}

/// Onboarding flow: import, skip, and the photosensitivity choice (1p/1j).
pub fn onboarding_actions(
    mut actions: MessageReader<UiAction>,
    mut next_state: ResMut<NextState<AppState>>,
    mut onboarding: ResMut<crate::Onboarding>,
    mut comfort: ResMut<Comfort>,
    mut import: ResMut<crate::audio::ImportState>,
) {
    for UiAction(action) in actions.read() {
        match action {
            ButtonAction::Start => {
                // Native folder picker (blocks the main thread while the
                // modal is open — required on macOS anyway). Cancelling
                // stays on the onboarding; "Skip for now" is the way past.
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title("Choose your music folder")
                    .pick_folder()
                {
                    crate::audio::start_import(folder, &mut import);
                    next_state.set(AppState::InWorld);
                }
            }
            ButtonAction::Skip => next_state.set(AppState::InWorld),
            ButtonAction::StartGentle => {
                comfort.reduce_flashing = true;
                comfort.gentler_motion = true;
                onboarding.step = 1;
            }
            ButtonAction::FullIntensity => onboarding.step = 1,
            ButtonAction::OnboardNext => onboarding.step = onboarding.step.saturating_add(1),
            _ => {}
        }
    }
}

/// Overlay navigation: the modal stack + pause interplay (R4).
pub fn overlay_actions(
    mut actions: MessageReader<UiAction>,
    mut stack: ResMut<OverlayStack>,
    mut paused: ResMut<Paused>,
    mut playback: ResMut<Playback>,
) {
    for UiAction(action) in actions.read() {
        match action {
            ButtonAction::Resume => {
                paused.0 = false;
                playback.playing = true;
            }
            ButtonAction::OpenSettings => {
                // Opens over the world; pause lifts underneath it.
                paused.0 = false;
                playback.playing = true;
                stack.open(Overlay::Settings);
            }
            ButtonAction::OpenCredits => stack.open(Overlay::Credits),
            ButtonAction::CloseSettings => stack.close(Overlay::Settings),
            ButtonAction::CloseCredits => stack.close(Overlay::Credits),
            ButtonAction::CloseLibrary => stack.close(Overlay::Library),
            _ => {}
        }
    }
}

/// World identity actions: re-roll, pin, photo (spec 1k).
#[allow(clippy::too_many_arguments)]
pub fn world_actions(
    mut actions: MessageReader<UiAction>,
    mut theme: ResMut<Theme>,
    mut layout: ResMut<crate::world_assets::WorldLayout>,
    mut analysis: ResMut<crate::analysis::AnalysisStore>,
    playback: Res<Playback>,
    mut photo: ResMut<crate::Photo>,
    mut paused: ResMut<Paused>,
    mut queue_open: ResMut<QueueOpen>,
    mut stack: ResMut<OverlayStack>,
) {
    for UiAction(action) in actions.read() {
        match action {
            ButtonAction::BuildWorld => {
                // "same song, a new place" — new palette and a re-rolled
                // layout seed.
                theme.mood = (theme.mood + 1) % theme::MOODS.len();
                let mut state = layout.seed;
                crate::world_assets::splitmix(&mut state);
                layout.seed = state;
            }
            ButtonAction::KeepWorld => {
                // Pin the current world — mood + layout seed — to this
                // track's analysis sidecar so the song always returns here.
                if !playback.queue.is_empty() {
                    let idx = playback.current % playback.queue.len();
                    if let Some(path) = playback.queue[idx].path.clone() {
                        analysis.toggle_pin(&path, theme.mood, layout.seed);
                    }
                }
            }
            ButtonAction::PhotoMode => {
                // Clear all chrome (overlays, queue, pause) and capture.
                photo.request();
                paused.0 = false;
                queue_open.0 = false;
                stack.clear();
            }
            _ => {}
        }
    }
}

/// Comfort toggles — apply instantly (spec 1n).
pub fn comfort_actions(mut actions: MessageReader<UiAction>, mut comfort: ResMut<Comfort>) {
    for UiAction(action) in actions.read() {
        match action {
            ButtonAction::RestoreComfort => *comfort = Comfort::default(),
            ButtonAction::ToggleReduceFlashing => {
                comfort.reduce_flashing = !comfort.reduce_flashing;
            }
            ButtonAction::ToggleGentlerMotion => {
                comfort.gentler_motion = !comfort.gentler_motion;
            }
            _ => {}
        }
    }
}

/// App-level actions.
pub fn app_actions(mut actions: MessageReader<UiAction>, mut exit: MessageWriter<AppExit>) {
    for UiAction(action) in actions.read() {
        if matches!(action, ButtonAction::Quit) {
            exit.write(AppExit::Success);
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
