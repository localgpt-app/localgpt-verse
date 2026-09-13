//! Real 3D world props — PLAN.md M6.
//!
//! Loads a manifest of CC0 glTF models (bundled from the `reverie-assets`
//! repo into `assets/models/`) and places them on the ground per world mood,
//! in three tiers (hero landmarks / medium props / ground scatter). These are
//! grounded *features*; the procedural drifters in `world.rs` stay as floating
//! ambient. When no manifest is present (fresh clone without the asset pack)
//! or a mood has no matching assets, the world simply keeps its primitives —
//! so the app always runs.

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use serde::Deserialize;

use crate::theme::{Arrangement, Theme};

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
    /// props read as landmarks without dwarfing the scene). Fallback when the
    /// manifest lacks native `dims` for a model.
    fn base_scale(self) -> f32 {
        match self {
            Tier::Hero => 1.5,
            Tier::Medium => 1.15,
            Tier::Scatter => 0.9,
        }
    }
    /// Target real-world span (metres) — models with known native size are
    /// rescaled to it, since Poly Haven scans range from 0.1 m (shell) to
    /// 90 m (cliff). Keeps every tier's footprint consistent across the pack.
    fn target_span(self) -> f32 {
        match self {
            Tier::Hero => 7.0,
            Tier::Medium => 2.5,
            Tier::Scatter => 1.0,
        }
    }
    /// Distance culling (ARCHITECTURE R7): ground cover drops out well inside
    /// the fog band (18–95), medium props just before the fog wall; hero
    /// landmarks stay visible — they define the skyline.
    fn visibility_range(self) -> Option<VisibilityRange> {
        match self {
            Tier::Hero => None,
            Tier::Medium => Some(VisibilityRange::abrupt(0.0, 80.0)),
            Tier::Scatter => Some(VisibilityRange::abrupt(0.0, 45.0)),
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
    /// Index into [`crate::theme::moods()`].
    ///
    /// Positional, and written by `fetch_polyhaven.py` in the separate
    /// `reverie-assets` repo — so reordering [`crate::theme::moods()`] silently
    /// repoints all 52 assets, and nothing in this repo would catch it. Read
    /// through [`AssetEntry::mood_index`], which prefers `mood_id`.
    pub mood: usize,
    /// Stable mood id ([`crate::theme::WorldMood::id`]), when the manifest
    /// carries one.
    ///
    /// Not yet emitted by the generator: adding it there is a change in another
    /// repo, and the numeric field keeps working until it lands. Accepting it
    /// now means a regenerated manifest is understood without a code change
    /// here, and that a mood added or reordered in the meantime is a
    /// recoverable mistake rather than a silent one.
    #[serde(default)]
    pub mood_id: Option<String>,
    #[serde(default = "one")]
    pub scale: f32,
    /// Native dimensions in metres `[x, y, z]` (from the source catalog) —
    /// placement rescales to the tier's target span when present.
    #[serde(default)]
    pub dims: Option<[f32; 3]>,
    pub license: String,
    pub author: String,
    #[allow(dead_code)]
    pub source: String,
}

fn one() -> f32 {
    1.0
}

impl AssetEntry {
    /// The live mood index, preferring the stable id over the stored position.
    pub fn mood_index(&self) -> usize {
        crate::theme::resolve_mood(crate::theme::moods(), self.mood_id.as_deref(), self.mood)
    }

    /// Human tier name for the Credits row.
    pub fn tier_label(&self) -> &'static str {
        match self.tier {
            Tier::Hero => "hero landmark",
            Tier::Medium => "prop",
            Tier::Scatter => "ground cover",
        }
    }

    /// Placement scale: normalized to the tier's target span from the
    /// model's native size when known, else the tier's legacy multiplier.
    pub fn placement_scale(&self) -> f32 {
        match self.dims {
            Some(d) => {
                let span = d[0].max(d[1]).max(d[2]).max(0.01);
                self.scale * self.tier.target_span() / span
            }
            None => self.scale * self.tier.base_scale(),
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

/// The current world layout seed. Same (mood, seed) → same placement; "Build
/// a different world" re-rolls it and "Keep this world" pins it per track
/// (ARCHITECTURE R6).
#[derive(Resource)]
pub struct WorldLayout {
    pub seed: u64,
}

impl Default for WorldLayout {
    fn default() -> Self {
        Self { seed: 0x5EED }
    }
}

/// splitmix64 — tiny deterministic PRNG (no `rand` dependency).
pub(crate) fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Uniform f32 in [0, 1).
fn rand01(state: &mut u64) -> f32 {
    (splitmix(state) >> 40) as f32 / (1u64 << 24) as f32
}

/// Deterministic per-track default seed (from the path, like `path_mood`).
pub(crate) fn path_seed(path: &std::path::Path) -> u64 {
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xC0FFEE_u64, |acc, &b| {
            acc.wrapping_mul(31).wrapping_add(b as u64)
        })
}

/// Place prop `i` under a world's arrangement ("structured layouts" as
/// deterministic rules — the M7-lite stand-in for full WFC, ARCHITECTURE §3
/// layer 4). Same (arrangement, seed) → same layout.
///
/// Which arrangement a world uses is [`crate::theme::WorldMood::arrangement`],
/// not this function's business: keying it on the mood index meant inserting a
/// mood handed its layout to whatever took that slot.
fn layout_position(arrangement: Arrangement, i: usize, rng: &mut u64) -> Vec3 {
    let fi = i as f32;
    match arrangement {
        Arrangement::Grid => {
            // City grid: 7.5m pitch, 10 columns, jittered ±1.8m.
            const PITCH: f32 = 7.5;
            const COLS: usize = 10;
            let x = (i % COLS) as f32 * PITCH - (COLS as f32 - 1.0) * PITCH / 2.0;
            let z = (i / COLS) as f32 * PITCH - 30.0;
            Vec3::new(
                x + (rand01(rng) - 0.5) * 3.6,
                -0.5,
                z + (rand01(rng) - 0.5) * 3.6,
            )
        }
        Arrangement::Rings => {
            // Rings: 8m inner radius growing 6m per ring, 6+3k seats per ring.
            let mut ring = 0usize;
            let mut first = 0usize; // first index in this ring
            let mut seats = 6usize;
            while i >= first + seats {
                first += seats;
                ring += 1;
                seats = 6 + 3 * ring;
            }
            let radius = 8.0 + 6.0 * ring as f32 + (rand01(rng) - 0.5) * 2.0;
            let ang = (i - first) as f32 / seats as f32 * std::f32::consts::TAU
                + (rand01(rng) - 0.5) * 0.35;
            Vec3::new(ang.cos() * radius, -0.5, ang.sin() * radius)
        }
        Arrangement::Clusters => {
            // Outcrops on open ground: clumps of CLUMP_SIZE around scattered
            // anchors, so the eye reads groups with empty flats between them
            // rather than an even field.
            const CLUMP_SIZE: usize = 4;
            const CLUMP_SPREAD: f32 = 3.4;
            // The anchor's own jitter must not depend on how many props landed
            // before it, so derive it from the clump index alone.
            let clump = i / CLUMP_SIZE;
            let mut anchor_rng = (clump as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let ang = rand01(&mut anchor_rng) * std::f32::consts::TAU;
            let radius = 11.0 + rand01(&mut anchor_rng) * 26.0;
            let anchor = Vec3::new(ang.cos() * radius, -0.5, ang.sin() * radius);

            let off = rand01(rng) * std::f32::consts::TAU;
            let dist = rand01(rng).sqrt() * CLUMP_SPREAD;
            anchor + Vec3::new(off.cos() * dist, 0.0, off.sin() * dist)
        }
        Arrangement::Terraces => {
            // Tidal terraces: long rows on a slow sine, each row offset so the
            // bands read as water lines rather than a grid.
            const ROW_LEN: usize = 7;
            const ROW_PITCH: f32 = 8.5;
            const SPAN: f32 = 46.0;
            let row = i / ROW_LEN;
            let seat = i % ROW_LEN;
            let z = row as f32 * ROW_PITCH - 26.0;
            let t = seat as f32 / (ROW_LEN - 1) as f32;
            // Stagger alternate rows by half a seat so terraces interlock.
            let stagger = if row.is_multiple_of(2) {
                0.0
            } else {
                0.5 / ROW_LEN as f32
            };
            let x = (t + stagger - 0.5) * SPAN;
            // The wave gives each row its curve; amplitude grows with distance.
            let wave = (z * 0.07).sin() * (4.0 + row as f32 * 0.6);
            Vec3::new(
                x + (rand01(rng) - 0.5) * 2.2,
                -0.5,
                z + wave + (rand01(rng) - 0.5) * 1.4,
            )
        }
        Arrangement::Spiral => {
            // Organic golden-angle spiral (the original arrangement); the
            // centre stays clear — it's the camera's focus and orbit path.
            let ang = fi * 2.399_963 + (rand01(rng) - 0.5) * 0.9;
            let radius = (9.0 + (fi + 2.0).sqrt() * 4.2 + (rand01(rng) - 0.5) * 3.0).max(8.0);
            Vec3::new(ang.cos() * radius, -0.5, ang.sin() * radius)
        }
    }
}

/// Marker for spawned prop entities, so a mood change can clear them.
#[derive(Component)]
pub struct WorldProp;

/// Marker for scatter-tier props: the only tier that follows the section
/// verbs' `scatter` multiplier after settling (the outro sinks the ground
/// cover back into the ground it rose from).
#[derive(Component)]
pub struct ScatterProp;

/// Ambient behaviour for a placed prop — a gentle per-prop sway/spin so the
/// grounded world breathes with the track instead of freezing after the
/// materialize. Heroes are monuments and stay still by design.
#[derive(Component)]
pub struct PropMotion {
    /// Home Y (bob returns here, never fights the placement).
    pub base_y: f32,
    /// Deterministic per-prop phase.
    pub seed: f32,
    /// Bob amplitude (metres) — mediums 0.15, scatter 0.06.
    pub bob: f32,
    /// Yaw speed (rad/s) — a slow drift, not a carousel.
    pub spin: f32,
}

/// Animate placed props' ambient behaviour after they settle: a slow bob and
/// spin scaled by the recipe's motion, the section feel, and the bass level
/// (the ground cover visibly rides the low end). Comfort › gentler world
/// motion damps the bob; the spin is slow enough to keep.
pub fn animate_props(
    time: Res<Time>,
    clock: Res<crate::WorldClock>,
    comfort: Res<crate::Comfort>,
    beat: Res<crate::playback::Beat>,
    stems: Res<crate::playback::StemLevels>,
    feel: Res<crate::world::SectionFeel>,
    mut props: Query<(&PropRise, &PropMotion, &mut Transform), Without<crate::world::Drifter>>,
) {
    let gentle = if comfort.gentler_motion { 0.4 } else { 1.0 };
    let bass = stems.0[1].max(beat.bass);
    let t = time.elapsed_secs() * clock.speed;
    let dt = time.delta_secs() * clock.speed;
    for (rise, motion, mut tf) in &mut props {
        if rise.t < rise.delay + rise.dur {
            continue; // still materializing — the rise owns the transform
        }
        let p = t + motion.seed * std::f32::consts::TAU;
        let bob = motion.bob * (1.0 + bass * 1.5) * gentle * feel.motion;
        tf.translation.y = motion.base_y + p.sin() * bob;
        tf.rotate_y(dt * motion.spin * feel.motion);
    }
}

/// A landmark's emissive beacon, carrying its full-intensity emissive so
/// [`pulse_beacons`] can breathe it with the drums/bass without recomputing
/// the colour.
#[derive(Component)]
pub struct Beacon {
    pub base: LinearRgba,
}

/// Breathe the landmark beacons with the rhythm: the drums stem (Demucs
/// curve) when present, else the live tap's bass envelope. Reduce-flashing
/// holds them steady — this is the one stem layer that modulates
/// *brightness*, so it takes the strictest gate; everything else reacts via
/// motion.
pub fn pulse_beacons(
    comfort: Res<crate::Comfort>,
    beat: Res<crate::playback::Beat>,
    stems: Res<crate::playback::StemLevels>,
    feel: Res<crate::world::SectionFeel>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&Beacon, &MeshMaterial3d<StandardMaterial>)>,
) {
    let drums = stems.0[0].max(beat.bass);
    let intensity = if comfort.reduce_flashing {
        0.8 * feel.beacons.clamp(0.0, 1.5)
    } else {
        // Motion-dominant: a slow swell with a small beat tickle on top. The
        // section's verb lights the chorus up and dims the bridge.
        (0.55 + drums * 0.45 + beat.pulse * 0.12) * feel.beacons.clamp(0.0, 1.5)
    };
    for (beacon, mat) in &mut q {
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.emissive = LinearRgba::rgb(
                beacon.base.red * intensity,
                beacon.base.green * intensity,
                beacon.base.blue * intensity,
            );
        }
    }
}

/// Materialize rise animation (spec 1g): each prop grows in from the ground,
/// staggered so the last one settles on the incoming track's first downbeat.
#[derive(Component)]
pub struct PropRise {
    delay: f32,
    dur: f32,
    /// Target scale, per-axis (non-uniform for the skyline stelae, which grow
    /// upward only).
    target: Vec3,
    t: f32,
}

/// Rise duration per prop.
const RISE_SECS: f32 = 0.9;
/// First prop starts shortly after the palette wash begins.
const FIRST_DELAY: f32 = 0.35;

/// Start delay for prop `i` of `n`, spread so the final prop finishes its
/// rise exactly at `settle` (the first downbeat), decelerating overall.
pub(crate) fn stagger_delay(i: usize, n: usize, settle: f32) -> f32 {
    let last = (settle - RISE_SECS).max(FIRST_DELAY);
    if n <= 1 {
        return last;
    }
    FIRST_DELAY + (last - FIRST_DELAY) * (i as f32 / (n as f32 - 1.0))
}

/// Grow rising props in with an ease-out; obeys the world clock so a paused
/// world holds its breath mid-materialize. After settling, scatter props keep
/// following their section verb's scale (the outro sinks them).
pub fn rise_props(
    time: Res<Time>,
    clock: Res<crate::WorldClock>,
    feel: Res<crate::world::SectionFeel>,
    mut props: Query<(&mut PropRise, &mut Transform, Option<&ScatterProp>)>,
) {
    let dt = time.delta_secs() * clock.speed;
    for (mut rise, mut tf, scatter) in &mut props {
        if rise.t >= rise.delay + rise.dur {
            if scatter.is_some() {
                tf.scale = rise.target * feel.scatter.max(0.0);
            }
            continue;
        }
        rise.t += dt;
        let f = ((rise.t - rise.delay) / rise.dur).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - f).powi(3); // out-cubic, no overshoot
        tf.scale = rise.target * eased.max(0.01);
    }
}

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

