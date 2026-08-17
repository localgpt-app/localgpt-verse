//! Design tokens for Reverie's UI — the single source of truth for the
//! "chrome rents, it doesn't own" aesthetic described in the design spec.
//!
//! The rule from the spec: **the chrome never changes; only the accent does.**
//! Every panel, veil, hairline, radius and type role is a constant here; the
//! one variable is the per-world `accent`, sampled from whatever world is
//! playing (see [`WorldMood`]).

use bevy::prelude::*;
use bevy::text::{FontSize, FontSource};

// ---------------------------------------------------------------------------
// Constants — the parts of the system that never change (spec 1q "constants").
// ---------------------------------------------------------------------------

/// Near-black page base (`#0b0c11`).
pub const BASE: Color = Color::srgb(0.043, 0.047, 0.067);

/// Primary UI text (`#F4F2EE`) — 15.8:1 on the veil (AAA).
pub const TEXT: Color = Color::srgb(0.957, 0.949, 0.933);
/// Secondary / warm-white text (`#e8e6e1`).
pub const TEXT_DIM: Color = Color::srgb(0.910, 0.902, 0.882);

/// Muted label text — `rgba(232,230,225,.55)`.
pub fn text_muted() -> Color {
    TEXT_DIM.with_alpha(0.55)
}

/// HUD veil — `rgba(10,10,18,.55)`. Used behind free-floating HUD clusters.
pub fn veil_hud() -> Color {
    Color::srgba(10.0 / 255.0, 10.0 / 255.0, 18.0 / 255.0, 0.55)
}

/// Panel veil — `rgba(10,10,18,.8)`. Used behind full panels (Queue, Pause…).
pub fn veil_panel() -> Color {
    Color::srgba(10.0 / 255.0, 10.0 / 255.0, 18.0 / 255.0, 0.8)
}

/// Hairline border — `rgba(255,255,255,.12)`.
pub fn hairline() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.12)
}

// Corner radii — 8 / 12 / pill.
pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
pub const RADIUS_PILL: f32 = 9999.0;

// ---------------------------------------------------------------------------
// Typography — Marcellus (display) + Hanken Grotesk (UI, tabular time).
// ---------------------------------------------------------------------------

/// Font roles resolved to concrete `Handle<Font>`s. When the bundled font
/// files are absent the handles fall back to Bevy's embedded default font, so
/// the app still runs — it just won't be Marcellus/Hanken until the `.ttf`s
/// are dropped into `assets/fonts/` (see `assets/fonts/README.md`).
#[derive(Resource, Clone)]
pub struct Fonts {
    /// Marcellus — world names, hero titles.
    pub display: Handle<Font>,
    /// Hanken Grotesk regular — body/labels.
    pub ui: Handle<Font>,
    /// Hanken Grotesk medium — controls, emphasis.
    pub ui_medium: Handle<Font>,
    /// Hanken Grotesk semibold — buttons, headers.
    pub ui_semibold: Handle<Font>,
}

/// Build a [`TextFont`] with a pixel size. Bevy 0.19 wraps text sizes in the
/// `FontSize` enum, so this centralises the `f32` → `FontSize::Px` conversion.
pub fn text_font(font: Handle<Font>, size: f32) -> TextFont {
    TextFont {
        font: FontSource::Handle(font),
        font_size: FontSize::from(size),
        ..default()
    }
}

impl Fonts {
    /// Load the design fonts, gracefully falling back to the embedded default
    /// font for any file that isn't present on disk.
    pub fn load(asset_server: &AssetServer) -> Self {
        // Resolve against the same asset root Bevy uses (see `world_assets`).
        let root = crate::world_assets::asset_root();
        let pick = |file: &str| -> Handle<Font> {
            if root.join(file).exists() {
                asset_server.load(file.to_string())
            } else {
                Handle::default()
            }
        };
        // Hanken Grotesk ships as one variable font; share it across the UI
        // weight roles (Bevy renders the default instance).
        let hanken = pick("fonts/HankenGrotesk.ttf");
        Self {
            display: pick("fonts/Marcellus-Regular.ttf"),
            ui: hanken.clone(),
            ui_medium: hanken.clone(),
            ui_semibold: hanken,
        }
    }
}

// ---------------------------------------------------------------------------
// World moods — the sampled accent + the world's own palette.
// ---------------------------------------------------------------------------

