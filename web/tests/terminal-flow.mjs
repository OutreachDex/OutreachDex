import assert from "node:assert/strict";
import { openContact, openAgent, openDossier, nav } from "./ui.mjs";
import path from "node:path";

export async function realTerminalFlow({
  page,
  app,
  root,
  folder,
  browserName,
}) {
  await page.waitForFunction(
    (workspace) =>
      document
        .querySelector(".ws summary")
        ?.getAttribute("title") === workspace,
    folder,
  );
  const nonce = `terminal-${crypto.randomUUID()}`;
  const { slug: initialSlug } = await app("contact.create", {
    Name: "Initial Terminal Context",
  });
  const { slug } = await app("contact.create", {
    Name: "Synthetic Terminal Check",
    Hook: `Synthetic contact check code: ${nonce}`,
  });
  await openContact(page, "Initial Terminal Context");
  await openAgent(page);
  const dock = page.getByRole("complementary", { name: "Agent workbench" });
  await dock.getByRole("button", { name: "New session", exact: true }).click();
  await dock.locator(".agent-terminal").waitFor();
  const { id } = (await app("terminal.list"))[0];
  try {
    await page.waitForTimeout(8000);
    const startup = await dock.locator(".agent-terminal").innerText();
    if (startup.includes("Trust this folder?")) {
      // This newly-created workspace contains only our synthetic contact.
      await dock.locator(".xterm-helper-textarea").focus();
      await page.keyboard.press("Enter");
      await page.waitForTimeout(4000);
    }
    console.log(
      "CLI ready:",
      await dock.locator(".agent-terminal").innerText(),
    );
    await dock.locator(".xterm-helper-textarea").focus();
    if (
      (await dock.locator(".agent-terminal").innerText()).includes("1 warning")
    ) {
      await page.keyboard.press("F2");
      await page.waitForTimeout(500);
      console.log(
        "CLI warning:",
        await dock.locator(".agent-terminal").innerText(),
      );
      await page.keyboard.press("Escape");
      await page.waitForTimeout(300);
    }
    await openContact(page, "Synthetic Terminal Check");
    await page.waitForFunction(
      async ({ folder, slug }) => {
        const context = await window.nativeInvoke("app_call", {
          root: folder,
          action: "context.get",
          args: {},
        });
        return context.contact === slug;
      },
      { folder, slug },
    );
    await dock.locator(".xterm-helper-textarea").focus();
    await page.keyboard.insertText(
      "This is an integration test with a synthetic contact. I switched the current contact. First use OutreachDex MCP context.get to confirm who I am looking at, then read only this person with contact.get, take the check code from Hook, and use dossier.write_section to replace this person's analysis section with “CLI verified: ” followed by the check code. Do not change anyone else, do not run shell commands, do not go online, and do not create drafts or send messages. Report briefly when done.",
    );
    // insertText is an unbracketed burst; allow the CLI's paste detector to
    // finish before pressing Enter (otherwise Enter becomes part of the paste).
    await page.waitForTimeout(800);
    await page.keyboard.press("Enter");
    let verified = false;
    for (let i = 0; i < 240; i++) {
      const text = (await app("note.read", { path: `people/${slug}.md` })).text;
      if (text.includes(`CLI verified: ${nonce}`)) {
        verified = true;
        break;
      }
      const screen = (
        await dock.locator(".agent-terminal").innerText()
      ).replace(/\s+/g, " ");
      if (
        screen.includes(
          'Allow the globalcontact MCP server to run tool "context.get"?',
        ) ||
        (screen.includes(`slug: ${slug}`) &&
          ["contact.get", "dossier.write_section"].some((tool) =>
            screen.includes(
              `Allow the globalcontact MCP server to run tool "${tool}"?`,
            ),
          ))
      ) {
        // Approve only context.get and these two tools on this synthetic contact, one call at a
        // time. Never choose session-wide or persistent approval.
        await dock.locator(".xterm-helper-textarea").focus();
        await page.keyboard.press("Enter");
      }
      if (i > 0 && i % 30 === 0)
        console.log(
          `CLI at ${i}s:`,
          await dock.locator(".agent-terminal").innerText(),
        );
      await page.waitForTimeout(1000);
    }
    console.log(
      "CLI result:",
      await dock.locator(".agent-terminal").innerText(),
    );
    assert(
      verified,
      "Real Codex must read the nonce via MCP and write it to the synthetic dossier",
    );
    assert.equal((await app("draft.list", { slug })).length, 0);
    assert(
      !(
        await app("note.read", { path: `people/${initialSlug}.md` })
      ).text.includes(nonce),
    );
    console.log(
      "PASS real Codex CLI → switch contact → context.get → contact.get → dossier.write_section; synthetic workspace only, no drafts or sends.",
    );
    await page.screenshot({
      path: path.join(root, `target/agent-real-terminal-${browserName}.png`),
    });
  } finally {
    await app("terminal.stop", { id }).catch(() => {});
  }
}

