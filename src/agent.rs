//! LLM scene-construction agent — a simplified port of `localgpt-gen`'s
//! tool-calling pattern (PLAN.md M7, `llm` feature).
//!
//! Unlike the [`crate::recipe`] path (where Bonsai emits one static JSON
//! object that the renderer interprets), this module makes Bonsai a true
//! **agent**: it calls tools — `spawn_primitive`, `set_light`, `scene_info`,
//! ... — that actually construct the 3D world entity-by-entity, sees the
//! results, and iterates. The world is authored by the LLM, not just
//! parameterized by it.
//!
//! # Architecture (mirrors `crates/gen/src/gen3d`, trimmed to essentials)
//!
//! ```text
//! ┌──────────────────────────┐   mpsc channels   ┌─────────────────────┐
//! │ Agent loop (tokio)       │ ◄────────────────►│ Bevy (main thread)  │
//! │  - Bonsai via mistral.rs │  AgentCommand ──►  │  - drains each frame│
//! │  - 6 tool schemas        │  ◄── AgentResponse │  - name registry    │
//! │  - executes tool_calls   │                    │  - spawns entities  │
//! └──────────────────────────┘                    └─────────────────────┘
//! ```
//!
//! Core tools (the simplified set): spawn_primitive, modify_entity,
//! delete_entity, set_light, set_environment, scene_info.
//!
//! # Graceful degradation
//! No `llm` feature → module not compiled; feature but no model → the agent
//! returns an empty [`SceneBuild`] and the renderer keeps the rule-derived
//! world. Same contract as [`crate::ml`] / [`crate::demucs`].
//!
//! # Status: scaffolded, not yet load-bearing
//! The protocol, bridge, executor, and tool schemas are complete and compile,
//! but the live agent loop (`run_session`) returns `None` until the mistral.rs
//! tool-calling API is confirmed at runtime AND the bridge is threaded from
//! `main()` into the analysis worker. Until then, dead-code warnings on the
//! not-yet-called agent API are expected and allowed.

#![allow(dead_code)]
#![allow(clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::sync::Arc;

use bevy::log::{info, warn};
use serde_json::{Value, json};

// Re-export the always-compiled data types so callers can reach them via
// `crate::agent::SceneBuild` etc., while the types themselves live in the
// ungated `agent_types` module (so `TrackAnalysis` can carry them without the
// `llm` feature).
pub use crate::agent_types::{
    AgentCommand, AgentResponse, EnvironmentCmd, ModifyEntityCmd, PrimitiveShape, SceneBuild,
    SetLightCmd, SpawnPrimitiveCmd,
};

// The bridge's async channels are tokio mpsc (matches gen's pattern). The
// command/response types are plain structs so the Bevy side (sync) can move
// them across the frame boundary without an async runtime.
use tokio::sync::{Mutex, mpsc};

use crate::analysis::TrackAnalysis;
use crate::theme::MOODS;

// ---------------------------------------------------------------------------
// (SceneBuild + AgentCommand + cmd structs live in crate::agent_types, always
// compiled, re-exported above. The feature-gated runtime begins below.)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Bridge — async agent ↔ sync Bevy
// ---------------------------------------------------------------------------

/// The agent's handle to send commands and await responses. Cloned cheaply
/// (Arc) so the agent loop and the tool layer share it.
pub struct AgentBridge {
    cmd_tx: mpsc::UnboundedSender<AgentCommand>,
    resp_rx: Mutex<mpsc::UnboundedReceiver<AgentResponse>>,
}

impl AgentBridge {
    /// Send a command and await Bevy's reply.
    pub async fn send(&self, cmd: AgentCommand) -> AgentResponse {
        if self.cmd_tx.send(cmd).is_err() {
            return AgentResponse::Error("Bevy side closed".into());
        }
        let mut rx = self.resp_rx.lock().await;
        rx.recv()
            .await
            .unwrap_or_else(|| AgentResponse::Error("no reply".into()))
    }
}

/// Global handle to the bridge, set once in `main()` before the analysis worker
/// spawns, and read by the worker to run agent sessions. `None` until `main()`
/// installs it (or when the `llm` feature is off). Using a `OnceLock` here is
/// the simplest way to thread an Arc from the Bevy app setup into the worker's
/// std::thread without restructuring `AnalysisStore::default()`.
static AGENT_BRIDGE: std::sync::OnceLock<Option<Arc<AgentBridge>>> = std::sync::OnceLock::new();

