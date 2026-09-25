//! Export the current track's world as a `localgpt-world-types` manifest.
//!
//! A Verse world is a performance: the mood palette, the analysis sidecar
//! and the agent's scene build together describe a world that plays to a
//! song. This module writes that description in LocalGPT's shared world
//! format, so the same world renders in Gen, MD and the web viewer
//! (localgpt.world) — and performs there too, because the manifest carries a
//! `SoundtrackDef` (the analysis curves, never the audio file or the CLAP
//! embedding) and `ModulationDef`s that bind entities to it: emissive
//! entities pulse with the beat, hero placements breathe with the energy
//! curve, agent lights follow the drums stem.
//!
//! What is exported: the environment from the mood palette, a ground plane,
//! a sun, and the agent-authored scene (`SceneBuild`: primitives, placed CC0
//! assets, scatter fields, lights, the environment override), replayed with
//! the same deterministic scatter as the live executor.
//!
//! Not exported yet: the rule-based props and ground cover
//! (`world_assets::populate_world_props` spawns them rather than describing
//! them), section scoping (`at_role`), particles, and the waveform skyline.
//! Agent rotations are recorded as given; the executor composes them YXZ,
//! the format XYZ, which only differs for compound rotations.
//!
//! `VERSE_EXPORT_WORLD=<dir>` writes `<dir>/<track id>.world.json` (the web
//! viewer's format) and `<dir>/<track id>.world.ron` (Gen's save format)
//! once per track as it becomes current. Personal libraries export with no
//! audio path: the world performs silently from its curves anywhere.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use localgpt_world_types as wt;

use crate::agent_types::{AgentCommand, SceneBuild};
use crate::analysis::{AnalysisStore, TrackAnalysis};
use crate::playback::{Playback, Track};
use crate::theme::{Theme, WorldMood};
use crate::world_assets::{AssetManifest, Tier, WorldAssets};

/// Ground plane size, world units.
const GROUND_SIZE: f32 = 400.0;
/// Ambient brightness (Bevy `GlobalAmbientLight` units) for exported worlds.
const AMBIENT_BRIGHTNESS: f32 = 260.0;
/// Sun illuminance in lux.
const SUN_LUX: f32 = 6000.0;

/// Audio the world may ship (the CC0 starter pack, the creator's own music).
#[derive(Debug, Clone)]
pub struct AudioRef {
    /// Path relative to the world's `assets/` directory.
    pub path: String,
    /// License of the audio, e.g. `CC0-1.0`.
    pub license: String,
}

/// Everything the export reads.
pub struct ExportInput<'a> {
    pub track: &'a Track,
    pub analysis: &'a TrackAnalysis,
    pub mood: &'a WorldMood,
    /// The asset pack, for placement scales and tiers of placed assets.
    pub assets: Option<&'a AssetManifest>,
    /// Audio to reference; `None` for a personal library.
    pub audio: Option<AudioRef>,
}

fn srgba(color: Color) -> [f32; 4] {
    let c = color.to_srgba();
    [c.red, c.green, c.blue, c.alpha]
}

