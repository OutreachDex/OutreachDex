#!/usr/bin/env node
// GCSP/1 reference server and mock data source (docs/gcsp.md). No dependencies.
//
//   node scripts/gcsp-mock.mjs [--port 8787] [--token dev-token] [--page-size 10]
//                              [--rate-limit N]   every Nth request answers 429 once
//                              [--broken-cursor]  never advances the cursor (for `source check`)
//
// Data imitates an AI thumbnail tool's admin: users with plans and credits, and events such as
// signups, purchases, feedback and failed jobs. POST /_mock/touch?id=<external_id> bumps a person's
// updated_at so incremental sync can be exercised; POST /_mock/optout?id=<external_id> turns the
// person into an opted-out tombstone (they asked not to be contacted).
import http from "node:http";

const arg = (name, fallback) => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] ?? true : fallback;
};
const port = Number(arg("port", 8787));
const token = String(arg("token", "dev-token"));
const pageSize = Number(arg("page-size", 10));
const rateLimit = Number(arg("rate-limit", 0));
const brokenCursor = process.argv.includes("--broken-cursor");

const manifest = {
  protocol: "gcsp/1",
  source: { id: "thumbnailsnap-mock", name: "ThumbnailSnap (mock)", url: "https://thumbnailsnap.com", default_locale: "en" },
  capabilities: ["people", "events", "person_detail"],
  attributes: [
    { key: "plan", label: "Plan", type: "string" },
    { key: "credits_balance", label: "Credits left", type: "number" },
    { key: "generations_30d", label: "Generations (30 days)", type: "number" },
    { key: "failed_jobs_30d", label: "Failures (30 days)", type: "number" },
    { key: "primary_format", label: "Main format", type: "string" },
    { key: "signup_at", label: "Signed up at", type: "datetime" },
    { key: "last_active_at", label: "Last active", type: "datetime" },
    { key: "country", label: "Country", type: "string", pii: true },
  ],
  event_types: [
    { type: "signup", label: "Sign-up" },
    { type: "first_generation", label: "First generation" },
    { type: "credits_exhausted", label: "Credits used up" },
    { type: "purchase", label: "Credits purchased" },
    { type: "subscription_started", label: "Subscription started" },
    { type: "subscription_canceled", label: "Subscription canceled" },
    { type: "job_failed_refunded", label: "Job failed, refunded" },
    { type: "feedback_submitted", label: "Feedback submitted" },
  ],
};

const base = Date.parse("2026-09-01T00:00:00Z");
const day = 86400000;
const iso = (ms) => new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");
const formats = ["YouTube 16:9", "Shorts 9:16", "Product shot", "Pinterest"];
const names = [
  "Lin Wei", "Chen Zhuo", "Maya Ortiz", "Sato Kenji", "Zhou Yining", "Tom Becker", "Rita Gomez", "He Siyuan",
  "Aria Chen", "Lucas Martin", "Priya Nair", "Wu Tong", "Emma Schulz", "Diego Ruiz", "Hana Kim", "Omar Haddad",
  "Liu Mei", "Noah Fischer", "Sofia Rossi", "Ivan Petrov", "Grace Lee", "Yuki Tanaka",
];
const people = names.map((name, i) => {
  const signup = base + i * 2 * day;
  const plan = i % 5 === 0 ? "creator" : i % 3 === 0 ? "credits" : "free";
  return {
    external_id: `u_${1000 + i}`,
    updated_at: iso(signup + (i % 7) * day + 3600000),
    display_name: name,
    identities: [{ kind: "email", value: `${name.toLowerCase().replace(/\s+/g, ".")}@example.com`, verified: i % 4 !== 3 }],
    attributes: {
      plan,
      credits_balance: plan === "free" ? (i % 2 ? 0 : 4) : 20 + i,
      generations_30d: (i * 7) % 40,
      failed_jobs_30d: i % 6 === 2 ? 5 : i % 3,
      primary_format: formats[i % formats.length],
      signup_at: iso(signup),
      last_active_at: iso(signup + (i % 9) * day),
      country: ["CN", "US", "JP", "DE", "BR"][i % 5],
      internal_score: 42, // not declared in the manifest: clients must drop it
    },
    url: `https://admin.thumbnailsnap.example/users/${1000 + i}`,
    // Most people have no locale (the manifest's default_locale applies); one is known to use Chinese.
    ...(name === "Zhou Yining" ? { locale: "zh-CN" } : {}),
  };
});
// Two people share emails with the bundled sample workspace: one verified (auto link), one not.
people[0].identities.push({ kind: "email", value: "nora.field@university.example", verified: true });
people[1].identities.push({ kind: "email", value: "leo.grant@mail.example", verified: false });
// A deleted account, as a tombstone.
people.push({ external_id: "u_0999", updated_at: iso(base + 50 * day), deleted: true });

