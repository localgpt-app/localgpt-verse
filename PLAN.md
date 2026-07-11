# Reverie — Backend Plan: Libraries & Implementation Steps

The UI-first milestone is done: every screen from the design spec runs over a
placeholder world, driven by a **simulated** transport and beat
(`src/playback.rs`). This plan chooses the libraries and sequences the work to
replace the simulation with the real thing described in `idea.md`:

> decode local music → analyze it (beats, sections, energy, mood) → imagine a
> world per song from a catalog of free 3D assets → react live, comfortably.

## 0. Constraints that drive every choice

1. **The seam is already built.** The UI reads two resources —
   `Playback { queue, current, elapsed, sections, playing }` and
   `Beat { bpm, phase, pulse, energy }` — and the theme system takes a mood
   index. The backend's only job is to feed these with real values. Nothing in
   `hud.rs` / `overlays.rs` / `theme.rs` should need to change.
2. **License: Apache-2.0 app.** No GPL dependencies (rules out `aubio`,
   `bliss-audio`, essentia). MPL-2.0 (file-level copyleft, e.g. Symphonia) is
   fine. Shipped *assets* must be CC0-first, CC-BY with attribution — the
   Credits screen already renders per-asset attribution; the manifest (§3)
   becomes its data source.
3. **Design-spec timing is a hard requirement**, not polish:
   audio starts at 0s; geometry settles **on the first downbeat**; song
   crossfades run 6–10s and land **on a section boundary** of the incoming
   track; progress notches **are** the section boundaries; every effect obeys
   the Comfort gates (reduce-flashing caps pulses at 2 Hz, gentler-motion
   swaps movement for opacity). This forces a **sample-accurate playback
   clock** and an **ahead-of-playback analysis pass**.
4. **On-device only.** "Nothing is uploaded, ever" is in the onboarding copy.
   All analysis and ML runs locally.
5. **Format promise:** onboarding says `MP3 · FLAC · WAV · OGG · AIFF`.
6. **Monorepo consistency:** gen already ships cpal audio; localgpt-core
   already ships ONNX inference (fastembed/ort). Prefer the same foundations.

## 1. Library choices

### 1.1 Playback & audio I/O

| Concern | Choice | Why | License |
|---|---|---|---|
| Decode | **symphonia** | Pure Rust; MP3/FLAC/WAV/OGG(Vorbis)/AAC/ALAC, AIFF in recent releases (verify the `aiff`/riff feature at pin time). Also what kira uses internally. | MPL-2.0 |
| Playback engine | **kira** (directly, *not* bevy_kira_audio) | Game-audio mixer on cpal with the three features the spec demands for free: **precise clocks** (schedule a start/crossfade on a musical boundary), **tweens** (6–10s equal-power crossfades, pause "time-dilation" as a tween on a track volume/rate), and **custom `Effect`s** (our FFT tap inside the audio thread). Symphonia loading built in. Engine-agnostic → immune to Bevy version lag. | MIT/Apache-2.0 |
| Device output | cpal (transitively via kira) | Already proven in `crates/gen` on this repo's targets. | Apache-2.0 |
| Tags/metadata | **lofty** | Title/artist/duration for the queue + library rows. | MIT/Apache-2.0 |
| Folder import | **rfd** (native folder picker) + **walkdir** | "Choose your music folder…" opens the OS picker (spec 1j). rfd must run on the main thread on macOS — call it from a Bevy system, or use `AsyncFileDialog`. | MIT |
| Loudness | **ebur128** | EBU R128 integrated loudness + short-term envelope: volume normalization across the queue and a robust energy curve. | MIT |

**Rejected:** `rodio`/`bevy_audio` (no clocks/scheduling — can't land a
crossfade on a beat), `bevy_kira_audio` (tracks Bevy releases; we'd be blocked
on 0.19/0.20 lag — we own the integration seam anyway), hand-rolled
cpal mixer (gen's pattern, but ~1–2k lines of decode/ring-buffer/seek/
crossfade code that kira provides; keep as fallback if kira's position
granularity disappoints), `aubio`/`bliss-audio` (GPL).

### 1.2 Analysis (MIR)

Two paths, both local:

| Concern | v1 (pure Rust, in-house) | Upgrade (ONNX via ort) |
|---|---|---|
| FFT | **realfft**/rustfft (MIT/Apache) | — |
| Resample to analysis rate | **rubato** (MIT) | — |
| Onsets & live pulse | Spectral-flux novelty (per-band, ~30 lines on realfft) | — |
| Tempo + beat grid | Autocorrelation tempo + Ellis dynamic-programming beat tracking over the onset envelope (well-documented, ~200 lines, good on pop/electronic) | **Beat This!** exported to ONNX (SOTA; CPJKU code is MIT — verify weights terms) |
| Sections (the notches) | Foote novelty on a self-similarity matrix of chroma/mel features; peaks = boundaries | (same, better features from CLAP embeddings) |
| Key/mode (valence input) | Chroma + Krumhansl-Schmuckler correlation | — |
| Mood / semantic tags | Rule proxies: tempo, mode, loudness dynamics, spectral centroid → valence/arousal (the color-emotion mapping in `idea.md`) | **CLAP** (LAION, ONNX from HF, e.g. `Xenova/larger_clap_music_and_speech`): zero-shot mood tags **and** the audio↔text embedding reused for asset selection — one model, two jobs. Verify checkpoint license before shipping (idea.md caveat). |
| ML runtime | — | **ort** (Apache/MIT) — same runtime family localgpt-core already ships via fastembed. Feature-gate: `ml`. |
| Stems (Demucs) | **Deferred.** Per-band energy (bass/mid/high from the live FFT) approximates stem reactivity at ~zero cost | Revisit post-M6; htdemucs ONNX export is awkward and heavy |

Cache keying: **blake3** content hash (CC0/Apache-2.0). Cache dir via
**dirs** (`~/Library/Application Support/reverie` / XDG equivalent).
Analysis artifacts serialize with serde to one JSON/RON sidecar per track.

### 1.3 Worlds & assets

| Concern | Choice | Why | License |
|---|---|---|---|
| Runtime loading | `bevy_gltf` (built-in `SceneRoot` + `GltfAssetLabel`) | No new dep. **No Draco/meshopt at runtime** — normalize offline (idea.md §2). | — |
| Offline normalization | `gltf-transform` / `gltfpack` CLIs, wrapped in a repo `xtask` (not shipped) | Decompress, quantize, normalize to `.glb`, emit the manifest | tooling only |
| Asset manifest | serde JSON: `source, author, license, url, date, blake3, tags, embedding` | Legal audit trail (idea.md Stage 0) **and** the live data source for the Credits screen (replaces its hardcoded rows) | — |
| Scatter/placement | **noise** (already in gen) + golden-angle/jittered-grid scatter we already have; **fast_poisson** when overlap matters | Blockout-first: hero / medium / ground-cover tiers | MIT/Apache |
| LOD / perf | `VisibilityRange` (built-in), GPU-driven rendering (automatic ≥0.16) | 60 fps @ ~5k instances on a mid GPU is the M6 budget | — |
| Structured layouts | **wfc** (gridbugs) | Only at M7 — organic scatter first | MIT |
| Scene recipes (LLM) | Rule-based mapper first; later **llama-cpp-2** + GBNF *or* optional Ollama HTTP | Deferred to M7; grammar-constrained JSON recipes per idea.md Stage 3 | MIT |

Asset sources for the starter pack (Stage-0 policy): **CC0 only** — Kenney,
Quaternius, Poly Haven, ambientCG. CC-BY enters later with per-asset
attribution already wired through the manifest → Credits screen.

## 2. Architecture (threads & data flow)

```
                    ┌────────────────────────────────────────────────┐
                    │ Analysis worker (AsyncComputeTaskPool / thread)│
 library scan ────► │ symphonia decode → rubato → realfft            │
 (walkdir+lofty)    │ onsets → tempo/beat grid → sections → key      │
                    │ loudness (ebur128) → [CLAP embed  (ort, `ml`)] │
                    └───────────────┬────────────────────────────────┘
                                    │ TrackAnalysis {beats, sections,
                                    │  energy curve, key, mood, embedding}
                                    ▼ (blake3-keyed sidecar cache)
 Bevy main ◄── Playback/Beat/Theme resources ◄── transport systems
     │                                              ▲
     ▼                                              │ position(), clock ticks
 kira AudioManager ── track: song A ──┐             │
     (audio thread)   track: song B ──┼── FFT-tap Effect (live bands,
                      clocks, tweens ─┘    lock-free → Beat.pulse/energy)
```

- **Clock truth** lives in the audio thread (kira clock + `position()`);
  `Playback.elapsed` becomes a read-model of it. `Beat.phase` = position
  mapped onto the precomputed beat grid; `Beat.pulse` spikes from the grid
  (predictive, tight) with the live onset tap as fallback for unanalyzed
  tracks.
- **Materialize:** world build is keyframed against the clock so geometry
  "settles" exactly on beat[0] ≥ the 1.3–2.4s terrain window.
- **Crossfade:** when A nears its end, B is pre-decoded and scheduled on a
  clock tick that coincides with B's nearest section boundary; equal-power
  tween 6–10s. Close moods (|Δvalence| < 0.2) morph palette in place instead.
- **Comfort gates stay central:** every new signal passes through the existing
  `Comfort` checks (the 2 Hz cap and gentler-motion damping already exist).

