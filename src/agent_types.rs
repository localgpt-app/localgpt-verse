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
//! The feature-gated runtime (mistral.rs tool-calling loop, tokio bridge, the
//! Bevy executor) lives in [`crate::agent`].

// These types form the durable agent data model (cached in sidecars, replayed
// by the renderer). Parts of the API are only read under the `llm` feature
// (e.g. AgentResponse during the live loop), so dead-code warnings on the
// not-yet-called surface are expected and allowed in other feature configs.
#![allow(dead_code)]

use std::collections::HashMap;

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
}

impl SceneBuild {
    /// `true` when no commands were captured (the session produced nothing).
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Command / Response protocol (agent ↔ Bevy)
// ---------------------------------------------------------------------------

/// A single tool-call the agent wants executed against the Bevy world.
/// The agent emits these; Bevy executes them and returns an [`AgentResponse`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentCommand {
    SpawnPrimitive(SpawnPrimitiveCmd),
    ModifyEntity(ModifyEntityCmd),
    DeleteEntity { name: String },
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
    Spawned {
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
            Self::Spawned { name } => format!("spawned '{name}'"),
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
            })],
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
}