const events = [];
people.filter((p) => !p.deleted).forEach((p, i) => {
  const signup = Date.parse(p.attributes.signup_at);
  const add = (offset, type, title, body, attributes) =>
    events.push({
      external_id: `ev_${p.external_id}_${type}`,
      person_external_id: p.external_id,
      occurred_at: iso(signup + offset),
      updated_at: iso(signup + offset),
      type,
      title,
      body_markdown: body,
      url: `https://admin.thumbnailsnap.example/events/${p.external_id}/${type}`,
      attributes,
    });
  add(0, "signup", "Signed up", null, { channel: i % 2 ? "google" : "email" });
  add(3600000, "first_generation", `First generation: ${p.attributes.primary_format}`, null, {});
  if (p.attributes.plan === "free" && p.attributes.credits_balance === 0)
    add(3 * day, "credits_exhausted", "Free credits used up", "All 12 free credits are used; no purchase yet.", {});
  if (p.attributes.plan === "credits") add(4 * day, "purchase", "Bought a $4.99 credit pack", null, { amount: 4.99 });
  if (p.attributes.plan === "creator") add(5 * day, "subscription_started", "Started Creator subscription", null, { amount: 9.9 });
  if (i % 7 === 4) add(9 * day, "subscription_canceled", "Canceled subscription", "Reason: generated text is often cropped.", { reason: "text_cropped" });
  if (p.attributes.failed_jobs_30d >= 5)
    add(6 * day, "job_failed_refunded", "5 failed generations, refunded automatically", "All failed jobs were 9:16 with a photo reference.", { refunded_credits: 20 });
  if (i % 4 === 1)
    add(
      7 * day,
      "feedback_submitted",
      "Feedback: too few fonts for Chinese titles",
      `> I want to make Chinese YouTube thumbnails, but there are only two Chinese fonts and the bold weight isn't striking enough.\n\nUser: ${p.display_name}`,
      { status: i % 8 === 1 ? "open" : "replied" },
    );
});

const order = (a, b) => a.updated_at.localeCompare(b.updated_at) || a.external_id.localeCompare(b.external_id);
const encode = (item) => Buffer.from(`${item.updated_at}|${item.external_id}`).toString("base64url");
const decode = (cursor) => {
  try {
    const [at, id] = Buffer.from(cursor, "base64url").toString().split("|");
    if (!at || !id) throw new Error();
    return { at, id };
  } catch {
    return null;
  }
};
function page(items, url) {
  const limit = Math.min(500, Math.max(1, Number(url.searchParams.get("limit")) || pageSize));
  const raw = url.searchParams.get("cursor");
  const after = raw ? decode(raw) : null;
  if (raw && !after) return [400, { error: { code: "invalid_cursor", message: "unrecognized cursor; sync from the start" } }];
  const sorted = [...items].sort(order);
  const rest = after
    ? sorted.filter((x) => x.updated_at > after.at || (x.updated_at === after.at && x.external_id > after.id))
    : sorted;
  const chunk = rest.slice(0, limit);
  const last = chunk.at(-1);
  const next = brokenCursor ? raw || (sorted[0] && encode(sorted[0])) : last ? encode(last) : raw || null;
  return [200, { items: chunk, next_cursor: next, has_more: rest.length > chunk.length }];
}

let requests = 0;
const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const send = (status, body, headers = {}) => {
    res.writeHead(status, { "content-type": "application/json; charset=utf-8", ...headers });
    res.end(JSON.stringify(body));
  };
  if (req.method === "POST" && url.pathname === "/_mock/touch") {
    const person = people.find((p) => p.external_id === url.searchParams.get("id"));
    if (!person) return send(404, { error: { code: "not_found", message: "unknown person" } });
    person.updated_at = iso(Date.now());
    if (person.attributes) person.attributes.last_active_at = person.updated_at;
    return send(200, person);
  }
  if (req.method === "POST" && url.pathname === "/_mock/optout") {
    const index = people.findIndex((p) => p.external_id === url.searchParams.get("id"));
    if (index < 0) return send(404, { error: { code: "not_found", message: "unknown person" } });
    people[index] = { external_id: people[index].external_id, updated_at: iso(Date.now()), deleted: true, reason: "opted_out" };
    return send(200, people[index]);
  }
  if (req.method !== "GET") return send(405, { error: { code: "method_not_allowed", message: "GCSP is read-only" } });
  const auth = req.headers.authorization || "";
  if (auth !== `Bearer ${token}` && req.headers["x-api-key"] !== token)
    return send(401, { error: { code: "unauthorized", message: "missing or wrong token" } });
  requests++;
  if (rateLimit && requests % rateLimit === 0) return send(429, { error: { code: "rate_limited", message: "slow down" } }, { "retry-after": "1" });
  const path = url.pathname.replace(/\/+$/, "");
  if (path === "/manifest") return send(200, manifest);
  if (path === "/people") return send(...page(people, url));
  if (path === "/events") return send(...page(events, url));
  const detail = path.match(/^\/people\/([^/]+)$/);
  if (detail) {
    const person = people.find((p) => p.external_id === decodeURIComponent(detail[1]));
    return person ? send(200, person) : send(404, { error: { code: "not_found", message: "no such person" } });
  }
  send(404, { error: { code: "not_found", message: `unknown path ${path}` } });
});
server.listen(port, "127.0.0.1", () =>
  console.log(`GCSP/1 mock: http://127.0.0.1:${port}  token: ${token}  people: ${people.length}  events: ${events.length}`),
);
