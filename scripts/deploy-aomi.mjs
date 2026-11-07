import { appendFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { setTimeout as pause } from 'node:timers/promises';

const repository = 'aomi-labs/sol-dottie';
const platform = 'community';
const appName = 'dottie';

export async function deployAomi(env, { fetch = globalThis.fetch, sleep = pause, log = console.log } = {}) {
  for (const key of ['AOMI_BACKEND_URL', 'AOMI_PROJECT_ID', 'AOMI_APPLICATION_ID', 'AOMI_DEPLOY_TOKEN', 'GH_TOKEN', 'GITHUB_SHA']) {
    if (!env[key]) throw new Error(`Missing ${key}; see docs/aomi-ci.md`);
  }
  if (env.GITHUB_REPOSITORY !== repository || env.GITHUB_REF !== 'refs/heads/main') {
    throw new Error('Deployment requires the sol-dottie main branch');
  }
  const sha = env.GITHUB_SHA;
  if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error('GITHUB_SHA must be an immutable commit');
  const backend = new URL(env.AOMI_BACKEND_URL);
  if (backend.origin !== 'https://api-staging.aomi.dev' || backend.pathname !== '/') {
    throw new Error('This workflow only deploys to the existing staging backend');
  }
  const projectId = Number(env.AOMI_PROJECT_ID);
  const applicationId = Number(env.AOMI_APPLICATION_ID);
  if (![projectId, applicationId].every(id => Number.isSafeInteger(id) && id > 0)) {
    throw new Error('Project and application IDs must be positive integers');
  }

  async function request(path, body) {
    const response = await fetch(new URL(path, backend), {
      method: body === undefined ? 'GET' : 'POST',
      headers: { Authorization: `Bearer ${env.AOMI_DEPLOY_TOKEN}`, 'Content-Type': 'application/json' },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      redirect: 'error',
      signal: AbortSignal.timeout(60_000),
    });
    // Do not dump remote bodies: they can include credentials or internal details.
    if (!response.ok) throw new Error(`Aomi ${path}: HTTP ${response.status}`);
    const result = await response.json();
    if (result.ok === false) throw new Error(`Aomi ${path}: operation failed`);
    return result;
  }

  async function isCurrentMain() {
    const response = await fetch(`https://api.github.com/repos/${repository}/git/ref/heads/main`, {
      headers: { Authorization: `Bearer ${env.GH_TOKEN}`, Accept: 'application/vnd.github+json' },
      redirect: 'error',
      signal: AbortSignal.timeout(30_000),
    });
    if (!response.ok) throw new Error(`Cannot verify main: HTTP ${response.status}`);
    return (await response.json()).object?.sha === sha;
  }

  function verifyDeployment(record) {
    if (record?.source?.project_id !== projectId || record.source.repository_link !== repository ||
        record.source.commit_hash !== sha || record.platform?.platform !== platform ||
        record.platform.apps?.length !== 1 || record.platform.apps[0].name !== appName || !record.id) {
      throw new Error('Aomi deployment does not match the requested project, repository, commit and app');
    }
    return record;
  }

  if (!await isCurrentMain()) return log('Skipping superseded main commit');
  // Project preflight resolves and authorizes the app through its scoped token.
  // A name-only read is ambiguous when playground and canonical share a name.
  const livePath = `/api/platforms/${platform}/apps/${appName}`;
  const deployPath = `/api/projects/${projectId}/deploy`;
  verifyDeployment((await request(deployPath, { source_ref: sha, preflight: true })).deployment);
  const deployment = verifyDeployment((await request(deployPath, { source_ref: sha, preflight: false, build: true })).deployment);
  const release = deployment.platform.apps[0].release_tag;
  if (!release) throw new Error('Deployment has no release tag');
  log(`Aomi deployment ${deployment.id} for ${sha}`);

  async function poll(check, label, attempts) {
    for (let attempt = 0; attempt < attempts; attempt++) {
      if (await check()) return;
      await sleep(15_000);
    }
    throw new Error(`Timed out waiting for ${label}`);
  }
  await poll(async () => {
    const status = await request(`/api/platforms/${platform}/deployments/${encodeURIComponent(deployment.id)}/status`);
    const record = verifyDeployment(status.deployment);
    if (record.id !== deployment.id || record.platform.apps[0].release_tag !== release) {
      throw new Error('Deployment identity changed while polling');
    }
    if (status.state === 'failed') throw new Error('Aomi candidate CI failed');
    if (status.state !== 'ready') return false;
    if (status.ci?.status !== 'passed' || status.apps?.length !== 1 ||
        status.apps[0].name !== appName || status.apps[0].release_tag !== release ||
        status.apps[0].release_ready !== true) {
      throw new Error('Aomi ready response lacks a verified Dottie release');
    }
    return true;
  }, 'Aomi build and verified release assets', 120);

  // Serialization avoids overlapping runs; this also prevents a delayed older
  // build from activating after a newer commit has reached main.
  if (!await isCurrentMain()) return log(`Leaving superseded candidate ${deployment.id} inactive`);
  const activated = await request(`/api/platforms/${platform}/apps/activate`, {
    target: { kind: 'deployment', value: deployment.id },
    apps: [appName],
    actor: `github-actions:${env.GITHUB_RUN_ID ?? sha}`,
  });
  const apps = activated.apps;
  // The manager's 202 acknowledgment may omit app snapshots. The live read
  // below is the readiness authority; reject conflicting snapshots if supplied.
  if (activated.ok !== true || (apps !== undefined && (apps?.length !== 1 || apps[0].error ||
      apps[0].application_id !== applicationId || apps[0].release_tag !== release || apps[0].is_active !== true))) {
    throw new Error('Aomi did not activate the expected Dottie release');
  }
  await poll(async () => {
    const app = (await request(`${livePath}?release_tag=${encodeURIComponent(release)}`)).app;
    return app?.id === applicationId && app.project_id === projectId &&
      app.app_release_tag === release && app.is_active === true && app.artifact_ready === true && app.loaded === true;
  }, 'the active Dottie artifact', 24);
  log(`Activated ${release} for application ${applicationId}; artifact ready`);
  if (env.GITHUB_STEP_SUMMARY) {
    await appendFile(env.GITHUB_STEP_SUMMARY,
      `### Dottie on Aomi Build (staging)\n\nCommit: \`${sha}\`\n\nDeployment: \`${deployment.id}\`\n\nApplication: ${applicationId}\n\nRelease: \`${release}\`\n\nActive artifact verified. Chat and trading were not exercised.\n`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  deployAomi(process.env).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
