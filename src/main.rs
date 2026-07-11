//! Reverie — a desktop app that imagines a 3D world for every song.
//!
//! This milestone is **UI-first**: the chrome from the imported design spec is
//! built on Bevy UI over a placeholder mood-tinted world. The music-analysis
//! and asset-assembly pipeline described in `idea.md` comes later; for now the
//! transport and beat are simulated (see [`playback`]).

mod hud;
mod overlays;
mod playback;
mod theme;
mod world;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use hud::HudActivity;
use playback::{Beat, Playback};
use theme::{Fonts, Theme};

// ---------------------------------------------------------------------------
// Shared app state & resources
// ---------------------------------------------------------------------------

/// Top-level screen.
#[derive(States, Default, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum AppState {
    /// First run — "Bring your music".
    #[default]
    FirstRun,
    /// Playing — HUD over a live world.
    InWorld,
}

/// Camera feel. Also stored inside each HUD mode tab.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    /// First-person WASD + mouse-look.
    #[default]
    Explore,
    /// Hands-off cinematic auto-orbit.
    Drift,
}

/// Whether the pause overlay is up (and the world is time-dilated).
#[derive(Resource, Default)]
pub struct Paused(pub bool);

/// Whether the queue panel is sliding in.
#[derive(Resource, Default)]
pub struct QueueOpen(pub bool);

/// Whether the settings overlay is open.
#[derive(Resource, Default)]
pub struct SettingsOpen(pub bool);

/// Whether the credits & licenses overlay is open.
#[derive(Resource, Default)]
pub struct CreditsOpen(pub bool);

/// Photo mode: frames to wait (HUD hidden) before capturing a clean shot.
#[derive(Resource, Default)]
pub struct Photo {
    pub pending: Option<u8>,
}

impl Photo {
    /// Request a photo — the HUD hides and a screenshot lands a few frames
    /// later (enough for the chrome to clear and the frame to settle).
    pub fn request(&mut self) {
        self.pending = Some(8);
    }
}

/// First-run onboarding step (0 = photosensitivity, 1 = controls, 2 = import).
#[derive(Resource, Default)]
pub struct Onboarding {
    pub step: u8,
}

/// World-intensity slider value (0..1), shown in the pause overlay.
#[derive(Resource)]
pub struct WorldIntensity(pub f32);

impl Default for WorldIntensity {
    fn default() -> Self {
        Self(0.5)
    }
}

/// Comfort settings — the reduce-flashing gate the spec insists on.
#[derive(Resource, Default)]
pub struct Comfort {
    pub reduce_flashing: bool,
    pub gentler_motion: bool,
}

/// The world's timescale (1.0 playing, eases to ~0.05 when paused).
#[derive(Resource)]
pub struct WorldClock {
    pub speed: f32,
}

impl Default for WorldClock {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Reverie".to_string(),
            resolution: (1280, 800).into(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(theme::BASE))
    .init_state::<AppState>()
    .init_resource::<Theme>()
    .init_resource::<Playback>()
    .init_resource::<Beat>()
    .init_resource::<HudActivity>()
    .init_resource::<CameraMode>()
    .init_resource::<Paused>()
    .init_resource::<QueueOpen>()
    .init_resource::<SettingsOpen>()
    .init_resource::<CreditsOpen>()
    .init_resource::<Photo>()
    .init_resource::<Onboarding>()
    .init_resource::<WorldIntensity>()
    .init_resource::<Comfort>()
    .init_resource::<WorldClock>()
    // Setup.
    .add_systems(Startup, world::setup_world)
    .add_systems(OnEnter(AppState::FirstRun), overlays::spawn_first_run)
    .add_systems(OnExit(AppState::FirstRun), overlays::despawn_first_run)
    .add_systems(OnEnter(AppState::InWorld), hud::setup_hud)
    // Always-on: buttons + a living world.
    .add_systems(
        Update,
        (
            overlays::handle_buttons,
            world::update_world_palette,
            world::animate_world,
            ease_world_clock,
        ),
    )
    .add_systems(
        Update,
        (input_first_run, overlays::refresh_onboarding).run_if(in_state(AppState::FirstRun)),
    )
    .add_systems(
        Update,
        world::camera_control
            .run_if(in_state(AppState::InWorld))
            .run_if(not_paused),
    )
    // In-world: input, transport, HUD, overlays.
    .add_systems(
        Update,
        (
            input_in_world,
            playback::advance_playback,
            hud::hud_depth,
            hud::apply_hud_alpha,
            hud::update_hud_accent,
            hud::update_hud_content,
            hud::update_mode_tabs,
            hud::update_reticle,
            hud::mode_tab_clicks,
            overlays::sync_pause_overlay,
            overlays::sync_queue_overlay,
            overlays::sync_settings_overlay,
            overlays::sync_credits_overlay,
            overlays::update_intensity_knob,
            overlays::update_comfort_toggles,
        )
            .run_if(in_state(AppState::InWorld)),
    )
    .add_systems(Update, photo_capture.run_if(in_state(AppState::InWorld)));