/// Read `assets/models/manifest.json` from disk if present. Used both by the
/// Bevy-side loader ([`load_asset_manifest`]) and by the analysis worker
/// thread, which needs the asset vocabulary to build the agent's `place_asset`
/// tool schema but cannot reach the `World` resource.
pub(crate) fn read_manifest_from_disk() -> Option<AssetManifest> {
    let path = asset_root().join("models/manifest.json");
    std::fs::read_to_string(&path).ok().and_then(|text| {
        match serde_json::from_str::<AssetManifest>(&text) {
            Ok(m) => Some(m),
            Err(e) => {
                warn!("Ignoring asset manifest ({e})");
                None
            }
        }
    })
}

/// Load `assets/models/manifest.json` if present. Absent → procedural worlds.
pub fn load_asset_manifest(mut commands: Commands) {
    let manifest = read_manifest_from_disk();
    match &manifest {
        Some(m) => info!("Asset pack: {} models", m.assets.len()),
        None => info!("No asset pack bundled — procedural worlds"),
    }
    commands.insert_resource(WorldAssets { manifest });
}

/// (Re)place ground props for the current mood on a mood change, rising in a
/// materialize sequence that settles on the current track's first downbeat
/// (when its analysis is already in — the next track is prefetched, so the
/// crossfade case normally has it).
#[allow(clippy::too_many_arguments)]
pub fn populate_world_props(
    theme: Res<Theme>,
    layout: Res<WorldLayout>,
    assets: Res<WorldAssets>,
    active_recipe: Res<crate::recipe::ActiveRecipe>,
    asset_server: Res<AssetServer>,
    analysis: Res<crate::analysis::AnalysisStore>,
    playback: Res<crate::playback::Playback>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    existing: Query<Entity, With<WorldProp>>,
    mut last: Local<Option<(usize, u64, Option<String>)>>,
) {
    let mood = theme.mood % crate::theme::moods().len();
    let recipe = active_recipe.get();
    // M7: the recipe's primary biome overrides the mood's default
    // arrangement; its world name keys layout identity so two recipes over the
    // same (mood, seed) can still ask for different worlds.
    let arrangement = recipe
        .and_then(|r| r.biomes.first())
        .map(|b| b.layout.arrangement())
        .unwrap_or(crate::theme::moods()[mood].arrangement);
    let recipe_key = recipe.map(|r| r.world_name.clone());
    if *last == Some((mood, layout.seed, recipe_key.clone())) {
        return;
    }
    *last = Some((mood, layout.seed, recipe_key));

    for e in &existing {
        commands.entity(e).despawn();
    }
    let Some(manifest) = &assets.manifest else {
        return;
    };

    // "Geometry settles on the first downbeat" (spec 1g): the stagger window
    // ends at the incoming track's beat offset when known, else mid-window.
    let current_id = playback
        .queue
        .get(playback.current % playback.queue.len().max(1))
        .and_then(|t| t.id.clone());
    let settle = current_id
        .as_deref()
        .and_then(|id| analysis.beat_offset_for(id))
        .map(|offset| offset.clamp(1.3, 3.5))
        .unwrap_or(2.4);

    // Whole-song materialize: with measured sections, props rise *across the
    // track* — scatter in the intro, mediums through the verses, heroes
    // landing exactly on the chorus — instead of everything in the first
    // 2.4 s. The first section still settles on the downbeat (spec 1g).
    let track_duration = playback
        .queue
        .get(playback.current % playback.queue.len().max(1))
        .map(|t| t.duration)
        .unwrap_or(0.0);
    let sections = &playback.sections;
    let sectioned = sections.len() >= 3 && track_duration > 0.0;
    let chorus_seg = sections.len() / 2;
    let section_start =
        |seg: usize| -> f32 { sections.get(seg).copied().unwrap_or(0.0) * track_duration };

    // M7: a recipe may scale prop density within [0.3, 2.0] (already clamped).
    // Absent recipe → 1.0 (today's per-tier counts).
    let density = recipe.map(|r| r.density).unwrap_or(1.0);

    // Seeded per-mood arrangement: same (mood, seed, recipe) → identical world.
    let mut rng = layout.seed ^ (mood as u64).wrapping_mul(0x9E37_79B9);
    let mut plan: Vec<PlannedProp> = Vec::new();

    // Primary mood props, three tiers, each assigned the section it rises in.
    // The extended moods (Cinder Reach, Mirage Circuit, …) borrow their base
    // quadrant's asset set — same neighbourhood, different hour — until the
    // pack carries entries of their own.
    let mut entries: Vec<_> = manifest
        .assets
        .iter()
        .filter(|a| a.mood_index() == mood)
        .collect();
    if entries.is_empty() && mood >= crate::theme::ASSET_BASE_MOODS {
        let base = mood % crate::theme::ASSET_BASE_MOODS;
        entries = manifest
            .assets
            .iter()
            .filter(|a| a.mood_index() == base)
            .collect();
    }
    for entry in &entries {
        let count = (entry.tier.count() as f32 * density).round().max(1.0) as usize;
        for _ in 0..count {
            let i = plan.len();
            plan.push(PlannedProp {
                file: entry.file.clone(),
                pos: layout_position(arrangement, i, &mut rng),
                rot_y: rand01(&mut rng) * std::f32::consts::TAU,
                scale: entry.placement_scale() * (0.85 + rand01(&mut rng) * 0.3),
                range: entry.tier.visibility_range(),
                beacon: None,
                tier: entry.tier,
                seg: match entry.tier {
                    // Scatter carpet: the intro. Mediums: spread over the
                    // verses. Heroes: land on the chorus.
                    Tier::Scatter => 0,
                    Tier::Medium => 1 + (i % chorus_seg.max(1)),
                    Tier::Hero => chorus_seg,
                },
            });
        }
    }

    // M7 secondary biomes: contrasting accents from each secondary mood's own
    // set (non-hero tiers), claiming a share of the primary budget scaled by
    // that biome's density. At most two secondaries, so accents accent rather
    // than take over. Accents arrive *with* the chorus.
    if let Some(recipe) = recipe {
        let primary = plan.len().max(1);
        for biome in recipe.biomes.iter().skip(1).take(2) {
            let bmood = biome.mood % crate::theme::moods().len();
            let accent_count = (biome.density * primary as f32 * 0.25)
                .round()
                .clamp(1.0, 24.0) as usize;
            let accents: Vec<_> = manifest
                .assets
                .iter()
                .filter(|a| a.mood_index() == bmood && a.tier != Tier::Hero)
                .collect();
            if accents.is_empty() {
                continue;
            }
            for k in 0..accent_count {
                let entry = &accents[k % accents.len()];
                let i = plan.len();
                plan.push(PlannedProp {
                    file: entry.file.clone(),
                    pos: layout_position(biome.layout.arrangement(), i, &mut rng),
                    rot_y: rand01(&mut rng) * std::f32::consts::TAU,
                    scale: entry.placement_scale() * (0.8 + rand01(&mut rng) * 0.3),
                    range: entry.tier.visibility_range(),
                    beacon: None,
                    tier: entry.tier,
                    seg: chorus_seg,
                });
            }
        }
    }

    // M7 landmarks: hero assets matched to the recipe's `kind` by name
    // keywords (see `pick_hero_for_kind`), placed at their anchors. Heroes
    // stay visible through the fog — they define the skyline — and
    // `emissive > 0.1` raises a beacon above them (baked glTF materials can't
    // take a runtime emissive). They rise on the chorus.
    if let Some(recipe) = recipe {
        let heroes: Vec<_> = manifest
            .assets
            .iter()
            .filter(|a| a.mood_index() == mood && a.tier == Tier::Hero)
            .collect();
        let mut used: Vec<String> = Vec::new();
        for (i, landmark) in recipe.landmarks.iter().enumerate() {
            let Some(entry) = pick_hero_for_kind(&heroes, landmark.kind, &mut used) else {
                break;
            };
            plan.push(PlannedProp {
                file: entry.file.clone(),
                pos: landmark_anchor(landmark.at, i),
                rot_y: rand01(&mut rng) * std::f32::consts::TAU,
                scale: landmark.scale * entry.placement_scale(),
                range: None,
                beacon: (landmark.emissive > 0.1).then_some(landmark.emissive),
                tier: Tier::Hero,
                seg: chorus_seg,
            });
        }
    }

    // One materialize over the whole plan: seg-0 props keep the
    // settle-on-the-downbeat stagger; later sections rise as they arrive.
    let total = plan.len();
    let beacon_mesh = meshes.add(Sphere::new(1.0).mesh().ico(2).unwrap());
    let accent = theme.current().accent;
    for (i, p) in plan.into_iter().enumerate() {
        let handle: Handle<_> =
            asset_server.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}", p.file)));
        let delay = if !sectioned || p.seg == 0 {
            stagger_delay(i, total, settle)
        } else {
            // Rise a moment into the section (deterministic jitter from the
            // plan index), never before the intro has settled.
            (section_start(p.seg) + (i as f32 * 0.37).fract() * 0.7).max(settle + 0.2)
        };
        let mut e = commands.spawn((
            WorldProp,
            WorldAssetRoot(handle),
            Transform::from_translation(p.pos)
                .with_scale(Vec3::splat(p.scale * 0.01))
                .with_rotation(Quat::from_rotation_y(p.rot_y)),
            PropRise {
                delay,
                dur: RISE_SECS,
                target: Vec3::splat(p.scale),
                t: 0.0,
            },
        ));
        if let Some(range) = p.range {
            e.insert(range);
        }
        if p.tier == Tier::Scatter {
            e.insert(ScatterProp);
        }
        // Ambient behaviour: mediums bob visibly, scatter shimmers — both
        // riding the bass. Heroes stay monuments.
        if p.tier != Tier::Hero {
            e.insert(PropMotion {
                base_y: p.pos.y,
                seed: rand01(&mut rng),
                bob: if p.tier == Tier::Medium { 0.15 } else { 0.06 },
                spin: if p.tier == Tier::Medium { 0.08 } else { 0.2 },
            });
        }
        if let Some(emissive) = p.beacon {
            // An unlit sphere — glow without a per-landmark light cost. Its
            // intensity breathes with the drums/bass in `pulse_beacons`
            // (Comfort-gated to steady under reduce-flashing).
            let radius = (0.22 * p.scale).clamp(0.1, 0.6);
            let height = (2.6 * p.scale).clamp(2.0, 14.0);
            let base = LinearRgba::new(
                accent.to_linear().red * 1.4 * emissive,
                accent.to_linear().green * 1.4 * emissive,
                accent.to_linear().blue * 1.4 * emissive,
                1.0,
            );
            commands.spawn((
                WorldProp,
                Beacon { base },
                Mesh3d(beacon_mesh.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: accent.with_alpha(0.9),
                    emissive: base,
                    unlit: true,
                    ..default()
                })),
                Transform::from_translation(p.pos + Vec3::new(0.0, height, 0.0))
                    .with_scale(Vec3::splat(radius)),
            ));
        }
    }

    // The skyline is the waveform: thin stelae around the rim whose heights
    // sample the track's energy curve, each rising as its section arrives —
    // walk the rim and you read the song's shape (drops are literal peaks).
    // Same song → same skyline; analysis pending → no skyline this pass, and
    // the (mood, seed) re-populate when it lands draws it.
    if let Some(curve) = current_id
        .as_deref()
        .and_then(|id| analysis.get(id))
        .map(|a| &a.energy)
        && !curve.is_empty()
    {
        const STELAE: usize = 28;
        let stela_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let stela_mat = materials.add(StandardMaterial {
            base_color: theme.current().ground,
            emissive: LinearRgba::new(
                accent.to_linear().red * 0.22,
                accent.to_linear().green * 0.22,
                accent.to_linear().blue * 0.22,
                1.0,
            ),
            unlit: true,
            ..default()
        });
        for k in 0..STELAE {
            let f = k as f32 / (STELAE - 1) as f32;
            let idx = (f * (curve.len() - 1) as f32).round() as usize;
            let h = 1.5 + curve[idx].clamp(0.0, 1.0) * 10.0;
            let ang = f * std::f32::consts::TAU;
            let pos = Vec3::new(ang.cos() * 46.0, -0.5, ang.sin() * 46.0);
            // Which section does stela k live in? It rises with it.
            let seg = sections
                .iter()
                .filter(|&&s| s <= f)
                .count()
                .saturating_sub(1);
            let delay = if sectioned {
                section_start(seg).max(1.3)
            } else {
                stagger_delay(k, STELAE, settle)
            };
            commands.spawn((
                WorldProp,
                Mesh3d(stela_mesh.clone()),
                MeshMaterial3d(stela_mat.clone()),
                Transform::from_translation(pos)
                    .looking_at(Vec3::new(0.0, pos.y, 0.0), Vec3::Y)
                    .with_scale(Vec3::new(0.8, h, 1.4)),
                PropRise {
                    delay,
                    dur: RISE_SECS,
                    target: Vec3::new(0.8, h, 1.4),
                    t: 0.0,
                },
            ));
        }
    }

    if total > 0 {
        info!(
            "Placed {total} props for {} (settle {settle:.2}s, density {density:.2})",
            crate::theme::moods()[mood].world_name
        );
    }
}

