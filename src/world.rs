//! A placeholder 3D world for the HUD to overlay.
//!
//! This is intentionally abstract — the real pipeline assembles worlds from a
//! catalog of glTF assets driven by music (see `idea.md`). For the UI-first
//! milestone we just need a living, mood-tinted backdrop: a ground plane, a
//! field of slowly drifting shapes, fog, and two camera feels (Explore / Drift)
//! so the chrome always has "a live world to justify itself against".

use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::pbr::DistanceFog;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::playback::Beat;
use crate::theme::Theme;
use crate::{CameraMode, WorldClock};

/// Marker for the single world camera.
#[derive(Component)]
pub struct WorldCamera {
    /// Yaw/pitch used in Explore mode.
    pub yaw: f32,
    pub pitch: f32,
    /// Auto-orbit angle used in Drift mode.
    pub orbit: f32,
}

/// A drifting decorative shape.
#[derive(Component)]
pub struct Drifter {
    pub seed: f32,
    pub base: Vec3,
}

/// Handles to the shared world materials so palette swaps are cheap.
#[derive(Resource)]
pub struct WorldMaterials {
    pub ground: Handle<StandardMaterial>,
    pub drifter: Handle<StandardMaterial>,
}

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    theme: Res<Theme>,
) {
    let mood = theme.current();

    // Camera — HDR + bloom give the "HDR bloom seed" glow the spec asks for.
    commands.spawn((
        Camera3d::default(),
        Hdr,
        Tonemapping::TonyMcMapface,
        Bloom::NATURAL,
        DistanceFog {
            color: mood.fog,
            falloff: FogFalloff::Linear {
                start: 18.0,
                end: 95.0,
            },
            ..default()
        },
        Transform::from_xyz(0.0, 3.0, 14.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y),
        // Ambient light is a per-camera component in Bevy 0.19.
        AmbientLight {
            color: mood.ambient,
            brightness: 260.0,
            ..default()
        },
        WorldCamera {
            yaw: 0.0,
            pitch: -0.08,
            orbit: 0.0,
        },
    ));

    // Key light.
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Shared materials.
    let ground_mat = materials.add(StandardMaterial {
        base_color: mood.ground,
        perceptual_roughness: 0.95,
        ..default()
    });
    let drifter_mat = materials.add(StandardMaterial {
        base_color: mood.sky_bottom,
        emissive: scaled_linear(mood.accent, 0.6),
        perceptual_roughness: 0.35,
        metallic: 0.1,
        ..default()
    });
    commands.insert_resource(WorldMaterials {
        ground: ground_mat.clone(),
        drifter: drifter_mat.clone(),
    });

    // Ground.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(240.0, 240.0))),
        MeshMaterial3d(ground_mat),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));

    // A field of drifting shapes, placed on a deterministic golden-angle
    // spiral so no RNG dependency is needed.
    let cube = meshes.add(Cuboid::new(0.7, 0.7, 0.7));
    let sphere = meshes.add(Sphere::new(0.5).mesh().ico(3).unwrap());
    let tet = meshes.add(Tetrahedron::default().mesh());
    let count = 90;
    for i in 0..count {
        let fi = i as f32;
        let ang = fi * 2.399_963; // golden angle
        let radius = (fi + 4.0).sqrt() * 3.2;
        let x = ang.cos() * radius;
        let z = ang.sin() * radius - 8.0;
        let y = 1.0 + ((fi * 1.7).sin() * 0.5 + 0.5) * 5.0;
        let base = Vec3::new(x, y, z);
        let mesh = match i % 3 {
            0 => cube.clone(),
            1 => sphere.clone(),
            _ => tet.clone(),
        };
        let scale = 0.6 + (fi * 0.37).fract() * 1.3;
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(drifter_mat.clone()),
            Transform::from_translation(base).with_scale(Vec3::splat(scale)),
            Drifter {
                seed: fi * 0.613,
                base,
            },
        ));
    }
}

