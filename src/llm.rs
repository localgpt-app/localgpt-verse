//! LLM-authored scene recipes — PLAN.md M7 (`llm` feature).
//!
//! The top rung of the signal-ownership ladder: takes the MIR features from
//! [`crate::analysis::TrackAnalysis`] (bpm, energy, sections, mood, and the CLAP
//! embedding when present) and asks a local LLM to author a [`crate::recipe::WorldRecipe`]
//! describing how to dress the world up. The recipe is constrained generation —
//! [`crate::recipe::WorldRecipe`] derives `schemars::JsonSchema` and mistral.rs's
//! `generate_structured` enforces it, so the model literally cannot emit an
//! invalid recipe.
//!
//! Like [`crate::ml::ClapModel`] / [`crate::demucs::StemModel`], this degrades
//! to `None`: no `llm` feature → module not compiled; feature but no model →
//! [`RecipeModel::try_load`] is `None`; model but generation fails →
//! [`RecipeModel::generate`] returns `None`. In every case the renderer keeps
//! the rule-derived recipe (today's path). The app is never broken by a
//! missing LLM tier.
//!
//! # Model
//! `GgufModelBuilder` is aimed at the GGUF fetched by `scripts/fetch-bonsai.sh`
//! (Bonsai 8B 1-bit by default — see PLAN §M7). A standard Q4_K_M GGUF (e.g.
//! Qwen2.5-7B / Llama-3.1-8B) is the documented fallback if the 1-bit model
//! underperforms or fails to load: just drop a different `.gguf` + matching
//! `tokenizer.json` into `assets/llm/` and `try_load` picks it up.
//!
//! # Async
//! mistral.rs's `build()` and `generate_structured()` are async. This crate is
//! otherwise sync (the analysis worker is a plain `std::thread`). We run a
//! dedicated single-threaded tokio runtime per generation inside that thread —
//! no async pollution of the rest of the app.

use std::path::PathBuf;

use bevy::log::{info, warn};
use mistralrs::{GgufModelBuilder, TextMessageRole, TextMessages};

use crate::analysis::TrackAnalysis;
use crate::recipe::WorldRecipe;
use crate::theme::MOODS;

/// The loaded LLM, ready to author recipes. `None` from `try_load` when the
/// model file is missing — the caller keeps the rule-derived recipe.
pub struct RecipeModel {
    model: mistralrs::Model,
}

impl RecipeModel {
    /// Borrow the underlying mistral.rs model. Used by the agent tier
    /// (`crate::agent::run_session`) to run tool-calling on the same loaded
    /// model, so we don't pay for two model loads.
    pub fn model_mut(&mut self) -> &mut mistralrs::Model {
        &mut self.model
    }

    /// Load the GGUF under `assets/llm/` if present. Returns `None` (and logs)
    /// when no model is found — see the module docs for the fallback contract.
    ///
    /// Builds a single-threaded tokio runtime on the calling thread: mistral.rs
    /// is async, and we keep the rest of the app sync.
    pub fn try_load() -> Option<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| warn!("llm: can't start tokio runtime: {e}"))
            .ok()?;

        let (model_id, gguf_file, tokenizer_json) = locate_model()?;

        rt.block_on(async move {
            // `GgufModelBuilder::new` takes `impl ToString`; pass the directory
            // and filenames as strings (mistral.rs treats a local path as the
            // model source, avoiding a HuggingFace fetch).
            let model_id_str = model_id.to_string_lossy().into_owned();
            let model = GgufModelBuilder::new(model_id_str, vec![gguf_file.clone()])
                .with_tokenizer_json(tokenizer_json.clone())
                .build()
                .await
                .map_err(|e| warn!("llm: can't build mistral.rs model: {e}"))
                .ok()?;
            info!("llm: recipe model loaded ({gguf_file})");
            Some(RecipeModel { model })
        })
    }

    /// Author a recipe for one track. `None` on any failure — the caller keeps
    /// the rule-derived recipe. The returned recipe is already [`WorldRecipe::clamped`]
    /// so no out-of-range value can reach the renderer.
    pub fn generate(&mut self, analysis: &TrackAnalysis) -> Option<WorldRecipe> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| warn!("llm: can't start tokio runtime: {e}"))
            .ok()?;

        rt.block_on(async {
            let messages = build_prompt(analysis);
            match self
                .model
                .generate_structured::<WorldRecipe>(messages)
                .await
            {
                Ok(recipe) => {
                    info!(
                        "llm: recipe for \"{}\" ({} biomes, {} landmarks)",
                        recipe.world_name,
                        recipe.biomes.len(),
                        recipe.landmarks.len()
                    );
                    Some(recipe.clamped())
                }
                Err(e) => {
                    warn!("llm: recipe generation failed ({e}) — keeping rule recipe");
                    None
                }
            }
        })
    }
}