/// One planned prop placement — collected first so every class of placement
/// (mood props, secondary-biome accents, recipe landmarks) shares one
/// staggered materialize pass and one denominator for the stagger math.
struct PlannedProp {
    /// glTF file relative to `assets/models/`.
    file: String,
    pos: Vec3,
    rot_y: f32,
    /// Target scale (metres-normalized); the entity starts at 1% and rises.
    scale: f32,
    range: Option<VisibilityRange>,
    /// M7 landmark emissive (0..1): beacon strength floated above the prop.
    /// `None` = no beacon.
    beacon: Option<f32>,
    /// Placement tier — drives the section-verb markers (`ScatterProp`) and
    /// the section the prop rises in.
    tier: Tier,
    /// The section (segment index) this prop rises in — the whole-song
    /// materialize. 0 keeps the settle-on-the-downbeat stagger.
    seg: usize,
}

/// Keyword vocabulary mapping a recipe [`crate::recipe::LandmarkKind`] onto
/// hero assets by name — the M7-lite stand-in for embedding-based selection
/// (which wants the CLAP text space). Ties break by manifest order, and every
/// hero is used once before any repeats (round-robin fallback), so two
/// "Spire" landmarks don't clone the same model when alternatives exist.
fn pick_hero_for_kind<'a>(
    heroes: &[&'a AssetEntry],
    kind: crate::recipe::LandmarkKind,
    used: &mut Vec<String>,
) -> Option<&'a AssetEntry> {
    use crate::recipe::LandmarkKind;
    let keywords: &[&str] = match kind {
        LandmarkKind::Spire => &[
            "tower",
            "spire",
            "monolith",
            "antenna",
            "lighthouse",
            "pillar",
            "column",
        ],
        LandmarkKind::Gateway => &["arch", "gate", "portal", "bridge", "torii", "ruin"],
        LandmarkKind::Mass => &["rock", "boulder", "cliff", "mesa", "mountain", "reef"],
        LandmarkKind::Monument => &["crystal", "sculpture", "monument", "statue", "obelisk"],
    };
    heroes
        .iter()
        .enumerate()
        .filter(|(_, e)| !used.contains(&e.file))
        .map(|(i, e)| {
            let score = e
                .name
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| keywords.contains(w))
                .count();
            (score, i, *e)
        })
        .filter(|(score, _, _)| *score > 0)
        .max_by_key(|(score, i, _)| score * 1000 - *i)
        .map(|(_, _, e)| {
            used.push(e.file.clone());
            e
        })
        // Fallback: unused hero in manifest order (the old round-robin).
        .or_else(|| {
            let e = heroes.iter().find(|e| !used.contains(&e.file)).copied()?;
            used.push(e.file.clone());
            Some(e)
        })
}

