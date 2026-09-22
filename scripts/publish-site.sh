#!/usr/bin/env bash
# Publish the marketing/docs site to Cloudflare Workers as "localgpt-verse"
# (site/wrangler.jsonc — static assets only, nothing to build).
#
# Usage: scripts/publish-site.sh
#   First time on a machine: npx wrangler login
#   CI: export CLOUDFLARE_API_TOKEN instead of logging in.
set -euo pipefail
cd "$(dirname "$0")/../site"

npx --yes wrangler deploy
