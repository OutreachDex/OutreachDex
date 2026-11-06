// Actual Rust application, actual table/note iframe artifacts. Only desktop IPC/dialogs are replaced.
import assert from "node:assert/strict";
import { openContact, openAgent, openDossier, nav } from "./ui.mjs";
import { spawn } from "node:child_process";
import { mkdtemp, realpath, rm, cp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { chromium, webkit } from "playwright";
import { createServer } from "vite";
import { terminalFlow, realTerminalFlow } from "./terminal-flow.mjs";
const root = fileURLToPath(new URL("../..", import.meta.url));
const folder = await realpath(
  await mkdtemp(path.join(tmpdir(), "globalcontact-flow-")),
);
const sample = process.argv.includes("--sample");
if (sample)
  await cp(path.join(root, "target/test-workspace"), folder, {
    recursive: true,
  });
const native = spawn(path.join(root, "target/debug/examples/host"), {
  stdio: ["pipe", "pipe", "inherit"],
  env: {
    ...process.env,
    GC_TEST_DESKTOP_BINARY: path.join(
      root,
      "target/debug/bundle/macos/OutreachDex.app/Contents/MacOS/globalcontact",
    ),
  },
});
const pending = new Map();
let seq = 0,
  page,
  eventCount = 0;
const lines = createInterface({ input: native.stdout });
lines.on("line", (line) => {
  const answer = JSON.parse(line);
  if (answer.event) {
    if (answer.event === "document-request") eventCount++;
    if (
      holdUpdate &&
      answer.event === "agent-update" &&
      answer.payload.update.sessionUpdate === "agent_message_chunk"
    ) {
      heldUpdate = answer;
      return;
    }
    void page
      ?.evaluate((data) => window.dispatchNative?.(data), answer)
      .catch(() => {});
    return;
  }
  const request = pending.get(answer.id);
  pending.delete(answer.id);
  if (!request) return;
  if (answer.error) request.reject(new Error(answer.error));
  else request.resolve(answer.result);
});
native.on("error", (e) => {
  for (const request of pending.values()) request.reject(e);
});
native.on("exit", (code) => {
  for (const request of pending.values())
    request.reject(new Error(`host exited ${code}`));
});
function call(command, args) {
  return new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, { resolve, reject });
    native.stdin.write(`${JSON.stringify({ id, command, args })}\n`);
  });
}
const app = (action, args = {}) =>
  call("app_call", { root: folder, action, args });
const server = await createServer({
  root: path.join(root, "web"),
  configFile: path.join(root, "web/vite.config.ts"),
  server: { host: "127.0.0.1", port: 0 },
  logLevel: "error",
});
await server.listen();
const browserName = process.argv[2] === "webkit" ? "webkit" : "chromium";
const browser = await { chromium, webkit }[browserName].launch();
page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
page.setDefaultTimeout(60000);
const errors = [];
let holdAction = "",
  releaseResponse;
let heldUpdate = null,
  holdUpdate = false;
