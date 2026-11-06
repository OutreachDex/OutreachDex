import { useEffect, useRef } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { confirm } from '@tauri-apps/plugin-dialog'
import { mountKit, KitRpcError, type KitConnection } from 'kit-host'
import type { DocumentState } from 'kit-protocol'
import { currentScheme, useScheme } from '../app/prefs'
import { t, locale } from '../app/i18n'
export const editors = new Map<string, KitConnection>()
const releases = new Map<string, Promise<unknown>>()
// Host lifecycle requests share the same queue as background ai/run operations.
const operations = new WeakMap<KitConnection, Promise<unknown>>()
function ordered<T>(connection: KitConnection, work: () => Promise<T>): Promise<T> { const next = (operations.get(connection) || Promise.resolve()).catch(() => {}).then(work); operations.set(connection, next); return next }
export async function saveEditors() { for (const connection of editors.values()) await ordered(connection, async () => { await connection.save(); if ((await connection.state()).dirty) throw new Error(t("The document still has unsaved changes. Please try again.")) }) }
// The editor follows the app's resolved scheme. Per the Kit UI protocol the host supplies the
// scheme's colors as tokens, so they mirror styles/theme.css.
const kitTokens = {
  light: { '--kit-surface': '#ffffff', '--kit-surface-muted': '#f7f7f8', '--kit-canvas': '#f2f2f4', '--kit-text': '#1c1d21', '--kit-text-muted': '#5c5f6a', '--kit-border': '#e1e1e6', '--kit-primary': '#5b5bd6', '--kit-primary-text': '#ffffff', '--kit-primary-soft': 'rgba(91, 91, 214, 0.12)', '--kit-danger': '#b42323', '--kit-shadow-card': '0 1px 2px rgba(16,17,26,.04), 0 2px 8px rgba(16,17,26,.04)' },
  dark: { '--kit-surface': '#0f1011', '--kit-surface-muted': '#151618', '--kit-canvas': '#0b0b0c', '--kit-text': '#e8e8ea', '--kit-text-muted': '#9b9ca3', '--kit-border': '#2a2b2f', '--kit-primary': '#7c7ff5', '--kit-primary-text': '#ffffff', '--kit-primary-soft': 'rgba(124, 127, 245, 0.18)', '--kit-danger': '#f27a7a', '--kit-shadow-card': '0 0 0 1px rgba(255,255,255,.05), 0 8px 24px rgba(0,0,0,.4)' },
}
const kitTheme = (scheme: 'light' | 'dark') => ({ colorScheme: scheme, tokens: kitTokens[scheme], brand: 'host' as const })
type Props = { root: string; path?: string; generate?: boolean; onConnection?: (connection: KitConnection | null) => void; onState?: (state: DocumentState) => void; onError: (message: string) => void; onOpen?: (path: string) => Promise<unknown> }
export function TableEditor(props: Props) {
  const { root, path = 'contacts.ktable', generate = false } = props
  const container = useRef<HTMLDivElement>(null)
  const callbacks = useRef(props); callbacks.current = props
  const scheme = useScheme()
  const live = useRef<KitConnection | null>(null)
  useEffect(() => { live.current?.setTheme(kitTheme(scheme)) }, [scheme])
  useEffect(() => {
    const frame = document.createElement('iframe'); frame.title = path; frame.className = 'kit-frame'
    frame.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-downloads')
    const controller = new AbortController(); const session = crypto.randomUUID(); const owner = `${root}:${path}`
    let connection: KitConnection | null = null; let stop: (() => void) | undefined
    let registered = false; let disposed = false
    const call = async <T,>(command: string, args: Record<string, unknown>): Promise<T> => {
      try { return await invoke<T>(command, { root, ...args }) } catch (error) { throw new KitRpcError(-32000, String(error)) }
    }
    // Requests can arrive as soon as the document is registered, before the frame has mounted;
    // they wait on this promise instead of reading a connection that does not exist yet.
    let markReady!: (connection: KitConnection) => void
    let markFailed!: (error: unknown) => void
    const ready = new Promise<KitConnection>((resolve, reject) => { markReady = resolve; markFailed = reject })
    ready.catch(() => {})
    const setup = async () => {
      stop = await listen<{ id: string; session: string; method?: string; steps: Parameters<KitConnection['ai']['run']>[0] }>('document-request', async ({ payload }) => {
        if (payload.session !== session) return
        try { const mounted = await ready; const result = await ordered<unknown>(mounted, () => payload.method === "model.describe" ? mounted.model.describe() : payload.method === "document.check" ? mounted.check() : mounted.ai.run(payload.steps, true)); await call('document_reply', { id: payload.id, session, result }) }
        catch (e) { await call('document_reply', { id: payload.id, session, result: null, error: String(e) }).catch(() => {}) }
      })
      await releases.get(owner); if (disposed) { stop?.(); return }
      await call('document_register', { path, session }); registered = true
      if (disposed) { await call('document_unregister', { path, session }); stop?.(); return }
      const mounted = mountKit(frame, {
        origin: location.origin, host: { name: 'OutreachDex', version: '0.1.0' },
        document: { path, name: path.split('/').pop()! }, theme: kitTheme(currentScheme()),
        locale, shortcuts: ['Cmd+S', 'Cmd+J'], signal: controller.signal, timeoutMs: 60000,
        handlers: {
          read: async ({ path: file, offset, length }) => { const result = await call<{ bytes: number[]; version: string; size: number }>('file_read', { path: file, offset, length }); return { ...result, bytes: new Uint8Array(result.bytes).buffer } },
          stamp: async ({ path: file }) => ({ version: await call<string | null>('file_stamp', { path: file }) }),
          write: async ({ path: file, bytes, mode, expected }) => ({ path: file, version: await call<string>('file_write', { path: file, bytes: Array.from(new Uint8Array(bytes)), mode, expected }) }),
          openUrl: async ({ url }) => { if (!await confirm(t("Open external link: {url}?", { url }), { title: t("Open link") })) return { opened: false }; await call('open_external', { url }); return { opened: true } },
          open: async ({ link }) => { const target = await call<string>('resolve_link', { source: path, target: link.path || link.name || '' }); await connection?.save(); if ((target.endsWith('.md') || target.endsWith('.ktable')) && callbacks.current.onOpen) await callbacks.current.onOpen(target); else await call('open_external', { path: target }); return { opened: true } },
          changed: (state) => callbacks.current.onState?.(state),
          notify: (notice) => { if (notice.level === 'error') callbacks.current.onError([notice.title, notice.description].filter(Boolean).join(t(': '))) },
          shortcut: (key) => { if ((key.meta || key.ctrl) && key.key.toLowerCase() === 'j') window.dispatchEvent(new Event('globalcontact-agent-toggle')); if ((key.meta || key.ctrl) && key.key.toLowerCase() === 's') void saveEditors().catch((e) => callbacks.current.onError(String(e))) },
          ...(generate ? { generate: (request: unknown) => call<{ text: string }>('generate', { request }) } : {}),
        },
      })
      mounted.then(markReady, markFailed)
      frame.src = `/kits/${path.endsWith('.ktable') ? 'table' : 'note'}/index.html`; container.current!.appendChild(frame)
      connection = await ready
      if (disposed) { connection.close(); return }
      editors.set(path, connection); live.current = connection; connection.setTheme(kitTheme(currentScheme())); callbacks.current.onConnection?.(connection); callbacks.current.onState?.(await connection.state())
    }
    void setup().catch((error) => { markFailed(error); if (!disposed) callbacks.current.onError(String(error)); if (registered) void call('document_unregister', { path, session }); stop?.() })
    return () => { markFailed(new Error(t("Editor closed"))); disposed = true; controller.abort(); editors.delete(path); live.current = null; connection?.close(); callbacks.current.onConnection?.(null); frame.remove(); stop?.(); if (registered) { const release = call('document_unregister', { path, session }).catch(() => {}); releases.set(owner, release); void release.then(() => { if (releases.get(owner) === release) releases.delete(owner) }) } }
  }, [root, path, generate])
  return <div ref={container} className="kit-container" />
}
