// Opt-in integration check with the signed-in Codex and actual bundled MCP executable.
// Only a synthetic contact is exposed; every permission request is denied.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import { randomUUID } from "node:crypto";
const project = fileURLToPath(new URL("..", import.meta.url));
const folder = await realpath(
  await mkdtemp(path.join(tmpdir(), "gc-codex-smoke-")),
);
const native = spawn(path.join(project, "target/debug/examples/host"), {
  stdio: ["pipe", "pipe", "inherit"],
  env: {
    ...process.env,
    GC_TEST_DESKTOP_BINARY: path.join(
      project,
      "target/debug/bundle/macos/OutreachDex.app/Contents/MacOS/globalcontact",
    ),
  },
});
const pending = new Map(),
  finished = new Map(),
  updates = [];
let seq = 0,
  analysisId;
const lines = createInterface({ input: native.stdout });
lines.on("line", (line) => {
  const value = JSON.parse(line);
  if (value.event) {
    if (value.event === "agent-permission")
      void call("app_call", {
        root: folder,
        action: "agent.permission",
        args: { id: value.payload.id, option: null },
      }).catch(() => {});
    if (value.event === "agent-finished")
      finished.set(value.payload.id, value.payload);
    if (value.event === "agent-update") updates.push(value.payload.update);
    return;
  }
  const request = pending.get(value.id);
  pending.delete(value.id);
  if (request) {
    clearTimeout(request.timer);
    value.error
      ? request.reject(new Error(value.error))
      : request.resolve(value.result);
  }
});
native.on("exit", (code) => {
  for (const p of pending.values()) {
    clearTimeout(p.timer);
    p.reject(new Error(`harness exited ${code}`));
  }
  pending.clear();
});
function call(command, args) {
  return new Promise((resolve, reject) => {
    const id = ++seq;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Timed out: ${command}`));
    }, 30000);
    pending.set(id, { resolve, reject, timer });
    native.stdin.write(JSON.stringify({ id, command, args }) + "\n");
  });
}
const app = (action, args = {}) =>
  call("app_call", { root: folder, action, args });
try {
  await call("workspace_open", { path: folder, create: true });
  const evidence = `fixture-${randomUUID()}`;
  const { slug } = await app("contact.create", {
    Name: "Synthetic Codex Test",
    Hook: `Local test check code: ${evidence}. This is a synthetic contact with no real email.`,
  });
  const { id } = await app("agent.start", {
    slug,
    agent: "codex",
    objective: "question",
    context: "general",
    prompt:
      "This is a read-only integration test. Read the current synthetic contact only through the OutreachDex MCP tools contact.get and timeline.get. Reply with the check code from Hook exactly as written, and briefly say how many timeline entries there are. Do not use the network, run shell commands, write files, update the dossier, contact or drafts, or call any other tool.",
  });
  analysisId = id;
  const deadline = Date.now() + 150000;
  while (!finished.has(id) && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 250));
  assert(finished.has(id), "real Codex analysis timed out");
  assert(!finished.get(id).error, finished.get(id).error);
  const history = await app("agent.history", { slug });
  const session = history.find((s) => s.id === id);
  assert.equal(session.status, "completed");
  if (!session.answer.includes(evidence)) {
    console.error('Codex answer:', session.answer);
    console.error('Tool updates:', JSON.stringify(updates.filter(u => ['tool_call', 'tool_call_update'].includes(u.sessionUpdate)).map(u => ({ title: u.title, status:u.status, content:u.content }))));
  }
  assert(
    session.answer.includes(evidence),
    "Codex must read the contact to discover the nonce",
  );
  assert(
    updates.some((u) => u.sessionUpdate === "tool_call"),
    "Codex must call a tool",
  );
  assert.equal((await app("draft.list", { slug })).length, 0);
  assert.equal(
    (await app("contact.get", { slug })).contact.values["Hook"],
    `Local test check code: ${evidence}. This is a synthetic contact with no real email.`,
  );
  console.log(
    "PASS real Codex: ACP → bundled MCP → synthetic contact read → streamed answer → persisted history; no drafts or sends.",
  );
  console.log(
    "Tools:",
    updates
      .filter((u) => u.sessionUpdate === "tool_call")
      .map((u) => u.title)
      .join(" | "),
  );
} finally {
  if (analysisId && !finished.has(analysisId))
    await app("agent.cancel", { id: analysisId }).catch(() => {});
  native.stdin.end();
  await new Promise((resolve) => {
    if (native.exitCode !== null) return resolve();
    const timer = setTimeout(() => native.kill(), 5000);
    native.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });
  lines.close();
  await rm(folder, { recursive: true, force: true });
}