    // Fonts must exist before any schedule runs: some `OnEnter` systems read
    // them, and the state machine's initial transition fires before a
    // `PreStartup` system would. Load them now that plugins (and the
    // `AssetServer`) have been built.
    let asset_server = app.world().resource::<AssetServer>().clone();
    app.insert_resource(Fonts::load(&asset_server));

    // Smoke test: `REVERIE_SMOKE=1 cargo run` drives the app through every UI
    // surface (world → queue → pause) then exits — a headful boot check.
    if std::env::var("REVERIE_SMOKE").is_ok() {
        app.add_systems(Update, smoke_drive);
    }

    app.run();
}

/// Walk the app through its screens for the smoke test, then exit cleanly.
/// Set `REVERIE_SHOT=<dir>` to also save PNG screenshots of each surface.
#[allow(clippy::too_many_arguments)]
fn smoke_drive(
    time: Res<Time>,
    state: Res<State<AppState>>,
    mut next: ResMut<NextState<AppState>>,
    mut queue_open: ResMut<QueueOpen>,
    mut paused: ResMut<Paused>,
    mut settings_open: ResMut<SettingsOpen>,
    mut credits_open: ResMut<CreditsOpen>,
    mut comfort: ResMut<Comfort>,
    mut photo: ResMut<Photo>,
    mut onboarding: ResMut<Onboarding>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut phase: Local<u8>,
) {
    let t = time.elapsed_secs();
    let dir = std::env::var("REVERIE_SHOT").ok();

    // --- Onboarding walk (FirstRun) ---
    if t > 0.4 && *phase == 0 {
        *phase = 1;
        shot(&mut commands, &dir, "reverie-onboard-1.png");
    }
    if t > 0.8 {
        onboarding.step = 1;
    }
    if t > 1.2 && *phase == 1 {
        *phase = 2;
        shot(&mut commands, &dir, "reverie-onboard-2.png");
    }
    if t > 1.6 {
        onboarding.step = 2;
    }
    if t > 2.0 && *phase == 2 {
        *phase = 3;
        shot(&mut commands, &dir, "reverie-onboard-3.png");
    }
    if t > 2.4 && *state.get() == AppState::FirstRun {
        next.set(AppState::InWorld); // spawns the HUD
    }

    // --- In-world walk ---
    if t > 3.0 && *phase == 3 {
        *phase = 4;
        shot(&mut commands, &dir, "reverie-hud.png");
    }
    if t > 3.4 {
        queue_open.0 = true; // spawns the queue panel
    }
    if t > 3.8 {
        paused.0 = true; // spawns the pause overlay
    }
    if t > 4.4 && *phase == 4 {
        *phase = 5;
        shot(&mut commands, &dir, "reverie-overlays.png");
    }
    if t > 4.8 {
        queue_open.0 = false;
        paused.0 = false; // Settings lifts pause (as the real button does)
        settings_open.0 = true;
        comfort.reduce_flashing = true; // show a toggle in the "on" state
    }
    if t > 5.4 && *phase == 5 {
        *phase = 6;
        shot(&mut commands, &dir, "reverie-settings.png");
    }
    if t > 5.8 {
        credits_open.0 = true;
    }
    if t > 6.4 && *phase == 6 {
        *phase = 7;
        shot(&mut commands, &dir, "reverie-credits.png");
    }
    if t > 6.6 {
        credits_open.0 = false;
        settings_open.0 = false;
    }
    // Request the photo only once the chrome has been closed for a while, so
    // the capture lands on a stable, clean frame.
    if t > 7.4 && *phase == 7 {
        *phase = 8;
        photo.request();
    }
    if t > 8.4 {
        exit.write(AppExit::Success);
    }
}

