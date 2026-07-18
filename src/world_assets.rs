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
    /// Index into [`crate::theme::MOODS`].
    pub mood: usize,
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

/// Per-mood arrangement ("structured layouts" as deterministic rules — the
/// M7-lite stand-in for full WFC, ARCHITECTURE §3 layer 4): organic spiral
/// for EMBER/TIDE, a jittered grid for VELVET CIRCUIT (city block), and
/// concentric rings for GLASS EXPANSE (crystal symmetry). Same (mood, seed)
/// → same layout.
fn layout_position(mood: usize, i: usize, rng: &mut u64) -> Vec3 {
    let fi = i as f32;
    match mood {
        1 => {
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
        3 => {
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
        _ => {
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

/// Materialize rise animation (spec 1g): each prop grows in from the ground,
/// staggered so the last one settles on the incoming track's first downbeat.
#[derive(Component)]
pub struct PropRise {
    delay: f32,
    dur: f32,
    target: f32,
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
/// world holds its breath mid-materialize.
pub fn rise_props(
    time: Res<Time>,
    clock: Res<crate::WorldClock>,
    mut props: Query<(&mut PropRise, &mut Transform)>,
) {
    let dt = time.delta_secs() * clock.speed;
    for (mut rise, mut tf) in &mut props {
        if rise.t >= rise.delay + rise.dur {
            continue;
        }
        rise.t += dt;
        let f = ((rise.t - rise.delay) / rise.dur).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - f).powi(3); // out-cubic, no overshoot
        tf.scale = Vec3::splat((rise.target * eased).max(rise.target * 0.01));
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

/// (Re)place ground props for the current mood on a mood change, rising in a
/// materialize sequence that settles on the current track's first downbeat
/// (when its analysis is already in — the next track is prefetched, so the
/// crossfade case normally has it).
#[allow(clippy::too_many_arguments)]
pub fn populate_world_props(
    theme: Res<Theme>,
    layout: Res<WorldLayout>,
    assets: Res<WorldAssets>,
    asset_server: Res<AssetServer>,
    analysis: Res<crate::analysis::AnalysisStore>,
    playback: Res<crate::playback::Playback>,
    mut commands: Commands,
    existing: Query<Entity, With<WorldProp>>,
    mut last: Local<Option<(usize, u64)>>,
) {
    let mood = theme.mood % crate::theme::MOODS.len();
    if *last == Some((mood, layout.seed)) {
        return;
    }
    *last = Some((mood, layout.seed));

    for e in &existing {
        commands.entity(e).despawn();
    }
    let Some(manifest) = &assets.manifest else {
        return;
    };

    // "Geometry settles on the first downbeat" (spec 1g): the stagger window
    // ends at the incoming track's beat offset when known, else mid-window.
    let settle = playback
        .queue
        .get(playback.current % playback.queue.len().max(1))
        .and_then(|t| t.id.as_deref())
        .and_then(|id| analysis.beat_offset_for(id))
        .map(|offset| offset.clamp(1.3, 3.5))
        .unwrap_or(2.4);

    let entries: Vec<_> = manifest.assets.iter().filter(|a| a.mood == mood).collect();
    let total: usize = entries.iter().map(|e| e.tier.count()).sum();

    // Seeded per-mood arrangement: same (mood, seed) → identical world.
    let mut rng = layout.seed ^ (mood as u64).wrapping_mul(0x9E37_79B9);
    let mut placed = 0usize;
    for entry in entries {
        let handle: Handle<_> = asset_server
            .load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}", entry.file)));
        let scale = entry.placement_scale();
        for _ in 0..entry.tier.count() {
            let pos = layout_position(mood, placed, &mut rng);
            let scale = scale * (0.85 + rand01(&mut rng) * 0.3);
            let mut e = commands.spawn((
                WorldProp,
                WorldAssetRoot(handle.clone()),
                Transform::from_translation(pos)
                    .with_scale(Vec3::splat(scale * 0.01))
                    .with_rotation(Quat::from_rotation_y(
                        rand01(&mut rng) * std::f32::consts::TAU,
                    )),
                PropRise {
                    delay: stagger_delay(placed, total, settle),
                    dur: RISE_SECS,
                    target: scale,
                    t: 0.0,
                },
            ));
            if let Some(range) = entry.tier.visibility_range() {
                e.insert(range);
            }
            placed += 1;
        }
    }
    if placed > 0 {
        info!(
            "Placed {placed} props for {} (settle {settle:.2}s)",
            crate::theme::MOODS[mood].world_name
        );
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
    let mood = theme.mood % crate::theme::MOODS.len();
    let mood_entries: Vec<_> = manifest.assets.iter().filter(|a| a.mood == mood).collect();
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
        let pos = layout_position(mood, i, &mut rng);
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
        for mood in 0..4 {
            let (mut a, mut b) = (99u64, 99u64);
            for i in 0..40 {
                assert_eq!(
                    layout_position(mood, i, &mut a),
                    layout_position(mood, i, &mut b)
                );
            }
        }
        // Grid (mood 1): positions snap to the 7.5m pitch lattice ±jitter.
        let mut rng = 5u64;
        let p = layout_position(1, 23, &mut rng);
        let lattice = |v: f32, off: f32| ((v - off) / 7.5).fract().abs();
        assert!(lattice(p.x, -33.75) < 0.25 || lattice(p.x, -33.75) > 0.75);
        // Rings (mood 3): radius stays within the ring band.
        let mut rng = 5u64;
        let p = layout_position(3, 40, &mut rng);
        let r = (p.x * p.x + p.z * p.z).sqrt();
        assert!((7.0..60.0).contains(&r), "ring radius {r}");
    }
}
