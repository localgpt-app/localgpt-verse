//! Offline track analysis — PLAN.md M3 (beats/sections/energy) + M4 (mood).
//!
//! A background worker decodes each track (symphonia), downmixes to low-rate
//! mono, and computes: an onset-novelty curve (spectral flux via realfft), a
//! tempo + beat grid (autocorrelation + phase fit), section boundaries
//! (feature-distance peaks), a per-second energy curve, and a mood via the
//! valence/arousal quadrant mapping from `idea.md`. Results are cached as one
//! JSON sidecar per track, keyed by blake3 content hash (rename/move-proof),
//! in the per-user data dir — the user's music folder is never written to.
//!
//! "Keep this world" (pause overlay) pins a mood into the sidecar; the pin
//! wins over the computed mood whenever the track plays again.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use bevy::prelude::*;
use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

use crate::playback::{Beat, Playback};
use crate::theme::Theme;

/// Analysis sample rate — plenty for onsets/brightness, cheap to decode into.
const ANALYSIS_SR: u32 = 11_025;
/// Cap the analyzed span; longer tracks are judged by their first 8 minutes.
const MAX_ANALYSIS_SECS: f32 = 480.0;
const FFT_SIZE: usize = 1024;
const HOP: usize = 256;

// ---------------------------------------------------------------------------
// Sidecar
// ---------------------------------------------------------------------------

/// One track's analysis, serialized as a JSON sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysis {
    pub version: u32,
    /// Analyzed duration in seconds.
    pub duration: f32,
    /// Beats per minute; `0.0` means no reliable grid was found.
    pub bpm: f32,
    /// First-beat offset in seconds.
    pub beat_offset: f32,
    /// Section boundaries as fractions 0..1 (always starts with 0).
    pub sections: Vec<f32>,
    /// Per-second loudness envelope, normalized 0..1.
    pub energy: Vec<f32>,
    /// Mood index into [`crate::theme::MOODS`] from the quadrant mapping.
    pub mood: usize,
    /// "Keep this world": a pinned mood that overrides `mood` (PLAN.md §5.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_mood: Option<usize>,
    /// The pinned layout seed — with `pinned_mood`, the full world identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_seed: Option<u64>,
}

fn cache_dir() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join("reverie").join("analysis");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Content hash of the file (chunked blake3) — the sidecar filename.
fn cache_key(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(hasher.finalize().to_hex()[..32].to_string())
}

fn sidecar_path(key: &str) -> Option<PathBuf> {
    Some(cache_dir()?.join(format!("{key}.json")))
}

fn load_sidecar(key: &str) -> Option<TrackAnalysis> {
    let text = std::fs::read_to_string(sidecar_path(key)?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_sidecar(key: &str, analysis: &TrackAnalysis) {
    let Some(path) = sidecar_path(key) else {
        return;
    };
    match serde_json::to_string(analysis) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!("Can't write analysis sidecar {}: {e}", path.display());
            }
        }
        Err(e) => warn!("Can't serialize analysis: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Decode (symphonia) → low-rate mono
// ---------------------------------------------------------------------------

/// Decode to mono at ~[`ANALYSIS_SR`] by boxcar decimation. Returns samples
/// and the actual rate.
fn decode_mono(path: &Path) -> Option<(Vec<f32>, u32)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let src_sr = track.codec_params.sample_rate?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let decim = (src_sr / ANALYSIS_SR).max(1) as usize;
    let out_sr = src_sr / decim as u32;
    let max_samples = (MAX_ANALYSIS_SECS * out_sr as f32) as usize;

    let mut out: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut acc = 0.0f32;
    let mut acc_n = 0usize;

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else {
            continue;
        };
        let spec = *decoded.spec();
        let needed = decoded.capacity() as u64;
        let buf = match &mut sample_buf {
            Some(b) if b.capacity() >= decoded.capacity() * spec.channels.count() => b,
            _ => sample_buf.insert(SampleBuffer::new(needed, spec)),
        };
        buf.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        for frame in buf.samples().chunks_exact(channels) {
            let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
            acc += mono;
            acc_n += 1;
            if acc_n == decim {
                out.push(acc / decim as f32);
                acc = 0.0;
                acc_n = 0;
                if out.len() >= max_samples {
                    return Some((out, out_sr));
                }
            }
        }
    }
    if out.len() < out_sr as usize {
        return None; // under a second of audio — not worth analyzing
    }
    Some((out, out_sr))
}