export async function terminalFlow({
  page,
  app,
  root,
  folder,
  browserName,
  errors,
}) {
  await page.waitForFunction(
    (workspace) =>
      document
        .querySelector(".ws summary")
        ?.getAttribute("title") === workspace,
    folder,
  );
  const { slug } = await app("contact.create", {
    Name: "Terminal User",
    Hook: "Wants a better import experience",
  });
  const { slug: otherSlug } = await app("contact.create", {
    Name: "Another Person",
  });
  const config = (await app("settings.get")).config;
  config.terminal = {
    codex: path.join(root, "crates/gc-service/tests/terminal_agent.py"),
  };
  await app("settings.save", config);
  await openContact(page, "Terminal User");
  await openDossier(page);
  const dossier = page.frameLocator(`iframe[title="people/${slug}.md"]`);
  await dossier.locator("canvas").first().waitFor();
  await openAgent(page);
  const dock = page.getByRole("complementary", { name: "Agent workbench" });
  await dock.getByRole("button", { name: "New session", exact: true }).click();
  const output = async (text) =>
    page.waitForFunction(
      (expected) =>
        document
          .querySelector(".agent-terminal")
          ?.textContent.includes(expected),
      text,
    );
  await output("fixture READY");
  const id = (await app("terminal.list"))[0].id;
  const type = async (text) => {
    await dock.locator(".xterm-helper-textarea").focus();
    await page.keyboard.insertText(text);
    await page.keyboard.press("Enter");
  };
  const currentSession = () => dock.getAttribute("data-session");
  const sessionName = () => dock.locator(".agent-workspace-name").innerText();
  const openFile = async (file, query = "") => {
    await dock.getByRole("button", { name: "Workspace documents", exact: true }).click();
    await dock.getByLabel("Search workspace documents").fill(query);
    await dock.getByRole("button", { name: file, exact: true }).click();
  };
  const closeFile = async () => {
    await page.getByRole("button", { name: "Save and close workspace document" }).click();
    await page.locator(".agent-document").waitFor({ state: "detached" });
  };
  const expectContext = async (slug) =>
    page.waitForFunction(
      async ({ folder, slug }) => {
        const value = await window.nativeInvoke("app_call", {
          root: folder,
          action: "context.get",
          args: {},
        });
        return value.contact === slug;
      },
      { folder, slug },
    );
  const fixedLayout = async () => {
    const main = await page.locator(".main").boundingBox();
    const agent = await dock.boundingBox();
    // The center shows the person page or a workspace document; either must stay readable.
    const detail = await page.locator(".rec-main, .agent-document").first().boundingBox();
    assert(
      main.x + main.width <= agent.x + 1,
      "Agent must not overlay the main workspace",
    );
    // Contacts stay reachable beside the Agent: the people list and the J / K stepper.
    await page.getByRole("navigation", { name: "Main navigation" }).getByRole("button", { name: /^People/ }).waitFor();
    assert(detail.width >= 280, "contact details must remain readable");
    assert.equal(Math.round(agent.x + agent.width), page.viewportSize().width);
    assert.equal(Math.round(agent.y), 0);
    assert(
      await page
        .locator(".app")
        .evaluate((el) => el.scrollWidth <= el.clientWidth),
    );
  };
  await fixedLayout();
  await expectContext(slug);
  await type("context");
  await output('"name": "Terminal User"');
  await openContact(page, "Terminal User");
  await openDossier(page);
  // Move an already-open dossier into the workbench. There must be one owner.
  await openFile(`people/${slug}.md`);
  await dossier.locator("canvas").first().waitFor();
  const resize = page.getByRole("separator", { name: "Resize Agent panel" });
  const originalWidth = (await dock.boundingBox()).width;
  const dragWidth = async (delta) => {
    const box = await resize.boundingBox();
    await page.mouse.move(box.x + box.width / 2, 400);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 - delta, 400, { steps: 12 });
    await page.mouse.up();
    await page.locator(".agent-resize-shield").waitFor({ state: "detached" });
  };
  // Drag crosses the live kit iframe; pointer capture and shield must keep working.
  await dragWidth(180);
  assert((await dock.boundingBox()).width >= originalWidth + 160);
  await fixedLayout();
  await type("size");
  await output("PTY COLUMNS");
  const firstSize = Number(
    (await dock.locator(".agent-terminal").innerText()).match(
      /PTY COLUMNS (\d+)/,
    )?.[1],
  );
  await dragWidth(-100);
  assert((await dock.boundingBox()).width <= originalWidth + 90);
  await type("size");
  await page.waitForFunction((before) => {
    const sizes = [
      ...(
        document.querySelector(".agent-terminal")?.textContent || ""
      ).matchAll(/PTY COLUMNS (\d+)/g),
    ];
    return sizes.length >= 2 && Number(sizes.at(-1)[1]) < before;
  }, firstSize);
  await resize.focus();
  await page.keyboard.press("End");
  await fixedLayout();
  await page.keyboard.press("Home");
  assert.equal(Math.round((await dock.boundingBox()).width), 330);
  await resize.dblclick();
  assert(Math.abs((await dock.boundingBox()).width - originalWidth) < 2);
  await dragWidth(160);
  await type("permission");
  await output("Allow synthetic test action?");
  assert(
    !(await dock.locator(".agent-terminal").innerText()).includes(
      "ENTER CONFIRMED",
    ),
    "resizing and task selection must never confirm a permission",
  );
  await dock.getByRole("button", { name: "Send Enter to the current terminal" }).click();
  await output("ENTER CONFIRMED");
  await type("size");
  await page.waitForFunction(() =>
    /PTY COLUMNS \d+/.test(
      document.querySelector(".agent-terminal")?.textContent || "",
    ),
  );
  await page.screenshot({
    path: path.join(root, `target/agent-resized-${browserName}.png`),
  });
  assert.equal(
    await page.locator(`iframe[title="people/${slug}.md"]`).count(),
    1,
  );
  await dock.getByRole("button", { name: "Hide Agent workbench" }).click();
  await openContact(page, "Terminal User");
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  await openDossier(page);
  await dossier.locator("canvas").first().waitFor();
  assert.equal(
    await page.locator(`iframe[title="people/${slug}.md"]`).count(),
    1,
  );
  await openAgent(page);
  await openFile(`people/${slug}.md`);
  await dossier.locator("canvas").first().waitFor();
  await closeFile();
  await type("中文 🐋");
  await output("ECHO: 中文 🐋");
  await dock.getByRole("button", { name: "Sessions", exact: true }).click();
  await dock.getByRole("button", { name: "Rename current session" }).click();
  await dock.getByLabel("Session name").fill("Import feedback follow-up");
  await dock.getByRole("button", { name: "Save name", exact: true }).click();
  await dock.getByRole("button", { name: "Sessions", exact: true }).click();
  assert((await sessionName()).includes("Import feedback follow-up"));
  // Capture clipboard writes locally instead of replacing the user's clipboard.
  await page.evaluate(() =>
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text) => {
          window.testClipboard = text;
        },
      },
    }),
  );
  await dock.getByRole("button", { name: "Copy log", exact: true }).click();
  // Copying goes through the host first, then the browser API; wait for it to land.
  await page.waitForFunction(() => window.testClipboard?.includes("ECHO: 中文 🐋"));
  await dock.getByRole("button", { name: "Copied", exact: true }).waitFor();
  await dock.getByRole("button", { name: "Hide Agent workbench" }).click();
  await page
    .locator('.side-foot .count[title$="sessions still connected"]')
    .filter({ hasText: "1" })
    .waitFor();
  await app("terminal.write", { id, text: "still running\n" });
  await page.keyboard.press("Control+j");
  await output("ECHO: still running");
  await type("report");
  await output("MCP OK document.create");
  await dock.getByRole("button", { name: "Workspace documents", exact: true }).click();
  await dock.getByLabel("Search workspace documents").fill("missing-note");
  await dock.getByText("No matching documents. Try other keywords.").waitFor();
  await dock.getByLabel("Search workspace documents").fill("feedback");
  await dock
    .getByRole("button", { name: "notes/feedback.md", exact: true })
    .click();
  const note = page.frameLocator('iframe[title="notes/feedback.md"]');
  await note.locator("canvas").first().waitFor();
  assert.equal(await page.getByRole("dialog").count(), 0);
  const termRect = await dock.locator(".agent-conversation").boundingBox(),
    docRect = await page.locator(".agent-document").boundingBox();
  assert(
    docRect.x + docRect.width <= termRect.x + 1,
    "document and terminal must not cover each other",
  );
  await note.locator("canvas").first().click();
  await page.keyboard.press("Meta+j");
  await dock.waitFor({ state: "hidden" });
  await page.keyboard.press("Meta+j");
  await dock.waitFor({ state: "visible" });
  // Issue a task while the editor is visible beside the terminal.
  await type("revise");
  await output("MCP OK document.replace");
  assert(
    (await app("note.read", { path: "notes/feedback.md" })).text.includes(
      "Confirm the file format",
    ),
  );
  await dock.getByRole("button", { name: "Find in log", exact: true }).click();
  await dock.getByLabel("Search terminal log").fill("MCP OK document.replace");
  await dock
    .locator(".agent-search-result")
    .filter({ hasText: /^Found/ })
    .waitFor();
  await dock.getByLabel("Search terminal log").fill("not-in-this-transcript");
  await dock
    .locator(".agent-search-result")
    .filter({ hasText: "Not found" })
    .waitFor();
  await dock.getByRole("button", { name: "Close log search" }).click();
  await dock.getByRole("button", { name: "Jump to latest", exact: true }).click();
  await page.screenshot({
    path: path.join(root, `target/workbench-split-${browserName}.png`),
  });
  await page.setViewportSize({ width: 900, height: 650 });
  await page.evaluate(
    () =>
      new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );
  await note.locator("canvas").first().waitFor();
  assert(await dock.locator(".agent-terminal").isVisible());
  await fixedLayout();
  await page.waitForTimeout(1000);
  await page.screenshot({
    path: path.join(
      root,
      `target/workbench-document-narrow-${browserName}.png`,
    ),
  });
  await type("after document switch");
  await output("ECHO: after document switch");
  assert.equal(
    await page.locator('iframe[title="notes/feedback.md"]').count(),
    1,
    "switching views keeps editor mounted",
  );
  await page.screenshot({
    path: path.join(
      root,
      `target/workbench-terminal-narrow-${browserName}.png`,
    ),
  });
  assert(await dock.evaluate((el) => el.scrollWidth <= el.clientWidth));
  await closeFile();
  await openFile(`people/${slug}.md`, "people/");
  await dossier.locator("canvas").first().waitFor();
  await type(`dossier ${slug}`);
  await output("MCP OK dossier.write_section");
  assert(
    !(await app("note.read", { path: `people/${slug}.md` })).text.includes(
      `dossier ${slug}`,
    ),
    "terminal input must never leak into the kit document",
  );
  assert(
    (await app("note.read", { path: `people/${slug}.md` })).text.includes(
      "The terminal Agent organized the user's import feedback.",
    ),
  );
  await closeFile();
  // A quick task pins its contact even if selection changes before execution.
  await page.setViewportSize({ width: 1440, height: 1000 });
  await dock.getByRole("button", { name: "Draft reply", exact: true }).click();
  await page.waitForFunction(
    (old) => document.querySelector(".agent-dock")?.dataset.session !== old,
    id,
  );
  const other = (await app("terminal.list")).find((s) => s.id !== id).id;
  await output(`TASK READY ${slug} reply`);
  await openContact(page, "Another Person");
  await expectContext(otherSlug);
  assert(await dock.isVisible());
  await type("context");
  await output('"name": "Another Person"');
  await type("task");
  await output(`TASK DONE ${slug}`);
  const drafts = await app("draft.list", { slug });
  assert.equal(drafts.length, 1);
  assert.equal(drafts[0].metadata.status, "draft");
  assert.equal((await app("draft.list", { slug: otherSlug })).length, 0);
  // The reply task opened its draft for review (ui.open); the terminal kept the keyboard.
  await page.getByRole("dialog").waitFor();
  assert(await page.evaluate(() => !!document.activeElement?.closest(".agent-dock")));
  await page.getByRole("button", { name: "Close dialog" }).click();
  // "These people": the Agent sees who is picked in bulk on the People page.
  await nav(page, "People");
  await page.getByLabel("Select Terminal User").check();
  await page.getByLabel("Select Another Person").check();
  // The selection reaches the app asynchronously, like the current contact (expectContext).
  await page.waitForFunction(
    async (folder) =>
      (await window.nativeInvoke("app_call", { root: folder, action: "context.get", args: {} })).selection_count === 2,
    folder,
  );
  await type("context");
  await output('"selection_count": 2');
  await openContact(page, "Terminal User");
  await page.getByRole("tab", { name: /Drafts/ }).click();
  await page.getByText("Fixture reply", { exact: true }).waitFor();
  assert(await dock.isVisible());
  await page.screenshot({
    path: path.join(root, `target/agent-fixed-reply-${browserName}.png`),
  });
  await app("terminal.stop", { id: other });
  await dock.getByRole("button", { name: "Understand", exact: true }).click();
  await output(`TASK READY ${slug} overview`);
  await type("task");
  await output(`TASK DONE ${slug}`);
  const overview = (await app("terminal.list")).find(
    (s) => s.id !== id && s.id !== other,
  );
  await app("terminal.stop", { id: overview.id });
  await dock.getByRole("button", { name: "Sessions", exact: true }).click();
  await dock.locator(`.agent-session-list button[data-id="${id}"]`).click();
  await output("MCP OK dossier.write_section");
  await type("exit");
  await output("SESSION FINISHED");
  const savedWidth = (await dock.boundingBox()).width;
  await page.reload();
  await openAgent(page);
  await output("SESSION FINISHED");
  assert(
    Math.abs((await dock.boundingBox()).width - savedWidth) < 2,
    "panel width persists after reload",
  );
  assert.equal(await currentSession(), id);
  assert((await sessionName()).includes("Import feedback follow-up"));
  const history = [];
  let historyAfter = 0;
  for (;;) {
    const { chunks } = await app("terminal.read", { id, after: historyAfter });
    if (!chunks.length) break;
    history.push(...chunks.flatMap((c) => c.bytes));
    historyAfter = chunks.at(-1).seq;
  }
  assert(
    Buffer.from(history).toString().includes("MCP OK dossier.write_section"),
  );
  assert(
    !Buffer.from(history).toString().includes("Current OutreachDex context"),
    "copy context must not inject keystrokes or authorize terminal prompts",
  );
  await dock.getByRole("region", { name: "Session ended notice" }).waitFor();
  assert.equal(
    await dock.getByRole("button", { name: "Send Enter to the current terminal" }).count(),
    0,
  );
  const previousSession = await currentSession();
  await dock.getByRole("button", { name: "Start an interactive session" }).click();
  await page.waitForFunction(
    (previous) =>
      document.querySelector(".agent-dock")?.dataset.session !== previous,
    previousSession,
  );
  await output("fixture READY");
  await type("permission");
  await output("Allow synthetic test action?");
  const ended = await currentSession();
  await app("terminal.stop", { id: ended });
  await dock.getByRole("region", { name: "Session ended notice" }).waitFor();
  const banner = await dock.locator(".agent-history-notice").boundingBox();
  const terminalBox = await dock.locator(".agent-terminal").boundingBox();
  assert(
    banner.y < terminalBox.y,
    "read-only notice must precede the old permission prompt",
  );
  await page.evaluate(() => {
    const invoke = window.nativeInvoke;
    window.testTerminalWrites = [];
    window.nativeInvoke = (command, args) => {
      if (command === "app_call" && args.action === "terminal.write")
        window.testTerminalWrites.push(args.args);
      return invoke(command, args);
    };
  });
  await dock.locator(".xterm-helper-textarea").focus();
  await page.keyboard.press("Enter");
  assert.deepEqual(await page.evaluate(() => window.testTerminalWrites), []);
  await page.screenshot({
    path: path.join(root, `target/agent-history-readonly-${browserName}.png`),
  });
  await dock.getByRole("button", { name: "Start an interactive session" }).click();
  await page.waitForFunction(
    (previous) =>
      document.querySelector(".agent-dock")?.dataset.session !== previous,
    ended,
  );
  await output("fixture READY");
  await type("keyboard works");
  await output("ECHO: keyboard works");
  await type("permission");
  await output("Allow synthetic test action?");
  await page.keyboard.press("Enter");
  await output("ENTER CONFIRMED");
  const fresh = await currentSession();
  assert.notEqual(fresh, ended);
  assert(
    (await page.evaluate(() => window.testTerminalWrites)).every(
      (write) => write.id === fresh,
    ),
  );
  await app("terminal.stop", { id: fresh });
  // At the running-session limit, a new task offers to end the oldest session instead of failing.
  const running = async () => (await app("terminal.list")).filter((s) => s.status === "running");
  while ((await running()).length < 6)
    await app("terminal.start", { agent: "codex", task: "", contact: null, document: null, cols: 80, rows: 24 });
  const oldest = (await running()).sort((a, b) => a.created_at - b.created_at)[0].id;
  // Sync-and-summarise is an Agent task too: it runs the sync tools through MCP, then reports.
  await dock.getByRole("button", { name: "Sync & summarize", exact: true }).click();
  await page.waitForFunction(
    (previous) =>
      document.querySelector(".agent-dock")?.dataset.session !== previous,
    fresh,
  );
  await output("SYNC READY");
  await type("sync");
  await output("SYNC DONE");
  assert((await sessionName()).includes("Sync and summarize"));
  const after = await running();
  assert.equal(after.length, 6, "still at the limit: one ended, one started");
  assert(!after.some((s) => s.id === oldest), "the oldest session was ended");
  for (const s of after) await app("terminal.stop", { id: s.id });
  assert.deepEqual(errors, []);
  assert.deepEqual(await page.getByRole("alert").allTextContents(), []);
  console.log(
    `PASS ${browserName}: draggable pane across iframe → PTY resize → keyboard/button Enter → readonly permission history → new interactive session → width persistence; fixed right pane → central document editing → live contact context → pinned reply/overview/sync tasks → draft only → bulk selection in context → session limit offers ending the oldest → 900px nonoverlap → history restoration → real PTY/MCP`,
  );
}
