# Dottie on Cloudflare Pages

The frontend is served at `https://dottie.aomi.dev` by the `sol-dottie` Pages project.
`web/pages/wrangler.jsonc` is the Pages configuration; the existing Workers configuration is separate.

Every push to `main` (including a merged PR) deploys the frontend after all three
jobs in `.github/workflows/checks.yml` pass. Pull requests run checks without
deploying. Production deployments are serialized and recorded in the GitHub
`production` environment, with HTTP checks for the landing page and `/app/`.

Before merging, add `CLOUDFLARE_API_TOKEN` as a repository secret or a secret in
the `production` environment. Use a Cloudflare API token with **Account →
Cloudflare Pages → Edit**, restricted to account
`210e3092c7936592ca2078b5e26f7ea2`. The account ID is public and set in the workflow.
Do not use a short-lived local Wrangler OAuth token as the CI secret.

For a manual deployment, run `scripts/deploy-pages.sh` from a checkout containing
the frontend version to publish. It builds and publishes to the Pages production
branch (`main`). Authenticate Wrangler with the API token or its local OAuth login.

`web/.env.production` contains public values only. The current rehearsal uses
`https://chat-staging.aomi.dev`, app `2937818`, and `https://t.me/DottieSolBot`,
matching the working Telegram registration. The hosted backend source is
`aomi-labs/sol-dottie-playground`, Project `1672`, application `2937818`; its
Aomi Build deployment runs separately from this website repository.
Publishing this frontend does not
promote the backend, change a wallet policy, or prove a live Solana trade.

Bind `dottie.aomi.dev` to the Pages project and point its proxied CNAME to
`sol-dottie.pages.dev`. Keep these checks after a deployment:

- Landing page returns 200 and renders its artwork.
- The displayed QR decodes to `https://t.me/DottieSolBot`.
- `/app` loads the live desk; built assets contain no localhost API origin.
- Share-card URLs use `https://dottie.aomi.dev`.

The initial deployment uses the current working frontend snapshot while its
visual changes remain owned by the frontend implementer. The deployment PR
contains only CI deployment wiring, Pages configuration, public environment
values, this launcher and these notes; it does not contain those separate design changes.
