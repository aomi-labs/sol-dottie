// Dottie's settings store: one small JSON document per Aomi account in KV,
// read and written by the Dottie app's quick actions.
//
//   GET /api/settings/:id  -> the saved document, or {} when there is none
//   PUT /api/settings/:id  -> replaces it (a JSON object, at most 2 KB)
//
// Both need `Authorization: Bearer <SETTINGS_TOKEN>` (a Worker secret,
// shared only with the Dottie app as DOTTIE_SETTINGS_TOKEN).

// env: { SETTINGS: KV namespace, SETTINGS_TOKEN: secret }

const MAX_BODY = 2048;
const ROUTE = /^\/api\/settings\/([A-Za-z0-9_-]{1,128})$/;

function json(body, status = 200) {
  return new Response(typeof body === "string" ? body : JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

// Compares without leaking where the first difference is.
function same(a, b) {
  const x = new TextEncoder().encode(a);
  const y = new TextEncoder().encode(b);
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  }
  return diff === 0;
}

export default {
  async fetch(request, env) {
    const { pathname } = new URL(request.url);
    const id = ROUTE.exec(pathname)?.[1];
    if (!id) return json({ error: "not found" }, 404);
    const auth = request.headers.get("authorization") ?? "";
    if (!env.SETTINGS_TOKEN || !same(auth, `Bearer ${env.SETTINGS_TOKEN}`)) {
      return json({ error: "unauthorized" }, 401);
    }
    const key = `settings:${id}`;
    if (request.method === "GET") return json((await env.SETTINGS.get(key)) ?? "{}");
    if (request.method !== "PUT") return json({ error: "method not allowed" }, 405);
    if (Number(request.headers.get("content-length") ?? 0) > MAX_BODY) {
      return json({ error: "too large" }, 413);
    }
    const body = await request.text();
    if (new TextEncoder().encode(body).length > MAX_BODY) return json({ error: "too large" }, 413);
    let doc;
    try {
      doc = JSON.parse(body);
    } catch {
      return json({ error: "not JSON" }, 400);
    }
    if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
      return json({ error: "expected a JSON object" }, 400);
    }
    const text = JSON.stringify(doc);
    await env.SETTINGS.put(key, text);
    return json(text);
  },
};
