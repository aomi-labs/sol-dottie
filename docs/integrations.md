# Integration evidence and remaining contracts

FluxBeam, Solomon and stock identity inspected 2026-11-02. GMGN, Fomo, Jupiter, RugCheck and execution re-checked 2026-11-08 against product-mono `origin/main` and Hoodit `234d9a6`.

## Runtime reuse

Enabled Solana host skills: Jupiter, Raydium, Kamino, Orca, Marinade, SPL Token and Swig. Dottie does not duplicate them. Execution stays host-side: `stage_jupiter_swap` (or `svm_stage_ix`) → `svm_simulate` (ordered ids) → `svm_commit_txs`. Confirmation arrives in chat as a callback, so the model never polls `get_transaction`.

`stage_jupiter_swap` takes `input_token`, `output_token` (symbol or exact mint), a human `amount` and `slippage_bps` (host cap 500). Its result carries the quote and `pending_tx_ids`, with `staged[].chain_kind = "svm"`. Dottie's direct buy and sell tools return exactly that call as `{"tx":{"tool":"stage_jupiter_swap","args":{…}}}` for the host's direct path to stage, simulate and commit. On product-mono main at the time of writing, `DirectExec::transact` picks `evm_simulate` whenever it sees `pending_tx_ids`; the host fix (choose `svm_*` from `chain_kind`) is a separate change.

## GMGN

`https://openapi.gmgn.ai`, `X-APIKEY` plus a timestamp and a fresh client id, IPv4. All routes take `chain=sol`: token info and security, market rank, trenches, KOL and smart-money feeds, token signals, top holders and traders, wallet stats and activity, created tokens, search and klines. There is no token trades route, so `dottie_trades` combines GMGN flow windows, labelled fills from the feeds and top traders' selling. Solana rows add `launchpad_platform` (Pump.fun, pump_mayhem, ray_launchpad, letsbonk…), `launchpad_status` (1 or 2 once graduated), `migrated_pool_exchange`, `renounced_mint`/`renounced_freeze_account`, bundler, sniper, rat and bot shares, `creator_token_status`, `twitter_create_token_count`, `cto_flag` and `is_wash_trading`. The key is shared with Hoodit: the gate keeps 4 weight per 1.4 s and a 429 pauses every caller.

## Fomo

`https://api.fomoapi.io`, `Authorization: Bearer`. Solana network id `1399811149`. Used: `/v2/leaderboard/{24h|7d|30d}`, `/v2/leaderboard/tokens/{trending|most-held}`, `/v2/users/{handle}` (2,500 credits; refunded while the wallet is still resolving), `/v2/users/{handle}/positions`, `/v2/thesis/token/{mint}?sort=likes` (one page, 1,250 credits), `/v2/token/{mint}/warnings` and `/v2/wallets/owner/{address}` (2,500 on a hit, 250 on a miss). Free plan: 250k credits a month, 20 requests a minute; Dottie keeps 18 a minute, FIFO, and caches boards 5 min, theses and warnings 15 min, traders 1 h and owners 24 h (misses too). A 402 (credits spent) or 429 is a normal failure.

## Jupiter quotes

`GET https://api.jup.ag/swap/v1/quote` answers keyless at about 0.5 requests a second; `x-api-key: $JUPITER_API_KEY` lifts it, as on the host. No route is a 400 with an error code, read as `no_sell_route`/`no_buy_route`, never as a honeypot. `swapUsdValue` and a 1 SOL → USDC quote give the SOL price.

## RugCheck and mint accounts

Official schema: https://api.rugcheck.xyz/swagger/doc.json

Dottie reads `GET /v1/tokens/{mint}/report/summary` (score, named risks, `lpLockedPct`). The full report runs to 2.5 MB for large tokens. **POST on the report path files a suspicious-token report and must not be used for scanning.** Mint and freeze authorities, the Token-2022 transfer fee and risky extensions (permanent delegate, transfer hook, non-transferable, pausable, default account state) come from the mint account itself (`getAccountInfo`, `jsonParsed`), the chain's own answer. FLUXB's mint shows a 300 bps transfer fee, which is why a FluxBeam quote alone can't give the net receipt.

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
