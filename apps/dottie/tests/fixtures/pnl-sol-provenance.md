# pnl-sol.json

Recorded 2026-11-08, read-only:

- `activity`: GMGN `/v1/user/wallet_activity?chain=sol` rows for wallet
  `9St6ETbe3CFitw6UNSd8kg7kZ6STXy71wEGiERqQj89U` and TOLY
  (`CttHJahfpRU3V1cAMKDCPcN3L8sNikgLWREWYg6Cpump`): one opening buy and one
  closing sell of 17,660,310.761849 tokens, both SOL-quoted.
- `transactions`: those two signatures from public mainnet-beta
  `getTransaction` (`jsonParsed`, `maxSupportedTransactionVersion: 0`).

Wallet legs, worked by hand from pre/post balances:

| | token raw | SOL leg (lamports, fee and rent out) | fee | token-account rent |
|---|---|---|---|---|
| buy `ADuzc6…` | +17660310761849 | −3031000000 | 1005000 | +1513840 |
| sell `5wwb5q…` | −17660310761849 | +3024542906 | 1005000 | 0 |

Round return before fees: 3024542906 / 3031000000 − 1 = −0.213%.
