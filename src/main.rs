//! Reverie — a desktop app that imagines a 3D world for every song.
//!
//! This milestone is **UI-first**: the chrome from the imported design spec is
//! built on Bevy UI over a placeholder mood-tinted world. The music-analysis
//! and asset-assembly pipeline described in `idea.md` comes later; for now the
//! transport and beat are simulated (see [`playback`]).

mod analysis;
mod audio;
mod hud;
mod overlays;
mod playback;
mod theme;
mod world;
mod world_assets;

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

/// Whether the library/home overlay is open.
#[derive(Resource, Default)]
pub struct LibraryOpen(pub bool);

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

/// True while a real audio stream owns the transport clock (see `audio.rs`).
/// When false, `playback::advance_playback` simulates it.
#[derive(Resource, Default)]
pub struct AudioActive(pub bool);

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
    .init_resource::<LibraryOpen>()
    .init_resource::<Photo>()
    .init_resource::<Onboarding>()
    .init_resource::<WorldIntensity>()
    .init_resource::<Comfort>()
    .init_resource::<WorldClock>()
    .init_resource::<world::PaletteWash>()
    .init_resource::<AudioActive>()
    .init_resource::<audio::AudioPlayer>()
    .init_resource::<audio::AudioTap>()
    .init_resource::<audio::ImportState>()
    .init_resource::<analysis::AnalysisStore>()
    .init_resource::<world_assets::WorldAssets>()
    // Setup.
    .add_systems(
        Startup,
        (
            world::setup_world,
            world_assets::load_asset_manifest,
            audio::init_audio,
            auto_import,
        ),
    )
    .add_systems(OnEnter(AppState::FirstRun), overlays::spawn_first_run)
    .add_systems(OnExit(AppState::FirstRun), overlays::despawn_first_run)
    .add_systems(OnEnter(AppState::InWorld), hud::setup_hud)
    // Always-on: buttons + a living world. The palette wash must write hues
    // before the beat-glow rescale reads them, hence the chain.
    .add_systems(Update, (overlays::handle_buttons, ease_world_clock))
    .add_systems(Update, (world::palette_wash, world::animate_world).chain())
    .add_systems(
        Update,
        (input_first_run, overlays::refresh_onboarding).run_if(in_state(AppState::FirstRun)),
    )
    // Audio: the import poll runs everywhere (the scan can start during
    // onboarding); the player syncs only in-world ("audio starts at 0s" on
    // materialize). Chained — each stage feeds the next within a frame.
    .add_systems(Update, audio::poll_import)
    .add_systems(
        Update,
        (
            audio::sync_track_playback,
            audio::sync_pause,
            audio::sync_clock,
        )
            .chain()
            .run_if(in_state(AppState::InWorld)),
    )
    .add_systems(
        Update,
        world::camera_control
            .run_if(in_state(AppState::InWorld))
            .run_if(not_paused),
    )
    // Transport & beat, ordered: analysis applies the grid/sections/mood on a
    // track change, the live tap feeds energy/onsets, then advance decays the
    // pulse, derives phase, and (when simulated) moves the clock.
    .add_systems(
        Update,
        (
            analysis::sync_analysis,
            audio::update_beat_from_tap,
            playback::advance_playback,
        )
            .chain()
            .run_if(in_state(AppState::InWorld)),
    )
    // In-world: input, HUD, overlays.
    .add_systems(
        Update,
        (
            input_in_world,
            world_assets::populate_world_props,
            world_assets::rise_props,
            hud::hud_depth,
            hud::apply_hud_alpha,
            hud::update_hud_accent,
            hud::update_hud_content,
            hud::update_mode_tabs,
            hud::update_reticle,
            hud::mode_tab_clicks,
            hud::chip_clicks,
            overlays::sync_pause_overlay,
            overlays::sync_queue_overlay,
            overlays::sync_settings_overlay,
            overlays::sync_credits_overlay,
            overlays::sync_library_overlay,
            overlays::handle_world_cards,
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
    mut library_open: ResMut<LibraryOpen>,
    mut comfort: ResMut<Comfort>,
    mut photo: ResMut<Photo>,
    mut onboarding: ResMut<Onboarding>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut phase: Local<u8>,
) {
    let t = time.elapsed_secs();
    let dir = std::env::var("REVERIE_SHOT").ok();

    // Single settled capture of the loaded world (avoids the multi-screenshot
    // readback flakiness): straight to the world, one shot at 6s, exit at 8s.
    if std::env::var("REVERIE_ONESHOT").is_ok() {
        if t > 0.5 && *state.get() == AppState::FirstRun {
            next.set(AppState::InWorld);
        }
        if t > 6.0 && *phase == 0 {
            *phase = 1;
            shot(&mut commands, &dir, "reverie-world.png");
        }
        if t > 8.0 {
            exit.write(AppExit::Success);
        }
        return;
    }

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
    if t > 3.3 {
        library_open.0 = true; // spawns the library
    }
    if t > 3.9 && *phase == 4 {
        *phase = 5;
        shot(&mut commands, &dir, "reverie-library.png");
        library_open.0 = false;
    }
    if t > 4.3 {
        queue_open.0 = true; // spawns the queue panel
    }
    if t > 4.7 {
        paused.0 = true; // spawns the pause overlay
    }
    if t > 5.3 && *phase == 5 {
        *phase = 6;
        shot(&mut commands, &dir, "reverie-overlays.png");
    }
    if t > 5.7 {
        queue_open.0 = false;
        paused.0 = false; // Settings lifts pause (as the real button does)
        settings_open.0 = true;
        comfort.reduce_flashing = true; // show a toggle in the "on" state
    }
    if t > 6.3 && *phase == 6 {
        *phase = 7;
        shot(&mut commands, &dir, "reverie-settings.png");
    }
    if t > 6.7 {
        credits_open.0 = true;
    }
    if t > 7.3 && *phase == 7 {
        *phase = 8;
        shot(&mut commands, &dir, "reverie-credits.png");
    }
    if t > 7.5 {
        credits_open.0 = false;
        settings_open.0 = false;
    }
    // Request the photo only once the chrome has been closed for a while, so
    // the capture lands on a stable, clean frame.
    if t > 8.3 && *phase == 8 {
        *phase = 9;
        photo.request();
    }
    if t > 9.3 {
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

/// Dev/smoke hook: `REVERIE_IMPORT=<dir>` imports a folder at startup,
/// skipping the folder picker.
fn auto_import(mut import: ResMut<audio::ImportState>) {
    if let Ok(dir) = std::env::var("REVERIE_IMPORT") {
        audio::start_import(std::path::PathBuf::from(dir), &mut import);
    }
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
    mut library_open: ResMut<LibraryOpen>,
    mut photo: ResMut<Photo>,
    mut mode: ResMut<CameraMode>,
    mut activity: ResMut<HudActivity>,
    mut beat: ResMut<Beat>,
    mut playback: ResMut<Playback>,
    mut theme: ResMut<Theme>,
    mut intensity: ResMut<WorldIntensity>,
) {
    let mut wake = keys.get_just_pressed().len() > 0 || motion.delta != Vec2::ZERO;

    if keys.just_pressed(KeyCode::Escape) {
        // Close the topmost overlay first; only pause when nothing else is up.
        if credits_open.0 {
            credits_open.0 = false;
        } else if settings_open.0 {
            settings_open.0 = false;
        } else if library_open.0 {
            library_open.0 = false;
        } else {
            paused.0 = !paused.0;
            playback.playing = !paused.0;
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        queue_open.0 = !queue_open.0;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        library_open.0 = !library_open.0;
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
    if keys.just_pressed(KeyCode::KeyN) {
        // Next track (the audio player follows `current` and fades over).
        let mood = playback.advance();
        theme.mood = mood;
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