/// World position for a landmark anchor. Cardinal rotates around the rim by
/// index so multiple Cardnals spread out; Rim sits on the far horizon.
fn landmark_anchor(anchor: crate::recipe::Anchor, i: usize) -> Vec3 {
    use crate::recipe::Anchor;
    match anchor {
        Anchor::Center => Vec3::new(0.0, -0.5, -6.0),
        Anchor::Cardinal => {
            let ang = i as f32 * std::f32::consts::FRAC_PI_2;
            Vec3::new(ang.cos() * 16.0, -0.5, ang.sin() * 16.0 - 4.0)
        }
        Anchor::Rim => {
            let ang = (i as f32 + 0.5) * std::f32::consts::FRAC_PI_2;
            Vec3::new(ang.cos() * 40.0, -0.5, ang.sin() * 40.0)
        }
    }
}

// --- perf stress test (REVERIE_STRESS) ---------------------------------------

/// Dev perf validation: `REVERIE_STRESS=5000 cargo run` spawns that many prop
/// instances (cycling the current mood's manifest set) and logs frame rates —
/// the idea.md Stage-1 budget is 60 fps @ ~5k instances on a mid GPU, which
/// ARCHITECTURE §3 flags as an unvalidated claim until measured.
#[derive(Resource)]
pub struct StressTest {
    pub target: usize,
    spawned: bool,
    frames: u32,
    total_frames: u32,
    window: f32,
    total: f32,
}

