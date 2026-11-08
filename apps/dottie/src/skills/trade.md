# Dottie trade

Use for requested buys, sells or sends on Solana. Swaps go through the host's `jupiter` skill; activate it only if missing. Host-restored skills survive turns, callbacks and compaction; don't reactivate them. Swaps follow `stage_jupiter_swap` → `svm_simulate` → `svm_commit_txs`.

## Direct execution
- A specified buy/sell is execution intent. Reuse this turn's checks for the same mint, wallet, size and slippage. Optional research, charts and advice follow execution.
- Every new buy or sell request stages fresh. Never commit ids from an earlier turn or from a shortcut.
- Activation, stage, simulation and commit are sequential: inspect ids and verdicts; never parallel financial mutations.

## The right token
- Pass the exact mint to `stage_jupiter_swap`, or `SOL`/`USDC`. A mint the user pasted is the token; copy mints from tool results, never retype them.
- SOL, stables, LSTs, majors and xStocks: the `dottie_assets` catalog mint. Stock tokens are not brokerage shares. A memecoin ticker: `dottie_find`.
- Ask which coin only for two liquid candidates; ignore illiquid copies.

## Before a buy (quick buttons run these in code)
- Memecoin: `dottie_token` mode `trade` at `sol_amount`/`usd_amount` (`slippage_bps` only when the user named one). It includes the exact-size exit; no separate `dottie_exit` or full card. `checks_passed` is advisory. If false, disclose the warnings (honeypot, live mint or freeze authority, risky extensions, transfer fee, tax, loss, unverified exit) and offer to proceed at that size. A clear yes after the warning authorizes it: stage, simulate, commit. Never repeat an accepted warning. A missing sell quote isn't proof of a honeypot.
- Consent covers that mint, wallet, size and slippage. Refresh stale facts; ask again only for materially new risks. Never silently resize, widen slippage or call unknown a pass.
- SOL, stables, stocks and majors: the Jupiter quote is the check. No meme research.
- Keep about 0.01 SOL for fees and a new token account's rent.

## Before a sell
Selling is always allowed: it's their bag. For a memecoin, quote with `dottie_exit` mode sell at `token_amount`; `loss_pct` over 10%: say so and offer to sell in parts. No route: say Jupiter can't sell it now; don't stage. Its `stage` is the sale to stage.

## Slippage
`dottie_token` trade and `dottie_exit` sell work out the slippage and return a `stage` object. Pass `stage` to `stage_jupiter_swap` as is; never drop or change `slippage_bps`. A user's own number goes to that tool as `slippage_bps`. Without a dottie check (SOL, stables, majors, xStocks), pass `slippage_bps` 50. The host caps it at 500: above that, say it can't go higher.

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
- A commit result with state failed is final: say it failed and why in one line, then offer to retry fresh. Never re-commit those ids.
- An expired quote: restage with the same `stage`, confirm changed terms, simulate the new ids. Never widen slippage or change size to pass.
- Timeout or unknown submission: check the commit state before any retry. Never duplicate a trade.

## Completion
- The commit result and the confirmation callback are the evidence. Never call `get_transaction` for your own commit, sleep, or search history for it. "sent" only when submitted or confirmed, "done" only when confirmed; `awaiting_broadcast` is signed but not on chain yet.
- A confirmation of a trade that's already done gets its receipt, not a second trade.
- After a sell that closes the position, make its PnL card (`dottie/card`) in the same reply. Exit advice and watch offers (`dottie/watch`) follow the result, never block it.
