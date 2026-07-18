//! Playback + beat state.
//!
//! There is no audio engine yet — this module *simulates* the signals the real
//! MIR pipeline (see `idea.md`) will eventually provide: a transport clock, a
//! beat phase, and a slow energy envelope. The UI reads these resources so the
//! HUD is already wired to react to music.

use std::path::PathBuf;

use bevy::prelude::*;

/// One track in the queue.
#[derive(Clone)]
pub struct Track {
    pub title: String,
    pub artist: String,
    /// Duration in seconds.
    pub duration: f32,
    /// Index into [`crate::theme::MOODS`] — the world this song imagines.
    pub mood: usize,
    /// Section label shown under the title, e.g. "Cascade Hour · Slow Light".
    pub section: String,
    /// Audio file on disk. `None` for the built-in demo tracks — those play
    /// silently on the simulated clock.
    pub path: Option<PathBuf>,
    /// Stable identity: the blake3 content hash (also the analysis sidecar
    /// key — ARCHITECTURE R5). `None` for demo tracks. Survives renames,
    /// moves, and queue reordering; used for import dedupe.
    pub id: Option<String>,
}

impl Track {
    /// A demo track (no file) for the built-in queue.
    fn demo(title: &str, artist: &str, duration: f32, mood: usize, section: &str) -> Self {
        Self {
            title: title.into(),
            artist: artist.into(),
            duration,
            mood,
            section: section.into(),
            path: None,
            id: None,
        }
    }
}

/// The transport: what is playing, where we are, and what's next.
#[derive(Resource)]
pub struct Playback {
    pub queue: Vec<Track>,
    /// Index of the now-playing track in `queue`.
    pub current: usize,
    /// Seconds elapsed in the current track.
    pub elapsed: f32,
    pub playing: bool,
    /// Section boundaries as fractions 0..1 of the track — drawn as notches.
    pub sections: Vec<f32>,
    /// Bumped on queue/current changes (advance, import, reorder) so UI
    /// panels can refresh without diffing the queue every frame.
    pub revision: u64,
}

impl Default for Playback {
    fn default() -> Self {
        // Sample queue drawn from the spec mockups (1a / 1l); replaced by the
        // user's own music on import (see `audio::poll_import`).
        let queue = vec![
            Track::demo(
                "Amber Waking",
                "Cascade Hour",
                243.0,
                0,
                "Cascade Hour · Slow Light",
            ),
            Track::demo("Night Bloom", "Lys", 227.0, 2, "Chrome Gardens · Rising"),
            Track::demo(
                "Glass Runner",
                "Nova Dusk",
                202.0,
                3,
                "Auto Camera · Slow Orbit",
            ),
            Track::demo("Static Bloom", "Vel", 195.0, 1, "Velvet Circuit · Surge"),
            Track::demo(
                "Undertow",
                "Saltwater Choir",
                311.0,
                2,
                "Tide Gardens · Ebb",
            ),
            Track::demo("Low Sun", "Miren", 280.0, 0, "Ember Flats · Dusk"),
            Track::demo(
                "Hollow Light",
                "The Quiet Party",
                232.0,
                3,
                "Glass Expanse · Late",
            ),
        ];
        Self {
            elapsed: 161.0, // 2:41, matching the hero mockup
            current: 0,
            playing: true,
            sections: vec![0.0, 0.18, 0.42, 0.63, 0.85],
            queue,
            revision: 0,
        }
    }
}

impl Playback {
    pub fn track(&self) -> &Track {
        &self.queue[self.current % self.queue.len()]
    }
    pub fn next_track(&self) -> &Track {
        &self.queue[(self.current + 1) % self.queue.len()]
    }
    pub fn duration(&self) -> f32 {
        self.track().duration
    }
    /// Progress 0..1 through the current track.
    pub fn fraction(&self) -> f32 {
        (self.elapsed / self.duration().max(1.0)).clamp(0.0, 1.0)
    }
    /// Advance to the next song, returning its mood index.
    pub fn advance(&mut self) -> usize {
        self.current = (self.current + 1) % self.queue.len();
        self.elapsed = 0.0;
        self.revision += 1;
        self.track().mood
    }
}

