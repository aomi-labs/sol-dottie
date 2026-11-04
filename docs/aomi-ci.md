# Deploy Dottie on main merges

The `deploy-aomi` job in Checks runs after the app and trace checks pass on a
push to `main`. PRs only run checks. Frontend publishing is independent.

The job uses the existing V2 Project API, rather than copying build artifacts
into the platform repository or merging Aomi's generated candidate PR:

1. Verify the existing Dottie application and current main commit.
2. Preflight and deploy the exact GitHub event SHA.
3. Wait up to 30 minutes for platform CI and verified release assets.
4. Recheck main; leave superseded candidates inactive.
5. Activate only Dottie from that deployment, then wait for its exact active artifact.

Runs are serialized without cancelling an in-flight activation. Request timeouts
and polling limits fail the job instead of leaving it green while Aomi is still
building. Deploy and activation POSTs are not automatically retried after an
ambiguous network error; rerun the GitHub job to reconcile the same immutable SHA.

## Target and credential

This updates the current **staging** app used by Telegram and the frontend:

- Backend: `https://api-staging.aomi.dev`
- Platform: `community`
- Project: `1671`, source repository `aomi-labs/sol-dottie`
- Application: `2937818`, name `dottie`
- GitHub environment: `aomi-staging`

Before merging, set `AOMI_DEPLOY_TOKEN` in repository Actions secrets or in the
`aomi-staging` environment. Use an **app-scoped activation token bound to
application 2937818**, issued by the staging platform operator through
`POST /api/platforms/community/tokens` with
`{"scope":"app","app_id":2937818}`. Do not put an operator/admin bearer,
platform-wide token, runtime chat key, or browser session in CI.

The workflow's GitHub token only needs `contents: read`, used to verify main.
The source config stays `.aomi/config.json` version 1. Aomi handles SDK/schema
validation, billing eligibility, candidate builds, release digests, and activation
authorization. An incompatible SDK or missing credits fails deployment; CI does
not edit dependencies, fund accounts, or bypass those gates.

## Verification and recovery

`node --test tests/deploy-aomi.test.mjs` checks the sequence and failure gates
with mocked APIs; it does not deploy. The live staging SDK currently requires
`aomi-sdk = "=6.0.0"`, matching the app. The Project ID comes from the existing
platform deployment manifest; CI confirms its binding through the live app read
before writing.

An activated, artifact-ready release is not proof of completed chat or trading.
Exercise those separately through the existing app. A failed activation/readiness
check requires inspection of the linked Build Project and desired release; it
does not automatically roll back. To roll back, the operator activates a previously
verified deployment through the same API. Reverting this workflow stops future
automatic deployments but does not undo a release already activated.
