import assert from "node:assert/strict";
import test from "node:test";
import worker from "./index.js";

function store() {
  const docs = new Map();
  return {
    SETTINGS_TOKEN: "local-test-token",
    SETTINGS: {
      async get(key) { return docs.get(key) ?? null; },
      async put(key, value) { docs.set(key, value); },
    },
  };
}

function request(account, method = "GET", body, auth = true) {
  return new Request(`https://settings.test/api/settings/${account}`, {
    method,
    headers: auth ? { authorization: "Bearer local-test-token" } : {},
    body,
  });
}

test("settings persist per account and include the chosen slippage", async () => {
  const env = store();
  const saved = { slippage_bps: 50, pay: "sol", buy: ["0.1 SOL"], sell: ["100%"] };
  assert.deepEqual(await (await worker.fetch(request("one"), env)).json(), {});
  assert.equal((await worker.fetch(request("one", "PUT", JSON.stringify(saved)), env)).status, 200);
  assert.deepEqual(await (await worker.fetch(request("one"), env)).json(), saved);
  assert.deepEqual(await (await worker.fetch(request("two"), env)).json(), {});
});

test("only the settings API answers", async () => {
  const res = await worker.fetch(new Request("https://settings.test/"), store());
  assert.equal(res.status, 404);
});

test("unauthorized or invalid writes cannot replace saved settings", async () => {
  const env = store();
  const saved = { slippage_bps: 50 };
  await worker.fetch(request("one", "PUT", JSON.stringify(saved)), env);
  for (const [body, auth, status] of [
    ['{"slippage_bps":500}', false, 401],
    ["broken", true, 400],
    ["[]", true, 400],
    ['"string"', true, 400],
    [JSON.stringify({ payload: "x".repeat(2048) }), true, 413],
    [JSON.stringify({ payload: "é".repeat(1100) }), true, 413],
  ]) {
    assert.equal((await worker.fetch(request("one", "PUT", body, auth), env)).status, status);
  }
  assert.deepEqual(await (await worker.fetch(request("one"), env)).json(), saved);
  assert.equal((await worker.fetch(request("one", "GET", undefined, false), env)).status, 401);
  assert.equal((await worker.fetch(request("one", "POST", "{}"), env)).status, 405);
});
