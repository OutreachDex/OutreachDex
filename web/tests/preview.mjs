// Screenshot the main pages against the real Rust host and a temporary copy of the sample workspace.
// Usage: node web/tests/preview.mjs [out-dir]   (needs target/debug/examples/host and target/test-workspace)
import { spawn } from "node:child_process";
import { mkdtemp, realpath, cp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

const root = fileURLToPath(new URL("../..", import.meta.url));
const out = path.resolve(process.argv[2] || path.join(root, "target/preview"));
await mkdir(out, { recursive: true });
const folder = await realpath(await mkdtemp(path.join(tmpdir(), "globalcontact-preview-")));
await cp(path.join(root, "target/test-workspace"), folder, { recursive: true });
const csv = path.join(await realpath(await mkdtemp(path.join(tmpdir(), "globalcontact-csv-"))), "users.csv");
await writeFile(
  csv,
  "Name,Email,Company,Plan,Notes\nLin Wei,lin@example.com,Northwind,Pro,PDF export fails\nChen Zhuo,chen@example.com,Indie developer,Trial,Asked about self-hosting\n",
);

const native = spawn(path.join(root, "target/debug/examples/host"), { stdio: ["pipe", "pipe", "inherit"] });
const mock = spawn("node", [path.join(root, "scripts/gcsp-mock.mjs"), "--port", "18799", "--token", "preview"], { stdio: "ignore" });
const pending = new Map();
let seq = 0,
  page;
createInterface({ input: native.stdout }).on("line", (line) => {
  const answer = JSON.parse(line);
  if (answer.event) return void page?.evaluate((d) => window.dispatchNative?.(d), answer).catch(() => {});
  const request = pending.get(answer.id);
  pending.delete(answer.id);
  if (answer.error) request?.reject(new Error(answer.error));
  else request?.resolve(answer.result);
});
const call = (command, args) =>
  new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    native.stdin.write(`${JSON.stringify({ id, command, args })}\n`);
  });

const server = await createServer({
  root: path.join(root, "web"),
  configFile: path.join(root, "web/vite.config.ts"),
  server: { host: "127.0.0.1", port: 0 },
  logLevel: "error",
});
await server.listen();
const browser = await chromium.launch({ channel: process.env.PREVIEW_CHANNEL || "chrome" });
page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on("pageerror", (e) => console.error("pageerror:", e.message));
await page.exposeFunction("nativeInvoke", async (command, args) => {
  if (command === "plugin:dialog|open") return args?.options?.filters ? csv : folder;
  if (command === "plugin:dialog|confirm") return true;
  // Tauri v2's confirm() goes through the message command and compares the button label.
  if (command === "plugin:dialog|message") return "Ok";
  const started = Date.now();
  const timer = setTimeout(() => console.error("slow:", command, JSON.stringify(args).slice(0, 200)), 8000);
  try {
    return await call(command, args);
  } catch (e) {
    console.error("failed:", command, args?.action, String(e).slice(0, 300));
    throw e;
  } finally {
    clearTimeout(timer);
    if (Date.now() - started > (Number(process.env.PREVIEW_SLOW) || 8000)) console.error("took", Date.now() - started, "ms:", command);
  }
});
// The suites assert English labels; the browser's own locale would otherwise pick the UI language.
await page.addInitScript(() => {
  if (!localStorage.getItem("gc.lang")) localStorage.setItem("gc.lang", "en");

  const callbacks = new Map(),
    listeners = new Map();
  let next = 1;
  window.dispatchNative = ({ event, payload }) => {
    for (const [id, entry] of listeners) if (entry.event === event) callbacks.get(entry.handler)?.({ event, payload, id });
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_, id) => listeners.delete(id) };
  window.__TAURI_INTERNALS__ = {
    invoke: (command, args) => {
      if (command === "plugin:event|listen") {
        const id = next++;
        listeners.set(id, args);
        return Promise.resolve(id);
      }
      if (command === "plugin:event|unlisten") return Promise.resolve();
      return window.nativeInvoke(command, args);
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback: (fn) => {
      const id = next++;
      callbacks.set(id, fn);
      return id;
    },
    unregisterCallback: (id) => callbacks.delete(id),
  };
});