/// Install the bridge globally. Called once from `main()` under the `llm`
/// feature. The worker reads it via [`agent_bridge`].
pub fn install_bridge(bridge: Arc<AgentBridge>) {
    let _ = AGENT_BRIDGE.set(Some(bridge));
}

/// The bridge the analysis worker uses to run agent sessions, or `None` when
/// the feature is off or `main()` hasn't installed it yet.
pub fn agent_bridge() -> Option<Arc<AgentBridge>> {
    AGENT_BRIDGE.get().and_then(|opt| opt.clone())
}

/// The Bevy-side channels, held as a resource and drained each frame.
pub struct AgentChannels {
    pub cmd_rx: mpsc::UnboundedReceiver<AgentCommand>,
    pub resp_tx: mpsc::UnboundedSender<AgentResponse>,
}

/// Create the matched (bridge, channels) pair — one per app instance.
pub fn create_channels() -> (Arc<AgentBridge>, AgentChannels) {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (resp_tx, resp_rx) = mpsc::unbounded_channel();
    let bridge = Arc::new(AgentBridge {
        cmd_tx,
        resp_rx: Mutex::new(resp_rx),
    });
    (bridge, AgentChannels { cmd_rx, resp_tx })
}

// ---------------------------------------------------------------------------
// Tool schemas — the LLM-facing JSON-Schema for each of the 6 core tools
// ---------------------------------------------------------------------------

/// All 6 core tool definitions, as mistral.rs `Tool`s ready for `set_tools`.
/// Each maps 1:1 to an [`AgentCommand`] variant executed by [`AgentExecutor`].
pub fn tool_schemas() -> Vec<mistralrs::Tool> {
    use mistralrs::{Function, Tool, ToolType};
    /// helper: build a Function from name/description/parameters JSON.
    fn f(name: &str, desc: &str, params: Value) -> Tool {
        Tool {
            tp: ToolType::Function,
            function: Function {
                description: Some(desc.into()),
                name: name.into(),
                parameters: Some(serde_json::from_value(params).unwrap_or_default()),
            },
        }
    }
    vec![
        f(
            "spawn_primitive",
            "Spawn a 3D primitive shape (Cuboid/Sphere/Cylinder/Cone/Torus/Plane) with a material and transform. This is how you build the world — call it many times to compose structures.",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Unique name for this entity (e.g. 'tower_base', 'crystal_1')"},
                    "shape": {"type": "string", "enum": ["Cuboid","Sphere","Cylinder","Cone","Torus","Plane"]},
                    "dimensions": {"type": "object", "description": "Cuboid:{x,y,z}. Sphere:{radius}. Cylinder:{radius,height}. Cone:{radius,height}. Torus:{major_radius,minor_radius}. Plane:{x,z}."},
                    "position": {"type": "array", "items": {"type":"number"}, "default": [0,0,0]},
                    "rotation_degrees": {"type": "array", "items": {"type":"number"}, "default": [0,0,0]},
                    "scale": {"type": "array", "items": {"type":"number"}, "default": [1,1,1]},
                    "color": {"type": "array", "items": {"type":"number"}, "default": [0.8,0.8,0.8,1.0], "description": "RGBA 0-1"},
                    "metallic": {"type": "number", "default": 0.0, "minimum": 0, "maximum": 1},
                    "roughness": {"type": "number", "default": 0.5, "minimum": 0, "maximum": 1},
                    "emissive": {"type": "array", "items": {"type":"number"}, "default": [0,0,0,0], "description": "Glow color RGBA"}
                },
                "required": ["name", "shape"]
            }),
        ),
        f(
            "modify_entity",
            "Partially update an existing entity by name. Only provided fields change.",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "position": {"type": "array", "items": {"type":"number"}},
                    "scale": {"type": "array", "items": {"type":"number"}},
                    "color": {"type": "array", "items": {"type":"number"}},
                    "emissive": {"type": "array", "items": {"type":"number"}}
                },
                "required": ["name"]
            }),
        ),
        f(
            "delete_entity",
            "Delete an entity by name.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
        ),
        f(
            "set_light",
            "Add or update a light. Omit direction for a point light; provide it for a directional (sun) light.",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "color": {"type": "array", "items": {"type":"number"}, "default": [1,1,1,1]},
                    "intensity": {"type": "number", "default": 1000},
                    "position": {"type": "array", "items": {"type":"number"}},
                    "direction": {"type": "array", "items": {"type":"number"}, "description": "Direction vector for a sun/directional light"}
                },
                "required": ["name"]
            }),
        ),
        f(
            "set_environment",
            "Set the background color and ambient light.",
            json!({
                "type": "object",
                "properties": {
                    "background_color": {"type": "array", "items": {"type":"number"}, "default": [0.04,0.05,0.07,1]},
                    "ambient_light": {"type": "array", "items": {"type":"number"}, "default": [0.3,0.3,0.4,1]}
                }
            }),
        ),
        f(
            "scene_info",
            "List all currently-spawned entities and their transforms, so you can review and iterate on the world you are building.",
            json!({"type":"object","properties":{}}),
        ),
    ]
}

