# Deploy Dottie on main merges

The `deploy-aomi` job in Checks runs after the app and trace checks pass on a
push to `main`. PRs only run checks. Cloudflare Pages publishing runs separately after app, web and trace checks, and binds the frontend to canonical application `2937820`.

The job uses the existing V2 Project API, rather than copying build artifacts
into the platform repository or merging Aomi's generated candidate PR:

1. Verify current main, then use app-scoped Project preflight to validate the repository, commit and app. Name-only app reads are ambiguous because playground and canonical share `dottie`.
2. Preflight the exact GitHub event SHA, then deploy with `build: true`. The
   manager dispatches the trusted platform workflow with activation disabled.
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
- Project: `1674`, source repository `aomi-labs/sol-dottie`
- Application: `2937820`, name `dottie`
- GitHub environment: `aomi-staging`

`AOMI_DEPLOY_TOKEN` is configured in repository Actions secrets. It is an
**app-scoped activation token bound to
application 2937820**, issued by the staging platform operator through
`POST /api/platforms/community/tokens` with
`{"scope":"app","app_id":2937820}`. Do not put an operator/admin bearer,
platform-wide token, runtime chat key, or browser session in CI.

The workflow's GitHub token only needs `contents: read`, used to verify main.
The source config stays `.aomi/config.json` version 1. Aomi handles SDK/schema
validation, billing eligibility, candidate builds, release digests, and activation
authorization. An incompatible SDK or missing credits fails deployment; CI does
not edit dependencies, fund accounts, or bypass those gates.

Requires the manager build-dispatch support in the linked backend PR before merging.

## Verification and recovery

`node --test tests/deploy-aomi.test.mjs` checks the sequence and failure gates
with mocked APIs; it does not deploy. The live staging SDK requires
`aomi-sdk = "=7.0.0"`, matching the app. The Project ID comes from the existing
platform deployment manifest; CI confirms source identity through preflight before applying, then verifies the exact release and application ID after activation.

An activated, artifact-ready release is not proof of completed chat or trading.
Exercise those separately through the existing app. A failed activation/readiness
check requires inspection of the linked Build Project and desired release; it
does not automatically roll back. To roll back, the operator activates a previously
verified deployment through the same API. Reverting this workflow stops future
automatic deployments but does not undo a release already activated.
