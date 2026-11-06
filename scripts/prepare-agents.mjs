// Bundles the ACP adapters and Node.js into src-tauri/resources/agents.
//
// The app bundler does not copy symlinks, and pnpm's default layout is made of them, so the
// adapters are installed into a scratch directory with the hoisted (real directories) layout and
// copied from there. The result must contain no symlinks at all.
import { cp, mkdir, mkdtemp, readFile, writeFile, chmod, rm, readdir, lstat } from 'node:fs/promises'
import { execFileSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
const root = fileURLToPath(new URL('..', import.meta.url))
const dest = path.join(root, 'src-tauri/resources/agents')
const adapters = [['claude-agent-acp', '@agentclientprotocol/claude-agent-acp'], ['codex-acp', '@agentclientprotocol/codex-acp']]
const versions = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8')).dependencies

const scratch = await mkdtemp(path.join(tmpdir(), 'gc-agents-'))
await writeFile(
  path.join(scratch, 'package.json'),
  JSON.stringify({ name: 'globalcontact-agents', private: true, dependencies: Object.fromEntries(adapters.map(([, p]) => [p, versions[p]])) }, null, 2),
)
const install = (extra) =>
  execFileSync('pnpm', ['install', '--prod', '--ignore-scripts', '--config.node-linker=hoisted', '--config.confirmModulesPurge=false', ...extra], { cwd: scratch, stdio: 'inherit' })
// Online by default: offline installs silently drop optional platform packages (the Codex
// binary is one), because their registry metadata is not in the local store.
try {
  install([])
} catch {
  console.log('Registry unreachable; installing from the local store only.')
  install(['--offline'])
}
const platformBinary = `@openai/codex-${process.platform === 'win32' ? 'win32' : process.platform}-${process.arch}`
try {
  await lstat(path.join(scratch, 'node_modules', platformBinary, 'package.json'))
} catch {
  throw new Error(`${platformBinary} is missing: the Codex adapter cannot start. Re-run with network access.`)
}

async function symlinks(dir, found = []) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isSymbolicLink()) found.push(full)
    else if (entry.isDirectory()) await symlinks(full, found)
  }
  return found
}

await rm(path.join(dest, 'node_modules'), { recursive: true, force: true })
await mkdir(dest, { recursive: true })
await cp(path.join(scratch, 'node_modules'), path.join(dest, 'node_modules'), {
  recursive: true,
  dereference: true,
  filter: (src) => !src.includes(`${path.sep}.bin`) && !src.endsWith('.modules.yaml'),
})
await rm(scratch, { recursive: true, force: true })
await mkdir(path.join(dest, 'node_modules/.bin'), { recursive: true })
await cp(process.execPath, path.join(dest, 'node_modules/.bin/node'))
await chmod(path.join(dest, 'node_modules/.bin/node'), 0o755)
for (const [name, packageName] of adapters) {
  const pkg = JSON.parse(await readFile(path.join(dest, 'node_modules', packageName, 'package.json'), 'utf8'))
  const entry = typeof pkg.bin === 'string' ? pkg.bin : pkg.bin[name]
  await lstat(path.join(dest, 'node_modules', packageName, entry))
  // Relative launcher: works after moving the .app and without shell-initialized PATH.
  const launcher = `#!/bin/sh\nbasedir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexport PATH="$basedir:$PATH"\nexec "$basedir/node" "$basedir/../${packageName}/${entry}" "$@"\n`
  await writeFile(path.join(dest, 'node_modules/.bin', name), launcher, { mode: 0o755 })
  await chmod(path.join(dest, 'node_modules/.bin', name), 0o755)
  console.log(`Prepared ${packageName} ${pkg.version}`)
}
const left = await symlinks(path.join(dest, 'node_modules'))
if (left.length) throw new Error(`Agent resources still contain symlinks, which the bundle would drop:\n${left.slice(0, 10).join('\n')}`)
await writeFile(path.join(dest, 'NOTICE.txt'), 'Agent adapters and Node.js are distributed with their original package licenses. See node_modules and https://nodejs.org/en/about/previous-releases for license information.\n')