// ---------------------------------------------------------------------------
// DSP
// ---------------------------------------------------------------------------

/// Per-frame spectral features from a hop-by-hop FFT sweep.
struct SpectralSweep {
    /// Positive spectral flux per frame (onset novelty).
    novelty: Vec<f32>,
    /// Spectral centroid per frame, in Hz.
    centroid: Vec<f32>,
    /// Frames per second.
    rate: f32,
}

fn spectral_sweep(samples: &[f32], sr: u32) -> SpectralSweep {
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| {
            let x = i as f32 / (FFT_SIZE - 1) as f32;
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * x).cos() // Hann
        })
        .collect();

    let bins = FFT_SIZE / 2 + 1;
    let hz_per_bin = sr as f32 / FFT_SIZE as f32;
    let mut prev_mag = vec![0.0f32; bins];
    let mut novelty = Vec::new();
    let mut centroid = Vec::new();

    let mut start = 0;
    while start + FFT_SIZE <= samples.len() {
        for (i, s) in input.iter_mut().enumerate() {
            *s = samples[start + i] * window[i];
        }
        if fft.process(&mut input, &mut spectrum).is_err() {
            break;
        }
        let mut flux = 0.0f32;
        let mut num = 0.0f32;
        let mut den = 0.0f32;
        for (i, c) in spectrum.iter().enumerate() {
            let mag = c.norm();
            flux += (mag - prev_mag[i]).max(0.0);
            num += i as f32 * hz_per_bin * mag;
            den += mag;
            prev_mag[i] = mag;
        }
        novelty.push(flux);
        centroid.push(if den > 1e-6 { num / den } else { 0.0 });
        start += HOP;
    }

    SpectralSweep {
        novelty,
        centroid,
        rate: sr as f32 / HOP as f32,
    }
}

/// Tempo from the autocorrelation of the novelty curve, with a mild prior
/// toward 90–140 BPM. Returns `(bpm, lag_frames)`, or `None` if the signal
/// has no periodicity worth trusting.
fn estimate_tempo(novelty: &[f32], rate: f32) -> Option<(f32, usize)> {
    if novelty.len() < (rate * 8.0) as usize {
        return None; // need ~8s of signal
    }
    let mean = novelty.iter().sum::<f32>() / novelty.len() as f32;
    let sig: Vec<f32> = novelty.iter().map(|v| (v - mean).max(0.0)).collect();

    let min_lag = (rate * 60.0 / 180.0) as usize; // 180 BPM
    let max_lag = ((rate * 60.0 / 55.0) as usize).min(sig.len() / 2); // 55 BPM
    if max_lag <= min_lag + 1 {
        return None;
    }
    // Prior-weighted autocorrelation across the whole lag range (prior is a
    // log-normal centred near 115 BPM, damping octave errors).
    let scores: Vec<f32> = (min_lag..=max_lag)
        .map(|lag| {
            let mut s = 0.0f32;
            for i in 0..sig.len() - lag {
                s += sig[i] * sig[i + lag];
            }
            s /= (sig.len() - lag) as f32;
            let bpm = 60.0 * rate / lag as f32;
            let prior = (-((bpm / 115.0).ln().powi(2)) / 0.45).exp();
            s * prior
        })
        .collect();

    let (peak_i, &peak) = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())?;
    if peak <= 1e-9 {
        return None;
    }
    let lag = min_lag + peak_i;
    // Parabolic interpolation of the peak → sub-frame lag precision, so e.g.
    // a true 120 BPM (21.53 frames) isn't quantized to the 117.5/123 the
    // integer lags allow.
    let frac = if peak_i > 0 && peak_i + 1 < scores.len() {
        let (l, r) = (scores[peak_i - 1], scores[peak_i + 1]);
        let denom = l - 2.0 * peak + r;
        if denom.abs() > 1e-12 {
            (0.5 * (l - r) / denom).clamp(-0.5, 0.5)
        } else {
            0.0
        }
    } else {
        0.0
    };
    // Integer lag drives the beat-offset comb; the refined lag gives the bpm.
    Some((60.0 * rate / (lag as f32 + frac), lag))
}