/// How a world arranges its ground props.
///
/// Carried by the mood rather than matched on its index: an arrangement is part
/// of what a world *is*, and keying it positionally meant inserting a mood
/// silently handed its layout to whoever took that slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arrangement {
    /// Golden-angle spiral, centre kept clear for the camera's orbit.
    Spiral,
    /// Jittered city grid — blocks and streets.
    Grid,
    /// Concentric rings; crystal symmetry.
    Rings,
    /// Clumps separated by open ground — outcrops on a plain.
    Clusters,
    /// Meandering rows following a slow wave — tidal terraces.
    Terraces,
}

impl Default for Arrangement {
    /// [`Arrangement::Spiral`] — the original layout, and what a world gets
    /// when it does not choose one. None of the built-ins use it any more (each
    /// has its own), so it is the neutral starting point for a new mood.
    fn default() -> Self {
        Self::Spiral
    }
}

/// A world's palette. The `accent` is the only value that reaches the chrome;
/// the rest paints the 3D backdrop so the HUD always overlays a live world.
#[derive(Clone, Copy)]
pub struct WorldMood {
    /// Stable identity, independent of this mood's position in [`MOODS`].
    ///
    /// Everything durable refers to a mood by **this**, not by index: sidecars
    /// persist it, and the asset manifest may carry it (see
    /// [`crate::world_assets::AssetEntry`]). Positions shift whenever a mood is
    /// added, removed, or reordered, and a stored index would then silently
    /// resolve to a different world. Never change an existing id — that is the
    /// one edit this field cannot survive.
    pub id: &'static str,
    /// Display name shown in the HUD, e.g. "EMBER FLATS".
    pub world_name: &'static str,
    /// The sampled accent — legible on the veil (OKLCH L clamped 0.78–0.86).
    pub accent: Color,
    /// Upper sky / far gradient stop.
    pub sky_top: Color,
    /// Lower sky / horizon gradient stop.
    pub sky_bottom: Color,
    /// Distance fog colour.
    pub fog: Color,
    /// Ground plane base colour.
    pub ground: Color,
    /// Ambient light tint.
    pub ambient: Color,
    /// How this world lays out its ground props.
    pub arrangement: Arrangement,
}

/// The built-in worlds, mirroring the moods named in the spec mockups
/// (Dawn Chorus, Neon Surge, Night Bloom, Glass Runner).
pub const MOODS: &[WorldMood] = &[
    // Dawn Chorus — warm ambers. Accent #FFB38A.
    WorldMood {
        id: "ember-flats",
        world_name: "EMBER FLATS",
        accent: Color::srgb(1.0, 0.702, 0.541),
        sky_top: Color::srgb(1.0, 0.851, 0.690),
        sky_bottom: Color::srgb(0.788, 0.514, 0.435),
        fog: Color::srgb(0.62, 0.42, 0.40),
        ground: Color::srgb(0.478, 0.310, 0.388),
        ambient: Color::srgb(1.0, 0.80, 0.62),
        arrangement: Arrangement::Clusters,
    },
    // Neon Surge — cyan over deep indigo/magenta. Accent #62F5FF.
    WorldMood {
        id: "velvet-circuit",
        world_name: "VELVET CIRCUIT",
        accent: Color::srgb(0.384, 0.961, 1.0),
        sky_top: Color::srgb(0.141, 0.063, 0.329),
        sky_bottom: Color::srgb(0.043, 0.027, 0.133),
        fog: Color::srgb(0.24, 0.10, 0.34),
        ground: Color::srgb(0.051, 0.043, 0.165),
        ambient: Color::srgb(0.42, 0.62, 0.95),
        arrangement: Arrangement::Grid,
    },
    // Night Bloom — cool aqua. Accent #A8D8DE.
    WorldMood {
        id: "tide-gardens",
        world_name: "TIDE GARDENS",
        accent: Color::srgb(0.659, 0.847, 0.871),
        sky_top: Color::srgb(0.071, 0.200, 0.243),
        sky_bottom: Color::srgb(0.043, 0.047, 0.067),
        fog: Color::srgb(0.176, 0.416, 0.447),
        ground: Color::srgb(0.071, 0.200, 0.243),
        ambient: Color::srgb(0.55, 0.80, 0.82),
        arrangement: Arrangement::Terraces,
    },
    // Glass Runner — pale ice over violet. Accent #8EF4FF.
    WorldMood {
        id: "glass-expanse",
        world_name: "GLASS EXPANSE",
        accent: Color::srgb(0.557, 0.957, 1.0),
        sky_top: Color::srgb(0.137, 0.106, 0.176),
        sky_bottom: Color::srgb(0.043, 0.027, 0.133),
        fog: Color::srgb(0.20, 0.16, 0.28),
        ground: Color::srgb(0.086, 0.067, 0.145),
        ambient: Color::srgb(0.62, 0.78, 0.92),
        arrangement: Arrangement::Rings,
    },
];

