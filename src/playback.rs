//! Playback + beat state.
//!
//! There is no audio engine yet — this module *simulates* the signals the real
//! MIR pipeline (see `idea.md`) will eventually provide: a transport clock, a
//! beat phase, and a slow energy envelope. The UI reads these resources so the
//! HUD is already wired to react to music.

use bevy::prelude::*;

/// One track in the queue.
#[derive(Clone)]
pub struct Track {
    pub title: &'static str,
    pub artist: &'static str,
    /// Duration in seconds.
    pub duration: f32,
    /// Index into [`crate::theme::MOODS`] — the world this song imagines.
    pub mood: usize,
    /// Section label shown under the title, e.g. "Cascade Hour · Slow Light".
    pub section: &'static str,
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
}

impl Default for Playback {
    fn default() -> Self {
        // Sample queue drawn from the spec mockups (1a / 1l).
        let queue = vec![
            Track {
                title: "Amber Waking",
                artist: "Cascade Hour",
                duration: 243.0,
                mood: 0,
                section: "Cascade Hour · Slow Light",
            },
            Track {
                title: "Night Bloom",
                artist: "Lys",
                duration: 227.0,
                mood: 2,
                section: "Chrome Gardens · Rising",
            },
            Track {
                title: "Glass Runner",
                artist: "Nova Dusk",
                duration: 202.0,
                mood: 3,
                section: "Auto Camera · Slow Orbit",
            },
            Track {
                title: "Static Bloom",
                artist: "Vel",
                duration: 195.0,
                mood: 1,
                section: "Velvet Circuit · Surge",
            },
            Track {
                title: "Undertow",
                artist: "Saltwater Choir",
                duration: 311.0,
                mood: 2,
                section: "Tide Gardens · Ebb",
            },
            Track {
                title: "Low Sun",
                artist: "Miren",
                duration: 280.0,
                mood: 0,
                section: "Ember Flats · Dusk",
            },
            Track {
                title: "Hollow Light",
                artist: "The Quiet Party",
                duration: 232.0,
                mood: 3,
                section: "Glass Expanse · Late",
            },
        ];
        Self {
            elapsed: 161.0, // 2:41, matching the hero mockup
            current: 0,
            playing: true,
            sections: vec![0.0, 0.18, 0.42, 0.63, 0.85],
            queue,
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
    /// Wall-clock accumulator used to shape the energy envelope.
    pub clock: f32,
}

impl Default for Beat {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            phase: 0.0,
            pulse: 0.0,
            energy: 0.6,
            clock: 0.0,
        }
    }
}

/// Advance the transport and synthesise the beat/energy signals.
pub fn advance_playback(
    time: Res<Time>,
    mut playback: ResMut<Playback>,
    mut beat: ResMut<Beat>,
    mut theme: ResMut<crate::theme::Theme>,
) {
    let dt = time.delta_secs();

    // Energy: a slow breathing envelope so the world/HUD feels alive even
    // without real audio. Range ~0.35..0.9.
    beat.clock += dt;
    beat.energy = 0.62 + 0.28 * (beat.clock * 0.35).sin() * (beat.clock * 0.11).cos();
    beat.energy = beat.energy.clamp(0.3, 0.95);

    if !playback.playing {
        // Pulse still decays so a paused world settles.
        beat.pulse = (beat.pulse - dt * 3.0).max(0.0);
        return;
    }

    // Beat phase.
    let beats_per_sec = beat.bpm / 60.0;
    let prev = beat.phase;
    beat.phase = (beat.phase + dt * beats_per_sec).fract();
    if beat.phase < prev {
        // Crossed a beat boundary — spike the pulse.
        beat.pulse = 1.0;
    }
    beat.pulse = (beat.pulse - dt * 4.0).max(0.0);

    // Transport.
    playback.elapsed += dt;
    if playback.elapsed >= playback.duration() {
        let mood = playback.advance();
        theme.mood = mood;
    }
}
