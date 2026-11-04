# Dottie on Cloudflare Pages

The frontend is served at `https://dottie.aomi.dev` by the `sol-dottie` Pages project.
`web/pages/wrangler.jsonc` is the Pages configuration; the existing Workers configuration is separate.

Run `scripts/deploy-pages.sh` from a checkout containing the frontend version to publish.
It builds the site and publishes the output to the Pages production branch (`main`).
Authenticate Wrangler with a Pages-capable Cloudflare token or its existing OAuth login.

`web/.env.production` contains public values only. The current rehearsal uses
`https://chat-staging.aomi.dev`, app `2937818`, and `https://t.me/DottieSolBot`,
matching the working Telegram registration. Publishing this frontend does not
promote the backend, change a wallet policy, or prove a live Solana trade.

Bind `dottie.aomi.dev` to the Pages project and point its proxied CNAME to
`sol-dottie.pages.dev`. Keep these checks after a deployment:

- Landing page returns 200 and renders its artwork.
- The displayed QR decodes to `https://t.me/DottieSolBot`.
- `/app` loads the live desk; built assets contain no localhost API origin.
- Share-card URLs use `https://dottie.aomi.dev`.

The initial deployment uses the current working frontend snapshot while its
visual changes remain owned by the frontend implementer. The deployment PR
contains only Pages configuration, public environment values, this launcher
and these notes; it does not claim to contain those separate design changes.
