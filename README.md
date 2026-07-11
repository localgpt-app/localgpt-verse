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

Opens a window titled **Reverie**. You start on the first-run screen; press
**Enter** (or click *Choose your music folder…*) to enter a world.

### Controls

| Key | Action |
|-----|--------|
| `W A S D` + mouse | Move / look (Explore mode) |
| `F` | Toggle Explore / Drift camera |
| `E` | Send a pulse |
| `Tab` | Open / close the queue |
| `Esc` | Pause (world time-dilates) · resume |
| `H` | Hide the HUD now |
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
- **Overlays** (`src/overlays.rs`) — first-run import, pause (with world-intensity
  slider), slide-in queue, **Settings** (Comfort group with working toggles), and
  **Credits & Licenses**.
- **Comfort** — *Reduce flashing* holds the world glow steady (no beat pulse) and
  caps UI pulses; *Gentler world motion* damps the sway. Both apply instantly.
- **Placeholder world** (`src/world.rs`) — a mood-tinted field of drifting shapes
  with HDR + bloom, Explore/Drift cameras, and pause time-dilation, so the HUD
  always overlays a live world.

Not yet built (follow-ups): the full 3-step onboarding, the Library/Home screen,
and the non-Comfort settings groups (display-only for now); real audio + MIR;
asset assembly.

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
