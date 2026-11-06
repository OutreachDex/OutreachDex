// Navigation helpers for the browser flows (new shell: People list → person page; fixed Agent pane).
// Rows ignore clicks while a save or navigation is still running (shown as aria-disabled).
export async function idle(page) {
  await page.waitForFunction(() => !document.querySelector(".status-text")?.textContent && !document.querySelector('tr[aria-disabled="true"]'));
}

export async function openContact(page, name) {
  if (!(await page.locator("table.grid.people").count()))
    await page.getByRole("navigation", { name: "Main navigation" }).getByRole("button", { name: /^People/ }).click();
  await idle(page);
  await page.locator("table.grid.people tbody tr").filter({ hasText: name }).first().click();
  await page.locator(".rec-head h1").filter({ hasText: name }).waitFor();
}

export async function openAgent(page) {
  const dock = page.getByRole("complementary", { name: "Agent workbench" });
  // After a reload the workspace (and the remembered Agent state) loads asynchronously.
  const toggle = page.locator(".topbar").getByRole("button", { name: /^Agent/ });
  await toggle.waitFor();
  await page.waitForTimeout(100);
  if (!(await dock.isVisible())) await toggle.click();
  await dock.waitFor();
  return dock;
}

export async function openDossier(page) {
  await page.getByRole("tab", { name: /^Dossier/ }).click();
}

export async function nav(page, name) {
  await page.getByRole("navigation", { name: "Main navigation" }).getByRole("button", { name: new RegExp(`^${name}`) }).click();
}