/// Best beat phase for a fixed lag: the offset whose comb of grid points
/// collects the most novelty. Returns seconds.
fn estimate_beat_offset(novelty: &[f32], lag: usize, rate: f32) -> f32 {
    let mut best_phase = 0usize;
    let mut best_score = f32::MIN;
    for phase in 0..lag {
        let mut score = 0.0;
        let mut i = phase;
        while i < novelty.len() {
            score += novelty[i];
            i += lag;
        }
        if score > best_score {
            best_score = score;
            best_phase = phase;
        }
    }
    best_phase as f32 / rate
}

/// Section boundaries as fractions of the duration. Adjacent 4s windows of
/// (novelty, centroid, energy) means; boundaries where the feature distance
/// peaks, ≥15s apart, at most 5 (+ the implicit 0.0).
fn estimate_sections(sweep: &SpectralSweep, samples: &[f32], sr: u32, duration: f32) -> Vec<f32> {
    let win_frames = (4.0 * sweep.rate) as usize;
    if win_frames == 0 || sweep.novelty.len() < win_frames * 3 {
        return vec![0.0];
    }
    let win_samples = 4 * sr as usize;

    // Per-window features, each dimension normalized afterwards.
    let n_windows = sweep.novelty.len() / win_frames;
    let mut feats: Vec<[f32; 3]> = Vec::with_capacity(n_windows);
    for w in 0..n_windows {
        let f0 = w * win_frames;
        let nov: f32 = sweep.novelty[f0..f0 + win_frames].iter().sum::<f32>() / win_frames as f32;
        let cen: f32 = sweep.centroid[f0..f0 + win_frames].iter().sum::<f32>() / win_frames as f32;
        let s0 = (w * win_samples).min(samples.len());
        let s1 = ((w + 1) * win_samples).min(samples.len());
        let rms = if s1 > s0 {
            (samples[s0..s1].iter().map(|s| s * s).sum::<f32>() / (s1 - s0) as f32).sqrt()
        } else {
            0.0
        };
        feats.push([nov, cen, rms]);
    }
    for dim in 0..3 {
        let max = feats.iter().map(|f| f[dim]).fold(1e-9f32, f32::max);
        for f in &mut feats {
            f[dim] /= max;
        }
    }

    let scores: Vec<f32> = feats
        .windows(2)
        .map(|p| {
            let d = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .collect();

    let min_gap_windows = 4; // 15s+ apart (windows are 4s)
    let picked = pick_peaks(&scores, min_gap_windows, 5);

    let mut sections = vec![0.0f32];
    for idx in picked {
        // Boundary between window idx and idx+1.
        let t = ((idx + 1) * win_frames) as f32 / sweep.rate;
        let frac = (t / duration.max(1.0)).clamp(0.0, 0.99);
        sections.push(frac);
    }
    sections.sort_by(|a, b| a.partial_cmp(b).unwrap());
    sections.dedup_by(|a, b| (*a - *b).abs() < 0.02);
    sections
}

/// Indices of up to `count` peaks above mean+0.5σ, greedily by height with a
/// minimum index separation.
fn pick_peaks(scores: &[f32], min_gap: usize, count: usize) -> Vec<usize> {
    if scores.is_empty() {
        return Vec::new();
    }
    let mean = scores.iter().sum::<f32>() / scores.len() as f32;
    let var = scores.iter().map(|s| (s - mean) * (s - mean)).sum::<f32>() / scores.len() as f32;
    let threshold = mean + 0.5 * var.sqrt();

    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap());

    let mut picked: Vec<usize> = Vec::new();
    for idx in order {
        if scores[idx] < threshold || picked.len() >= count {
            break;
        }
        if picked.iter().all(|&p| p.abs_diff(idx) >= min_gap) {
            picked.push(idx);
        }
    }
    picked.sort_unstable();
    picked
}

/// Per-second RMS loudness normalized to 0..1 by the 95th percentile.
fn energy_curve(samples: &[f32], sr: u32) -> Vec<f32> {
    let sec = sr as usize;
    if sec == 0 {
        return Vec::new();
    }
    let mut curve: Vec<f32> = samples
        .chunks(sec)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len().max(1) as f32).sqrt())
        .collect();
    let mut sorted = curve.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = sorted[(sorted.len() as f32 * 0.95) as usize % sorted.len()].max(1e-6);
    for v in &mut curve {
        *v = (*v / p95).clamp(0.0, 1.0);
    }
    curve
}

