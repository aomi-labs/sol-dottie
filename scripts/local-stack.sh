#!/usr/bin/env bash
# Run Sol Dottie against a full local Aomi stack.
#
#   scripts/local-stack.sh start    compile the app, start the stack and the site
#   scripts/local-stack.sh stop     stop the site and the stack
#   scripts/local-stack.sh status   show the stack, the app ID and the URLs
#
# The stack runs in its own aomi-dev workspace with its own ports and database,
# so other local stacks keep running. It uses the self-hosted `aomi-backend`
# because only that entrypoint registers libraries in $AOMI_APPS_DIR/local/.
# It needs aomi-dev, aomi-build, psql, the local Supabase Postgres on 54322, and
# OPENAI_API_KEY, OPENROUTER_API_KEY and ANTHROPIC_API_KEY in the environment.
#
# Overrides: AOMI_WORKTREES_ROOT, DOTTIE_WORKSPACE.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORKSPACE="${DOTTIE_WORKSPACE:-dottie-local}"
WORKTREES="${AOMI_WORKTREES_ROOT:-$HOME/Code/.worktrees}"
BACKEND_ROOT="$WORKTREES/$WORKSPACE/product-mono"
FRONTEND_ROOT="$WORKTREES/$WORKSPACE/aomi-widget"
# The checkout's aomi-dev matches the checkout's launchers. The installed one can lag.
AOMI_DEV=(python3 "$BACKEND_ROOT/.agents/skills/aomi-workflow/scripts/aomi-dev")
PORTAL_ORIGIN="http://localhost:8078"
TRUSTED_ORIGINS="$PORTAL_ORIGIN,http://localhost:3012,http://localhost:3004,http://127.0.0.1:3004"
SITE_LOG="$REPO_ROOT/target/local-stack-web.log"
SITE_PID="$REPO_ROOT/target/local-stack-web.pid"

runtime_dir() {
  local dir
  dir="$(ls -d "$WORKTREES/.state/runtime/$WORKSPACE"-* 2>/dev/null | head -1)"
  [ -n "$dir" ] || { echo "no runtime directory for $WORKSPACE" >&2; exit 1; }
  echo "$dir"
}

database_url() {
  python3 - "$(runtime_dir)/plan.json" <<'PY'
import json, sys
db = json.load(open(sys.argv[1]))["database"]
print(f"postgresql://{db['user']}:postgres@{db['host']}:{db['port']}/{db['name']}")
PY
}

application_id() {
  psql "$(database_url)" -Atc "select id from applications where name = 'dottie' and app_release_tag like 'local-%'"
}

ensure_workspace() {
  [ -d "$BACKEND_ROOT" ] && return
  aomi-dev workspace create "$WORKSPACE" --repos both --base main --profile standard >/dev/null
  "${AOMI_DEV[@]}" workspace sync-config "$WORKSPACE" --apply >/dev/null
}

start_stack() {
  local runtime edge
  "${AOMI_DEV[@]}" cargo build --bin aomi-backend
  runtime="$(runtime_dir)"
  edge="$FRONTEND_ROOT/apps/edge"

  (cd "$REPO_ROOT" && aomi-build compile --app dottie)
  mkdir -p "$runtime/plugins/local"
  cp "$REPO_ROOT/plugins/dottie.dylib" "$runtime/plugins/local/"

  # aomi-dev still reads the dev-only portal identity from this removed file.
  local portal_env="$FRONTEND_ROOT/apps/portal/LOCAL_ENV.example"
  [ -f "$portal_env" ] ||
    git -C "$FRONTEND_ROOT" show 113c01051^:apps/portal/LOCAL_ENV.example >"$portal_env"

  "${AOMI_DEV[@]}" down "$WORKSPACE" >/dev/null 2>&1 || true
  "${AOMI_DEV[@]}" plan "$WORKSPACE" --mode full --app portal --profile standard --database shared \
    --command "backend=$runtime/bin/aomi-backend --providers $runtime/providers.toml" \
    --service-env "frontend:AOMI_TRUSTED_ORIGINS=$TRUSTED_ORIGINS" \
    --service-env "portal_edge:AOMI_TRUSTED_ORIGINS=$TRUSTED_ORIGINS" \
    --service-env "portal_edge:PORTAL_TRUSTED_ORIGINS=$TRUSTED_ORIGINS" \
    --service-env "edge:AOMI_EDGE_ROOT=$edge" \
    --service-env "portal_edge:AOMI_EDGE_ROOT=$edge" >/dev/null
  "${AOMI_DEV[@]}" up

  # The reconciler registers the library shortly after the backend starts.
  local id=""
  for _ in $(seq 1 60); do
    id="$(application_id)"
    [ -n "$id" ] && break
    sleep 2
  done
  [ -n "$id" ] || { echo "dottie never registered as a local application" >&2; exit 1; }

  cat >"$REPO_ROOT/web/.env.local" <<EOF
VITE_AOMI_API_URL=$PORTAL_ORIGIN
VITE_DOTTIE_APPLICATION_ID=$id
VITE_DOTTIE_SHOW_MODEL=1
EOF
}

start_site() {
  if lsof -nP -iTCP:3004 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "site: port 3004 is already serving"
    return
  fi
  mkdir -p "$(dirname "$SITE_LOG")"
  (cd "$REPO_ROOT/web" && nohup pnpm dev >"$SITE_LOG" 2>&1 & echo $! >"$SITE_PID")
}

case "${1:-}" in
  start)
    ensure_workspace
    cd "$BACKEND_ROOT"
    start_stack
    start_site
    echo "site:   http://localhost:3004/"
    echo "portal: $PORTAL_ORIGIN"
    echo "app id: $(application_id)"
    ;;
  stop)
    if [ -f "$SITE_PID" ]; then
      kill "$(cat "$SITE_PID")" 2>/dev/null || true
      rm "$SITE_PID"
    fi
    cd "$BACKEND_ROOT"
    "${AOMI_DEV[@]}" down "$WORKSPACE"
    ;;
  status)
    cd "$BACKEND_ROOT"
    "${AOMI_DEV[@]}" status "$WORKSPACE" --compact
    echo "app id: $(application_id)"
    ;;
  *)
    echo "usage: $0 start|stop|status" >&2
    exit 2
    ;;
esac
