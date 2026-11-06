// Checks a built app bundle: the bundled ACP adapters must start and answer `initialize`.
// The bundler drops symlinks, so a bad resource layout shows up here rather than at first use.
//   node scripts/check-bundle.mjs [path/to/OutreachDex.app]
import { spawn } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
const root = fileURLToPath(new URL('..', import.meta.url))
const app = path.resolve(process.argv[2] || path.join(root, 'target/debug/bundle/macos/OutreachDex.app'))
const bin = path.join(app, 'Contents/Resources/resources/agents/node_modules/.bin')
const handshake = (name) =>
  new Promise((resolve, reject) => {
    const child = spawn(path.join(bin, name), [], { stdio: ['pipe', 'pipe', 'pipe'] })
    let out = '', err = ''
    const timer = setTimeout(() => (child.kill(), reject(new Error(`${name}: no answer in 20 s\n${err.slice(-800)}`))), 20000)
    child.stdout.on('data', (d) => {
      out += d
      if (out.includes('"protocolVersion"')) {
        clearTimeout(timer)
        child.kill()
        resolve()
      }
    })
    child.stderr.on('data', (d) => (err += d))
    child.on('exit', (code) => code && code !== 0 && (clearTimeout(timer), reject(new Error(`${name} exited ${code}\n${err.slice(-800)}`))))
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: 1, clientCapabilities: {} } }) + '\n')
  })
for (const name of ['claude-agent-acp', 'codex-acp']) {
  await handshake(name)
  console.log(`✓ ${name} starts from the bundle`)
}