/// Save a screenshot to `<dir>/<name>` when the smoke test requests one.
fn shot(commands: &mut Commands, dir: &Option<String>, name: &str) {
    if let Some(d) = dir {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(format!("{d}/{name}")));
    }
}

fn not_paused(paused: Res<Paused>) -> bool {
    !paused.0
}

/// Photo mode: force the HUD hidden, then capture a clean screenshot to
/// `reverie-photos/`. Runs for a few frames so the chrome fully fades first.
fn photo_capture(
    mut photo: ResMut<Photo>,
    mut activity: ResMut<HudActivity>,
    mut commands: Commands,
) {
    let Some(n) = photo.pending else {
        return;
    };
    // Snap the HUD fully hidden for a clean frame.
    activity.idle = 100.0;
    activity.force_visible = false;
    activity.full = 0.0;
    activity.minimal = 0.0;

    if n == 0 {
        let path = photo_path();
        info!("Reverie photo saved to {path}");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        photo.pending = None;
    } else {
        photo.pending = Some(n - 1);
    }
}

/// A timestamped path under `reverie-photos/` (created on demand).
fn photo_path() -> String {
    let dir = "reverie-photos";
    let _ = std::fs::create_dir_all(dir);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{dir}/reverie-{ms}.png")
}

/// Ease the world timescale toward its target (time-dilation on pause).
fn ease_world_clock(time: Res<Time>, paused: Res<Paused>, mut clock: ResMut<WorldClock>) {
    let target = if paused.0 { 0.05 } else { 1.0 };
    let rate = if paused.0 { 6.0 } else { 4.0 };
    clock.speed += (target - clock.speed) * (time.delta_secs() * rate).min(1.0);
}

/// First-run: any confirm key drops you into the world.
fn input_first_run(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        next.set(AppState::InWorld);
    }
}

/// In-world keybindings (Esc pause · Tab queue · H hide · E pulse · F mode).
#[allow(clippy::too_many_arguments)]
fn input_in_world(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut paused: ResMut<Paused>,
    mut queue_open: ResMut<QueueOpen>,
    mut settings_open: ResMut<SettingsOpen>,
    mut credits_open: ResMut<CreditsOpen>,
    mut photo: ResMut<Photo>,
    mut mode: ResMut<CameraMode>,
    mut activity: ResMut<HudActivity>,
    mut beat: ResMut<Beat>,
    mut playback: ResMut<Playback>,
    mut intensity: ResMut<WorldIntensity>,
) {
    let mut wake = keys.get_just_pressed().len() > 0 || motion.delta != Vec2::ZERO;

    if keys.just_pressed(KeyCode::Escape) {
        // Close the topmost overlay first; only pause when nothing else is up.
        if credits_open.0 {
            credits_open.0 = false;
        } else if settings_open.0 {
            settings_open.0 = false;
        } else {
            paused.0 = !paused.0;
            playback.playing = !paused.0;
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        queue_open.0 = !queue_open.0;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        // Jump straight to Hidden.
        activity.idle = 100.0;
        activity.force_visible = false;
        wake = false;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        beat.pulse = 1.0;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        *mode = match *mode {
            CameraMode::Explore => CameraMode::Drift,
            CameraMode::Drift => CameraMode::Explore,
        };
    }
    if keys.just_pressed(KeyCode::KeyP) {
        // Photo mode — clear the chrome and capture the world.
        photo.request();
        paused.0 = false;
        playback.playing = true;
        queue_open.0 = false;
        settings_open.0 = false;
        credits_open.0 = false;
    }
    if paused.0 {
        if keys.just_pressed(KeyCode::ArrowLeft) {
            intensity.0 = (intensity.0 - 0.05).max(0.0);
        }
        if keys.just_pressed(KeyCode::ArrowRight) {
            intensity.0 = (intensity.0 + 0.05).min(1.0);
        }
    }

    if wake {
        activity.wake();
    }
}
