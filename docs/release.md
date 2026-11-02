# Release gates

Owner for app work: Dottie implementer. Owner for Solana infrastructure: Aaron. Host configuration and billing: platform operator.

## Current local release

Frontend, dynamic app, public provider reads, fixtures and deterministic tests are available. No hosted Dottie application ID, activated GitHub revision, Telegram bot binding, funded wallet or successful sandbox payment has been established by this implementation. No real transaction was submitted.

## Aaron handoff (required for live execution)

- Record target environment, host revision and SDK compatibility.
- Deploy Dottie; inspect its catalog and activate the existing Jupiter skill in a fresh bound thread.
- Read operating Solana wallet context; prepare, simulate and confirm one Jupiter swap through Dottie using the selected signer.
- Exercise browser rejection, expiry and duplicate callback behavior; verify a confirmed trade cannot be repeated by a later-step retry.
- Verify the staging lane chosen for the eventual FluxBeam payload and mainnet RPC reliability.
- Rehearse Telegram wallet-family selection and approval handoff separately. If unavailable, Telegram remains research/quotes with a fresh web request for execution.

Browser wallet is the required path. Agent-wallet signing and Swig are optional, independently rehearsed additions. Do not flip production Swig for this app.

## Provider writes

FluxBeam and Solomon writes are disabled in code, not hidden behind a switch that can be enabled without validation. See integrations.md for the precise missing contracts. Local instruction checks do not satisfy the full transaction release gate.

## Build / billing

Use a fresh builder account to import the repository, pass preflight and activate. Record initial balance, app charge timing, negative-balance admission and the actual hosted checkout mode. Only use Stripe test cards after confirming test mode on that environment. Verify successful webhook crediting and retry deployment; browser success alone does not prove balance credit. Do not advertise the sandbox payment path until this passes.

## Freeze

Pin app commit, host revision and frontend artifact. Record a backup of the rehearsed live flow. On the workshop day run read health checks and one authorized small transaction if the live path passed. If not, show the working research/quote workflow and explicitly identify the blocked execution steps.