/// Map a tool name + arguments JSON into an [`AgentCommand`]. Returns `None`
/// for an unknown tool name or malformed arguments.
fn parse_tool_call(name: &str, args: &str) -> Option<AgentCommand> {
    let args: Value = serde_json::from_str(args).ok()?;
    match name {
        "spawn_primitive" => Some(AgentCommand::SpawnPrimitive(SpawnPrimitiveCmd {
            name: args["name"].as_str()?.into(),
            shape: serde_json::from_value(args["shape"].clone()).ok()?,
            dimensions: args
                .get("dimensions")
                .and_then(|v| v.as_object())
                .map(|o| {
                    o.iter()
                        .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f as f32)))
                        .collect()
                })
                .unwrap_or_default(),
            position: parse_arr3(&args["position"]),
            rotation_degrees: parse_arr3(&args["rotation_degrees"]),
            scale: parse_arr3_scale(&args["scale"]),
            color: parse_arr4(&args["color"]),
            metallic: args["metallic"].as_f64().unwrap_or(0.0) as f32,
            roughness: args["roughness"].as_f64().unwrap_or(0.5) as f32,
            emissive: parse_arr4(&args["emissive"]),
        })),
        "modify_entity" => Some(AgentCommand::ModifyEntity(ModifyEntityCmd {
            name: args["name"].as_str()?.into(),
            position: args.get("position").and_then(parse_opt_arr3),
            rotation_degrees: args.get("rotation_degrees").and_then(parse_opt_arr3),
            scale: args.get("scale").and_then(parse_opt_arr3),
            color: args.get("color").and_then(parse_opt_arr4),
            metallic: args["metallic"].as_f64().map(|f| f as f32),
            roughness: args["roughness"].as_f64().map(|f| f as f32),
            emissive: args.get("emissive").and_then(parse_opt_arr4),
        })),
        "delete_entity" => Some(AgentCommand::DeleteEntity {
            name: args["name"].as_str()?.into(),
        }),
        "set_light" => Some(AgentCommand::SetLight(SetLightCmd {
            name: args["name"].as_str()?.into(),
            color: parse_arr4(&args["color"]),
            intensity: args["intensity"].as_f64().unwrap_or(1000.0) as f32,
            position: args.get("position").and_then(parse_opt_arr3),
            direction: args.get("direction").and_then(parse_opt_arr3),
        })),
        "set_environment" => Some(AgentCommand::SetEnvironment(EnvironmentCmd {
            background_color: parse_arr4(&args["background_color"]),
            ambient_light: parse_arr4(&args["ambient_light"]),
        })),
        "scene_info" => Some(AgentCommand::SceneInfo),
        _ => None,
    }
}

