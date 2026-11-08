# pnl-fork.json

Recorded 2026-11-08 from the local Surfpool fork (`127.0.0.1:18899`), read-only,
after the Telegram test's diagnostic round trip (wallet
`DfFpn9sPxGcuEgs7M6KJ9qKtNneMZisb4EDfNKgZuJQD`, BONK):

- `signatures`: `getSignaturesForAddress` (`limit` 50, `confirmed`) for the
  wallet's BONK associated token account under each token program. The
  classic one (`53Cdxg…`) has the buy and the sell; the Token-2022 one is empty.
- `transactions`: both signatures' `getTransaction` receipts, as kept in
  `notes/test-fork-diagnostic-{buy,sell}-receipt.json`.
- `mint`: BONK's `getAccountInfo` (`jsonParsed`).

GMGN never indexed the fork, so no wallet activity is recorded.

| | SOL leg (lamports, fee and rent out) | fee | token-account rent |
|---|---|---|---|
| buy `3rFF4J…` | −50,000,000 | 15,001 | +1,488,440 |
| sell `2LsgJs…` | +49,846,039 | 5,003 | 0 |

Round return before fees: 49,846,039 / 50,000,000 − 1 = −0.307922%. Fees
20,004 lamports; the 1,488,440 lamports of rent stay in the open account.
