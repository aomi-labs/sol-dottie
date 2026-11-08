# Sol Dottie

Sol Dottie is the Solana sidekick from the "Ship a Financial Agent in an Hour"
clinic (Solana Hacker House London, Breakpoint 2026). This repo holds the site
and the Rust Aomi app behind it.

- `apps/dottie/`: the dynamic Aomi app, "Hoodit for Solana": memecoin research
  on GMGN (`chain=sol`), real traders and theses from Fomo, Jupiter exit
  quotes, Solana holdings, history and receipt-based PnL cards, watches,
  Telegram quick actions and settings. FluxBeam quotes and Solomon USDv reads
  live in their own skills.
- `worker/`: the settings Worker (Cloudflare KV) behind quick-action settings.
- `web/`: the landing page and live desk, a Vite and React site on the published
  `@aomi-labs/widget` (4.0.2). It comes from the `dottie.html` design, in the
  workshop palette (#8E44AD, #A3D9A1, #F5B041, #D5DBDB, #6C3483).
- `.aomi/config.json`: the repository import for Aomi Build.
- `docs/`: integration evidence, validation notes, release gates and the
  workshop script.
- `client/`: Node scripts that chat with a running Dottie, the same shape as
  the `aomi-labs/playground-example` template. `ask` sends one prompt and
  `tour` runs a scripted check of every tool.
- `tests/evals/`, `scripts/check-trace.mjs`: hosted conversation cases and the
  checker for normalized host traces.

Node 22.12+ and Rust 1.93.0 are required. `web/` and `client/` form one pnpm
workspace. The root `pnpm-lock.yaml` and `Cargo.lock` pin dependencies.

## Run the site

```sh
pnpm install
pnpm dev            # http://localhost:3004
```

The desk has two modes:

- **Concept** (default): the sample-data conversation from the design. Nothing
  touches a wallet.
- **Live**: open `http://localhost:3004/?application_id=<id>` or set
  `VITE_DOTTIE_APPLICATION_ID` in `web/.env.local`. The middle column becomes
  the real Aomi widget, with Solana wallets only (Phantom, Solflare, Backpack,
  Glow), pinned Direct to that app. The playbook cards put a starter prompt in
  the composer without sending it. `?concept` forces the sample desk.

Build the static site with:

```sh
NODE_OPTIONS=--max-old-space-size=6144 pnpm build:web
```

`pnpm -C web run deploy` publishes `web/dist` through Cloudflare when credentials
are configured.

### Site config

Defaults live in `web/src/site-config.ts`. A Vite env var in `web/.env.local`
(see `web/.env.example`) overrides each one:

| Variable | Meaning |
| --- | --- |
| `VITE_DOTTIE_APPLICATION_ID` | Hosted Dottie app. Empty means concept mode. `VITE_DOTTIE_APP_ID` and `VITE_AOMI_APPLICATION_ID` are accepted as fallbacks. |
| `VITE_AOMI_API_URL` | Aomi backend. Empty means `https://chat.aomi.dev`. `VITE_AOMI_BASE_URL` is accepted as a fallback. |
| `VITE_DOTTIE_TELEGRAM_URL` | Telegram bot link for the "Meet me on Telegram" buttons. |

### Site files

- `web/index.html`: the page markup from the concept.
- `web/src/dottie.css`: the concept's styles, recoloured.
- `web/src/mock-desk.ts`: page chrome and the sample-data desk.
- `web/src/live-desk.tsx` and `web/src/live.css`: the live widget desk.

## Telegram groups

`apps/dottie/aomi.toml` opts Dottie into group chats with an explicit public
research tool allowlist: token discovery, charts, holders, traders, exit quotes,
Solana reads and web research. Personal wallet/account tools and direct trading
shortcuts are excluded; the host continues personal or trading requests in the
asker's DM.

Group support requires the backend group-mode changes from
[product-mono#1675](https://github.com/aomi-labs/product-mono/pull/1675). Deploy
that backend first, then deploy and activate this app revision. The existing
`main` workflow deploys and activates Dottie on staging.

In a group, use `/ask@<bot_username> <question>`, mention Dottie, or reply to her
message. For mentions to work reliably, disable privacy mode through BotFather
(`/setprivacy` → Disable), then remove and re-add Dottie to existing groups.
`/ask` works with privacy mode on too. Untagged conversation receives no reply.
Start Dottie in a private chat once so she can send DM handoffs directly;
otherwise, follow the Open chat button she posts for the asker.

## Test the app

```sh
pnpm test:app                                    # cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
pnpm test:trace                                  # trace checker rules
pnpm test:settings                               # settings Worker
cargo build --release -p dottie                  # target/release/libdottie.so on Linux
```

The replay, direct-trade and PnL tests run on recorded responses, no network.
`scripts/dottie-record-replay.py` re-records the replay fixture (read-only; the
GMGN key is shared with Hoodit's bot, so run it once).

Live read-only dispatch signs nothing. Keys come from the environment:

```sh
set -a; . ~/.config/hoodit/gmgn.env; . ~/.config/dottie/fomo.env; set +a
cargo run -p dottie --example read -- manifest
cargo run -p dottie --example read -- dottie_token '{"token":"DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"}'
cargo run -p dottie --example read -- --plan apps/dottie/tests/fixtures/replay-plan.json
```

Run several calls as one `--plan`: each process has its own provider gate.

### App secrets

| Secret | Needed | Used for |
| --- | --- | --- |
| `GMGN_API_KEY` | required | boards, token research, KOL and smart-money feeds, wallets, history |
| `FOMO_API_KEY` | optional | top traders, positions, theses, Fomo warnings (credits) |
| `JUPITER_API_KEY` | optional | exit quotes; keyless quotes are paced to one per 2.2 s |
| `SOLANA_RPC_URL` | optional | holdings, mint accounts, PnL receipts; default public mainnet-beta |
| `DOTTIE_SETTINGS_URL`, `DOTTIE_SETTINGS_TOKEN` | optional | saved quick-action settings; unset means defaults (0.1/0.5/1 SOL buys, 25/50/100% sells) |

For local settings: `npx wrangler dev -c worker/wrangler.jsonc --var SETTINGS_TOKEN:local-dev-token`,
then point `DOTTIE_SETTINGS_URL` at `http://127.0.0.1:8787`.

## What works today

| Capability | Status |
| --- | --- |
| Site, concept desk and live desk | Implemented. The real widget needs a hosted app ID. |
| Research: boards, find, card, chart, trades, holders, signals, wallets, exit | Implemented on GMGN `chain=sol`, mint accounts, RugCheck summaries and Jupiter quotes. |
| Real traders (`dottie_people`) | Implemented on the Fomo API, gated to 18 requests a minute and cached. |
| Positions, history, PnL and PnL cards | Implemented: RPC holdings on both token programs, GMGN history, receipts from `getTransaction`. |
| Telegram quick actions (paste a mint, /buy, /sell, /positions, /settings) | Direct tools return `stage_jupiter_swap`; the host stages, simulates and commits. Needs the host's svm direct path. |
| FluxBeam WSOL/FLUXB quotes, Solomon USDv reads | In the `dottie/fluxbeam` and `dottie/earn` skills. |
| FluxBeam trade preparation, Solomon enrollment | Disabled until verified. |

The app never signs or broadcasts. Common Solana capabilities come from the
existing Aomi skills through the `aomi-core` and `svm-core` namespaces.

One process-wide request path paces every provider: GMGN's weight window,
Fomo's per-minute cap, keyless Jupiter and the public RPC. It shares
identical requests, caches reads and falls back to an aged answer (said as a
gap) when a provider is down. Quotes are never served stale.

## Chat from Node

`client/` reaches Dottie in direct mode with a guest sign-in:

```sh
cp client/.env.example client/.env       # set AOMI_APPLICATION_ID and AOMI_BASE_URL
pnpm --filter client ask "Which build are you?"
pnpm tour
```

`tour` runs scripted sessions against the hosted app. It
prints each tool call and the final reply. Bump `BUILD` in
`apps/dottie/src/lib.rs` on every change so `dottie_about` shows which build
answered.

## Run a full local stack

`scripts/local-stack.sh` runs the site against a local Aomi stack that has
loaded `apps/dottie`:

```sh
scripts/local-stack.sh start    # compile the app, start the stack and the site
scripts/local-stack.sh status
scripts/local-stack.sh stop
```

It needs `aomi-dev`, `aomi-build`, `psql`, the local Supabase Postgres on port
54322, and `OPENAI_API_KEY`, `OPENROUTER_API_KEY` and `ANTHROPIC_API_KEY` in the
environment. The stack runs in its own `dottie-local` workspace with its own
database, so other local stacks keep running. The portal is at
`http://localhost:8078`.

The script writes `web/.env.local` with the local application ID. It also sets
`VITE_DOTTIE_SHOW_MODEL=1`, which shows the model picker. Pick a model whose
supplier has credit, such as `anthropic:claude-sonnet-5-5`.

## Live chat setup

The site needs an application ID from a host that has loaded `apps/dottie`. No
hosted ID exists yet. For a local one, use `scripts/local-stack.sh`.

1. Import this repository root in Aomi Build, or use a local backend that loads
   the app.
2. Run the platform's SDK and manifest preflight before activation. This repo's
   v3 app pins SDK 7.0.0 (FFI 1) and requires the renamed Solana host tools.
   See [the version split](docs/sdk-versions.md). Record the activated commit and app ID.
3. Set `VITE_DOTTIE_APPLICATION_ID` and `VITE_AOMI_API_URL` in `web/.env.local`,
   or open `http://localhost:3004/?application_id=<id>`.
4. Point the existing Telegram adapter at the same app.
5. Complete the [release gates](docs/release.md). A successful compile does not
   prove hosted skill activation or trades.
