# Validation

## Automated local checks

Rust provider fixtures are captured public responses from 2026-11-02. They test exact identity, quote amount/slippage, unknown holdings, report freshness semantics, malformed response handling and disabled write routes. The captured FluxBeam instruction uses a public non-user payer solely to inspect the wire format; it was not signed or executed.

Node trace-checker tests use **synthetic** events to verify rejection rules. They do not demonstrate model behavior. `tests/evals/cases.json` supplies prompts and required tools for a later hosted run; these cases are not claimed as passed until actual host traces are recorded.

Normalize exported host evidence to `{events:[...],requiredTools:[...]}` and run `node scripts/check-trace.mjs trace.json`. Events use canonical action IDs. `approval` means explicit user authorization; `confirmed` requires a recorded signature. Keep raw host traces alongside normalized evidence. The checker validates ordering/claims, not cryptographic receipts or the truth of supplied evidence. Core pipelines may request wallet signatures after commit preparation; normalize authorization separately from that signing callback.

Browser tests exercise desktop/mobile layout, navigation, prompt handoff and the honest unconfigured app state. Configured widget startup, a real streamed response, wallet signing and hosted confirmations require a deployed app ID and separate rehearsal.

## Required hosted evidence

Record: app ID + commit, host revision, skill activation result, real streamed chat response, operating wallet, stage/simulation identifiers, approval outcome and confirmed signature. Repeat on Telegram and a fresh builder account. Exercise failed simulation, expired quote, rejected approval and duplicate callback without resending confirmed actions.

## Results for this local implementation (2026-11-02)

- Rust: 17 provider/manifest/contract tests pass; fmt and clippy pass; release cdylib builds.
- Frontend: TypeScript/Vite build and ESLint pass.
- Trace checker: six synthetic contract tests pass.
- Browser checks: all five pass — four desktop/mobile scenarios plus configured-widget mounting against a deliberately unavailable test host.
- Live RugCheck, FluxBeam quote and Solomon position reads pass through compiled app dispatch; timestamps and sources are in evidence/live-smoke-summary.json.
- No hosted conversation eval, wallet signature, live trade, Telegram approval or Stripe checkout is claimed.
- Remote creation was attempted through authenticated GitHub CLI and rejected: `Resource not accessible by integration (createRepository)`. No GitHub repo or deployment was created.