fn parse_arr3(v: &Value) -> [f32; 3] {
    let a = v.as_array();
    [
        a.and_then(|a| a.first())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        a.and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        a.and_then(|a| a.get(2))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
    ]
}
fn parse_arr3_scale(v: &Value) -> [f32; 3] {
    let a = v.as_array();
    [
        a.and_then(|a| a.first())
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32,
        a.and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32,
        a.and_then(|a| a.get(2))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32,
    ]
}
fn parse_arr4(v: &Value) -> [f32; 4] {
    let a = v.as_array();
    [
        a.and_then(|a| a.first())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        a.and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        a.and_then(|a| a.get(2))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        a.and_then(|a| a.get(3))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32,
    ]
}
fn parse_opt_arr3(v: &Value) -> Option<[f32; 3]> {
    v.as_array().filter(|a| a.len() == 3).map(|_| parse_arr3(v))
}
fn parse_opt_arr4(v: &Value) -> Option<[f32; 4]> {
    v.as_array().filter(|a| a.len() == 4).map(|_| parse_arr4(v))
}

// ---------------------------------------------------------------------------
// Bevy-side execution — name registry + frame drain
// ---------------------------------------------------------------------------

use bevy::prelude::{
    Assets, Commands, Component, Entity, Local, Mesh, Query, Res, ResMut, StandardMaterial,
};

/// Marker for every agent-spawned entity, carrying its stable name.
#[derive(Component)]
pub struct AgentEntity {
    pub name: String,
}

/// Name → Entity registry. Lets modify/delete reference entities by the names
/// the LLM chose. Held as a Resource alongside the channels.
#[derive(bevy::prelude::Resource, Default)]
pub struct NameRegistry {
    pub map: HashMap<String, Entity>,
}

impl NameRegistry {
    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }
}

/// The Bevy-side resource: the channels to drain + the name registry. Created
/// once at startup and held for the app's lifetime.
#[derive(bevy::prelude::Resource)]
pub struct AgentExecutor {
    pub channels: AgentChannels,
    pub registry: NameRegistry,
}

impl AgentExecutor {
    /// Drain pending commands and execute them against the world. Called once
    /// per frame. Non-blocking (`try_recv`) so an idle agent costs nothing.
    pub fn drain(
        &mut self,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
        agent_entities: &Query<&AgentEntity>,
    ) {
        // Reconcile the registry against despawned entities (e.g. a mood change
        // may have cleared agent entities). Cheap: only walks the registry.
        self.registry
            .map
            .retain(|_, e| agent_entities.get(*e).is_ok());

        while let Ok(cmd) = self.channels.cmd_rx.try_recv() {
            let resp = self.execute(cmd, commands, meshes, materials);
            let _ = self.channels.resp_tx.send(resp);
        }
    }