/// The M4 quadrant mapping: arousal (tempo + loudness) × brightness
/// (spectral centroid) → one of the four moods. Documented in PLAN.md.
fn map_mood(bpm: f32, mean_energy: f32, mean_centroid_hz: f32) -> usize {
    let tempo_norm = ((bpm - 70.0) / 90.0).clamp(0.0, 1.0); // 70..160 BPM
    let arousal = 0.6 * tempo_norm + 0.4 * mean_energy;
    let bright = (mean_centroid_hz / 2500.0).clamp(0.0, 1.0);
    match (arousal > 0.55, bright > 0.45) {
        (true, true) => 1,   // Velvet Circuit — driving, bright/neon
        (true, false) => 0,  // Ember Flats — driving, warm/dark
        (false, true) => 3,  // Glass Expanse — calm, bright/icy
        (false, false) => 2, // Tide Gardens — calm, deep/dark
    }
}

/// Full pipeline for one file.
fn analyze(path: &Path) -> Option<TrackAnalysis> {
    let (samples, sr) = decode_mono(path)?;
    let duration = samples.len() as f32 / sr as f32;
    let sweep = spectral_sweep(&samples, sr);

    let (bpm, beat_offset) = match estimate_tempo(&sweep.novelty, sweep.rate) {
        Some((bpm, lag)) => (bpm, estimate_beat_offset(&sweep.novelty, lag, sweep.rate)),
        None => (0.0, 0.0),
    };
    let sections = estimate_sections(&sweep, &samples, sr, duration);
    let energy = energy_curve(&samples, sr);
    let mean_energy = energy.iter().sum::<f32>() / energy.len().max(1) as f32;
    let mean_centroid = sweep.centroid.iter().sum::<f32>() / sweep.centroid.len().max(1) as f32;
    let mood = map_mood(bpm, mean_energy, mean_centroid);

    Some(TrackAnalysis {
        version: 1,
        duration,
        bpm,
        beat_offset,
        sections,
        energy,
        mood,
        pinned_mood: None,
        pinned_seed: None,
    })
}

// ---------------------------------------------------------------------------
// Worker + Bevy plumbing
// ---------------------------------------------------------------------------

type WorkerResult = (PathBuf, String, TrackAnalysis);

/// Analysis results by path, plus the worker channels.
#[derive(Resource)]
pub struct AnalysisStore {
    map: HashMap<PathBuf, TrackAnalysis>,
    /// Cache keys for sidecar writes (pinning).
    keys: HashMap<PathBuf, String>,
    pending: HashSet<PathBuf>,
    tx: Sender<PathBuf>,
    rx: Mutex<Receiver<WorkerResult>>,
}