## 3. Implementation steps (each lands green: build + clippy + fmt + tests + smoke)

**M1 — Real playback core.**
Add kira/symphonia/lofty/rfd/walkdir/dirs. Folder import fills `Playback.queue`
from disk (tags via lofty); play/pause/seek/skip drive kira; `elapsed` reads
the kira clock; pause overlay tweens volume (time-dilation you can hear).
*Exit:* onboarding folder-pick → your actual music audibly plays; HUD time,
progress, and queue are real. Beat stays simulated.

**M2 — Live tap.**
Custom kira `Effect`: windowed FFT → per-band energy + spectral flux, pushed
lock-free into `Beat.pulse`/`energy`. Delete the sine-wave energy simulator.
*Exit:* playhead pulse and world glow visibly follow the actual song;
reduce-flashing still clamps to steady.

**M3 — Pre-analysis pass + musical timing.**
Background worker computes `TrackAnalysis` (onsets → tempo → DP beat grid,
Foote sections, key, loudness curve), cached by blake3. Wire: progress notches
= real sections; `Beat.bpm/phase` from the grid; materialize settles on the
first downbeat; crossfades land on section boundaries; next track analyzed
"quietly" during the current one (the queue already promises this).
*Exit:* notches match audible structure on a test set; skip lands a 400ms
smear into a boundary-aligned entry. Unit tests on the DSP (synthetic clicks →
recovered BPM ±2%; boundary detection on constructed signals).

**M4 — Mood → world mapping (rule-based).**
Valence/arousal from key mode, tempo, dynamics, centroid → pick among the four
moods + continuous params (density, motion speed, glow ceiling) through the
existing `Theme`/`WorldIntensity` plumbing. "Build a different world" re-rolls
the seed, keeps the mood.
*Exit:* an aggressive track and a calm track land in audibly-fitting worlds
deterministically (same file → same world).

**M5 — CLAP embeddings (`ml` feature).**
ort + CLAP ONNX: zero-shot mood tags + stored 512-d embedding per track.
Mapping upgrades from rules to nearest-neighbor over tagged palettes; the
embedding is the hook M6 reuses for asset selection.
*Exit:* blind A/B where CLAP mood beats the rule mapper on a small panel;
graceful fallback to M4 rules when the feature/model is absent.

**M6 — Asset pipeline v1.**
`xtask normalize-assets`: CC0 starter pack (~50–100 Kenney/Quaternius/Poly
Haven models) → normalized `.glb` + manifest. Runtime: per-mood asset sets,
hero/medium/scatter placement (poisson + noise), `VisibilityRange` LOD;
drifting primitives become real props. Credits screen reads the manifest.
*Exit:* 60 fps with ~5k instances on a mid GPU; Credits shows real
attribution; `assets/` passes a `cargo deny`-style license audit.

**M7 — Deferred tier.**
Beat This! ONNX upgrade; WFC structured layouts; grammar-constrained LLM scene
recipes (llama-cpp-2 or optional Ollama); Demucs stems. Each behind a feature
flag, none load-bearing.

**Cross-cutting from M1:** add a reverie job to CI (build/clippy/fmt/test,
`REVERIE_SMOKE` needs a GPU—keep it local-only); `cargo deny` for license
enforcement; keep `default` features light (`ml` opt-in — ort binaries are
large).

## 4. Risks & mitigations

- **kira position/scheduling granularity** for "settle on the downbeat" —
  prototype in M1; fallback is gen-style custom cpal mixer (pattern already in
  `crates/gen`).
- **AIFF coverage** in kira's symphonia features — if gapped, decode AIFF
  ourselves into a kira static sound (raw frames), or trim the onboarding
  format line.
- **In-house beat tracker quality** on rubato/ambient material — acceptable
  for v1 (worlds tolerate imprecision; spec: "the void just keeps listening"),
  upgrade path is M7 Beat This!.
- **ort binary size / build friction** — feature-gated; rules-only build stays
  pure Rust.
- **CLAP checkpoint license** — verify before shipping weights (idea.md
  caveat); app must run without them.
- **Bevy 0.20 churn** — pin 0.19; migration notes live in the project memory.

## 5. Open questions (need a product call, not research)

1. Ship the CC0 starter pack in-repo (repo weight) or as a first-run download
   (contradicts "nothing is uploaded" *tone*, though it's a download)?
   → suggests a separate `reverie-assets` repo, matching the monorepo's
   `*-assets` convention.
2. Library persistence: JSON sidecars are fine through M5; adopt rusqlite
   (monorepo standard) when the library view needs search/sort at scale?
3. Does "Keep this world" (pause overlay) pin mood-per-track in the analysis
   sidecar (my assumption: yes, it's one field)?
