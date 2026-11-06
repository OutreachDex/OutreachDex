// Every page in English (the source language), Chinese, Japanese and Korean against the real Rust
// host and the sample workspace: screenshots, plus a check that no visible UI text is left
// untranslated (in English: no visible text is still Chinese UI copy).
// Usage: node web/tests/i18n.mjs [out-dir]   (needs target/debug/examples/host and target/test-workspace)
import { spawn } from "node:child_process";
import { mkdtemp, realpath, cp, mkdir, writeFile, readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { tmpdir } from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

const root = fileURLToPath(new URL("../..", import.meta.url));
const out = path.resolve(process.argv[2] || path.join(root, "target/i18n"));
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
const browser = await chromium.launch();
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
await page.addInitScript(() => {
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


const dictionaries = Object.fromEntries(
  await Promise.all(["zh", "ja", "ko"].map(async (l) => [l, JSON.parse(await readFile(path.join(root, `web/src/locales/${l}.json`), "utf8"))])),
);
// English is the source: flag any visible text that is still a Chinese translation instead.
dictionaries.en = Object.fromEntries(Object.values(dictionaries.zh).map((zh) => [zh, `(Chinese) ${zh}`]));
// The language picker names each language in itself.
delete dictionaries.en["中文"];
// Visible text nodes and accessible labels that equal a source key with a different translation.
const untranslated = (dict) =>
  page.evaluate((dict) => {
    const found = new Set();
    const check = (text) => {
      const value = (text || "").trim();
      if (value && dict[value] && dict[value] !== value) found.add(value);
    };
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const node = walker.currentNode;
      if (node.parentElement?.closest(".xterm, iframe, .help-body, script, style")) continue;
      if (node.parentElement && node.parentElement.offsetParent === null) continue;
      check(node.textContent);
    }
    for (const el of document.querySelectorAll("[aria-label], [title], [placeholder]"))
      for (const attr of ["aria-label", "title", "placeholder"]) {
        // A profile field's tooltip is its raw column name (data, as the agent and CSV see it).
        if (attr === "title" && el.matches(".lab")) continue;
        check(el.getAttribute(attr));
      }
    return [...found];
  }, dict);
const problems = {};
const visit = async (lang, name) => {
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(out, `${lang}-${name}.png`) });
  const left = await untranslated(dictionaries[lang]);
  if (left.length) problems[`${lang}/${name}`] = left;
};
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`);
  for (const lang of ["en", "zh", "ja", "ko"]) {
    const tr = (en) => (lang === "en" ? en : dictionaries[lang][en] || en);
    await page.evaluate((l) => localStorage.setItem("gc.lang", l), lang);
    await page.reload();
    await page.waitForTimeout(800);
    assert.equal(await page.evaluate(() => document.documentElement.lang.slice(0, 2)), lang);
    if (await page.locator(".welcome").count()) {
      await visit(lang, "welcome");
      await page.locator(".ws summary").click();
      await page.getByRole("button", { name: tr("Open workspace…") }).click();
    }
    await page.locator(".today h1").waitFor();
    await visit(lang, "today");
    const navTo = (en) => page.locator("aside.side").getByRole("button", { name: new RegExp(`^${tr(en)}`) }).first().click();
    await navTo("People");
    await page.locator("table.grid tbody tr").first().waitFor();
    await visit(lang, "people");
    await page.locator("table.grid tbody tr").nth(2).click();
    await page.locator(".rec-head h1").waitFor();
    await visit(lang, "person");
    await page.getByRole("tab", { name: new RegExp(tr("Drafts")) }).click();
    await visit(lang, "person-drafts");
    await page.getByRole("tab", { name: new RegExp(tr("Activity")) }).click();
    await visit(lang, "person-timeline");
    let help = "";
    for (const [label, name] of [["Lists", "lists"], ["Mail", "mail"], ["Sources", "sources"], ["Help", "help"], ["Settings", "settings"]]) {
      await navTo(label);
      await visit(lang, name);
      if (name === "help") help = (await page.locator(".help-body").innerText()).split("```")[0].slice(0, 400);
    }
    if (lang === "en") assert(!/[\u4e00-\u9fff]{4,}/.test(help), "help article is in English");
    if (lang === "zh") assert(/[\u4e00-\u9fff]{4,}/.test(help), "help article is in Chinese");
    await page.getByLabel(tr("Interface language")).waitFor();
  }
  // Back to English through the real control in Settings.
  await page.getByLabel(dictionaries.ko["Interface language"]).selectOption("en");
  await page.waitForFunction(() => document.documentElement.lang === "en-US");
  await page.getByRole("navigation").getByRole("button", { name: /^Today/ }).waitFor();
  for (const [where, left] of Object.entries(problems)) console.log(`untranslated on ${where}: ${left.join(" | ")}`);
  assert.deepEqual(problems, {}, "all visible UI text is translated");
  console.log(`PASS i18n: en / zh / ja / ko on today, people, person (overview, drafts, activity), lists, mail, sources, help, settings; switch back to English. Screenshots in ${out}`);
} finally {
  await browser.close();
  await server.close();
  native.kill();
  mock.kill();
}
