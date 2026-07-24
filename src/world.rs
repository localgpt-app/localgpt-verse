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

/// An ambient particle (M7 recipe `particles`). Drifts upward or downward per
/// `kind`, recycled when it leaves the volume. Spawned/refreshed on recipe
/// change by [`spawn_particles`]; animated by [`animate_world`].
#[derive(Component)]
pub struct Particle {
    pub kind: crate::recipe::ParticleKind,
    pub seed: f32,
    /// Home position (kept for a future "settle back" reset; recycle currently
    /// clamps Y in place).
    #[allow(dead_code)]
    pub base: Vec3,
    /// Per-particle drift speed multiplier.
    pub drift: f32,
}

/// Half-extent of the particle volume (x/z spread, max height).
const PARTICLE_SPREAD: f32 = 30.0;
const PARTICLE_MAX_Y: f32 = 18.0;

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

/// One paintable snapshot of the world's colours.
#[derive(Clone, Copy)]
struct Palette {
    sky_top: Color,
    ambient: Color,
    fog: Color,
    ground: Color,
    drift_base: Color,
    accent: Color,
}

impl Palette {
    fn of(mood: &crate::theme::WorldMood) -> Self {
        Self {
            sky_top: mood.sky_top,
            ambient: mood.ambient,
            fog: mood.fog,
            ground: mood.ground,
            drift_base: mood.sky_bottom,
            accent: mood.accent,
        }
    }

    fn mix(&self, to: &Self, f: f32) -> Self {
        use bevy::color::Mix as _;
        Self {
            sky_top: self.sky_top.mix(&to.sky_top, f),
            ambient: self.ambient.mix(&to.ambient, f),
            fog: self.fog.mix(&to.fog, f),
            ground: self.ground.mix(&to.ground, f),
            drift_base: self.drift_base.mix(&to.drift_base, f),
            accent: self.accent.mix(&to.accent, f),
        }
    }
}

/// Palette-wash duration (spec 1g: "palette wash 0.8s").
const WASH_SECS: f32 = 0.8;

/// Eased world-recolour state — the first beat of the materialize sequence.
#[derive(Resource)]
pub struct PaletteWash {
    current: Palette,
    from: Palette,
    t: f32,
    active: bool,
    last_mood: Option<usize>,
}

impl Default for PaletteWash {
    fn default() -> Self {
        let p = Palette::of(&crate::theme::MOODS[0]);
        Self {
            current: p,
            from: p,
            t: 0.0,
            active: false,
            last_mood: Some(0),
        }
    }
}

/// Ease the world's colours toward the current mood instead of snapping
/// (spec 1g's 0.8s palette wash). Runs before `animate_world`, which rescales
/// the drifter emissive per-beat while keeping the washed hue.
#[allow(clippy::too_many_arguments)]
pub fn palette_wash(
    time: Res<Time>,
    theme: Res<Theme>,
    mut wash: ResMut<PaletteWash>,
    world_mats: Option<Res<WorldMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient_q: Query<&mut AmbientLight>,
    mut fog_q: Query<&mut DistanceFog>,
    mut clear: ResMut<ClearColor>,
) {
    let mood_idx = theme.mood % crate::theme::MOODS.len();
    if wash.last_mood != Some(mood_idx) {
        wash.last_mood = Some(mood_idx);
        wash.from = wash.current;
        wash.t = 0.0;
        wash.active = true;
    }
    if !wash.active {
        return;
    }

    wash.t += time.delta_secs();
    let f = (wash.t / WASH_SECS).clamp(0.0, 1.0);
    let s = f * f * (3.0 - 2.0 * f); // smoothstep
    let to = Palette::of(theme.current());
    wash.current = wash.from.mix(&to, s);
    if f >= 1.0 {
        wash.active = false;
    }

    let p = wash.current;
    clear.0 = p.sky_top;
    for mut ambient in &mut ambient_q {
        ambient.color = p.ambient;
    }
    for mut fog in &mut fog_q {
        fog.color = p.fog;
    }
    let Some(world_mats) = world_mats else { return };
    if let Some(mut m) = materials.get_mut(&world_mats.ground) {
        m.base_color = p.ground;
    }
    if let Some(mut m) = materials.get_mut(&world_mats.drifter) {
        m.base_color = p.drift_base;
        m.emissive = scaled_linear(p.accent, 0.6);
    }
}