/// Build the manifest for a track's world.
pub fn world_manifest(input: &ExportInput<'_>) -> wt::WorldManifest {
    let ExportInput {
        track,
        analysis,
        mood,
        assets,
        audio,
    } = input;
    let recipe = analysis.recipe.as_ref();
    let build = analysis.build.as_ref();

    let name = recipe
        .map(|r| r.world_name.trim())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} — {}", mood.world_name, track.title));
    let mut manifest = wt::WorldManifest::new(name);
    manifest.meta.description = Some(build.and_then(|b| b.description.clone()).unwrap_or_else(
        || {
            format!(
                "A LocalGPT Verse world for \"{}\" by {}",
                track.title, track.artist
            )
        },
    ));
    manifest.meta.tags = Some(vec!["verse".to_string(), mood.id.to_string()]);
    manifest.meta.source = Some("verse".to_string());
    manifest.meta.bevy_version = Some("0.19".to_string());

    manifest.environment = Some(wt::EnvironmentDef {
        background_color: Some(srgba(mood.sky_bottom)),
        ambient_intensity: Some(AMBIENT_BRIGHTNESS),
        ambient_color: Some(srgba(mood.ambient)),
        fog_density: Some(0.012),
        fog_color: Some(srgba(mood.fog)),
    });
    manifest.camera = Some(wt::CameraDef {
        position: [0.0, 6.0, 22.0],
        look_at: [0.0, 2.0, 0.0],
        fov_degrees: 50.0,
    });
    manifest.avatar = Some(wt::AvatarDef {
        spawn_position: [0.0, 1.7, 22.0],
        spawn_look_at: [0.0, 2.0, 0.0],
        pov: wt::PointOfView::FirstPerson,
        movement_speed: 6.0,
        height: 1.7,
        model_entity: None,
    });

    let mut next_id = 1u64;
    let mut alloc = || {
        let id = next_id;
        next_id += 1;
        id
    };

    let mut ground = wt::WorldEntity::new(alloc(), "ground");
    ground.shape = Some(wt::Shape::Plane {
        x: GROUND_SIZE,
        z: GROUND_SIZE,
    });
    ground.material = Some(wt::MaterialDef {
        color: srgba(mood.ground),
        roughness: 0.95,
        ..Default::default()
    });
    manifest.entities.push(ground);

    let mut sun = wt::WorldEntity::new(alloc(), "sun");
    sun.transform.position = [0.0, 40.0, 0.0];
    sun.light = Some(wt::LightDef {
        light_type: wt::LightType::Directional,
        color: srgba(mood.accent.with_alpha(1.0)),
        intensity: SUN_LUX,
        direction: Some([-0.35, -1.0, -0.45]),
        shadows: true,
        ..Default::default()
    });
    manifest.entities.push(sun);

    if let Some(build) = build {
        let track_id = track.id.as_deref().unwrap_or("");
        replay_build(build, track_id, *assets, &mut manifest, &mut alloc);
    }

    manifest.soundtrack = Some(soundtrack(track, analysis, audio.clone()));
    manifest.next_entity_id = next_id;
    manifest
}

