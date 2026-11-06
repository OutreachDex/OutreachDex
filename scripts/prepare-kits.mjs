// Consume supplier artifacts and public catalogs; no editor source is compiled by the host.
// `--build` first builds the supplier's current local working tree (uncommitted changes included),
// fingerprinting its sources before and after so the imported artifacts match one source state.
import { cp, mkdir, readFile, rm, access, chmod, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
const root = fileURLToPath(new URL('..', import.meta.url))
const args = process.argv.slice(2)
const build = args.includes('--build')
const requested = args.filter(a => a !== '--build')
if (requested.some(name => !['table', 'note'].includes(name))) throw new Error('Usage: prepare-kits.mjs [--build] [table] [note]')
const git = (cwd, ...a) => execFileSync('git', a, { cwd, encoding: 'utf8', maxBuffer: 1 << 28 })
const sha = (data) => createHash('sha256').update(data).digest('hex')
const digest = async (file) => sha(await readFile(file))
const generated = /(^|\/)(target|node_modules|dist|\.turbo)(\/|$)/

// Hash of every tracked and untracked (non-ignored) source file, so uncommitted fixes count.
async function fingerprint(repo) {
  const files = git(repo, 'ls-files', '-co', '--exclude-standard', '-z').split('\0').filter(f => f && !generated.test(f)).sort()
  const entries = {}
  for (const f of files) { try { entries[f] = await digest(path.join(repo, f)) } catch { entries[f] = 'missing' } }
  return { revision: git(repo, 'rev-parse', 'HEAD').trim(), modifiedFiles: git(repo, 'status', '--porcelain').split('\n').filter(Boolean).length, sha256: sha(JSON.stringify(entries)), entries }
}
const drift = (a, b) => [...new Set([...Object.keys(a.entries), ...Object.keys(b.entries)])].filter(f => a.entries[f] !== b.entries[f])
const run = (cmd, a, cwd) => { console.log(`$ ${cmd} ${a.join(' ')}  (${path.relative(root, cwd) || '.'})`); execFileSync(cmd, a, { cwd, stdio: 'inherit' }) }

for (const name of new Set(requested.length ? requested : ['table', 'note'])) {
  const repo = process.env[`KIT_${name.toUpperCase()}_ROOT`] || path.resolve(root, `../../KitionAI/kition-${name}`)
  const sdk = process.env.KIT_SDK || path.resolve(repo, '../kition-sdk')
  // Sources the editor build links to: the kit itself, the SDK packages and (note) the icon set.
  const sources = { [name]: repo, sdk }
  if (name === 'note') sources.icons = process.env.KIT_TESSERA || path.resolve(repo, '../kition-tessera')
  let localSources
  if (build) {
    const before = {}; for (const [k, dir] of Object.entries(sources)) before[k] = await fingerprint(dir)
    const startedAt = new Date().toISOString()
    run('cargo', ['build', '-p', `kition-${name}`], repo)
    run('pnpm', ['--dir', 'web', 'engine'], repo)
    run('pnpm', ['--dir', 'web', 'build'], repo)
    const changed = {}
    for (const [k, dir] of Object.entries(sources)) { const d = drift(before[k], await fingerprint(dir)); if (d.length) changed[k] = d }
    if (Object.keys(changed).length) {
      for (const [k, files] of Object.entries(changed)) console.error(`${k} changed during the build:\n  ${files.slice(0, 20).join('\n  ')}${files.length > 20 ? `\n  … ${files.length - 20} more` : ''}`)
      throw new Error(`kit.${name} sources changed while building; artifacts were not imported. Re-run once the supplier tree is stable.`)
    }
    localSources = Object.fromEntries(Object.entries(before).map(([k, f]) => [k, { revision: f.revision, modifiedFiles: f.modifiedFiles, sourceSha256: f.sha256 }]))
    localSources.buildStartedAt = startedAt
    localSources.sourceStateVerifiedAt = new Date().toISOString()
  }
  const source = process.env[`KIT_${name.toUpperCase()}_DIST`] || path.join(repo, 'web/dist')
  const html = await readFile(path.join(source, 'index.html'), 'utf8')
  if (!html.includes('<html')) throw new Error(`Invalid kit.${name} editor artifact`)
  await access(path.join(source, `engine/kition_${name}_sdk_bg.wasm`))
  const binary = process.env[`KIT_${name.toUpperCase()}_BIN`] || path.join(repo, `target/debug/kition-${name}`)
  try { await access(binary) } catch { throw new Error(`Build the supplier executable first: cargo build --manifest-path ${repo}/Cargo.toml -p kition-${name}`) }
  const dest = path.join(root, `web/public/kits/${name}`)
  await mkdir(path.dirname(dest), { recursive: true }); await rm(dest, { recursive: true, force: true }); await cp(source, dest, { recursive: true })
  const resources = path.join(root, 'src-tauri/resources/kits'); await mkdir(path.join(resources, name), { recursive: true })
  for (const [from, to] of [['kit.json', 'kit.json'], ...(name === 'table' ? [['agent/commands.json', 'commands.json']] : [])]) await cp(path.join(repo, from), path.join(resources, name, to))
  // Bundled under the host's stable name, whatever the supplier calls its package and binary.
  await cp(binary, path.join(resources, `kit-${name}`)); await chmod(path.join(resources, `kit-${name}`), 0o755)
  const manifest = JSON.parse(await readFile(path.join(repo, 'kit.json'), 'utf8'))
  const provenance = {
    kit: `kit.${name}`, version: manifest.version,
    revision: git(repo, 'rev-parse', 'HEAD').trim(), modified: !!git(repo, 'status', '--porcelain').trim(),
    binarySha256: await digest(binary), wasmSha256: await digest(path.join(source, `engine/kition_${name}_sdk_bg.wasm`)), editorHtmlSha256: await digest(path.join(source, 'index.html')),
    sdkRevision: git(sdk, 'rev-parse', 'HEAD').trim(), sdkModified: !!git(sdk, 'status', '--porcelain').trim(),
    builtFromLocalTree: build, ...(localSources ? { localSources } : {}),
  }
  await writeFile(path.join(resources, name, 'provenance.json'), JSON.stringify(provenance, null, 2) + '\n')
  console.log(`Prepared kit.${name} ${manifest.version}${build ? ' (built from verified local tree)' : ''}`)
}
