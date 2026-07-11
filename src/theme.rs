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
        Self {
            display: pick("fonts/Marcellus-Regular.ttf"),
            ui: pick("fonts/HankenGrotesk-Regular.ttf"),
            ui_medium: pick("fonts/HankenGrotesk-Medium.ttf"),
            ui_semibold: pick("fonts/HankenGrotesk-SemiBold.ttf"),
        }
    }
}

// ---------------------------------------------------------------------------
// World moods — the sampled accent + the world's own palette.
// ---------------------------------------------------------------------------

/// A world's palette. The `accent` is the only value that reaches the chrome;
/// the rest paints the 3D backdrop so the HUD always overlays a live world.
#[derive(Clone, Copy)]
pub struct WorldMood {
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
}

/// The built-in worlds, mirroring the moods named in the spec mockups
/// (Dawn Chorus, Neon Surge, Night Bloom, Glass Runner).
pub const MOODS: &[WorldMood] = &[
    // Dawn Chorus — warm ambers. Accent #FFB38A.
    WorldMood {
        world_name: "EMBER FLATS",
        accent: Color::srgb(1.0, 0.702, 0.541),
        sky_top: Color::srgb(1.0, 0.851, 0.690),
        sky_bottom: Color::srgb(0.788, 0.514, 0.435),
        fog: Color::srgb(0.62, 0.42, 0.40),
        ground: Color::srgb(0.478, 0.310, 0.388),
        ambient: Color::srgb(1.0, 0.80, 0.62),
    },
    // Neon Surge — cyan over deep indigo/magenta. Accent #62F5FF.
    WorldMood {
        world_name: "VELVET CIRCUIT",
        accent: Color::srgb(0.384, 0.961, 1.0),
        sky_top: Color::srgb(0.141, 0.063, 0.329),
        sky_bottom: Color::srgb(0.043, 0.027, 0.133),
        fog: Color::srgb(0.24, 0.10, 0.34),
        ground: Color::srgb(0.051, 0.043, 0.165),
        ambient: Color::srgb(0.42, 0.62, 0.95),
    },
    // Night Bloom — cool aqua. Accent #A8D8DE.
    WorldMood {
        world_name: "TIDE GARDENS",
        accent: Color::srgb(0.659, 0.847, 0.871),
        sky_top: Color::srgb(0.071, 0.200, 0.243),
        sky_bottom: Color::srgb(0.043, 0.047, 0.067),
        fog: Color::srgb(0.176, 0.416, 0.447),
        ground: Color::srgb(0.071, 0.200, 0.243),
        ambient: Color::srgb(0.55, 0.80, 0.82),
    },
    // Glass Runner — pale ice over violet. Accent #8EF4FF.
    WorldMood {
        world_name: "GLASS EXPANSE",
        accent: Color::srgb(0.557, 0.957, 1.0),
        sky_top: Color::srgb(0.137, 0.106, 0.176),
        sky_bottom: Color::srgb(0.043, 0.027, 0.133),
        fog: Color::srgb(0.20, 0.16, 0.28),
        ground: Color::srgb(0.086, 0.067, 0.145),
        ambient: Color::srgb(0.62, 0.78, 0.92),
    },
];

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
}