/// Find the recipe model on disk. Returns `(model_id, gguf_file, tokenizer_json)`
/// as the trio `GgufModelBuilder` needs, or `None` (with a warning) when no
/// model is present. Scans `assets/llm/` for the first `.gguf` it finds, so the
/// default (Bonsai 8B 1-bit) and the documented standard-GGUF fallback are both
/// discovered automatically — the user just drops a model in.
///
/// `model_id` is the local directory (mistral.rs treats a local path as the
/// model source, avoiding a HuggingFace fetch); `gguf_file`/`tokenizer_json` are
/// the bare filenames within that directory.
fn locate_model() -> Option<(PathBuf, String, String)> {
    let dir = crate::world_assets::asset_root().join("llm");
    if !dir.exists() {
        warn!(
            "llm: {} not found — rule recipes only (run scripts/fetch-bonsai.sh)",
            dir.display()
        );
        return None;
    }

    // First .gguf in the directory wins.
    let gguf = std::fs::read_dir(&dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".gguf") {
            Some(name)
        } else {
            None
        }
    })?;
    if gguf.is_empty() {
        warn!("llm: no .gguf in {} — rule recipes only", dir.display());
        return None;
    }

    let tokenizer = std::fs::read_dir(&dir)
        .ok()?
        .find_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == "tokenizer.json" {
                Some(name)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "tokenizer.json".to_string());

    Some((dir, gguf, tokenizer))
}

/// Turn a track's MIR analysis into the system+user prompt for recipe authoring.
/// Kept compact (analysis already carries the hard facts) and explicit about
/// the mood palette so the model modulates *within* the right world.
fn build_prompt(analysis: &TrackAnalysis) -> TextMessages {
    let mood_name = MOODS
        .get(analysis.mood)
        .map(|m| m.world_name)
        .unwrap_or("UNKNOWN");

    let bpm = if analysis.bpm > 0.0 {
        format!("{:.0}", analysis.bpm)
    } else {
        "unknown".into()
    };
    let mean_energy = analysis.energy.iter().sum::<f32>() / analysis.energy.len().max(1) as f32;
    let energy_band = match (mean_energy * 5.0) as usize {
        0..=1 => "low",
        2..=3 => "medium",
        _ => "high",
    };
    let sections = analysis.sections.len().max(1);

    let system = "You design immersive 3D worlds for a music visualizer. Given a song's \
analysis, return a JSON object describing how to dress the world: biome(s), landmarks, \
atmosphere, section-synced choreography, and particles. Modulate WITHIN the given mood — \
do not pick a different base palette. Keep it tasteful and performant (few landmarks, \
modest density). Return only the JSON.";

    let user = format!(
        "Mood: {mood_name} (must be the primary biome's mood, index {idx}).\n\
Tempo: {bpm} BPM. Mean energy: {energy_band} ({mean_energy:.2}). Sections: {sections}.\n\
Author a WorldRecipe for this track.",
        idx = analysis.mood
    );

    TextMessages::new()
        .add_message(TextMessageRole::System, system)
        .add_message(TextMessageRole::User, user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_model_returns_none_when_dir_absent() {
        // The default asset root has no `llm/` dir, so this is the no-model
        // path — the graceful-fallback contract.
        assert!(locate_model().is_none());
    }
}
