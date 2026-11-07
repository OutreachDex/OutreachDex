// Help center, CSV import, GCSP sources and outreach lists, end to end in a real browser against
// the real Rust host, the GCSP mock and a deterministic terminal Agent that prepares list drafts
// through the real MCP path. Nothing is sent and no model is called: the Agent is
// crates/gc-service/tests/terminal_agent.py.
//   node web/tests/outreach.mjs [webkit]
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, realpath, cp, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { chromium, webkit } from "playwright";
import { createServer } from "vite";
import { openContact, nav } from "./ui.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const browserName = process.argv[2] === "webkit" ? "webkit" : "chromium";
const folder = await realpath(await mkdtemp(path.join(tmpdir(), "globalcontact-outreach-")));
await cp(path.join(root, "target/test-workspace"), folder, { recursive: true });
const scratch = await realpath(await mkdtemp(path.join(tmpdir(), "globalcontact-outreach-csv-")));
const csv = path.join(scratch, "users.csv");
await writeFile(csv, "Name,Email,Company,Plan\nLin Wei,lin@example.test,Northwind,Pro\nChen Zhuo,chen@example.test,Indie developer,Trial\n");
// A WhatsApp "Export chat" file: the partner is named in the file name, the owner is the other author.
const chat = path.join(scratch, "WhatsApp Chat with Lin Wei.txt");
await writeFile(chat, "[02/03/2026, 09:15:00] Me Myself: Hi Lin, thanks for trying the beta\n[02/03/2026, 10:02:11] Lin Wei: Happy to! The export is great\nbut import is slow\n");
const port = browserName === "webkit" ? 18792 : 18791;
const mock = spawn("node", [path.join(root, "scripts/gcsp-mock.mjs"), "--port", String(port), "--token", "e2e"], { stdio: "ignore" });
const native = spawn(path.join(root, "target/debug/examples/host"), {
  stdio: ["pipe", "pipe", "inherit"],
  env: { ...process.env, GC_TEST_DESKTOP_BINARY: path.join(root, "target/debug/bundle/macos/OutreachDex.app/Contents/MacOS/globalcontact") },
});
const pending = new Map();
let seq = 0,
  page,
  sends = 0;
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
const app = (action, args = {}) => call("app_call", { root: folder, action, args });