impl StressTest {
    pub fn from_env() -> Option<Self> {
        let target = std::env::var("REVERIE_STRESS").ok()?.parse().ok()?;
        // N=0 is the control run: no props, just the frame-rate report.
        Some(Self {
            target,
            spawned: false,
            frames: 0,
            total_frames: 0,
            window: 0.0,
            total: 0.0,
        })
    }
}

/// Spawn the stress field once, cycling the current mood's manifest entries on
/// the same golden-angle spiral as the real world. No `PropRise` — we measure
/// the steady-state frame cost, not the materialize hit.
pub fn stress_spawn(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    assets: Res<WorldAssets>,
    theme: Res<Theme>,
    mut stress: ResMut<StressTest>,
) {
    if stress.spawned {
        return;
    }
    // The manifest loads at Startup; wait until it (or its absence) is known.
    let Some(manifest) = &assets.manifest else {
        return;
    };
    stress.spawned = true;
    let mood = theme.mood % crate::theme::moods().len();
    let arrangement = crate::theme::moods()[mood].arrangement;
    let mood_entries: Vec<_> = manifest
        .assets
        .iter()
        .filter(|a| a.mood_index() == mood)
        .collect();
    let entries: Vec<_> = if mood_entries.is_empty() {
        manifest.assets.iter().collect()
    } else {
        mood_entries
    };
    if entries.is_empty() {
        warn!("stress: manifest has no assets");
        return;
    }

    let mut rng = 0xDEAD_5EED_u64;
    for i in 0..stress.target {
        let entry = entries[i % entries.len()];
        let handle: Handle<_> = asset_server
            .load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}", entry.file)));
        let pos = layout_position(arrangement, i, &mut rng);
        let scale = entry.placement_scale() * (0.85 + rand01(&mut rng) * 0.3);
        let mut e = commands.spawn((
            WorldProp,
            WorldAssetRoot(handle),
            Transform::from_translation(pos)
                .with_scale(Vec3::splat(scale))
                .with_rotation(Quat::from_rotation_y(
                    rand01(&mut rng) * std::f32::consts::TAU,
                )),
        ));
        if let Some(range) = entry.tier.visibility_range() {
            e.insert(range);
        }
    }
    info!("stress: spawning {} prop roots", stress.target);
}

