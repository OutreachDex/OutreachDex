// Lists UI text passed to t() that a locale does not translate yet, and stale locale entries.
// English is the source language: t("…") keys are English, and locales/{zh,ja,ko}.json map them
// to translations. Untranslated text falls back to English, so missing translations are reported
// for information only (they are batched later); the check fails only when a locale file is broken.
//   node scripts/i18n-check.mjs [--write] [--all]
//   --write  adds missing keys with an empty value   --all  lists every missing key, not just 20
import { readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const web = fileURLToPath(new URL("../web/src", import.meta.url));
async function files(dir, ext) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory() && entry.name !== "node_modules" && entry.name !== "tests") out.push(...(await files(full, ext)));
    else if (ext.some((e) => entry.name.endsWith(e))) out.push(full);
  }
  return out;
}
const keys = new Set();
// t("…") and t('…') with a literal first argument; template literals are not keys.
const call = /\bt\(\s*(["'])((?:\\.|(?!\1).)*)\1/g;
for (const file of await files(web, [".ts", ".tsx"])) {
  const text = await readFile(file, "utf8");
  for (const m of text.matchAll(call)) keys.add(JSON.parse(`"${m[2].replace(/\\'/g, "'").replace(/"/g, '\\"')}"`));
}
// Backend messages listed in locales/backend.txt (English) are translated where the UI shows errors.
let backend = 0;
try {
  for (const line of (await readFile(path.join(web, "locales/backend.txt"), "utf8")).split("\n"))
    if (line.trim()) {
      keys.add(line);
      backend++;
    }
} catch {}
const cjk = /[぀-ヿ㐀-鿿가-힯]/;
const sourceCjk = [...keys].filter((k) => cjk.test(k));
console.log(`${keys.size} source strings (${backend} backend messages)`);
if (sourceCjk.length) {
  console.log(`${sourceCjk.length} source strings are not English (keys must be English):`);
  for (const k of sourceCjk.slice(0, 20)) console.log(`  ${k}`);
}
const all = process.argv.includes("--all");
let broken = 0;
for (const lang of ["zh", "ja", "ko"]) {
  const file = path.join(web, `locales/${lang}.json`);
  let dict;
  try {
    dict = JSON.parse(await readFile(file, "utf8"));
  } catch (e) {
    console.log(`${lang}: cannot read ${path.relative(process.cwd(), file)}: ${e.message}`);
    broken++;
    continue;
  }
  const absent = [...keys].filter((k) => !dict[k]);
  const stale = Object.keys(dict).filter((k) => !keys.has(k));
  // `{name}` placeholders must survive translation.
  const holes = (s) => (s.match(/\{\w+\}/g) || []).sort().join(" ");
  const mismatched = Object.entries(dict).filter(([k, v]) => v && holes(k) !== holes(v));
  console.log(`${lang}: ${keys.size - absent.length}/${keys.size} translated${absent.length ? `, ${absent.length} missing (falls back to English)` : ""}${stale.length ? `, ${stale.length} unused` : ""}`);
  for (const [k, v] of mismatched) console.log(`  placeholder mismatch: ${k} => ${v}`);
  broken += mismatched.length;
  if (process.argv.includes("--write")) {
    for (const k of absent) dict[k] = "";
    await writeFile(file, JSON.stringify(Object.fromEntries(Object.entries(dict).sort(([a], [b]) => a.localeCompare(b))), null, 2) + "\n");
  } else for (const k of all ? absent : absent.slice(0, 20)) console.log(`  missing: ${k}`);
}
process.exitCode = broken ? 1 : 0;
