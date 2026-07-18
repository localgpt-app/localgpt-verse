#!/usr/bin/env bash
# Assemble a distributable Reverie bundle (PLAN.md §5.1: assets are bundled at
# ship time from the separate reverie-assets repo — no first-run download).
#
# Usage: scripts/bundle.sh
#   REVERIE_ASSETS=<path>  override the reverie-assets checkout location
#
# Produces apps/reverie/dist/ with the release binary + assets/. The bundle is
# portable: on machines without the build tree, the binary resolves assets
# relative to its working directory (see world_assets::asset_root).
set -euo pipefail
cd "$(dirname "$0")/.."

ASSETS_REPO="${REVERIE_ASSETS:-../../../reverie-assets}"

echo "==> release build"
cargo build --release

echo "==> assembling dist/"
rm -rf dist
mkdir -p dist/assets
cp target/release/reverie dist/
cp -R assets/fonts dist/assets/fonts

if [ -d "$ASSETS_REPO/models" ]; then
  # Ship only the manifest-referenced (normalized .glb) files, not the
  # original .gltf/.bin/texture trees (PLAN.md §1.3 offline normalization).
  python3 "$ASSETS_REPO/normalize.py" --sync "$(pwd)/dist/assets/models"
  echo "    bundled 3D pack from $ASSETS_REPO"
else
  echo "    warn: $ASSETS_REPO/models not found — bundling without the 3D pack (procedural worlds)"
fi

if [ -f "$ASSETS_REPO/music/music.json" ]; then
  # CC0 starter music so the app has something to play on first run.
  mkdir -p dist/assets/music
  cp "$ASSETS_REPO"/music/*.mp3 "$ASSETS_REPO"/music/music.json dist/assets/music/
  [ -f "$ASSETS_REPO/music/NOTICE" ] && cp "$ASSETS_REPO/music/NOTICE" dist/assets/music/
  echo "    bundled $(ls "$ASSETS_REPO"/music/*.mp3 | wc -l | tr -d ' ') CC0 starter tracks"
else
  echo "    warn: $ASSETS_REPO/music not found — shipping without starter music"
fi

du -sh dist
echo "done: $(pwd)/dist — run ./reverie from inside dist/"