/// The agent's scene build as manifest entities, with the modulations that
/// make them perform.
fn replay_build(
    build: &SceneBuild,
    track_id: &str,
    assets: Option<&AssetManifest>,
    manifest: &mut wt::WorldManifest,
    alloc: &mut impl FnMut() -> u64,
) {
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut env_override: Option<wt::EnvironmentDef> = None;

    fn push(
        by_name: &mut HashMap<String, usize>,
        manifest: &mut wt::WorldManifest,
        entity: wt::WorldEntity,
    ) {
        by_name.insert(entity.name.0.clone(), manifest.entities.len());
        manifest.entities.push(entity);
    }

    for cmd in &build.commands {
        match cmd {
            AgentCommand::BeginSession { .. } | AgentCommand::SceneInfo => {}
            AgentCommand::SpawnPrimitive(c) => {
                if by_name.contains_key(&c.name) {
                    continue;
                }
                let mut e = wt::WorldEntity::new(alloc(), c.name.clone());
                e.transform.position = c.position;
                e.transform.rotation_degrees = c.rotation_degrees;
                e.transform.scale = c.scale;
                e.shape = Some(primitive_shape(c.shape, &c.dimensions));
                e.material = Some(wt::MaterialDef {
                    color: c.color,
                    metallic: c.metallic,
                    roughness: c.roughness,
                    emissive: c.emissive,
                    ..Default::default()
                });
                add_emissive_pulse(&mut e);
                push(&mut by_name, manifest, e);
            }
            AgentCommand::PlaceAsset(c) => {
                if by_name.contains_key(&c.name) || c.asset.is_empty() {
                    continue;
                }
                let entry = assets.and_then(|m| m.assets.iter().find(|a| a.file == c.asset));
                let mut e = wt::WorldEntity::new(alloc(), c.name.clone());
                e.transform.position = c.position;
                e.transform.rotation_degrees = c.rotation_degrees;
                let scale = entry.map_or(1.0, |a| a.placement_scale()) * c.scale.max(0.05);
                e.transform.scale = [scale; 3];
                e.mesh_asset = Some(wt::MeshAssetRef {
                    path: format!("models/{}", c.asset),
                    node: None,
                });
                if entry.is_some_and(|a| a.tier == Tier::Hero) {
                    e.modulations.push(
                        wt::ModulationDef::new(
                            wt::ModulationTarget::Scale,
                            wt::SignalSource::Energy,
                            [0.95, 1.08],
                        )
                        .with_smoothing(0.4),
                    );
                }
                push(&mut by_name, manifest, e);
            }
            AgentCommand::ScatterField(c) => {
                if by_name.contains_key(&c.name) || c.assets.is_empty() {
                    continue;
                }
                let entries: Vec<_> = c
                    .assets
                    .iter()
                    .map(|file| {
                        (
                            file.clone(),
                            assets
                                .and_then(|m| m.assets.iter().find(|a| &a.file == file))
                                .map_or(1.0, |a| a.placement_scale()),
                        )
                    })
                    .collect();
                // The executor's deterministic field (ARCHITECTURE R6).
                let seed = crate::world_assets::fold_seed(&format!("{}|{}", track_id, c.name));
                let offsets =
                    crate::world_assets::scatter_offsets(seed, c.count as usize, c.radius);
                let mut rng = seed ^ 0xA5A5_5EED_u64;
                let base = Vec3::from(c.position);
                for (i, off) in offsets.into_iter().enumerate() {
                    let (file, placement) = &entries[i % entries.len()];
                    let yaw = crate::world_assets::rand01(&mut rng) * std::f32::consts::TAU;
                    let scale = placement
                        * c.scale.max(0.05)
                        * (0.7 + crate::world_assets::rand01(&mut rng) * 0.7);
                    let mut e = wt::WorldEntity::new(alloc(), format!("{}_{}", c.name, i + 1));
                    e.transform.position = (base + off).to_array();
                    e.transform.rotation_degrees = [0.0, yaw.to_degrees(), 0.0];
                    e.transform.scale = [scale; 3];
                    e.mesh_asset = Some(wt::MeshAssetRef {
                        path: format!("models/{file}"),
                        node: None,
                    });
                    push(&mut by_name, manifest, e);
                }
                by_name.insert(c.name.clone(), usize::MAX);
            }
            AgentCommand::ModifyEntity(c) => {
                let Some(&idx) = by_name.get(&c.name) else {
                    continue;
                };
                let Some(e) = manifest.entities.get_mut(idx) else {
                    continue;
                };
                if let Some(p) = c.position {
                    e.transform.position = p;
                }
                if let Some(r) = c.rotation_degrees {
                    e.transform.rotation_degrees = r;
                }
                if let Some(s) = c.scale {
                    e.transform.scale = s;
                }
                if c.color.is_some()
                    || c.metallic.is_some()
                    || c.roughness.is_some()
                    || c.emissive.is_some()
                {
                    let mut m = e.material.take().unwrap_or_default();
                    if let Some(color) = c.color {
                        m.color = color;
                    }
                    if let Some(metallic) = c.metallic {
                        m.metallic = metallic;
                    }
                    if let Some(roughness) = c.roughness {
                        m.roughness = roughness;
                    }
                    if let Some(emissive) = c.emissive {
                        m.emissive = emissive;
                    }
                    e.material = Some(m);
                    e.modulations
                        .retain(|m| m.target != wt::ModulationTarget::Emissive);
                    add_emissive_pulse(e);
                }
            }
            AgentCommand::DeleteEntity { name } => {
                if let Some(idx) = by_name.remove(name)
                    && idx != usize::MAX
                {
                    manifest.entities.remove(idx);
                    for v in by_name.values_mut() {
                        if *v != usize::MAX && *v > idx {
                            *v -= 1;
                        }
                    }
                }
            }
            AgentCommand::SetLight(c) => {
                if by_name.contains_key(&c.name) {
                    continue;
                }
                let mut e = wt::WorldEntity::new(alloc(), c.name.clone());
                e.transform.position = c.position.unwrap_or([0.0, 6.0, 0.0]);
                let directional = c.direction.is_some();
                e.light = Some(wt::LightDef {
                    light_type: if directional {
                        wt::LightType::Directional
                    } else {
                        wt::LightType::Point
                    },
                    color: c.color,
                    intensity: c.intensity,
                    direction: c.direction,
                    shadows: directional,
                    range: (!directional).then_some(40.0),
                    ..Default::default()
                });
                e.modulations.push(
                    wt::ModulationDef::new(
                        wt::ModulationTarget::LightIntensity,
                        wt::SignalSource::Stem(wt::StemKind::Drums),
                        [0.6, 1.4],
                    )
                    .with_smoothing(0.1),
                );
                push(&mut by_name, manifest, e);
            }
            AgentCommand::SetEnvironment(c) => {
                env_override = Some(wt::EnvironmentDef {
                    background_color: Some(c.background_color),
                    ambient_intensity: Some(AMBIENT_BRIGHTNESS),
                    ambient_color: Some(c.ambient_light),
                    fog_density: manifest.environment.as_ref().and_then(|e| e.fog_density),
                    fog_color: Some(c.background_color),
                });
            }
        }
    }
    if let Some(env) = env_override {
        manifest.environment = Some(env);
    }
}