page.on("pageerror", (e) => { errors.push(e.message); console.error(e.stack || e.message); });
await page.exposeFunction("nativeInvoke", async (command, args) => {
  if (command === "plugin:dialog|open") return folder;
  if (command === "plugin:dialog|confirm") return true;
  // Tauri v2's confirm() goes through the message command and compares the button label.
  if (command === "plugin:dialog|message") return "Ok";
  const result = await call(command, args);
  if (command === "app_call" && args.action === holdAction) {
    holdAction = "";
    await new Promise((resolve) => {
      releaseResponse = resolve;
    });
  }
  return result;
});
// The suites assert English labels; the browser's own locale would otherwise pick the UI language.
await page.addInitScript(() => {
  if (!localStorage.getItem("gc.lang")) localStorage.setItem("gc.lang", "en");

  const callbacks = new Map(),
    listeners = new Map();
  let next = 1;
  window.dispatchNative = ({ event, payload }) => {
    for (const [id, entry] of listeners)
      if (entry.event === event)
        callbacks.get(entry.handler)?.({ event, payload, id });
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (_, id) => listeners.delete(id),
  };
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
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main" },
    },
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
  await page.locator(".ws summary").click();
  await page
    .getByRole("button", {
      name: sample ? "Open workspace…" : "New workspace…",
      exact: true,
    })
    .first()
    .click();
  // A new workspace starts from a template (blank by default), then the folder is chosen.
  if (!sample) await page.getByRole("button", { name: "Choose folder…", exact: true }).click();
  if (process.argv.includes("--real-terminal")) {
    await realTerminalFlow({ page, app, root, folder, browserName, errors });
  } else if (process.argv.includes("--terminal")) {
    await terminalFlow({ page, app, root, folder, browserName, errors });
  } else if (sample) {
    await page.locator(".today h1").waitFor();
    const contacts = await app("contacts.list");
    assert(contacts.length >= 21);
    let chosen;
    for (const contact of contacts) {
      const info = await app("contact.get", { slug: contact.values.Slug });
      if (info.drafts.length >= 1) {
        chosen = info;
        break;
      }
    }
    assert(chosen, "sample should contain imported drafts");
    await openContact(page, chosen.contact.values.Name);
    await page.locator(".rec-head h1").filter({ hasText: chosen.contact.values.Name }).waitFor();
    await page.screenshot({
      path: path.join(root, `target/redesign-sample-${browserName}.png`),
    });
    await page.getByRole("button", { name: "Write draft", exact: true }).click();
    await page
      .getByRole("dialog")
      .getByLabel("Subject", { exact: true })
      .fill("Short version · research chat invite");
    await page
      .getByRole("dialog")
      .getByLabel("Body")
      .fill(
        "Hello, I read your research on agent tool interfaces and found it very helpful. We are working on agent interfaces for professional documents and would value your advice on our experiment design. Would you be open to a short conversation?",
      );
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Save draft", exact: true })
      .click();
    await page.getByRole("button", { name: "Close dialog" }).click();
    await page.getByRole("tab", { name: /^Drafts/ }).click();
    await page.locator(".draft-card").filter({ hasText: "Short version" }).click();
    await page.getByLabel("Compare draft").selectOption({ index: 1 });
    await page.locator(".reference-draft").waitFor();
    await page.screenshot({
      path: path.join(root, `target/redesign-review-${browserName}.png`),
    });
    await page.setViewportSize({ width: 900, height: 650 });
    // WebKit applies a viewport resize asynchronously; inspect the settled layout.
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    assert(
      await page
        .locator(".modal-body")
        .evaluate((el) => el.scrollWidth <= el.clientWidth),
    );
    await page.screenshot({
      path: path.join(root, `target/redesign-review-narrow-${browserName}.png`),
    });
    await page.getByRole("button", { name: "Close dialog" }).click();
    await page.getByRole("tab", { name: "Overview", exact: true }).click();
    assert(
      await page
        .locator(".rec-main")
        .evaluate((el) => el.scrollWidth <= el.clientWidth),
    );
    await page.locator(".rec-main").evaluate((el) => {
      el.scrollTop = 0;
    });
    await page.screenshot({
      path: path.join(root, `target/redesign-sample-narrow-${browserName}.png`),
    });
    await openAgent(page);
    for (const width of [900, 1440]) {
      await page.setViewportSize({ width, height: width === 900 ? 650 : 1000 });
      await page.evaluate(
        () =>
          new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          ),
      );
      const main = await page.locator(".main").boundingBox();
      const dock = await page.locator(".agent-dock").boundingBox();
      assert(
        main.x + main.width <= dock.x + 1,
        "fixed Agent never covers the sample contacts",
      );
      assert(
        await page
          .locator(".rec-main")
          .evaluate((el) => el.scrollWidth <= el.clientWidth),
        "real contact details must fit next to Agent",
      );
      await page.screenshot({
        path: path.join(
          root,
          `target/agent-fixed-sample-${width}-${browserName}.png`,
        ),
      });
    }
    assert.deepEqual(errors, []);
    assert.deepEqual(await page.getByRole("alert").allTextContents(), []);
    console.log(
      `PASS ${browserName}: real ${contacts.length}-contact workspace, draft comparison, 1440px and 900px layouts including fixed Agent`,
    );
  } else {
    await nav(page, "People");
    await page.getByRole("button", { name: "New contact", exact: true }).click();
    const modal = page.getByRole("dialog", { name: "New contact" });
    await modal.getByLabel("Name", { exact: true }).fill("Elena Park");
    await modal.getByLabel("Affiliation", { exact: true }).fill("NUS");
    await modal.getByLabel("Email", { exact: true }).fill("expert@example.test");
    await modal.getByRole("button", { name: "Save", exact: true }).click();
    await page.locator(".rec-head h1").filter({ hasText: "Elena Park" }).waitFor();
    await nav(page, "People");
    await page.getByRole("button", { name: "Table editor" }).click();
    await page.getByRole("status").filter({ hasText: /^Saved/ }).waitFor();
    const table = page.frameLocator('iframe[title="contacts.ktable"]');
    for (const title of ["contacts", "channels", "identities", "interactions"])
      await table.getByText(title, { exact: true }).first().waitFor();
    await table
      .getByRole("button", { name: "Add record", exact: true })
      .first()
      .click();
    await page.getByRole("status").filter({ hasText: "Unsaved" }).waitFor();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await page.getByRole("status").filter({ hasText: /^Saved/ }).waitFor();
    await openContact(page, "Elena Park");
    const before = eventCount;
    await page.getByLabel("Contact stage").selectOption("In progress");
    await page.waitForFunction(
      () =>
        document.querySelector('[aria-label="Contact stage"]')?.value === "In progress",
    );
    assert.equal(
      (await app("contact.get", { slug: "elena-park" })).contact.values.Stage,
      "In progress",
    );
    assert(
      eventCount > before,
      "host updates must route through the open editor",
    );
    await openDossier(page);
    const noteFrame = page.frameLocator(
      'iframe[title="people/elena-park.md"]',
    );
    await noteFrame.locator("canvas").first().waitFor();
    await call("mcp", {
      workspace: folder,
      method: "tools/call",
      params: {
        name: "dossier.write_section",
        arguments: {
          slug: "elena-park",
          section: "analysis",
          content: "Native host and iframe agree.",
        },
      },
    });
    assert.match(
      (await app("note.read", { path: "people/elena-park.md" })).text,
      /Native host and iframe agree/,
    );
    await page.getByRole("button", { name: "Save and close", exact: true }).click();
    await page.getByRole("button", { name: "Write draft", exact: true }).click();
    const draftModal = page.getByRole("dialog");
    // A recipient that is not one of the person's known addresses is flagged, with a one-click fix.
    const recipient = draftModal.getByLabel("To", { exact: true });
    const known = await recipient.inputValue();
    await recipient.fill("expert@exampel.test");
    await draftModal.getByRole("alert").filter({ hasText: "is not a known address for this person" }).waitFor();
    await draftModal.getByRole("button", { name: `Use ${known}` }).click();
    assert.equal(await recipient.inputValue(), known);
    await draftModal.getByRole("alert").filter({ hasText: "is not a known address for this person" }).waitFor({ state: "hidden" });
    await draftModal.getByLabel("Subject", { exact: true }).fill("Paper feedback");
    // A draft in the other language than the contact's (English, the default) is flagged.
    // Chinese text on purpose: it triggers the language-mismatch hint for this English-language contact.
    await draftModal.locator("textarea").fill("请帮忙看看这篇论文，谢谢你抽时间阅读。");
    await draftModal.getByText("This contact's language is English, but the draft is mostly Chinese.").waitFor();
    await draftModal.locator("textarea").fill("Hi Elena, I read your recent paper and wanted to ask whether you would be open to a short call next week about evaluating agents on office files.");
    await draftModal.getByText("This contact's language is English, but the draft is mostly Chinese.").waitFor({ state: "hidden" });
    await draftModal.getByLabel("Body").press("Escape");
    await page.getByRole("dialog").waitFor({ state: "hidden" });
    await nav(page, "Mail");
    await nav(page, "People");
    await openContact(page, "New page");
    await page.locator(".rec-head h1").filter({ hasText: "New page" }).waitFor();
    await openContact(page, "Elena Park");
    await page.locator(".rec-head h1").filter({ hasText: "Elena Park" }).waitFor();
    await page
      .getByRole("button", { name: "Continue draft", exact: true })
      .click();
    assert.equal(
      await draftModal.getByLabel("Subject", { exact: true }).inputValue(),
      "Paper feedback",
    );
    assert.equal(
      await draftModal.getByLabel("Body").inputValue(),
      "Hi Elena, I read your recent paper and wanted to ask whether you would be open to a short call next week about evaluating agents on office files.",
    );
    // The dialog cycles keyboard focus and never moves to controls underneath it.
    await draftModal.getByLabel("Channel").focus();
    for (let i = 0; i < 14; i++) {
      await page.keyboard.press("Tab");
      assert(
        await page.evaluate(
          () => !!document.activeElement.closest("[role=dialog]"),
        ),
      );
    }
    await draftModal
      .getByRole("button", { name: "Save draft", exact: true })
      .click();
    await draftModal
      .getByRole("button", { name: "Save and approve", exact: true })
      .click();
    await page.getByRole("dialog", { name: "email · Approved" }).waitFor();
    await page.getByRole("button", { name: "Close dialog" }).click();
    await page.getByRole("tab", { name: /^Drafts/ }).click();
    await page
      .locator(".draft-card")
      .filter({ hasText: "Paper feedback" })
      .click();
    await page
      .getByRole("dialog")
      .getByLabel("Body")
      .fill("The edited body needs another review.");
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Send email", exact: true })
      .waitFor({ state: "hidden" });
    await page.getByRole("dialog").getByLabel("Body").press("Escape");
    await page
      .locator(".draft-card")
      .filter({ hasText: "Paper feedback" })
      .click();
    assert.equal(
      await page.getByRole("dialog").getByLabel("Body").inputValue(),
      "The edited body needs another review.",
    );
    await page
      .getByRole("dialog")
      .getByRole("button", { name: "Save and approve", exact: true })
      .click();
    await page.getByRole("dialog", { name: "email · Approved" }).waitFor();
    await page.getByRole("button", { name: "Close dialog" }).click();
    const drafts = await app("draft.list", { slug: "elena-park" });
    assert.equal(drafts[0].metadata.status, "approved");
    await page.getByRole("tab", { name: /Activity/ }).click();
    await page.getByRole("button", { name: "Log info / feedback" }).first().click();
    const intake = page.getByRole("dialog", { name: "Log info / feedback" });
    await intake.getByLabel("Channel", { exact: true }).selectOption("x");
    await intake.getByLabel("Title", { exact: true }).fill("A relevant update");
    await intake.getByLabel("Link or original text").fill("https://example.test/research");
    await intake.getByRole("button", { name: "Save", exact: true }).click();
    await page
      .locator(".tl-item")
      .filter({ hasText: "A relevant update" })
      .waitFor();
    await page.screenshot({
      path: path.join(root, `target/product-${browserName}.png`),
    });
    // Analysis runs in the right-hand Agent terminal. A deterministic PTY fixture drives the
    // real MCP path; it never contacts a model provider, reads private accounts or sends.
    const settings = await app("settings.get");
    await app("settings.save", {
      ...settings.config,
      terminal: {
        codex: path.join(root, "crates/gc-service/tests/terminal_agent.py"),
      },
    });
    assert.equal(
      await page.getByRole("tab", { name: "Analysis", exact: true }).count(),
      0,
      "analysis no longer lives on the left",
    );
    await page.getByRole("button", { name: "Log info / feedback" }).first().click();
    const feedback = page.getByRole("dialog", { name: "Log info / feedback" });
    await feedback.getByLabel("Record type").selectOption("feedback");
    await feedback
      .getByLabel("Title", { exact: true })
      .fill("Problem importing a spreadsheet");
    await feedback.getByLabel("Source link or description").fill("Interview notes · local test");
    await feedback
      .getByLabel("Link or original text")
      .fill("I want to import my existing spreadsheets into OutreachDex but don't know which formats are supported.");
    holdAction = "intake.add";
    await feedback.getByRole("button", { name: "Save and ask the Agent" }).click();
    await page.waitForFunction(
      () => document.querySelector('[role="dialog"] button.primary')?.disabled,
    );
    assert(
      await feedback.getByLabel("Link or original text").isDisabled(),
      "feedback fields must freeze while the submitted copy is being saved",
    );
    for (let i = 0; !releaseResponse && i < 500; i++)
      await new Promise((resolve) => setTimeout(resolve, 20));
    assert(releaseResponse, "the delayed save response should arrive");
    releaseResponse();
    releaseResponse = undefined;
    const dock = page.getByRole("complementary", { name: "Agent workbench" });
    const terminalShows = (text) =>
      page.waitForFunction(
        (expected) =>
          document
            .querySelector(".agent-terminal")
            ?.textContent.includes(expected),
        text,
        { timeout: 20000 },
      );
    const typeInTerminal = async (text) => {
      await dock.locator(".xterm-helper-textarea").focus();
      await page.keyboard.insertText(text);
      await page.keyboard.press("Enter");
    };
    await dock
      .locator(".agent-workspace-name")
      .filter({ hasText: "Understand · Elena Park" })
      .waitFor();
    await terminalShows("TASK READY elena-park overview");
    await terminalShows("NOTE RECEIVED");
    await typeInTerminal("task");
    await terminalShows("TASK DONE elena-park");
    // What the Agent learned is kept in the person's memory and shown on the left.
    await page.getByRole("tab", { name: "Overview", exact: true }).click();
    const memory = page.getByRole("region", { name: "Agent memory" });
    await memory.getByText("cares about the spreadsheet import format").waitFor();
    // The full analysis is saved on the person and rendered, not left in the terminal.
    const saved = page.getByRole("region", { name: "Agent analysis" });
    await saved.getByText("Evidence: 1 · Open questions: 1").waitFor();
    await saved.locator("table td", { hasText: "Format to confirm" }).waitFor();
    assert.equal(await saved.locator("script, img").count(), 0);
    const remembered = await app("memory.get", { slug: "elena-park" });
    assert(remembered.text.includes("Fact · Agent"), remembered.text);
    // Drafting a reply from the person page starts a pinned reply task; it only saves a draft.
    await page.getByRole("button", { name: "Agent drafts reply" }).click();
    await dock
      .locator(".agent-workspace-name")
      .filter({ hasText: "Draft reply · Elena Park" })
      .waitFor();
    await terminalShows("TASK READY elena-park reply");
    // While the Agent works on this person, their page says so and links to the session.
    await typeInTerminal("touch elena-park");
    await terminalShows("MCP OK contact.get");
    const workingBadge = page.getByRole("button", { name: "Agent working" });
    await workingBadge.waitFor();
    await typeInTerminal("task");
    await terminalShows("TASK DONE elena-park");
    const agentDrafts = await app("draft.list", { slug: "elena-park" });
    const fixtureDraft = agentDrafts.find((d) => d.metadata.subject === "Fixture reply");
    assert(fixtureDraft, "the reply task saves a draft");
    assert.equal(fixtureDraft.metadata.status, "draft", "never approved or sent");
    // ui.open shows the draft for review beside the terminal, without taking the keyboard.
    const opened = page.getByRole("dialog");
    await opened.waitFor();
    assert.equal(await opened.getByLabel("Subject", { exact: true }).inputValue(), "Fixture reply");
    await page.waitForTimeout(700);
    assert(
      await page.evaluate(() => !!document.activeElement?.closest(".agent-dock")),
      "the Agent terminal keeps focus when it opens a result",
    );
    const overlay = await page.locator(".overlay").boundingBox();
    assert(overlay.x + overlay.width <= (await dock.boundingBox()).x + 1, "dialogs never cover the Agent pane");
    await typeInTerminal("still typing");
    await terminalShows("ECHO: still typing");
    // The Agent's own status and readable steps, next to the raw terminal.
    const progress = dock.getByRole("region", { name: "Agent progress" });
    await progress.locator(".agent-task.needs_input").waitFor();
    await progress.getByText("Draft opened for review").waitFor();
    await workingBadge.waitFor({ state: "hidden" });
    await progress.locator(".agent-activity-head").click();
    await progress.getByText("Save draft · Elena Park").waitFor();
    await progress.getByText("Read activity · Elena Park").waitFor();
    await page.getByRole("button", { name: "Close dialog" }).click();
    await opened.waitFor({ state: "hidden" });
    await memory.getByRole("button", { name: "Include change log" }).click();
    await memory.getByText("Saved draft for review").waitFor();
    // The session list replaces the old picker; both tasks are listed with the Agent's status.
    await dock.getByRole("button", { name: "Sessions", exact: true }).click();
    assert(
      (await dock.locator(".agent-session-list button").count()) >= 2,
    );
    await dock.locator(".agent-session-list .agent-task.done").waitFor();
    await dock.locator(".agent-session-list .agent-task.needs_input").waitFor();
    await dock.getByRole("button", { name: "Sessions", exact: true }).click();
    // Today lists the Agent's latest outcomes; clicking one returns to its session.
    await nav(page, "Today");
    const results = page.locator(".today section", { hasText: "Recent Agent results" });
    await results.getByText("Understood: cares about the spreadsheet import format").waitFor();
    await results.getByText("Draft opened for review").waitFor();
    await results.getByText("Understood: cares about the spreadsheet import format").click();
    await dock
      .locator(".agent-workspace-name")
      .filter({ hasText: "Understand · Elena Park" })
      .waitFor();
    // Back on the person: the saved analysis and memory are there again.
    await page.getByRole("region", { name: "Agent analysis" }).getByText("Evidence: 1 · Open questions: 1").waitFor();
    await page.getByRole("region", { name: "Agent memory" }).getByText("cares about the spreadsheet import format").waitFor();
    await dock.getByRole("button", { name: "Sessions", exact: true }).click();
    await dock.getByRole("button", { name: "Sessions", exact: true }).click();
    await page.screenshot({
      path: path.join(root, `target/relationship-analysis-${browserName}.png`),
      animations: "disabled",
    });
    // Save and unmount the live kit before reloading the browser transport.
    // A browser reload cannot run React's async document_unregister cleanup.
    await page.locator(".ws summary").click();
    await page.getByRole("button", { name: "Open workspace…", exact: true }).click();
    await page.locator("iframe").waitFor({ state: "detached" });
    await page.reload();
    await openContact(page, "Elena Park");
    await page.locator(".rec-head h1").filter({ hasText: "Elena Park" }).waitFor();
    await page
      .getByRole("region", { name: "Agent memory" })
      .getByText("cares about the spreadsheet import format")
      .waitFor();
    await page.setViewportSize({ width: 900, height: 650 });
    // WebKit applies a viewport resize asynchronously; inspect the settled layout.
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    assert(
      await page
        .locator(".rec-main")
        .evaluate((el) => el.scrollWidth <= el.clientWidth),
    );
    await page.screenshot({
      path: path.join(
        root,
        `target/relationship-analysis-narrow-${browserName}.png`,
      ),
      animations: "disabled",
    });
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("heading", { name: "Mail accounts" }).waitFor();
    await page.getByLabel("Default assistant").selectOption("codex");
    await page.getByLabel("Connection name", { exact: true }).fill("Unfinished account");
    await nav(page, "People");
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    assert.equal(
      await page.getByLabel("Connection name", { exact: true }).inputValue(),
      "Unfinished account",
    );
    await openContact(page, "Elena Park");
    await page.setViewportSize({ width: 900, height: 650 });
    // WebKit applies a viewport resize asynchronously; inspect the settled layout.
    await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    await page.getByRole("tab", { name: "Overview", exact: true }).click();
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    );
    assert(
      await page
        .locator(".rec-main")
        .evaluate((el) => el.scrollWidth <= el.clientWidth),
    );
    await page.screenshot({
      path: path.join(root, `target/product-narrow-${browserName}.png`),
    });
    assert.deepEqual(errors, []);
    const alerts = await page.getByRole("alert").allTextContents();
    assert.deepEqual(alerts, []);
    console.log(
      `PASS ${browserName}: contact → actual table edits → live host routing → actual note section edit → recipient check → language hint → draft approval → feedback → Agent overview/reply tasks via MCP → working badge → ui.open draft keeps terminal focus → task.report status + readable steps → saved analysis card → Today results → memory log → persistence → settings`,
    );
  }
} catch (error) {
  console.error(
    "Overflow:",
    await page.locator(".modal-body").evaluateAll((elements) =>
      elements.map((el) => ({
        width: el.clientWidth,
        scroll: el.scrollWidth,
        rect: el.getBoundingClientRect().toJSON(),
        children: [...el.querySelectorAll("*")]
          .filter(
            (child) =>
              child.scrollWidth > child.clientWidth && child.clientWidth > 0,
          )
          .map((child) => ({
            tag: child.tagName,
            class: child.className,
            width: child.clientWidth,
            scroll: child.scrollWidth,
            rect: child.getBoundingClientRect().toJSON(),
          })),
      })),
    ),
  );
  console.error("UI state:", await page.locator("body").innerText());
  await page.screenshot({
    path: path.join(root, `target/flow-failed-${browserName}.png`),
  });
  throw error;
} finally {
  releaseResponse?.();
  await browser.close();
  await server.close();
  native.stdin.end();
  lines.close();
  // Cleanup must never hide the real failure (WebKit can still be writing into the folder).
  await rm(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }).catch(() => {});
}