    fn execute(
        &mut self,
        cmd: AgentCommand,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> AgentResponse {
        use bevy::prelude::*;
        match cmd {
            AgentCommand::SpawnPrimitive(c) => {
                if self.registry.contains(&c.name) {
                    return AgentResponse::Error(format!("'{}' already exists", c.name));
                }
                let mesh = build_primitive_mesh(c.shape, &c.dimensions, meshes);
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgba(c.color[0], c.color[1], c.color[2], c.color[3]),
                    metallic: c.metallic,
                    perceptual_roughness: c.roughness,
                    emissive: LinearRgba::new(
                        c.emissive[0],
                        c.emissive[1],
                        c.emissive[2],
                        c.emissive[3],
                    ),
                    ..default()
                });
                let entity = commands
                    .spawn((
                        AgentEntity {
                            name: c.name.clone(),
                        },
                        Name::new(c.name.clone()),
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform {
                            translation: Vec3::from(c.position),
                            rotation: Quat::from_euler(
                                EulerRot::YXZ,
                                c.rotation_degrees[1].to_radians(),
                                c.rotation_degrees[0].to_radians(),
                                c.rotation_degrees[2].to_radians(),
                            ),
                            scale: Vec3::from(c.scale),
                        },
                    ))
                    .id();
                self.registry.map.insert(c.name.clone(), entity);
                AgentResponse::Spawned { name: c.name }
            }
            AgentCommand::ModifyEntity(c) => {
                let Some(&entity) = self.registry.map.get(&c.name) else {
                    return AgentResponse::Error(format!("'{}' not found", c.name));
                };
                // Partial transform patch: keep existing fields where the
                // command doesn't supply one. Spawn a fresh Transform only when
                // both are present; otherwise fall back to a deferred queue
                // that mutates the live component.
                let mut ecmd = commands.entity(entity);
                if let (Some(pos), Some(scale)) = (c.position, c.scale) {
                    ecmd.insert(Transform {
                        translation: Vec3::from(pos),
                        scale: Vec3::from(scale),
                        ..default()
                    });
                } else {
                    ecmd.queue(move |mut entity: EntityWorldMut| {
                        if let Some(mut tf) = entity.get_mut::<Transform>() {
                            if let Some(pos) = c.position {
                                tf.translation = Vec3::from(pos);
                            }
                            if let Some(scale) = c.scale {
                                tf.scale = Vec3::from(scale);
                            }
                        }
                    });
                }
                AgentResponse::Modified { name: c.name }
            }
            AgentCommand::DeleteEntity { name } => {
                if let Some(entity) = self.registry.map.remove(&name) {
                    commands.entity(entity).despawn();
                    AgentResponse::Deleted { name }
                } else {
                    AgentResponse::Error(format!("'{}' not found", name))
                }
            }
            AgentCommand::SetLight(c) => {
                // A directional light if direction is set; else a point light.
                if let Some(dir) = c.direction {
                    let dir_v = Vec3::from(dir).normalize_or_zero();
                    commands.spawn((
                        DirectionalLight {
                            color: Color::srgba(c.color[0], c.color[1], c.color[2], c.color[3]),
                            illuminance: c.intensity,
                            ..default()
                        },
                        Transform::from_xyz(0.0, 10.0, 0.0).looking_to(dir_v, Vec3::Y),
                    ));
                } else {
                    commands.spawn((
                        PointLight {
                            color: Color::srgba(c.color[0], c.color[1], c.color[2], c.color[3]),
                            intensity: c.intensity,
                            ..default()
                        },
                        Transform::from_translation(Vec3::from(
                            c.position.unwrap_or([0.0, 5.0, 0.0]),
                        )),
                    ));
                }
                AgentResponse::LightSet { name: c.name }
            }
            AgentCommand::SetEnvironment(c) => {
                // Background via a ClearColor resource; ambient via AmbientLight
                // on the camera. We set a global ambient entity (idempotent-ish:
                // one per call; acceptable for a simplified agent).
                commands.spawn((bevy::prelude::AmbientLight {
                    color: Color::srgba(
                        c.ambient_light[0],
                        c.ambient_light[1],
                        c.ambient_light[2],
                        c.ambient_light[3],
                    ),
                    brightness: 0.5,
                    ..default()
                },));
                AgentResponse::EnvironmentSet
            }
            AgentCommand::SceneInfo => {
                let mut summary = String::from("Scene:\n");
                for name in self.registry.map.keys() {
                    summary.push_str(&format!("  - {name}\n"));
                }
                if self.registry.map.is_empty() {
                    summary.push_str("  (empty)");
                }
                AgentResponse::SceneInfo(summary)
            }
        }
    }

    /// Despawn every agent-spawned entity and clear the name registry. Used
    /// before replaying a cached [`SceneBuild`] so the scene is rebuilt clean
    /// (spawn rejects duplicate names, so a stale scene would block replay).
    pub fn clear_scene(&mut self, commands: &mut Commands, agent_entities: &Query<&AgentEntity>) {
        for &e in self.registry.map.values() {
            if agent_entities.get(e).is_ok() {
                commands.entity(e).despawn();
            }
        }
        self.registry.map.clear();
    }

    /// Replay a cached [`SceneBuild`] — iterate its commands through `execute`
    /// without the LLM, bridge, or async runtime. Deterministic: the same build
    /// → the same world. Call [`clear_scene`] first. Returns the count applied.
    pub fn replay(
        &mut self,
        build: &SceneBuild,
        commands: &mut Commands,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> usize {
        let mut n = 0;
        for cmd in &build.commands {
            // SceneInfo has no effect on replay (it only reports state).
            if matches!(cmd, AgentCommand::SceneInfo) {
                continue;
            }
            self.execute(cmd.clone(), commands, meshes, materials);
            n += 1;
        }
        n
    }
}

