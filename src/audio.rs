//! Real audio playback — PLAN.md M1.
//!
//! kira (mixer/clocks/tweens on cpal) + symphonia (decode) feed the same
//! `Playback` resource the UI already reads; the simulated transport in
//! `playback.rs` stays as the fallback. Degrades gracefully twice over:
//! no audio device → silent world, simulated clock; a track with no file
//! (the built-in demo queue) → same.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use bevy::prelude::*;
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::sound::{FromFileError, PlaybackState};
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, Tween};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;

use crate::playback::{Playback, Track};
use crate::theme::Theme;
use crate::{AudioActive, Paused};

/// A tween with the given duration (start immediately, linear).
fn tween_ms(ms: u64) -> Tween {
    Tween {
        duration: Duration::from_millis(ms),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

struct AudioInner {
    manager: AudioManager<DefaultBackend>,
    handle: Option<StreamingSoundHandle<FromFileError>>,
    /// Queue index + path the current handle was started for.
    playing: Option<(usize, PathBuf)>,
    /// One-time "clock live" log for smoke verification.
    clock_logged: bool,
}

/// The audio engine. `Mutex` because `AudioManager` is `Send` but not `Sync`
/// (kira parks the cpal stream on its own thread); locks are held for
/// microseconds a few times per frame.
#[derive(Resource, Default)]
pub struct AudioPlayer(Mutex<Option<AudioInner>>);

/// Create the audio device. On failure the app runs silent (simulated clock).
pub fn init_audio(player: ResMut<AudioPlayer>) {
    let mut guard = player.0.lock().unwrap();
    match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
        Ok(manager) => {
            *guard = Some(AudioInner {
                manager,
                handle: None,
                playing: None,
                clock_logged: false,
            });
            info!("Audio device ready");
        }
        Err(e) => warn!("No audio device — running silent ({e})"),
    }
}

/// Start/stop streams so the playing sound always matches `Playback.current`.
pub fn sync_track_playback(
    player: Res<AudioPlayer>,
    paused: Res<Paused>,
    mut playback: ResMut<Playback>,
    mut audio_active: ResMut<AudioActive>,
) {
    let mut guard = player.0.lock().unwrap();
    let Some(inner) = guard.as_mut() else {
        audio_active.0 = false;
        return;
    };
    if playback.queue.is_empty() {
        audio_active.0 = false;
        return;
    }

    let idx = playback.current % playback.queue.len();
    let want = playback.queue[idx].path.clone().map(|p| (idx, p));
    let switching = inner.playing != want;

    if switching {
        if let Some(handle) = &mut inner.handle {
            // Skip smear — a quick fade instead of a hard cut (spec 1r).
            handle.stop(tween_ms(300));
        }
        inner.handle = None;
        inner.playing = want.clone();

        if let Some((i, path)) = &want {
            match StreamingSoundData::from_file(path) {
                Ok(sound) => {
                    // Trust the decoder's duration over the tag header.
                    let duration = sound.duration().as_secs_f32();
                    if duration > 1.0 {
                        playback.queue[*i].duration = duration;
                    }
                    match inner.manager.play(sound) {
                        Ok(mut handle) => {
                            // A track can start while paused (pause → skip):
                            // hold it silently until resume.
                            if paused.0 {
                                handle.pause(tween_ms(0));
                            }
                            let t = &playback.queue[*i];
                            info!("Now playing: {} — {}", t.title, t.artist);
                            inner.handle = Some(handle);
                        }
                        Err(e) => warn!("Can't play {}: {e}", path.display()),
                    }
                }
                Err(e) => warn!("Can't decode {}: {e}", path.display()),
            }
        }
    }
    audio_active.0 = inner.handle.is_some();
}

/// Pause/resume with the spec's time-dilation tweens (220ms in / 320ms out).
///
/// Acts only on actual value flips (`Local` tracks the last state): re-issuing
/// `pause()` restarts its fade tween, so a writer that sets `Paused` every
/// frame would otherwise keep the sound in a never-finishing fade.
pub fn sync_pause(player: Res<AudioPlayer>, paused: Res<Paused>, mut last: Local<Option<bool>>) {
    if *last == Some(paused.0) {
        return;
    }
    let was = last.replace(paused.0);
    if was.is_none() && !paused.0 {
        return; // startup default — nothing to do
    }
    let mut guard = player.0.lock().unwrap();
    let Some(inner) = guard.as_mut() else { return };
    let Some(handle) = &mut inner.handle else {
        return;
    };
    if paused.0 {
        handle.pause(tween_ms(220));
    } else {
        handle.resume(tween_ms(320));
    }
}

/// The audio clock owns `elapsed` while a stream is live, and advances the
/// queue when a track ends naturally.
pub fn sync_clock(
    player: Res<AudioPlayer>,
    mut playback: ResMut<Playback>,
    mut theme: ResMut<Theme>,
) {
    let mut guard = player.0.lock().unwrap();
    let Some(inner) = guard.as_mut() else { return };
    let Some(handle) = &mut inner.handle else {
        return;
    };

    let pos = handle.position() as f32;
    playback.elapsed = pos;
    if !inner.clock_logged && pos > 0.25 {
        inner.clock_logged = true;
        info!("Audio clock live: {pos:.2}s");
    }

    if handle.state() == PlaybackState::Stopped {
        // Natural end of file — mirror the simulated end-of-track path.
        inner.handle = None;
        inner.playing = None;
        let mood = playback.advance();
        theme.mood = mood;
    }
}

// ---------------------------------------------------------------------------
// Library import
// ---------------------------------------------------------------------------

/// Background folder scan feeding tracks into the queue as they're found.
#[derive(Resource, Default)]
pub struct ImportState {
    rx: Option<Mutex<Receiver<Track>>>,
    /// True once imported tracks have replaced the demo queue.
    pub imported_any: bool,
    count: usize,
}

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "oga", "m4a", "aac", "aiff", "aif",
];

