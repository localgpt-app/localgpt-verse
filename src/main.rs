//! Reverie — placeholder Bevy application.
//!
//! A minimal 3D scene (ground plane, cube, directional light, camera) that
//! bootstraps the project. Replace [`setup`] with real content as Reverie
//! takes shape.

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Reverie".to_string(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.10, 0.10, 0.12)))
        .add_systems(Startup, setup)
        .run();
}

/// Spawn a placeholder scene: a ground plane, a cube, a light, and a camera.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(10.0, 10.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.30, 0.50, 0.30))),
        Name::new("ground"),
    ));

    // Cube.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.80, 0.70, 0.60))),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Name::new("cube"),
    ));

    // Directional light.
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        Name::new("sun"),
    ));

    // Camera.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-3.0, 4.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        Name::new("main_camera"),
    ));
}
