# Dottie SDK versions

| Repository | SDK | Purpose |
| --- | --- | --- |
| `aomi-labs/sol-dottie-playground` | `=6.0.0` | Existing Telegram bot/app deployment, with managed Solana wallets. |
| `aomi-labs/sol-dottie` | `=7.0.0` | Dottie v3, prepared for the renamed Solana host tools. |

Both apps request managed Solana wallets (`agent = true`, `create = true`,
`families = ["svm"]`, `suggest = "instant"`). Host configuration and a successful
provider response are still required for wallet provisioning.

Do not move the existing bot to this repository as part of the SDK upgrade.
Its current deployment remains owned by `sol-dottie-playground`. This repository
does not inherit the playground's Project 1672/app 2937818 deployment workflow.

SDK 7.0.0 retains FFI 1. FFI compatibility permits loading the library; it does
not establish availability of renamed runtime tools. The v3 trading skill uses
`svm_simulate` with ordered `transactions: [{id: ...}]`, rather than
`svm_simulate_ix` / `svm_simulate_tx`.

Before merging or deploying v3, merge and deploy the renamed host tools from
[product-mono #1618](https://github.com/aomi-labs/product-mono/pull/1618) and verify
them in the target environment. Its sub-PR #1638 merged into the feature branch,
which did not by itself ship those tools to main. Normal deployment preflight,
artifact, activation and live-chat checks remain required.
