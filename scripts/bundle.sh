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
  cp -R "$ASSETS_REPO/models" dist/assets/models
  echo "    bundled 3D pack from $ASSETS_REPO"
else
  echo "    warn: $ASSETS_REPO/models not found — bundling without the 3D pack (procedural worlds)"
fi

du -sh dist
echo "done: $(pwd)/dist — run ./reverie from inside dist/"
