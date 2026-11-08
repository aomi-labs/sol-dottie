# Dottie trade

Use for requested buys, sells or sends on Solana. Swaps go through the host's `jupiter` skill; activate it only if missing. Host-restored skills survive turns, callbacks and compaction; don't reactivate them. Swaps follow `stage_jupiter_swap` → `svm_simulate` → `svm_commit_txs`.

## Direct execution
- A specified buy/sell is execution intent. Reuse this turn's checks for the same mint, wallet, size and slippage. Optional research, charts and advice follow execution.
- Activation, stage, simulation and commit are sequential: inspect ids and verdicts; never parallel financial mutations.

## The right token
- Pass the exact mint to `stage_jupiter_swap`, or `SOL`/`USDC`. A mint the user pasted is the token; copy mints from tool results, never retype them.
- SOL, stables, LSTs, majors and xStocks: the `dottie_assets` catalog mint. Stock tokens are not brokerage shares. A memecoin ticker: `dottie_find`.
- Ask which coin only for two liquid candidates; ignore illiquid copies.

## Before a buy (quick buttons run these in code)
- Memecoin: `dottie_token` mode `trade` at `sol_amount`/`usd_amount` and intended `slippage_pct`. It includes the exact-size exit; no separate `dottie_exit` or full card. `checks_passed` is advisory. If false, disclose the warnings (honeypot, live mint or freeze authority, risky extensions, transfer fee, tax, loss, unverified exit) and offer to proceed at that size. A clear yes after the warning authorizes it: stage, simulate, commit. Never repeat an accepted warning. A missing sell quote isn't proof of a honeypot.
- Consent covers that mint, wallet, size and slippage. Refresh stale facts; ask again only for materially new risks. Never silently resize, widen slippage or call unknown a pass.
- SOL, stables, stocks and majors: the Jupiter quote is the check. No meme research.
- Keep about 0.01 SOL for fees and a new token account's rent.

## Before a sell
Selling is always allowed: it's their bag. For a memecoin, quote with `dottie_exit` mode sell at `token_amount`; `loss_pct` over 10%: say so and offer to sell in parts. No route: say Jupiter can't sell it now; don't stage.

## Slippage
Always pass `slippage_bps`: the user's number if given; otherwise 50 for SOL, stables, majors and xStocks, 300 for memecoins with over $50k liquidity, 500 at or below that or on a curve. Above 500 only if the user agrees; never above 1000. The host may cap it lower.

## Commit
A clear request ("buy 0.1 sol of X", "sell all my Y") is the go-ahead: after a passing simulation, commit in the same turn. Don't ask "go?". Instant signs at once; Ask shows an Approve card, so say "approve it below".
- `stage_jupiter_swap` quotes, builds and stages in one call; its result carries the quote and the staged ids. Never restage or rebuild it. `svm_simulate` those ids in order (`{"transactions":[{"id":…}]}`), then `svm_commit_txs` with the same ids.
- With the commit, one line: coin, mint, amount in, expected and minimum out, slippage.
- Amounts read like a person wrote them: "0.1 SOL (~$9)", four significant digits.
- Fees: principal + network fee + account rent (back when the account closes) = wallet change. Never count the fee twice.
- For a quote-only request, stage and show the terms without committing.

## Withdrawals
"Send my X to <address>" is a transfer from the Dottie wallet. Confirm token, amount and the full destination, say it can't be undone, and commit only after a clear yes. Never send to an address the user didn't write.

## Failed or stale quote
- On a failed simulation or a rejected approval, stop and say the real cause in plain words (balance, fee, slippage, expired quote).
- An expired quote: restage with the same mint, size and slippage, confirm changed terms, simulate the new ids. Never widen slippage or change size to pass.
- Timeout or unknown submission: check the commit state before any retry. Never duplicate a trade.

## Completion
- The commit result and the confirmation callback are the evidence. Never poll `get_transaction`, sleep, or search history for it. "sent" only when submitted or confirmed, "done" only when confirmed; `awaiting_broadcast` is signed but not on chain yet.
- A confirmation of a trade that's already done gets its receipt, not a second trade.
- After a sell that closes the position, make its PnL card (`dottie/card`) in the same reply. Exit advice and watch offers (`dottie/watch`) follow the result, never block it.
