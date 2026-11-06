import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deployAomi } from '../scripts/deploy-aomi.mjs';

const sha = 'a'.repeat(40);
const env = {
  AOMI_BACKEND_URL: 'https://api-staging.aomi.dev',
  AOMI_PROJECT_ID: '1674',
  AOMI_APPLICATION_ID: '2937820',
  AOMI_DEPLOY_TOKEN: 'test-aomi-credential',
  GH_TOKEN: 'test-github-credential',
  GITHUB_SHA: sha,
  GITHUB_REPOSITORY: 'aomi-labs/sol-dottie',
  GITHUB_REF: 'refs/heads/main',
};
const release = 'apps-141232970-rf6da202c44-dottie-aaaaaaaaaaaa';
const deployment = {
  id: 'dep_141232970_rf6da202c44_aaaaaaaaaaaa',
  source: { project_id: 1674, repository_link: 'aomi-labs/sol-dottie', commit_hash: sha },
  platform: { platform: 'community', apps: [{ name: 'dottie', release_tag: release }] },
};
const app = { id: 2937820, project_id: 1674, app_release_tag: release, is_active: true, artifact_ready: true };

function harness(overrides = {}) {
  const calls = [];
  const logs = [];
  let mainReads = 0;
  let polls = 0;
  let liveReads = 0;
  const ready = {
    ok: true, state: 'ready', deployment,
    ci: { status: 'passed' },
    apps: [{ name: 'dottie', release_tag: release, release_ready: true }],
  };
  const options = {
    log: message => logs.push(message),
    sleep: async () => {},
    fetch: async (url, init) => {
      url = new URL(url);
      const body = init.body && JSON.parse(init.body);
      calls.push({ path: url.pathname, body, host: url.host });
      let result;
      if (url.host === 'api.github.com') {
        assert.equal(init.headers.Authorization, 'Bearer test-github-credential');
        result = { object: { sha: overrides.main?.(++mainReads) ?? sha } };
      } else {
        assert.equal(url.origin, 'https://api-staging.aomi.dev');
        assert.equal(init.headers.Authorization, 'Bearer test-aomi-credential');
        assert.equal(init.redirect, 'error');
        if (url.pathname.endsWith('/apps/dottie')) {
          if (overrides.duplicateName && !url.searchParams.has('release_tag')) {
            return Response.json({ error: 'app token may only read its own app' }, { status: 403 });
          }
          result = { app: overrides.app?.(++liveReads) ?? app };
        } else if (url.pathname.endsWith('/deploy')) {
          assert.deepEqual(body, body.preflight ? { source_ref: sha, preflight: true } : { source_ref: sha, preflight: false, build: true });
          if (body.preflight && overrides.preflightFailure) return Response.json({ error: 'private' }, { status: 422 });
          result = { ok: true, deployment: overrides.deployment ?? deployment };
        } else if (url.pathname.endsWith('/status')) {
          result = overrides.status?.(++polls, structuredClone(ready)) ?? ready;
        } else if (url.pathname.endsWith('/activate')) {
          assert.deepEqual(body.target, { kind: 'deployment', value: deployment.id });
          assert.deepEqual(body.apps, ['dottie']);
          result = overrides.activation ?? {
            ok: true, activation: { status: 'activating' }, apps: [{ name: 'dottie', application_id: 2937820, release_tag: release, is_active: true }],
          };
        } else throw new Error(`Unexpected API path: ${url.pathname}`);
      }
      return Response.json(result);
    },
  };
  return { calls, logs, run: (input = env) => deployAomi(input, options) };
}

test('preflight, deploy exact SHA, await assets, activate and verify the active artifact', async () => {
  const h = harness({
    status: (n, ready) => n === 1 ? { ...ready, state: 'building' } : ready,
    app: n => ({ ...app, artifact_ready: n !== 1 }),
  });
  await h.run();
  assert.deepEqual(h.calls.filter(c => c.body).map(c => c.body.preflight ?? c.body.target.kind), [true, false, 'deployment']);
  assert.equal(h.calls.filter(c => c.path.endsWith('/status')).length, 2);
  assert.equal(h.calls.filter(c => c.path.endsWith('/apps/dottie')).length, 2);
  assert.match(h.logs.at(-1), /artifact ready/);
});

test('PR context and missing token fail before any API calls', async () => {
  for (const patch of [{ GITHUB_REF: 'refs/pull/3/merge' }, { AOMI_DEPLOY_TOKEN: '' }, { AOMI_BACKEND_URL: 'https://api.aomi.dev' }]) {
    const h = harness();
    await assert.rejects(h.run({ ...env, ...patch }));
    assert.equal(h.calls.length, 0);
  }
});

test('superseded commit makes no Aomi calls', async () => {
  const h = harness({ main: () => 'b'.repeat(40) });
  await h.run();
  assert.equal(h.calls.length, 1);
});

test('wrong project in preflight fails before applying a deployment', async () => {
  const h = harness({ deployment: { ...deployment, source: { ...deployment.source, project_id: 999 } } });
  await assert.rejects(h.run(), /does not match/);
  assert.equal(h.calls.filter(c => c.body).length, 1);
});

test('failed preflight never submits an apply request or leaks remote body', async () => {
  const h = harness({ preflightFailure: true });
  await assert.rejects(h.run(), /HTTP 422$/);
  assert.equal(h.calls.filter(c => c.body).length, 1);
});

for (const [name, overrides, message] of [
  ['wrong source commit', { deployment: { ...deployment, source: { ...deployment.source, commit_hash: 'b'.repeat(40) } } }, /does not match/],
  ['failed candidate', { status: (_, r) => ({ ...r, state: 'failed' }) }, /candidate CI failed/],
  ['unverified assets', { status: (_, r) => ({ ...r, apps: [{ ...r.apps[0], release_ready: false }] }) }, /lacks a verified/],
  ['wrong release', { status: (_, r) => ({ ...r, deployment: { ...deployment, platform: { ...deployment.platform, apps: [{ name: 'dottie', release_tag: 'other' }] } } }) }, /identity changed/],
  ['build timeout', { status: (_, r) => ({ ...r, state: 'building' }) }, /Timed out/],
]) {
  test(`${name} prevents activation`, async () => {
    const h = harness(overrides);
    await assert.rejects(h.run(), message);
    assert.equal(h.calls.some(c => c.path.endsWith('/activate')), false);
  });
}

test('main advancing during the build leaves the candidate inactive', async () => {
  const h = harness({ main: n => n === 1 ? sha : 'b'.repeat(40) });
  await h.run();
  assert.equal(h.calls.some(c => c.path.endsWith('/activate')), false);
  assert.match(h.logs.at(-1), /inactive/);
});

test('partial activation fails even with HTTP 202 success envelope', async () => {
  const h = harness({ activation: { ok: false, apps: [{ error: 'failed' }] } });
  await assert.rejects(h.run(), /operation failed/);
});

test('activation of another app is not treated as success', async () => {
  const h = harness({ activation: { ok: true, apps: [{ application_id: 999, is_active: true, release_tag: release }] } });
  await assert.rejects(h.run(), /did not activate/);
});

test('active metadata without a ready artifact times out', async () => {
  const h = harness({ app: () => ({ ...app, artifact_ready: false }) });
  await assert.rejects(h.run(), /Timed out waiting for the active Dottie artifact/);
});

test('same-name playground app cannot block canonical deployment', async () => {
  const h = harness({ duplicateName: true });
  await h.run();
  assert.match(h.logs.at(-1), /artifact ready/);
});