const server = await createServer({
  root: path.join(root, "web"),
  configFile: path.join(root, "web/vite.config.ts"),
  server: { host: "127.0.0.1", port: 0 },
  logLevel: "error",
});
await server.listen();
const browser = await { chromium, webkit }[browserName].launch();
page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.setDefaultTimeout(30000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
await page.exposeFunction("nativeInvoke", async (command, args) => {
  if (command === "plugin:dialog|open") {
    const filter = args?.options?.filters?.[0]?.name;
    return filter === "WhatsApp" ? chat : filter ? csv : folder;
  }
  if (command === "plugin:dialog|confirm") return true;
  // Tauri v2's confirm() goes through the message command and compares the button label.
  if (command === "plugin:dialog|message") return "Ok";
  if (command === "app_call" && ["draft.send", "draft.mark_sent"].includes(args.action)) sends++;
  if (command !== "app_call" && !command.startsWith("file_") && !command.startsWith("document_")) console.error("native command:", command);
  return call(command, args);
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

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);

  // Help works before any workspace is open, with search and links between topics.
  await page.getByRole("button", { name: "Help", exact: true }).click();
  await page.locator(".help-body h1", { hasText: "Quick start" }).waitFor();
  await page.getByLabel("Search help").fill("suppression");
  const found = await page.getByRole("navigation", { name: "Help contents" }).getByRole("button").allTextContents();
  assert(found.length >= 2 && found.length < 14, `search narrows topics: ${found}`);
  await page.getByLabel("Search help").fill("");
  await page.getByRole("navigation", { name: "Help contents" }).getByRole("button", { name: "Quick start" }).click();
  await page.locator(".help-body").getByRole("button", { name: "Import contacts" }).click();
  await page.locator(".help-body h1", { hasText: "Import contacts" }).waitFor();
  // GFM tables render as tables, not as pipe-separated text.
  await page.getByRole("navigation", { name: "Help contents" }).getByRole("button", { name: "People and the person page" }).click();
  await page.locator(".help-body table th", { hasText: "Section" }).waitFor();
  assert(!(await page.locator(".help-body").innerText()).includes("|---|"), "no raw table markup");

  // Open the sample workspace; the deterministic agent replaces any real CLI.
  await page.locator(".ws summary").click();
  await page.getByRole("button", { name: "Open workspace…" }).click();
  await page.locator(".today h1").waitFor();
  const settings = await app("settings.get");
  await app("settings.save", {
    ...settings.config,
    defaultAgent: "codex",
    terminal: { codex: path.join(root, "crates/gc-service/tests/terminal_agent.py") },
  });
  const contactsBefore = (await app("contacts.list")).length;

  // CSV import with suggested mapping; Plan is not a field and stays unmapped.
  await nav(page, "People");
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: /^Contacts \(CSV/ }).click();
  const dialog = page.getByRole("dialog");
  await dialog.locator(".import-grid").waitFor();
  assert.equal(await dialog.getByLabel("Field for Name").inputValue(), "Name");
  assert.equal(await dialog.getByLabel("Field for Email").inputValue(), "Email");
  assert.equal(await dialog.getByLabel("Field for Plan").inputValue(), "");
  await dialog.getByRole("button", { name: /^Import 2 rows/ }).click();
  await page.locator(".banner.notice", { hasText: "2 created" }).waitFor();
  assert.equal(await page.locator("table.grid.people tbody tr").count(), contactsBefore + 2, "imported people show with the result");
  const lin = (await app("contacts.list")).find((r) => r.values.Name === "Lin Wei");
  assert.equal(lin.values.Email, "lin@example.test");

  // A WhatsApp chat export: the partner matches Lin Wei by name and the chat lands on her timeline.
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: /^WhatsApp chat/ }).click();
  const chatDialog = page.getByRole("dialog");
  await chatDialog.locator(".import-grid").waitFor();
  assert.equal(await chatDialog.getByLabel("Contact for Lin Wei").inputValue(), lin.values.Slug);
  await chatDialog.getByText("1 from them · 1 from you").waitFor();
  await chatDialog.getByRole("button", { name: "Import conversations (1)" }).click();
  await page.locator(".banner.notice", { hasText: "New messages: 2" }).waitFor();
  const linDetail = await app("contact.get", { slug: lin.values.Slug });
  const chats = linDetail.timeline.filter((r) => r.values.Channel === "whatsapp");
  assert.deepEqual(chats.map((r) => r.values.Direction).sort(), ["in", "out"], "one row per day and direction");
  assert(linDetail.contact.values["Last reply"].startsWith("2026-03-02"), "her reply is the last reply");

  // ⌘K finds a person and opens them; bulk-select two rows and change their stage.
  await page.keyboard.press(process.platform === "darwin" ? "Meta+k" : "Control+k");
  await page.getByLabel("Search people, lists, drafts or pages").fill("Lin Wei");
  await page.keyboard.press("Enter");
  await page.locator(".rec-head h1", { hasText: "Lin Wei" }).waitFor();
  await nav(page, "People");
  await page.getByLabel("Select Lin Wei").check();
  await page.getByLabel("Select Chen Zhuo").check();
  await page.getByRole("toolbar", { name: "Bulk actions" }).getByText("2 selected").waitFor();
  await page.getByLabel("Change stage for selected").selectOption("On hold");
  await page.locator(".banner.notice", { hasText: "Moved 2 people to" }).waitFor();
  const staged = (await app("contacts.list")).filter((r) => ["Lin Wei", "Chen Zhuo"].includes(r.values.Name));
  assert(staged.every((r) => r.values.Stage === "On hold"), "bulk stage applies to both");

  // Connect the GCSP mock: check first, then connect and sync in one step.
  await nav(page, "Sources");
  await page.getByRole("button", { name: /Connect source/ }).first().click();
  await page.getByPlaceholder("https://admin.example.com/gcsp").fill(`http://127.0.0.1:${port}`);
  await page.getByPlaceholder("Read-only token issued by your backend").fill("wrong");
  await page.getByRole("button", { name: "Check", exact: true }).click();
  await page.locator(".check-summary.fail").waitFor();
  assert(await page.getByRole("button", { name: "Connect and sync" }).isDisabled(), "a failing source cannot be connected");
  await page.getByPlaceholder("Read-only token issued by your backend").fill("e2e");
  await page.getByRole("button", { name: "Check", exact: true }).click();
  await page.locator(".check-summary.ok").waitFor();
  assert(await page.locator(".check-row", { hasText: "internal_score" }).count(), "the undeclared attribute is a warning");
  await page.getByRole("button", { name: "Connect and sync" }).click();
  await page.locator(".banner.notice", { hasText: "Connected" }).waitFor({ timeout: 60000 });
  const [source] = await app("source.list");
  assert.equal(source.counts.linked, 1, "verified email links automatically");
  assert.equal(source.last_run.status, "completed");

  // The unverified match waits for a person; confirming it links and backfills events.
  const pendingCount = source.counts.pending;
  console.log("pending before confirm:", JSON.stringify((await app("source.people", { source: source.key, match: "pending" })).map((p) => [p.name, p.contact])));
  if (pendingCount) {
    await page.getByRole("button", { name: /^To confirm/ }).click();
    await page.getByRole("button", { name: "Same person" }).first().click();
    await page.locator(".banner.notice", { hasText: "Linked to" }).waitFor();
    const left = await app("source.people", { source: source.key, match: "pending" });
    assert.equal(left.length, pendingCount - 1, `pending after confirm: ${JSON.stringify(left.map((p) => [p.name, p.contact]))}`);
  }
  await page.getByRole("button", { name: /^Unlinked/ }).click();
  assert((await page.locator(".identity").count()) >= 15);
  // An unlinked user can be understood without making them a contact: details, then the Agent.
  const firstUnlinked = page.locator(".identity").first();
  const unlinkedName = (await firstUnlinked.locator(".identity-main b").innerText()).trim();
  await firstUnlinked.locator(".identity-open").click();
  const personDialog = page.getByRole("dialog", { name: unlinkedName });
  await personDialog.getByText("Source facts").waitFor();
  await personDialog.locator(".source-events li").first().waitFor();
  await personDialog.getByRole("button", { name: "Ask Agent" }).click();
  await personDialog.waitFor({ state: "hidden" });
  const agentDock = page.getByRole("complementary", { name: "Agent workbench" });
  await agentDock.locator(".agent-workspace-name", { hasText: `Understand user · ${unlinkedName}` }).waitFor();
  const shows = (text) => page.waitForFunction((t) => document.querySelector(".agent-terminal")?.textContent.includes(t), text, { timeout: 20000 });
  await shows("SOURCE READY");
  await agentDock.locator(".xterm-helper-textarea").focus();
  await page.keyboard.insertText("task");
  await page.keyboard.press("Enter");
  await shows("SOURCE DONE");
  await agentDock.getByRole("region", { name: "Agent progress" }).locator(".agent-task.done").waitFor();
  assert.equal((await app("source.people", { source: source.key, match: "unlinked" })).some((p) => p.name === unlinkedName), true, "still not a contact");

  // Source facts on the auto-linked person.
  await openContact(page, "Nora Field");
  await page.locator(".card", { hasText: "Source facts" }).getByText("Plan").waitFor();
  await page.getByRole("tab", { name: /Activity/ }).click();
  await page.locator(".tl-item", { hasText: "Signed up" }).waitFor();

  // A list built in the editor, with a live count.
  await nav(page, "Lists");
  await page.getByRole("button", { name: "New list" }).first().click();
  const editor = page.getByRole("dialog");
  await editor.getByPlaceholder("e.g. Used up free credits but never paid").fill("Creator follow-up");
  await editor.getByPlaceholder("e.g. find out where they got stuck and whether they need help").fill("Learn how they use the subscription");
  await editor.getByRole("button", { name: "Add condition" }).click();
  await editor.locator(".filter-row select").first().selectOption("attr.plan");
  await editor.locator(".filter-row input").first().fill("creator");
  await editor.getByText(/Currently matching: \d+ people/).waitFor();
  const matched = Number((await editor.getByText(/Currently matching: \d+ people/).innerText()).match(/(\d+) people/)[1]);
  assert(matched >= 3, `creator users match: ${matched}`);
  await editor.getByRole("button", { name: "Save list" }).click();
  await page.locator(".list-header h1", { hasText: "Creator follow-up" }).waitFor();
  assert.equal(await page.locator("table.grid.members tbody tr").count(), matched);
  const [list] = await app("list.all");
  assert.match(await readFile(path.join(folder, `lists/${list.id}.toml`), "utf8"), /attr\.plan/);

  // Prepare one person: the list queues them and the right-hand Agent does the work via MCP.
  await page.locator("table.grid.members tbody tr").nth(1).getByRole("button", { name: "Prepare" }).click();
  const dock = page.getByRole("complementary", { name: "Agent workbench" });
  await dock.locator(".agent-workspace-name", { hasText: "Prepare list" }).waitFor();
  const terminalShows = (text) =>
    page.waitForFunction((t) => document.querySelector(".agent-terminal")?.textContent.includes(t), text, { timeout: 20000 });
  await terminalShows(`LIST READY ${list.id}`);
  await page.locator("table.grid.members tbody tr").nth(1).getByText("Queued").waitFor();
  await dock.locator(".xterm-helper-textarea").focus();
  await page.keyboard.insertText("list");
  await page.keyboard.press("Enter");
  await terminalShows("LIST DONE 1");
  // Batch progress and the final report come back to the panel (task.report).
  await dock.getByRole("region", { name: "Agent progress" }).locator(".agent-task.done").waitFor();
  await dock.getByRole("region", { name: "Agent progress" }).getByText("List ready: 1 drafts").waitFor();
  await page.locator("table.grid.members tbody tr").nth(1).getByText("Pending review").first().waitFor({ timeout: 20000 });
  const agentDraft = (await app("draft.list")).find((d) => d.metadata.list === list.id);
  assert.equal(agentDraft?.metadata.status, "draft", "the Agent's list draft waits for review");

  // Review: two list drafts, approve one, mark the other do-not-contact.
  const members = (await app("list.get", { id: list.id })).members.filter((m) => m.contact);
  assert(members.length >= 2, "prepared and linked members have contacts");
  for (const m of members.slice(0, 2)) {
    const saved = await app("draft.save", { contact_id: m.contact, channel: "email", subject: `Hi ${m.name}`, body: "I'd like to hear how the Creator subscription is working for you." });
    await writeFile(
      path.join(folder, saved.path),
      (await readFile(path.join(folder, saved.path), "utf8")).replace("\n---\n", `\nlist: "${list.id}"\n---\n`),
    );
  }
  await page.reload();
  await nav(page, "Lists");
  await page.getByRole("button", { name: /Review one by one/ }).click();
  await page.locator(".draft-text").waitFor();
  assert((await page.locator(".why-now li").count()) >= 1, "why now lists the evidence");
  const first = await page.locator(".review-main .rec-head h1").innerText();
  await page.getByRole("button", { name: "Approve and next" }).click();
  await page.locator(".banner.notice", { hasText: "Approved" }).waitFor();
  await page.waitForFunction((name) => document.querySelector(".review-main .rec-head h1")?.textContent !== name, first);
  const second = await page.locator(".review-main .rec-head h1").innerText();
  await page.getByRole("button", { name: "Do not contact" }).click();
  await page.locator(".review-done").waitFor();
  const suppression = await readFile(path.join(folder, "suppression.txt"), "utf8");
  assert(suppression.includes("@"), "do-not-contact writes suppression.txt");
  const after = await app("list.get", { id: list.id });
  assert(after.members.some((m) => m.name === second && m.status === "suppressed"), "the suppressed person is recorded");
  const approved = (await app("draft.list")).find((d) => d.metadata.list === list.id && d.metadata.status === "approved");
  assert(approved, "approval does not send");
  assert.equal(approved.metadata.status, "approved");

  // The send dialog requires an SMTP account; none is configured, so nothing can go out.
  await page.getByRole("button", { name: "Back to list" }).click();
  assert.equal(sends, 0, "no send or mark-sent request was ever made");
  assert.deepEqual(errors, []);
  assert.deepEqual(await page.getByRole("alert").allTextContents(), []);
  console.log(`PASS ${browserName}: help → CSV import → GCSP check/connect/sync → identity confirm → unlinked user details → Agent understands a non-contact → source facts → list editor → prepare via Agent terminal + MCP → review approve / do-not-contact; 0 sends`);
} catch (error) {
  console.error("UI state:", (await page.locator("body").innerText()).slice(0, 3000));
  await page.screenshot({ path: path.join(root, `target/outreach-failed-${browserName}.png`) });
  throw error;
} finally {
  await browser.close();
  await server.close();
  native.kill();
  mock.kill();
  await rm(folder, { recursive: true, force: true });
  await rm(scratch, { recursive: true, force: true });
}