impl Default for AnalysisStore {
    fn default() -> Self {
        let (req_tx, req_rx) = channel::<PathBuf>();
        let (res_tx, res_rx) = channel::<WorkerResult>();
        std::thread::spawn(move || {
            for path in req_rx {
                let Some(key) = cache_key(&path) else {
                    continue;
                };
                let analysis = match load_sidecar(&key) {
                    Some(a) => a,
                    None => {
                        let started = std::time::Instant::now();
                        let Some(a) = analyze(&path) else {
                            warn!("Analysis failed for {}", path.display());
                            continue;
                        };
                        info!(
                            "Analyzed {} — {:.0} BPM, {} sections ({:.1}s)",
                            path.display(),
                            a.bpm,
                            a.sections.len(),
                            started.elapsed().as_secs_f32()
                        );
                        save_sidecar(&key, &a);
                        a
                    }
                };
                if res_tx.send((path, key, analysis)).is_err() {
                    return;
                }
            }
        });
        Self {
            map: HashMap::new(),
            keys: HashMap::new(),
            pending: HashSet::new(),
            tx: req_tx,
            rx: Mutex::new(res_rx),
        }
    }
}

impl AnalysisStore {
    /// First-beat offset for a track, when analyzed and a grid was found.
    /// Used to land the materialize sequence on the first downbeat.
    pub fn beat_offset_for(&self, path: &Path) -> Option<f32> {
        self.map
            .get(path)
            .filter(|a| a.bpm > 0.0)
            .map(|a| a.beat_offset)
    }

    fn request(&mut self, path: &Path) {
        if self.map.contains_key(path) || self.pending.contains(path) {
            return;
        }
        self.pending.insert(path.to_path_buf());
        let _ = self.tx.send(path.to_path_buf());
    }

    /// Toggle the "Keep this world" pin for `path` at `mood`. Returns the new
    /// pin state, or `None` when the track has no analysis yet.
    pub fn toggle_pin(&mut self, path: &Path, mood: usize, seed: u64) -> Option<bool> {
        let analysis = self.map.get_mut(path)?;
        let pinned = if analysis.pinned_mood.is_some() {
            analysis.pinned_mood = None;
            analysis.pinned_seed = None;
            false
        } else {
            analysis.pinned_mood = Some(mood);
            analysis.pinned_seed = Some(seed);
            true
        };
        if let Some(key) = self.keys.get(path) {
            save_sidecar(key, analysis);
        }
        Some(pinned)
    }
}