/// Log windowed FPS every 5 s; exit with an overall summary after 30 s.
pub fn stress_report(
    time: Res<Time<bevy::time::Real>>,
    mut stress: ResMut<StressTest>,
    mut exit: MessageWriter<AppExit>,
) {
    let dt = time.delta_secs();
    stress.frames += 1;
    stress.total_frames += 1;
    stress.window += dt;
    stress.total += dt;
    if stress.window >= 5.0 {
        info!(
            "stress: {:.1} fps ({} frames / {:.1}s)",
            stress.frames as f32 / stress.window,
            stress.frames,
            stress.window
        );
        stress.frames = 0;
        stress.window = 0.0;
    }
    if stress.total >= 30.0 {
        info!(
            "stress: done — overall {:.1} fps over {:.0}s",
            stress.total_frames as f32 / stress.total,
            stress.total
        );
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stagger_ends_on_settle_and_orders() {
        let settle = 2.4;
        let n = 16;
        let first = stagger_delay(0, n, settle);
        let last = stagger_delay(n - 1, n, settle);
        assert_eq!(first, FIRST_DELAY);
        // Last prop finishes its rise exactly at the settle time.
        assert!((last + RISE_SECS - settle).abs() < 1e-5);
        // Monotonic stagger.
        for i in 1..n {
            assert!(stagger_delay(i, n, settle) >= stagger_delay(i - 1, n, settle));
        }
    }

    #[test]
    fn stagger_clamps_tight_settles() {
        // A settle earlier than one rise can finish still yields a sane delay.
        let d = stagger_delay(3, 4, 0.5);
        assert!(d >= FIRST_DELAY);
    }

    #[test]
    fn seeded_layout_is_deterministic() {
        let (mut a, mut b) = (42u64, 42u64);
        for _ in 0..8 {
            assert_eq!(splitmix(&mut a), splitmix(&mut b));
        }
        let mut s = 7u64;
        for _ in 0..100 {
            let v = rand01(&mut s);
            assert!((0.0..1.0).contains(&v));
        }
        let p = std::path::Path::new("/music/a.flac");
        assert_eq!(path_seed(p), path_seed(p));
        assert_ne!(
            path_seed(p),
            path_seed(std::path::Path::new("/music/b.flac"))
        );
    }

    #[test]
    fn layout_styles_are_deterministic_and_distinct() {
        for arrangement in ALL_ARRANGEMENTS {
            let (mut a, mut b) = (99u64, 99u64);
            for i in 0..40 {
                assert_eq!(
                    layout_position(arrangement, i, &mut a),
                    layout_position(arrangement, i, &mut b)
                );
            }
        }
        // Grid: positions snap to the 7.5m pitch lattice ±jitter.
        let mut rng = 5u64;
        let p = layout_position(Arrangement::Grid, 23, &mut rng);
        let lattice = |v: f32, off: f32| ((v - off) / 7.5).fract().abs();
        assert!(lattice(p.x, -33.75) < 0.25 || lattice(p.x, -33.75) > 0.75);
        // Rings: radius stays within the ring band.
        let mut rng = 5u64;
        let p = layout_position(Arrangement::Rings, 40, &mut rng);
        let r = (p.x * p.x + p.z * p.z).sqrt();
        assert!((7.0..60.0).contains(&r), "ring radius {r}");
    }

    const ALL_ARRANGEMENTS: [Arrangement; 5] = [
        Arrangement::Spiral,
        Arrangement::Grid,
        Arrangement::Rings,
        Arrangement::Clusters,
        Arrangement::Terraces,
    ];

    /// Average nearest-neighbour distance — a cheap proxy for "does this read
    /// as clumped or as evenly spread".
    fn mean_nearest_neighbour(arrangement: Arrangement, n: usize) -> f32 {
        let mut rng = 7u64;
        let points: Vec<Vec3> = (0..n)
            .map(|i| layout_position(arrangement, i, &mut rng))
            .collect();
        let total: f32 = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                points
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, q)| p.distance(*q))
                    .fold(f32::MAX, f32::min)
            })
            .sum();
        total / n as f32
    }

    #[test]
    fn the_base_moods_have_distinct_arrangements_and_variants_match_their_base() {
        let moods = crate::theme::moods();
        let base = &moods[..crate::theme::ASSET_BASE_MOODS.min(moods.len())];
        // The base four each own a distinct arrangement (world identity).
        for (i, a) in base.iter().map(|m| m.arrangement).enumerate() {
            for (j, b) in base.iter().map(|m| m.arrangement).enumerate() {
                assert!(
                    i == j || a != b,
                    "`{}` and `{}` share the {a:?} arrangement",
                    base[i].id,
                    base[j].id
                );
            }
        }
        // The extended variants borrow their base quadrant's asset set — and
        // must lay it out the same way, or the "same neighbourhood, different
        // hour" promise breaks.
        for (i, m) in moods.iter().enumerate().skip(base.len()) {
            let b = &base[i % base.len()];
            assert_eq!(
                m.arrangement, b.arrangement,
                "`{}` must arrange like its base `{}`",
                m.id, b.id
            );
        }
    }

    #[test]
    fn clusters_clump_more_tightly_than_the_spiral() {
        // The point of Clusters is groups with open ground between them, so
        // neighbours sit closer than in an evenly-spread layout.
        let clustered = mean_nearest_neighbour(Arrangement::Clusters, 40);
        let spread = mean_nearest_neighbour(Arrangement::Spiral, 40);
        assert!(
            clustered < spread,
            "clusters ({clustered}) should be tighter than spiral ({spread})"
        );
    }

    #[test]
    fn terraces_form_distinct_rows() {
        // Seven props per row, so the first seven share a row band and the
        // eighth starts the next one.
        let mut rng = 3u64;
        let points: Vec<Vec3> = (0..14)
            .map(|i| layout_position(Arrangement::Terraces, i, &mut rng))
            .collect();
        let row0_spread = points[..7].iter().map(|p| p.z).fold(f32::MIN, f32::max)
            - points[..7].iter().map(|p| p.z).fold(f32::MAX, f32::min);
        let row_gap = (points[7].z - points[0].z).abs();
        assert!(
            row_gap > row0_spread / 2.0,
            "rows ({row_gap}) are not separated from within-row spread ({row0_spread})"
        );
    }

    #[test]
    fn open_centre_arrangements_keep_the_camera_clear() {
        // Spiral, Rings, and Clusters treat the centre as the camera's focus
        // and orbit path. Grid deliberately does not — a city block reads as a
        // city because you stand inside it — and Terraces runs rows across the
        // whole span, so neither promises a clear middle.
        for arrangement in [
            Arrangement::Spiral,
            Arrangement::Rings,
            Arrangement::Clusters,
        ] {
            let mut rng = 11u64;
            let close = (0..60)
                .map(|i| layout_position(arrangement, i, &mut rng))
                .filter(|p| (p.x * p.x + p.z * p.z).sqrt() < 3.0)
                .count();
            assert_eq!(close, 0, "{arrangement:?} placed props on the camera");
        }
    }
}