/// Emissive entities pulse with the beat, the way `pulse_beacons` drives
/// the live world.
fn add_emissive_pulse(entity: &mut wt::WorldEntity) {
    let glows = entity
        .material
        .as_ref()
        .is_some_and(|m| m.emissive[0] > 0.0 || m.emissive[1] > 0.0 || m.emissive[2] > 0.0);
    if glows {
        entity.modulations.push(
            wt::ModulationDef::new(
                wt::ModulationTarget::Emissive,
                wt::SignalSource::Beat,
                [0.6, 1.6],
            )
            .with_smoothing(0.05),
        );
    }
}

/// The agent's primitive vocabulary as a format shape, with the executor's
/// default dimensions.
fn primitive_shape(
    shape: crate::agent_types::PrimitiveShape,
    dims: &HashMap<String, f32>,
) -> wt::Shape {
    use crate::agent_types::PrimitiveShape as P;
    let d = |key: &str, default: f32| dims.get(key).copied().unwrap_or(default);
    match shape {
        P::Cuboid => wt::Shape::Cuboid {
            x: d("x", 1.0),
            y: d("y", 1.0),
            z: d("z", 1.0),
        },
        P::Sphere => wt::Shape::Sphere {
            radius: d("radius", 0.5),
        },
        P::Cylinder => wt::Shape::Cylinder {
            radius: d("radius", 0.5),
            height: d("height", 1.0),
        },
        P::Cone => wt::Shape::Cone {
            radius: d("radius", 0.5),
            height: d("height", 1.0),
        },
        P::Torus => wt::Shape::Torus {
            major_radius: d("major_radius", 1.0),
            minor_radius: d("minor_radius", 0.25),
        },
        P::Plane => wt::Shape::Plane {
            x: d("x", 10.0),
            z: d("z", 10.0),
        },
    }
}

