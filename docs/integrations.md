# Integration evidence and remaining contracts

Inspected 2026-11-02. Backend reference: product-mono `9cbfed7250a945e98a5f3eaaae0b486f969e8555`. Hoodit reference: `7f829f5cba21696aa956cd6f7190134997deb7d2`.

## Runtime reuse

Enabled Solana host skills (product-mono `f9ab23d5c`): Jupiter, Raydium, Kamino, Orca, Marinade, SPL Token and Swig. Sanctum, Drift, Meteora, OpenBook and Squads are disabled. Jupiter, Raydium and Kamino expose stage tools (`stage_jupiter_swap` carries its own quote); the rest are instruction/IDL playbooks. Dottie does not duplicate these.

The host merges app-scoped skills into the bound thread; built-in skill activation is gated by available tool names. Declaring `svm-core` requires an activation smoke test on the deployed host. Host `svm_stage_ix` or a protocol stage tool → `svm_simulate` (ordered ids, instruction and venue ids may mix) → `svm_commit_txs` remains the only execution path; `svm_stage_tx` is host-only. Confirmation arrives in chat as a callback, so the model never polls `get_transaction`. Current custom providers emit no execution routes.

## RugCheck

Official schema: https://api.rugcheck.xyz/swagger/doc.json

`GET /v1/tokens/{mint}/report` is a read. **POST on the same path reports a suspicious token and must not be used for scanning.** Current implementation only uses GET, never forces refresh and requires no API key. Original schema and sanitized read fixtures are retained. `detectedAt` describes token detection, not report generation. Cache retrieval times are separate from provider freshness.

Live FLUXB report identified Token-2022 transfer-fee basis points of 300. This is why net wallet receipt cannot be inferred merely from the FluxBeam quote. Provider flags are evidence, not a guarantee.

## FluxBeam

Official schema: https://api.fluxbeam.xyz/swagger/doc.json

- `GET /v1/quote?inputMint=…&outputMint=…&amount=…&slippageBps=…`
- `POST /v1/swap/instruction` with `{quote,userPublicKey,wrapAndUnwrapSol,priorityFeeLamports}`
- `POST /v1/swap/transaction` with the same request shape

Verified quote pair: WSOL / FLUXB, pool `GeNc51GBjhXiYruWrx6VPh6xMNeTjYb5hUBpFJi9cW27`, program `FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X`.

The live instruction differs from the Swagger type declaration: `data` is base64, and `accounts` is an array of `{PublicKey,IsWritable,IsSigner}`. A captured instruction is 17 bytes: discriminator 1, little-endian u64 input amount and minimum output. Offline validation checks quote freshness, payer, mint/pool/program identity, sole expected signer and amounts. It is not a complete transaction validator.

The `/pools/{id}` detail endpoint returned 404 for the quoted pool; do not pretend it supplied independent pool validation. A transaction request using the public non-user test address returned 400. No signed or simulated trade was produced.

Before enabling prepare: verify pool/account state, source/destination token-account owners and mint programs; validate ATA creation and SOL wrapping; enforce output destinations and all instructions in the complete payload; check fee-aware receipt against quote; then hand the payload to the host's staging lane and rehearse. No env switch bypasses this missing implementation.

## Solomon

Primary product docs: https://docs.solomonlabs.org/llms-full.txt

Current app: https://app.solomonlabs.org/ (public bundle inspected to identify the read contract).

Current USDv mint: `USDvUSpnhCr9yBgj3UyVrD239HRUv4RsHwH2FxsWuMk`, 6 decimals. Legacy vault/sUSDv examples do not implement today's enrollment.

Live read: `GET https://yaas.solomonlabs.io/api/yaas/wallet/{address}?view=v2`.

Response contains walletAddress, nullable account, programPositions, holdings and observation. Preserve observation completeness, null holdings, owner identity and timestamps. An unavailable/partial response is not an empty wallet. No invented APR.

Current public app refers to `/api/yaas/wallet/{address}/enroll/challenge` and `/enroll`; enrollment includes nonce, signed ownership proof and terms validation. No write requests were issued. Before implementing enrollment, verify exact challenge bytes/domain/expiry/terms, use canonical host message/transaction signing as required, bind the reply to the same wallet and nonce, and validate submission/status with replay handling. The existing prepare tool explicitly returns unavailable and links the official app through product info.

## Stock identity

NVDAx mint and 8 decimals are corroborated by Trust Wallet's catalog and FluxBeam's verified listing; source records are retained. The issuer pages at xstocks.com/assets.backed.fi returned 403 in this environment. Dottie exposes core quote support, not a proven live stock purchase. Confirm the issuer catalog, current route and token scaling before adding a signed stock trade to the script. Never substitute a same-name mint, or equate token balances to shares without issuer conversion data.