/// Spawn a scan thread over `folder`; tracks stream in via `poll_import`.
pub fn start_import(folder: PathBuf, import: &mut ImportState) {
    info!("Importing music from {}", folder.display());
    let (tx, rx) = channel();
    import.rx = Some(Mutex::new(rx));
    import.count = 0;
    std::thread::spawn(move || {
        for entry in walkdir::WalkDir::new(&folder)
            .follow_links(true)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !AUDIO_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
                continue;
            }
            if let Some(track) = read_track(path)
                && tx.send(track).is_err()
            {
                return; // receiver dropped — app shutting down
            }
        }
    });
}

/// Read tags + duration for one file. `None` skips it (unreadable/too short).
fn read_track(path: &Path) -> Option<Track> {
    let tagged = lofty::read_from_path(path).ok()?;
    let duration = tagged.properties().duration().as_secs_f32();
    if duration < 1.0 {
        return None;
    }
    let tag = tagged.primary_tag().or_else(|| tagged.tags().first());
    let title = tag
        .and_then(|t| t.title().map(|s| s.into_owned()))
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into())
        });
    let artist = tag
        .and_then(|t| t.artist().map(|s| s.into_owned()))
        .unwrap_or_else(|| "Unknown Artist".into());
    Some(Track {
        title,
        artist,
        duration,
        mood: path_mood(path),
        section: "Your library".into(),
        path: Some(path.to_path_buf()),
    })
}

/// Deterministic mood per file until the real mapper lands (PLAN.md M4):
/// same file → same world.
pub(crate) fn path_mood(path: &Path) -> usize {
    let bytes = path.as_os_str().as_encoded_bytes();
    let hash = bytes.iter().fold(0usize, |acc, &b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    });
    hash % crate::theme::MOODS.len()
}

/// Drain the scan channel into the queue. The first batch replaces the demo
/// queue and restarts playback on the user's own music.
pub fn poll_import(
    mut import: ResMut<ImportState>,
    mut playback: ResMut<Playback>,
    mut theme: ResMut<Theme>,
    player: Res<AudioPlayer>,
) {
    let Some(rx_mutex) = &import.rx else { return };

    let mut batch = Vec::new();
    let mut done = false;
    {
        let rx = rx_mutex.lock().unwrap();
        loop {
            match rx.try_recv() {
                Ok(track) => {
                    batch.push(track);
                    if batch.len() >= 64 {
                        break; // bound per-frame work
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
    }

    if !batch.is_empty() {
        if !import.imported_any {
            import.imported_any = true;
            playback.queue.clear();
            playback.current = 0;
            playback.elapsed = 0.0;
            playback.playing = true;
            theme.mood = batch[0].mood;
            // Force the player to restart on the new queue.
            if let Some(inner) = player.0.lock().unwrap().as_mut() {
                if let Some(handle) = &mut inner.handle {
                    handle.stop(tween_ms(200));
                }
                inner.handle = None;
                inner.playing = None;
            }
        }
        import.count += batch.len();
        playback.queue.append(&mut batch);
    }

    if done {
        info!("Library import finished: {} tracks", import.count);
        import.rx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_mood_is_stable_and_in_range() {
        let p = Path::new("/music/artist/song.flac");
        assert_eq!(path_mood(p), path_mood(p));
        for path in ["/a.mp3", "/b.mp3", "/c/d.flac", "/e/f/g.wav"] {
            assert!(path_mood(Path::new(path)) < crate::theme::MOODS.len());
        }
    }

    #[test]
    fn path_mood_varies_across_paths() {
        // Not all paths land on one mood (would make the hash pointless).
        let moods: std::collections::HashSet<usize> = (0..32)
            .map(|i| path_mood(Path::new(&format!("/music/track-{i}.mp3"))))
            .collect();
        assert!(moods.len() > 1);
    }
}
