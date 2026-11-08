# Dottie watch

Use when the user wants to hear when something happens to a coin.

1. Read `dottie_check` for the token once, so the threshold is relative to now and you can quote the current value.
2. Turn the ask into one numeric condition on a `dottie_check` field: `curve_pct`, `price_usd`, `fdv_usd`, `liquidity_usd`, `holders`, `top10_pct`, `dev_pct`, `snipers_pct`, `kol_wallets`, `smart_wallets`, `buy_5m_usd`, `sell_5m_usd`, `change_1h_pct`, `graduated` (1 or 0). Operators are `>=`, `<=`, `>`, `<` only.
   - "ping me at 80% on the pump.fun curve" → curve_pct >= 80 (`curve_pct` reads 100 once it graduates, so this still fires if it graduates between polls)
   - "tell me if the dev sells" → dev_pct < (current dev_pct − 0.5). If dev_pct is already 0, say the dev holds nothing and offer another trigger.
   - "if it dumps 30%" → price_usd <= current × 0.7
   - "when KOLs pile in" → kol_wallets >= current + 3
   - "when it graduates" → graduated >= 1
3. Arm it with the host `wake_on_condition`:
   - `condition`: a JSON string, e.g. `{"read":"dottie_check","args":{"token":"<mint>"},"path":"curve_pct","op":">=","value":80}`
   - `intent`: what to do when it fires, standalone: "Dottie alert: $TICKER (<mint>) curve_pct reached 80. Read dottie_token for <mint> and give the news plus a fresh two-line take. Don't trade."
   - `poll_seconds`: 600; 300 only for price stops ("if it dumps 30%"). Never below 300. `expires_at`: `as_of` from the check + 86400 unless the user asks for longer.
   - Keep it to 5 active watches per user.
4. Confirm in one line: what you're watching, the current value, the trigger, how long, and that the alert arrives in this chat and on Telegram if linked.

When a watch fires: read `dottie_token`, then give the news and a fresh take in two lines. Never trade automatically.

`list_scheduled` / `cancel_scheduled` show and stop watches. If arming says the thread needs a signed-in user, tell guests to sign in first.