/// Build the Bevy mesh for a primitive from its shape + dimensions.
fn build_primitive_mesh(
    shape: PrimitiveShape,
    dims: &HashMap<String, f32>,
    meshes: &mut ResMut<Assets<Mesh>>,
) -> bevy::asset::Handle<Mesh> {
    use bevy::prelude::*;
    match shape {
        PrimitiveShape::Cuboid => {
            let x = dims.get("x").copied().unwrap_or(1.0);
            let y = dims.get("y").copied().unwrap_or(1.0);
            let z = dims.get("z").copied().unwrap_or(1.0);
            meshes.add(Cuboid::new(x, y, z))
        }
        PrimitiveShape::Sphere => {
            let r = dims.get("radius").copied().unwrap_or(0.5);
            meshes.add(Sphere::new(r).mesh().uv(32, 18))
        }
        PrimitiveShape::Cylinder => {
            let r = dims.get("radius").copied().unwrap_or(0.5);
            let h = dims.get("height").copied().unwrap_or(1.0);
            meshes.add(Cylinder::new(r, h))
        }
        PrimitiveShape::Cone => {
            let r = dims.get("radius").copied().unwrap_or(0.5);
            let h = dims.get("height").copied().unwrap_or(1.0);
            meshes.add(Cone {
                radius: r,
                height: h,
            })
        }
        PrimitiveShape::Torus => {
            let major = dims.get("major_radius").copied().unwrap_or(1.0);
            let minor = dims.get("minor_radius").copied().unwrap_or(0.25);
            meshes.add(Torus::new(minor, major))
        }
        PrimitiveShape::Plane => {
            let x = dims.get("x").copied().unwrap_or(10.0);
            let z = dims.get("z").copied().unwrap_or(10.0);
            meshes.add(Plane3d::new(Vec3::Y, Vec2::new(x / 2.0, z / 2.0)))
        }
    }
}

/// The frame drain system. Registered in main.rs; runs every frame, costs
/// nothing when the agent is idle.
pub fn drain_agent_commands(
    mut executor: ResMut<AgentExecutor>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    agent_entities: Query<&AgentEntity>,
) {
    executor.drain(&mut commands, &mut meshes, &mut materials, &agent_entities);
}

/// Replay a cached `SceneBuild` when the current track changes and has a build
/// stored in its analysis sidecar (PLAN.md M7 — no LLM re-run). Clears the
/// agent scene first, then iterates the cached commands. No-op when the current
/// track has no cached build or hasn't changed since the last replay.
#[allow(clippy::too_many_arguments)]
pub fn replay_cached_build(
    mut executor: ResMut<AgentExecutor>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    analysis: Res<crate::analysis::AnalysisStore>,
    playback: Res<crate::playback::Playback>,
    agent_entities: Query<&AgentEntity>,
    mut last: Local<Option<Option<String>>>,
) {
    let current_id = playback
        .queue
        .get(playback.current % playback.queue.len().max(1))
        .and_then(|t| t.id.clone());
    // Re-replay only when the current track id changes.
    if *last == Some(current_id.clone()) {
        return;
    }
    *last = Some(current_id.clone());

    let Some(id) = current_id.as_deref() else {
        return;
    };
    let Some(build) = analysis.get(id).and_then(|a| a.build.as_ref()) else {
        // No cached build for this track — nothing to replay. Any previously
        // replayed entities stay until the next track with a build clears them.
        return;
    };
    executor.clear_scene(&mut commands, &agent_entities);
    let n = executor.replay(build, &mut commands, &mut meshes, &mut materials);
    info!("Replayed {n} cached agent commands for track {id}");
}

// ---------------------------------------------------------------------------
// Agent session — runs Bonsai in a tool-calling loop until it stops calling
// ---------------------------------------------------------------------------

