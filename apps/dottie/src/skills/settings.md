# Dottie settings

Use when the user wants to change their quick actions: the Telegram buy buttons ("set my buys to 0.25, 1 and 2 SOL"), the sell buttons, what pays for quick buys (auto, SOL or USDC), or their default slippage.

- Read `dottie_settings_get` only when you need the current values; otherwise pass just what changes to `dottie_settings_set`.
- Buy sizes are SOL or dollars ("0.5 SOL", "$20"), 1 to 3 of them; a bare number is SOL. Sell buttons are 1 to 3 percents. Slippage is in bps; 0 goes back to Dottie's per-coin defaults.
- Confirm the saved values in one line. A refusal ("Sign in to save settings.", or settings not configured here) or a validation message: say it plainly; quick buttons then use the defaults (0.1 / 0.5 / 1 SOL, 25 / 50 / 100%).
