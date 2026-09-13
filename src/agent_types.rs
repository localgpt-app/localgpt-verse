//! Agent scene-construction data types — always compiled (no feature gate).
//!
//! These are the durable, serializable types shared between the analysis
//! worker (which caches a [`SceneBuild`] in the track sidecar) and the agent
//! runtime (`crate::agent`, `llm` feature, which produces builds). Keeping them
//! ungated means [`crate::analysis::TrackAnalysis`] can carry a
//! `build: Option<SceneBuild>` in every build configuration — old sidecars
//! load, the renderer can replay a cached build without the LLM, and the JSON
//! is human-inspectable for debugging.
//!
//! The marker components ([`AgentEntity`], [`AgentLight`], [`AgentAmbient`])
//! and the [`EnvOverride`] resource live here for the same reason: ungated
//! code (`world.rs`) must be able to exclude agent-owned entities from its
//! queries even in builds that never spawn them.
//!
//! The feature-gated runtime (mistral.rs tool-calling loop, tokio bridge, the
//! Bevy executor) lives in [`crate::agent`].

// These types form the durable agent data model (cached in sidecars, replayed
// by the renderer). Parts of the API are only read under the `llm` feature
// (e.g. AgentResponse during the live loop), so dead-code warnings on the
// not-yet-called surface are expected and allowed in other feature configs.
#![allow(dead_code)]

use std::collections::HashMap;

use bevy::prelude::{Color, Component, Resource};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Scene build — the durable result of an agent session
// ---------------------------------------------------------------------------

/// A fully agent-authored scene: the commands the LLM issued, replayable into
/// any Bevy world. Stored in the track sidecar (like a recipe) so a track's
/// world is rebuilt identically on replay without re-running the LLM.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneBuild {
    /// The ordered commands the agent issued (spawn/modify/...). The renderer
    /// replays these to reconstruct the world deterministically.
    pub commands: Vec<AgentCommand>,
    /// The agent's closing description of the world (its final no-tool-call
    /// message), cached for logging/debugging and future UI surfacing.
    /// `None` when the session ended without one (budget exhausted).
    #[serde(default)]
    pub description: Option<String>,
}