/// Spawn/refresh the ambient particle field from the recipe (M7). Runs each
/// frame but only rebuilds when the recipe's particle signature changes
/// (kind and rounded rate), so it's cheap. No recipe or rate 0 means no
/// particles (and any existing ones are despawned). Seeded deterministically
/// from the recipe seed.
pub fn spawn_particles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    active_recipe: Res<crate::recipe::ActiveRecipe>,
    theme: Res<Theme>,
    existing: Query<Entity, With<Particle>>,
    mut last: Local<Option<(crate::recipe::ParticleKind, u32)>>,
) {
    let signature = active_recipe.get().and_then(|r| {
        (r.particles.rate > 0.0).then_some((r.particles.kind, (r.particles.rate * 10.0) as u32))
    });

    // Rebuild only on a signature change (kind, or rate to 1 decimal).
    if *last == signature {
        return;
    }
    *last = signature;

    // Always clear first (covers rate→0 and kind swaps).
    for e in &existing {
        commands.entity(e).despawn();
    }

    let Some(recipe) = active_recipe.get() else {
        return;
    };
    if recipe.particles.rate <= 0.0 {
        return;
    }

    // Particle count scales with rate (capped for perf). rate 0..1 → 0..200.
    let count = (recipe.particles.rate * 200.0).round() as usize;
    let mood = theme.current();
    let tint = match recipe.particles.kind {
        crate::recipe::ParticleKind::Ember => mood.accent,
        crate::recipe::ParticleKind::Spark => mood.accent,
        crate::recipe::ParticleKind::Snow => mood.sky_top,
        crate::recipe::ParticleKind::Spore => mood.ambient,
        crate::recipe::ParticleKind::Dust => mood.fog,
    };
    let mesh = meshes.add(Sphere::new(0.06).mesh().ico(2).unwrap());
    // One shared material for the whole field (cheap); per-kind tint only.
    let material = materials.add(StandardMaterial {
        base_color: tint.with_alpha(0.7),
        emissive: scaled_linear(tint, 0.5),
        unlit: true,
        ..default()
    });
    let mut rng = recipe.seed;
    for _ in 0..count {
        let x = (crate::world_assets::splitmix(&mut rng) as f32 / u32::MAX as f32 - 0.5)
            * 2.0
            * PARTICLE_SPREAD;
        let y = (crate::world_assets::splitmix(&mut rng) as f32 / u32::MAX as f32) * PARTICLE_MAX_Y;
        let z = (crate::world_assets::splitmix(&mut rng) as f32 / u32::MAX as f32 - 0.5)
            * 2.0
            * PARTICLE_SPREAD;
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, y, z),
            Particle {
                kind: recipe.particles.kind,
                seed: crate::world_assets::splitmix(&mut rng) as f32 / u32::MAX as f32,
                base: Vec3::new(x, y, z),
                drift: recipe.particles.drift,
            },
        ));
    }
}

/// Drift the shapes; pulse their glow with the beat; obey the world clock so a
/// paused world slows to a near-freeze (time-dilation).
#[allow(clippy::too_many_arguments)]
pub fn animate_world(
    time: Res<Time>,
    clock: Res<WorldClock>,
    beat: Res<Beat>,
    comfort: Res<crate::Comfort>,
    active_recipe: Res<crate::recipe::ActiveRecipe>,
    world_mats: Option<Res<WorldMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut drifters: Query<(&Drifter, &mut Transform), Without<Particle>>,
    mut particles: Query<(&Particle, &mut Transform), Without<Drifter>>,
) {
    let t = time.elapsed_secs();
    let dt = time.delta_secs() * clock.speed;
    // Comfort › Gentler world motion damps the sway (spec 1n).
    let gentle = if comfort.gentler_motion { 0.4 } else { 1.0 };
    // M7: a recipe may scale drift speed within [0.25, 2.5] (already clamped).
    // Absent recipe → 1.0 (today's behaviour).
    let motion = active_recipe.get().map(|r| r.motion_speed).unwrap_or(1.0);

    for (d, mut tf) in &mut drifters {
        let p = t * clock.speed;
        tf.translation.y =
            d.base.y + (p * 0.4 * motion + d.seed * std::f32::consts::TAU).sin() * 0.6 * gentle;
        tf.translation.x =
            d.base.x + (p * 0.23 * motion + d.seed * std::f32::consts::PI).cos() * 0.4 * gentle;
        tf.rotate_y(dt * (0.2 + d.seed.fract() * 0.4) * gentle * motion);
    }

    // M7: drift the recipe's particle layer. Direction is kind-dependent
    // (embers/sparks rise, snow falls, dust/spores hover), speed scaled by the
    // recipe's drift and the global motion multiplier. Particles recycle to the
    // bottom (rising kinds) or top (falling) when they leave the volume.
    use crate::recipe::ParticleKind;
    for (p, mut tf) in &mut particles {
        let vertical = match p.kind {
            ParticleKind::Ember | ParticleKind::Spark => 1.0, // rise
            ParticleKind::Snow => -0.6,                       // fall
            ParticleKind::Dust | ParticleKind::Spore => 0.15, // hover
        };
        let speed = vertical * p.drift * motion * gentle * 1.5;
        tf.translation.y += dt * speed;
        // Lateral wander for life.
        tf.translation.x += (t * 0.5 + p.seed * std::f32::consts::TAU).sin() * dt * 0.3 * gentle;
        // Recycle when out of bounds.
        if tf.translation.y > PARTICLE_MAX_Y {
            tf.translation.y = 0.0;
        } else if tf.translation.y < 0.0 {
            tf.translation.y = PARTICLE_MAX_Y;
        }
    }

    // Beat-reactive emissive on the shared drifter material. (Split out of a
    // let-chain: chained `let` bindings are read-only in Rust 2024.)
    let Some(world_mats) = world_mats else { return };
    if let Some(mut material) = materials.get_mut(&world_mats.drifter) {
        // Comfort › Reduce flashing holds the glow steady (no beat pulse).
        let glow = if comfort.reduce_flashing {
            0.7
        } else {
            0.55 + beat.pulse * 0.9 * beat.energy
        };
        // M7: scale the glow by the recipe's bloom ceiling (1.0 = today).
        let glow = glow
            * active_recipe
                .get()
                .map(|r| r.atmosphere.bloom_ceiling)
                .unwrap_or(1.0);
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