/// Format seconds as `M:SS` (tabular, matching the HUD).
pub fn fmt_time(secs: f32) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Beat + energy signals. `phase` ramps 0→1 each beat; `pulse` spikes to 1 on
/// each beat and decays; `energy` is a slow 0..1 envelope for the song's drive.
#[derive(Resource)]
pub struct Beat {
    pub bpm: f32,
    pub phase: f32,
    pub pulse: f32,
    pub energy: f32,
    /// Wall-clock accumulator used to shape the simulated energy envelope.
    pub clock: f32,
    /// First-beat offset in seconds (from analysis, PLAN.md M3).
    pub offset: f32,
    /// True once `bpm`/`offset` come from real analysis — the phase is then
    /// derived from the transport clock instead of integrated.
    pub grid: bool,
}

impl Default for Beat {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            phase: 0.0,
            pulse: 0.0,
            energy: 0.6,
            clock: 0.0,
            offset: 0.0,
            grid: false,
        }
    }
}

/// Advance the transport and synthesise the beat/energy signals.
///
/// Signal ownership by source (PLAN.md M1–M3):
/// - no real audio → everything simulated here;
/// - real audio, no analysis → `audio.rs` owns elapsed + energy/pulse (live
///   tap); phase free-runs at the default bpm;
/// - real audio + analysis grid → phase/pulse derive from the transport
///   clock against the measured beat grid (predictive, tight).
pub fn advance_playback(
    time: Res<Time>,
    audio_active: Res<crate::AudioActive>,
    mut playback: ResMut<Playback>,
    mut beat: ResMut<Beat>,
    mut theme: ResMut<crate::theme::Theme>,
) {
    let dt = time.delta_secs();
    let live = audio_active.0;

    // Simulated energy envelope — only while no live tap feeds it.
    beat.clock += dt;
    if !live {
        beat.energy = 0.62 + 0.28 * (beat.clock * 0.35).sin() * (beat.clock * 0.11).cos();
        beat.energy = beat.energy.clamp(0.3, 0.95);
    }

    if !playback.playing {
        // Pulse still decays so a paused world settles.
        beat.pulse = (beat.pulse - dt * 3.0).max(0.0);
        return;
    }
    beat.pulse = (beat.pulse - dt * 4.0).max(0.0);

    // Beat phase.
    if live && beat.grid {
        // Derived from the real clock against the measured grid.
        let prev = beat.phase;
        beat.phase = ((playback.elapsed - beat.offset).max(0.0) * beat.bpm / 60.0).fract();
        if beat.phase < prev {
            beat.pulse = 1.0;
        }
    } else {
        let prev = beat.phase;
        beat.phase = (beat.phase + dt * beat.bpm / 60.0).fract();
        // Spike on wrap only when fully simulated; with a live tap (but no
        // grid yet) real onsets own the pulse.
        if beat.phase < prev && !live {
            beat.pulse = 1.0;
        }
    }

    // Transport — simulated only while no real stream owns the clock.
    if live {
        return;
    }
    playback.elapsed += dt;
    if playback.elapsed >= playback.duration() {
        let mood = playback.advance();
        theme.mood = mood;
    }
}

#[cfg(test)]
mod tests {
    // Tests tweak a couple of fields on the default (queue-building) transport.
    #![allow(clippy::field_reassign_with_default)]
    use super::*;

    #[test]
    fn fmt_time_formats_and_clamps() {
        assert_eq!(fmt_time(0.0), "0:00");
        assert_eq!(fmt_time(65.0), "1:05");
        assert_eq!(fmt_time(161.0), "2:41");
        assert_eq!(fmt_time(243.0), "4:03");
        assert_eq!(fmt_time(-5.0), "0:00");
    }

    #[test]
    fn fraction_is_clamped_0_to_1() {
        let mut p = Playback::default();
        p.elapsed = 0.0;
        assert_eq!(p.fraction(), 0.0);
        p.elapsed = p.duration() * 2.0;
        assert_eq!(p.fraction(), 1.0);
    }

    #[test]
    fn advance_wraps_and_resets_elapsed() {
        let mut p = Playback::default();
        p.current = p.queue.len() - 1;
        p.elapsed = 99.0;
        let mood = p.advance();
        assert_eq!(p.current, 0);
        assert_eq!(p.elapsed, 0.0);
        assert_eq!(mood, p.queue[0].mood);
    }

    #[test]
    fn next_track_wraps_to_first() {
        let mut p = Playback::default();
        p.current = p.queue.len() - 1;
        assert_eq!(p.next_track().title, p.queue[0].title);
    }
}
