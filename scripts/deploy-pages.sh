#!/usr/bin/env bash
# Publish the built frontend to Pages. The custom domain is bound in Cloudflare.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/web"
pnpm build
pnpm exec wrangler pages deploy ../dist --cwd pages --project-name sol-dottie --branch main