impl SceneBuild {
    /// `true` when no commands were captured (the session produced nothing).
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Track scoping — agent entities belong to the track they were authored for
// ---------------------------------------------------------------------------

/// Marker for every agent-spawned entity, carrying its stable name and the
/// content-hash id of the track whose session spawned it. Entities spawned
/// while a *lookahead* track is being analyzed start hidden and are revealed
/// only when their track becomes current (`sync_agent_scene_scope`), so an
/// ahead-of-playback agent session never pops into the world mid-song.
#[derive(Component)]
pub struct AgentEntity {
    pub name: String,
    pub track: String,
}

/// An agent entity that belongs to a *section* of its track (the agent's
/// `at_role` placement timing): revealed when the transport is in that
/// section, hidden otherwise — structures that rise on the drop or appear
/// only for the bridge.
#[derive(Component)]
pub struct SectionScoped {
    pub role: crate::recipe::SectionRole,
    pub track: String,
}

/// Marker for an agent-owned light, with its authored intensity so it can be
/// zeroed while its track is not current and restored when it is.
#[derive(Component)]
pub struct AgentLight {
    pub name: String,
    pub track: String,
    /// The pre-Comfort, pre-scope intensity (`illuminance` for directional).
    pub base_intensity: f32,
}

/// Marker for the agent's single ambient-light entity (spawned once by
/// `set_environment`, updated by later calls). Excluded from `world.rs`'s
/// palette wash, which owns the world's own ambient.
#[derive(Component)]
pub struct AgentAmbient {
    pub track: String,
    /// The authored brightness, zeroed while the track is not current.
    pub base_brightness: f32,
}

/// The agent's background-colour override, if its track is current. Written by
/// the agent executor (via `set_environment`), read by `world.rs`'s palette
/// wash, which falls back to the mood's sky when `None`. A resource rather
/// than a direct `ClearColor` write so the two writers never fight.
#[derive(Resource, Default)]
pub struct EnvOverride {
    pub background: Option<Color>,
}

// ---------------------------------------------------------------------------
// Command / Response protocol (agent ↔ Bevy)
// ---------------------------------------------------------------------------

/// A single tool-call the agent wants executed against the Bevy world.
/// The agent emits these; Bevy executes them and returns an [`AgentResponse`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentCommand {
    /// Scope everything the session spawns to this track (content-hash id).
    /// Issued once at session start by `run_session`; never recorded in a
    /// [`SceneBuild`] (replay sets its own scope) and never model-facing.
    BeginSession {
        track: String,
    },
    SpawnPrimitive(SpawnPrimitiveCmd),
    /// Place one of the curated CC0 assets from the world manifest (see
    /// `place_asset` in the agent toolset) — the asset-vocabulary sibling of
    /// `spawn_primitive`.
    PlaceAsset(PlaceAssetCmd),
    ModifyEntity(ModifyEntityCmd),
    DeleteEntity {
        name: String,
    },
    SetLight(SetLightCmd),
    SetEnvironment(EnvironmentCmd),
    SceneInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPrimitiveCmd {
    pub name: String,
    pub shape: PrimitiveShape,
    #[serde(default)]
    pub dimensions: HashMap<String, f32>,
    #[serde(default = "zero3")]
    pub position: [f32; 3],
    #[serde(default = "zero3")]
    pub rotation_degrees: [f32; 3],
    #[serde(default = "one3")]
    pub scale: [f32; 3],
    #[serde(default = "default_color")]
    pub color: [f32; 4],
    #[serde(default)]
    pub metallic: f32,
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default = "zero4")]
    pub emissive: [f32; 4],
    /// Song section this structure appears in — the entity stays hidden until
    /// the transport reaches that section (and hides again after). `None` =
    /// visible whenever its track is current.
    #[serde(default)]
    pub at_role: Option<crate::recipe::SectionRole>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PrimitiveShape {
    Cuboid,
    Sphere,
    Cylinder,
    Cone,
    Torus,
    Plane,
}