/// Run one agent session: Bonsai gets the track's analysis as context, then
/// loops calling tools to build a world, until it emits a message with no
/// tool_calls (it's done) or the step budget is exhausted. Each tool_call is
/// parsed into an [`AgentCommand`], sent to Bevy via the bridge, and the result
/// is fed back. Returns the ordered commands issued (the [`SceneBuild`]).
///
/// Blocks the calling thread on a dedicated tokio runtime (the analysis worker
/// is a plain std::thread — no async pollution of the rest of the app).
pub fn run_session(
    model: &mut mistralrs::Model,
    bridge: Arc<AgentBridge>,
    analysis: &TrackAnalysis,
) -> Option<SceneBuild> {
    use mistralrs::{RequestBuilder, TextMessageRole, ToolChoice};

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| warn!("agent: tokio runtime failed: {e}"))
        .ok()?;

    rt.block_on(async move {
        let tools = tool_schemas();
        let mut build = SceneBuild::default();

        // Seed the conversation with the world-design brief.
        let mut messages = RequestBuilder::new()
            .add_message(TextMessageRole::User, build_system_prompt(analysis))
            .set_tools(tools)
            .set_tool_choice(ToolChoice::Auto);

        for step in 0..MAX_AGENT_STEPS {
            let response = match model.send_chat_request(messages.clone()).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("agent: chat request failed at step {step}: {e}");
                    break;
                }
            };
            let Some(message) = response.choices.first().map(|c| &c.message) else {
                break;
            };
            let Some(tool_calls) = &message.tool_calls else {
                // No tool calls → the agent is done (it emitted a description).
                break;
            };

            // Execute every tool_call in this turn, recording each command.
            // Then feed the assistant turn + tool results back into the convo.
            messages = messages.add_message_with_tool_call(
                TextMessageRole::Assistant,
                message.content.as_deref().unwrap_or("").to_string(),
                tool_calls.clone(),
            );

            for call in tool_calls {
                let name = &call.function.name;
                let args = &call.function.arguments;
                match parse_tool_call(name, args) {
                    Some(cmd) => {
                        build.commands.push(cmd.clone());
                        let resp = bridge.send(cmd).await;
                        messages = messages.add_tool_message(resp.to_message(), call.id.clone());
                    }
                    None => {
                        messages = messages.add_tool_message(
                            format!("error: unknown tool '{name}'"),
                            call.id.clone(),
                        );
                    }
                }
            }
        }

        if build.is_empty() {
            warn!("agent: session produced no commands — keeping rule-derived world");
            None
        } else {
            info!("agent: session produced {} commands", build.commands.len());
            Some(build)
        }
    })
}

/// Cap on agent turns per session — bounds LLM cost on a single track.
const MAX_AGENT_STEPS: usize = 12;

/// The system prompt: tells Bonsai what it is, gives it the track's mood/BPM/
/// energy as context, and instructs it to build a world with the tools.
fn build_system_prompt(analysis: &TrackAnalysis) -> String {
    let mood = MOODS
        .get(analysis.mood)
        .map(|m| m.world_name)
        .unwrap_or("UNKNOWN");
    let bpm = if analysis.bpm > 0.0 {
        format!("{:.0}", analysis.bpm)
    } else {
        "unknown".into()
    };
    format!(
        "You are a 3D world designer for a music visualizer. Build an immersive world that \
matches this song by calling the spawn_primitive, set_light, and set_environment tools. \
Call scene_info to review your work and iterate.\n\n\
Song context: mood = {mood}, tempo = {bpm} BPM, {n} sections.\n\
Keep it tasteful and performant: 8-25 primitives is plenty. Place a ground plane, \
a few hero structures, and accent lighting that suits the mood. When you are done, \
reply with a short description instead of calling more tools.",
        n = analysis.sections.len().max(1)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_schemas_are_six_complete_functions() {
        let schemas = tool_schemas();
        assert_eq!(schemas.len(), 6, "core simplified toolset");
        for s in &schemas {
            assert!(!s.function.name.is_empty());
            assert!(s.function.description.is_some());
            assert!(s.function.parameters.is_some());
        }
    }

    #[test]
    fn parse_tool_call_round_trips_spawn() {
        let args = r#"{"name":"tower","shape":"Cuboid","dimensions":{"x":2,"y":8,"z":2},"position":[0,4,0]}"#;
        let cmd = parse_tool_call("spawn_primitive", args).expect("parses");
        match cmd {
            AgentCommand::SpawnPrimitive(s) => {
                assert_eq!(s.name, "tower");
                assert_eq!(s.shape, PrimitiveShape::Cuboid);
                assert_eq!(s.position, [0.0, 4.0, 0.0]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_tool_call_rejects_unknown() {
        assert!(parse_tool_call("nope", "{}").is_none());
    }

    #[test]
    fn response_messages_are_readable() {
        assert_eq!(
            AgentResponse::Spawned { name: "x".into() }.to_message(),
            "spawned 'x'"
        );
        assert!(
            AgentResponse::Error("bad".into())
                .to_message()
                .contains("bad")
        );
    }
}