/// Resolve a durable mood reference to a live index into `moods`.
///
/// Callers hold two things: an `id` written by whatever produced the record,
/// and an `index` that was the only representation before ids existed. The id
/// wins whenever it matches, which is what lets [`MOODS`] be reordered or
/// extended without silently repointing every sidecar and manifest entry at a
/// different world.
///
/// Falls back to `index` (wrapped) when the id is absent — an older sidecar —
/// or unrecognised — a mood that has since been removed or renamed. Degrading
/// to some world is right here: a missing world should cost the user their
/// pinned look, not their library.
pub fn resolve_mood(moods: &[WorldMood], id: Option<&str>, index: usize) -> usize {
    if let Some(id) = id
        && let Some(found) = moods.iter().position(|mood| mood.id == id)
    {
        return found;
    }
    if moods.is_empty() {
        0
    } else {
        index % moods.len()
    }
}

/// The stable id of the mood at `index`, for writing into a durable record.
pub fn mood_id(index: usize) -> &'static str {
    MOODS[index % MOODS.len()].id
}

/// The currently playing world's palette, plus a smoothed accent used by the
/// chrome. Index rotates as tracks change.
#[derive(Resource, Default)]
pub struct Theme {
    /// Index into [`MOODS`].
    pub mood: usize,
}

impl Theme {
    pub fn current(&self) -> &'static WorldMood {
        &MOODS[self.mood % MOODS.len()]
    }
    pub fn accent(&self) -> Color {
        self.current().accent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_is_first_mood() {
        let t = Theme::default();
        assert_eq!(t.mood, 0);
        assert_eq!(t.accent(), MOODS[0].accent);
    }

    #[test]
    fn mood_index_wraps() {
        let t = Theme { mood: MOODS.len() };
        assert_eq!(t.current().world_name, MOODS[0].world_name);
    }

    #[test]
    fn mood_ids_are_unique() {
        let mut ids: Vec<&str> = MOODS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "two moods share an id");
    }

    /// The whole point of the id: a record written against one ordering still
    /// resolves to the same world after the list is reordered or extended.
    #[test]
    fn a_stored_id_survives_reordering() {
        let original = MOODS;
        let mut shuffled: Vec<WorldMood> = original.to_vec();
        shuffled.reverse();

        for (index, mood) in original.iter().enumerate() {
            let resolved = resolve_mood(&shuffled, Some(mood.id), index);
            assert_eq!(
                shuffled[resolved].id, mood.id,
                "`{}` resolved to the wrong world after reordering",
                mood.id
            );
        }
    }

    #[test]
    fn a_stored_id_survives_an_insertion() {
        let mut extended: Vec<WorldMood> = MOODS.to_vec();
        let newcomer = WorldMood {
            id: "new-world",
            ..MOODS[0]
        };
        extended.insert(0, newcomer);

        // Written when "glass-expanse" was at index 3; it is at 4 now.
        let resolved = resolve_mood(&extended, Some("glass-expanse"), 3);
        assert_eq!(extended[resolved].id, "glass-expanse");
        // And the positional reading is exactly the silent corruption the id
        // prevents.
        assert_ne!(extended[3].id, "glass-expanse");
    }

    #[test]
    fn an_absent_id_falls_back_to_the_stored_index() {
        // A sidecar written before ids existed.
        assert_eq!(resolve_mood(MOODS, None, 2), 2);
        assert_eq!(resolve_mood(MOODS, None, MOODS.len() + 1), 1);
    }

    #[test]
    fn an_unknown_id_falls_back_rather_than_failing() {
        // A mood that has since been removed: the user loses their pinned look,
        // not their library.
        let resolved = resolve_mood(MOODS, Some("retired-world"), 2);
        assert_eq!(resolved, 2);
    }

    #[test]
    fn resolving_against_an_empty_list_is_not_a_panic() {
        assert_eq!(resolve_mood(&[], Some("ember-flats"), 3), 0);
        assert_eq!(resolve_mood(&[], None, 3), 0);
    }

    #[test]
    fn mood_id_round_trips_through_resolve() {
        for index in 0..MOODS.len() {
            assert_eq!(resolve_mood(MOODS, Some(mood_id(index)), 999), index);
        }
    }
}