/// Request analysysis for the current + next track, drain worker results, and
/// apply the current track's analysis to the transport/beat/theme exactly
/// once per (track, availability) state.
#[allow(clippy::type_complexity)]
pub fn sync_analysis(
    mut store: ResMut<AnalysisStore>,
    mut playback: ResMut<Playback>,
    mut beat: ResMut<Beat>,
    mut theme: ResMut<Theme>,
    mut layout: ResMut<crate::world_assets::WorldLayout>,
    mut applied: Local<Option<(Option<PathBuf>, bool)>>,
) {
    if playback.queue.is_empty() {
        return;
    }

    // Keep the current and next tracks in flight ("the next world is
    // prepared quietly" — queue panel).
    let idx = playback.current % playback.queue.len();
    let paths: Vec<PathBuf> = [idx, (idx + 1) % playback.queue.len()]
        .iter()
        .filter_map(|&i| playback.queue[i].path.clone())
        .collect();
    for p in paths {
        store.request(&p);
    }

    // Drain results.
    let mut arrived = Vec::new();
    {
        let rx = store.rx.lock().unwrap();
        while let Ok(r) = rx.try_recv() {
            arrived.push(r);
        }
    }
    for (path, key, analysis) in arrived {
        store.pending.remove(&path);
        store.keys.insert(path.clone(), key);
        store.map.insert(path, analysis);
    }

    // Apply to the live signals when the current track (keyed by path, so a
    // queue replacement on import is caught) or its analysis availability
    // changes.
    let current_path = playback.queue[idx].path.clone();
    let has = current_path
        .as_ref()
        .is_some_and(|p| store.map.contains_key(p));
    if applied.as_ref() == Some(&(current_path.clone(), has)) {
        return;
    }
    *applied = Some((current_path.clone(), has));

    // Per-track layout seed: the pin wins, else a deterministic default from
    // the path (same song → same place until re-rolled).
    match current_path.as_ref().and_then(|p| store.map.get(p)) {
        Some(a) => {
            playback.sections = a.sections.clone();
            if a.bpm > 0.0 {
                beat.bpm = a.bpm;
                beat.offset = a.beat_offset;
                beat.grid = true;
            } else {
                beat.grid = false;
            }
            let mood = a.pinned_mood.unwrap_or(a.mood);
            layout.seed = a
                .pinned_seed
                .unwrap_or_else(|| crate::world_assets::path_seed(current_path.as_ref().unwrap()));
            playback.queue[idx].mood = mood;
            theme.mood = mood;
        }
        None => {
            // Demo track (no path) keeps its authored mock sections; a real
            // file with analysis still pending shows a clean bar until it lands.
            if let Some(path) = &current_path {
                playback.sections.clear();
                layout.seed = crate::world_assets::path_seed(path);
            }
            beat.grid = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic novelty with impulses every `period` frames.
    fn click_novelty(period: usize, phase: usize, len: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; len];
        let mut i = phase;
        while i < len {
            v[i] = 1.0;
            i += period;
        }
        v
    }

    #[test]
    fn tempo_recovers_120_bpm() {
        // 43.066 frames/sec (11025/256); 120 BPM → beat every 0.5s ≈ 21.5 fr.
        let rate = ANALYSIS_SR as f32 / HOP as f32;
        let period = (rate * 0.5).round() as usize;
        let novelty = click_novelty(period, 3, (rate * 60.0) as usize);
        let (bpm, _lag) = estimate_tempo(&novelty, rate).expect("tempo found");
        assert!((bpm - 120.0).abs() < 3.0, "got {bpm}");
    }

    #[test]
    fn beat_offset_recovers_phase() {
        let rate = ANALYSIS_SR as f32 / HOP as f32;
        let period = (rate * 0.5).round() as usize;
        let phase = 10usize;
        let novelty = click_novelty(period, phase, (rate * 30.0) as usize);
        let offset = estimate_beat_offset(&novelty, period, rate);
        assert!((offset - phase as f32 / rate).abs() < 0.02, "got {offset}");
    }

    #[test]
    fn peaks_respect_gap_and_count() {
        let scores = vec![0.0, 0.0, 5.0, 0.0, 0.0, 8.0, 0.0, 0.1];
        let picked = pick_peaks(&scores, 2, 2);
        assert_eq!(picked, vec![2, 5]);
    }

    #[test]
    fn mood_quadrants() {
        assert_eq!(map_mood(150.0, 0.8, 4000.0), 1); // driving + bright
        assert_eq!(map_mood(150.0, 0.8, 500.0), 0); //  driving + dark
        assert_eq!(map_mood(70.0, 0.1, 4000.0), 3); //  calm + bright
        assert_eq!(map_mood(70.0, 0.1, 500.0), 2); //   calm + dark
    }

    #[test]
    fn energy_curve_normalized() {
        let sr = 100u32;
        let mut samples = vec![0.1f32; 500];
        samples.extend(vec![0.9f32; 500]);
        let curve = energy_curve(&samples, sr);
        assert_eq!(curve.len(), 10);
        assert!(curve.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(curve[9] > curve[0]);
    }
}