/// The track's analysis as a soundtrack definition. Never the embedding.
fn soundtrack(
    track: &Track,
    analysis: &TrackAnalysis,
    audio: Option<AudioRef>,
) -> wt::SoundtrackDef {
    let unit = |v: &[f32]| v.iter().map(|x| x.clamp(0.0, 1.0)).collect::<Vec<_>>();
    // `StemEnergy` is `[drums, bass, vocals, other]` (demucs::STEM_NAMES).
    let stems = analysis.stems.as_ref().map(|s| wt::StemCurves {
        drums: unit(&s[0]),
        bass: unit(&s[1]),
        vocals: unit(&s[2]),
        other: unit(&s[3]),
    });
    wt::SoundtrackDef {
        path: audio.as_ref().map(|a| a.path.clone()),
        license: audio.map(|a| a.license),
        title: Some(track.title.clone()),
        artist: Some(track.artist.clone()),
        duration: analysis.duration.max(track.duration).max(0.0),
        bpm: analysis.bpm.max(0.0),
        beat_offset: analysis.beat_offset.max(0.0),
        sections: unit(&analysis.sections),
        energy: unit(&analysis.energy),
        stems,
    }
}

// ---------------------------------------------------------------------------
// `VERSE_EXPORT_WORLD=<dir>`: write the current track's world once
// ---------------------------------------------------------------------------

/// Where exports go (`VERSE_EXPORT_WORLD`).
#[derive(Resource)]
pub struct ExportDir(pub PathBuf);

/// Writes each track's world the first time it is current with an analysis.
pub struct WorldExportPlugin;

impl Plugin for WorldExportPlugin {
    fn build(&self, app: &mut App) {
        if let Some(dir) = std::env::var_os("VERSE_EXPORT_WORLD") {
            app.insert_resource(ExportDir(PathBuf::from(dir)))
                .add_systems(Update, export_current_track);
        }
    }
}

fn export_current_track(
    dir: Res<ExportDir>,
    playback: Res<Playback>,
    analysis: Option<Res<AnalysisStore>>,
    theme: Res<Theme>,
    assets: Option<Res<WorldAssets>>,
    mut done: Local<HashSet<String>>,
) {
    let Some(analysis) = analysis else {
        return;
    };
    let Some(track) = playback.queue.get(playback.current) else {
        return;
    };
    let Some(id) = track.id.as_deref() else {
        return;
    };
    if done.contains(id) {
        return;
    }
    let Some(track_analysis) = analysis.get(id) else {
        return;
    };
    let manifest = world_manifest(&ExportInput {
        track,
        analysis: track_analysis,
        mood: theme.current(),
        assets: assets.as_deref().and_then(|a| a.manifest.as_ref()),
        audio: None,
    });
    done.insert(id.to_string());
    if let Err(e) = write_manifest(&dir.0, id, &manifest) {
        warn!("World export failed for {}: {e}", track.title);
    } else {
        info!(
            "Exported world for {} — {} entities → {}",
            track.title,
            manifest.entities.len(),
            dir.0.join(format!("{id}.world.json")).display()
        );
    }
}

