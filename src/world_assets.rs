//! Real 3D world props — PLAN.md M6.
//!
//! Loads a manifest of CC0 glTF models (bundled from the `reverie-assets`
//! repo into `assets/models/`) and places them on the ground per world mood,
//! in three tiers (hero landmarks / medium props / ground scatter). These are
//! grounded *features*; the procedural drifters in `world.rs` stay as floating
//! ambient. When no manifest is present (fresh clone without the asset pack)
//! or a mood has no matching assets, the world simply keeps its primitives —
//! so the app always runs.

use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use serde::Deserialize;

use crate::theme::Theme;

/// Placement tier — governs instance count, size, and spread.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Hero,
    Medium,
    Scatter,
}

impl Tier {
    /// How many instances of each asset in this tier to place.
    fn count(self) -> usize {
        match self {
            Tier::Hero => 3,
            Tier::Medium => 5,
            Tier::Scatter => 9,
        }
    }
    /// Base scale multiplier (glTF are real-world metres; a touch larger so
    /// props read as landmarks without dwarfing the scene).
    fn base_scale(self) -> f32 {
        match self {
            Tier::Hero => 1.5,
            Tier::Medium => 1.15,
            Tier::Scatter => 0.9,
        }
    }
}

/// One manifest entry. Provenance fields (author/license/source) double as the
/// Credits screen's data + the CC0/CC-BY audit trail.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetEntry {
    /// Stable id / source URL — provenance kept in the manifest (audit trail)
    /// even though the UI currently renders only name/author/license.
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    /// glTF path relative to `assets/models/`.
    pub file: String,
    pub tier: Tier,
    /// Index into [`crate::theme::MOODS`].
    pub mood: usize,
    #[serde(default = "one")]
    pub scale: f32,
    pub license: String,
    pub author: String,
    #[allow(dead_code)]
    pub source: String,
}

fn one() -> f32 {
    1.0
}

impl AssetEntry {
    /// Human tier name for the Credits row.
    pub fn tier_label(&self) -> &'static str {
        match self.tier {
            Tier::Hero => "hero landmark",
            Tier::Medium => "prop",
            Tier::Scatter => "ground cover",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetManifest {
    #[allow(dead_code)]
    pub version: u32,
    pub assets: Vec<AssetEntry>,
}

/// The loaded manifest (None when no asset pack is bundled). Read by both the
/// world populator and the Credits screen.
#[derive(Resource, Default)]
pub struct WorldAssets {
    pub manifest: Option<AssetManifest>,
}

/// Marker for spawned prop entities, so a mood change can clear them.
#[derive(Component)]
pub struct WorldProp;

/// The asset root Bevy uses: `CARGO_MANIFEST_DIR/assets` under `cargo run`
/// (baked at compile time, matching Bevy's own resolution), else `assets`
/// relative to the working dir (shipped/exe-relative).
pub(crate) fn asset_root() -> std::path::PathBuf {
    let compiled = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    if compiled.exists() {
        compiled
    } else {
        std::path::PathBuf::from("assets")
    }
}

/// Load `assets/models/manifest.json` if present. Absent → procedural worlds.
pub fn load_asset_manifest(mut commands: Commands) {
    let path = asset_root().join("models/manifest.json");
    let manifest =
        std::fs::read_to_string(&path).ok().and_then(|text| {
            match serde_json::from_str::<AssetManifest>(&text) {
                Ok(m) => Some(m),
                Err(e) => {
                    warn!("Ignoring asset manifest ({e})");
                    None
                }
            }
        });
    match &manifest {
        Some(m) => info!("Asset pack: {} models", m.assets.len()),
        None => info!("No asset pack bundled — procedural worlds"),
    }
    commands.insert_resource(WorldAssets { manifest });
}

/// (Re)place ground props for the current mood on a mood change.
pub fn populate_world_props(
    theme: Res<Theme>,
    assets: Res<WorldAssets>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    existing: Query<Entity, With<WorldProp>>,
    mut last_mood: Local<Option<usize>>,
) {
    let mood = theme.mood % crate::theme::MOODS.len();
    if *last_mood == Some(mood) {
        return;
    }
    *last_mood = Some(mood);

    for e in &existing {
        commands.entity(e).despawn();
    }
    let Some(manifest) = &assets.manifest else {
        return;
    };

    // Deterministic golden-angle scatter on the ground plane (top at y=-0.5).
    let golden = 2.399_963_f32;
    let mut placed = 0u32;
    for entry in manifest.assets.iter().filter(|a| a.mood == mood) {
        let handle: Handle<_> = asset_server
            .load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}", entry.file)));
        let scale = entry.scale * entry.tier.base_scale();
        for _ in 0..entry.tier.count() {
            let fi = placed as f32;
            let ang = fi * golden;
            let radius = 6.0 + (fi + 2.0).sqrt() * 4.2;
            let pos = Vec3::new(ang.cos() * radius, -0.5, ang.sin() * radius);
            commands.spawn((
                WorldProp,
                WorldAssetRoot(handle.clone()),
                Transform::from_translation(pos)
                    .with_scale(Vec3::splat(scale))
                    .with_rotation(Quat::from_rotation_y(ang * 1.7)),
            ));
            placed += 1;
        }
    }
    if placed > 0 {
        info!(
            "Placed {placed} props for {}",
            crate::theme::MOODS[mood].world_name
        );
    }
}