/// Re-tint the world when the mood changes.
pub fn update_world_palette(
    theme: Res<Theme>,
    world_mats: Option<Res<WorldMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient_q: Query<&mut AmbientLight>,
    mut fog_q: Query<&mut DistanceFog>,
    mut clear: ResMut<ClearColor>,
) {
    if !theme.is_changed() {
        return;
    }
    let mood = theme.current();

    // Sky tint sits behind the fog.
    clear.0 = mood.sky_top;
    for mut ambient in &mut ambient_q {
        ambient.color = mood.ambient;
    }
    for mut fog in &mut fog_q {
        fog.color = mood.fog;
    }

    let Some(world_mats) = world_mats else { return };
    if let Some(mut m) = materials.get_mut(&world_mats.ground) {
        m.base_color = mood.ground;
    }
    if let Some(mut m) = materials.get_mut(&world_mats.drifter) {
        m.base_color = mood.sky_bottom;
        m.emissive = scaled_linear(mood.accent, 0.6);
    }
}

/// Drift the shapes; pulse their glow with the beat; obey the world clock so a
/// paused world slows to a near-freeze (time-dilation).
pub fn animate_world(
    time: Res<Time>,
    clock: Res<WorldClock>,
    beat: Res<Beat>,
    world_mats: Option<Res<WorldMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut drifters: Query<(&Drifter, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs() * clock.speed;

    for (d, mut tf) in &mut drifters {
        let p = t * clock.speed;
        tf.translation.y = d.base.y + (p * 0.4 + d.seed * std::f32::consts::TAU).sin() * 0.6;
        tf.translation.x = d.base.x + (p * 0.23 + d.seed * std::f32::consts::PI).cos() * 0.4;
        tf.rotate_y(dt * (0.2 + d.seed.fract() * 0.4));
    }

    // Beat-reactive emissive on the shared drifter material. (Split out of a
    // let-chain: chained `let` bindings are read-only in Rust 2024.)
    let Some(world_mats) = world_mats else { return };
    if let Some(mut material) = materials.get_mut(&world_mats.drifter) {
        let glow = 0.55 + beat.pulse * 0.9 * beat.energy;
        material.emissive = scaled_linear_from(material.emissive, glow);
    }
}

/// Camera feel: Drift auto-orbits slowly; Explore is WASD + mouse-look.
pub fn camera_control(
    time: Res<Time>,
    clock: Res<WorldClock>,
    mode: Res<CameraMode>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    mut cam_q: Query<(&mut Transform, &mut WorldCamera)>,
) {
    let Ok((mut tf, mut cam)) = cam_q.single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    match *mode {
        CameraMode::Drift => {
            cam.orbit += dt * 0.06 * clock.speed;
            let r = 16.0;
            let target = Vec3::new(0.0, 2.2, -6.0);
            tf.translation = target + Vec3::new(cam.orbit.cos() * r, 3.0, cam.orbit.sin() * r);
            tf.look_at(target, Vec3::Y);
        }
        CameraMode::Explore => {
            // Mouse-look (no cursor grab — subtle, always-on).
            let d = motion.delta;
            cam.yaw -= d.x * 0.0022;
            cam.pitch = (cam.pitch - d.y * 0.0022).clamp(-1.2, 0.6);
            let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
            tf.rotation = rot;

            // WASD move on the yaw plane.
            let mut mv = Vec3::ZERO;
            let fwd = rot * Vec3::NEG_Z;
            let right = rot * Vec3::X;
            let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
            if keys.pressed(KeyCode::KeyW) {
                mv += flat(fwd);
            }
            if keys.pressed(KeyCode::KeyS) {
                mv -= flat(fwd);
            }
            if keys.pressed(KeyCode::KeyD) {
                mv += flat(right);
            }
            if keys.pressed(KeyCode::KeyA) {
                mv -= flat(right);
            }
            tf.translation += mv.normalize_or_zero() * dt * 7.0;
            tf.translation.y = tf.translation.y.clamp(0.8, 12.0);
        }
    }
}

// --- colour helpers -------------------------------------------------------

/// `color * intensity` as a linear-RGB emissive value.
fn scaled_linear(color: Color, intensity: f32) -> LinearRgba {
    let l = color.to_linear();
    LinearRgba::rgb(l.red * intensity, l.green * intensity, l.blue * intensity)
}

/// Rescale an existing emissive to a new intensity while keeping its hue.
fn scaled_linear_from(current: LinearRgba, intensity: f32) -> LinearRgba {
    // Normalise by the max channel so repeated scaling doesn't drift to black.
    let max = current.red.max(current.green).max(current.blue).max(1e-4);
    LinearRgba::rgb(
        current.red / max * intensity,
        current.green / max * intensity,
        current.blue / max * intensity,
    )
}