/// Write `<dir>/<id>.world.json` and `<dir>/<id>.world.ron`.
pub fn write_manifest(
    dir: &std::path::Path,
    id: &str,
    manifest: &wt::WorldManifest,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(manifest).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(format!("{id}.world.json")), json)?;
    let ron = ron::ser::to_string_pretty(manifest, ron::ser::PrettyConfig::default())
        .map_err(std::io::Error::other)?;
    std::fs::write(dir.join(format!("{id}.world.ron")), ron)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_types::{PlaceAssetCmd, ScatterFieldCmd, SetLightCmd, SpawnPrimitiveCmd};
    use crate::world_assets::AssetEntry;

    fn track() -> Track {
        Track {
            title: "Amber Drift".into(),
            artist: "LocalGPT".into(),
            album: None,
            duration: 30.0,
            mood: 0,
            section: String::new(),
            path: None,
            id: Some("deadbeef".into()),
        }
    }

    fn analysis(build: Option<SceneBuild>) -> TrackAnalysis {
        TrackAnalysis {
            version: 2,
            duration: 30.0,
            bpm: 120.0,
            beat_offset: 0.2,
            sections: vec![0.0, 0.5],
            energy: (0..31).map(|i| (i % 10) as f32 / 10.0).collect(),
            centroid_hz: None,
            mood: 0,
            mood_id: None,
            loudness_lufs: None,
            pinned_mood: None,
            pinned_mood_id: None,
            pinned_seed: None,
            embedding: Some(vec![0.5; 512]),
            stems: None,
            recipe: None,
            build,
        }
    }

    fn pack() -> AssetManifest {
        AssetManifest {
            version: 2,
            assets: vec![AssetEntry {
                id: "rock_a".into(),
                name: "Rock A".into(),
                file: "rock_a.glb".into(),
                kind: "rock".into(),
                tier: Tier::Hero,
                mood: 0,
                mood_id: None,
                scale: 1.0,
                dims: Some([2.0, 1.0, 2.0]),
                license: "CC0".into(),
                author: "Poly Haven".into(),
                source: String::new(),
            }],
        }
    }

    fn build() -> SceneBuild {
        SceneBuild {
            commands: vec![
                AgentCommand::SpawnPrimitive(SpawnPrimitiveCmd {
                    name: "beacon".into(),
                    shape: crate::agent_types::PrimitiveShape::Sphere,
                    dimensions: HashMap::from([("radius".to_string(), 0.8)]),
                    position: [0.0, 3.0, 0.0],
                    rotation_degrees: [0.0; 3],
                    scale: [1.0; 3],
                    color: [0.1, 0.1, 0.1, 1.0],
                    metallic: 0.0,
                    roughness: 0.5,
                    emissive: [2.0, 1.0, 0.2, 1.0],
                    at_role: None,
                }),
                AgentCommand::PlaceAsset(PlaceAssetCmd {
                    name: "monolith".into(),
                    kind: "rock".into(),
                    asset: "rock_a.glb".into(),
                    position: [4.0, 0.0, -3.0],
                    rotation_degrees: [0.0, 30.0, 0.0],
                    scale: 1.2,
                    at_role: None,
                }),
                AgentCommand::ScatterField(ScatterFieldCmd {
                    name: "pebbles".into(),
                    kind: "rock".into(),
                    assets: vec!["rock_a.glb".into()],
                    count: 5,
                    radius: 6.0,
                    position: [0.0, 0.0, 4.0],
                    scale: 0.5,
                    at_role: None,
                }),
                AgentCommand::SetLight(SetLightCmd {
                    name: "lamp".into(),
                    color: [1.0, 0.9, 0.8, 1.0],
                    intensity: 5000.0,
                    position: Some([0.0, 5.0, 0.0]),
                    direction: None,
                }),
                AgentCommand::DeleteEntity {
                    name: "pebbles_3".into(),
                },
            ],
            description: Some("a lonely beacon".into()),
        }
    }

    #[test]
    fn manifest_validates_and_performs() {
        let t = track();
        let a = analysis(Some(build()));
        let pack = pack();
        let m = world_manifest(&ExportInput {
            track: &t,
            analysis: &a,
            mood: &crate::theme::moods()[0],
            assets: Some(&pack),
            audio: None,
        });
        let issues = wt::validate_manifest(&m, &wt::WorldLimits::default());
        assert!(
            issues.iter().all(|i| i.severity != wt::Severity::Error),
            "{issues:?}"
        );
        let names: Vec<&str> = m.entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"ground") && names.contains(&"sun"));
        assert!(
            names.contains(&"beacon") && names.contains(&"monolith") && names.contains(&"lamp")
        );
        assert!(names.contains(&"pebbles_1") && !names.contains(&"pebbles_3"));
        assert_eq!(
            names.iter().filter(|n| n.starts_with("pebbles_")).count(),
            4
        );
        let unique: HashSet<&&str> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "names are unique");
        assert!(m.next_entity_id > m.entities.iter().map(|e| e.id.0).max().unwrap());

        let beacon = m
            .entities
            .iter()
            .find(|e| e.name.as_str() == "beacon")
            .unwrap();
        assert!(matches!(
            beacon.modulations[0].signal,
            wt::SignalSource::Beat
        ));
        let monolith = m
            .entities
            .iter()
            .find(|e| e.name.as_str() == "monolith")
            .unwrap();
        assert_eq!(
            monolith.mesh_asset.as_ref().unwrap().path,
            "models/rock_a.glb"
        );
        assert!(
            (monolith.transform.scale[0] - 7.0 / 2.0 * 1.2).abs() < 1e-4,
            "hero span 7 / dims 2"
        );
        assert!(matches!(
            monolith.modulations[0].target,
            wt::ModulationTarget::Scale
        ));
        let lamp = m
            .entities
            .iter()
            .find(|e| e.name.as_str() == "lamp")
            .unwrap();
        assert_eq!(
            lamp.light.as_ref().unwrap().light_type,
            wt::LightType::Point
        );

        let st = m.soundtrack.as_ref().unwrap();
        assert!(
            st.path.is_none() && st.license.is_none(),
            "personal library ships no audio"
        );
        assert_eq!(st.title.as_deref(), Some("Amber Drift"));
        assert_eq!(st.bpm, 120.0);
        assert_eq!(st.energy.len(), 31);
        assert_eq!(m.meta.description.as_deref(), Some("a lonely beacon"));
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            !json.contains("embedding"),
            "the CLAP embedding never leaves the sidecar"
        );
    }

    #[test]
    fn scatter_replays_like_the_executor() {
        let t = track();
        let a = analysis(Some(build()));
        let pack = pack();
        let input = ExportInput {
            track: &t,
            analysis: &a,
            mood: &crate::theme::moods()[0],
            assets: Some(&pack),
            audio: None,
        };
        let first = world_manifest(&input);
        let second = world_manifest(&input);
        assert_eq!(first, second, "deterministic");
        let seed = crate::world_assets::fold_seed("deadbeef|pebbles");
        let offsets = crate::world_assets::scatter_offsets(seed, 5, 6.0);
        let p1 = first
            .entities
            .iter()
            .find(|e| e.name.as_str() == "pebbles_1")
            .unwrap();
        let expected = (Vec3::new(0.0, 0.0, 4.0) + offsets[0]).to_array();
        assert_eq!(p1.transform.position, expected);
    }

    #[test]
    fn rule_world_without_build_still_exports() {
        let t = track();
        let a = analysis(None);
        let m = world_manifest(&ExportInput {
            track: &t,
            analysis: &a,
            mood: &crate::theme::moods()[1],
            assets: None,
            audio: Some(AudioRef {
                path: "music/amber-drift.mp3".into(),
                license: "CC0-1.0".into(),
            }),
        });
        assert_eq!(m.entities.len(), 2);
        assert!(m.meta.name.contains("Amber Drift"));
        let st = m.soundtrack.as_ref().unwrap();
        assert_eq!(st.path.as_deref(), Some("music/amber-drift.mp3"));
        assert_eq!(st.license.as_deref(), Some("CC0-1.0"));
        assert!(
            wt::validate_manifest(&m, &wt::WorldLimits::default())
                .iter()
                .all(|i| i.severity != wt::Severity::Error)
        );
    }

    #[test]
    fn write_manifest_writes_both_formats() {
        let dir = std::env::temp_dir().join(format!("verse-export-{}", std::process::id()));
        let m = wt::WorldManifest::new("tmp");
        write_manifest(&dir, "abc", &m).unwrap();
        assert!(dir.join("abc.world.json").is_file());
        assert!(dir.join("abc.world.ron").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