const shot = async (name) => {
  await page.waitForTimeout(700);
  await page.screenshot({ path: path.join(out, `${name}.png`) });
  console.log("saved", name);
};
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
  for (const theme of ["light", "dark"]) {
    await page.evaluate((t) => localStorage.setItem("gc.ui", JSON.stringify({ theme: t, density: "comfortable" })), theme);
    await page.reload();
    await page.waitForTimeout(800);
    if (await page.locator(".welcome").count()) {
      await page.locator(".ws summary").click();
      await page.getByRole("button", { name: "Open workspace…" }).click();
    }
    await page.locator(".today h1").waitFor();
    await shot(`${theme}-today`);
    await page.getByRole("button", { name: /^People/ }).first().click();
    await page.locator("table.grid tbody tr").first().waitFor();
    await shot(`${theme}-people`);
    await page.locator("table.grid tbody tr").nth(2).click();
    await page.locator(".rec-head h1").waitFor();
    await shot(`${theme}-person`);
    await page.getByRole("tab", { name: /Drafts/ }).click();
    await shot(`${theme}-person-drafts`);
    await page.getByRole("tab", { name: /Dossier/ }).click();
    await page.waitForTimeout(2500);
    await shot(`${theme}-person-dossier`);
    await page.getByRole("button", { name: /^Settings/ }).click();
    await shot(`${theme}-settings`);
    if (theme === "light") {
      await page.getByRole("button", { name: /^People/ }).first().click();
      await page.getByRole("button", { name: /Import/ }).click();
      await page.getByRole("dialog").getByRole("button", { name: /^Contacts \(CSV/ }).click();
      await page.locator(".import-grid").waitFor();
      await shot("light-import");
      await page.getByRole("button", { name: /^Import \d+ rows/ }).click();
      await page.locator(".banner.notice").waitFor();
      await shot("light-import-done");
      await page.waitForTimeout(4000);
      console.error("rows after import:", await page.locator("table.grid tbody tr").count());
      await page.getByRole("button", { name: /^Sources/ }).click();
      await shot("light-sources-empty");
      await page.getByRole("button", { name: /Connect source/ }).first().click();
      await page.getByPlaceholder("https://admin.example.com/gcsp").fill("http://127.0.0.1:18799");
      await page.getByPlaceholder("Read-only token issued by your backend").fill("preview");
      await page.getByRole("button", { name: "Check", exact: true }).click();
      await page.locator(".check-summary").waitFor();
      await shot("light-source-check");
      await page.getByRole("button", { name: "Connect and sync" }).click();
      await page.locator(".source-card.on").waitFor();
      await page.locator(".banner.notice", { hasText: "Connected" }).waitFor({ timeout: 120000 });
      await shot("light-sources");
      await page.getByRole("button", { name: /^Unlinked/ }).click();
      await shot("light-sources-unlinked");
      await page.locator(".identity-open").first().click();
      await page.getByRole("dialog").locator(".source-events li").first().waitFor();
      await shot("light-source-person");
      await page.getByRole("button", { name: "Close dialog" }).click();
      await page.getByRole("button", { name: /^People/ }).first().click();
      const target = page.locator("table.grid tbody tr").filter({ hasText: "Nora Field" });
      console.error("rows:", await page.locator("table.grid tbody tr").count(), "matches:", await target.count());
      await target.first().click({ timeout: 10000 });
      await page.locator(".rec-head h1").waitFor();
      await shot("light-person-facts");
      // Lists: created through the host; never "batch prepare" here (that would run a real agent).
      const app = (action, args = {}) => call("app_call", { root: folder, action, args });
      await app("list.save", { list: { name: "Creator user follow-up", description: "Learn how they use the subscription and what gets in the way", source: "thumbnailsnap-mock", channel: "email", filters: [
        { field: "attr.plan", op: "eq", value: "creator" },
        { field: "event.subscription_started", op: "within_days", value: 3650 },
      ] } });
      const hou = (await app("contacts.list")).find((r) => String(r.values.Name).includes("Nora Field"));
      const listId = (await app("list.all"))[0].id;
      await mkdir(path.join(folder, `drafts/${hou.values.Slug}`), { recursive: true });
      await writeFile(
        path.join(folder, `drafts/${hou.values.Slug}/20261103-preview-email.md`),
        `---\ncontact_id: "${hou.values.Slug}"\nchannel: "email"\nto: "lin.wei@example.com"\nsubject: "How is the Creator subscription going?"\nstatus: "draft"\ncreated_by: "codex"\nlist: "${listId}"\n---\n\nHi,\n\nI saw you started the Creator subscription last month and mostly make 16:9 YouTube thumbnails. Has anything gotten in your way so far?\n\nThanks!\n`,
      );
      await page.getByRole("button", { name: /^Lists/ }).click();
      await page.locator(".list-header h1").waitFor();
      await shot("light-lists");
      await page.getByRole("button", { name: /Edit filters/ }).click();
      await page.locator(".filters-edit").waitFor();
      await page.waitForTimeout(800);
      await shot("light-list-editor");
      await page.getByRole("button", { name: "Cancel" }).click();
      await page.getByRole("button", { name: /Review one by one/ }).click();
      await page.locator(".draft-text").waitFor();
      await shot("light-list-review");
      await page.locator(".queue-head .link").click();
      await page.locator(".ws summary").click();
      await page.getByRole("button", { name: "New workspace…" }).click();
      await page.locator(".choices").waitFor();
      await shot("light-templates");
      await page.getByRole("button", { name: "Cancel" }).click();
    }
    await page.getByRole("button", { name: /^Today/ }).click();
  }
} catch (error) {
  await page.screenshot({ path: path.join(out, "failure.png") });
  throw error;
} finally {
  await browser.close();
  await server.close();
  native.kill();
  mock.kill();
}