/// Place a curated manifest asset (CC0 glTF) into the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceAssetCmd {
    pub name: String,
    /// The manifest entry's `file` (relative to `assets/models/`). The tool
    /// schema enumerates the valid values, so the model literally cannot name
    /// an asset that isn't there.
    pub asset: String,
    #[serde(default = "zero3")]
    pub position: [f32; 3],
    #[serde(default = "zero3")]
    pub rotation_degrees: [f32; 3],
    /// Uniform scale multiplier on the asset's normalized placement size.
    #[serde(default = "one_f")]
    pub scale: f32,
    /// Song section this placement appears in (`None` = with the track).
    #[serde(default)]
    pub at_role: Option<crate::recipe::SectionRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyEntityCmd {
    pub name: String,
    pub position: Option<[f32; 3]>,
    pub rotation_degrees: Option<[f32; 3]>,
    pub scale: Option<[f32; 3]>,
    pub color: Option<[f32; 4]>,
    pub metallic: Option<f32>,
    pub roughness: Option<f32>,
    pub emissive: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLightCmd {
    pub name: String,
    #[serde(default = "default_white")]
    pub color: [f32; 4],
    #[serde(default = "default_intensity")]
    pub intensity: f32,
    pub position: Option<[f32; 3]>,
    /// Direction for a directional light (normalized). None → point light.
    pub direction: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCmd {
    #[serde(default = "default_bg")]
    pub background_color: [f32; 4],
    #[serde(default = "default_ambient")]
    pub ambient_light: [f32; 4],
}

/// Bevy's reply to one command. Serialized to a short string for the LLM.
#[derive(Debug, Clone)]
pub enum AgentResponse {
    SessionBegun,
    Spawned {
        name: String,
    },
    AssetPlaced {
        name: String,
    },
    Modified {
        name: String,
    },
    Deleted {
        name: String,
    },
    LightSet {
        name: String,
    },
    EnvironmentSet,
    /// A compact textual summary of the scene (entity names + transforms),
    /// so the agent can reason about what it has built and iterate.
    SceneInfo(String),
    Error(String),
}

impl AgentResponse {
    /// The human/LLM-readable result string.
    pub fn to_message(&self) -> String {
        match self {
            Self::SessionBegun => "session begun".into(),
            Self::Spawned { name } => format!("spawned '{name}'"),
            Self::AssetPlaced { name } => format!("placed asset '{name}'"),
            Self::Modified { name } => format!("modified '{name}'"),
            Self::Deleted { name } => format!("deleted '{name}'"),
            Self::LightSet { name } => format!("light '{name}' set"),
            Self::EnvironmentSet => "environment set".into(),
            Self::SceneInfo(s) => s.clone(),
            Self::Error(e) => format!("error: {e}"),
        }
    }
}

// --- serde defaults --------------------------------------------------------

fn zero3() -> [f32; 3] {
    [0.0, 0.0, 0.0]
}
fn one3() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}
fn zero4() -> [f32; 4] {
    [0.0, 0.0, 0.0, 0.0]
}
fn default_color() -> [f32; 4] {
    [0.8, 0.8, 0.8, 1.0]
}
fn default_white() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
fn default_bg() -> [f32; 4] {
    [0.043, 0.047, 0.067, 1.0]
}
fn default_ambient() -> [f32; 4] {
    [0.3, 0.3, 0.4, 1.0]
}
fn default_roughness() -> f32 {
    0.5
}
fn default_intensity() -> f32 {
    1000.0
}
fn one_f() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_build_roundtrip() {
        let b = SceneBuild {
            commands: vec![AgentCommand::SpawnPrimitive(SpawnPrimitiveCmd {
                name: "tower".into(),
                shape: PrimitiveShape::Cuboid,
                dimensions: HashMap::from([
                    ("x".into(), 2.0),
                    ("y".into(), 8.0),
                    ("z".into(), 2.0),
                ]),
                position: [0.0, 4.0, 0.0],
                rotation_degrees: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                color: [0.2, 0.3, 0.8, 1.0],
                metallic: 0.5,
                roughness: 0.4,
                emissive: [0.0, 0.0, 0.0, 0.0],
                at_role: None,
            })],
            description: Some("a jagged neon skyline".into()),
        };
        let json = serde_json::to_string(&b).unwrap();
        let back: SceneBuild = serde_json::from_str(&json).unwrap();
        assert_eq!(back.commands.len(), 1);
        assert!(!back.is_empty());
    }

    #[test]
    fn empty_scene_build_serializes() {
        let b = SceneBuild::default();
        assert!(b.is_empty());
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("commands"));
    }

    #[test]
    fn place_asset_roundtrip() {
        let b = SceneBuild {
            commands: vec![AgentCommand::PlaceAsset(PlaceAssetCmd {
                name: "gate".into(),
                asset: "rock_arch.glb".into(),
                position: [0.0, 0.0, -6.0],
                rotation_degrees: [0.0, 30.0, 0.0],
                scale: 1.4,
                at_role: None,
            })],
            ..Default::default()
        };
        let json = serde_json::to_string(&b).unwrap();
        let back: SceneBuild = serde_json::from_str(&json).unwrap();
        match &back.commands[0] {
            AgentCommand::PlaceAsset(c) => {
                assert_eq!(c.asset, "rock_arch.glb");
                assert_eq!(c.scale, 1.4);
            }
            _ => panic!("wrong variant"),
        }
        // Old sidecars (no description field) still deserialize.
        let legacy = json.replace(&serde_json::to_string(&b.description).unwrap(), "null");
        assert!(serde_json::from_str::<SceneBuild>(&legacy).is_ok());
    }
}
