# Sol Dottie

Sol Dottie is the Solana sidekick from the "Ship a Financial Agent in an Hour"
clinic (Solana Hacker House London, Breakpoint 2026). This repo holds the site
and the Rust Aomi app behind it.

- `apps/dottie/`: the dynamic Aomi app. It adds RugCheck research, direct
  FluxBeam quotes and Solomon USDv reads.
- `web/`: the landing page and live desk, a Vite and React site on the published
  `@aomi-labs/widget` (4.0.2). It comes from the `dottie.html` design, in the
  workshop palette (#8E44AD, #A3D9A1, #F5B041, #D5DBDB, #6C3483).
- `.aomi/config.json`: the repository import for Aomi Build.
- `docs/`: integration evidence, validation notes, release gates and the
  workshop script.
- `tests/evals/`, `scripts/check-trace.mjs`: hosted conversation cases and the
  checker for normalized host traces.

Node 22.12+ and Rust 1.93.0 are required. `web/pnpm-lock.yaml` and `Cargo.lock`
pin dependencies.

## Run the site

```sh
pnpm -C web install
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
| `VITE_DOTTIE_APPLICATION_ID` | Hosted Dottie app. Empty means concept mode. `VITE_DOTTIE_APP_ID` is accepted as a fallback. |
| `VITE_AOMI_API_URL` | Aomi backend. Empty means `https://chat.aomi.dev`. |
| `VITE_DOTTIE_TELEGRAM_URL` | Telegram bot link for the "Meet me on Telegram" buttons. |

### Site files

- `web/index.html`: the page markup from the concept.
- `web/src/dottie.css`: the concept's styles, recoloured.
- `web/src/mock-desk.ts`: page chrome and the sample-data desk.
- `web/src/live-desk.tsx` and `web/src/live.css`: the live widget desk.

## Test the app

```sh
pnpm test:app                                    # cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
pnpm test:trace                                  # trace checker rules
cargo build --release -p dottie                  # target/release/libdottie.dylib on macOS
```

Live read-only dispatch needs no keys and signs nothing:

```sh
cargo run -p dottie --example read -- manifest
cargo run -p dottie --example read -- dottie_token_check '{"mint":"FLUXBmPhT3Fd1EDVFdg46YREqHBeNypn1h4EbnTzWERX"}'
cargo run -p dottie --example read -- dottie_fluxbeam_quote '{"input_mint":"So11111111111111111111111111111111111111112","output_mint":"FLUXBmPhT3Fd1EDVFdg46YREqHBeNypn1h4EbnTzWERX","amount":"10000000","slippage_bps":100}'
cargo run -p dottie --example read -- dottie_solomon_position '{"wallet":"11111111111111111111111111111111"}'
```

The last address is a public non-user address. These reads do not prove
execution.

## What works today

| Capability | Status |
| --- | --- |
| Site, concept desk and live desk | Implemented. The real widget needs a hosted app ID. |
| RugCheck exact-mint reports | Implemented, with pacing, cache and unknown-data handling. |
| FluxBeam WSOL/FLUXB quotes | Implemented with a capped demo size and exact pool and program checks. |
| Solomon USDv model and wallet status | Implemented. Partial and unknown observations stay unknown. |
| Curated assets and NVDAx | Exact identity catalog. Quotes come from the core Jupiter skill. |
| FluxBeam trade preparation | Disabled until the full transaction and transfer-fee behavior is verified. |
| Solomon enrollment | Disabled until the challenge, proof and terms contract is verified. |
| Hosted core swaps, Telegram approvals, billing | Deployment dependencies. Local tests do not show them. |

The app never signs or broadcasts. Common Solana capabilities come from the
existing Aomi skills through the `aomi-core` and `svm-core` namespaces.

The HTTP runtime uses fixed provider origins, a 12-second timeout, one request
per second per provider per process, bounded cache and response sizes, a 429
cooldown and a five-second queue limit. Quotes are never cached. Several app
instances need an operator-level rate-limit plan before a large workshop.

## Live chat setup

The site needs an application ID from a host that has loaded `apps/dottie`. No
hosted ID exists yet.

1. Import this repository root in Aomi Build, or use a local backend that loads
   the app.
2. Run the platform's SDK and manifest preflight and activation against SDK
   6.0.0. Record the activated commit and app ID.
3. Set `VITE_DOTTIE_APPLICATION_ID` and `VITE_AOMI_API_URL` in `web/.env.local`,
   or open `http://localhost:3004/?application_id=<id>`.
4. Point the existing Telegram adapter at the same app.
5. Complete the [release gates](docs/release.md). A successful compile does not
   prove hosted skill activation or trades.
