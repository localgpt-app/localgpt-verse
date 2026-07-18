# Reverie

A desktop app that imagines a **3D world for every song** — built with
[Bevy](https://bevyengine.org/). See [`idea.md`](idea.md) for the concept
(a music-reactive world assembled from free 3D assets, driven by on-device
music analysis).

This milestone is **UI-first**: the chrome from the Reverie design spec is
implemented on Bevy UI over a placeholder, mood-tinted 3D world. The music
analysis + asset-assembly pipeline comes later; for now the transport and beat
are simulated (`src/playback.rs`) so the HUD is already wired to react.

Reverie is a **standalone Cargo project** (it declares its own `[workspace]`),
intentionally excluded from the main `localgpt` workspace build and CI — the
same way the other `apps/` entries are separate from `crates/*`.

## Run

```bash
cd apps/reverie
cargo run
```

Opens a window titled **Reverie**. You start in a three-step onboarding
(photosensitivity → controls → import); click through it, or *Skip setup* /
press **Enter** to jump straight into a world.

**Real playback + analysis (PLAN.md M1–M4):** *Choose your music folder…*
scans a folder (MP3/FLAC/WAV/OGG/M4A/AIFF), replaces the demo queue with your
tracks, and plays them through kira/symphonia — the HUD clock, progress bar,
and queue follow the actual audio, pause audibly holds its breath, and track
ends advance the world. A **live audio tap** (a kira effect) drives the beat
pulse and world glow from the real signal, and a **background analysis pass**
(symphonia decode → realfft) recovers tempo, a beat grid, section boundaries
(the progress notches), an energy curve, and a **mood** (which world) — cached
as a JSON sidecar per track (blake3-keyed, in the app data dir; your music
folder is never written to). *Keep this world* pins the mood into that sidecar.
Without an import (or an audio device) the app falls back to the silent
simulated transport. Dev shortcut: `REVERIE_IMPORT=<dir>` imports at startup.

**ML moods (PLAN.md M5, optional):** build with `--features ml` and run
`scripts/fetch-clap.sh` once (78 MB CLAP audio model, LAION/Xenova ONNX) —
tracks then get a 512-d CLAP embedding (3 windows averaged) and a zero-shot
mood from it, upgrading the rule mapper; the embedding is stored in the
sidecar for future asset selection. Without the feature or the model file the
rule mapper runs. Note the CLAP weights are CC-BY-NC (see PLAN.md §4).

**Asset pack:** 52 CC0 Poly Haven models (~13 per mood) live in the separate
`reverie-assets` repo (`fetch_polyhaven.py` downloads + writes the manifest;
`normalize.py` packs each model to a single uncompressed `.glb` — bevy_gltf
supports neither quantized nor meshopt-compressed geometry — and syncs the
manifest-referenced set into `assets/models/`). Placement rescales each model
to its tier's span (hero 7 m / prop 2.5 m / cover 1 m), culls by
`VisibilityRange`, and arranges per mood: organic spiral (Ember/Tide), city
grid (Velvet Circuit), crystal rings (Glass Expanse). The Credits screen
renders the manifest.

### Controls

| Key | Action |
|-----|--------|
| `W A S D` + mouse | Move / look (Explore mode) |
| `F` / click tabs | Toggle Explore / Drift camera |
| `E` | Send a pulse |
| `N` | Next track |
| `Tab` | Open / close the queue |
| `L` | Open / close the library (pick a world) |
| `Esc` | Pause (world time-dilates) · resume · close the top overlay |
| `H` | Hide the HUD now |
| `P` | Photo mode — hide the chrome and save a shot to `reverie-photos/` |
| `←` / `→` | Adjust world intensity (while paused) |

The HUD follows the spec's "one system, three states": **Visible** while you're
active, fading to a **Minimized** hairline after 4s idle, then **Hidden**
entirely. Any input wakes it. The one variable is the accent, sampled from the
current world's palette.

## What's implemented

- **Design system** (`src/theme.rs`) — veils, hairline, radii, type roles, and
  four world "moods" (Dawn Chorus, Neon Surge, Night Bloom, Glass Runner), each
  contributing the single sampled accent.
- **HUD** (`src/hud.rs`) — now-playing cluster, beat-reactive progress with
  section notches, control hints, Explore/Drift toggle, corner affordances, and
  the three-depth fade behaviour.
- **Overlays** (`src/overlays.rs`) — three-step **onboarding** (photosensitivity
  → controls → import), pause (with world-intensity slider), slide-in queue,
  **Settings** (Comfort group with working toggles), **Credits & Licenses**, and
  a **Library** that shows the world moods as selectable cards (`L`).
- **Comfort** — *Reduce flashing* holds the world glow steady (no beat pulse) and
  caps UI pulses; *Gentler world motion* damps the sway. Both apply instantly.
- **Photo mode** (`P` or the pause button) — clears the chrome and saves a clean
  screenshot of the world to `reverie-photos/`.
- **World props** (`src/world_assets.rs`) — the manifest-driven glTF pack:
  per-mood placement in three tiers (hero/prop/ground cover), span-normalized
  from each model's native size, with `VisibilityRange` LOD, per-mood
  arrangements, and a rise-in materialize that settles on the first downbeat —
  over the same procedural drifting backdrop (`src/world.rs`) with HDR + bloom,
  Explore/Drift cameras, and pause time-dilation.
- **Queue panel** (`Tab`) — now-playing-first view with ↑/↓ reorder buttons;
  tracks carry a content-hash id (blake3) so imports dedupe and reorder never
  restarts the playing track.

Not yet built (follow-ups): the non-Comfort settings groups (display-only for
now); the M7 tier (Beat This!/WFC/LLM/Demucs — see [PLAN.md](PLAN.md) for the
status of each).

## Fonts

The design uses **Marcellus** + **Hanken Grotesk** (both OFL). They aren't
committed yet — drop the `.ttf`s into `assets/fonts/` and Reverie picks them up;
until then it falls back to Bevy's embedded font. See
[`assets/fonts/README.md`](assets/fonts/README.md).

## Smoke test

```bash
REVERIE_SMOKE=1 cargo run                    # boots through every screen, then exits
REVERIE_SMOKE=1 REVERIE_SHOT=/tmp cargo run  # also saves reverie-hud.png / reverie-overlays.png
```

## Perf stress test

```bash
REVERIE_STRESS=5000 cargo run --release      # spawns 5000 prop instances, logs fps, exits after 30s
REVERIE_STRESS=0 cargo run --release         # control run: report only, no extra props
```

Runs uncapped (vsync off) so the numbers show true frame cost. On the dev
machine (2026-07): baseline world ~60 fps, ~1k props ≈ 14 fps, 5k ≈ 9 fps —
dense packs beyond that need real instancing (PLAN.md status).
