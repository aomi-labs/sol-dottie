# Sol Dottie

The landing page and live desk for Sol Dottie, the Solana sidekick from the
"Ship a Financial Agent in an Hour" clinic (Solana Hacker House London,
Breakpoint 2026). Built from the `dottie.html` concept design, in the workshop
palette (#8E44AD, #A3D9A1, #F5B041, #D5DBDB, #6C3483).

Standalone Vite + React app on the published `@aomi-labs/widget` (4.0.2).

## Run it

```sh
pnpm install      # or: npx pnpm@10 install
pnpm dev          # http://localhost:3004
```

The desk has two modes:

- **Concept** (default): the sample-data conversation from the design. Nothing
  touches a wallet.
- **Live**: open `http://localhost:3004/?application_id=<id>` (or set
  `VITE_DOTTIE_APPLICATION_ID` in `.env.local`). The middle column becomes the
  real Aomi widget, Solana wallets only (Phantom, Solflare, Backpack, Glow),
  pinned Direct to that App. The playbook cards put a starter prompt in the
  composer without sending it. `?concept` forces the sample desk.

## Config

Defaults live in `src/site-config.ts`; a Vite env var of the same name in
`.env.local` (see `.env.example`) overrides each:

| Variable | Meaning |
| --- | --- |
| `VITE_DOTTIE_APPLICATION_ID` | Hosted Dottie App. Empty means concept mode. |
| `VITE_AOMI_API_URL` | Aomi backend. Empty means `https://chat.aomi.dev`. |
| `VITE_DOTTIE_TELEGRAM_URL` | Telegram bot link for the "Meet me on Telegram" buttons. |

## Build

```sh
NODE_OPTIONS=--max-old-space-size=6144 pnpm build   # static site in dist/
```

## Files

- `index.html`: the page markup from the concept.
- `src/dottie.css`: the concept's styles, recoloured.
- `src/mock-desk.ts`: page chrome and the sample-data desk.
- `src/live-desk.tsx` + `src/live.css`: the live widget desk.
