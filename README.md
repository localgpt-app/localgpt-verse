# Reverie

Placeholder [Bevy](https://bevyengine.org/) application.

Reverie is a **standalone Cargo project** (it declares its own `[workspace]`),
intentionally excluded from the main `localgpt` workspace build and CI — the
same way the other `apps/` entries are separate from `crates/*`.

## Run

```bash
cd apps/reverie
cargo run
```

This opens a window titled **Reverie** with a minimal 3D scene: a ground plane,
a cube, a directional light, and a camera. Replace `src/main.rs` with real
content as the project develops.
