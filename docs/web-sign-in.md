# Web sign-in

Dottie's `/app/` uses the widget's shared sign-in sheet with Privy and Para,
matching chat.aomi.dev. Privy offers email, Google, X and Apple; Para offers
email and Google. The selected provider is remembered per backend and app.
The widget owns provider sessions and their exchange for an Aomi session.

Set these public GitHub repository variables for the Pages deployment:

- `VITE_PRIVY_APP_ID`: the target portal's public Privy app ID.
- `VITE_PARA_API_KEY`: the target portal's public Para project key.
- `VITE_PARA_ENVIRONMENT`: `PROD` or `BETA`, matching that project key.

These values are browser configuration. Server API secrets must never be
passed to Vite. The deployment fails if either public credential or the Para
environment is missing. Local development may omit the credentials to use
browser wallets alone.

Add `https://dottie.aomi.dev` to each provider project's allowed origins before
releasing. Local tests also need their exact localhost origin allowed. A
provider modal opening does not prove a completed Aomi session: acceptance
requires a real login, account resolution and the same account after reload.

Widget 4.0.2 requires its EVM runtime to mount either social provider, even
for a Solana app. Dottie keeps its Solana wallet presentation, chain pill and
direct app routing. The Vite configuration includes the Node browser shims
required by the provider SDKs; Para uses its real stylesheet.

The pinned x402 SVM dependency matches the portal's Kit-compatible version.
Para package extensions supply undeclared runtime imports; Privy's optional
Solana peers are installed explicitly so clean builds can load its SDK.
